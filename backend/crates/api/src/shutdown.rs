// crates/api/src/shutdown.rs
//! Stopping the server cleanly on SIGTERM or SIGINT: it stops accepting
//! connections, answers the requests under way, then waits for the
//! background tasks (the mails sent after an answer, the cleanup), all
//! within `SHUTDOWN_GRACE_SECS`. Past it, the server stops anyway and logs
//! how many tasks it abandons.
//!
//! What this cannot cover: a kill that gives no notice, SIGKILL, a crash or
//! a power loss, takes the mails under way with it. The database stays
//! consistent, since Postgres rolls back a transaction whose connection
//! goes, but those mails are not sent.

use std::future::Future;
use std::net::SocketAddr;
use std::time::Duration;

use axum::Router;
use services::background::{BackgroundTasks, ShutdownTrigger};
use tokio::net::TcpListener;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::watch;
use tokio::time::Instant;

/// How a shutdown went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Outcome {
    /// Every request under way was answered before the deadline.
    pub(crate) requests_answered: bool,
    /// Background tasks still running at the deadline.
    pub(crate) abandoned: usize,
}

/// Resolves on the first SIGTERM or SIGINT, with its name. The handlers are
/// installed here, at once, not on the first poll: the server runs as PID 1
/// in its container, where a signal with no handler is dropped.
pub(crate) fn signal_received() -> std::io::Result<impl Future<Output = &'static str>> {
    let mut terminate = signal(SignalKind::terminate())?;
    let mut interrupt = signal(SignalKind::interrupt())?;
    Ok(async move {
        tokio::select! {
            _ = terminate.recv() => "SIGTERM",
            _ = interrupt.recv() => "SIGINT",
        }
    })
}

/// Serves `app` until `stop` resolves, then shuts down as the module docs
/// say. `trigger` tells the loops that start new work, the cleanup's among
/// them, to stop doing so.
pub(crate) async fn serve<S>(
    listener: TcpListener,
    app: Router,
    background: &BackgroundTasks,
    trigger: ShutdownTrigger,
    stop: S,
    grace: Duration,
) -> std::io::Result<Outcome>
where
    S: Future<Output = &'static str> + Send + 'static,
{
    let (deadline_tx, mut deadline_rx) = watch::channel(None::<Instant>);

    let server = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        let signal = stop.await;
        let deadline = Instant::now() + grace;
        tracing::info!(
            signal,
            grace_secs = grace.as_secs(),
            "shutdown requested: no new connections, finishing the requests under way"
        );
        trigger.trigger();
        deadline_tx.send_replace(Some(deadline));
    })
    .into_future();

    // The requests under way get until the deadline too; past it, the server
    // is dropped with whatever it still holds.
    let past_deadline = async {
        // Copied out at once: a borrow of the channel held across the sleep
        // would block its sender.
        let deadline = match deadline_rx.wait_for(Option::is_some).await {
            Ok(deadline) => *deadline,
            Err(_) => None,
        };
        match deadline {
            Some(deadline) => tokio::time::sleep_until(deadline).await,
            None => std::future::pending::<()>().await,
        }
    };

    let requests_answered = tokio::select! {
        served = server => {
            served?;
            true
        }
        () = past_deadline => {
            tracing::warn!("shutdown grace period elapsed with requests still under way, dropping them");
            false
        }
    };

    let deadline = (*deadline_rx.borrow()).unwrap_or_else(Instant::now);
    tracing::info!(
        tasks = background.running(),
        "requests finished, waiting for the background tasks"
    );
    let abandoned = background.wait_until(deadline).await;
    if abandoned > 0 {
        tracing::warn!(
            abandoned,
            "shutdown grace period elapsed, abandoning the background tasks still running"
        );
    }
    tracing::info!("shutdown complete");

    Ok(Outcome {
        requests_answered,
        abandoned,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{MailSink, app_with_background, in_scratch_database};
    use axum::routing::get;
    use domain::legal::{CURRENT_PRIVACY_VERSION, CURRENT_TERMS_VERSION};
    use serde_json::json;
    use services::background::Shutdown;
    use std::net::{IpAddr, Ipv6Addr};
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio::sync::{Notify, oneshot};
    use tokio::task::JoinHandle;
    use uuid::Uuid;

    /// Far more than any of these shutdowns takes, so that one that never
    /// ends fails the test instead of hanging it.
    const NEVER: Duration = Duration::from_secs(15);

    /// A server from `serve` on a port of its own, stopped by `stop`.
    struct Server {
        addr: SocketAddr,
        stop: oneshot::Sender<()>,
        served: JoinHandle<std::io::Result<Outcome>>,
    }

    async fn start(app: Router, background: BackgroundTasks, grace: Duration) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a port must be free");
        let addr = listener.local_addr().expect("an address");
        let (stop, stopped) = oneshot::channel::<()>();
        let (trigger, _) = Shutdown::channel();
        let served = tokio::spawn(async move {
            let stop = async move {
                let _ = stopped.await;
                "SIGTEST"
            };
            serve(listener, app, &background, trigger, stop, grace).await
        });
        Server { addr, stop, served }
    }

    impl Server {
        /// Sends the stop and waits for the shutdown to end.
        async fn stop(self) -> Outcome {
            let _ = self.stop.send(());
            tokio::time::timeout(NEVER, self.served)
                .await
                .expect("the shutdown must end")
                .expect("the server task must not panic")
                .expect("the server must not fail")
        }
    }

    /// One HTTP/1.1 request on a connection of its own, and the status code
    /// of the answer.
    async fn send(
        addr: SocketAddr,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> std::io::Result<u16> {
        let mut stream = TcpStream::connect(addr).await?;
        let mut request = format!(
            "{method} {path} HTTP/1.1\r\nhost: localhost\r\nconnection: close\r\n\
             content-length: {}\r\n",
            body.len()
        );
        for (name, value) in headers {
            request.push_str(&format!("{name}: {value}\r\n"));
        }
        request.push_str("\r\n");
        request.push_str(body);
        stream.write_all(request.as_bytes()).await?;

        let mut answer = Vec::new();
        stream.read_to_end(&mut answer).await?;
        String::from_utf8_lossy(&answer)
            .split(' ')
            .nth(1)
            .and_then(|code| code.parse().ok())
            .ok_or_else(|| std::io::Error::other("no status line"))
    }

    /// A router whose one route lets the test know it was reached, then
    /// takes `delay` to answer, or never answers without one.
    fn slow_app(reached: Arc<Notify>, delay: Option<Duration>) -> Router {
        Router::new().route(
            "/slow",
            get(move || async move {
                reached.notify_one();
                match delay {
                    Some(delay) => tokio::time::sleep(delay).await,
                    None => std::future::pending().await,
                }
                "done"
            }),
        )
    }

    #[tokio::test]
    async fn a_request_under_way_is_answered_and_new_connections_are_refused() {
        let reached = Arc::new(Notify::new());
        let app = slow_app(Arc::clone(&reached), Some(Duration::from_secs(2)));
        let server = start(app, BackgroundTasks::new(), Duration::from_secs(10)).await;
        let addr = server.addr;

        let under_way = tokio::spawn(async move { send(addr, "GET", "/slow", &[], "").await });
        reached.notified().await;
        let _ = server.stop.send(());

        // The listener closes as the shutdown starts; the request under way
        // is still being served while it does.
        let mut refused = false;
        for _ in 0..40 {
            match TcpStream::connect(addr).await {
                Err(err) if err.kind() == std::io::ErrorKind::ConnectionRefused => {
                    refused = true;
                    break;
                }
                _ => tokio::time::sleep(Duration::from_millis(25)).await,
            }
        }
        assert!(refused, "a connection after the signal must be refused");
        assert!(
            !under_way.is_finished(),
            "the refusal comes while the request is still under way"
        );

        let status = under_way
            .await
            .expect("the client task must not panic")
            .expect("the request under way must get its answer");
        assert_eq!(status, 200);

        let outcome = tokio::time::timeout(NEVER, server.served)
            .await
            .expect("the shutdown must end")
            .expect("the server task must not panic")
            .expect("the server must not fail");
        assert_eq!(
            outcome,
            Outcome {
                requests_answered: true,
                abandoned: 0
            }
        );
    }

    #[tokio::test]
    async fn a_request_past_the_grace_period_does_not_hold_the_shutdown() {
        let reached = Arc::new(Notify::new());
        let app = slow_app(Arc::clone(&reached), None);
        let server = start(app, BackgroundTasks::new(), Duration::from_secs(1)).await;
        let addr = server.addr;

        let _under_way = tokio::spawn(async move { send(addr, "GET", "/slow", &[], "").await });
        reached.notified().await;

        let outcome = server.stop().await;

        assert_eq!(
            outcome,
            Outcome {
                requests_answered: false,
                abandoned: 0
            }
        );
    }

    // ---- With the real application ----------------------------------------

    /// A documentation address (RFC 3849) of its own per call, so that no
    /// run shares a rate-limit bucket with another.
    fn fresh_ip() -> String {
        let id = Uuid::now_v7().as_u128();
        IpAddr::V6(Ipv6Addr::new(
            0x2001,
            0x0db8,
            0x5d0,
            (id >> 64) as u16,
            (id >> 48) as u16,
            (id >> 32) as u16,
            (id >> 16) as u16,
            id as u16,
        ))
        .to_string()
    }

    /// A registration of a fresh account, answered at once, its mail sent
    /// by a background task; the account's address.
    async fn register(addr: SocketAddr) -> String {
        let name = format!("s{}", &Uuid::now_v7().simple().to_string()[1..]);
        let address = format!("{name}@example.test");
        let body = json!({
            "username": name,
            "email": address,
            "password": "a-long-test-password-1!",
            "legal": {
                "terms_version": CURRENT_TERMS_VERSION,
                "privacy_version": CURRENT_PRIVACY_VERSION,
            },
            "turnstile_token": "",
        })
        .to_string();
        let ip = fresh_ip();
        let status = send(
            addr,
            "POST",
            "/api/v1/auth/register",
            &[
                ("content-type", "application/json"),
                ("x-forwarded-for", &ip),
            ],
            &body,
        )
        .await
        .expect("the registration must be answered");
        assert_eq!(status, 202);
        address
    }

    /// The configuration for `sink`, the client address taken from the
    /// header the test sets.
    fn config_for(sink: &MailSink) -> services::config::Config {
        let mut config = sink.config();
        config.trusted_proxy_count = 1;
        config
    }

    #[tokio::test]
    async fn a_mail_under_way_is_sent_before_the_shutdown_ends() {
        in_scratch_database(|db| async move {
            let sink = MailSink::slow(Duration::from_secs(1)).await;
            let background = BackgroundTasks::new();
            let app = app_with_background(&db, config_for(&sink), background.clone()).await;
            let server = start(app, background, Duration::from_secs(10)).await;

            let address = register(server.addr).await;
            assert!(sink.mails().is_empty(), "the mail is still under way");

            let outcome = server.stop().await;

            assert_eq!(outcome.abandoned, 0);
            // Read once, at the end of the shutdown: no waiting here.
            let mails = sink.mails();
            assert_eq!(mails.len(), 1, "the mail must be out before the end");
            assert_eq!(mails[0].to, [address]);
        })
        .await;
    }

    /// Everything the server logs, as text.
    #[derive(Clone, Default)]
    struct Logs(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Logs {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("the log lock").extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Logs {
        fn text(&self) -> String {
            String::from_utf8_lossy(&self.0.lock().expect("the log lock")).into_owned()
        }
    }

    // A runtime on this thread alone: the subscriber set below is this
    // thread's, and every task of the test runs on it.
    #[tokio::test(flavor = "current_thread")]
    async fn past_the_grace_period_the_shutdown_ends_and_logs_the_abandon() {
        // tracing-core caches whether a log line is enabled when it is first
        // reached, and while one dispatcher alone is registered it asks only
        // the reaching thread's: a line first reached by a test running on
        // another thread, with no subscriber, would stay disabled for this
        // one too. A second dispatcher, alive for the test and discarding
        // what it gets, makes it ask every registered one.
        let _second = tracing::Dispatch::new(
            tracing_subscriber::fmt()
                .with_max_level(tracing::Level::INFO)
                .with_writer(std::io::sink)
                .finish(),
        );
        let logs = Logs::default();
        let writer = logs.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        let _logging = tracing::subscriber::set_default(subscriber);

        let started = Instant::now();
        let address = Arc::new(Mutex::new(String::new()));
        let registered = Arc::clone(&address);
        in_scratch_database(|db| async move {
            let sink = MailSink::silent().await;
            let background = BackgroundTasks::new();
            let app = app_with_background(&db, config_for(&sink), background.clone()).await;
            let server = start(app, background, Duration::from_secs(1)).await;

            *registered.lock().expect("the address lock") = register(server.addr).await;

            let outcome = server.stop().await;

            assert_eq!(
                outcome,
                Outcome {
                    requests_answered: true,
                    abandoned: 1
                }
            );
        })
        .await;
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the shutdown must not wait for the mail past its grace period"
        );

        let logs = logs.text();
        for line in [
            "shutdown requested: no new connections, finishing the requests under way \
             signal=\"SIGTEST\" grace_secs=1",
            "requests finished, waiting for the background tasks tasks=1",
            "shutdown grace period elapsed, abandoning the background tasks still running \
             abandoned=1",
            "shutdown complete",
        ] {
            assert!(logs.contains(line), "the logs must say {line:?}:\n{logs}");
        }
        let address = address.lock().expect("the address lock").clone();
        let local_part = address.split('@').next().expect("a local part");
        assert!(
            !logs.contains(local_part),
            "the shutdown logs must not name the account:\n{logs}"
        );
    }

    #[tokio::test]
    async fn the_signals_end_the_wait_and_are_named() {
        // Installed first, so that the process outlives a signal even if
        // the code under test fails to handle it.
        let _term = signal(SignalKind::terminate()).expect("a SIGTERM handler");
        let _int = signal(SignalKind::interrupt()).expect("a SIGINT handler");

        for (flag, name) in [("-TERM", "SIGTERM"), ("-INT", "SIGINT")] {
            let received = signal_received().expect("the handlers must install");
            let sent = std::process::Command::new("kill")
                .args([flag, &std::process::id().to_string()])
                .status()
                .expect("kill must run");
            assert!(sent.success(), "the signal must be sent");

            let got = tokio::time::timeout(NEVER, received)
                .await
                .expect("the signal must end the wait");
            assert_eq!(got, name);
        }
    }
}

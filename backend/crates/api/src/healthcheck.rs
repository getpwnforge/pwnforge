// crates/api/src/healthcheck.rs
//! `api healthcheck`: the container healthcheck, built into the binary.
//!
//! The runtime image is debian-slim with no curl or wget, so the probe has to
//! ship with the server. It loads no configuration and opens no database or
//! Redis connection: it reads `PORT`, asks the running server for
//! `/api/v1/health` over plain HTTP/1.1 on the loopback, and turns the answer
//! into an exit code. Whether Postgres and Redis are up is the health route's
//! job; it answers 503 when either is down.

use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Short enough that a hung server fails the check well inside the compose
/// healthcheck's own timeout.
const TIMEOUT: Duration = Duration::from_secs(3);

/// Same default as `Config::from_env`.
const DEFAULT_PORT: u16 = 3000;

const REQUEST: &[u8] =
    b"GET /api/v1/health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";

#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Healthy,
    Unhealthy(String),
}

/// Runs the check and returns the process exit code: 0 healthy, 1 otherwise.
pub async fn run() -> i32 {
    let port = match std::env::var("PORT") {
        Ok(raw) => match raw.parse::<u16>() {
            Ok(port) => port,
            Err(_) => {
                eprintln!("healthcheck: PORT is not a port number: {raw}");
                return 1;
            }
        },
        Err(_) => DEFAULT_PORT,
    };

    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    match probe(addr, TIMEOUT).await {
        Outcome::Healthy => 0,
        Outcome::Unhealthy(reason) => {
            eprintln!("healthcheck: {reason}");
            1
        }
    }
}

async fn probe(addr: SocketAddr, timeout: Duration) -> Outcome {
    match tokio::time::timeout(timeout, status_line(addr)).await {
        Err(_) => Outcome::Unhealthy(format!("no answer from {addr} within {timeout:?}")),
        Ok(Err(error)) => Outcome::Unhealthy(format!("{addr}: {error}")),
        Ok(Ok(line)) => interpret(&line),
    }
}

/// Sends the request and returns the response's status line.
async fn status_line(addr: SocketAddr) -> std::io::Result<String> {
    let mut stream = TcpStream::connect(addr).await?;
    stream.write_all(REQUEST).await?;

    // The status line is the first line of the response; nothing after it is
    // needed, and a bounded buffer keeps a misbehaving peer from feeding us
    // an endless one.
    let mut buffer = Vec::with_capacity(128);
    let mut chunk = [0u8; 128];
    loop {
        if let Some(end) = buffer.windows(2).position(|pair| pair == b"\r\n") {
            buffer.truncate(end);
            break;
        }
        if buffer.len() > 1024 {
            break;
        }
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }

    Ok(String::from_utf8_lossy(&buffer).into_owned())
}

/// `HTTP/1.1 200 OK` is healthy; any other status, or anything that is not a
/// status line, is not.
fn interpret(line: &str) -> Outcome {
    let mut parts = line.split_whitespace();
    let version = parts.next().unwrap_or_default();
    let status = parts.next().unwrap_or_default();

    if !version.starts_with("HTTP/") {
        return Outcome::Unhealthy(format!("not an HTTP response: {line:?}"));
    }
    if status == "200" {
        Outcome::Healthy
    } else {
        Outcome::Unhealthy(format!("health answered {line:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    /// A server that answers every connection with `response` once the
    /// request has arrived.
    async fn serving(response: &'static [u8]) -> SocketAddr {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut request = [0u8; 256];
                let _ = socket.read(&mut request).await;
                let _ = socket.write_all(response).await;
            }
        });
        addr
    }

    #[tokio::test]
    async fn a_200_is_healthy() {
        let addr = serving(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n{}").await;
        assert_eq!(probe(addr, TIMEOUT).await, Outcome::Healthy);
    }

    #[tokio::test]
    async fn any_other_status_is_unhealthy() {
        let addr = serving(b"HTTP/1.1 503 Service Unavailable\r\n\r\n").await;
        assert!(matches!(probe(addr, TIMEOUT).await, Outcome::Unhealthy(_)));
    }

    #[tokio::test]
    async fn a_refused_connection_is_unhealthy() {
        // Bind to get a free port, then close it: nothing listens there.
        let addr = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap()
            .local_addr()
            .unwrap();

        assert!(matches!(probe(addr, TIMEOUT).await, Outcome::Unhealthy(_)));
    }

    #[tokio::test]
    async fn a_server_that_never_answers_times_out() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        // Accept and hold the connection open without writing anything.
        tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((socket, _)) = listener.accept().await {
                held.push(socket);
            }
        });

        let outcome = probe(addr, Duration::from_millis(200)).await;
        assert!(
            matches!(&outcome, Outcome::Unhealthy(reason) if reason.contains("within")),
            "expected a timeout, got {outcome:?}"
        );
    }

    #[test]
    fn only_a_status_line_with_200_counts() {
        assert_eq!(interpret("HTTP/1.1 200 OK"), Outcome::Healthy);
        assert_eq!(interpret("HTTP/1.0 200"), Outcome::Healthy);
        assert!(matches!(
            interpret("HTTP/1.1 2000 OK"),
            Outcome::Unhealthy(_)
        ));
        assert!(matches!(interpret("200 OK"), Outcome::Unhealthy(_)));
        assert!(matches!(interpret(""), Outcome::Unhealthy(_)));
    }
}

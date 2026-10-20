// crates/services/src/background.rs
//! The work that outlives the request that started it, tracked so that the
//! server can wait for it before it stops: a mail sent after the answer, the
//! token it carries, the cleanup task. Every such task goes through
//! `BackgroundTasks::spawn`, never a bare `tokio::spawn`, or a shutdown cuts
//! it in the middle.
//!
//! Tokio alone: a counter of the tasks still running and a `Notify` rung when
//! it reaches zero, so the tracker holds nothing for a task that is done.

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::sync::{Notify, watch};
use tokio::time::Instant;

#[derive(Clone, Default)]
pub struct BackgroundTasks {
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    running: AtomicUsize,
    idle: Notify,
}

/// Counts its task out when dropped, whether the task returned or panicked.
struct Running(Arc<Inner>);

impl Drop for Running {
    fn drop(&mut self) {
        if self.0.running.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.0.idle.notify_waiters();
        }
    }
}

impl BackgroundTasks {
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs `task` on its own, counted until it ends.
    pub fn spawn<F>(&self, task: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.inner.running.fetch_add(1, Ordering::AcqRel);
        let running = Running(Arc::clone(&self.inner));
        tokio::spawn(async move {
            let _running = running;
            task.await;
        });
    }

    /// How many tasks are still running.
    pub fn running(&self) -> usize {
        self.inner.running.load(Ordering::Acquire)
    }

    /// Waits until every task has ended, or until `deadline`. Returns how
    /// many were still running then: the ones the caller abandons.
    pub async fn wait_until(&self, deadline: Instant) -> usize {
        loop {
            // Registered before the count is read, so a task ending in
            // between still wakes this one.
            let idle = self.inner.idle.notified();
            tokio::pin!(idle);
            idle.as_mut().enable();

            let running = self.running();
            if running == 0 {
                return 0;
            }
            if tokio::time::timeout_at(deadline, idle).await.is_err() {
                return self.running();
            }
        }
    }
}

/// Whether the server has been asked to stop, for the loops that must not
/// start new work then. Cloned freely; `Shutdown::never()` for callers that
/// run to completion, such as the CLI and the tests.
#[derive(Clone)]
pub struct Shutdown(watch::Receiver<bool>);

impl Shutdown {
    /// A shutdown that `trigger` requests.
    pub fn channel() -> (ShutdownTrigger, Self) {
        let (sender, receiver) = watch::channel(false);
        (ShutdownTrigger(sender), Self(receiver))
    }

    /// A shutdown that never comes.
    pub fn never() -> Self {
        Self::channel().1
    }

    pub fn requested(&self) -> bool {
        *self.0.borrow()
    }

    /// Resolves once the shutdown is requested, at once if it already is.
    pub async fn wait(&mut self) {
        // An error means the trigger is gone without ever firing: the
        // shutdown never comes, so neither does this.
        if self.0.wait_for(|stop| *stop).await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}

/// Requests the shutdown the paired `Shutdown`s wait for.
pub struct ShutdownTrigger(watch::Sender<bool>);

impl ShutdownTrigger {
    pub fn trigger(&self) {
        self.0.send_replace(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn waits_for_the_tasks_to_end() {
        let tasks = BackgroundTasks::new();
        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        tasks.spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let _ = done_tx.send(());
        });
        assert_eq!(tasks.running(), 1);

        let abandoned = tasks
            .wait_until(Instant::now() + Duration::from_secs(5))
            .await;

        assert_eq!(abandoned, 0);
        assert!(done_rx.await.is_ok(), "the task ran to its end");
    }

    #[tokio::test]
    async fn gives_up_at_the_deadline_and_counts_what_is_left() {
        let tasks = BackgroundTasks::new();
        tasks.spawn(std::future::pending());
        tasks.spawn(async {});

        let abandoned = tokio::time::timeout(
            Duration::from_secs(10),
            tasks.wait_until(Instant::now() + Duration::from_millis(200)),
        )
        .await
        .expect("the wait must end at its deadline");

        assert_eq!(abandoned, 1);
    }

    #[tokio::test]
    async fn a_task_that_panics_is_counted_out() {
        let tasks = BackgroundTasks::new();
        tasks.spawn(async { panic!("a task that fails") });

        let abandoned = tasks
            .wait_until(Instant::now() + Duration::from_secs(5))
            .await;

        assert_eq!(abandoned, 0);
    }

    #[tokio::test]
    async fn a_shutdown_is_seen_once_triggered() {
        let (trigger, mut shutdown) = Shutdown::channel();
        assert!(!shutdown.requested());

        trigger.trigger();

        assert!(shutdown.requested());
        tokio::time::timeout(Duration::from_secs(1), shutdown.wait())
            .await
            .expect("the wait ends once triggered");
    }
}

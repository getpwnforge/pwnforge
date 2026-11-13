//! Waiting on a lock, for the tests that order transactions through one. The
//! wait is read from `pg_blocking_pids` rather than slept for a guessed
//! amount, so a test neither races ahead of the statement it waits for nor
//! sleeps longer than it has to.
//!
//! Included with `#[path]` by the tests that use it; see `scratch.rs`.

use std::time::Duration;

use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};

/// The server process behind `conn`: for a transaction, the one holding its
/// locks.
pub async fn backend_pid<C: ConnectionTrait>(conn: &C) -> i32 {
    conn.query_one_raw(Statement::from_string(
        DbBackend::Postgres,
        "SELECT pg_backend_pid() AS pid",
    ))
    .await
    .expect("the pid query must succeed")
    .and_then(|row| row.try_get::<i32>("", "pid").ok())
    .expect("the server answers with its pid")
}

/// How many statements wait on a lock that the process `holder` holds,
/// directly or behind another waiter: a second request for a row waits on
/// the first one in the queue, not on the holder.
async fn blocked_by(db: &DatabaseConnection, holder: i32) -> i64 {
    db.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "WITH RECURSIVE waiting(pid) AS ( \
             SELECT pid FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid)) \
             UNION \
             SELECT activity.pid FROM pg_stat_activity activity \
             JOIN waiting ON waiting.pid = ANY(pg_blocking_pids(activity.pid)) \
         ) \
         SELECT count(*) AS n FROM waiting",
        [holder.into()],
    ))
    .await
    .expect("the activity query must succeed")
    .and_then(|row| row.try_get::<i64>("", "n").ok())
    .unwrap_or(0)
}

/// Waits until `count` statements wait on a lock that `holder` holds,
/// directly or not (see `blocked_by`).
pub async fn wait_for_blocked(db: &DatabaseConnection, holder: i32, count: i64) {
    for _ in 0..200 {
        if blocked_by(db, holder).await >= count {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("{count} statement(s) never waited on the locks of process {holder}");
}

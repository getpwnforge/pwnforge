//! Throwaway databases, for the tests that need a state the shared database
//! is past (an instance not set up yet, a schema behind its last migration)
//! or that must see no rows but their own. CI provisions the instance and
//! sets `DATABASE_URL`; locally the dev compose exposes one.
//!
//! Included with `#[path]` by the tests that use it, rather than through one
//! `common` module: each test binary compiles what it includes, and an unused
//! helper would be dead code in that binary.

use sea_orm::{ConnectionTrait, Database, DatabaseConnection};
use uuid::Uuid;

pub fn base_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://pwnforge:changeme@127.0.0.1:5432/pwnforge".to_owned())
}

/// Swaps the database name in a connection URL, keeping everything else.
///
/// Splits on the last `/` rather than parsing the URL: the database name is
/// the only part that changes, and a password may contain characters a naive
/// parser would trip on.
fn with_database(url: &str, database: &str) -> String {
    let (prefix, _old) = url
        .rsplit_once('/')
        .expect("DATABASE_URL must contain a database name");

    format!("{prefix}/{database}")
}

async fn admin_connection() -> DatabaseConnection {
    // `postgres` always exists and is never the database under test, so it is
    // safe to issue CREATE/DROP DATABASE from.
    Database::connect(with_database(&base_url(), "postgres"))
        .await
        .expect("a Postgres instance must be reachable (see the module docs)")
}

/// Runs `body` against a fresh, empty database and drops it afterwards, even
/// when an assertion fails.
///
/// The body runs in a task so that a failed assertion surfaces as a
/// `JoinError` here rather than unwinding straight out of the test. Without
/// it, every red run would leave a scratch database behind, and the
/// twentieth failure would be a disk-space problem instead of a test failure.
pub async fn in_scratch_database<F, Fut>(body: F)
where
    F: FnOnce(DatabaseConnection) -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    // Unique per run, so a leftover from an interrupted run cannot collide
    // with this one. Rendered without dashes because it has to be a valid
    // unquoted identifier.
    let name = format!("pwnforge_scratch_test_{}", Uuid::now_v7().simple());
    admin_connection()
        .await
        .execute_unprepared(&format!("CREATE DATABASE {name}"))
        .await
        .expect("must be able to create a scratch database");

    let db = Database::connect(with_database(&base_url(), &name))
        .await
        .expect("the scratch database must be reachable");

    let outcome = tokio::spawn(body(db)).await;

    // `WITH (FORCE)` because a pooled connection can outlive `close()` by a
    // moment, and DROP DATABASE refuses while anything is still attached.
    admin_connection()
        .await
        .execute_unprepared(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"))
        .await
        .expect("must be able to drop the scratch database");

    if let Err(join_error) = outcome {
        // Re-raise with the original panic payload, so the failure reads as
        // the assertion that actually failed.
        std::panic::resume_unwind(join_error.into_panic());
    }
}

//! Integration test for the `avatar_version` migration, both directions.
//!
//! Needs a real Postgres: what is under test is DDL, and the point of `down()`
//! is that it actually runs against a database that already has the column.
//! CI provisions the instance and sets `DATABASE_URL`; locally the dev compose
//! exposes one.
//!
//! `down()` is exercised rather than merely present. A migration whose `down()`
//! has never run is a migration that does not roll back: the usual failures -
//! dropping in an order the foreign keys refuse, or naming a column that was
//! spelled differently in `up()` - only show up when it executes.
//!
//! ## Why this creates its own database
//!
//! Rolling a migration back is destructive, and `cargo test` runs this crate's
//! test binaries **in parallel** against one `DATABASE_URL`. Doing it in place
//! would drop a column out from under `team_invitations` and
//! `session_rotation` while they are mid-run, and the failure would land on
//! them rather than here. So this creates a throwaway database, migrates it
//! from empty, works there, and drops it - which also means `up()` is
//! exercised from nothing rather than against whatever state the shared
//! database happened to be in.

use migration::MigratorTrait;
use sea_orm::{ConnectionTrait, DatabaseConnection, Statement};

#[path = "common/migrations.rs"]
mod migrations;
#[path = "common/scratch.rs"]
mod scratch;

use migrations::position_of;
use scratch::in_scratch_database;

const MIGRATION: &str = "m20260922_101500_avatar_version";

/// The three tables the migration touches.
const TARGETS: [&str; 3] = ["users", "teams", "workspaces"];

/// Reads the column's type straight out of the catalog, so the assertion is
/// about the schema rather than about what SeaORM believes the schema is.
async fn column_type(db: &DatabaseConnection, table: &str, column: &str) -> Option<String> {
    let statement = Statement::from_sql_and_values(
        db.get_database_backend(),
        "SELECT data_type FROM information_schema.columns \
         WHERE table_name = $1 AND column_name = $2",
        [table.into(), column.into()],
    );

    db.query_one_raw(statement)
        .await
        .expect("catalog query must succeed")
        .map(|row| {
            row.try_get::<String>("", "data_type")
                .expect("data_type is a text column")
        })
}

/// `up` adds a nullable uuid to all three tables, `down` removes it from all
/// three, and `up` can then re-apply - which is the only thing that makes a
/// rollback useful in practice.
///
/// One test rather than three: the migration is one unit, and three tests
/// would each need a scratch database of their own to avoid racing.
#[tokio::test]
async fn avatar_version_applies_and_rolls_back() {
    in_scratch_database(|db| async move {
        assertions(&db).await;
        db.close().await.ok();
    })
    .await;
}

async fn assertions(db: &DatabaseConnection) {
    // From empty: this scratch database has never been migrated.
    migration::Migrator::up(db, None)
        .await
        .expect("migrations must apply to an empty database");

    for table in TARGETS {
        assert_eq!(
            column_type(db, table, "avatar_version").await.as_deref(),
            Some("uuid"),
            "{table}.avatar_version must exist as a uuid after up()"
        );
    }

    // Nullable with no default: NULL is how "no avatar" is represented, and a
    // default would make every existing row claim one.
    let statement = Statement::from_sql_and_values(
        db.get_database_backend(),
        "SELECT is_nullable, column_default FROM information_schema.columns \
         WHERE table_name = 'users' AND column_name = 'avatar_version'",
        [],
    );
    let row = db
        .query_one_raw(statement)
        .await
        .expect("catalog query must succeed")
        .expect("the column must exist");

    assert_eq!(
        row.try_get::<String>("", "is_nullable")
            .expect("is_nullable is a text column"),
        "YES",
        "avatar_version must be nullable: NULL means no avatar"
    );
    assert_eq!(
        row.try_get::<Option<String>>("", "column_default")
            .expect("column_default is nullable text"),
        None,
        "avatar_version must have no default"
    );

    // --- down ---
    // Rolled back by a count derived from the migrator's own list, not by a
    // hardcoded 1. `down(Some(1))` undoes whichever migration happens to be
    // last, so the moment another migration is added after this one the test
    // would quietly exercise that one's `down()` instead - which is exactly
    // what happened when `drop_avatar_url` landed. Everything from the target
    // to the end is unwound, so each of their `down()`s runs too.
    let steps = migration::Migrator::migrations().len() as u32 - position_of(MIGRATION);

    migration::Migrator::down(db, Some(steps))
        .await
        .expect("down() must roll the migration back");

    for table in TARGETS {
        assert_eq!(
            column_type(db, table, "avatar_version").await,
            None,
            "{table}.avatar_version must be gone after down()"
        );
    }

    // `down()` drops a column, never a table. A mistake there would take user
    // data with it, so the tables are checked to still be standing.
    for table in TARGETS {
        assert!(
            column_type(db, table, "id").await.is_some(),
            "{table} must still exist after down()"
        );
    }

    let rolled_back = Statement::from_sql_and_values(
        db.get_database_backend(),
        "SELECT count(*) AS n FROM seaql_migrations WHERE version = $1",
        [MIGRATION.into()],
    );
    assert_eq!(
        db.query_one_raw(rolled_back)
            .await
            .expect("migration table query must succeed")
            .expect("one row")
            .try_get::<i64>("", "n")
            .expect("count is bigint"),
        0,
        "down() must also un-record the migration"
    );

    // --- up again ---
    migration::Migrator::up(db, None)
        .await
        .expect("up() must re-apply after a rollback");

    for table in TARGETS {
        assert_eq!(
            column_type(db, table, "avatar_version").await.as_deref(),
            Some("uuid"),
            "{table}.avatar_version must be back after re-applying"
        );
    }
}

//! Integration tests for the CHECK constraints `workspaces_enum_checks` puts
//! on the text columns of `workspaces`.
//!
//! Each negative test writes a row straight through the entity, the way the
//! admin CLI, a data migration or hand SQL could, so the only thing between
//! the bad value and the table is the constraint under test. Every other
//! field of the row is valid, so the error can only come from that one
//! constraint, and the test checks it is named.
//!
//! These need a real Postgres: `MockDatabase` evaluates no constraint. CI
//! provisions the instance and sets `DATABASE_URL`; locally the dev compose
//! exposes one.

use std::sync::atomic::{AtomicU32, Ordering};

use domain::entities::{users, workspaces};
use migration::MigratorTrait;
use sea_orm::{ActiveModelTrait, Database, DatabaseConnection, Set, TransactionTrait};
use uuid::Uuid;

async fn connect() -> DatabaseConnection {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://pwnforge:changeme@127.0.0.1:5432/pwnforge".to_owned());

    let db = Database::connect(url)
        .await
        .expect("a Postgres instance must be reachable (see the module docs)");

    migration::Migrator::up(&db, None)
        .await
        .expect("migrations must apply");

    db
}

fn unique_suffix() -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nth = COUNTER.fetch_add(1, Ordering::Relaxed);

    format!("{}-{nth}", Uuid::now_v7().simple())
}

async fn create_user(db: &DatabaseConnection) -> Uuid {
    users::ActiveModel {
        id: Set(Uuid::now_v7()),
        username: Set(format!("ws-checks-test-{}", unique_suffix())),
        password_hash: Set("not-a-real-hash".to_owned()),
        locale: Set("en".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture user must insert")
    .id
}

/// A solo workspace that satisfies every constraint. Each test changes one
/// field of it.
fn valid_workspace(owner_id: Uuid) -> workspaces::ActiveModel {
    let suffix = unique_suffix();

    workspaces::ActiveModel {
        id: Set(Uuid::now_v7()),
        owner_id: Set(Some(owner_id)),
        slug: Set(format!("ws-checks-test-{suffix}")),
        name: Set(format!("Workspace Checks Test {suffix}")),
        color: Set("ember".to_owned()),
        r#type: Set("platform".to_owned()),
        platform: Set(Some("htb".to_owned())),
        mode: Set("solo".to_owned()),
        default_writeup_visibility: Set("draft".to_owned()),
        ..Default::default()
    }
}

/// Asserts that inserting `row` is refused by `constraint` and by nothing
/// else.
///
/// The insert runs in a transaction that is always rolled back. Should the
/// constraint ever be missing, the invalid row must not outlive the test: a
/// value the migration does not normalise ("root_me", "public") would
/// otherwise make the next run of the migration refuse to add the constraint
/// back.
///
/// The driver's message is the only place the violated constraint is named
/// (`sql_err()` reports nothing for a CHECK), so the assertion reads it. The
/// name is matched with its quotes: `ck_workspaces_platform` is a prefix of
/// `ck_workspaces_platform_required`, and a bare substring match would let
/// one stand in for the other.
async fn assert_refused_by(
    db: &DatabaseConnection,
    row: workspaces::ActiveModel,
    constraint: &str,
) {
    let txn = db.begin().await.expect("a transaction must open");
    let result = row.insert(&txn).await;
    txn.rollback()
        .await
        .expect("the transaction must roll back");

    let err = result.expect_err("the insert must be refused");
    let message = err.to_string();
    let needle = format!("check constraint \"{constraint}\"");

    assert!(
        message.contains(&needle),
        "expected a violation of {constraint}, got {message}"
    );
}

/// Positive: rows inside every closed set are accepted, including the two
/// shapes the platform rule allows, a platform workspace with its platform
/// and an event with none.
#[tokio::test]
async fn a_workspace_inside_every_closed_set_is_accepted() {
    let db = connect().await;
    let owner = create_user(&db).await;

    let platform = valid_workspace(owner).insert(&db).await;
    assert!(
        platform.is_ok(),
        "a platform workspace must insert: {platform:?}"
    );

    let event = workspaces::ActiveModel {
        r#type: Set("event".to_owned()),
        platform: Set(None),
        color: Set("amber".to_owned()),
        default_writeup_visibility: Set("team".to_owned()),
        ..valid_workspace(owner)
    }
    .insert(&db)
    .await;
    assert!(
        event.is_ok(),
        "an event without a platform must insert: {event:?}"
    );
}

/// Negative: a hex code is the shape the frontend cannot resolve.
#[tokio::test]
async fn a_color_outside_the_palette_is_refused() {
    let db = connect().await;
    let owner = create_user(&db).await;

    assert_refused_by(
        &db,
        workspaces::ActiveModel {
            color: Set("#EB5927".to_owned()),
            ..valid_workspace(owner)
        },
        "ck_workspaces_color",
    )
    .await;
}

/// Negative: 'ctf' is the legacy value the migration normalised away; it must
/// not come back.
#[tokio::test]
async fn an_unknown_type_is_refused() {
    let db = connect().await;
    let owner = create_user(&db).await;

    assert_refused_by(
        &db,
        workspaces::ActiveModel {
            r#type: Set("ctf".to_owned()),
            ..valid_workspace(owner)
        },
        "ck_workspaces_type",
    )
    .await;
}

/// Negative: "root_me" is the API spelling, not the stored one. An event, so
/// the platform-required rule is not the one that fires.
#[tokio::test]
async fn an_unknown_platform_is_refused() {
    let db = connect().await;
    let owner = create_user(&db).await;

    assert_refused_by(
        &db,
        workspaces::ActiveModel {
            r#type: Set("event".to_owned()),
            platform: Set(Some("root_me".to_owned())),
            ..valid_workspace(owner)
        },
        "ck_workspaces_platform",
    )
    .await;
}

/// Negative: a platform workspace has to name its platform.
#[tokio::test]
async fn a_platform_workspace_without_a_platform_is_refused() {
    let db = connect().await;
    let owner = create_user(&db).await;

    assert_refused_by(
        &db,
        workspaces::ActiveModel {
            platform: Set(None),
            ..valid_workspace(owner)
        },
        "ck_workspaces_platform_required",
    )
    .await;
}

/// Negative: only 'draft' and 'team' exist.
#[tokio::test]
async fn an_unknown_writeup_visibility_is_refused() {
    let db = connect().await;
    let owner = create_user(&db).await;

    assert_refused_by(
        &db,
        workspaces::ActiveModel {
            default_writeup_visibility: Set("public".to_owned()),
            ..valid_workspace(owner)
        },
        "ck_workspaces_visibility",
    )
    .await;
}

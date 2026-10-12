//! The migration that makes `ck_workspaces_ownership` exclusive: what the
//! check refuses afterwards, the refusal of a row it has no answer for, and
//! `down()`.
//!
//! Each test works in a throwaway database of its own, migrated up to the
//! migration before this one, because a row breaking the new rule can only be
//! written under the previous check. CI provisions the instance and sets
//! `DATABASE_URL`; locally the dev compose exposes one.

use domain::entities::{teams, users, workspaces};
use migration::MigratorTrait;
use sea_orm::{
    ActiveModelTrait, ConnectionTrait, DatabaseConnection, DbBackend, DbErr, EntityTrait, Set,
    Statement,
};
use uuid::Uuid;

#[path = "common/migrations.rs"]
mod migrations;
#[path = "common/scratch.rs"]
mod scratch;

use migrations::position_of;
use scratch::in_scratch_database;

const MIGRATION: &str = "m20261012_090000_workspaces_exclusive_ownership";

/// Applies every migration before the one under test, found by name.
async fn migrate_to_just_before(db: &DatabaseConnection) {
    migration::Migrator::up(db, Some(position_of(MIGRATION)))
        .await
        .expect("the earlier migrations must apply");
}

async fn create_owner(db: &DatabaseConnection) -> Uuid {
    users::ActiveModel {
        id: Set(Uuid::now_v7()),
        username: Set(format!("own-{}", Uuid::now_v7().simple())),
        password_hash: Set("not-a-real-hash".to_owned()),
        locale: Set("en".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture user must insert")
    .id
}

async fn create_team(db: &DatabaseConnection) -> Uuid {
    teams::ActiveModel {
        id: Set(Uuid::now_v7()),
        slug: Set(format!("own-{}", Uuid::now_v7().simple())),
        name: Set("Ownership".to_owned()),
        color: Set("ember".to_owned()),
        personal: Set(false),
        created_by: Set(None),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture team must insert")
    .id
}

async fn insert_workspace(
    db: &DatabaseConnection,
    mode: &str,
    team_id: Option<Uuid>,
    owner_id: Option<Uuid>,
) -> Result<workspaces::Model, DbErr> {
    workspaces::ActiveModel {
        id: Set(Uuid::now_v7()),
        team_id: Set(team_id),
        owner_id: Set(owner_id),
        slug: Set(format!("own-{}", Uuid::now_v7().simple())),
        name: Set("Ownership".to_owned()),
        color: Set("ember".to_owned()),
        r#type: Set("event".to_owned()),
        mode: Set(mode.to_owned()),
        default_writeup_visibility: Set("draft".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
}

/// The definition Postgres holds for the check, as it would print it.
async fn ownership_definition(db: &DatabaseConnection) -> Option<String> {
    db.query_one_raw(Statement::from_string(
        DbBackend::Postgres,
        "SELECT pg_get_constraintdef(oid) AS definition FROM pg_constraint \
         WHERE conrelid = 'workspaces'::regclass AND conname = 'ck_workspaces_ownership'",
    ))
    .await
    .expect("the catalog query must succeed")
    .map(|row| {
        row.try_get::<String>("", "definition")
            .expect("the definition is text")
    })
}

/// The check is read from the message: `sql_err()` classifies nothing for a
/// CHECK violation.
fn assert_refused(result: Result<workspaces::Model, DbErr>, what: &str) {
    let message = result
        .expect_err(&format!("{what} must be refused"))
        .to_string();
    assert!(
        message.contains("check constraint \"ck_workspaces_ownership\""),
        "{what} must be refused by ck_workspaces_ownership, got {message}"
    );
}

/// Each mode is accepted with its own owner only, a team workspace with an
/// owner and a solo workspace with a team are refused, and `down()` puts the
/// previous definition back.
#[tokio::test]
async fn the_migration_makes_ownership_exclusive_and_rolls_back() {
    in_scratch_database(|db| async move {
        migrate_to_just_before(&db).await;
        let owner = create_owner(&db).await;
        let team = create_team(&db).await;
        let before = ownership_definition(&db).await;
        assert!(before.is_some(), "the previous check must exist");

        // Exactly one step: the migration under test.
        migration::Migrator::up(&db, Some(1))
            .await
            .expect("the migration must apply");
        assert_ne!(
            ownership_definition(&db).await,
            before,
            "the check must change"
        );

        insert_workspace(&db, "team", Some(team), None)
            .await
            .expect("a team workspace with a team and no owner inserts");
        insert_workspace(&db, "solo", None, Some(owner))
            .await
            .expect("a solo workspace with an owner and no team inserts");

        assert_refused(
            insert_workspace(&db, "team", Some(team), Some(owner)).await,
            "a team workspace with an owner",
        );
        assert_refused(
            insert_workspace(&db, "solo", Some(team), Some(owner)).await,
            "a solo workspace with a team",
        );

        migration::Migrator::down(&db, Some(1))
            .await
            .expect("down() must roll the migration back");
        assert_eq!(
            ownership_definition(&db).await,
            before,
            "down() must restore the previous check exactly"
        );
        insert_workspace(&db, "team", Some(team), Some(owner))
            .await
            .expect("under the previous check a team workspace with an owner inserts again");

        db.close().await.ok();
    })
    .await;
}

/// A row the new rule refuses has no answer the migration could apply, so
/// the migration fails, names the row, and changes nothing.
#[tokio::test]
async fn a_row_breaking_the_rule_fails_the_migration() {
    in_scratch_database(|db| async move {
        migrate_to_just_before(&db).await;
        let owner = create_owner(&db).await;
        let team = create_team(&db).await;
        let before = ownership_definition(&db).await;

        let breaking = insert_workspace(&db, "team", Some(team), Some(owner))
            .await
            .expect("the previous check accepts a team workspace with an owner");

        let message = migration::Migrator::up(&db, Some(1))
            .await
            .expect_err("the migration must fail on that row")
            .to_string();
        assert!(
            message.contains(&breaking.id.to_string()) && message.contains("1 workspace"),
            "the failure must count and name the row, got {message}"
        );

        assert_eq!(
            ownership_definition(&db).await,
            before,
            "the previous check must still be in place"
        );
        assert_eq!(
            migration::Migrator::get_pending_migrations(&db)
                .await
                .expect("the pending migrations must load")
                .len(),
            1,
            "the migration must not be recorded as applied"
        );
        let kept = workspaces::Entity::find_by_id(breaking.id)
            .one(&db)
            .await
            .expect("the lookup must succeed")
            .expect("the row must still exist");
        assert_eq!(
            (kept.team_id, kept.owner_id),
            (Some(team), Some(owner)),
            "the row must be left as it was"
        );

        db.close().await.ok();
    })
    .await;
}

//! The migration that adds `teams.description` and
//! `teams.members_can_create_workspaces`: what an existing team reads once it
//! applies, the length check, and `down()`.
//!
//! Works in a throwaway database migrated up to the migration before this
//! one, so a team can exist before the columns do. CI provisions the instance
//! and sets `DATABASE_URL`; locally the dev compose exposes one.

use migration::MigratorTrait;
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, DbErr, Statement};
use uuid::Uuid;

#[path = "common/migrations.rs"]
mod migrations;
#[path = "common/scratch.rs"]
mod scratch;

use migrations::position_of;
use scratch::in_scratch_database;

const MIGRATION: &str = "m20261102_090000_team_settings";

/// Written with a statement rather than through the entity: the entity reads
/// back the columns this schema does not have yet.
async fn insert_team(db: &DatabaseConnection) -> Uuid {
    let id = Uuid::now_v7();
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO teams (id, slug, name, color) VALUES ($1, $2, 'Settings', 'ember')",
        [id.into(), format!("ts-{}", id.simple()).into()],
    ))
    .await
    .expect("the fixture team must insert");
    id
}

/// Which of the two columns exist on `teams`, in name order.
async fn present_columns(db: &DatabaseConnection) -> Vec<String> {
    db.query_all_raw(Statement::from_string(
        DbBackend::Postgres,
        "SELECT column_name FROM information_schema.columns \
         WHERE table_name = 'teams' \
           AND column_name IN ('description', 'members_can_create_workspaces') \
         ORDER BY column_name",
    ))
    .await
    .expect("the catalog query must succeed")
    .into_iter()
    .map(|row| {
        row.try_get::<String>("", "column_name")
            .expect("column_name is text")
    })
    .collect()
}

/// The two settings of `team_id`, as stored.
async fn settings(db: &DatabaseConnection, team_id: Uuid) -> (Option<String>, bool) {
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT description, members_can_create_workspaces FROM teams WHERE id = $1",
            [team_id.into()],
        ))
        .await
        .expect("the lookup must succeed")
        .expect("the team must exist");

    (
        row.try_get("", "description")
            .expect("description is nullable text"),
        row.try_get("", "members_can_create_workspaces")
            .expect("members_can_create_workspaces is a boolean"),
    )
}

async fn set_description(
    db: &DatabaseConnection,
    team_id: Uuid,
    description: &str,
) -> Result<(), DbErr> {
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "UPDATE teams SET description = $1 WHERE id = $2",
        [description.into(), team_id.into()],
    ))
    .await
    .map(|_| ())
}

/// An existing team reads as no description and the old rule, the check
/// counts characters rather than bytes and refuses the 501st, and `down()`
/// drops both columns while the team stays. `up()` applies again after it.
#[tokio::test]
async fn the_migration_adds_both_settings_and_rolls_back() {
    in_scratch_database(|db| async move {
        migration::Migrator::up(&db, Some(position_of(MIGRATION)))
            .await
            .expect("the earlier migrations must apply");
        let team = insert_team(&db).await;
        assert!(present_columns(&db).await.is_empty(), "no column yet");

        // Exactly one step: the migration under test.
        migration::Migrator::up(&db, Some(1))
            .await
            .expect("the migration must apply");

        assert_eq!(
            present_columns(&db).await,
            ["description", "members_can_create_workspaces"]
        );
        assert_eq!(
            settings(&db, team).await,
            (None, false),
            "an existing team has no description and keeps the Admin rule"
        );

        // 500 two-byte characters: 1000 bytes, still within the bound.
        set_description(&db, team, &"é".repeat(500))
            .await
            .expect("500 characters is the ceiling, not past it");
        let refused = set_description(&db, team, &"a".repeat(501))
            .await
            .expect_err("a 501-character description must be refused");
        assert!(
            refused.to_string().contains("ck_teams_description_length"),
            "the refusal must name the check, got {refused}"
        );

        migration::Migrator::down(&db, Some(1))
            .await
            .expect("down() must roll the migration back");

        assert!(
            present_columns(&db).await.is_empty(),
            "down() drops both columns"
        );
        let remaining = db
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT count(*) AS n FROM teams WHERE id = $1",
                [team.into()],
            ))
            .await
            .expect("the lookup must succeed")
            .and_then(|row| row.try_get::<i64>("", "n").ok());
        assert_eq!(remaining, Some(1), "the team itself stays");

        migration::Migrator::up(&db, Some(1))
            .await
            .expect("the migration must apply again after down()");
        assert_eq!(settings(&db, team).await, (None, false));

        db.close().await.ok();
    })
    .await;
}

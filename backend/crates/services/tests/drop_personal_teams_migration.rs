//! The migration that drops personal teams: what becomes of their
//! workspaces, with a creator and without, what it leaves alone, and
//! `down()`, which restores the schema and not the data.
//!
//! Each test works in a throwaway database of its own, migrated up to the
//! migration before this one, the last point where `teams.personal` exists.
//! The entity no longer has that column, so teams are written with a
//! statement of their own. CI provisions the instance and sets
//! `DATABASE_URL`; locally the dev compose exposes one.

use chrono::{Duration, Utc};
use domain::entities::{teams, users, workspace_favorites, workspaces};
use migration::MigratorTrait;
use sea_orm::{
    ActiveModelTrait, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait, Set, Statement,
};
use uuid::Uuid;

#[path = "common/migrations.rs"]
mod migrations;
#[path = "common/scratch.rs"]
mod scratch;

use migrations::position_of;
use scratch::in_scratch_database;

const MIGRATION: &str = "m20261014_090000_drop_personal_teams";

async fn migrate_to_just_before(db: &DatabaseConnection) {
    migration::Migrator::up(db, Some(position_of(MIGRATION)))
        .await
        .expect("the earlier migrations must apply");
}

async fn create_user(db: &DatabaseConnection) -> Uuid {
    users::ActiveModel {
        id: Set(Uuid::now_v7()),
        username: Set(format!("dp-{}", Uuid::now_v7().simple())),
        password_hash: Set("not-a-real-hash".to_owned()),
        locale: Set("en".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture user must insert")
    .id
}

/// A team as the schema before the migration holds it, with its creator as
/// Captain when there is one.
async fn insert_team(db: &DatabaseConnection, personal: bool, created_by: Option<Uuid>) -> Uuid {
    let id = Uuid::now_v7();
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO teams (id, slug, name, color, personal, created_by) \
         VALUES ($1, $2, $3, $4, $5, $6)",
        [
            id.into(),
            format!("dp-{}", id.simple()).into(),
            "Personal".into(),
            "ember".into(),
            personal.into(),
            created_by.into(),
        ],
    ))
    .await
    .expect("the fixture team must insert");

    if let Some(creator) = created_by {
        add_team_member(db, id, creator, "captain").await;
    }
    id
}

/// Membership rows, overrides and invitations are written and counted with
/// statements too: the entities read back columns this schema predates
/// (`invited_by`, and the targeted shape of `workspace_invitations`).
async fn add_team_member(db: &DatabaseConnection, team_id: Uuid, user_id: Uuid, role: &str) {
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO team_members (team_id, user_id, role) VALUES ($1, $2, $3)",
        [team_id.into(), user_id.into(), role.into()],
    ))
    .await
    .expect("the fixture membership must insert");
}

/// A team workspace with a member override, an invitation, and favourites by
/// each user given.
async fn team_workspace(
    db: &DatabaseConnection,
    team_id: Uuid,
    member: Uuid,
    favourited_by: &[Uuid],
) -> Uuid {
    let id = workspaces::ActiveModel {
        id: Set(Uuid::now_v7()),
        team_id: Set(Some(team_id)),
        slug: Set(format!("dp-{}", Uuid::now_v7().simple())),
        name: Set("Workspace".to_owned()),
        color: Set("ember".to_owned()),
        r#type: Set("event".to_owned()),
        mode: Set("team".to_owned()),
        default_writeup_visibility: Set("draft".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture workspace must insert")
    .id;

    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO workspace_members (workspace_id, user_id, role, is_guest) \
         VALUES ($1, $2, 'viewer', false)",
        [id.into(), member.into()],
    ))
    .await
    .expect("the fixture override must insert");

    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO workspace_invitations (id, workspace_id, token, default_role, expires_at) \
         VALUES ($1, $2, $3, 'member', $4)",
        [
            Uuid::now_v7().into(),
            id.into(),
            format!("hash-{}", Uuid::now_v7().simple()).into(),
            (Utc::now() + Duration::days(7)).fixed_offset().into(),
        ],
    ))
    .await
    .expect("the fixture invitation must insert");

    for user in favourited_by {
        workspace_favorites::ActiveModel {
            user_id: Set(*user),
            workspace_id: Set(id),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("the fixture favourite must insert");
    }

    id
}

async fn workspace(db: &DatabaseConnection, id: Uuid) -> Option<workspaces::Model> {
    workspaces::Entity::find_by_id(id)
        .one(db)
        .await
        .expect("the lookup must succeed")
}

async fn team_exists(db: &DatabaseConnection, id: Uuid) -> bool {
    teams::Entity::find_by_id(id)
        .one(db)
        .await
        .expect("the lookup must succeed")
        .is_some()
}

async fn count_on(db: &DatabaseConnection, sql: &str, workspace_id: Uuid) -> usize {
    db.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        sql,
        [workspace_id.into()],
    ))
    .await
    .expect("the lookup must succeed")
    .and_then(|row| row.try_get::<i64>("", "n").ok())
    .expect("a count returns a row") as usize
}

async fn rows_on(db: &DatabaseConnection, workspace_id: Uuid) -> (usize, usize) {
    (
        count_on(
            db,
            "SELECT count(*) AS n FROM workspace_members WHERE workspace_id = $1",
            workspace_id,
        )
        .await,
        count_on(
            db,
            "SELECT count(*) AS n FROM workspace_invitations WHERE workspace_id = $1",
            workspace_id,
        )
        .await,
    )
}

async fn favourited(db: &DatabaseConnection, workspace_id: Uuid, user_id: Uuid) -> bool {
    workspace_favorites::Entity::find_by_id((user_id, workspace_id))
        .one(db)
        .await
        .expect("the lookup must succeed")
        .is_some()
}

async fn catalog_has(db: &DatabaseConnection, sql: &str) -> bool {
    db.query_one_raw(Statement::from_string(DbBackend::Postgres, sql.to_owned()))
        .await
        .expect("the catalog query must succeed")
        .is_some()
}

async fn personal_schema_exists(db: &DatabaseConnection) -> (bool, bool) {
    (
        catalog_has(
            db,
            "SELECT 1 FROM information_schema.columns \
             WHERE table_name = 'teams' AND column_name = 'personal'",
        )
        .await,
        catalog_has(
            db,
            "SELECT 1 FROM pg_indexes WHERE indexname = 'idx_teams_one_personal_per_creator'",
        )
        .await,
    )
}

/// A personal team's workspace becomes its creator's solo workspace, with
/// no members, no invitations and only the creator's favourite left; one
/// whose creator is gone is deleted; an ordinary team is not touched. The
/// teams go, then the column and its index. `down()` brings the column and
/// the index back, and none of the data.
#[tokio::test]
async fn personal_teams_go_and_their_workspaces_become_solo() {
    in_scratch_database(|db| async move {
        migrate_to_just_before(&db).await;
        let creator = create_user(&db).await;
        let other = create_user(&db).await;

        let personal = insert_team(&db, true, Some(creator)).await;
        add_team_member(&db, personal, other, "member").await;
        let kept = team_workspace(&db, personal, other, &[creator, other]).await;

        let orphaned = insert_team(&db, true, None).await;
        let lost = team_workspace(&db, orphaned, other, &[other]).await;

        let ordinary_captain = create_user(&db).await;
        let ordinary = insert_team(&db, false, Some(ordinary_captain)).await;
        add_team_member(&db, ordinary, other, "member").await;
        let untouched = team_workspace(&db, ordinary, other, &[ordinary_captain, other]).await;

        // Exactly one step: the migration under test.
        migration::Migrator::up(&db, Some(1))
            .await
            .expect("the migration must apply");

        let solo = workspace(&db, kept).await.expect("the workspace must stay");
        assert_eq!(
            (solo.mode.as_str(), solo.owner_id, solo.team_id),
            ("solo", Some(creator), None),
            "it is its creator's solo workspace"
        );
        assert_eq!(
            rows_on(&db, kept).await,
            (0, 0),
            "no members, no invitations"
        );
        assert!(
            favourited(&db, kept, creator).await,
            "the creator's favourite stays"
        );
        assert!(!favourited(&db, kept, other).await, "nobody else's");
        assert!(
            !team_exists(&db, personal).await,
            "the personal team is gone"
        );

        assert!(
            workspace(&db, lost).await.is_none(),
            "a deleted account's workspace goes"
        );
        assert!(!team_exists(&db, orphaned).await);

        let team_workspace = workspace(&db, untouched).await.expect("it must stay");
        assert_eq!(
            (team_workspace.mode.as_str(), team_workspace.team_id),
            ("team", Some(ordinary)),
            "an ordinary team's workspace is not converted"
        );
        assert_eq!(rows_on(&db, untouched).await, (1, 1), "nor stripped");
        assert!(favourited(&db, untouched, other).await);
        assert!(team_exists(&db, ordinary).await);

        assert_eq!(
            personal_schema_exists(&db).await,
            (false, false),
            "the column and its index are gone"
        );

        migration::Migrator::down(&db, Some(1))
            .await
            .expect("down() must roll the migration back");
        assert_eq!(
            personal_schema_exists(&db).await,
            (true, true),
            "down() restores the column and its index"
        );
        assert!(!team_exists(&db, personal).await, "and none of the data");
        let still_solo = workspace(&db, kept).await.expect("the workspace must stay");
        assert_eq!(still_solo.mode, "solo");

        db.close().await.ok();
    })
    .await;
}

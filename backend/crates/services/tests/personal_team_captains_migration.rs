//! The migration that backfilled the Captain of every personal team that had
//! none, `m20261008_120000_personal_team_captains`.
//!
//! Personal teams are gone since `m20261014_090000_drop_personal_teams`, but
//! this migration is history and still runs on every fresh database, so it
//! stays tested at its own point: a throwaway database migrated up to just
//! before it, where `teams.personal` still exists. The entity no longer has
//! that column, so the teams are written with a statement of their own. CI
//! provisions the instance and sets `DATABASE_URL`; locally the dev compose
//! exposes one.

use domain::dto::team::TeamRole;
use domain::entities::{team_members, users};
use domain::types::AccentColor;
use migration::MigratorTrait;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait,
    QueryFilter, Set, Statement,
};
use uuid::Uuid;

#[path = "common/migrations.rs"]
mod migrations;
#[path = "common/scratch.rs"]
mod scratch;

use migrations::position_of;
use scratch::in_scratch_database;

async fn create_user(db: &DatabaseConnection, is_instance_admin: bool) -> users::Model {
    users::ActiveModel {
        id: Set(Uuid::now_v7()),
        username: Set(format!("pt-{}", Uuid::now_v7().simple())),
        password_hash: Set("not-a-real-hash".to_owned()),
        locale: Set("en".to_owned()),
        is_instance_admin: Set(is_instance_admin),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture user must insert")
}

/// A team as the schema before the backfill holds it, `personal` included.
async fn insert_team(
    db: &DatabaseConnection,
    slug: &str,
    personal: bool,
    created_by: Option<Uuid>,
) -> Uuid {
    let id = Uuid::now_v7();
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO teams (id, slug, name, color, personal, created_by) \
         VALUES ($1, $2, $3, $4, $5, $6)",
        [
            id.into(),
            slug.into(),
            slug.into(),
            AccentColor::default().as_str().into(),
            personal.into(),
            created_by.into(),
        ],
    ))
    .await
    .expect("the fixture team must insert");
    id
}

const BACKFILL: &str = "m20261008_120000_personal_team_captains";

/// The backfill inserts the missing Captain of a personal team and nothing
/// else: a personal team with a member, an ordinary team, an orphaned
/// personal team and an instance admin's personal team are left alone.
/// `down()` runs and removes nothing.
#[tokio::test]
async fn the_backfill_adds_missing_captains_and_nothing_else() {
    in_scratch_database(|db| async move {
        // Every migration before the backfill, found by name: an index would
        // point at another migration as soon as one is appended.
        migration::Migrator::up(&db, Some(position_of(BACKFILL)))
            .await
            .expect("the earlier migrations must apply");

        let empty_owner = create_user(&db, false).await;
        let empty = insert_team(&db, "empty-personal", true, Some(empty_owner.id)).await;

        let staffed_owner = create_user(&db, false).await;
        let staffed = insert_team(&db, "staffed-personal", true, Some(staffed_owner.id)).await;
        // Someone already holds it, and not as Captain: it must stay as is.
        team_members::ActiveModel {
            team_id: Set(staffed),
            user_id: Set(staffed_owner.id),
            role: Set(TeamRole::Admin.as_str().to_owned()),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("fixture member");

        let ordinary_owner = create_user(&db, false).await;
        let ordinary = insert_team(&db, "ordinary", false, Some(ordinary_owner.id)).await;

        let orphan = insert_team(&db, "orphan-personal", true, None).await;

        // Made by the old code before it refused instance admins.
        let admin = create_user(&db, true).await;
        let admins = insert_team(&db, "admin-personal", true, Some(admin.id)).await;

        // Exactly one step: the backfill and nothing appended after it.
        migration::Migrator::up(&db, Some(1))
            .await
            .expect("the backfill must apply");

        let members = |team_id: Uuid| {
            let db = &db;
            async move {
                team_members::Entity::find()
                    .filter(team_members::Column::TeamId.eq(team_id))
                    .all(db)
                    .await
                    .expect("the lookup must succeed")
            }
        };

        let backfilled = members(empty).await;
        assert_eq!(
            backfilled.len(),
            1,
            "the empty personal team gets one member"
        );
        assert_eq!(backfilled[0].user_id, empty_owner.id);
        assert_eq!(backfilled[0].role, TeamRole::Captain.as_str());

        let kept = members(staffed).await;
        assert_eq!(kept.len(), 1, "a staffed team gains no member");
        assert_eq!(kept[0].role, TeamRole::Admin.as_str(), "nor changes role");

        assert!(
            members(ordinary).await.is_empty(),
            "ordinary teams are ignored"
        );
        assert!(
            members(orphan).await.is_empty(),
            "an orphan has no one to add"
        );
        assert!(
            members(admins).await.is_empty(),
            "an instance admin is not made Captain of a personal team"
        );

        // The backfill is the last migration applied here, whatever comes
        // after it in the Migrator.
        migration::Migrator::down(&db, Some(1))
            .await
            .expect("down must run");
        assert_eq!(
            members(empty).await.len(),
            1,
            "down removes nothing it cannot tell apart"
        );

        db.close().await.ok();
    })
    .await;
}

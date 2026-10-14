//! The migration that allows one Captain per team: what the index and the
//! invitation check refuse afterwards, the refusal of rows it has no answer
//! for, and `down()`.
//!
//! Each test works in a throwaway database of its own, migrated up to the
//! migration before this one, because a team with two Captains, or an
//! invitation granting the role, can only be written before it. CI
//! provisions the instance and sets `DATABASE_URL`; locally the dev compose
//! exposes one.

use domain::entities::{team_members, teams, users};
use migration::MigratorTrait;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DbBackend, DbErr,
    EntityTrait, QueryFilter, Set, Statement,
};
use uuid::Uuid;

#[path = "common/migrations.rs"]
mod migrations;
#[path = "common/scratch.rs"]
mod scratch;

use migrations::position_of;
use scratch::in_scratch_database;

const MIGRATION: &str = "m20261012_120000_teams_single_captain";

/// Applies every migration before the one under test, found by name.
async fn migrate_to_just_before(db: &DatabaseConnection) {
    migration::Migrator::up(db, Some(position_of(MIGRATION)))
        .await
        .expect("the earlier migrations must apply");
}

async fn create_user(db: &DatabaseConnection) -> Uuid {
    users::ActiveModel {
        id: Set(Uuid::now_v7()),
        username: Set(format!("cpt-{}", Uuid::now_v7().simple())),
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
        slug: Set(format!("cpt-{}", Uuid::now_v7().simple())),
        name: Set("Captains".to_owned()),
        color: Set("ember".to_owned()),
        created_by: Set(None),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture team must insert")
    .id
}

/// Written with a statement rather than through the entity, as the two
/// below: the schema at this point predates `team_members.invited_by` and a
/// required `team_invitations.expires_at`, which the entities read back.
async fn insert_member(
    db: &DatabaseConnection,
    team_id: Uuid,
    user_id: Uuid,
    role: &str,
) -> Result<(), DbErr> {
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO team_members (team_id, user_id, role) VALUES ($1, $2, $3)",
        [team_id.into(), user_id.into(), role.into()],
    ))
    .await
    .map(|_| ())
}

async fn insert_invitation(
    db: &DatabaseConnection,
    team_id: Uuid,
    role: &str,
) -> Result<Uuid, DbErr> {
    let id = Uuid::now_v7();
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO team_invitations (id, team_id, token, default_role, max_uses) \
         VALUES ($1, $2, $3, $4, 1)",
        [
            id.into(),
            team_id.into(),
            format!("hash-{}", Uuid::now_v7().simple()).into(),
            role.into(),
        ],
    ))
    .await?;
    Ok(id)
}

async fn captains_of(db: &DatabaseConnection, team_id: Uuid) -> i64 {
    db.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT count(*) AS n FROM team_members WHERE team_id = $1 AND role = 'captain'",
        [team_id.into()],
    ))
    .await
    .expect("the lookup must succeed")
    .and_then(|row| row.try_get::<i64>("", "n").ok())
    .expect("a count returns a row")
}

async fn default_role_of(db: &DatabaseConnection, invitation_id: Uuid) -> Option<String> {
    db.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT default_role FROM team_invitations WHERE id = $1",
        [invitation_id.into()],
    ))
    .await
    .expect("the lookup must succeed")
    .map(|row| {
        row.try_get::<String>("", "default_role")
            .expect("the role is text")
    })
}

/// The definition Postgres holds for the invitation role check, as it would
/// print it.
async fn default_role_definition(db: &DatabaseConnection) -> Option<String> {
    db.query_one_raw(Statement::from_string(
        DbBackend::Postgres,
        "SELECT pg_get_constraintdef(oid) AS definition FROM pg_constraint \
         WHERE conrelid = 'team_invitations'::regclass \
         AND conname = 'team_invitations_default_role_check'",
    ))
    .await
    .expect("the catalog query must succeed")
    .map(|row| {
        row.try_get::<String>("", "definition")
            .expect("the definition is text")
    })
}

/// Whether Postgres holds the index under its name.
async fn index_exists(db: &DatabaseConnection) -> bool {
    db.query_one_raw(Statement::from_string(
        DbBackend::Postgres,
        "SELECT 1 AS found FROM pg_indexes \
         WHERE tablename = 'team_members' AND indexname = 'idx_team_members_one_captain'",
    ))
    .await
    .expect("the catalog query must succeed")
    .is_some()
}

/// A second Captain is refused by the index, and an invitation granting the
/// role by the check, each by name; another role in the same team, a
/// Captain in another team and an invitation granting another role are
/// not. `down()` drops the index and puts the previous check back exactly,
/// after which both refused rows insert again.
#[tokio::test]
async fn the_migration_allows_one_captain_per_team_and_rolls_back() {
    in_scratch_database(|db| async move {
        migrate_to_just_before(&db).await;
        let first = create_user(&db).await;
        let second = create_user(&db).await;
        let team = create_team(&db).await;
        let other_team = create_team(&db).await;
        insert_member(&db, team, first, "captain")
            .await
            .expect("the first Captain inserts");
        let previous_check = default_role_definition(&db).await;
        assert!(
            previous_check
                .as_deref()
                .is_some_and(|check| check.contains("'captain'")),
            "control: the previous check accepts captain, got {previous_check:?}"
        );

        // Exactly one step: the migration under test.
        migration::Migrator::up(&db, Some(1))
            .await
            .expect("the migration must apply");
        assert!(index_exists(&db).await, "the index must exist");
        let check = default_role_definition(&db).await;
        assert!(
            check
                .as_deref()
                .is_some_and(|check| !check.contains("'captain'") && check.contains("'viewer'")),
            "the check must accept every role but captain, got {check:?}"
        );

        let message = insert_invitation(&db, team, "captain")
            .await
            .expect_err("an invitation granting Captain must be refused")
            .to_string();
        assert!(
            message.contains("team_invitations_default_role_check"),
            "the refusal must come from the check, got {message}"
        );
        insert_invitation(&db, team, "admin")
            .await
            .expect("an invitation granting another role inserts");

        let message = insert_member(&db, team, second, "captain")
            .await
            .expect_err("a second Captain must be refused")
            .to_string();
        assert!(
            message.contains("idx_team_members_one_captain"),
            "the refusal must come from the index, got {message}"
        );

        insert_member(&db, team, second, "admin")
            .await
            .expect("another role in the same team inserts");
        insert_member(&db, other_team, second, "captain")
            .await
            .expect("a Captain of another team inserts");

        migration::Migrator::down(&db, Some(1))
            .await
            .expect("down() must roll the migration back");
        assert!(!index_exists(&db).await, "down() must drop the index");
        assert_eq!(
            default_role_definition(&db).await,
            previous_check,
            "down() must restore the previous check exactly"
        );

        insert_invitation(&db, team, "captain")
            .await
            .expect("under the previous check an invitation granting Captain inserts again");

        team_members::Entity::update_many()
            .col_expr(
                team_members::Column::Role,
                sea_orm::sea_query::Expr::value("captain"),
            )
            .filter(team_members::Column::TeamId.eq(team))
            .filter(team_members::Column::UserId.eq(second))
            .exec(&db)
            .await
            .expect("without the index a second Captain is written again");

        db.close().await.ok();
    })
    .await;
}

/// Teams with two Captains have no answer the migration could apply, so it
/// fails, counts and names them, and changes nothing.
#[tokio::test]
async fn teams_with_two_captains_fail_the_migration() {
    in_scratch_database(|db| async move {
        migrate_to_just_before(&db).await;
        let first = create_user(&db).await;
        let second = create_user(&db).await;
        let mut duplicated = Vec::new();
        for _ in 0..2 {
            let team = create_team(&db).await;
            for user in [first, second] {
                insert_member(&db, team, user, "captain")
                    .await
                    .expect("without the index two Captains insert");
            }
            duplicated.push(team);
        }

        let message = migration::Migrator::up(&db, Some(1))
            .await
            .expect_err("the migration must fail on those teams")
            .to_string();
        assert!(
            message.contains("2 team(s)")
                && duplicated
                    .iter()
                    .all(|team| message.contains(&team.to_string())),
            "the failure must count and name the teams, got {message}"
        );

        assert!(!index_exists(&db).await, "no index must be left behind");
        // The first one still pending, whatever comes after it.
        let pending = migration::Migrator::get_pending_migrations(&db)
            .await
            .expect("the pending migrations must load");
        assert_eq!(
            pending.first().map(|m| m.name()),
            Some(MIGRATION),
            "the migration must not be recorded as applied"
        );
        for team in duplicated {
            assert_eq!(
                captains_of(&db, team).await,
                2,
                "the rows must be left as they were"
            );
        }

        db.close().await.ok();
    })
    .await;
}

/// Invitations granting the Captain role, expired or not, have no answer
/// the migration could apply either: it fails, counts and names them, and
/// changes nothing.
#[tokio::test]
async fn invitations_granting_captain_fail_the_migration() {
    in_scratch_database(|db| async move {
        migrate_to_just_before(&db).await;
        let team = create_team(&db).await;
        let previous_check = default_role_definition(&db).await;
        let mut granting = Vec::new();
        for _ in 0..2 {
            let invitation = insert_invitation(&db, team, "captain")
                .await
                .expect("the previous check accepts an invitation granting Captain");
            granting.push(invitation);
        }

        let message = migration::Migrator::up(&db, Some(1))
            .await
            .expect_err("the migration must fail on those invitations")
            .to_string();
        assert!(
            message.contains("2 invitation(s)")
                && granting
                    .iter()
                    .all(|invitation| message.contains(&invitation.to_string())),
            "the failure must count and name the invitations, got {message}"
        );

        assert_eq!(
            default_role_definition(&db).await,
            previous_check,
            "the previous check must still be in place"
        );
        assert!(!index_exists(&db).await, "no index must be left behind");
        let pending = migration::Migrator::get_pending_migrations(&db)
            .await
            .expect("the pending migrations must load");
        assert_eq!(
            pending.first().map(|m| m.name()),
            Some(MIGRATION),
            "the migration must not be recorded as applied"
        );
        for invitation in granting {
            assert_eq!(
                default_role_of(&db, invitation).await.as_deref(),
                Some("captain"),
                "the row must be left as it was"
            );
        }

        db.close().await.ok();
    })
    .await;
}

//! `team_service::transfer_captaincy` as the admin CLI calls it, as an
//! instance administrator, and the two things that differ when the Captain
//! asks: the check that they still are, and the audit entry they do not
//! write.
//!
//! The CLI command itself is not driven here: its errors end the process
//! with `std::process::exit`, and building its context needs the whole
//! environment. It resolves the slug and the account, then calls this
//! function, so the refusals it shares with the route are the ones below.
//!
//! Each test works in a throwaway database of its own. CI provisions the
//! instance and sets `DATABASE_URL`; locally the dev compose exposes one.

use chrono::Utc;
use domain::dto::team::TeamRole;
use domain::entities::{instance_audit_logs, team_members, teams, users};
use migration::MigratorTrait;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use services::audit_service::AuditContext;
use services::team_service::{self, TeamError, TransferBy};
use uuid::Uuid;

#[path = "common/scratch.rs"]
mod scratch;

use scratch::in_scratch_database;

/// What the CLI records without `--actor`.
static CLI: AuditContext = AuditContext {
    actor_id: None,
    ip_address: None,
    user_agent: None,
};

/// The CLI's mode: an instance administrator, no reason given.
fn by_admin() -> TransferBy<'static> {
    TransferBy::InstanceAdmin {
        ctx: &CLI,
        reason: None,
    }
}

async fn migrated(db: &DatabaseConnection) {
    migration::Migrator::up(db, None)
        .await
        .expect("migrations must apply");
}

async fn create_user(db: &DatabaseConnection, scheduled: bool) -> Uuid {
    users::ActiveModel {
        id: Set(Uuid::now_v7()),
        username: Set(format!("tr-{}", Uuid::now_v7().simple())),
        password_hash: Set("not-a-real-hash".to_owned()),
        locale: Set("en".to_owned()),
        purge_scheduled_at: Set(
            scheduled.then(|| (Utc::now() + chrono::Duration::days(30)).fixed_offset())
        ),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture user must insert")
    .id
}

/// A team with its members and their roles; no Captain unless one is listed.
async fn create_team(db: &DatabaseConnection, personal: bool, members: &[(Uuid, &str)]) -> Uuid {
    let team = teams::ActiveModel {
        id: Set(Uuid::now_v7()),
        slug: Set(format!("tr-{}", Uuid::now_v7().simple())),
        name: Set("Transfer".to_owned()),
        color: Set("ember".to_owned()),
        personal: Set(personal),
        created_by: Set(members.first().map(|(user, _)| *user)),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture team must insert");

    for (user, role) in members {
        team_members::ActiveModel {
            team_id: Set(team.id),
            user_id: Set(*user),
            role: Set((*role).to_owned()),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("the fixture membership must insert");
    }

    team.id
}

async fn role_of(db: &DatabaseConnection, team_id: Uuid, user_id: Uuid) -> Option<String> {
    team_members::Entity::find_by_id((team_id, user_id))
        .one(db)
        .await
        .expect("the lookup must succeed")
        .map(|member| member.role)
}

/// From the CLI: no seat needed, the Captain becomes an Admin and the
/// target Captain.
#[tokio::test]
async fn an_administrator_transfers_without_a_seat_in_the_team() {
    in_scratch_database(|db| async move {
        migrated(&db).await;
        let captain = create_user(&db, false).await;
        let member = create_user(&db, false).await;
        let team = create_team(&db, false, &[(captain, "captain"), (member, "viewer")]).await;

        team_service::transfer_captaincy(&db, team, member, by_admin())
            .await
            .expect("the transfer must succeed");

        assert_eq!(role_of(&db, team, member).await.as_deref(), Some("captain"));
        assert_eq!(role_of(&db, team, captain).await.as_deref(), Some("admin"));

        db.close().await.ok();
    })
    .await;
}

/// The refusals the CLI shares with the route, each leaving the roles as
/// they were.
#[tokio::test]
async fn the_administrator_path_refuses_what_the_route_refuses() {
    in_scratch_database(|db| async move {
        migrated(&db).await;
        let captain = create_user(&db, false).await;
        let member = create_user(&db, false).await;
        let leaving = create_user(&db, true).await;
        let outsider = create_user(&db, false).await;
        let team = create_team(
            &db,
            false,
            &[(captain, "captain"), (member, "member"), (leaving, "admin")],
        )
        .await;
        let personal = create_team(&db, true, &[(captain, "captain"), (member, "member")]).await;

        let outcome = team_service::transfer_captaincy(&db, personal, member, by_admin()).await;
        assert!(
            matches!(outcome, Err(TeamError::PersonalTeamNotTransferable)),
            "a personal team, got {outcome:?}"
        );
        let outcome = team_service::transfer_captaincy(&db, team, outsider, by_admin()).await;
        assert!(
            matches!(outcome, Err(TeamError::MemberNotFound)),
            "a target outside the team, got {outcome:?}"
        );
        let outcome = team_service::transfer_captaincy(&db, team, leaving, by_admin()).await;
        assert!(
            matches!(outcome, Err(TeamError::TargetPendingDeletion)),
            "a target scheduled for deletion, got {outcome:?}"
        );
        let outcome = team_service::transfer_captaincy(&db, team, captain, by_admin()).await;
        assert!(
            matches!(outcome, Err(TeamError::TargetAlreadyCaptain)),
            "a target already Captain, got {outcome:?}"
        );

        for team_id in [team, personal] {
            assert_eq!(
                role_of(&db, team_id, captain).await.as_deref(),
                Some("captain")
            );
            assert_eq!(
                role_of(&db, team_id, member).await.as_deref(),
                Some("member")
            );
        }
        assert_eq!(role_of(&db, team, leaving).await.as_deref(), Some("admin"));

        db.close().await.ok();
    })
    .await;
}

/// A member who asks has to be the Captain once the team is locked: after
/// handing the role over, the former Captain cannot hand it over again.
#[tokio::test]
async fn a_former_captain_cannot_transfer_again() {
    in_scratch_database(|db| async move {
        migrated(&db).await;
        let captain = create_user(&db, false).await;
        let first = create_user(&db, false).await;
        let second = create_user(&db, false).await;
        let team = create_team(
            &db,
            false,
            &[(captain, "captain"), (first, "member"), (second, "member")],
        )
        .await;

        team_service::transfer_captaincy(&db, team, first, TransferBy::Captain(captain))
            .await
            .expect("the first transfer must succeed");
        let again =
            team_service::transfer_captaincy(&db, team, second, TransferBy::Captain(captain)).await;

        assert!(
            matches!(again, Err(TeamError::InsufficientRole)),
            "the former Captain must be refused, got {again:?}"
        );
        assert_eq!(role_of(&db, team, first).await.as_deref(), Some("captain"));
        assert_eq!(role_of(&db, team, second).await.as_deref(), Some("member"));

        db.close().await.ok();
    })
    .await;
}

/// A team left without a Captain gets one from the CLI: there is nobody to
/// demote, only the target to promote.
#[tokio::test]
async fn an_administrator_gives_a_captainless_team_a_captain() {
    in_scratch_database(|db| async move {
        migrated(&db).await;
        let member = create_user(&db, false).await;
        let other = create_user(&db, false).await;
        let team = create_team(&db, false, &[(member, "admin"), (other, "member")]).await;

        team_service::transfer_captaincy(&db, team, member, by_admin())
            .await
            .expect("the transfer must succeed");

        assert_eq!(role_of(&db, team, member).await.as_deref(), Some("captain"));
        assert_eq!(role_of(&db, team, other).await.as_deref(), Some("member"));

        db.close().await.ok();
    })
    .await;
}

/// Joining never makes a Captain: `add_member` refuses the role with a code
/// of its own, before the index would refuse a second one.
#[tokio::test]
async fn adding_a_member_as_captain_is_refused() {
    in_scratch_database(|db| async move {
        migrated(&db).await;
        let captain = create_user(&db, false).await;
        let joining = create_user(&db, false).await;
        let team = create_team(&db, false, &[(captain, "captain")]).await;

        let outcome = team_service::add_member(&db, team, joining, TeamRole::Captain).await;

        assert!(
            matches!(outcome, Err(TeamError::CaptainMustTransfer)),
            "joining as Captain must be refused with its own code, got {outcome:?}"
        );
        assert_eq!(role_of(&db, team, joining).await, None);
        assert_eq!(
            role_of(&db, team, captain).await.as_deref(),
            Some("captain")
        );

        db.close().await.ok();
    })
    .await;
}

async fn transfer_entries(db: &DatabaseConnection) -> Vec<instance_audit_logs::Model> {
    instance_audit_logs::Entity::find()
        .filter(instance_audit_logs::Column::Action.eq("team.captaincy_transferred"))
        .all(db)
        .await
        .expect("the audit lookup must succeed")
}

/// A transfer by an instance administrator is audited with identifiers
/// only, the reason given and the administrator as actor; the Captain's own
/// transfer writes nothing, like every other team action.
#[tokio::test]
async fn only_an_administrator_transfer_is_audited() {
    in_scratch_database(|db| async move {
        migrated(&db).await;
        let captain = create_user(&db, false).await;
        let member = create_user(&db, false).await;
        let by_captain = create_team(&db, false, &[(captain, "captain"), (member, "member")]).await;
        let by_cli = create_team(&db, false, &[(captain, "captain"), (member, "member")]).await;

        team_service::transfer_captaincy(&db, by_captain, member, TransferBy::Captain(captain))
            .await
            .expect("the Captain's transfer must succeed");
        assert!(
            transfer_entries(&db).await.is_empty(),
            "a Captain's transfer is not audited"
        );

        let operator = create_user(&db, false).await;
        let ctx = AuditContext {
            actor_id: Some(operator),
            ip_address: None,
            user_agent: None,
        };
        team_service::transfer_captaincy(
            &db,
            by_cli,
            member,
            TransferBy::InstanceAdmin {
                ctx: &ctx,
                reason: Some("captain left the event"),
            },
        )
        .await
        .expect("the administrator's transfer must succeed");

        let entries = transfer_entries(&db).await;
        assert_eq!(entries.len(), 1, "exactly the administrator's transfer");
        let entry = &entries[0];
        assert_eq!(entry.actor_id, Some(operator));
        assert_eq!(entry.target_team_id, Some(by_cli));
        assert_eq!(entry.target_user_id, Some(member));
        assert_eq!(entry.reason.as_deref(), Some("captain left the event"));
        assert_eq!(
            entry.meta,
            serde_json::json!({
                "team_id": by_cli,
                "previous_captain": captain,
                "new_captain": member,
            }),
            "identifiers only"
        );

        db.close().await.ok();
    })
    .await;
}

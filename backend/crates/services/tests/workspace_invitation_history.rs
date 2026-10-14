//! What becomes of a workspace invitation once it stops being pending: the
//! address is erased on every way out (accepted, declined, cancelled,
//! archived, expired), the row stays as history for ninety days, and a
//! pending one is never purged. Also the lock that makes an acceptance the
//! only answer an invitation gets.
//!
//! Real Postgres: the rules are `WHERE` clauses, constraints and row locks.
//! CI provisions the instance and sets `DATABASE_URL`; locally the dev
//! compose exposes one. Rows are left behind, as in the other tests on the
//! shared database.

use chrono::{DateTime, Duration, Utc};
use domain::dto::workspace::{InvitationStatus, Membership};
use domain::entities::{
    team_members, teams, user_emails, users, workspace_invitations, workspace_members, workspaces,
};
use migration::MigratorTrait;
use sea_orm::{
    ActiveModelTrait, ConnectionTrait, Database, DatabaseConnection, DbBackend, EntityTrait, Set,
    Statement, TransactionTrait,
};
use services::workspace_invitation_service::{
    self as invitations, InvitationRef, WorkspaceInvitationError,
};
use services::{token_service, workspace_service};
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

fn unique(prefix: &str) -> String {
    format!("{prefix}-{}", Uuid::now_v7().simple())
}

async fn create_user(db: &DatabaseConnection) -> Uuid {
    users::ActiveModel {
        id: Set(Uuid::now_v7()),
        username: Set(unique("wih")),
        password_hash: Set("not-a-real-hash".to_owned()),
        locale: Set("en".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture user must insert")
    .id
}

/// A user holding `email` verified.
async fn create_user_with_address(db: &DatabaseConnection, email: &str) -> Uuid {
    let user = create_user(db).await;
    user_emails::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(user),
        email: Set(email.to_owned()),
        is_primary: Set(true),
        verified_at: Set(Some(Utc::now().into())),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture address must insert");
    user
}

/// A team workspace and its Captain.
struct Fixture {
    workspace: workspaces::Model,
    captain: Uuid,
}

async fn fixture(db: &DatabaseConnection) -> Fixture {
    let captain = create_user(db).await;
    let team = teams::ActiveModel {
        id: Set(Uuid::now_v7()),
        slug: Set(unique("wih-team")),
        name: Set("History".to_owned()),
        color: Set("ember".to_owned()),
        created_by: Set(Some(captain)),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture team must insert");
    team_members::ActiveModel {
        team_id: Set(team.id),
        user_id: Set(captain),
        role: Set("captain".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture captain must insert");
    let workspace = workspaces::ActiveModel {
        id: Set(Uuid::now_v7()),
        team_id: Set(Some(team.id)),
        slug: Set(unique("wih-ws")),
        name: Set("History".to_owned()),
        color: Set("ember".to_owned()),
        r#type: Set("event".to_owned()),
        mode: Set("team".to_owned()),
        default_writeup_visibility: Set("team".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture workspace must insert");

    Fixture { workspace, captain }
}

enum Target<'a> {
    User(Uuid),
    Email(&'a str),
}

/// A pending invitation written directly, and the secret of its token.
async fn pending(
    db: &DatabaseConnection,
    fixture: &Fixture,
    target: Target<'_>,
    expires_at: DateTime<Utc>,
) -> (Uuid, String) {
    let token = token_service::generate_opaque_token();
    let (user, email) = match target {
        Target::User(user) => (Some(user), None),
        Target::Email(email) => (None, Some(email.to_owned())),
    };
    let row = workspace_invitations::ActiveModel {
        id: Set(Uuid::now_v7()),
        workspace_id: Set(fixture.workspace.id),
        invited_user_id: Set(user),
        invited_email: Set(email),
        role: Set("member".to_owned()),
        token: Set(token.hash),
        invited_by: Set(Some(fixture.captain)),
        expires_at: Set(expires_at.into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture invitation must insert");
    (row.id, token.secret)
}

async fn row(db: &DatabaseConnection, id: Uuid) -> Option<workspace_invitations::Model> {
    workspace_invitations::Entity::find_by_id(id)
        .one(db)
        .await
        .expect("the lookup must succeed")
}

fn in_a_week() -> DateTime<Utc> {
    Utc::now() + Duration::days(7)
}

/// Accepting makes the invitee a guest with the invited role and the author
/// recorded, and closes the invitation as accepted by them, address erased.
#[tokio::test]
async fn accepting_records_the_guest_and_erases_the_address() {
    let db = connect().await;
    let fixture = fixture(&db).await;
    let address = format!("{}@example.test", unique("accept"));
    let invitee = create_user_with_address(&db, &address).await;
    let (id, _) = pending(&db, &fixture, Target::Email(&address), in_a_week()).await;

    let (_, membership) = invitations::accept(&db, InvitationRef::Id(id), invitee)
        .await
        .expect("the invitee may accept");
    assert_eq!(
        membership,
        Membership::Active(domain::dto::team::TeamRole::Member)
    );

    let closed = row(&db, id).await.expect("the history row stays");
    assert_eq!(closed.status, InvitationStatus::Accepted.as_str());
    assert_eq!(closed.invited_email, None, "the address is erased");
    assert_eq!(closed.accepted_by, Some(invitee));
    assert!(closed.resolved_at.is_some());

    let guest = workspace_members::Entity::find_by_id((fixture.workspace.id, invitee))
        .one(&db)
        .await
        .expect("the lookup must succeed")
        .expect("the invitee is a guest");
    assert!(guest.is_guest);
    assert_eq!(guest.role.as_deref(), Some("member"));
    assert_eq!(guest.invited_by, Some(fixture.captain));

    // A second acceptance finds it answered.
    assert!(matches!(
        invitations::accept(&db, InvitationRef::Id(id), invitee).await,
        Err(WorkspaceInvitationError::NotPending)
    ));
}

#[tokio::test]
async fn declining_erases_the_address() {
    let db = connect().await;
    let fixture = fixture(&db).await;
    let address = format!("{}@example.test", unique("decline"));
    let invitee = create_user_with_address(&db, &address).await;
    let (id, _) = pending(&db, &fixture, Target::Email(&address), in_a_week()).await;

    invitations::decline(&db, id, invitee)
        .await
        .expect("the invitee may decline");

    let closed = row(&db, id).await.expect("the history row stays");
    assert_eq!(closed.status, InvitationStatus::Declined.as_str());
    assert_eq!(closed.invited_email, None);
    assert!(closed.resolved_at.is_some());
}

#[tokio::test]
async fn cancelling_erases_the_address() {
    let db = connect().await;
    let fixture = fixture(&db).await;
    let address = format!("{}@example.test", unique("cancel"));
    let (id, _) = pending(&db, &fixture, Target::Email(&address), in_a_week()).await;

    invitations::cancel_invitation(&db, &fixture.workspace, fixture.captain, id)
        .await
        .expect("the Captain may cancel");

    let closed = row(&db, id).await.expect("the history row stays");
    assert_eq!(closed.status, InvitationStatus::Cancelled.as_str());
    assert_eq!(closed.cancel_reason.as_deref(), Some("revoked"));
    assert_eq!(closed.invited_email, None);
}

/// Archiving cancels every pending invitation in its own transaction,
/// address erased; one already past its date is recorded as expired. Neither
/// comes back on unarchiving.
#[tokio::test]
async fn archiving_closes_every_pending_invitation_for_good() {
    let db = connect().await;
    let fixture = fixture(&db).await;
    let address = format!("{}@example.test", unique("archive"));
    let (live, _) = pending(&db, &fixture, Target::Email(&address), in_a_week()).await;
    let stale_address = format!("{}@example.test", unique("stale"));
    let (stale, _) = pending(
        &db,
        &fixture,
        Target::Email(&stale_address),
        Utc::now() - Duration::hours(1),
    )
    .await;

    workspace_service::archive_workspace(&db, fixture.workspace.id)
        .await
        .expect("the workspace archives");

    let cancelled = row(&db, live).await.expect("the history row stays");
    assert_eq!(cancelled.status, InvitationStatus::Cancelled.as_str());
    assert_eq!(
        cancelled.cancel_reason.as_deref(),
        Some("workspace_archived")
    );
    assert_eq!(cancelled.invited_email, None);
    let expired = row(&db, stale).await.expect("the history row stays");
    assert_eq!(expired.status, InvitationStatus::Expired.as_str());
    assert_eq!(expired.invited_email, None);

    workspace_service::unarchive_workspace(&db, fixture.workspace.id)
        .await
        .expect("the workspace unarchives");
    assert_eq!(
        row(&db, live).await.map(|row| row.status),
        Some(InvitationStatus::Cancelled.as_str().to_owned()),
        "unarchiving brings no invitation back"
    );
}

/// The cleanup records a pending invitation past its date as expired, on
/// the date it expired, and erases its address.
#[tokio::test]
async fn expiry_erases_the_address() {
    let db = connect().await;
    let fixture = fixture(&db).await;
    let address = format!("{}@example.test", unique("expire"));
    let expired_on = Utc::now() - Duration::hours(2);
    let (id, _) = pending(&db, &fixture, Target::Email(&address), expired_on).await;

    invitations::expire_stale(&db, Utc::now())
        .await
        .expect("the cleanup runs");

    let closed = row(&db, id).await.expect("the history row stays");
    assert_eq!(closed.status, InvitationStatus::Expired.as_str());
    assert_eq!(closed.invited_email, None);
    assert_eq!(
        closed.resolved_at.map(|at| at.timestamp()),
        Some(expired_on.timestamp()),
        "closed on the date it expired"
    );
}

/// Closed rows are kept for ninety days from when they closed, then deleted.
#[tokio::test]
async fn the_history_is_purged_after_ninety_days() {
    let db = connect().await;
    let fixture = fixture(&db).await;
    let invitee = create_user(&db).await;
    let other = create_user(&db).await;
    let (old, _) = pending(&db, &fixture, Target::User(invitee), in_a_week()).await;
    let (recent, _) = pending(&db, &fixture, Target::User(other), in_a_week()).await;

    for (id, days) in [(old, 91), (recent, 89)] {
        db.execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "UPDATE workspace_invitations SET status = 'declined', resolved_at = $2 WHERE id = $1",
            [
                id.into(),
                (Utc::now() - Duration::days(days)).fixed_offset().into(),
            ],
        ))
        .await
        .expect("the fixture closes");
    }

    invitations::purge_history(&db, Utc::now())
        .await
        .expect("the purge runs");

    assert!(row(&db, old).await.is_none(), "closed 91 days ago: purged");
    assert!(row(&db, recent).await.is_some(), "closed 89 days ago: kept");
}

/// A pending invitation is never purged, however old: the retention runs
/// from when an invitation closes, and this one has not.
#[tokio::test]
async fn a_pending_invitation_is_never_purged() {
    let db = connect().await;
    let fixture = fixture(&db).await;
    let invitee = create_user(&db).await;
    let (id, _) = pending(&db, &fixture, Target::User(invitee), in_a_week()).await;
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "UPDATE workspace_invitations SET created_at = $2 WHERE id = $1",
        [
            id.into(),
            (Utc::now() - Duration::days(400)).fixed_offset().into(),
        ],
    ))
    .await
    .expect("the fixture ages");

    invitations::expire_stale(&db, Utc::now())
        .await
        .expect("the cleanup runs");
    invitations::purge_history(&db, Utc::now())
        .await
        .expect("the purge runs");

    let kept = row(&db, id)
        .await
        .expect("a pending invitation is never purged");
    assert_eq!(kept.status, InvitationStatus::Pending.as_str());
}

/// An acceptance waits for an answer given meanwhile and then sees it: the
/// invitation is read under lock, so it is answered once. Without the lock
/// the acceptance would read it pending, then write `accepted` over the
/// decline.
#[tokio::test]
async fn an_invitation_answered_meanwhile_is_not_accepted_over_it() {
    let db = connect().await;
    let fixture = fixture(&db).await;
    let invitee = create_user(&db).await;
    let (id, _) = pending(&db, &fixture, Target::User(invitee), in_a_week()).await;

    // A concurrent answer, holding the row until it commits.
    let answer = db.begin().await.expect("the transaction opens");
    answer
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT id FROM workspace_invitations WHERE id = $1 FOR UPDATE",
            [id.into()],
        ))
        .await
        .expect("the row locks");
    answer
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "UPDATE workspace_invitations SET status = 'declined', resolved_at = now() \
             WHERE id = $1",
            [id.into()],
        ))
        .await
        .expect("the answer writes");

    let acceptance = {
        let db = db.clone();
        tokio::spawn(async move { invitations::accept(&db, InvitationRef::Id(id), invitee).await })
    };
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    answer.commit().await.expect("the answer commits");

    let outcome = acceptance
        .await
        .expect("the acceptance task must not panic");
    assert!(
        matches!(outcome, Err(WorkspaceInvitationError::NotPending)),
        "the acceptance must see the answer given meanwhile, got {outcome:?}"
    );
    assert_eq!(
        row(&db, id).await.map(|row| row.status),
        Some(InvitationStatus::Declined.as_str().to_owned())
    );
    assert!(
        workspace_members::Entity::find_by_id((fixture.workspace.id, invitee))
            .one(&db)
            .await
            .expect("the lookup must succeed")
            .is_none(),
        "and nobody joined"
    );
}

/// The constraints hold one layer down: an address on a closed row, and a
/// second pending invitation for the same invitee, are refused by name.
#[tokio::test]
async fn the_table_refuses_an_address_on_a_closed_row_and_a_second_pending_one() {
    let db = connect().await;
    let fixture = fixture(&db).await;
    let address = format!("{}@example.test", unique("constraint"));
    let (id, _) = pending(&db, &fixture, Target::Email(&address), in_a_week()).await;

    let err = db
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "UPDATE workspace_invitations SET status = 'declined', resolved_at = now() \
             WHERE id = $1",
            [id.into()],
        ))
        .await
        .expect_err("a closed row keeping its address must be refused");
    assert!(
        err.to_string()
            .contains("ck_workspace_invitations_email_while_pending"),
        "got {err}"
    );

    let token = token_service::generate_opaque_token();
    let err = workspace_invitations::ActiveModel {
        id: Set(Uuid::now_v7()),
        workspace_id: Set(fixture.workspace.id),
        invited_email: Set(Some(address.to_uppercase())),
        role: Set("viewer".to_owned()),
        token: Set(token.hash),
        expires_at: Set(in_a_week().into()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect_err("a second pending invitation for the address must be refused");
    assert!(
        err.to_string()
            .contains("idx_workspace_invitations_pending_per_email"),
        "got {err}"
    );
}

//! Integration tests for who a team invitation is for, and what answering it
//! takes.
//!
//! These need a real Postgres. What is under test is a `WHERE` clause, and
//! `MockDatabase` does not execute SQL: it replays whatever rows the test
//! hands it, filters and all. Asserting against it would prove only that the
//! test author remembered to leave a row out.
//!
//! An invitation by address names no account: it is for whoever has the
//! address verified when it is read or answered, and nothing ever accepts it
//! on their behalf, verifying the address included.
//!
//! CI provisions the instance and sets `DATABASE_URL`; locally the dev compose
//! exposes one. These run in autocommit and leave their rows behind, which is
//! fine against a test database.

use std::sync::atomic::{AtomicU32, Ordering};

use chrono::{Duration, Utc};
use domain::entities::{team_invitations, team_members, teams, user_emails, users};
use domain::types::TokenKind;
use migration::MigratorTrait;
use sea_orm::{ActiveModelTrait, Database, DatabaseConnection, EntityTrait, Set};
use services::team_service::{self, TeamError};
use services::{auth_service, auth_token_service, token_service};
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

/// A distinct suffix per call, so rows from one test never collide with
/// another's on the UNIQUE columns (`users.username`, `teams.slug`).
fn unique_suffix() -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nth = COUNTER.fetch_add(1, Ordering::Relaxed);

    format!("{}-{nth}", Uuid::now_v7().simple())
}

fn fresh_address(prefix: &str) -> String {
    format!("{prefix}-{}@example.test", unique_suffix())
}

async fn create_user(db: &DatabaseConnection) -> Uuid {
    let user = users::ActiveModel {
        id: Set(Uuid::now_v7()),
        username: Set(format!("invite-test-{}", unique_suffix())),
        password_hash: Set("not-a-real-hash".to_owned()),
        locale: Set("en".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture user must insert");

    user.id
}

/// An address of `user_id`, not verified yet.
async fn add_address(db: &DatabaseConnection, user_id: Uuid, email: &str, primary: bool) -> Uuid {
    user_emails::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(user_id),
        email: Set(email.to_owned()),
        is_primary: Set(primary),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture address must insert")
    .id
}

/// Verifies an address the way the link in the verification mail does.
async fn verify(db: &DatabaseConnection, user_id: Uuid, email_id: Uuid) {
    let secret = auth_token_service::issue(db, user_id, email_id, TokenKind::EmailVerification)
        .await
        .expect("a verification token must be issued");
    auth_service::verify_email(db, &secret)
        .await
        .expect("the address must verify");
}

async fn create_team(db: &DatabaseConnection, creator: Uuid) -> Uuid {
    let suffix = unique_suffix();

    let team = teams::ActiveModel {
        id: Set(Uuid::now_v7()),
        slug: Set(format!("invite-test-{suffix}")),
        name: Set(format!("Invite Test {suffix}")),
        color: Set("ember".to_owned()),
        created_by: Set(Some(creator)),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture team must insert");

    team.id
}

/// Writes an invitation row directly, bypassing `create_invitation`, and
/// returns its id and the secret of its token.
///
/// These tests are about the read and answer side, so the rows are built
/// here rather than through the service: going the long way round would drag
/// in the whole creation path (role checks, rate limits, mail) for scenarios
/// that only care about which rows a later query picks up.
async fn try_insert_invitation(
    db: &DatabaseConnection,
    team_id: Uuid,
    invited_user_id: Option<Uuid>,
    invited_email: Option<&str>,
    expires_at: chrono::DateTime<Utc>,
    created_by: Option<Uuid>,
) -> Result<(Uuid, String), sea_orm::DbErr> {
    let token = token_service::generate_opaque_token();
    let invitation = team_invitations::ActiveModel {
        id: Set(Uuid::now_v7()),
        team_id: Set(team_id),
        token: Set(token.hash),
        default_role: Set("member".to_owned()),
        invited_user_id: Set(invited_user_id),
        invited_email: Set(invited_email.map(str::to_owned)),
        expires_at: Set(expires_at.into()),
        max_uses: Set(Some(1)),
        created_by: Set(created_by),
        ..Default::default()
    }
    .insert(db)
    .await?;

    Ok((invitation.id, token.secret))
}

async fn insert_invitation(
    db: &DatabaseConnection,
    team_id: Uuid,
    invited_email: Option<&str>,
    created_by: Option<Uuid>,
) -> (Uuid, String) {
    try_insert_invitation(
        db,
        team_id,
        None,
        invited_email,
        Utc::now() + Duration::days(7),
        created_by,
    )
    .await
    .expect("the fixture invitation must insert")
}

async fn membership(
    db: &DatabaseConnection,
    team_id: Uuid,
    user_id: Uuid,
) -> Option<team_members::Model> {
    team_members::Entity::find_by_id((team_id, user_id))
        .one(db)
        .await
        .expect("membership query must succeed")
}

async fn invitation_exists(db: &DatabaseConnection, invitation_id: Uuid) -> bool {
    team_invitations::Entity::find_by_id(invitation_id)
        .one(db)
        .await
        .expect("invitation query must succeed")
        .is_some()
}

async fn pending_ids(db: &DatabaseConnection, user_id: Uuid) -> Vec<Uuid> {
    team_service::list_pending_invitations_for_user(db, user_id)
        .await
        .expect("the pending list must load")
        .into_iter()
        .map(|invitation| invitation.id)
        .collect()
}

/// An address invited before its owner had an account: signing up and
/// verifying the address joins no team. The invitation waits in the
/// account's pending list, and only an explicit acceptance joins, recording
/// the author.
#[tokio::test]
async fn verifying_an_address_accepts_no_invitation_addressed_to_it() {
    let db = connect().await;

    let captain = create_user(&db).await;
    let team_id = create_team(&db, captain).await;
    let address = fresh_address("newcomer");
    let (invitation_id, _) = insert_invitation(&db, team_id, Some(&address), Some(captain)).await;

    let newcomer = create_user(&db).await;
    let email_id = add_address(&db, newcomer, &address, true).await;
    verify(&db, newcomer, email_id).await;

    assert!(
        membership(&db, team_id, newcomer).await.is_none(),
        "verifying the address must not join the team"
    );
    assert!(
        invitation_exists(&db, invitation_id).await,
        "the invitation must still be pending"
    );
    assert_eq!(
        pending_ids(&db, newcomer).await,
        vec![invitation_id],
        "the invitation must be in the account's pending list"
    );

    team_service::accept_invitation_by_id(&db, invitation_id, newcomer)
        .await
        .expect("the account holding the address verified may accept");

    let member = membership(&db, team_id, newcomer)
        .await
        .expect("an explicit acceptance joins");
    assert_eq!(member.invited_by, Some(captain), "the author is recorded");
    assert!(
        !invitation_exists(&db, invitation_id).await,
        "and the invitation is used up"
    );
}

/// An address the account verifies only after the invitation was sent:
/// the invitation appears then, matched when read, not when sent.
#[tokio::test]
async fn an_address_verified_later_brings_its_invitation_into_the_pending_list() {
    let db = connect().await;

    let captain = create_user(&db).await;
    let team_id = create_team(&db, captain).await;
    let account = create_user(&db).await;
    let first = add_address(&db, account, &fresh_address("first"), true).await;
    verify(&db, account, first).await;

    let second = fresh_address("second");
    let (invitation_id, _) = insert_invitation(&db, team_id, Some(&second), Some(captain)).await;
    assert!(pending_ids(&db, account).await.is_empty());

    let second_id = add_address(&db, account, &second, false).await;
    assert!(
        pending_ids(&db, account).await.is_empty(),
        "an unverified address matches nothing"
    );

    verify(&db, account, second_id).await;
    assert_eq!(pending_ids(&db, account).await, vec![invitation_id]);
    assert!(membership(&db, team_id, account).await.is_none());
}

/// Negative: holding the address unverified is not holding it. The account
/// neither sees the invitation nor answers it, by id or by its link, and the
/// preview says nothing about it.
#[tokio::test]
async fn an_unverified_address_neither_sees_nor_answers_the_invitation() {
    let db = connect().await;

    let captain = create_user(&db).await;
    let team_id = create_team(&db, captain).await;
    let address = fresh_address("unverified");
    let (invitation_id, secret) =
        insert_invitation(&db, team_id, Some(&address), Some(captain)).await;

    let account = create_user(&db).await;
    add_address(&db, account, &address, true).await;

    assert!(pending_ids(&db, account).await.is_empty());
    assert!(matches!(
        team_service::accept_invitation_by_id(&db, invitation_id, account).await,
        Err(TeamError::InvitationEmailMismatch)
    ));
    assert!(matches!(
        team_service::accept_invitation_by_token(&db, &secret, account).await,
        Err(TeamError::InvitationEmailMismatch)
    ));
    assert!(matches!(
        team_service::preview_invitation(&db, &secret, account).await,
        Err(TeamError::InvitationNotFound)
    ));
    assert!(membership(&db, team_id, account).await.is_none());
    assert!(invitation_exists(&db, invitation_id).await);
}

/// The preview shows the invitee what the link offers, and gives anyone
/// else the answer an unknown token gets.
#[tokio::test]
async fn the_preview_is_for_the_invitee_alone() {
    let db = connect().await;

    let captain = create_user(&db).await;
    let team_id = create_team(&db, captain).await;
    let address = fresh_address("preview");
    let (_, secret) = insert_invitation(&db, team_id, Some(&address), Some(captain)).await;

    let invitee = create_user(&db).await;
    let email_id = add_address(&db, invitee, &address, true).await;
    verify(&db, invitee, email_id).await;
    let stranger = create_user(&db).await;

    let preview = team_service::preview_invitation(&db, &secret, invitee)
        .await
        .expect("the invitee sees the preview");
    let author = users::Entity::find_by_id(captain)
        .one(&db)
        .await
        .expect("the lookup must succeed")
        .expect("the author exists");
    assert_eq!(
        preview.invited_by_username.as_deref(),
        Some(author.username.as_str())
    );
    assert!(preview.team_name.starts_with("Invite Test"));

    for (user, token) in [(stranger, secret.as_str()), (invitee, "no-such-token")] {
        assert!(matches!(
            team_service::preview_invitation(&db, token, user).await,
            Err(TeamError::InvitationNotFound)
        ));
    }
}

/// An expired invitation is dead, not merely hidden from the listings.
#[tokio::test]
async fn an_expired_invitation_is_neither_listed_nor_accepted() {
    let db = connect().await;

    let captain = create_user(&db).await;
    let team_id = create_team(&db, captain).await;
    let invitee = create_user(&db).await;
    let (invitation_id, _) = try_insert_invitation(
        &db,
        team_id,
        Some(invitee),
        None,
        Utc::now() - Duration::hours(1),
        Some(captain),
    )
    .await
    .expect("the fixture invitation must insert");

    assert!(pending_ids(&db, invitee).await.is_empty());
    assert!(matches!(
        team_service::accept_invitation_by_id(&db, invitation_id, invitee).await,
        Err(TeamError::InvitationExpired)
    ));
    assert!(membership(&db, team_id, invitee).await.is_none());
}

/// A generic link still joins whoever holds it, and records its author.
#[tokio::test]
async fn a_generic_link_joins_its_holder_and_records_the_author() {
    let db = connect().await;

    let captain = create_user(&db).await;
    let team_id = create_team(&db, captain).await;
    let holder = create_user(&db).await;
    let (_, secret) = insert_invitation(&db, team_id, None, Some(captain)).await;

    team_service::accept_invitation_by_token(&db, &secret, holder)
        .await
        .expect("a generic link joins its holder");

    let member = membership(&db, team_id, holder)
        .await
        .expect("the holder is a member");
    assert_eq!(member.invited_by, Some(captain));
}

/// Negative: the database refuses a row that names an account *and* an
/// address.
///
/// `invitee_is_named_once` already refuses it at the edge, with a usable 422.
/// This is the same invariant one layer down, for every writer that never
/// passes through the DTO: the admin CLI, a data migration, hand SQL. The two
/// targeting columns saying different things is what let an invitation read as
/// addressed to one person while granting the team to whoever held the
/// mailbox.
#[tokio::test]
async fn an_invitation_may_not_carry_both_targets() {
    let db = connect().await;

    let captain = create_user(&db).await;
    let named_account = create_user(&db).await;
    let team_id = create_team(&db, captain).await;

    let result = try_insert_invitation(
        &db,
        team_id,
        Some(named_account),
        Some(&fresh_address("both")),
        Utc::now() + Duration::days(7),
        None,
    )
    .await;

    let err = result.expect_err("a row naming both a user and an address must be refused");

    // Matched on the constraint name rather than on `DbErr::sql_err()`, which
    // only classifies unique and foreign-key violations and returns None for a
    // CHECK. Naming the constraint in the migration is what makes this
    // assertion possible at all.
    assert!(
        err.to_string()
            .contains("ck_team_invitations_single_target"),
        "the refusal must come from the single-target constraint, got {err}"
    );
}

/// Positive: the three reachable shapes are all accepted, so the constraint
/// bites only the contradictory one.
///
/// Both columns NULL is the generic link, and it matters that it stays legal:
/// the rule is "at most one target", not "exactly one".
#[tokio::test]
async fn an_invitation_may_name_one_target_or_none() {
    let db = connect().await;

    let captain = create_user(&db).await;
    let invitee = create_user(&db).await;
    let team_id = create_team(&db, captain).await;
    let expires_at = Utc::now() + Duration::days(7);

    assert!(
        try_insert_invitation(&db, team_id, None, None, expires_at, None)
            .await
            .is_ok(),
        "a generic link names nobody and must be accepted"
    );
    assert!(
        try_insert_invitation(
            &db,
            team_id,
            None,
            Some(&fresh_address("single")),
            expires_at,
            None
        )
        .await
        .is_ok(),
        "an invitation addressed to an email must be accepted"
    );
    assert!(
        try_insert_invitation(&db, team_id, Some(invitee), None, expires_at, None)
            .await
            .is_ok(),
        "an invitation pinned to an account must be accepted"
    );
}

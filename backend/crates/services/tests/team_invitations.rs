//! Integration tests for which invitations email verification may redeem.
//!
//! These need a real Postgres. What is under test is a `WHERE` clause, and
//! `MockDatabase` does not execute SQL: it replays whatever rows the test
//! hands it, filters and all. Asserting against it would prove only that the
//! test author remembered to leave a row out.
//!
//! CI provisions the instance and sets `DATABASE_URL`; locally the dev compose
//! exposes one.
//!
//! These run in autocommit and leave their rows behind, which is fine against a
//! test database but does mean the tables grow with every run. Wrapping each
//! test in a transaction rolled back at the end would fix that; it needs
//! `verify_address` to take an outer transaction rather than open and commit
//! its own, so it is a refactor of its own rather than a line to change here.
//! `an_invitation_may_not_carry_both_targets` would not be the exception it
//! looks like: a statement that trips a constraint aborts the surrounding
//! transaction, so its failing insert belongs in a nested `begin()`, which
//! SeaORM maps to `SAVEPOINT`, rolled back so the outer transaction survives.

use std::sync::atomic::{AtomicU32, Ordering};

use chrono::{Duration, Utc};
use domain::entities::{team_invitations, team_members, teams, users};
use migration::MigratorTrait;
use sea_orm::{ActiveModelTrait, Database, DatabaseConnection, EntityTrait, Set, TransactionTrait};
use services::team_service;
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

/// Writes an invitation row directly, bypassing `create_invitation`.
///
/// These tests are about the read side, so the rows are built here rather than
/// through the service: going the long way round would drag in the whole
/// creation path (role checks, quotas, mail) for scenarios that only care
/// about which rows a later query picks up.
async fn try_insert_invitation(
    db: &DatabaseConnection,
    team_id: Uuid,
    invited_user_id: Option<Uuid>,
    invited_email: Option<&str>,
    expires_at: Option<chrono::DateTime<Utc>>,
) -> Result<Uuid, sea_orm::DbErr> {
    let invitation = team_invitations::ActiveModel {
        id: Set(Uuid::now_v7()),
        team_id: Set(team_id),
        token: Set(format!("hash-{}", unique_suffix())),
        default_role: Set("member".to_owned()),
        invited_user_id: Set(invited_user_id),
        invited_email: Set(invited_email.map(str::to_owned)),
        expires_at: Set(expires_at.map(Into::into)),
        max_uses: Set(Some(1)),
        ..Default::default()
    }
    .insert(db)
    .await?;

    Ok(invitation.id)
}

async fn insert_invitation(
    db: &DatabaseConnection,
    team_id: Uuid,
    invited_user_id: Option<Uuid>,
    invited_email: Option<&str>,
    expires_at: Option<chrono::DateTime<Utc>>,
) -> Uuid {
    try_insert_invitation(db, team_id, invited_user_id, invited_email, expires_at)
        .await
        .expect("the fixture invitation must insert")
}

async fn is_member(db: &DatabaseConnection, team_id: Uuid, user_id: Uuid) -> bool {
    team_members::Entity::find_by_id((team_id, user_id))
        .one(db)
        .await
        .expect("membership query must succeed")
        .is_some()
}

async fn invitation_exists(db: &DatabaseConnection, invitation_id: Uuid) -> bool {
    team_invitations::Entity::find_by_id(invitation_id)
        .one(db)
        .await
        .expect("invitation query must succeed")
        .is_some()
}

/// Runs the hook the way `auth_service::verify_email` does, in its own
/// transaction, and commits.
async fn verify_address(db: &DatabaseConnection, email: &str, user_id: Uuid) {
    let txn = db.begin().await.expect("transaction must open");

    team_service::accept_pending_email_invitations(&txn, email, user_id)
        .await
        .expect("accepting pending invitations must succeed");

    txn.commit().await.expect("transaction must commit");
}

/// The flow this hook exists for: an address invited before its owner had an
/// account joins them on verification.
#[tokio::test]
async fn verifying_an_address_redeems_an_invitation_addressed_to_it() {
    let db = connect().await;

    let captain = create_user(&db).await;
    let newcomer = create_user(&db).await;
    let team_id = create_team(&db, captain).await;

    let address = format!("newcomer-{}@example.test", unique_suffix());
    let invitation_id = insert_invitation(&db, team_id, None, Some(&address), None).await;

    verify_address(&db, &address, newcomer).await;

    assert!(
        is_member(&db, team_id, newcomer).await,
        "an invitation addressed to this address must join its owner"
    );
    assert!(
        !invitation_exists(&db, invitation_id).await,
        "a redeemed invitation must be consumed"
    );
}

/// An expired invitation is dead, not merely hidden from the listings.
#[tokio::test]
async fn verifying_an_address_does_not_redeem_an_expired_invitation() {
    let db = connect().await;

    let captain = create_user(&db).await;
    let latecomer = create_user(&db).await;
    let team_id = create_team(&db, captain).await;

    let address = format!("latecomer-{}@example.test", unique_suffix());
    let expired = Utc::now() - Duration::hours(1);
    insert_invitation(&db, team_id, None, Some(&address), Some(expired)).await;

    verify_address(&db, &address, latecomer).await;

    assert!(
        !is_member(&db, team_id, latecomer).await,
        "an expired invitation must not join anyone"
    );
}

/// A generic link carries no address, so email verification must leave it
/// alone rather than sweeping it up for every account that happens to verify.
#[tokio::test]
async fn verifying_an_address_leaves_generic_links_untouched() {
    let db = connect().await;

    let captain = create_user(&db).await;
    let passer_by = create_user(&db).await;
    let team_id = create_team(&db, captain).await;

    let invitation_id = insert_invitation(&db, team_id, None, None, None).await;

    verify_address(
        &db,
        &format!("passer-{}@example.test", unique_suffix()),
        passer_by,
    )
    .await;

    assert!(!is_member(&db, team_id, passer_by).await);
    assert!(
        invitation_exists(&db, invitation_id).await,
        "a generic link is not consumed by an unrelated verification"
    );
}

/// Verifying an address twice, or an account already in the team through
/// another route, must not fail the verification it is attached to.
#[tokio::test]
async fn redeeming_an_invitation_for_an_existing_member_is_not_an_error() {
    let db = connect().await;

    let captain = create_user(&db).await;
    let member = create_user(&db).await;
    let team_id = create_team(&db, captain).await;

    team_members::ActiveModel {
        team_id: Set(team_id),
        user_id: Set(member),
        role: Set("member".to_owned()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("the fixture membership must insert");

    let address = format!("member-{}@example.test", unique_suffix());
    let invitation_id = insert_invitation(&db, team_id, None, Some(&address), None).await;

    verify_address(&db, &address, member).await;

    assert!(is_member(&db, team_id, member).await);
    assert!(
        !invitation_exists(&db, invitation_id).await,
        "the now-redundant invitation is dropped rather than left pending"
    );
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
        Some(&format!("both-{}@example.test", unique_suffix())),
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

    let address = format!("single-{}@example.test", unique_suffix());

    assert!(
        try_insert_invitation(&db, team_id, None, None, None)
            .await
            .is_ok(),
        "a generic link names nobody and must be accepted"
    );
    assert!(
        try_insert_invitation(&db, team_id, None, Some(&address), None)
            .await
            .is_ok(),
        "an invitation addressed to an email must be accepted"
    );
    assert!(
        try_insert_invitation(&db, team_id, Some(invitee), None, None)
            .await
            .is_ok(),
        "an invitation pinned to an account must be accepted"
    );
}

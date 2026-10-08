//! Integration tests for refresh-token rotation under concurrency.
//!
//! These need a real Postgres. The property under test *is* the visibility of
//! a conditional `UPDATE` between two overlapping transactions, so a mock would
//! assert our assumptions about the database rather than the database itself.
//! CI provisions the instance and sets `DATABASE_URL`; locally the dev compose
//! exposes one.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use tokio::sync::Barrier;

use domain::dto::auth::SessionContext;
use domain::entities::refresh_tokens;
use migration::MigratorTrait;
use sea_orm::{ActiveModelTrait, Database, DatabaseConnection, EntityTrait, Set};
use services::session_service::{self, SessionError};
use services::token_service;
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

/// Inserts a throwaway account to hang sessions off. `refresh_tokens.user_id`
/// is a foreign key, so the rows under test need a real owner.
async fn create_user(db: &DatabaseConnection) -> Uuid {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nth = COUNTER.fetch_add(1, Ordering::Relaxed);

    let user = domain::entities::users::ActiveModel {
        id: Set(Uuid::now_v7()),
        // Unique per test and per run: `username` is a UNIQUE citext column.
        username: Set(format!("rotation-test-{}-{nth}", Uuid::now_v7().simple())),
        password_hash: Set("not-a-real-hash".to_owned()),
        locale: Set("en".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture user must insert");

    user.id
}

fn ctx() -> SessionContext {
    SessionContext {
        user_agent: Some("rotation-test".to_owned()),
        ip_address: None,
    }
}

async fn live_session_count(db: &DatabaseConnection, user_id: Uuid) -> usize {
    session_service::list_active(db, user_id)
        .await
        .expect("listing sessions must succeed")
        .len()
}

/// The race this whole change exists for: two refreshes carrying the same
/// secret, overlapping. Exactly one may win. Before the conditional claim both
/// of them succeeded, turning one stolen token into two working sessions with
/// nothing logged.
///
/// Needs a genuinely parallel runtime and a barrier: on a current-thread
/// runtime `join!` simply interleaves at await points, and the two rotations
/// end up serialized, which makes the test pass against the broken code too.
/// Repeated because even with a barrier the overlap is not guaranteed on any
/// single attempt. One clean interleaving out of `ATTEMPTS` is enough to fail
/// a regression, and a correct implementation passes all of them.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_rotation_lets_exactly_one_caller_win() {
    const ATTEMPTS: usize = 25;

    let db = connect().await;

    for attempt in 0..ATTEMPTS {
        let user_id = create_user(&db).await;
        let issued = session_service::issue(&db, user_id, &ctx())
            .await
            .expect("issuing the initial session must succeed");

        let gate = Arc::new(Barrier::new(2));
        let mut racers = Vec::with_capacity(2);

        for _ in 0..2 {
            let db = db.clone();
            let secret = issued.secret.clone();
            let gate = Arc::clone(&gate);

            racers.push(tokio::spawn(async move {
                // Both tasks arrive here, then start their rotation together.
                gate.wait().await;
                session_service::rotate(&db, &secret, &ctx()).await
            }));
        }

        let mut winners = 0;
        let mut losers = 0;

        for racer in racers {
            match racer.await.expect("the rotation task must not panic") {
                Ok(_) => winners += 1,
                Err(SessionError::Reused) => losers += 1,
                Err(other) => panic!("unexpected rotation failure: {other:?}"),
            }
        }

        assert_eq!(
            winners, 1,
            "attempt {attempt}: exactly one concurrent rotation may succeed, \
             got {winners} winners and {losers} refusals"
        );
    }
}

/// Whichever caller won, the presented token ends up retired, so the secret
/// can never be spent a third time.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_rotation_retires_the_presented_token() {
    let db = connect().await;
    let user_id = create_user(&db).await;

    let issued = session_service::issue(&db, user_id, &ctx())
        .await
        .expect("issuing the initial session must succeed");

    let gate = Arc::new(Barrier::new(2));
    let mut racers = Vec::with_capacity(2);

    for _ in 0..2 {
        let db = db.clone();
        let secret = issued.secret.clone();
        let gate = Arc::clone(&gate);

        racers.push(tokio::spawn(async move {
            gate.wait().await;
            session_service::rotate(&db, &secret, &ctx()).await
        }));
    }

    for racer in racers {
        let _ = racer.await.expect("the rotation task must not panic");
    }

    let presented = refresh_tokens::Entity::find_by_id(issued.session_id)
        .one(&db)
        .await
        .expect("query must succeed")
        .expect("the issued row must still exist");

    assert!(
        presented.revoked_at.is_some(),
        "the presented token must be revoked once the race resolves"
    );
}

/// The deterministic replay: rotate once, then present the spent secret again.
/// This is the case the conditional claim does not change, pinned here so the
/// rewrite cannot regress it.
#[tokio::test]
async fn replaying_a_spent_token_revokes_the_whole_chain() {
    let db = connect().await;
    let user_id = create_user(&db).await;

    let issued = session_service::issue(&db, user_id, &ctx())
        .await
        .expect("issuing the initial session must succeed");

    session_service::rotate(&db, &issued.secret, &ctx())
        .await
        .expect("the first rotation must succeed");

    assert_eq!(
        live_session_count(&db, user_id).await,
        1,
        "after a clean rotation the replacement is the only live session"
    );

    let replay = session_service::rotate(&db, &issued.secret, &ctx()).await;

    assert!(
        matches!(replay, Err(SessionError::Reused)),
        "replaying a spent token must be detected, got {:?}",
        replay.err()
    );
    assert_eq!(
        live_session_count(&db, user_id).await,
        0,
        "reuse detection must revoke every live session of the account"
    );
}

/// A rotation that is not contended still works end to end, and leaves the
/// account with exactly one usable session rather than two.
#[tokio::test]
async fn uncontended_rotation_replaces_the_session() {
    let db = connect().await;
    let user_id = create_user(&db).await;

    let issued = session_service::issue(&db, user_id, &ctx())
        .await
        .expect("issuing the initial session must succeed");

    let rotated = session_service::rotate(&db, &issued.secret, &ctx())
        .await
        .expect("an uncontended rotation must succeed");

    assert_ne!(
        rotated.secret, issued.secret,
        "rotation must hand back a different secret"
    );
    assert_eq!(rotated.user_id, user_id);
    assert_eq!(live_session_count(&db, user_id).await, 1);
}

// ---- users.last_activity_at ---------------------------------------------

/// The two timestamps the activity stamp is about: the one it writes, and the
/// one it must leave alone.
async fn activity_and_updated_at(
    db: &DatabaseConnection,
    user_id: Uuid,
) -> (
    Option<sea_orm::prelude::DateTimeWithTimeZone>,
    sea_orm::prelude::DateTimeWithTimeZone,
) {
    let user = domain::entities::users::Entity::find_by_id(user_id)
        .one(db)
        .await
        .expect("the lookup must succeed")
        .expect("the user must exist");

    (user.last_activity_at, user.updated_at)
}

/// A sign-in issues the first token, and that is the first activity.
#[tokio::test]
async fn issuing_a_session_records_last_activity() {
    let db = connect().await;
    let user_id = create_user(&db).await;
    let (never, created) = activity_and_updated_at(&db, user_id).await;
    assert_eq!(never, None, "a fresh account has no activity yet");

    session_service::issue(&db, user_id, &ctx())
        .await
        .expect("issuing must succeed");

    let (activity, updated_at) = activity_and_updated_at(&db, user_id).await;
    assert!(activity.is_some(), "the sign-in must record activity");
    assert_eq!(updated_at, created, "updated_at must not move");
}

/// Each successful refresh moves the activity forward, and still leaves
/// updated_at where it was.
#[tokio::test]
async fn a_successful_refresh_moves_last_activity_forward() {
    let db = connect().await;
    let user_id = create_user(&db).await;
    let issued = session_service::issue(&db, user_id, &ctx())
        .await
        .expect("issuing must succeed");
    let (signed_in, updated_before) = activity_and_updated_at(&db, user_id).await;

    session_service::rotate(&db, &issued.secret, &ctx())
        .await
        .expect("the rotation must succeed");

    let (refreshed, updated_after) = activity_and_updated_at(&db, user_id).await;
    assert!(
        refreshed > signed_in,
        "the refresh must move last_activity_at: {signed_in:?} then {refreshed:?}"
    );
    assert_eq!(updated_after, updated_before, "updated_at must not move");
}

/// A refused refresh is not activity: an unknown, an expired and a replayed
/// token all leave the stamp where the last success put it.
#[tokio::test]
async fn a_refused_refresh_leaves_last_activity_alone() {
    let db = connect().await;
    let user_id = create_user(&db).await;
    let issued = session_service::issue(&db, user_id, &ctx())
        .await
        .expect("issuing must succeed");
    session_service::rotate(&db, &issued.secret, &ctx())
        .await
        .expect("the first rotation must succeed");
    let before = activity_and_updated_at(&db, user_id).await;

    // Expired: a live row whose expiry has passed.
    let expired = token_service::generate_opaque_token();
    refresh_tokens::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(user_id),
        token_hash: Set(expired.hash),
        expires_at: Set((chrono::Utc::now() - chrono::Duration::minutes(1)).into()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("the expired fixture must insert");

    let refusals = [
        ("unknown", "not-a-token-anyone-was-given".to_owned()),
        ("expired", expired.secret),
        // Replayed last: it revokes the whole chain.
        ("replayed", issued.secret),
    ];
    for (kind, secret) in refusals {
        assert!(
            session_service::rotate(&db, &secret, &ctx()).await.is_err(),
            "an {kind} token must be refused"
        );
        assert_eq!(
            activity_and_updated_at(&db, user_id).await,
            before,
            "an {kind} token must not touch last_activity_at or updated_at"
        );
    }
}

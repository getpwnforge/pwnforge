//! The suspension cache against a request that read the account just before
//! an administrator blocked it: the change writes the blocked state into the
//! entry, and the request's late fill, which only takes an empty entry, can
//! no longer put the active state back.
//!
//! A real Postgres and a real Redis: what is under test is `SET NX` against
//! a write that came first. Every test works on an account of its own. CI
//! provides both; locally `make test-backend` points at the dev compose.

use domain::entities::users;
use migration::MigratorTrait;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use sea_orm::{ActiveModelTrait, Database, DatabaseConnection, Set};
use services::admin_service::{self, DeletionKind, DeletionRequest};
use services::audit_service::AuditContext;
use services::auth_service::AuthError;
use services::storage_service::{ObjectStore, S3Store};
use services::suspension_cache::{self, SuspensionState, suspension_cache_key};
use uuid::Uuid;

#[path = "common/config.rs"]
mod config;
#[path = "common/storage.rs"]
mod storage;

use config::config;

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

async fn connect_redis() -> ConnectionManager {
    let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_owned());
    let client = redis::Client::open(url).expect("REDIS_URL must be a valid Redis URL");
    ConnectionManager::new(client)
        .await
        .expect("a Redis instance must be reachable")
}

async fn create_user(db: &DatabaseConnection) -> Uuid {
    let id = Uuid::now_v7();
    users::ActiveModel {
        id: Set(id),
        username: Set(format!("s{}", &id.simple().to_string()[1..])),
        password_hash: Set("not-a-real-hash".to_owned()),
        locale: Set("en".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture user must insert");
    id
}

fn ctx() -> AuditContext {
    AuditContext {
        actor_id: None,
        ip_address: None,
        user_agent: None,
    }
}

fn active() -> SuspensionState {
    SuspensionState {
        suspended_at: None,
        suspended_until: None,
        suspended_reason: None,
        purge_scheduled_at: None,
    }
}

/// What the entry holds, if anything.
async fn cached(redis: &mut ConnectionManager, user: Uuid) -> Option<SuspensionState> {
    let raw: Option<String> = redis
        .get(suspension_cache_key(user))
        .await
        .expect("the read must succeed");
    raw.map(|raw| serde_json::from_str(&raw).expect("the entry must parse"))
}

/// The race, in order: the request reads the account active, the change
/// commits and writes its state, the request fills. The request's fill is
/// played by hand, with the state it read.
async fn assert_blocked_after_a_late_fill(
    redis: &mut ConnectionManager,
    user: Uuid,
    blocked: fn(&Option<AuthError>) -> bool,
) {
    let written = cached(redis, user).await;
    assert!(
        written
            .as_ref()
            .is_some_and(|state| blocked(&state.as_error())),
        "the change writes the blocked state into the entry, got {written:?}"
    );

    suspension_cache::fill(redis, user, &active()).await;

    let after = cached(redis, user).await;
    assert!(
        after
            .as_ref()
            .is_some_and(|state| blocked(&state.as_error())),
        "a late fill must not put the active state back, got {after:?}"
    );
}

/// A fill takes an empty entry only.
#[tokio::test]
async fn a_fill_never_overwrites_an_entry() {
    let mut redis = connect_redis().await;
    let user = Uuid::now_v7();
    let suspended = SuspensionState {
        suspended_at: Some(chrono::Utc::now().into()),
        ..active()
    };

    suspension_cache::fill(&mut redis, user, &suspended).await;
    suspension_cache::fill(&mut redis, user, &active()).await;

    let entry = cached(&mut redis, user).await;
    assert!(
        entry
            .as_ref()
            .is_some_and(|state| state.suspended_at.is_some()),
        "the first fill stays, got {entry:?}"
    );
}

#[tokio::test]
async fn a_suspension_is_not_undone_by_a_late_fill() {
    let db = connect().await;
    let mut redis = connect_redis().await;
    let user = create_user(&db).await;
    suspension_cache::fill(&mut redis, user, &active()).await;

    admin_service::suspend_user(&db, &mut redis, &config(), &ctx(), user, None, "test")
        .await
        .expect("the suspension must succeed");

    assert_blocked_after_a_late_fill(&mut redis, user, |error| {
        matches!(error, Some(AuthError::AccountSuspended { .. }))
    })
    .await;
}

async fn delete(
    db: &DatabaseConnection,
    redis: &mut ConnectionManager,
    user: Uuid,
    kind: DeletionKind,
) {
    admin_service::delete_user(
        db,
        redis,
        &ObjectStore::S3(S3Store::new(&storage::storage_config())),
        &config(),
        &ctx(),
        user,
        DeletionRequest {
            kind,
            reason: Some("suspension cache test"),
        },
    )
    .await
    .expect("the deletion must succeed");
}

#[tokio::test]
async fn a_deletion_request_is_not_undone_by_a_late_fill() {
    let db = connect().await;
    let mut redis = connect_redis().await;
    let user = create_user(&db).await;
    suspension_cache::fill(&mut redis, user, &active()).await;

    delete(&db, &mut redis, user, DeletionKind::Standard).await;

    assert_blocked_after_a_late_fill(&mut redis, user, |error| {
        matches!(error, Some(AuthError::PendingDeletion))
    })
    .await;
}

/// The account has no row left: the entry says pending deletion.
#[tokio::test]
async fn an_immediate_deletion_is_not_undone_by_a_late_fill() {
    let db = connect().await;
    let mut redis = connect_redis().await;
    let user = create_user(&db).await;
    suspension_cache::fill(&mut redis, user, &active()).await;

    delete(&db, &mut redis, user, DeletionKind::SecurityThreat).await;

    assert_blocked_after_a_late_fill(&mut redis, user, |error| {
        matches!(error, Some(AuthError::PendingDeletion))
    })
    .await;
}

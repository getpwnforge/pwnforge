//! Taking administrators out of the count, two at a time: a demotion, a
//! deletion request and an immediate deletion all count under one lock, so
//! the second of two removals in flight sees the first and refuses to leave
//! the instance with no administrator.
//!
//! Each test works in a throwaway database: the count spans every account,
//! and the shared database holds the administrators of other tests. Each
//! one stops a removal right after its count, on a row the test holds, then
//! starts a second: without the lock, both would count two administrators.
//! CI provisions the instance and sets `DATABASE_URL`; locally the dev
//! compose exposes one.

use domain::entities::users;
use migration::MigratorTrait;
use redis::aio::ConnectionManager;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait,
    PaginatorTrait, QueryFilter, Set, Statement, TransactionTrait,
};
use services::admin_service::{self, AdminError, DeletionKind, DeletionRequest};
use services::audit_service::AuditContext;
use services::storage_service::{ObjectStore, S3Store};
use uuid::Uuid;

#[path = "common/config.rs"]
mod config;
#[path = "common/locks.rs"]
mod locks;
#[path = "common/scratch.rs"]
mod scratch;
#[path = "common/storage.rs"]
mod storage;

use config::config;
use scratch::in_scratch_database;

async fn create_admin(db: &DatabaseConnection) -> Uuid {
    users::ActiveModel {
        id: Set(Uuid::now_v7()),
        username: Set(format!("adm-{}", Uuid::now_v7().simple())),
        password_hash: Set("not-a-real-hash".to_owned()),
        locale: Set("en".to_owned()),
        is_instance_admin: Set(true),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture admin must insert")
    .id
}

/// Administrators still counted: not scheduled for deletion.
async fn counted_admins(db: &DatabaseConnection) -> u64 {
    users::Entity::find()
        .filter(users::Column::IsInstanceAdmin.eq(true))
        .filter(users::Column::PurgeScheduledAt.is_null())
        .count(db)
        .await
        .expect("the count must succeed")
}

fn ctx() -> AuditContext {
    AuditContext {
        actor_id: None,
        ip_address: None,
        user_agent: None,
    }
}

async fn demote(db: DatabaseConnection, user: Uuid) -> Result<(), AdminError> {
    admin_service::demote_admin(&db, &config(), &ctx(), user, None).await
}

/// `admin_service::delete_user`, the way the admin CLI calls it.
async fn delete(db: DatabaseConnection, user: Uuid, kind: DeletionKind) -> Result<(), AdminError> {
    let client = redis::Client::open(
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_owned()),
    )
    .expect("REDIS_URL must be a valid Redis URL");
    let mut redis = ConnectionManager::new(client)
        .await
        .expect("a Redis instance must be reachable");

    admin_service::delete_user(
        &db,
        &mut redis,
        &ObjectStore::S3(S3Store::new(&storage::storage_config())),
        &config(),
        &ctx(),
        user,
        DeletionRequest {
            kind,
            reason: Some("admin quorum test"),
        },
    )
    .await
}

/// How many statements of this database wait on a lock, whoever holds it.
/// Meant for a throwaway database, where every connection is the test's.
async fn waiting_in_database(db: &DatabaseConnection) -> i64 {
    db.query_one_raw(Statement::from_string(
        DbBackend::Postgres,
        "SELECT count(*) AS n FROM pg_stat_activity \
         WHERE datname = current_database() AND wait_event_type = 'Lock'",
    ))
    .await
    .expect("the activity query must succeed")
    .and_then(|row| row.try_get::<i64>("", "n").ok())
    .unwrap_or(0)
}

/// Two administrators, `first` and `second`. Holds the row of `first`, so
/// that `remove_first` stops on it once it has counted; then runs a demotion
/// of `second` to its end or to a lock, releases the row, and returns both
/// outcomes.
async fn race<F, Fut>(db: &DatabaseConnection, remove_first: F) -> [Result<(), AdminError>; 2]
where
    F: FnOnce(DatabaseConnection, Uuid) -> Fut,
    Fut: Future<Output = Result<(), AdminError>> + Send + 'static,
{
    let first = create_admin(db).await;
    let second = create_admin(db).await;

    let holder = db.begin().await.expect("the transaction opens");
    holder
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT id FROM users WHERE id = $1 FOR UPDATE",
            [first.into()],
        ))
        .await
        .expect("the row locks");
    let holder_pid = locks::backend_pid(&holder).await;

    let first_removal = tokio::spawn(remove_first(db.clone(), first));
    locks::wait_for_blocked(db, holder_pid, 1).await;

    let second_removal = tokio::spawn(demote(db.clone(), second));
    for _ in 0..200 {
        if second_removal.is_finished() || waiting_in_database(db).await >= 2 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    holder.commit().await.expect("the holder commits");

    [
        first_removal
            .await
            .expect("the first removal must not panic"),
        second_removal
            .await
            .expect("the second removal must not panic"),
    ]
}

fn assert_one_refused(outcomes: &[Result<(), AdminError>; 2], admins: u64) {
    assert!(
        outcomes[0].is_ok() && matches!(outcomes[1], Err(AdminError::LastAdmin)),
        "the first removal goes through and the second is refused, got {outcomes:?}"
    );
    assert_eq!(admins, 1, "one administrator is left");
}

/// A demotion in flight, then another.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_demotions_at_once_leave_one_administrator() {
    in_scratch_database(|db| async move {
        migration::Migrator::up(&db, None)
            .await
            .expect("migrations must apply");

        let outcomes = race(&db, demote).await;
        assert_one_refused(&outcomes, counted_admins(&db).await);

        db.close().await.ok();
    })
    .await;
}

/// A deletion request in flight, then a demotion.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_deletion_request_and_a_demotion_at_once_leave_one_administrator() {
    in_scratch_database(|db| async move {
        migration::Migrator::up(&db, None)
            .await
            .expect("migrations must apply");

        let outcomes = race(&db, |db, user| delete(db, user, DeletionKind::Standard)).await;
        assert_one_refused(&outcomes, counted_admins(&db).await);

        db.close().await.ok();
    })
    .await;
}

/// An immediate deletion in flight, then a demotion.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_immediate_deletion_and_a_demotion_at_once_leave_one_administrator() {
    in_scratch_database(|db| async move {
        migration::Migrator::up(&db, None)
            .await
            .expect("migrations must apply");

        let outcomes = race(&db, |db, user| {
            delete(db, user, DeletionKind::SecurityThreat)
        })
        .await;
        assert_one_refused(&outcomes, counted_admins(&db).await);

        db.close().await.ok();
    })
    .await;
}

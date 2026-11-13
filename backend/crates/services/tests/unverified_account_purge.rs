//! The cleanup task's purge of accounts that never verified an address: it
//! deletes one created more than seven days ago that has no address ever
//! verified, is no administrator and is not the setup's account, and keeps
//! every account that misses one of these, each test on one of them. An
//! address verified between the selection and the deletion keeps the
//! account, and the audit entry names the account by id only.
//!
//! Every test works in a throwaway database: the purge deletes every account
//! that matches, and the shared database holds the accounts of other tests,
//! and of whoever develops on it. CI provisions the instance and sets
//! `DATABASE_URL`; locally the dev compose exposes one.

use chrono::{Duration, Utc};
use domain::dto::auth::SessionContext;
use domain::entities::{instance_audit_logs, user_emails, users};
use domain::legal::{AcceptanceMethod, LegalDocument};
use migration::MigratorTrait;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait,
    QueryFilter, Set, Statement, TransactionTrait,
};
use services::admin_service::{self, PurgeOutcome};
use services::background::Shutdown;
use services::legal_service;
use services::storage_service::{ObjectStore, S3Store};
use uuid::Uuid;

#[path = "common/locks.rs"]
mod locks;
#[path = "common/scratch.rs"]
mod scratch;
#[path = "common/storage.rs"]
mod storage;

use scratch::in_scratch_database;

async fn migrated(db: &DatabaseConnection) {
    migration::Migrator::up(db, None)
        .await
        .expect("migrations must apply");
}

/// One fixture account.
struct Account<'a> {
    age_days: i64,
    /// Its addresses, primary first: whether each one is verified.
    addresses: &'a [bool],
    admin: bool,
    /// Accepted the documents as the setup records them, rather than as
    /// registration does.
    operator: bool,
}

/// An account that registration would create and never verified, eight
/// days old: what the purge deletes.
const NEVER_VERIFIED: Account<'static> = Account {
    age_days: 8,
    addresses: &[false],
    admin: false,
    operator: false,
};

async fn create(db: &DatabaseConnection, account: Account<'_>) -> Uuid {
    let id = Uuid::now_v7();
    users::ActiveModel {
        id: Set(id),
        username: Set(format!("p{}", &id.simple().to_string()[1..])),
        password_hash: Set("not-a-real-hash".to_owned()),
        locale: Set("en".to_owned()),
        is_instance_admin: Set(account.admin),
        created_at: Set((Utc::now() - Duration::days(account.age_days)).into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture user must insert");

    for (nth, verified) in account.addresses.iter().enumerate() {
        user_emails::ActiveModel {
            id: Set(Uuid::now_v7()),
            user_id: Set(id),
            email: Set(format!("{nth}-{}@example.test", id.simple())),
            is_primary: Set(nth == 0),
            verified_at: Set(verified.then(|| Utc::now().into())),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("the fixture address must insert");
    }

    let method = if account.operator {
        AcceptanceMethod::Operator
    } else {
        AcceptanceMethod::Explicit
    };
    let ctx = SessionContext {
        user_agent: None,
        ip_address: None,
    };
    for document in LegalDocument::ALL {
        legal_service::record(db, id, document, document.current_version(), method, &ctx)
            .await
            .expect("the fixture acceptance must insert");
    }

    id
}

async fn purge(db: &DatabaseConnection) -> PurgeOutcome {
    admin_service::purge_unverified_accounts(
        db,
        &ObjectStore::S3(S3Store::new(&storage::storage_config())),
        &Shutdown::never(),
    )
    .await
    .expect("the purge pass must run")
}

async fn exists(db: &DatabaseConnection, id: Uuid) -> bool {
    users::Entity::find_by_id(id)
        .one(db)
        .await
        .expect("the lookup must succeed")
        .is_some()
}

/// Runs a pass and checks that it kept `id` and deleted nothing.
async fn assert_kept(db: &DatabaseConnection, id: Uuid) {
    let outcome = purge(db).await;
    assert_eq!(outcome, PurgeOutcome::default(), "nothing is purged");
    assert!(exists(db, id).await, "the account is kept");
}

/// The account is deleted, and its audit entry holds identifiers only: the
/// account's id and the kind of deletion, no actor, no reason, no address
/// or user agent, nothing of what the account held.
#[tokio::test]
async fn an_account_that_never_verified_is_purged_after_seven_days() {
    in_scratch_database(|db| async move {
        migrated(&db).await;
        let purged = create(&db, NEVER_VERIFIED).await;
        let username = users::Entity::find_by_id(purged)
            .one(&db)
            .await
            .expect("the lookup must succeed")
            .expect("the account exists")
            .username;

        let outcome = purge(&db).await;

        assert_eq!(
            outcome,
            PurgeOutcome {
                purged: 1,
                failed: 0
            }
        );
        assert!(!exists(&db, purged).await, "the account is deleted");
        let entries = instance_audit_logs::Entity::find()
            .filter(instance_audit_logs::Column::Action.eq("user.unverified_purged"))
            .all(&db)
            .await
            .expect("the lookup must succeed");
        assert_eq!(entries.len(), 1, "one entry for the account");
        let entry = &entries[0];
        assert_eq!(
            entry.meta,
            serde_json::json!({ "user_id": purged, "kind": "unverified" })
        );
        assert_eq!(entry.actor_id, None);
        assert_eq!(entry.reason, None);
        assert_eq!(entry.ip_address, None);
        assert_eq!(entry.user_agent, None);
        let text = serde_json::to_string(&entry.meta).expect("the meta serializes");
        assert!(!text.contains(&username) && !text.contains("example.test"));

        db.close().await.ok();
    })
    .await;
}

#[tokio::test]
async fn an_account_younger_than_seven_days_is_kept() {
    in_scratch_database(|db| async move {
        migrated(&db).await;
        let young = create(
            &db,
            Account {
                age_days: 6,
                ..NEVER_VERIFIED
            },
        )
        .await;

        assert_kept(&db, young).await;

        db.close().await.ok();
    })
    .await;
}

/// One verified address is enough, whichever it is: a verified address with
/// a newer one not verified yet, and an unverified primary next to a
/// verified one.
#[tokio::test]
async fn an_account_with_any_verified_address_is_kept() {
    in_scratch_database(|db| async move {
        migrated(&db).await;
        let pending_change = create(
            &db,
            Account {
                addresses: &[true, false],
                ..NEVER_VERIFIED
            },
        )
        .await;
        let verified_secondary = create(
            &db,
            Account {
                addresses: &[false, true],
                ..NEVER_VERIFIED
            },
        )
        .await;

        assert_kept(&db, pending_change).await;
        assert!(exists(&db, verified_secondary).await, "the account is kept");

        db.close().await.ok();
    })
    .await;
}

/// Another administrator stays, so that the last-admin guard is not what
/// keeps this one.
#[tokio::test]
async fn an_administrator_is_kept() {
    in_scratch_database(|db| async move {
        migrated(&db).await;
        create(
            &db,
            Account {
                age_days: 0,
                addresses: &[true],
                admin: true,
                operator: false,
            },
        )
        .await;
        let admin = create(
            &db,
            Account {
                admin: true,
                ..NEVER_VERIFIED
            },
        )
        .await;

        assert_kept(&db, admin).await;

        db.close().await.ok();
    })
    .await;
}

/// The setup's account carries the operator's acceptance. It is also an
/// administrator with a verified address; here it is neither, so that only
/// this criterion keeps it.
#[tokio::test]
async fn the_setup_account_is_kept() {
    in_scratch_database(|db| async move {
        migrated(&db).await;
        let setup = create(
            &db,
            Account {
                operator: true,
                ..NEVER_VERIFIED
            },
        )
        .await;

        assert_kept(&db, setup).await;

        db.close().await.ok();
    })
    .await;
}

/// Selected while unverified, then verified before its deletion could take
/// the address: the verification commits while the purge waits on it, and
/// the purge, reading the criteria again, keeps the account.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_address_verified_meanwhile_keeps_the_account() {
    in_scratch_database(|db| async move {
        migrated(&db).await;
        let account = create(&db, NEVER_VERIFIED).await;

        let verification = db.begin().await.expect("the transaction opens");
        verification
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "UPDATE user_emails SET verified_at = now() WHERE user_id = $1",
                [account.into()],
            ))
            .await
            .expect("the address is verified");
        let holder = locks::backend_pid(&verification).await;

        let pass = {
            let db = db.clone();
            tokio::spawn(async move { purge(&db).await })
        };
        locks::wait_for_blocked(&db, holder, 1).await;
        verification
            .commit()
            .await
            .expect("the verification commits");

        let outcome = pass.await.expect("the purge must not panic");
        assert_eq!(outcome, PurgeOutcome::default(), "nothing is purged");
        assert!(exists(&db, account).await, "the account is kept");

        db.close().await.ok();
    })
    .await;
}

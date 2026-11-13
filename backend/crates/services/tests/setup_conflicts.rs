//! The setup's administrator against an account that holds its address or
//! its username: refused with an answer the wizard can show, whether the
//! account was there before the setup started or arrived between its checks
//! and its inserts, and nothing is created either way.
//!
//! Every test works in a throwaway database: the setup needs an instance that
//! has not run it, which the shared database is past. The race is ordered
//! through the unique index: the competing account is inserted in a
//! transaction left open, the setup's insert waits on it, and the commit
//! decides. CI provisions the instance and sets `DATABASE_URL`; locally the
//! dev compose exposes one.

use domain::dto::setup::{SetupRequest, ValidateAdminRequest};
use domain::entities::{instance_settings, user_emails, users};
use migration::MigratorTrait;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, Set, TransactionTrait,
};
use services::audit_service::AuditContext;
use services::auth_service::AuthError;
use services::instance_service::SETTINGS_ID;
use services::setup_service::{self, SetupError};
use uuid::Uuid;

#[path = "common/config.rs"]
mod config;
#[path = "common/locks.rs"]
mod locks;
#[path = "common/scratch.rs"]
mod scratch;

use config::config;
use scratch::in_scratch_database;

const PASSWORD: &str = "Correct-Horse-Battery-42!";

async fn migrated(db: &DatabaseConnection) {
    migration::Migrator::up(db, None)
        .await
        .expect("migrations must apply");
}

/// A username of its own, at most 32 characters as the API requires.
fn name() -> String {
    format!("a{}", &Uuid::now_v7().simple().to_string()[1..])
}

fn request(username: &str, email: &str) -> SetupRequest {
    SetupRequest {
        admin_username: username.to_owned(),
        admin_email: email.to_owned(),
        admin_password: PASSWORD.to_owned(),
        default_locale: "en".to_owned(),
        default_timezone: "Europe/Paris".to_owned(),
        allow_public_signup: false,
        hide_landing_page: false,
    }
}

async fn complete(db: &DatabaseConnection, request: SetupRequest) -> Result<(), SetupError> {
    let ctx = AuditContext {
        actor_id: None,
        ip_address: None,
        user_agent: None,
    };
    setup_service::complete(db, &config(), &ctx, request)
        .await
        .map(|_| ())
}

/// An account holding `username` and `email`, inserted on `conn`: on a
/// transaction, its rows are held until it ends.
async fn account<C: ConnectionTrait>(conn: &C, username: &str, email: &str) {
    let user = users::ActiveModel {
        id: Set(Uuid::now_v7()),
        username: Set(username.to_owned()),
        password_hash: Set("not-a-real-hash".to_owned()),
        locale: Set("en".to_owned()),
        ..Default::default()
    }
    .insert(conn)
    .await
    .expect("the account inserts");
    user_emails::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(user.id),
        email: Set(email.to_owned()),
        is_primary: Set(true),
        ..Default::default()
    }
    .insert(conn)
    .await
    .expect("the address inserts");
}

/// The setup has not run, and no administrator exists.
async fn assert_nothing_created(db: &DatabaseConnection) {
    let settings = instance_settings::Entity::find_by_id(SETTINGS_ID)
        .one(db)
        .await
        .expect("the settings read")
        .expect("the settings row exists");
    assert!(
        settings.setup_completed_at.is_none(),
        "the setup is still to run"
    );
    let admins = users::Entity::find()
        .filter(users::Column::IsInstanceAdmin.eq(true))
        .count(db)
        .await
        .expect("the count succeeds");
    assert_eq!(admins, 0, "no administrator was created");
}

/// Already taken when the setup starts: the dry run and the setup itself
/// both name it, whatever the case of the address.
#[tokio::test]
async fn an_address_already_taken_refuses_the_setup() {
    in_scratch_database(|db| async move {
        migrated(&db).await;
        account(&db, &name(), "operator@example.test").await;
        let username = name();

        let dry_run = setup_service::validate_admin(
            &db,
            &config(),
            &ValidateAdminRequest {
                admin_username: username.clone(),
                admin_email: "Operator@Example.test".to_owned(),
                admin_password: PASSWORD.to_owned(),
            },
        )
        .await;
        assert!(
            matches!(dry_run, Err(SetupError::AdminEmailTaken)),
            "{dry_run:?}"
        );

        let outcome = complete(&db, request(&username, "Operator@Example.test")).await;
        assert!(
            matches!(outcome, Err(SetupError::AdminEmailTaken)),
            "{outcome:?}"
        );
        assert_nothing_created(&db).await;

        db.close().await.ok();
    })
    .await;
}

/// Taken between the setup's check and its insert: the same answer.
#[tokio::test]
async fn an_address_taken_during_the_setup_refuses_it() {
    in_scratch_database(|db| async move {
        migrated(&db).await;
        let address = format!("{}@example.test", name());

        let other = db.begin().await.expect("a transaction");
        account(&other, &name(), &address).await;
        let holder = locks::backend_pid(&other).await;

        let setup = {
            let (db, address) = (db.clone(), address.clone());
            tokio::spawn(async move { complete(&db, request(&name(), &address)).await })
        };
        locks::wait_for_blocked(&db, holder, 1).await;
        other.commit().await.expect("the other account commits");

        let outcome = setup.await.expect("the setup must not panic");
        assert!(
            matches!(outcome, Err(SetupError::AdminEmailTaken)),
            "{outcome:?}"
        );
        assert_nothing_created(&db).await;

        db.close().await.ok();
    })
    .await;
}

/// The username, taken the same way: the answer the wizard already shows
/// for a taken username.
#[tokio::test]
async fn a_username_taken_during_the_setup_refuses_it() {
    in_scratch_database(|db| async move {
        migrated(&db).await;
        let username = name();

        let other = db.begin().await.expect("a transaction");
        account(&other, &username, &format!("{}@example.test", name())).await;
        let holder = locks::backend_pid(&other).await;

        let setup = {
            let (db, username) = (db.clone(), username.clone());
            let address = format!("{}@example.test", name());
            tokio::spawn(async move { complete(&db, request(&username, &address)).await })
        };
        locks::wait_for_blocked(&db, holder, 1).await;
        other.commit().await.expect("the other account commits");

        let outcome = setup.await.expect("the setup must not panic");
        assert!(
            matches!(outcome, Err(SetupError::Auth(AuthError::UsernameTaken))),
            "{outcome:?}"
        );
        assert_nothing_created(&db).await;

        db.close().await.ok();
    })
    .await;
}

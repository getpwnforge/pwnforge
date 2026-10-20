//! What sign-up refuses before it creates anything: every registration while
//! the instance is not set up yet, and an address on a domain an operator
//! blocked by hand, the disposable list's own domains included.
//!
//! Each test works in a throwaway database: the first needs an instance that
//! has not run its setup, which the shared database may be past, and neither
//! may write to the settings or the lists of the shared one. CI provisions
//! the instance and sets `DATABASE_URL`; locally the dev compose exposes one.

use std::net::{IpAddr, Ipv4Addr};

use domain::dto::auth::{RegisterRequest, SessionContext};
use domain::dto::legal::LegalAcceptanceInput;
use domain::entities::{blocked_emails_domains, instance_settings, users};
use domain::legal::{CURRENT_PRIVACY_VERSION, CURRENT_TERMS_VERSION};
use migration::MigratorTrait;
use redis::aio::ConnectionManager;
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, PaginatorTrait, Set};
use services::auth_service::{self, AuthError};
use services::background::BackgroundTasks;
use services::blocked_email_service::{self, BlockedEmailError};
use services::instance_service::SETTINGS_ID;
use uuid::Uuid;

#[path = "common/config.rs"]
mod config;
#[path = "common/scratch.rs"]
mod scratch;

use config::config;
use scratch::in_scratch_database;

async fn migrated(db: &DatabaseConnection) {
    migration::Migrator::up(db, None)
        .await
        .expect("migrations must apply");
}

fn registration(tag: &str) -> RegisterRequest {
    RegisterRequest {
        email: format!("{tag}@example.test"),
        username: tag.to_owned(),
        password: "Correct-Horse-Battery-42!".to_owned(),
        legal: LegalAcceptanceInput {
            terms_version: CURRENT_TERMS_VERSION.to_owned(),
            privacy_version: CURRENT_PRIVACY_VERSION.to_owned(),
        },
        turnstile_token: String::new(),
    }
}

async fn register(db: &DatabaseConnection, request: RegisterRequest) -> Result<(), AuthError> {
    let redis = redis::Client::open(
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_owned()),
    )
    .expect("REDIS_URL must be a valid Redis URL");
    let redis = ConnectionManager::new(redis)
        .await
        .expect("a Redis instance must be reachable");

    auth_service::register(
        db,
        &redis,
        &config(),
        &BackgroundTasks::new(),
        request,
        SessionContext {
            user_agent: None,
            ip_address: None,
        },
        IpAddr::V4(Ipv4Addr::LOCALHOST),
    )
    .await
}

/// Sign-up is closed until the setup wizard has run, although public signup
/// defaults to open: no account can take the name or the address meant for
/// the administrator. The same registration goes through once it has run.
#[tokio::test]
async fn registration_is_refused_until_the_setup_has_run() {
    in_scratch_database(|db| async move {
        migrated(&db).await;
        let settings = instance_settings::Entity::find_by_id(SETTINGS_ID)
            .one(&db)
            .await
            .expect("the lookup must succeed")
            .expect("the migration inserts the settings row");
        assert!(
            settings.setup_completed_at.is_none() && settings.allow_public_signup,
            "control: a fresh instance is not set up and has public signup on"
        );

        let refused = register(&db, registration("early-admin")).await;
        assert!(
            matches!(refused, Err(AuthError::PublicSignupDisabled)),
            "got {:?}",
            refused.err()
        );
        assert_eq!(
            users::Entity::find()
                .count(&db)
                .await
                .expect("the count must succeed"),
            0,
            "nothing is created"
        );

        instance_settings::ActiveModel {
            id: Set(SETTINGS_ID),
            setup_completed_at: Set(Some(chrono::Utc::now().into())),
            ..Default::default()
        }
        .update(&db)
        .await
        .expect("the setup must be marked as run");
        register(&db, registration("early-admin"))
            .await
            .expect("the same registration must go through once the setup has run");
        assert_eq!(
            users::Entity::find()
                .count(&db)
                .await
                .expect("the count must succeed"),
            1,
            "the account is created"
        );

        db.close().await.ok();
    })
    .await;
}

/// Blocking a domain the disposable list already holds turns it into a
/// manual block, instead of failing on the existing row: it then applies
/// with the disposable check off, as every manual block does.
#[tokio::test]
async fn a_manual_block_takes_over_a_disposable_list_entry() {
    in_scratch_database(|db| async move {
        migrated(&db).await;
        let domain = format!("dispo-{}.test", Uuid::now_v7().simple());
        blocked_email_service::sync_disposable_domains(&db, &domain)
            .await
            .expect("the disposable entry must insert");
        assert!(
            blocked_email_service::check_domain(&db, &domain, false)
                .await
                .is_ok(),
            "control: a disposable entry does not apply with the check off"
        );

        blocked_email_service::block_domain(&db, &domain, "abuse", None)
            .await
            .expect("blocking a listed domain must succeed");

        let entry = blocked_emails_domains::Entity::find_by_id(&domain)
            .one(&db)
            .await
            .expect("the lookup must succeed")
            .expect("the entry exists");
        assert_eq!(entry.source, "manual");
        assert_eq!(entry.reason.as_deref(), Some("abuse"));
        let refused = blocked_email_service::check_domain(&db, &domain, false).await;
        assert!(
            matches!(refused, Err(BlockedEmailError::DomainNotAllowed)),
            "the manual block applies with the disposable check off, got {refused:?}"
        );

        db.close().await.ok();
    })
    .await;
}

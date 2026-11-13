//! Integration tests for `updated_at`: the server writes it on every update,
//! with no trigger and no `before_save` hook in the regenerated entities, so
//! each write path has to set it itself.
//!
//! One representative path per table, run against a real Postgres: what is
//! under test is the SQL an update emits, which `MockDatabase` does not
//! execute. The avatar paths are covered from the avatar route tests, which
//! already drive them. CI provisions the instance and sets `DATABASE_URL`;
//! locally the dev compose exposes one.

use std::sync::atomic::{AtomicU32, Ordering};

use domain::dto::auth::SessionContext;
use domain::entities::{user_emails, users};
use domain::legal::{AcceptanceMethod, CURRENT_TERMS_VERSION, LegalDocument};
use domain::types::TokenKind;
use migration::MigratorTrait;
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::{ActiveModelTrait, Database, DatabaseConnection, EntityTrait, Set};
use services::{auth_service, auth_token_service, legal_service};
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

fn unique_suffix() -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nth = COUNTER.fetch_add(1, Ordering::Relaxed);

    format!("{}-{nth}", Uuid::now_v7().simple())
}

async fn create_user(db: &DatabaseConnection) -> users::Model {
    users::ActiveModel {
        id: Set(Uuid::now_v7()),
        username: Set(format!("updated-at-{}", unique_suffix())),
        password_hash: Set("not-a-real-hash".to_owned()),
        locale: Set("en".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture user must insert")
}

async fn user_updated_at(db: &DatabaseConnection, id: Uuid) -> DateTimeWithTimeZone {
    users::Entity::find_by_id(id)
        .one(db)
        .await
        .expect("the lookup must succeed")
        .expect("the user must exist")
        .updated_at
}

/// `users`, through the bulk `update_many` shape most of its writes share:
/// recording a legal acceptance refreshes the cached version column.
#[tokio::test]
async fn recording_a_legal_acceptance_moves_the_users_updated_at() {
    let db = connect().await;
    let user = create_user(&db).await;
    let before = user_updated_at(&db, user.id).await;

    let ctx = SessionContext {
        user_agent: None,
        ip_address: None,
    };
    legal_service::record(
        &db,
        user.id,
        LegalDocument::Terms,
        CURRENT_TERMS_VERSION,
        AcceptanceMethod::Explicit,
        &ctx,
    )
    .await
    .expect("the acceptance must be recorded");

    let after = user_updated_at(&db, user.id).await;
    assert!(
        after > before,
        "updated_at must move: {before} then {after}"
    );
}

/// `user_emails`, through verification, the one update its rows get.
#[tokio::test]
async fn verifying_an_address_moves_the_user_emails_updated_at() {
    let db = connect().await;
    let user = create_user(&db).await;

    let email = user_emails::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(user.id),
        email: Set(format!("updated-at-{}@example.test", unique_suffix())),
        is_primary: Set(true),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("the fixture address must insert");

    let secret = auth_token_service::issue(&db, user.id, email.id, TokenKind::EmailVerification)
        .await
        .expect("a verification token must be issued");

    auth_service::verify_email(&db, &secret)
        .await
        .expect("the address must verify");

    let verified = user_emails::Entity::find_by_id(email.id)
        .one(&db)
        .await
        .expect("the lookup must succeed")
        .expect("the address must exist");

    assert!(
        verified.verified_at.is_some(),
        "the address must be verified"
    );
    assert!(
        verified.updated_at > email.updated_at,
        "updated_at must move: {} then {}",
        email.updated_at,
        verified.updated_at
    );
}

//! The reset link against its expiry. A real Postgres: what is under test
//! is the claim of the link, one conditional `UPDATE`. CI provides it;
//! locally `make test-backend` points at the dev compose.

use chrono::Utc;
use domain::entities::{auth_tokens, user_emails, users};
use domain::types::TokenKind;
use migration::MigratorTrait;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Database, DatabaseConnection, EntityTrait, QueryFilter, Set,
};
use services::auth_token_service::{self, AuthTokenError};
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

/// An account with a verified primary address, and that address.
async fn account(db: &DatabaseConnection) -> (Uuid, String) {
    let id = Uuid::now_v7();
    let address = format!("reset-{}@example.test", id.simple());
    users::ActiveModel {
        id: Set(id),
        username: Set(format!("r{}", &id.simple().to_string()[1..])),
        password_hash: Set("original-hash".to_owned()),
        locale: Set("en".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture user must insert");
    user_emails::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(id),
        email: Set(address.clone()),
        is_primary: Set(true),
        verified_at: Set(Some(Utc::now().into())),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture address must insert");
    (id, address)
}

/// A reset link for the account, expiring at `expires_in` from now, and its
/// secret.
async fn reset_link(
    db: &DatabaseConnection,
    user_id: Uuid,
    expires_in: chrono::Duration,
) -> (Uuid, String) {
    let email = user_emails::Entity::find()
        .filter(user_emails::Column::UserId.eq(user_id))
        .one(db)
        .await
        .expect("the lookup must succeed")
        .expect("the address exists");
    let token = token_service::generate_opaque_token();
    let row = auth_tokens::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(user_id),
        email_id: Set(Some(email.id)),
        kind: Set(TokenKind::PasswordReset.as_str().to_owned()),
        token_hash: Set(token.hash),
        expires_at: Set((Utc::now() + expires_in).into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture link must insert");
    (row.id, token.secret)
}

async fn link_used(db: &DatabaseConnection, id: Uuid) -> bool {
    auth_tokens::Entity::find_by_id(id)
        .one(db)
        .await
        .expect("the lookup must succeed")
        .expect("the link exists")
        .used_at
        .is_some()
}

/// An expired link is refused and stays unspent: the claim itself requires
/// the link unexpired, not only a check made before it.
#[tokio::test]
async fn an_expired_link_is_refused_by_the_claim_itself() {
    let db = connect().await;
    let (user, _) = account(&db).await;
    let (id, secret) = reset_link(&db, user, chrono::Duration::minutes(-1)).await;

    let outcome = auth_token_service::consume(&db, &secret, TokenKind::PasswordReset).await;

    assert!(
        matches!(outcome, Err(AuthTokenError::Expired)),
        "got {outcome:?}"
    );
    assert!(!link_used(&db, id).await, "an expired link is not spent");
}

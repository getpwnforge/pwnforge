// crates/services/src/auth_token_service.rs
use crate::token_service::{self, OpaqueToken};
use chrono::{Duration, Utc};
use domain::{entities::auth_tokens, types::TokenKind};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DbErr, EntityTrait,
    ExprTrait, QueryFilter, Set, TransactionTrait, sea_query::Expr,
};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AuthTokenError {
    #[error("auth token not found")]
    NotFound,

    #[error("auth token has expired")]
    Expired,

    #[error("auth token was already used")]
    Consumed,

    #[error(transparent)]
    Db(#[from] DbErr),
}

/// Called by password_forgot, email verification send and resend.
/// Calls invalidate_pending first.
pub async fn issue(
    db: &DatabaseConnection,
    user_id: Uuid,
    email_id: Uuid,
    kind: TokenKind,
) -> Result<String, AuthTokenError> {
    let txn = db.begin().await?;

    invalidate_pending(&txn, user_id, kind).await?;

    let OpaqueToken { secret, hash } = token_service::generate_opaque_token();

    auth_tokens::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(user_id),
        email_id: Set(Some(email_id)),
        kind: Set(kind.as_str().to_string()),
        token_hash: Set(hash),
        expires_at: Set((Utc::now() + ttl_for(kind)).into()),
        ..Default::default()
    }
    .insert(&txn)
    .await?;

    txn.commit().await?;

    Ok(secret)
}

/// Called by password_reset and email_verify. Marks used_at.
///
/// The claim itself decides: one UPDATE that requires the token unused and
/// not yet expired, by the database's clock, so a token cannot be spent
/// after its expiry by a request that read it just before. The row read
/// first only names which refusal it is. Generic over the connection so a
/// caller can spend the token inside the transaction that uses it.
pub async fn consume<C: ConnectionTrait>(
    db: &C,
    secret: &str,
    kind: TokenKind,
) -> Result<auth_tokens::Model, AuthTokenError> {
    let secret_hash = token_service::hash_opaque_token(secret);

    let token = auth_tokens::Entity::find()
        .filter(auth_tokens::Column::Kind.eq(kind.as_str()))
        .filter(auth_tokens::Column::TokenHash.eq(secret_hash))
        .one(db)
        .await?
        .ok_or(AuthTokenError::NotFound)?;

    let result = auth_tokens::Entity::update_many()
        .col_expr(auth_tokens::Column::UsedAt, Expr::current_timestamp())
        .filter(auth_tokens::Column::Id.eq(token.id))
        .filter(auth_tokens::Column::UsedAt.is_null())
        .filter(Expr::col(auth_tokens::Column::ExpiresAt).gt(Expr::current_timestamp()))
        .exec(db)
        .await?;

    if result.rows_affected == 0 {
        return Err(if token.expires_at <= Utc::now() {
            AuthTokenError::Expired
        } else {
            AuthTokenError::Consumed
        });
    }

    Ok(token)
}

/// Looks up a token without consuming it. Used to recover the account behind
/// an expired or consumed verification link when requesting a replacement.
pub async fn find<C: ConnectionTrait>(
    db: &C,
    secret: &str,
    kind: TokenKind,
) -> Result<auth_tokens::Model, AuthTokenError> {
    find_any(db, secret, &[kind]).await
}

/// `find`, for a token that may be of any of `kinds`.
pub async fn find_any<C: ConnectionTrait>(
    db: &C,
    secret: &str,
    kinds: &[TokenKind],
) -> Result<auth_tokens::Model, AuthTokenError> {
    let secret_hash = token_service::hash_opaque_token(secret);

    auth_tokens::Entity::find()
        .filter(auth_tokens::Column::Kind.is_in(kinds.iter().map(TokenKind::as_str)))
        .filter(auth_tokens::Column::TokenHash.eq(secret_hash))
        .one(db)
        .await?
        .ok_or(AuthTokenError::NotFound)
}

/// Spends every unused token of `kind` for the user. Called by `issue`, so
/// only one link per kind is live, and by the forced password reset, inside
/// its own transaction, so no reset link survives it.
pub(crate) async fn invalidate_pending<C: ConnectionTrait>(
    txn: &C,
    user_id: Uuid,
    kind: TokenKind,
) -> Result<(), DbErr> {
    auth_tokens::Entity::update_many()
        .col_expr(auth_tokens::Column::UsedAt, Expr::current_timestamp())
        .filter(auth_tokens::Column::UserId.eq(user_id))
        .filter(auth_tokens::Column::Kind.eq(kind.as_str()))
        .filter(auth_tokens::Column::UsedAt.is_null())
        .exec(txn)
        .await?;

    Ok(())
}

// Private: 1h for a reset and for a resend handle, 24h for a verification.
fn ttl_for(kind: TokenKind) -> Duration {
    match kind {
        TokenKind::EmailVerification => Duration::hours(24),
        TokenKind::PasswordReset | TokenKind::EmailVerificationResend => Duration::hours(1),
    }
}

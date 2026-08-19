// crates/api/src/services/session_service.rs
use crate::services::{
    audit_service::{self, AuditContext, AuditError, AuditTarget},
    jwt_service, token_service,
};
use chrono::Utc;
use domain::{
    dto::auth::{IssuedSession, RotatedSession, SessionContext},
    entities::refresh_tokens,
    types::RevokedReason,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DbErr, EntityTrait,
    QueryFilter, Set, TransactionTrait, sea_query::Expr,
};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("refresh token not found")]
    NotFound,

    #[error("refresh token has expired")]
    Expired,

    #[error("refresh token was already used")]
    Reused,

    #[error(transparent)]
    Audit(#[from] AuditError),

    #[error(transparent)]
    Db(#[from] sea_orm::DbErr),
}

/// Called by auth_service::login and, later, the OAuth callback.
pub async fn issue(
    db: &DatabaseConnection,
    user_id: Uuid,
    ctx: SessionContext,
) -> Result<IssuedSession, SessionError> {
    let refresh_token = token_service::generate_opaque_token();

    // Store the refresh token in the database, associated with the user.
    let issued_session = refresh_tokens::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(user_id),
        token_hash: Set(refresh_token.hash),
        expires_at: Set((Utc::now()
            + chrono::Duration::seconds(jwt_service::REFRESH_TOKEN_TTL_SECS))
        .into()),
        user_agent: Set(ctx.user_agent),
        ip_address: Set(ctx.ip_address),
        last_used_at: Set(Some(Utc::now().into())),
        ..Default::default()
    }
    .insert(db)
    .await?;

    Ok(IssuedSession {
        secret: refresh_token.secret,
        session_id: issued_session.id,
    })
}

/// Called by auth_service::refresh. Calls revoke_all internally when it
/// detects a replayed token.
pub async fn rotate(
    db: &DatabaseConnection,
    presented: &str,
    ctx: SessionContext,
) -> Result<RotatedSession, SessionError> {
    let token = find_by_secret(db, presented)
        .await?
        .ok_or(SessionError::NotFound)?;

    // Reuse of a revoked token means the chain has been duplicated: either an
    // attacker or the legitimate user is replaying a copy. Revoke every live
    // token of this user so both are forced to re-authenticate.
    if token.revoked_at.is_some() {
        let was_rotated = token.revoked_reason.as_deref() == Some(RevokedReason::Rotated.as_str());

        if was_rotated {
            let sessions_revoked =
                revoke_all(db, token.user_id, RevokedReason::ReuseDetected, None).await?;
            tracing::warn!(
                user_id = %token.user_id,
                "refresh token reuse detected, all sessions revoked"
            );

            audit_service::record(
                db,
                domain::types::AuditAction::AuthRefreshReuseDetected,
                &AuditContext {
                    actor_id: None,
                    ip_address: ctx.ip_address,
                    user_agent: ctx.user_agent,
                },
                AuditTarget {
                    user_id: Some(token.user_id),
                    ..Default::default()
                },
                None,
                serde_json::json!({
                    "presented_token_id": token.id,
                    "revoked_reason": token.revoked_reason,
                    "sessions_revoked": sessions_revoked,
                }),
            )
            .await?;
            return Err(SessionError::Reused);
        }

        return Err(SessionError::NotFound);
    }

    if token.expires_at < Utc::now() {
        return Err(SessionError::Expired);
    }

    let new_refresh_token = token_service::generate_opaque_token();

    let txn = db.begin().await?;

    let new_row = refresh_tokens::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(token.user_id),
        token_hash: Set(new_refresh_token.hash),
        expires_at: Set((Utc::now()
            + chrono::Duration::seconds(jwt_service::REFRESH_TOKEN_TTL_SECS))
        .into()),
        user_agent: Set(ctx.user_agent),
        ip_address: Set(ctx.ip_address),
        last_used_at: Set(Some(Utc::now().into())),
        ..Default::default()
    }
    .insert(&txn)
    .await?;

    // 4. Record when the rotated token was last used before retiring it.
    refresh_tokens::ActiveModel {
        id: Set(token.id),
        revoked_at: Set(Some(Utc::now().into())),
        revoked_reason: Set(Some(RevokedReason::Rotated.as_str().to_owned())),
        ..Default::default()
    }
    .update(&txn)
    .await?;

    txn.commit().await?;

    Ok(RotatedSession {
        user_id: token.user_id,
        secret: new_refresh_token.secret,
        session_id: new_row.id,
    })
}

/// Called by auth_service::logout. Idempotent.
pub async fn revoke(
    db: &DatabaseConnection,
    presented: &str,
    reason: RevokedReason,
) -> Result<(), SessionError> {
    let token = find_by_secret(db, presented)
        .await?
        .ok_or(SessionError::NotFound)?;

    if token.revoked_at.is_some() {
        return Ok(());
    }

    refresh_tokens::ActiveModel {
        id: Set(token.id),
        revoked_at: Set(Some(Utc::now().into())),
        revoked_reason: Set(Some(reason.as_str().to_owned())),
        ..Default::default()
    }
    .update(db)
    .await?;

    Ok(())
}

/// Called by rotate (reuse), password_reset, password_change,
/// admin_service::suspend_user (2.13) and the CLI (2.24).
pub async fn revoke_all<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    reason: RevokedReason,
    except: Option<Uuid>,
) -> Result<u64, SessionError> {
    let update_result = refresh_tokens::Entity::update_many()
        .col_expr(refresh_tokens::Column::RevokedAt, Expr::current_timestamp())
        .col_expr(
            refresh_tokens::Column::RevokedReason,
            Expr::value(reason.as_str().to_owned()),
        )
        .filter(refresh_tokens::Column::UserId.eq(user_id))
        .filter(refresh_tokens::Column::RevokedAt.is_null())
        .filter(refresh_tokens::Column::Id.ne(except.unwrap_or_else(Uuid::nil)))
        .exec(db)
        .await?;

    Ok(update_result.rows_affected)
}

/// List all active (non-revoked, non-expired) refresh tokens for a user. Used by the CLI and the web UI.
#[expect(dead_code, reason = "consumed by the sessions list (2.13 frontend)")]
pub async fn list_active(
    db: &DatabaseConnection,
    user_id: Uuid,
) -> Result<Vec<refresh_tokens::Model>, SessionError> {
    let tokens = refresh_tokens::Entity::find()
        .filter(refresh_tokens::Column::UserId.eq(user_id))
        .filter(refresh_tokens::Column::RevokedAt.is_null())
        .filter(refresh_tokens::Column::ExpiresAt.gt(Utc::now()))
        .all(db)
        .await?;

    Ok(tokens)
}

/// Returns the id of the live session behind this secret, but only if it
/// belongs to `user_id`. Any mismatch yields None rather than an error: the
/// caller only wants to know which session to spare.
pub async fn find_session_id(
    db: &DatabaseConnection,
    presented: &str,
    user_id: Uuid,
) -> Result<Option<Uuid>, SessionError> {
    let hash = token_service::hash_opaque_token(presented);

    let id = refresh_tokens::Entity::find()
        .filter(refresh_tokens::Column::TokenHash.eq(hash))
        .filter(refresh_tokens::Column::UserId.eq(user_id))
        .filter(refresh_tokens::Column::RevokedAt.is_null())
        .one(db)
        .await?
        .map(|row| row.id);

    Ok(id)
}

async fn find_by_secret(
    db: &DatabaseConnection,
    presented: &str,
) -> Result<Option<refresh_tokens::Model>, DbErr> {
    let token_hash = token_service::hash_opaque_token(presented);

    refresh_tokens::Entity::find()
        .filter(refresh_tokens::Column::TokenHash.eq(token_hash))
        .one(db)
        .await
}

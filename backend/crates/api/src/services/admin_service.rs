#![expect(dead_code, reason = "consumed by the admin CLI (2.23 to 2.26)")]
use crate::{
    config::Config,
    middleware::auth::suspension_cache_key,
    services::{
        audit_service::{self, AuditContext, AuditError},
        auth_service,
        auth_token_service::{self, AuthTokenError},
        email_service::{self, EmailError},
        password_service::{self, PasswordError},
        session_service::{self, SessionError, revoke_all},
    },
};
use domain::entities::users;
use redis::{AsyncCommands, aio::ConnectionManager};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, DbErr, EntityTrait, PaginatorTrait,
    QueryFilter, TransactionTrait, prelude::DateTimeWithTimeZone, sea_query::Expr,
};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AdminError {
    /// Target row is gone. Callers surface 404, never 403: a non-admin must
    /// not learn that the admin surface exists.
    #[error("user not found")]
    UserNotFound,
    /// Refused so an instance cannot be left with no administrator.
    #[error("last admin")]
    LastAdmin,
    /// An admin acting on their own account, where it makes no sense.
    #[error("self target")]
    SelfTarget,

    #[error(transparent)]
    Session(#[from] SessionError),

    #[error(transparent)]
    AuthToken(#[from] AuthTokenError),

    #[error(transparent)]
    Email(#[from] EmailError),

    #[error(transparent)]
    Audit(#[from] AuditError),

    #[error(transparent)]
    Db(#[from] DbErr),

    #[error(transparent)]
    Password(#[from] PasswordError),
}

/// Suspends an account, optionally until a date.
///
/// Called by the admin CLI (2.24) and, later, by POST /admin/users/:id/suspend.
pub async fn suspend_user(
    db: &DatabaseConnection,
    redis: &mut ConnectionManager,
    ctx: &AuditContext,
    user_id: Uuid,
    until: Option<DateTimeWithTimeZone>,
    reason: &str,
) -> Result<(), AdminError> {
    let user = users::Entity::find_by_id(user_id)
        .one(db)
        .await?
        .ok_or(AdminError::UserNotFound)?;

    if Some(user.id) == ctx.actor_id {
        return Err(AdminError::SelfTarget);
    }

    let txn = db.begin().await?;

    users::Entity::update_many()
        .col_expr(users::Column::SuspendedAt, Expr::current_timestamp())
        .col_expr(users::Column::SuspendedUntil, Expr::value(until))
        .col_expr(
            users::Column::SuspendedReason,
            Expr::value(reason.to_owned()),
        )
        .col_expr(users::Column::SuspendedBy, Expr::value(ctx.actor_id))
        .filter(users::Column::Id.eq(user.id))
        .exec(&txn)
        .await?;

    let revoked_count = session_service::revoke_all(
        &txn,
        user.id,
        domain::types::RevokedReason::AdminRevoked,
        None,
    )
    .await?;

    // Record the action after the session revocation, so the log entry is not lost if the transaction fails.
    audit_service::record(
        &txn,
        domain::types::AuditAction::UserSuspend,
        ctx,
        audit_service::AuditTarget {
            user_id: Some(user.id),
            team_id: None,
            workspace_id: None,
        },
        Some(reason),
        serde_json::json!({
            "previous": {
                "suspended_at": user.suspended_at,
                "suspended_until": user.suspended_until,
                "suspended_reason": user.suspended_reason,
                "suspended_by": user.suspended_by,
            },
            "applied": {
                "suspended_until": until,
            },
            "sessions_revoked": revoked_count,
        }),
    )
    .await?;

    txn.commit().await?;

    invalidate_suspension_cache(redis, user.id).await;

    Ok(())
}

/// Lifts a suspension, whether it had a deadline or not.
pub async fn unsuspend_user(
    db: &DatabaseConnection,
    redis: &mut ConnectionManager,
    ctx: &AuditContext,
    user_id: Uuid,
    reason: Option<&str>,
) -> Result<(), AdminError> {
    let user = users::Entity::find_by_id(user_id)
        .one(db)
        .await?
        .ok_or(AdminError::UserNotFound)?;

    let txn = db.begin().await?;

    users::Entity::update_many()
        .col_expr(
            users::Column::SuspendedAt,
            Expr::value(None::<DateTimeWithTimeZone>),
        )
        .col_expr(
            users::Column::SuspendedUntil,
            Expr::value(None::<DateTimeWithTimeZone>),
        )
        .col_expr(users::Column::SuspendedReason, Expr::value(None::<String>))
        .col_expr(users::Column::SuspendedBy, Expr::value(None::<Uuid>))
        .filter(users::Column::Id.eq(user.id))
        .exec(&txn)
        .await?;

    // Record the action after the session revocation, so the log entry is not lost if the transaction fails.
    audit_service::record(
        &txn,
        domain::types::AuditAction::UserUnsuspend,
        ctx,
        audit_service::AuditTarget {
            user_id: Some(user.id),
            team_id: None,
            workspace_id: None,
        },
        reason,
        serde_json::json!({
            "previous": {
                "suspended_at": user.suspended_at,
                "suspended_until": user.suspended_until,
                "suspended_reason": user.suspended_reason,
                "suspended_by": user.suspended_by,
            },
        }),
    )
    .await?;

    txn.commit().await?;

    invalidate_suspension_cache(redis, user.id).await;

    Ok(())
}

/// Grants instance administrator rights.
pub async fn promote_admin(
    db: &DatabaseConnection,
    ctx: &AuditContext,
    user_id: Uuid,
    reason: Option<&str>,
) -> Result<(), AdminError> {
    let user = users::Entity::find_by_id(user_id)
        .one(db)
        .await?
        .ok_or(AdminError::UserNotFound)?;

    if user.is_instance_admin {
        return Ok(());
    }

    let count = admin_count(db).await?;

    let txn = db.begin().await?;

    users::Entity::update_many()
        .col_expr(users::Column::IsInstanceAdmin, Expr::value(true))
        .filter(users::Column::Id.eq(user.id))
        .exec(&txn)
        .await?;

    // Record the action after the session revocation, so the log entry is not lost if the transaction fails.
    audit_service::record(
        &txn,
        domain::types::AuditAction::UserPromoteAdmin,
        ctx,
        audit_service::AuditTarget {
            user_id: Some(user.id),
            team_id: None,
            workspace_id: None,
        },
        reason,
        serde_json::json!({
            "previous": { "is_instance_admin": user.is_instance_admin },
            "admin_count_after": count,
        }),
    )
    .await?;

    txn.commit().await?;

    // TODO(2.14): notify the user by mail. Being handed admin rights without being told is worse than noisy.

    Ok(())
}

/// Revokes instance administrator rights.
pub async fn demote_admin(
    db: &DatabaseConnection,
    ctx: &AuditContext,
    user_id: Uuid,
    reason: Option<&str>,
) -> Result<(), AdminError> {
    let user = users::Entity::find_by_id(user_id)
        .one(db)
        .await?
        .ok_or(AdminError::UserNotFound)?;

    if !user.is_instance_admin {
        return Ok(());
    }

    let count = admin_count(db).await?;
    if count == 1 {
        return Err(AdminError::LastAdmin);
    }

    if Some(user.id) == ctx.actor_id {
        return Err(AdminError::SelfTarget);
    }

    // 4. Transaction: clear is_instance_admin, record UserDemoteAdmin.
    let txn = db.begin().await?;

    users::Entity::update_many()
        .col_expr(users::Column::IsInstanceAdmin, Expr::value(false))
        .filter(users::Column::Id.eq(user.id))
        .exec(&txn)
        .await?;

    audit_service::record(
        &txn,
        domain::types::AuditAction::UserDemoteAdmin,
        ctx,
        audit_service::AuditTarget {
            user_id: Some(user.id),
            team_id: None,
            workspace_id: None,
        },
        reason,
        serde_json::json!({
            "previous": { "is_instance_admin": user.is_instance_admin },
            "admin_count_after": count - 1,
        }),
    )
    .await?;

    txn.commit().await?;

    Ok(())
}

/// Forces a password reset on an account believed to be compromised.
pub async fn force_password_reset(
    db: &DatabaseConnection,
    config: &Config,
    ctx: &AuditContext,
    user_id: Uuid,
    reason: Option<&str>,
) -> Result<(), AdminError> {
    // 1. Load the target and its primary address.
    let user = users::Entity::find_by_id(user_id)
        .one(db)
        .await?
        .ok_or(AdminError::UserNotFound)?;

    let primary_email = auth_service::primary_email(db, user.id)
        .await?
        .ok_or(AdminError::UserNotFound)?;

    let locked_hash = password_service::unusable_hash().await?;

    let txn = db.begin().await?;

    users::Entity::update_many()
        .col_expr(users::Column::PasswordHash, Expr::value(locked_hash))
        .filter(users::Column::Id.eq(user.id))
        .exec(&txn)
        .await?;

    let session_count = revoke_all(
        &txn,
        user_id,
        domain::types::RevokedReason::AdminForcePasswordReset,
        None,
    )
    .await?;

    audit_service::record(
        &txn,
        domain::types::AuditAction::UserPasswordResetForce,
        ctx,
        audit_service::AuditTarget {
            user_id: Some(user.id),
            team_id: None,
            workspace_id: None,
        },
        reason,
        serde_json::json!({ "sessions_revoked": session_count }),
    )
    .await?;

    txn.commit().await?;

    // 6. Issue a reset token and mail it, so the user has a way back in.
    //    Best effort: a mail failure must not undo the lockout, which is the
    //    part that matters.
    let token = auth_token_service::issue(
        db,
        user.id,
        primary_email.id,
        domain::types::TokenKind::PasswordReset,
    )
    .await?;

    if let Err(err) =
        email_service::send_password_reset(config, &primary_email.email, &user.locale, &token).await
    {
        tracing::warn!(error = ?err, %user_id, "failed to send password reset mail");
    }

    Ok(())
}

/// Revokes every session without touching the password.
///
/// Lighter than a forced reset: use it when sessions may have leaked but the
/// password is believed intact.
pub async fn revoke_sessions(
    db: &DatabaseConnection,
    ctx: &AuditContext,
    user_id: Uuid,
    reason: Option<&str>,
) -> Result<u64, AdminError> {
    // Transaction: session_service::revoke_all(AdminRevoked, except: None),
    // then record UserSessionsRevokeAll with the count in meta.
    let txn = db.begin().await?;

    let revoked_count = session_service::revoke_all(
        &txn,
        user_id,
        domain::types::RevokedReason::AdminRevoked,
        None,
    )
    .await?;

    audit_service::record(
        &txn,
        domain::types::AuditAction::UserSessionsRevokeAll,
        ctx,
        audit_service::AuditTarget {
            user_id: Some(user_id),
            team_id: None,
            workspace_id: None,
        },
        reason,
        serde_json::json!({ "sessions_revoked": revoked_count }),
    )
    .await?;

    txn.commit().await?;

    Ok(revoked_count)
}

/// Counts remaining administrators, used by demote_admin.
async fn admin_count<C: ConnectionTrait>(db: &C) -> Result<u64, DbErr> {
    // Counts users where is_instance_admin is true.
    users::Entity::find()
        .filter(users::Column::IsInstanceAdmin.eq(true))
        .count(db)
        .await
}

/// Drops the cached suspension state so a decision applies immediately.
///
/// Best effort by design: the cache is an optimisation, and its TTL bounds
/// the damage of a failure here to thirty seconds.
async fn invalidate_suspension_cache(redis: &mut ConnectionManager, user_id: Uuid) {
    let key = suspension_cache_key(user_id);

    if let Err(err) = redis.del::<_, ()>(&key).await {
        tracing::warn!(error = ?err, %user_id, "failed to invalidate suspension cache");
    }
}

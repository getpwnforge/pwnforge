use crate::{
    audit_service::{self, AuditContext, AuditError},
    auth_service,
    auth_token_service::{self, AuthTokenError},
    config::Config,
    email_service::{self, EmailError},
    password_service::{self, PasswordError},
    session_service::{self, SessionError, revoke_all},
};
use chrono::{Duration, Utc};
use domain::entities::users;
use redis::{AsyncCommands, aio::ConnectionManager};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, DbErr, EntityTrait, PaginatorTrait,
    QueryFilter, QuerySelect, TransactionTrait, prelude::DateTimeWithTimeZone, sea_query::Expr,
};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AdminError {
    /// Target row is gone. Callers surface 404, never 403: a non-admin must
    /// not learn that the admin surface exists.
    #[error("user not found")]
    UserNotFound,

    #[error("user is not suspended")]
    UserNotSuspended,

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
pub async fn suspend_user(
    db: &DatabaseConnection,
    redis: &mut ConnectionManager,
    config: &Config,
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

    let primary_email = auth_service::primary_email(db, user.id).await?;

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

    // Send an email alert to the user, if they have a primary email. This is best-effort: the account is already suspended, and the transaction has committed.
    if let Some(email) = primary_email {
        let until_detail = until
            .map(email_service::SuspensionDuration::Until)
            .or(Some(email_service::SuspensionDuration::Permanent));

        if let Err(err) = email_service::send_account_notice(
            config,
            &email.email,
            &user.locale,
            email_service::AccountEventKind::Suspended,
            email_service::AccountEventDetails {
                username: user.username.clone(),
                until: until_detail,
                purge_date: None,
            },
        )
        .await
        {
            tracing::warn!(error = ?err, %user_id, "failed to send suspension notice mail");
        }
    }

    Ok(())
}

/// Lifts a suspension, whether it had a deadline or not.
pub async fn unsuspend_user(
    db: &DatabaseConnection,
    redis: &mut ConnectionManager,
    config: &Config,
    ctx: &AuditContext,
    user_id: Uuid,
    reason: Option<&str>,
) -> Result<(), AdminError> {
    let user = users::Entity::find_by_id(user_id)
        .one(db)
        .await?
        .ok_or(AdminError::UserNotFound)?;

    if let suspended_at = user.suspended_at
        && suspended_at.is_none()
    {
        // Not suspended, return information to the admin.
        return Err(AdminError::UserNotSuspended);
    }

    let primary_email = auth_service::primary_email(db, user.id).await?;

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

    if let Some(primary_email) = primary_email
        && let Err(err) = email_service::send_account_notice(
            config,
            &primary_email.email,
            &user.locale,
            email_service::AccountEventKind::Unsuspended,
            email_service::AccountEventDetails {
                username: user.username.clone(),
                until: None,
                purge_date: None,
            },
        )
        .await
    {
        tracing::warn!(error = ?err, %user_id, "failed to send account suspension mail");
    }

    Ok(())
}

pub enum DeletionKind {
    Standard,
    IllicitContent,
    SecurityThreat,
}

impl DeletionKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::IllicitContent => "illicit_content",
            Self::SecurityThreat => "security_threat",
        }
    }
}

/// Deletes or schedules the deletion of an account, depending on `kind`.
/// `Standard` opens the 30-day grace period promised in the Terms (§16.5):
/// the row is marked, not removed, and a cron job (`purge_scheduled_deletions`)
/// performs the actual cascade delete once `purge_scheduled_at` is reached.
/// `IllicitContent`/`SecurityThreat` bypass the grace period entirely, same
/// immediate cascade as before.
pub async fn delete_user(
    db: &DatabaseConnection,
    redis: &mut ConnectionManager,
    config: &Config,
    ctx: &AuditContext,
    user_id: Uuid,
    kind: DeletionKind,
    reason: Option<&str>,
) -> Result<(), AdminError> {
    let user = users::Entity::find_by_id(user_id)
        .one(db)
        .await?
        .ok_or(AdminError::UserNotFound)?;

    if Some(user.id) == ctx.actor_id {
        return Err(AdminError::SelfTarget);
    }

    if user.is_instance_admin && admin_count(db).await? == 1 {
        return Err(AdminError::LastAdmin);
    }

    let primary_email = auth_service::primary_email(db, user.id).await?;

    let txn = db.begin().await?;

    let purge_date: Option<DateTimeWithTimeZone> = match kind {
        DeletionKind::Standard => {
            let purge_scheduled_at = (Utc::now() + Duration::days(30)).fixed_offset();

            users::Entity::update_many()
                .col_expr(
                    users::Column::PurgeScheduledAt,
                    Expr::value(purge_scheduled_at),
                )
                .col_expr(
                    users::Column::DeletionRequestedBy,
                    Expr::value(ctx.actor_id),
                )
                .filter(users::Column::Id.eq(user.id))
                .exec(&txn)
                .await?;

            session_service::revoke_all(
                &txn,
                user.id,
                domain::types::RevokedReason::AdminRevoked,
                None,
            )
            .await?;

            audit_service::record(
                &txn,
                domain::types::AuditAction::UserDeletionScheduled,
                ctx,
                audit_service::AuditTarget { user_id: Some(user.id), team_id: None, workspace_id: None },
                reason,
                serde_json::json!({ "kind": kind.as_str(), "purge_scheduled_at": purge_scheduled_at }),
            )
            .await?;

            Some(purge_scheduled_at)
        }
        DeletionKind::IllicitContent | DeletionKind::SecurityThreat => {
            audit_service::record(
                &txn,
                domain::types::AuditAction::UserDeleteByAdmin,
                ctx,
                audit_service::AuditTarget {
                    user_id: Some(user.id),
                    team_id: None,
                    workspace_id: None,
                },
                reason,
                serde_json::json!({
                    "kind": kind.as_str(),
                    "username": user.username,
                    "is_instance_admin": user.is_instance_admin,
                    "created_at": user.created_at,
                }),
            )
            .await?;

            users::Entity::delete_by_id(user.id).exec(&txn).await?;
            None
        }
    };

    txn.commit().await?;

    invalidate_suspension_cache(redis, user.id).await;

    if let Some(primary_email) = primary_email {
        let event_kind = match kind {
            DeletionKind::Standard => email_service::AccountEventKind::DeletionScheduled,
            DeletionKind::IllicitContent | DeletionKind::SecurityThreat => {
                email_service::AccountEventKind::Deleted
            }
        };

        if let Err(err) = email_service::send_account_notice(
            config,
            &primary_email.email,
            &user.locale,
            event_kind,
            email_service::AccountEventDetails {
                username: user.username.clone(),
                until: None,
                purge_date,
            },
        )
        .await
        {
            tracing::warn!(error = ?err, %user_id, "failed to send account deletion mail");
        }
    }

    Ok(())
}

pub async fn cancel_deletion(
    db: &DatabaseConnection,
    config: &Config,
    ctx: &AuditContext,
    user_id: Uuid,
    reason: Option<&str>,
) -> Result<(), AdminError> {
    let user = users::Entity::find_by_id(user_id)
        .one(db)
        .await?
        .ok_or(AdminError::UserNotFound)?;

    if user.purge_scheduled_at.is_none() {
        return Err(AdminError::UserNotSuspended);
    }

    let primary_email = auth_service::primary_email(db, user.id).await?;

    let txn = db.begin().await?;

    users::Entity::update_many()
        .col_expr(
            users::Column::PurgeScheduledAt,
            Expr::value(None::<DateTimeWithTimeZone>),
        )
        .col_expr(
            users::Column::DeletionRequestedBy,
            Expr::value(None::<Uuid>),
        )
        .filter(users::Column::Id.eq(user.id))
        .exec(&txn)
        .await?;

    audit_service::record(
        &txn,
        domain::types::AuditAction::UserDeletionCancelled,
        ctx,
        audit_service::AuditTarget {
            user_id: Some(user.id),
            team_id: None,
            workspace_id: None,
        },
        reason,
        serde_json::json!({ "previous": { "purge_scheduled_at": user.purge_scheduled_at } }),
    )
    .await?;

    txn.commit().await?;

    if let Some(primary_email) = primary_email
        && let Err(err) = email_service::send_account_notice(
            config,
            &primary_email.email,
            &user.locale,
            email_service::AccountEventKind::DeletionCancelled,
            email_service::AccountEventDetails {
                username: user.username.clone(),
                until: None,
                purge_date: None,
            },
        )
        .await
    {
        tracing::warn!(error = ?err, %user_id, "failed to send account deletion cancellation mail");
    }

    Ok(())
}

/// Grants instance administrator rights.
pub async fn promote_admin(
    db: &DatabaseConnection,
    config: &Config,
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

    let primary_email = auth_service::primary_email(db, user.id).await?;

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

    if let Some(primary_email) = primary_email
        && let Err(err) = email_service::send_account_notice(
            config,
            &primary_email.email,
            &user.locale,
            email_service::AccountEventKind::PromotedAdmin,
            email_service::AccountEventDetails {
                username: user.username.clone(),
                until: None,
                purge_date: None,
            },
        )
        .await
    {
        tracing::warn!(error = ?err, %user_id, "failed to send account promotion mail");
    }

    Ok(())
}

/// Revokes instance administrator rights.
pub async fn demote_admin(
    db: &DatabaseConnection,
    config: &Config,
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

    let primary_email = auth_service::primary_email(db, user.id).await?;

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

    if let Some(primary_email) = primary_email
        && let Err(err) = email_service::send_account_notice(
            config,
            &primary_email.email,
            &user.locale,
            email_service::AccountEventKind::DemotedAdmin,
            email_service::AccountEventDetails {
                username: user.username.clone(),
                until: None,
                purge_date: None,
            },
        )
        .await
    {
        tracing::warn!(error = ?err, %user_id, "failed to send account demotion mail");
    }

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

pub fn suspension_cache_key(user_id: Uuid) -> String {
    format!("user:{user_id}:suspension")
}

/// Deletes every account past its purge deadline. Called from the periodic
/// task in api/src/tasks/cleanup.rs, not from the CLI.
///
/// Cannot be a single bulk DELETE like the token cleanups: each row needs its
/// own UserPurged audit entry, same granularity as every other action here.
pub async fn purge_scheduled_deletions(db: &DatabaseConnection) -> Result<u64, AdminError> {
    const BATCH_SIZE: u64 = 1000;
    let mut total = 0;

    loop {
        let ids: Vec<Uuid> = users::Entity::find()
            .select_only()
            .column(users::Column::Id)
            .filter(users::Column::PurgeScheduledAt.lte(Utc::now()))
            .limit(BATCH_SIZE)
            .into_tuple()
            .all(db)
            .await?;

        if ids.is_empty() {
            break;
        }

        for user_id in ids {
            let txn = db.begin().await?;
            // System-initiated: no human actor behind a scheduled purge.
            let ctx = AuditContext {
                actor_id: None,
                ip_address: None,
                user_agent: None,
            };

            audit_service::record(
                &txn,
                domain::types::AuditAction::UserPurged,
                &ctx,
                audit_service::AuditTarget {
                    user_id: Some(user_id),
                    team_id: None,
                    workspace_id: None,
                },
                None,
                serde_json::json!({}),
            )
            .await?;

            users::Entity::delete_by_id(user_id).exec(&txn).await?;
            txn.commit().await?;
            total += 1;
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    Ok(total)
}

/// Counts remaining administrators, used by demote_admin.
async fn admin_count<C: ConnectionTrait>(db: &C) -> Result<u64, DbErr> {
    // Counts users where is_instance_admin is true and purge_scheduled_at is null, to avoid counting admins that are scheduled for deletion.
    users::Entity::find()
        .filter(users::Column::IsInstanceAdmin.eq(true))
        .filter(users::Column::PurgeScheduledAt.is_null())
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

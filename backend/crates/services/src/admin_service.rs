use crate::{
    audit_service::{self, AuditContext, AuditError},
    auth_service,
    auth_token_service::{self, AuthTokenError},
    config::Config,
    email_service::{self, EmailError},
    password_service::{self, PasswordError},
    session_service::{self, SessionError, revoke_all},
    storage_service::{AvatarKind, ObjectStore, ObjectsToPurge},
    team_service::{self, TeamError},
    workspace_service::{self, WorkspaceError},
};
use chrono::{Duration, Utc};
use domain::dto::workspace::WorkspaceMode;
use domain::entities::{teams, users, workspaces};
use redis::{AsyncCommands, aio::ConnectionManager};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, DbErr, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, TransactionTrait, prelude::DateTimeWithTimeZone,
    sea_query::Expr,
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

    /// Deleting an account's solo workspaces failed.
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),

    /// Deleting an account's personal team failed.
    #[error(transparent)]
    Team(#[from] TeamError),
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
        .col_expr(users::Column::UpdatedAt, Expr::current_timestamp())
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
        .col_expr(users::Column::UpdatedAt, Expr::current_timestamp())
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

/// What an administrator asks for when deleting an account.
pub struct DeletionRequest<'a> {
    pub kind: DeletionKind,
    pub reason: Option<&'a str>,
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
    store: &ObjectStore,
    config: &Config,
    ctx: &AuditContext,
    user_id: Uuid,
    request: DeletionRequest<'_>,
) -> Result<(), AdminError> {
    let DeletionRequest { kind, reason } = request;

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

    let purge_date: Option<DateTimeWithTimeZone> = match kind {
        DeletionKind::Standard => {
            let txn = db.begin().await?;
            let purge_scheduled_at = (Utc::now() + Duration::days(30)).fixed_offset();

            users::Entity::update_many()
                .col_expr(users::Column::UpdatedAt, Expr::current_timestamp())
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

            txn.commit().await?;
            Some(purge_scheduled_at)
        }
        DeletionKind::IllicitContent | DeletionKind::SecurityThreat => {
            let record = DeletionRecord {
                action: domain::types::AuditAction::UserDeleteByAdmin,
                ctx,
                reason,
                details: serde_json::json!({
                    "kind": kind.as_str(),
                    "username": user.username,
                    "is_instance_admin": user.is_instance_admin,
                    "created_at": user.created_at,
                }),
            };

            delete_account(db, store, user.id, record).await?;
            None
        }
    };

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
        .col_expr(users::Column::UpdatedAt, Expr::current_timestamp())
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
        .col_expr(users::Column::UpdatedAt, Expr::current_timestamp())
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
        .col_expr(users::Column::UpdatedAt, Expr::current_timestamp())
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
        .col_expr(users::Column::UpdatedAt, Expr::current_timestamp())
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

/// What one pass of `purge_scheduled_deletions` did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PurgeOutcome {
    pub purged: u64,
    pub failed: u64,
}

/// Deletes every account past its purge deadline. Called from the periodic
/// task in api/src/tasks/cleanup.rs, not from the CLI.
///
/// Cannot be a single bulk DELETE like the token cleanups: each row needs its
/// own UserPurged audit entry, same granularity as every other action here.
///
/// Each account is deleted on its own. One that fails is logged and left for
/// the next pass, and the accounts after it are still purged: the walk goes
/// by id, past the failures, so one account that cannot be deleted can
/// neither stop the pass nor be selected again until it ends.
pub async fn purge_scheduled_deletions(
    db: &DatabaseConnection,
    store: &ObjectStore,
) -> Result<PurgeOutcome, AdminError> {
    const BATCH_SIZE: u64 = 1000;
    let due = Utc::now();
    let mut outcome = PurgeOutcome::default();
    let mut after: Option<Uuid> = None;

    // System-initiated: no human actor behind a scheduled purge.
    let ctx = AuditContext {
        actor_id: None,
        ip_address: None,
        user_agent: None,
    };

    loop {
        let mut batch = users::Entity::find()
            .select_only()
            .column(users::Column::Id)
            .filter(users::Column::PurgeScheduledAt.lte(due))
            .order_by_asc(users::Column::Id)
            .limit(BATCH_SIZE);
        if let Some(after) = after {
            batch = batch.filter(users::Column::Id.gt(after));
        }
        let ids: Vec<Uuid> = batch.into_tuple().all(db).await?;

        let Some(last) = ids.last().copied() else {
            break;
        };

        for user_id in ids {
            let record = DeletionRecord {
                action: domain::types::AuditAction::UserPurged,
                ctx: &ctx,
                reason: None,
                details: serde_json::json!({}),
            };

            match delete_account(db, store, user_id, record).await {
                Ok(()) => outcome.purged += 1,
                Err(error) => {
                    tracing::error!(
                        %user_id,
                        ?error,
                        "scheduled deletion of this account failed, moving on to the next"
                    );
                    outcome.failed += 1;
                }
            }
        }

        after = Some(last);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    Ok(outcome)
}

/// The audit entry an account deletion writes, in the same transaction as
/// the deletion itself.
struct DeletionRecord<'a> {
    action: domain::types::AuditAction,
    ctx: &'a AuditContext,
    reason: Option<&'a str>,
    details: serde_json::Value,
}

/// Deletes one account and what is only its own, in one transaction, then
/// purges their objects once it is committed. The only path that deletes a
/// user row: the scheduled purge and the immediate deletions both come here.
async fn delete_account(
    db: &DatabaseConnection,
    store: &ObjectStore,
    user_id: Uuid,
    record: DeletionRecord<'_>,
) -> Result<(), AdminError> {
    let txn = db.begin().await?;

    audit_service::record(
        &txn,
        record.action,
        record.ctx,
        audit_service::AuditTarget {
            user_id: Some(user_id),
            team_id: None,
            workspace_id: None,
        },
        record.reason,
        record.details,
    )
    .await?;

    let objects = delete_account_rows(&txn, user_id).await?;
    txn.commit().await?;

    store.purge(objects).await;
    Ok(())
}

/// The database half of `delete_account`: the account's solo workspaces, its
/// personal team with that team's workspaces, then the account. Returns
/// every object to purge once the caller has committed.
///
/// A team the account merely belongs to is not touched; its membership goes
/// with the account through the cascade.
async fn delete_account_rows<C: ConnectionTrait>(
    conn: &C,
    user_id: Uuid,
) -> Result<ObjectsToPurge, AdminError> {
    // Locked first: a solo workspace or a personal team created for this
    // account meanwhile waits on this row, then fails on its foreign key,
    // rather than landing between the lookups below and the delete.
    users::Entity::find_by_id(user_id)
        .lock_exclusive()
        .one(conn)
        .await?
        .ok_or(AdminError::UserNotFound)?;

    let mut objects = ObjectsToPurge::default();

    let solo_workspaces: Vec<Uuid> = workspaces::Entity::find()
        .select_only()
        .column(workspaces::Column::Id)
        .filter(workspaces::Column::Mode.eq(WorkspaceMode::Solo.as_str()))
        .filter(workspaces::Column::OwnerId.eq(user_id))
        .into_tuple()
        .all(conn)
        .await?;
    for workspace_id in solo_workspaces {
        objects.add(workspace_service::delete_workspace_rows(conn, workspace_id).await?);
    }

    let personal_team: Option<Uuid> = teams::Entity::find()
        .select_only()
        .column(teams::Column::Id)
        .filter(teams::Column::CreatedBy.eq(user_id))
        .filter(teams::Column::Personal.eq(true))
        .into_tuple()
        .one(conn)
        .await?;
    if let Some(team_id) = personal_team {
        objects.add(team_service::delete_team_rows(conn, team_id).await?);
    }

    objects.add(ObjectsToPurge::avatar(AvatarKind::User, user_id));
    users::Entity::delete_by_id(user_id).exec(conn).await?;

    Ok(objects)
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

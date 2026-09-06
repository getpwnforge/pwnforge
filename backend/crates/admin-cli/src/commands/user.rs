//! `pwnforge-admin user ...` - account lifecycle and access management.
//!
//! Destructive actions require interactive confirmation. Self-suspension and
//! "last remaining instance admin" guards described in SECURITY.md apply to
//! the future HTTP admin routes; here we still enforce the admin-quorum check
//! since the CLI can just as easily lock out the instance.

use crate::CliContext;
use crate::datetime::parse_datetime;
use crate::lookup::resolve_user;

use chrono::{DateTime, Utc};
use clap::{Args, Subcommand};
use dialoguer::Confirm;
use domain::entities::users;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use services::admin_service::{self, AdminError};
use services::audit_service::AuditContext;
use services::auth_service;
use tabled::{Table, Tabled};
use uuid::Uuid;

#[derive(Args)]
pub struct UserArgs {
    #[command(subcommand)]
    command: UserCommand,
}

#[derive(Tabled)]
struct UserRow {
    id: Uuid,
    username: String,
    #[tabled(rename = "admin")]
    is_instance_admin: bool,
    status: String,
}

#[derive(Tabled)]
struct UserDetailRow {
    id: Uuid,
    username: String,
    email: String,
    #[tabled(rename = "admin")]
    is_instance_admin: bool,
    status: String,
}

#[derive(Subcommand)]
enum UserCommand {
    Suspend {
        id: String,
        #[arg(long)]
        reason: String,
        /// Suspend until this date instead of permanently. Accepts an RFC 3339
        /// timestamp, e.g. `2026-09-01T00:00:00Z`.
        /// Accepts a bare date (midnight UTC) for convenience, e.g. `2026-09-01`.
        #[arg(long, value_parser = parse_datetime)]
        until: Option<DateTime<Utc>>,
    },
    Unsuspend {
        id: String,
        #[arg(long)]
        reason: Option<String>,
    },
    List,
    ListSuspended,
    Delete {
        id: String,
        #[arg(long, value_enum, default_value = "standard")]
        kind: DeletionKindArg,
        #[arg(long)]
        reason: Option<String>,
    },
    CancelDeletion {
        id: String,
        #[arg(long)]
        reason: Option<String>,
    },
    PromoteInstanceAdmin {
        id: String,
        #[arg(long)]
        reason: Option<String>,
    },
    DemoteInstanceAdmin {
        id: String,
        #[arg(long)]
        reason: Option<String>,
    },
    RevokeSessions {
        id: String,
        #[arg(long)]
        reason: Option<String>,
    },
    ForcePasswordReset {
        id: String,
        #[arg(long)]
        reason: Option<String>,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum DeletionKindArg {
    Standard,
    IllicitContent,
    SecurityThreat,
}

impl From<DeletionKindArg> for admin_service::DeletionKind {
    fn from(value: DeletionKindArg) -> Self {
        match value {
            DeletionKindArg::Standard => admin_service::DeletionKind::Standard,
            DeletionKindArg::IllicitContent => admin_service::DeletionKind::IllicitContent,
            DeletionKindArg::SecurityThreat => admin_service::DeletionKind::SecurityThreat,
        }
    }
}

/// Every field `None` marks the action as coming from the CLI rather than an
/// HTTP request, per `AuditContext`'s own doc comment.
fn cli_ctx(ctx: &CliContext) -> AuditContext {
    AuditContext {
        actor_id: ctx.actor_id,
        ip_address: None,
        user_agent: None,
    }
}

/// Clean message + non-zero exit for the errors an operator can act on;
/// anything else (Db, Session, ...) bubbles up through `?` as-is.
fn report(err: AdminError) -> anyhow::Error {
    match err {
        AdminError::UserNotFound => {
            eprintln!("Error: no such user.");
            std::process::exit(1);
        }
        AdminError::LastAdmin => {
            eprintln!("Error: refusing to leave the instance with no administrator.");
            std::process::exit(1);
        }
        AdminError::SelfTarget => {
            eprintln!("Error: refusing to target the account you are running as.");
            std::process::exit(1);
        }
        other => other.into(),
    }
}

pub async fn handle(ctx: &CliContext, args: UserArgs) -> anyhow::Result<()> {
    match args.command {
        UserCommand::Suspend { id, reason, until } => suspend(ctx, id, reason, until).await,
        UserCommand::Unsuspend { id, reason } => unsuspend(ctx, id, reason).await,
        UserCommand::List => list(ctx, false).await,
        UserCommand::ListSuspended => list(ctx, true).await,
        UserCommand::Delete { id, kind, reason } => delete(ctx, id, kind, reason).await,
        UserCommand::CancelDeletion { id, reason } => cancel_deletion(ctx, id, reason).await,
        UserCommand::PromoteInstanceAdmin { id, reason } => {
            set_instance_admin(ctx, id, true, reason).await
        }
        UserCommand::DemoteInstanceAdmin { id, reason } => {
            set_instance_admin(ctx, id, false, reason).await
        }
        UserCommand::RevokeSessions { id, reason } => revoke_sessions(ctx, id, reason).await,
        UserCommand::ForcePasswordReset { id, reason } => {
            force_password_reset(ctx, id, reason).await
        }
    }
}

/// Suspends a user (soft suspension: data preserved, login refused).
async fn suspend(
    ctx: &CliContext,
    identifier: String,
    reason: String,
    until: Option<DateTime<Utc>>,
) -> anyhow::Result<()> {
    let id = resolve_user(&ctx.db, &identifier).await?;

    if let Some(until) = until
        && until <= Utc::now()
    {
        anyhow::bail!("--until must be in the future");
    }

    let mut redis = ctx.redis.clone();
    let until = until.map(|dt| dt.fixed_offset());

    admin_service::suspend_user(
        &ctx.db,
        &mut redis,
        &ctx.config,
        &cli_ctx(ctx),
        id,
        until,
        &reason,
    )
    .await
    .map_err(report)?;

    match until {
        Some(until) => println!(
            "User {id} suspended until {}. Reason: {reason}",
            until.to_rfc3339()
        ),
        None => println!("User {id} suspended (permanent). Reason: {reason}"),
    }

    Ok(())
}

/// Reactivates a previously suspended user.
async fn unsuspend(
    ctx: &CliContext,
    identifier: String,
    reason: Option<String>,
) -> anyhow::Result<()> {
    let id = resolve_user(&ctx.db, &identifier).await?;
    let mut redis = ctx.redis.clone();

    admin_service::unsuspend_user(
        &ctx.db,
        &mut redis,
        &ctx.config,
        &cli_ctx(ctx),
        id,
        reason.as_deref(),
    )
    .await
    .map_err(report)?;

    println!(
        "User {id} unsuspended. Reason: {}",
        reason.unwrap_or_else(|| "none".to_string())
    );

    Ok(())
}

/// Lists users, optionally filtered to suspended-only.
async fn list(ctx: &CliContext, suspended_only: bool) -> anyhow::Result<()> {
    let mut query = users::Entity::find();

    if suspended_only {
        query = query.filter(users::Column::SuspendedAt.is_not_null());
    }

    let rows: Vec<UserRow> = query
        .all(&ctx.db)
        .await?
        .into_iter()
        .map(|user| UserRow {
            id: user.id,
            username: user.username,
            is_instance_admin: user.is_instance_admin,
            status: if user.suspended_at.is_some() {
                "suspended".into()
            } else {
                "active".into()
            },
        })
        .collect();

    println!("{}", Table::new(rows));

    Ok(())
}

/// Schedules or immediately deletes a user, depending on the `kind` argument.
/// The `reason` argument is optional but recommended for audit purposes.
/// The operator is prompted for confirmation before proceeding.
async fn delete(
    ctx: &CliContext,
    identifier: String,
    kind: DeletionKindArg,
    reason: Option<String>,
) -> anyhow::Result<()> {
    let id = resolve_user(&ctx.db, &identifier).await?;
    let (user, email) = auth_service::fetch_user(&ctx.db, id).await?;

    println!(
        "{}",
        Table::new(vec![UserDetailRow {
            id: user.id,
            username: user.username,
            email: email.email,
            is_instance_admin: user.is_instance_admin,
            status: if user.suspended_at.is_some() {
                "suspended".into()
            } else {
                "active".into()
            },
        }])
    );

    let prompt = match kind {
        DeletionKindArg::Standard => format!(
            "This schedules user {id} for deletion in 30 days (data stays exportable until then). Continue?"
        ),
        DeletionKindArg::IllicitContent | DeletionKindArg::SecurityThreat => format!(
            "This permanently deletes user {id} immediately and cannot be undone. Continue?"
        ),
    };

    if !Confirm::new()
        .with_prompt(prompt)
        .default(false)
        .interact()?
    {
        println!("Aborted.");
        return Ok(());
    }

    let mut redis = ctx.redis.clone();

    admin_service::delete_user(
        &ctx.db,
        &mut redis,
        &ctx.config,
        &cli_ctx(ctx),
        id,
        kind.into(),
        reason.as_deref(),
    )
    .await
    .map_err(report)?;

    let outcome = match kind {
        DeletionKindArg::Standard => "scheduled for deletion in 30 days",
        DeletionKindArg::IllicitContent | DeletionKindArg::SecurityThreat => "deleted immediately",
    };

    println!(
        "User {id} {outcome}. Reason: {}",
        reason.unwrap_or_else(|| "none".to_string())
    );

    Ok(())
}

async fn cancel_deletion(
    ctx: &CliContext,
    id: String,
    reason: Option<String>,
) -> anyhow::Result<()> {
    let id = resolve_user(&ctx.db, &id).await?;
    let (user, email) = auth_service::fetch_user(&ctx.db, id).await?;

    println!(
        "{}",
        Table::new(vec![UserDetailRow {
            id: user.id,
            username: user.username,
            email: email.email,
            is_instance_admin: user.is_instance_admin,
            status: if user.suspended_at.is_some() {
                "suspended".into()
            } else {
                "active".into()
            },
        }])
    );

    if !Confirm::new()
        .with_prompt(format!(
            "This cancels the scheduled deletion of user {id}. Continue?"
        ))
        .default(false)
        .interact()?
    {
        println!("Aborted.");
        return Ok(());
    }

    admin_service::cancel_deletion(&ctx.db, &ctx.config, &cli_ctx(ctx), id, reason.as_deref())
        .await?;

    println!(
        "User {id} deletion cancelled. Reason: {}",
        reason.unwrap_or_else(|| "none".to_string())
    );

    Ok(())
}

/// Grants or revokes instance-admin rights.
async fn set_instance_admin(
    ctx: &CliContext,
    identifier: String,
    promote: bool,
    reason: Option<String>,
) -> anyhow::Result<()> {
    let id = resolve_user(&ctx.db, &identifier).await?;
    let result = if promote {
        admin_service::promote_admin(&ctx.db, &ctx.config, &cli_ctx(ctx), id, reason.as_deref())
            .await
    } else {
        admin_service::demote_admin(&ctx.db, &ctx.config, &cli_ctx(ctx), id, reason.as_deref())
            .await
    };

    result.map_err(report)?;

    println!(
        "User {id} {} instance admin. Reason: {}",
        if promote {
            "promoted to"
        } else {
            "demoted from"
        },
        reason.unwrap_or_else(|| "none".to_string())
    );

    Ok(())
}

/// Revokes every active refresh token for a user (forces logout everywhere).
async fn revoke_sessions(
    ctx: &CliContext,
    identifier: String,
    reason: Option<String>,
) -> anyhow::Result<()> {
    let id = resolve_user(&ctx.db, &identifier).await?;
    let count = admin_service::revoke_sessions(&ctx.db, &cli_ctx(ctx), id, reason.as_deref())
        .await
        .map_err(report)?;

    println!(
        "Revoked {} active sessions for user {id}. Reason: {}",
        count,
        reason.unwrap_or_else(|| "none".to_string())
    );

    Ok(())
}

/// Forces a password reset on next login (invalidates the current password).
async fn force_password_reset(
    ctx: &CliContext,
    identifier: String,
    reason: Option<String>,
) -> anyhow::Result<()> {
    let id = resolve_user(&ctx.db, &identifier).await?;
    admin_service::force_password_reset(&ctx.db, &ctx.config, &cli_ctx(ctx), id, reason.as_deref())
        .await
        .map_err(report)?;

    println!(
        "Forced password reset for user {id}. Reason: {}. A reset link has been sent to the user's email address.",
        reason.unwrap_or_else(|| "none".to_string())
    );

    Ok(())
}

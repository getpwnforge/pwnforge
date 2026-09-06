//! `pwnforge-admin audit ...` - read-only access to the instance audit log.
//! The DB role has no DELETE/UPDATE grant on `instance_audit_log` (see SECURITY.md),
//! so this module is intentionally read/export only, never write.

use crate::{CliContext, datetime::parse_datetime, lookup::resolve_user};

use chrono::{DateTime, Utc};
use clap::{Args, Subcommand};
use domain::entities::instance_audit_logs;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use tabled::{Table, Tabled};

#[derive(Args)]
pub struct AuditArgs {
    #[command(subcommand)]
    command: AuditCommand,
}

#[derive(Subcommand)]
enum AuditCommand {
    /// Show the most recent audit entries
    Recent {
        #[arg(long, default_value_t = 50)]
        limit: u64,
    },
    /// Search the audit log by actor, action type and/or date
    Search {
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        action: Option<String>,
        /// Accepts an RFC 3339 timestamp, e.g. `2026-09-01T00:00:00Z`.
        /// Accepts a bare date (midnight UTC) for convenience, e.g. `2026-09-01`.
        #[arg(long, value_parser = parse_datetime)]
        since: Option<DateTime<Utc>>,
    },
    /// Export the full audit log to a file
    Export {
        #[arg(long, default_value = "json")]
        format: String,
    },
}

/// Entry point called from `main.rs`, routes to the matching handler below.
pub async fn handle(ctx: &CliContext, args: AuditArgs) -> anyhow::Result<()> {
    match args.command {
        AuditCommand::Recent { limit } => recent(ctx, limit).await,
        AuditCommand::Search {
            actor,
            action,
            since,
        } => search(ctx, actor, action, since).await,
        AuditCommand::Export { format } => export(ctx, format).await,
    }
}

#[derive(Tabled)]
struct AuditRow {
    created_at: String,
    actor_id: String,
    action: String,
    target_user_id: String,
}

impl From<instance_audit_logs::Model> for AuditRow {
    fn from(row: instance_audit_logs::Model) -> Self {
        AuditRow {
            created_at: row.created_at.to_rfc3339(),
            actor_id: row
                .actor_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "-".into()),
            action: row.action,
            target_user_id: row
                .target_user_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "-".into()),
        }
    }
}

fn print_rows(rows: Vec<instance_audit_logs::Model>) {
    let rows: Vec<AuditRow> = rows.into_iter().map(AuditRow::from).collect();
    println!("{}", Table::new(rows));
}

/// Prints the N most recent audit log entries, newest first.
async fn recent(ctx: &CliContext, limit: u64) -> anyhow::Result<()> {
    let rows = instance_audit_logs::Entity::find()
        .order_by_desc(instance_audit_logs::Column::CreatedAt)
        .limit(limit)
        .all(&ctx.db)
        .await?;

    print_rows(rows);

    Ok(())
}

/// Filters the audit log by actor handle, action type, and/or a lower date bound.
async fn search(
    ctx: &CliContext,
    actor: Option<String>,
    action: Option<String>,
    since: Option<DateTime<Utc>>,
) -> anyhow::Result<()> {
    let mut query = instance_audit_logs::Entity::find();

    if let Some(identifier) = actor {
        let actor_id = resolve_user(&ctx.db, &identifier).await?;
        query = query.filter(instance_audit_logs::Column::ActorId.eq(actor_id));
    }

    if let Some(action) = action {
        query = query.filter(instance_audit_logs::Column::Action.eq(action));
    }

    if let Some(since) = since {
        query = query.filter(instance_audit_logs::Column::CreatedAt.gte(since));
    }

    let rows = query
        .order_by_desc(instance_audit_logs::Column::CreatedAt)
        .all(&ctx.db)
        .await?;

    print_rows(rows);

    Ok(())
}

/// Exports the full audit log to a file in the requested format ("json" only for now).
async fn export(ctx: &CliContext, format: String) -> anyhow::Result<()> {
    if format != "json" {
        anyhow::bail!("unsupported export format: {format} (only 'json' is supported)");
    }

    let rows = instance_audit_logs::Entity::find()
        .order_by_desc(instance_audit_logs::Column::CreatedAt)
        .all(&ctx.db)
        .await?;

    let json = serde_json::to_string_pretty(
        &rows
            .into_iter()
            .map(|row| {
                serde_json::json!({
                    "id": row.id,
                    "created_at": row.created_at.to_rfc3339(),
                    "actor_id": row.actor_id,
                    "action": row.action,
                    "target_user_id": row.target_user_id,
                    "target_team_id": row.target_team_id,
                    "target_workspace_id": row.target_workspace_id,
                    "reason": row.reason,
                    "meta": row.meta,
                    "ip_address": row.ip_address,
                    "user_agent": row.user_agent,
                })
            })
            .collect::<Vec<_>>(),
    )?;

    let filename = format!(
        "pwnforge_audit_export_{}.json",
        Utc::now().format("%Y%m%dT%H%M%SZ")
    );
    std::fs::write(&filename, json)?;
    eprintln!("Audit log exported to {}", filename);

    Ok(())
}

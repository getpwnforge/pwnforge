//! `pwnforge-admin alert ...` - manage instance-wide banner alerts.

use chrono::{DateTime, Utc};
use clap::{Args, Subcommand};
use dialoguer::Confirm;
use domain::entities::instance_alerts;
use domain::types::AlertKind;
use sea_orm::EntityTrait;
use services::alert_service::{self, AlertError};
use services::audit_service::AuditContext;
use tabled::{Table, Tabled};
use uuid::Uuid;

use crate::CliContext;
use crate::datetime::parse_datetime;

#[derive(Args)]
pub struct AlertArgs {
    #[command(subcommand)]
    command: AlertCommand,
}

/// Mirrors `domain::types::AlertKind`, kept separate rather than deriving
/// `clap::ValueEnum` directly on it: `domain` stays free of CLI-parsing
/// dependencies, the conversion below is the only cost.
#[derive(Clone, Copy, clap::ValueEnum)]
enum AlertKindArg {
    Info,
    Warning,
    Danger,
    Maintenance,
}

impl From<AlertKindArg> for AlertKind {
    fn from(kind: AlertKindArg) -> Self {
        match kind {
            AlertKindArg::Info => AlertKind::Info,
            AlertKindArg::Warning => AlertKind::Warning,
            AlertKindArg::Danger => AlertKind::Danger,
            AlertKindArg::Maintenance => AlertKind::Maintenance,
        }
    }
}

#[derive(Subcommand)]
enum AlertCommand {
    /// Publish a new alert.
    Create {
        #[arg(long, value_enum)]
        kind: AlertKindArg,
        #[arg(long)]
        message: String,
        #[arg(long)]
        link_url: Option<String>,
        #[arg(long, value_parser = parse_datetime)]
        starts_at: Option<DateTime<Utc>>,
        #[arg(long, value_parser = parse_datetime)]
        ends_at: Option<DateTime<Utc>>,
    },
    /// Update an existing alert. Omitted fields are left untouched.
    Update {
        id: Uuid,
        #[arg(long)]
        message: Option<String>,
        #[arg(long, conflicts_with = "clear_link_url")]
        link_url: Option<String>,
        #[arg(long)]
        clear_link_url: bool,
        #[arg(long, conflicts_with = "clear_ends_at", value_parser = parse_datetime)]
        ends_at: Option<DateTime<Utc>>,
        #[arg(long)]
        clear_ends_at: bool,
        /// Set to `false` to take a live alert down immediately.
        #[arg(long)]
        active: Option<bool>,
    },
    /// Permanently delete an alert.
    Delete { id: Uuid },
    /// List every alert, active and past.
    List,
    /// Show the alert currently showing to users, if any.
    Active,
}

fn cli_ctx(ctx: &CliContext) -> AuditContext {
    AuditContext {
        actor_id: ctx.actor_id,
        ip_address: None,
        user_agent: None,
    }
}

fn report(err: AlertError) -> anyhow::Error {
    match err {
        AlertError::NotFound => {
            eprintln!("Error: no alert with that id.");
            std::process::exit(1);
        }
        other => other.into(),
    }
}

#[derive(Tabled)]
struct AlertRow {
    id: Uuid,
    kind: String,
    message: String,
    link_url: String,
    starts_at: String,
    ends_at: String,
    active: bool,
    created_at: String,
}

impl From<instance_alerts::Model> for AlertRow {
    fn from(row: instance_alerts::Model) -> Self {
        AlertRow {
            id: row.id,
            kind: row.kind,
            message: row.message,
            link_url: row.link_url.unwrap_or_else(|| "-".into()),
            starts_at: row.starts_at.to_rfc3339(),
            ends_at: row
                .ends_at
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| "-".into()),
            active: row.is_active,
            created_at: row.created_at.to_rfc3339(),
        }
    }
}

pub async fn handle(ctx: &CliContext, args: AlertArgs) -> anyhow::Result<()> {
    match args.command {
        AlertCommand::Create {
            kind,
            message,
            link_url,
            starts_at,
            ends_at,
        } => create(ctx, kind, message, link_url, starts_at, ends_at).await,
        AlertCommand::Update {
            id,
            message,
            link_url,
            clear_link_url,
            ends_at,
            clear_ends_at,
            active,
        } => {
            update(
                ctx,
                id,
                message,
                link_url,
                clear_link_url,
                ends_at,
                clear_ends_at,
                active,
            )
            .await
        }
        AlertCommand::Delete { id } => delete(ctx, id).await,
        AlertCommand::List => list(ctx).await,
        AlertCommand::Active => active(ctx).await,
    }
}

#[allow(clippy::too_many_arguments)]
async fn create(
    ctx: &CliContext,
    kind: AlertKindArg,
    message: String,
    link_url: Option<String>,
    starts_at: Option<DateTime<Utc>>,
    ends_at: Option<DateTime<Utc>>,
) -> anyhow::Result<()> {
    let starts_at = starts_at.unwrap_or_else(Utc::now).fixed_offset();
    let ends_at = ends_at.map(|dt| dt.fixed_offset());

    let alert = alert_service::create_alert(
        &ctx.db,
        &cli_ctx(ctx),
        kind.into(),
        message,
        link_url.as_deref(),
        starts_at,
        ends_at,
    )
    .await
    .map_err(report)?;

    println!("Alert {} created.", alert.id);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn update(
    ctx: &CliContext,
    id: Uuid,
    message: Option<String>,
    link_url: Option<String>,
    clear_link_url: bool,
    ends_at: Option<DateTime<Utc>>,
    clear_ends_at: bool,
    active: Option<bool>,
) -> anyhow::Result<()> {
    let link_url = if clear_link_url {
        Some(None)
    } else {
        link_url.map(Some)
    };
    let ends_at = if clear_ends_at {
        Some(None)
    } else {
        ends_at.map(|dt| Some(dt.fixed_offset()))
    };

    let updated = alert_service::update_alert(
        &ctx.db,
        &cli_ctx(ctx),
        id,
        message,
        link_url,
        ends_at,
        active,
    )
    .await
    .map_err(report)?;

    println!("Alert {} updated.", updated.id);
    Ok(())
}

async fn delete(ctx: &CliContext, id: Uuid) -> anyhow::Result<()> {
    let alert = instance_alerts::Entity::find_by_id(id)
        .one(&ctx.db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("No alert with that id."))?;

    println!("{}", Table::new(vec![AlertRow::from(alert)]));

    let confirmed = Confirm::new()
        .with_prompt(
            "This alert will be permanently deleted and cannot be undone. Continue?".to_string(),
        )
        .default(false)
        .interact()?;

    if !confirmed {
        println!("Aborted.");
        return Ok(());
    }

    alert_service::delete_alert(&ctx.db, &cli_ctx(ctx), id)
        .await
        .map_err(report)?;
    println!("Alert {id} deleted.");
    Ok(())
}

async fn list(ctx: &CliContext) -> anyhow::Result<()> {
    let rows: Vec<AlertRow> = alert_service::list_alerts(&ctx.db)
        .await
        .map_err(report)?
        .into_iter()
        .map(AlertRow::from)
        .collect();

    println!("{}", Table::new(rows));
    Ok(())
}

async fn active(ctx: &CliContext) -> anyhow::Result<()> {
    let alerts = alert_service::active_alerts(&ctx.db)
        .await
        .map_err(report)?;

    if alerts.is_empty() {
        println!("No alert currently active.");
        return Ok(());
    }

    // Highest priority first, same order the public banner walks through.
    println!(
        "{}",
        Table::new(alerts.into_iter().map(AlertRow::from).collect::<Vec<_>>())
    );

    Ok(())
}

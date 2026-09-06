//! Entry point for the `pwnforge-admin` CLI.
//! Loads config, opens the DB and Redis connections once, and dispatches
//! the parsed subcommand to its handler.

mod commands;
mod datetime;
mod lookup;

use clap::{Parser, Subcommand};
use sea_orm::DatabaseConnection;
use services::{ConnectionManager, config::Config};

use commands::{alerts, audit, email, stats, user};
use uuid::Uuid;

use crate::lookup::resolve_user;

#[derive(Parser)]
#[command(name = "pwnforge-admin", version, about)]
struct Cli {
    #[arg(long, global = true)]
    actor: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// User management (suspend, promote, sessions, ...)
    User(user::UserArgs),
    /// Read-only access to the instance audit log
    Audit(audit::AuditArgs),
    /// Manage blocked email domains
    Email(email::EmailArgs),
    /// Instance-wide statistics
    Stats(stats::StatsArgs),
    /// Alert management
    Alerts(alerts::AlertArgs),
}

/// Everything a command handler needs, built once per invocation.
/// Owned outright (no `Arc`): unlike `AppState`, nothing here is shared
/// across concurrent requests, so there is nothing to guard or share.
pub struct CliContext {
    pub db: DatabaseConnection,
    pub redis: ConnectionManager,
    pub config: Config,
    pub actor_id: Option<Uuid>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    let ctx = build_context(cli.actor.as_deref()).await?;

    match cli.command {
        Commands::User(args) => user::handle(&ctx, args).await,
        Commands::Audit(args) => audit::handle(&ctx, args).await,
        Commands::Email(args) => email::handle(&ctx, args).await,
        Commands::Stats(args) => stats::handle(&ctx, args).await,
        Commands::Alerts(args) => alerts::handle(&ctx, args).await,
    }
}

/// Loads `Config` from env, then opens the DB and Redis connections it describes.
/// Fails fast on any missing var or unreachable service, same convention as `api`.
async fn build_context(actor: Option<&str>) -> anyhow::Result<CliContext> {
    let config = services::config::Config::from_env()?;
    let db = services::connect_db(&config).await?;
    let redis = services::connect_redis(&config).await?;
    let actor_id = match actor {
        Some(identifier) => Some(resolve_user(&db, identifier).await?),
        None => None,
    };
    Ok(CliContext {
        db,
        redis,
        config,
        actor_id,
    })
}

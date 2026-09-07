//! `pwnforge-admin stats ...` - instance-wide statistics.
//! Not yet scoped as a numbered sub-phase in TODO.md; placeholder matching
//! the crate layout from PROJECT_TREE.md.

use crate::CliContext;
use clap::{Args, Subcommand};
use sea_orm::DatabaseConnection;
use services::stats_service;
use tabled::{Table, Tabled};

#[derive(Args)]
pub struct StatsArgs {
    #[command(subcommand)]
    command: StatsCommand,
}

#[derive(Subcommand)]
enum StatsCommand {
    /// Total registered users
    Users,
    /// Signups per day over a given window
    Signups {
        #[arg(long, default_value_t = 30)]
        days: u64,
    },
}

#[derive(Tabled)]
struct SignupsRow {
    day: String,
    count: i64,
}

/// Entry point called from `main.rs`, routes to the matching handler below.
pub async fn handle(ctx: &CliContext, args: StatsArgs) -> anyhow::Result<()> {
    match args.command {
        StatsCommand::Users => users_count(&ctx.db).await,
        StatsCommand::Signups { days } => signups(&ctx.db, days).await,
    }
}

/// Prints the total number of registered user accounts.
async fn users_count(db: &DatabaseConnection) -> anyhow::Result<()> {
    let count = stats_service::users_count(db).await?;
    println!("Total registered users: {}", count);
    Ok(())
}

/// Prints a day-by-day signup count over the last `days` days.
async fn signups(db: &DatabaseConnection, days: u64) -> anyhow::Result<()> {
    let signups = stats_service::signups_by_day(db, days as u32).await?;

    let rows = signups.into_iter().map(|signup| SignupsRow {
        day: signup.day.to_string(),
        count: signup.count,
    });

    println!("{}", Table::new(rows));

    Ok(())
}

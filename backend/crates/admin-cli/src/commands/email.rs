//! `pwnforge-admin email ...` - manage the blocked email domains list
//! (disposable domains seeded from `backend/data/disposable_email_blocklist.conf`,
//! plus manually added domains).

use crate::CliContext;

use clap::{Args, Subcommand};
use domain::entities::{allowed_emails_domains, blocked_emails_domains};
use services::blocked_email_service::{self, BlockedEmailError};
use tabled::{Table, Tabled};

#[derive(Args)]
pub struct EmailArgs {
    #[command(subcommand)]
    command: EmailCommand,
}

#[derive(Tabled)]
struct BlockedDomainRow {
    domain: String,
    source: String,
    reason: String,
}

impl From<blocked_emails_domains::Model> for BlockedDomainRow {
    fn from(row: blocked_emails_domains::Model) -> Self {
        BlockedDomainRow {
            domain: row.domain,
            source: row.source,
            reason: row.reason.unwrap_or_else(|| "-".into()),
        }
    }
}

#[derive(Tabled)]
struct AllowedDomainRow {
    domain: String,
    added_at: String,
}

impl From<allowed_emails_domains::Model> for AllowedDomainRow {
    fn from(row: allowed_emails_domains::Model) -> Self {
        AllowedDomainRow {
            domain: row.domain,
            added_at: row.added_at.to_string(),
        }
    }
}

#[derive(Subcommand)]
enum EmailCommand {
    /// Manually block a domain
    BlockDomain {
        domain: String,
        #[arg(long)]
        reason: String,
    },
    /// Unblock a manually-blocked domain (refused for disposable-list entries)
    UnblockDomain {
        domain: String,
    },
    /// List all currently blocked domains
    ListBlocked,
    AllowDomain {
        domain: String,
        #[arg(long)]
        reason: String,
    },
    DisallowDomain {
        domain: String,
    },
    ListAllowed,
    /// Re-seed the disposable-list entries from the static file
    RefreshDisposable,
}

/// Entry point called from `main.rs`, routes to the matching handler below.
pub async fn handle(ctx: &CliContext, args: EmailArgs) -> anyhow::Result<()> {
    match args.command {
        EmailCommand::BlockDomain { domain, reason } => block_domain(ctx, domain, reason).await,
        EmailCommand::UnblockDomain { domain } => unblock_domain(ctx, domain).await,
        EmailCommand::ListBlocked => list_blocked(ctx).await,
        EmailCommand::RefreshDisposable => refresh_disposable(ctx).await,
        EmailCommand::AllowDomain { domain, reason } => allow_domain(ctx, domain, reason).await,
        EmailCommand::DisallowDomain { domain } => disallow_domain(ctx, domain).await,
        EmailCommand::ListAllowed => list_allowed(ctx).await,
    }
}

fn report(err: BlockedEmailError) -> anyhow::Error {
    match err {
        BlockedEmailError::NotFound => {
            eprintln!("Error: domain not found in the blocked list.");
            std::process::exit(1);
        }
        BlockedEmailError::DisposableListEntry => {
            eprintln!(
                "Error: this domain comes from the disposable list, use 'refresh-disposable' instead."
            );
            std::process::exit(1);
        }
        other => other.into(),
    }
}

/// Adds a domain to the blocked list with `source = "manual"`.
async fn block_domain(ctx: &CliContext, domain: String, reason: String) -> anyhow::Result<()> {
    blocked_email_service::block_domain(&ctx.db, &domain, &reason, ctx.actor_id)
        .await
        .map_err(report)?;

    println!("Domain {domain} blocked. Reason: {reason}");

    Ok(())
}

/// Removes a manually-blocked domain. Refuses if the entry's source is "disposable-list".
async fn unblock_domain(ctx: &CliContext, domain: String) -> anyhow::Result<()> {
    blocked_email_service::unblock_domain(&ctx.db, &domain)
        .await
        .map_err(report)?;

    println!("Domain {domain} unblocked.");

    Ok(())
}

/// Lists every blocked domain with its source (disposable-list vs manual) and reason.
async fn list_blocked(ctx: &CliContext) -> anyhow::Result<()> {
    let rows: Vec<BlockedDomainRow> = blocked_email_service::list_blocked(&ctx.db)
        .await?
        .into_iter()
        .map(BlockedDomainRow::from)
        .collect();

    println!("{}", Table::new(rows));
    Ok(())
}

/// Adds a domain to the allowed list, which takes precedence over the blocked list.
async fn allow_domain(ctx: &CliContext, domain: String, reason: String) -> anyhow::Result<()> {
    blocked_email_service::allow_domain(&ctx.db, &domain, &reason, ctx.actor_id).await?;
    println!("Domain {domain} allowed.");
    Ok(())
}

/// Removes a domain from the allowed list.
async fn disallow_domain(ctx: &CliContext, domain: String) -> anyhow::Result<()> {
    blocked_email_service::disallow_domain(&ctx.db, &domain).await?;
    println!("Domain {domain} disallowed.");
    Ok(())
}

/// Lists every allowed domain, most recently added first.
async fn list_allowed(ctx: &CliContext) -> anyhow::Result<()> {
    let rows: Vec<AllowedDomainRow> = blocked_email_service::list_allowed(&ctx.db)
        .await?
        .into_iter()
        .map(AllowedDomainRow::from)
        .collect();

    println!("{}", Table::new(rows));
    Ok(())
}

/// Re-seeds disposable-list entries from `backend/data/disposable_email_blocklist.conf`.
async fn refresh_disposable(ctx: &CliContext) -> anyhow::Result<()> {
    let summary =
        blocked_email_service::sync_disposable_domains(&ctx.db, seed::DISPOSABLE_DOMAINS).await?;

    if summary == 0 {
        println!(
            "Disposable list is already up-to-date. If you think this is wrong, run ops/scripts/refresh-disposable-domains.sh to update the static list and then re-run this command."
        );
    } else {
        println!("Disposable list synced: {summary} entries added.");
    }

    Ok(())
}

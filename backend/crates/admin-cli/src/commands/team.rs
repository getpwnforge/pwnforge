//! `pwnforge-admin team ...` - team administration.
//!
//! `transfer` hands a team's Captain role to one of its members through the
//! same service function as `POST /api/v1/teams/{slug}/transfer`, with the
//! same refusals. The operator needs no seat in the team: the check that the
//! caller is the Captain is the one thing the API does and this does not.
//! Unlike a Captain's transfer, this one is audited, as every administrator
//! action is.

use crate::CliContext;
use crate::lookup::resolve_user;

use clap::{Args, Subcommand};
use services::audit_service::AuditContext;
use services::team_service::{self, TeamError, TransferBy};

#[derive(Args)]
pub struct TeamArgs {
    #[command(subcommand)]
    command: TeamCommand,
}

#[derive(Subcommand)]
enum TeamCommand {
    /// Make a member the team's Captain; the current Captain becomes an Admin
    Transfer {
        /// The team's slug
        slug: String,
        /// The member who becomes Captain, by username (a user id or an
        /// email address works too)
        user: String,
        #[arg(long)]
        reason: Option<String>,
    },
}

/// Every field `None` but the actor marks the action as coming from the CLI
/// rather than an HTTP request, per `AuditContext`'s own doc comment.
fn cli_ctx(ctx: &CliContext) -> AuditContext {
    AuditContext {
        actor_id: ctx.actor_id,
        ip_address: None,
        user_agent: None,
    }
}

/// Clean message + non-zero exit for the refusals an operator can act on;
/// anything else (Db, ...) bubbles up through `?` as-is.
fn report(err: TeamError) -> anyhow::Error {
    let message = match err {
        TeamError::TeamNotFound => "no such team.",
        TeamError::MemberNotFound => "that account is not a member of the team.",
        TeamError::PersonalTeamNotTransferable => "a personal team cannot be transferred.",
        TeamError::TargetAlreadyCaptain => "that member is already the Captain.",
        TeamError::TargetPendingDeletion => {
            "that account is scheduled for deletion and cannot become Captain."
        }
        other => return other.into(),
    };

    eprintln!("Error: {message}");
    std::process::exit(1);
}

pub async fn handle(ctx: &CliContext, args: TeamArgs) -> anyhow::Result<()> {
    match args.command {
        TeamCommand::Transfer { slug, user, reason } => transfer(ctx, slug, user, reason).await,
    }
}

async fn transfer(
    ctx: &CliContext,
    slug: String,
    identifier: String,
    reason: Option<String>,
) -> anyhow::Result<()> {
    let team = team_service::get_team_by_slug(&ctx.db, &slug)
        .await
        .map_err(report)?;
    let target = resolve_user(&ctx.db, &identifier).await?;

    let audit = cli_ctx(ctx);
    team_service::transfer_captaincy(
        &ctx.db,
        team.id,
        target,
        TransferBy::InstanceAdmin {
            ctx: &audit,
            reason: reason.as_deref(),
        },
    )
    .await
    .map_err(report)?;

    println!(
        "{identifier} is now the Captain of {slug}. The previous Captain, if any, is an Admin."
    );

    Ok(())
}

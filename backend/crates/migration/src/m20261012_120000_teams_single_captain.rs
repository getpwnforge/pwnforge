//! One Captain per team, held by the database, in two rules:
//!
//! - a unique index on `team_members (team_id)`, partial on the rows whose
//!   role is `captain`, so a team never has two;
//! - `team_invitations.default_role` no longer accepts `captain`, so no
//!   invitation can make a second one by being accepted.
//!
//! The service layer only ever hands the role over (the old Captain becomes
//! an Admin in the same transaction) and refuses both cases with a readable
//! code first, but a check in code is a race and a forgotten path.
//!
//! No normalisation. A team with two Captains, or an invitation that grants
//! the role, has no honest answer (which Captain stays? what should the
//! invitation grant instead?), so the migration fails in its own
//! transaction, counting the rows and naming the first ones, and nothing
//! changes. `down()` puts the previous check back exactly as it was and
//! drops the index.

use sea_orm_migration::prelude::*;

#[derive(DeriveIden)]
enum TeamMembers {
    Table,
    TeamId,
    UserId,
    Role,
}

#[derive(DeriveIden)]
enum TeamInvitations {
    Table,
    Id,
    DefaultRole,
}

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20261012_120000_teams_single_captain"
    }
}

const IDX_ONE_CAPTAIN: &str = "idx_team_members_one_captain";

/// The name Postgres gave the column check in `m20260911_141751_teams`,
/// kept so both definitions live under it.
const CK_DEFAULT_ROLE: &str = "team_invitations_default_role_check";

/// The roles an invitation may grant from now on.
const INVITABLE_ROLES: [&str; 3] = ["admin", "member", "viewer"];

/// The roles `m20260911_141751_teams` allowed, in its order, so that
/// `down()` restores the same definition.
const PREVIOUS_ROLES: [&str; 4] = ["captain", "admin", "member", "viewer"];

/// The first ids `query` returns, cast to text, all of them counted.
async fn offending_ids(
    manager: &SchemaManager<'_>,
    query: SelectStatement,
) -> Result<Vec<String>, DbErr> {
    manager
        .get_connection()
        .query_all_raw(manager.get_database_backend().build(&query))
        .await?
        .into_iter()
        .map(|row| row.try_get_by_index::<String>(0))
        .collect()
}

fn refusal(count: usize, what: &str, ids: &[String], fix: &str) -> DbErr {
    let shown: Vec<&str> = ids.iter().take(5).map(String::as_str).collect();
    DbErr::Migration(format!(
        "{count} {what}. First ids: {shown}. There is no answer to apply for them: \
         {fix} by hand, then run the migration again.",
        shown = shown.join(", "),
    ))
}

/// Drops the default-role check and adds it back over `roles`, under the
/// same name. The expression is rendered by sea-query, which quotes the
/// values; the statement around it carries only constant names, as in
/// `m20261012_090000_workspaces_exclusive_ownership`.
async fn replace_default_role_check(
    manager: &SchemaManager<'_>,
    roles: &[&str],
) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(TeamInvitations::Table)
                .drop_constraint(CK_DEFAULT_ROLE)
                .to_owned(),
        )
        .await?;

    let mut condition = String::new();
    PostgresQueryBuilder.prepare_expr(
        &Expr::col(TeamInvitations::DefaultRole).is_in(roles.iter().copied()),
        &mut condition,
    );

    manager
        .get_connection()
        .execute_unprepared(&format!(
            r#"ALTER TABLE "team_invitations" ADD CONSTRAINT "{CK_DEFAULT_ROLE}" CHECK ({condition})"#
        ))
        .await?;

    Ok(())
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Both looked for first so the failure names the rows: Postgres
        // alone would only report the first one it met.
        let duplicated = offending_ids(
            manager,
            Query::select()
                .expr(Expr::col(TeamMembers::TeamId).cast_as("text"))
                .from(TeamMembers::Table)
                .and_where(Expr::col(TeamMembers::Role).eq("captain"))
                .group_by_col(TeamMembers::TeamId)
                .and_having(Expr::expr(Func::count(Expr::col(TeamMembers::UserId))).gt(1))
                .order_by(TeamMembers::TeamId, Order::Asc)
                .to_owned(),
        )
        .await?;

        if !duplicated.is_empty() {
            return Err(refusal(
                duplicated.len(),
                "team(s) have more than one Captain, and this migration allows one per team",
                &duplicated,
                "keep one Captain in each",
            ));
        }

        // Expired ones included: the check applies to every row.
        let granting = offending_ids(
            manager,
            Query::select()
                .expr(Expr::col(TeamInvitations::Id).cast_as("text"))
                .from(TeamInvitations::Table)
                .and_where(Expr::col(TeamInvitations::DefaultRole).eq("captain"))
                .order_by(TeamInvitations::Id, Order::Asc)
                .to_owned(),
        )
        .await?;

        if !granting.is_empty() {
            return Err(refusal(
                granting.len(),
                "invitation(s) grant the Captain role, which this migration forbids",
                &granting,
                "revoke them or change the role they grant",
            ));
        }

        manager
            .create_index(
                Index::create()
                    .name(IDX_ONE_CAPTAIN)
                    .table(TeamMembers::Table)
                    .col(TeamMembers::TeamId)
                    .and_where(Expr::col(TeamMembers::Role).eq("captain"))
                    .unique()
                    .to_owned(),
            )
            .await?;

        replace_default_role_check(manager, &INVITABLE_ROLES).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        replace_default_role_check(manager, &PREVIOUS_ROLES).await?;

        manager
            .drop_index(
                Index::drop()
                    .name(IDX_ONE_CAPTAIN)
                    .table(TeamMembers::Table)
                    .to_owned(),
            )
            .await
    }
}

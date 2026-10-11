//! Makes `ck_workspaces_ownership` exclusive: a team workspace names a team and
//! no owner, a solo workspace names an owner and no team.
//!
//! The check it replaces only required the column `mode` points at, so a team
//! workspace could also carry an `owner_id`. Nothing writes one, but the
//! schema allowed it, and an account's solo workspaces have to be exactly
//! those with `mode = 'solo'` and its `owner_id`: a team workspace with an
//! owner would sit under `fk_workspaces_owner_id` with no rule saying whose
//! it is.
//!
//! No normalisation. A row that breaks the rule has no honest answer (drop
//! the owner, or the team?), so the migration fails in its own transaction,
//! naming the rows, and nothing changes. `down()` puts the previous check
//! back exactly as it was.

use sea_orm_migration::prelude::*;

#[derive(DeriveIden)]
enum Workspaces {
    Table,
    Id,
    Mode,
    TeamId,
    OwnerId,
}

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20261012_090000_workspaces_exclusive_ownership"
    }
}

/// The existing name, kept: it already follows `ck_workspaces_*`.
const CK_OWNERSHIP: &str = "ck_workspaces_ownership";

/// Each mode names its own owner, and only that one.
fn exclusive() -> Expr {
    Expr::col(Workspaces::Mode)
        .eq("team")
        .and(Expr::col(Workspaces::TeamId).is_not_null())
        .and(Expr::col(Workspaces::OwnerId).is_null())
        .or(Expr::col(Workspaces::Mode)
            .eq("solo")
            .and(Expr::col(Workspaces::OwnerId).is_not_null())
            .and(Expr::col(Workspaces::TeamId).is_null()))
}

/// The check this migration replaces, built the way
/// `m20260917_163621_workspaces` built it, so that `down()` restores the same
/// definition.
fn previous() -> Expr {
    Expr::col(Workspaces::TeamId)
        .is_not_null()
        .and(Expr::col(Workspaces::Mode).eq("team"))
        .or(Expr::col(Workspaces::OwnerId)
            .is_not_null()
            .and(Expr::col(Workspaces::Mode).eq("solo")))
}

/// Drops the check and adds `check` under the same name. The expression is
/// rendered by sea-query, which quotes the values; the statement around it
/// carries only constant names, as in `m20261008_090000_workspaces_enum_checks`.
async fn replace_check(manager: &SchemaManager<'_>, check: Expr) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(Workspaces::Table)
                .drop_constraint(CK_OWNERSHIP)
                .to_owned(),
        )
        .await?;

    let mut condition = String::new();
    PostgresQueryBuilder.prepare_expr(&check, &mut condition);

    manager
        .get_connection()
        .execute_unprepared(&format!(
            r#"ALTER TABLE "workspaces" ADD CONSTRAINT "{CK_OWNERSHIP}" CHECK ({condition})"#
        ))
        .await?;

    Ok(())
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Looked for first so the failure names the rows: Postgres alone would
        // only say that "some row" violates the check.
        let breaking = Query::select()
            .expr(Expr::col(Workspaces::Id).cast_as("text"))
            .from(Workspaces::Table)
            .and_where(exclusive().not())
            .order_by(Workspaces::Id, Order::Asc)
            .to_owned();

        let ids: Vec<String> = manager
            .get_connection()
            .query_all_raw(manager.get_database_backend().build(&breaking))
            .await?
            .into_iter()
            .map(|row| row.try_get_by_index::<String>(0))
            .collect::<Result<_, _>>()?;

        if !ids.is_empty() {
            let shown: Vec<&str> = ids.iter().take(5).map(String::as_str).collect();
            return Err(DbErr::Migration(format!(
                "{count} workspace(s) break the ownership rule this migration enforces: \
                 a team workspace names a team and no owner, a solo workspace an owner \
                 and no team. First ids: {shown}. There is no answer to apply for them: \
                 decide each one by hand, then run the migration again.",
                count = ids.len(),
                shown = shown.join(", "),
            )));
        }

        replace_check(manager, exclusive()).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        replace_check(manager, previous()).await
    }
}

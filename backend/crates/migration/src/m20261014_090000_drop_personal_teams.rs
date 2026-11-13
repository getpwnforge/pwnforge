//! Drops the personal team. A solo workspace belongs to its owner through
//! `owner_id` and never needed a team; the team every account got on
//! verifying an address served nothing, and nothing creates one any more.
//!
//! The rows go in this order, in the migration's transaction:
//!
//! - a workspace of a personal team whose creator still exists becomes a
//!   solo workspace of that creator. A solo workspace has neither members nor
//!   invitations, so its `workspace_members` rows and its invitations go, and
//!   so do the favourites of anyone but the creator, who alone can still see
//!   it;
//! - a workspace of a personal team whose creator is gone (the account was
//!   deleted) is deleted: it is a deleted account's data;
//! - the personal teams are deleted, their members and invitations with them
//!   by cascade, then the `personal` column and its index.
//!
//! It prints what it converted and deleted. It cannot reach the object store,
//! so the avatars of the deleted teams and workspaces stay in the bucket; the
//! count printed says how many. A converted workspace keeps its id and so its
//! avatar.
//!
//! `down()` restores the schema only, the column and its index. Not the
//! data: the personal teams, their members and the deleted workspaces are
//! gone, and a converted workspace stays solo.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::Statement;

#[derive(DeriveIden)]
enum Teams {
    Table,
    Personal,
    CreatedBy,
}

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20261014_090000_drop_personal_teams"
    }
}

const IDX_ONE_PERSONAL: &str = "idx_teams_one_personal_per_creator";

/// The workspaces of a personal team whose creator exists: the ones that
/// become solo.
const KEPT: &str = "SELECT w.id FROM workspaces w JOIN teams t ON t.id = w.team_id \
                    WHERE t.personal AND t.created_by IS NOT NULL";

/// Runs a constant statement and returns how many rows it touched.
async fn run(manager: &SchemaManager<'_>, sql: &str) -> Result<u64, DbErr> {
    Ok(manager
        .get_connection()
        .execute_unprepared(sql)
        .await?
        .rows_affected())
}

/// Runs a constant `SELECT count(*)` and returns the count.
async fn count(manager: &SchemaManager<'_>, sql: &str) -> Result<i64, DbErr> {
    let row = manager
        .get_connection()
        .query_one_raw(Statement::from_string(
            manager.get_database_backend(),
            sql.to_owned(),
        ))
        .await?
        .ok_or_else(|| DbErr::Migration("a count returned no row".to_owned()))?;

    row.try_get_by_index::<i64>(0)
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Counted before anything goes: once the rows are deleted, nothing
        // says which objects in the bucket were theirs.
        let orphaned_avatars = count(
            manager,
            "SELECT (SELECT count(*) FROM teams WHERE personal AND avatar_version IS NOT NULL) \
             + (SELECT count(*) FROM workspaces w JOIN teams t ON t.id = w.team_id \
                WHERE t.personal AND t.created_by IS NULL AND w.avatar_version IS NOT NULL)",
        )
        .await?;

        // While the workspaces still name their team, which says who the
        // creator is.
        let members = run(
            manager,
            &format!("DELETE FROM workspace_members WHERE workspace_id IN ({KEPT})"),
        )
        .await?;
        let invitations = run(
            manager,
            &format!("DELETE FROM workspace_invitations WHERE workspace_id IN ({KEPT})"),
        )
        .await?;
        let favorites = run(
            manager,
            "DELETE FROM workspace_favorites f USING workspaces w, teams t \
             WHERE w.id = f.workspace_id AND t.id = w.team_id \
             AND t.personal AND t.created_by IS NOT NULL AND f.user_id <> t.created_by",
        )
        .await?;

        let converted = run(
            manager,
            "UPDATE workspaces w SET mode = 'solo', owner_id = t.created_by, team_id = NULL, \
             updated_at = now() \
             FROM teams t WHERE t.id = w.team_id AND t.personal AND t.created_by IS NOT NULL",
        )
        .await?;

        let deleted_workspaces = run(
            manager,
            "DELETE FROM workspaces w USING teams t \
             WHERE t.id = w.team_id AND t.personal AND t.created_by IS NULL",
        )
        .await?;

        let deleted_teams = run(manager, "DELETE FROM teams WHERE personal").await?;

        manager
            .drop_index(
                Index::drop()
                    .name(IDX_ONE_PERSONAL)
                    .table(Teams::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Teams::Table)
                    .drop_column(Teams::Personal)
                    .to_owned(),
            )
            .await?;

        println!(
            "{name}: {converted} workspace(s) of a personal team made solo (dropping \
             {members} member row(s), {invitations} invitation(s) and {favorites} \
             favourite(s) of others than the owner), {deleted_workspaces} workspace(s) \
             of a deleted account removed, {deleted_teams} personal team(s) removed. \
             {orphaned_avatars} avatar(s) of the removed teams and workspaces stay \
             in the bucket: the migration cannot reach it.",
            name = self.name(),
        );

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // The schema only, as in `m20260911_141751_teams`: the rows this
        // migration removed are not coming back.
        manager
            .alter_table(
                Table::alter()
                    .table(Teams::Table)
                    .add_column(
                        ColumnDef::new(Teams::Personal)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name(IDX_ONE_PERSONAL)
                    .table(Teams::Table)
                    .col(Teams::CreatedBy)
                    .and_where(Expr::col(Teams::Personal))
                    .unique()
                    .to_owned(),
            )
            .await
    }
}

//! Backfills the Captain of every personal team that has none.
//!
//! `create_personal_team` used to create the team without inserting its
//! creator as a member, so every personal team made before the fix is empty:
//! invisible to the person it was made for, and with no Captain to manage it.
//! This inserts `created_by` as Captain for each personal team that has a
//! creator and no member at all. A team whose creator was deleted
//! (`created_by` NULL) has no one to backfill, and a team that already has any
//! member is left exactly as it is.

use sea_orm_migration::prelude::*;

#[derive(DeriveIden)]
enum Teams {
    Table,
    Id,
    Personal,
    CreatedBy,
}

#[derive(DeriveIden)]
enum TeamMembers {
    Table,
    TeamId,
    UserId,
    Role,
}

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // INSERT INTO team_members (team_id, user_id, role)
        // SELECT id, created_by, 'captain' FROM teams
        // WHERE personal AND created_by IS NOT NULL
        //   AND NOT EXISTS (SELECT 1 FROM team_members WHERE team_id = teams.id)
        //
        // `joined_at` takes its column default. Built with sea-query, so the
        // only literal, the role, is bound rather than spliced into the text.
        let has_member = Query::select()
            .expr(Expr::val(1))
            .from(TeamMembers::Table)
            .and_where(
                Expr::col((TeamMembers::Table, TeamMembers::TeamId))
                    .equals((Teams::Table, Teams::Id)),
            )
            .to_owned();

        let teams_without_member = Query::select()
            .column((Teams::Table, Teams::Id))
            .column((Teams::Table, Teams::CreatedBy))
            .expr(Expr::val("captain"))
            .from(Teams::Table)
            .and_where(Expr::col((Teams::Table, Teams::Personal)).eq(true))
            .and_where(Expr::col((Teams::Table, Teams::CreatedBy)).is_not_null())
            .and_where(Expr::exists(has_member).not())
            .to_owned();

        let backfill = Query::insert()
            .into_table(TeamMembers::Table)
            .columns([TeamMembers::TeamId, TeamMembers::UserId, TeamMembers::Role])
            .select_from(teams_without_member)
            .map_err(|error| DbErr::Migration(error.to_string()))?
            .to_owned();

        manager.exec_stmt(backfill).await
    }

    /// Deliberately removes nothing. Once this has run, a backfilled Captain
    /// row cannot be told apart from one `create_personal_team` inserted
    /// later, or from one a user created by changing roles: they carry the
    /// same team, user and role. Deleting by that shape would strip legitimate
    /// Captains, and the rows this added are correct data the application now
    /// creates itself, so leaving them in place is the safe rollback.
    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}

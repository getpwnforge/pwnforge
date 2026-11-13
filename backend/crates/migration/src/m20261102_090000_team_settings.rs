//! Two team settings: `teams.description`, free text of at most 500
//! characters, and `teams.members_can_create_workspaces`, which lets a Member
//! create workspaces under the team (see `workspace_service::create_workspace`).
//!
//! Both land on a table that already holds rows: a nullable column, and a
//! boolean with a constant default, which Postgres adds without rewriting the
//! table. Existing teams have no description and keep the rule they had,
//! Admin and above.

use sea_orm_migration::prelude::*;

#[derive(DeriveIden)]
enum Teams {
    Table,
    Description,
    MembersCanCreateWorkspaces,
}

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20261102_090000_team_settings"
    }
}

const CK_DESCRIPTION: &str = "ck_teams_description_length";

/// Frozen here rather than imported, as the palettes in the earlier
/// migrations. Mirrors the bound on `TeamPatch::description`: the validator
/// gives the caller a usable 422, this refuses every other writer. Both count
/// characters, not bytes.
const DESCRIPTION_MAX_CHARS: i32 = 500;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Teams::Table)
                    .add_column(
                        ColumnDef::new(Teams::Description)
                            .text()
                            .null()
                            .check(Check::named(
                                CK_DESCRIPTION,
                                Expr::expr(Func::char_length(Expr::col(Teams::Description)))
                                    .lte(DESCRIPTION_MAX_CHARS),
                            )),
                    )
                    .add_column(
                        ColumnDef::new(Teams::MembersCanCreateWorkspaces)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await
    }

    /// Drops both columns, the check with its column. The descriptions and
    /// the setting are lost; the workspaces a Member created under it stay,
    /// as they do when the setting is turned off.
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Teams::Table)
                    .drop_column(Teams::MembersCanCreateWorkspaces)
                    .drop_column(Teams::Description)
                    .to_owned(),
            )
            .await
    }
}

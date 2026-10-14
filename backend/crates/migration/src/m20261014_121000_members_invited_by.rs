//! Who brought each member in: `team_members.invited_by` and
//! `workspace_members.invited_by`, the author of the invitation they
//! accepted. Filled at acceptance from now on; rows that exist stay NULL,
//! nothing records who invited them.
//!
//! SET NULL on the author's deletion: the membership stands, only its
//! attribution goes, as for the invitations themselves.

use sea_orm_migration::prelude::*;

#[derive(DeriveIden)]
enum TeamMembers {
    Table,
    InvitedBy,
}

#[derive(DeriveIden)]
enum WorkspaceMembers {
    Table,
    InvitedBy,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20261014_121000_members_invited_by"
    }
}

const FK_TEAM: &str = "fk_team_members_invited_by";
const FK_WORKSPACE: &str = "fk_workspace_members_invited_by";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(TeamMembers::Table)
                    .add_column(ColumnDef::new(TeamMembers::InvitedBy).uuid().null())
                    .add_foreign_key(
                        TableForeignKey::new()
                            .name(FK_TEAM)
                            .from_tbl(TeamMembers::Table)
                            .from_col(TeamMembers::InvitedBy)
                            .to_tbl(Users::Table)
                            .to_col(Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(WorkspaceMembers::Table)
                    .add_column(ColumnDef::new(WorkspaceMembers::InvitedBy).uuid().null())
                    .add_foreign_key(
                        TableForeignKey::new()
                            .name(FK_WORKSPACE)
                            .from_tbl(WorkspaceMembers::Table)
                            .from_col(WorkspaceMembers::InvitedBy)
                            .to_tbl(Users::Table)
                            .to_col(Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(WorkspaceMembers::Table)
                    .drop_foreign_key(FK_WORKSPACE)
                    .drop_column(WorkspaceMembers::InvitedBy)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(TeamMembers::Table)
                    .drop_foreign_key(FK_TEAM)
                    .drop_column(TeamMembers::InvitedBy)
                    .to_owned(),
            )
            .await
    }
}

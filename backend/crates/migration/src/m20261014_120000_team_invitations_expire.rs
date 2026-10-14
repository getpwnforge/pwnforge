//! Every team invitation expires. The author still picks the date, the
//! service defaults it to seven days and refuses more than thirty, and this
//! gives a date to the rows that had none.
//!
//! A row without one gets seven days from its creation, or seven days from
//! now when that date is already past: an invitation its author meant never
//! to expire must not die at the upgrade, under a link someone may be about
//! to follow. The column then becomes NOT NULL.
//!
//! `down()` only makes the column nullable again: the dates written here are
//! not taken back, since nothing records which rows had none.

use sea_orm_migration::prelude::*;

#[derive(DeriveIden)]
enum TeamInvitations {
    Table,
    ExpiresAt,
}

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20261014_120000_team_invitations_expire"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "UPDATE team_invitations SET expires_at = CASE \
                 WHEN created_at + interval '7 days' > now() THEN created_at + interval '7 days' \
                 ELSE now() + interval '7 days' END \
                 WHERE expires_at IS NULL",
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(TeamInvitations::Table)
                    .modify_column(
                        ColumnDef::new(TeamInvitations::ExpiresAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(TeamInvitations::Table)
                    .modify_column(
                        ColumnDef::new(TeamInvitations::ExpiresAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .to_owned(),
            )
            .await
    }
}

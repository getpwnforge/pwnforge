use sea_orm_migration::prelude::*;

#[derive(DeriveIden)]
enum Users {
    Table,
    AvatarUrl,
}

#[derive(DeriveIden)]
enum Teams {
    Table,
    AvatarUrl,
}

#[derive(DeriveIden)]
enum Workspaces {
    Table,
    AvatarUrl,
}

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260922_143000_drop_avatar_url"
    }
}

/// Drops `avatar_url` from the three tables that carried it.
///
/// All three migrations that created it describe the same thing: "the URL of
/// an uploaded image". That is a row naming an object in the bucket, which is
/// the pattern `avatar_version` exists to replace - a column holding a key or
/// a URL means any write that reaches it can point the row at an arbitrary
/// object, while a version uuid can only ever name the objects belonging to
/// its own entity (see `storage_service::avatar_key`).
///
/// This is not an external-avatar column. There is no OAuth anywhere in the
/// codebase - no provider table, no identity linking - so nothing ever holds a
/// GitHub or Discord picture here. Had it been one, it would have been kept:
/// an external URL is not a key into our bucket and the two would coexist
/// under a documented precedence rule.
///
/// Nothing is lost. No code path ever wrote the column: it is read in four
/// places, all of which copy it straight into a response DTO, and the frontend
/// does not reference the field at all. Every row is NULL.
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .drop_column(Users::AvatarUrl)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Teams::Table)
                    .drop_column(Teams::AvatarUrl)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Workspaces::Table)
                    .drop_column(Workspaces::AvatarUrl)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    /// Restores the column as it was: nullable `text`, no default, no
    /// constraint.
    ///
    /// The data is not restored, because there was none - every row was NULL
    /// when `up()` ran, and a rollback therefore loses nothing. Stated here
    /// rather than assumed, since a `down()` that silently discards a populated
    /// column would be a very different thing from this one.
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Workspaces::Table)
                    .add_column(ColumnDef::new(Workspaces::AvatarUrl).text().null())
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Teams::Table)
                    .add_column(ColumnDef::new(Teams::AvatarUrl).text().null())
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .add_column(ColumnDef::new(Users::AvatarUrl).text().null())
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

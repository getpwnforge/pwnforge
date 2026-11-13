use sea_orm_migration::prelude::*;

#[derive(DeriveIden)]
enum Users {
    Table,
    AvatarVersion,
}

#[derive(DeriveIden)]
enum Teams {
    Table,
    AvatarVersion,
}

#[derive(DeriveIden)]
enum Workspaces {
    Table,
    AvatarVersion,
}

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260922_101500_avatar_version"
    }
}

/// Adds `avatar_version` to the three entities that can carry an avatar.
///
/// The column holds a uuid and nothing else. It is not a key and not a URL:
/// the object key is derived from `(kind, id, version)` by
/// `storage_service::avatar_key`, so a row can only ever name the objects
/// belonging to its own entity. Storing the key itself would mean any write
/// that reaches this column could point the row at an arbitrary object in the
/// bucket, which is a far larger surface than the cache-busting convenience it
/// would buy.
///
/// `NULL` means no avatar, which is why the column is nullable rather than
/// defaulted: there is no sensible "version zero", and a default would make
/// every existing row claim an avatar that does not exist.
///
/// Three `ALTER TABLE`s on tables that already hold rows. Each adds a nullable
/// column with no default, which Postgres does as a catalog-only change - no
/// table rewrite, no long-held `ACCESS EXCLUSIVE` lock beyond the catalog
/// update itself.
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .add_column(ColumnDef::new(Users::AvatarVersion).uuid().null())
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Teams::Table)
                    .add_column(ColumnDef::new(Teams::AvatarVersion).uuid().null())
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Workspaces::Table)
                    .add_column(ColumnDef::new(Workspaces::AvatarVersion).uuid().null())
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    /// Dropping the column loses which avatar each entity pointed at, and the
    /// objects in the bucket are not touched: `down` runs against the database
    /// and has no credential for the object store. Re-running `up` afterwards
    /// leaves every entity with no avatar while the old objects remain, which
    /// `avatar_service::remove` collects by prefix the next time an entity is
    /// cleared. That is the honest trade for a reversible migration here -
    /// the alternative would be a `down` that deletes user data from a second
    /// system, which is worse.
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Workspaces::Table)
                    .drop_column(Workspaces::AvatarVersion)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Teams::Table)
                    .drop_column(Teams::AvatarVersion)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .drop_column(Users::AvatarVersion)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

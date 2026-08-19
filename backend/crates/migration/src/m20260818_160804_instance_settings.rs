use sea_orm_migration::prelude::*;

#[derive(DeriveIden)]
enum InstanceSettings {
    Table,
    Id,
    DefaultLocale,
    DefaultTimezone,
    AllowPublicSignup,
    SetupCompletedAt,
    UpdatedAt,
}

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260818_160804_instance_settings"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(InstanceSettings::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(InstanceSettings::Id)
                            .small_integer()
                            .not_null()
                            .primary_key()
                            .default(1),
                    )
                    .col(
                        ColumnDef::new(InstanceSettings::DefaultLocale)
                            .text()
                            .not_null()
                            .default("en"),
                    )
                    .col(
                        ColumnDef::new(InstanceSettings::DefaultTimezone)
                            .text()
                            .not_null()
                            .default("UTC"),
                    )
                    .col(
                        ColumnDef::new(InstanceSettings::AllowPublicSignup)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(InstanceSettings::SetupCompletedAt)
                            .timestamp_with_time_zone()
                    )
                    .col(
                        ColumnDef::new(InstanceSettings::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .check((
                        "ck_instance_settings_singleton",
                        Expr::col(InstanceSettings::Id).eq(1),
                    ))
                    .to_owned(),
            )
            .await?;

        manager
            .exec_stmt(
                Query::insert()
                    .into_table(InstanceSettings::Table)
                    .columns([InstanceSettings::Id])
                    .values_panic([1.into()])
                    .on_conflict(OnConflict::column(InstanceSettings::Id).do_nothing().to_owned())
                    .to_owned(),
            )
            .await?;

        manager
            .get_connection()
            .execute_unprepared(
                "UPDATE instance_settings SET setup_completed_at = NOW() \
                WHERE EXISTS (SELECT 1 FROM users);",
            )
            .await?;

        Ok(())

    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(InstanceSettings::Table).to_owned())
            .await
    }
}

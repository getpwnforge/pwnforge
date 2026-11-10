use sea_orm_migration::prelude::*;

#[derive(DeriveIden)]
enum Workspaces {
    Table,
    EventUrl,
}

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20261110_215222_workspace_event_url"
    }
}

const CK_EVENT_URL: &str = "ck_workspaces_event_url";

/// Frozen here rather than imported. Mirrors `EVENT_URL_MAX_CHARS` in the
/// service; both count characters, not bytes.
const EVENT_URL_MAX_CHARS: i32 = 2048;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Workspaces::Table)
                    .add_column(
                        ColumnDef::new(Workspaces::EventUrl)
                            .text()
                            .null()
                            // The service stores the parsed form, whose scheme
                            // is always lowercase: no case-insensitive match.
                            .check(Check::named(
                                CK_EVENT_URL,
                                Expr::expr(Func::char_length(Expr::col(Workspaces::EventUrl)))
                                    .lte(EVENT_URL_MAX_CHARS)
                                    .and(Expr::cust(r#""event_url" ~ '^https?://'"#)),
                            )),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Workspaces::Table)
                    .drop_column(Workspaces::EventUrl)
                    .to_owned(),
            )
            .await
    }
}

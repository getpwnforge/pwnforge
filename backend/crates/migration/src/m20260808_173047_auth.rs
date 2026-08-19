// crates/migration/src/m20260808_173047_auth.rs
use sea_orm_migration::prelude::*;

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
    PasswordHash,
    Username,
    FirstName,
    LastName,
    Bio,
    AvatarUrl,
    Theme,
    Locale,
    IsInstanceAdmin,
    SuspendedAt,
    SuspendedUntil,
    SuspendedReason,
    SuspendedBy,
    CreatedAt,
    UpdatedAt,
    LastActivityAt,
}

#[derive(DeriveIden)]
enum UserEmails {
    Table,
    Id,
    UserId,
    Email,
    IsPrimary,
    CreatedAt,
    UpdatedAt,
    VerifiedAt,
}

#[derive(DeriveIden)]
enum AuthTokens {
    Table,
    Id,
    UserId,
    Kind,
    EmailId,
    TokenHash,
    CreatedAt,
    ExpiresAt,
    UsedAt,
}

#[derive(DeriveIden)]
enum RefreshTokens {
    Table,
    Id,
    UserId,
    TokenHash,
    ExpiresAt,
    UserAgent,
    IpAddress,
    LastUsedAt,
    CreatedAt,
    RevokedAt,
    RevokedReason,
}

#[derive(DeriveIden)]
enum InstanceAuditLogs {
    Table,
    Id,
    ActorId,
    Action,
    TargetUserId,
    TargetTeamId,
    TargetWorkspaceId,
    Reason,
    Meta,
    IpAddress,
    UserAgent,
    CreatedAt,
}

#[derive(DeriveIden)]
enum InstanceAlerts {
    Table,
    Id,
    Kind,
    Message,
    LinkUrl,
    StartsAt,
    EndsAt,
    IsActive,
    CreatedBy,
    CreatedAt,
}

#[derive(DeriveIden)]
enum BlockedEmailsDomains {
    Table,
    Domain,
    Source,
    Reason,
    AddedBy,
    AddedAt,
}

#[derive(DeriveIden)]
enum AllowedEmailsDomains {
    Table,
    Domain,
    Reason,
    AddedBy,
    AddedAt,
}

#[derive(DeriveIden)]
enum ReservedUsernames {
    Table,
    Username,
    Source,
    Reason,
    AddedBy,
    AddedAt,
}

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260808_173047_auth"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("CREATE EXTENSION IF NOT EXISTS citext;")
            .await?;

        // USERS TABLE
        manager
            .create_table(
                Table::create()
                    .table(Users::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Users::Id)
                            .uuid()
                            .not_null()
                            .default(Expr::cust("uuidv7()"))
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Users::PasswordHash).text().not_null())
                    .col(
                        ColumnDef::new(Users::Username)
                            .custom("citext")
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(Users::FirstName).text())
                    .col(ColumnDef::new(Users::LastName).text())
                    .col(ColumnDef::new(Users::Bio).text())
                    .col(ColumnDef::new(Users::AvatarUrl).text())
                    .col(
                        ColumnDef::new(Users::Theme)
                            .text()
                            .not_null()
                            .default("dark"),
                    )
                    .col(
                        ColumnDef::new(Users::Locale)
                            .text()
                            .not_null()
                            .default("en"),
                    )
                    .col(
                        ColumnDef::new(Users::IsInstanceAdmin)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(ColumnDef::new(Users::SuspendedAt).timestamp_with_time_zone())
                    .col(ColumnDef::new(Users::SuspendedUntil).timestamp_with_time_zone())
                    .col(ColumnDef::new(Users::SuspendedReason).text())
                    .col(ColumnDef::new(Users::SuspendedBy).uuid())
                    .col(
                        ColumnDef::new(Users::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(Users::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(ColumnDef::new(Users::LastActivityAt).timestamp_with_time_zone())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_users_suspended_by")
                            .from(Users::Table, Users::SuspendedBy)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .check((
                        "ck_suspension_consistency",
                        Expr::col(Users::SuspendedAt)
                            .is_null()
                            .and(Expr::col(Users::SuspendedUntil).is_null())
                            .and(Expr::col(Users::SuspendedReason).is_null())
                            .and(Expr::col(Users::SuspendedBy).is_null())
                            .or(Expr::col(Users::SuspendedAt).is_not_null()),
                    ))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_users_suspended")
                    .table(Users::Table)
                    .col(Users::Id)
                    .and_where(Expr::col(Users::SuspendedAt).is_not_null())
                    .to_owned(),
            )
            .await?;

        // USER EMAILS TABLE
        manager
            .create_table(
                Table::create()
                    .table(UserEmails::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(UserEmails::Id)
                            .uuid()
                            .not_null()
                            .default(Expr::cust("uuidv7()"))
                            .primary_key(),
                    )
                    .col(ColumnDef::new(UserEmails::UserId).uuid().not_null())
                    .col(
                        ColumnDef::new(UserEmails::Email)
                            .custom("citext")
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(UserEmails::IsPrimary)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(UserEmails::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(UserEmails::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(ColumnDef::new(UserEmails::VerifiedAt).timestamp_with_time_zone())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_user_emails_user_id")
                            .from(UserEmails::Table, UserEmails::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_unique_primary_email")
                    .table(UserEmails::Table)
                    .col(UserEmails::UserId)
                    .unique()
                    .and_where(Expr::col(UserEmails::IsPrimary).eq(true))
                    .to_owned(),
            )
            .await?;

        // AUTH TOKENS TABLE
        manager
            .create_table(
                Table::create()
                    .table(AuthTokens::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AuthTokens::Id)
                            .uuid()
                            .not_null()
                            .default(Expr::cust("uuidv7()"))
                            .primary_key(),
                    )
                    .col(ColumnDef::new(AuthTokens::UserId).uuid().not_null())
                    .col(ColumnDef::new(AuthTokens::Kind).text().not_null())
                    .col(ColumnDef::new(AuthTokens::EmailId).uuid())
                    .col(
                        ColumnDef::new(AuthTokens::TokenHash)
                            .text()
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(AuthTokens::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(AuthTokens::ExpiresAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(ColumnDef::new(AuthTokens::UsedAt).timestamp_with_time_zone())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_auth_tokens_user_id")
                            .from(AuthTokens::Table, AuthTokens::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_auth_tokens_email_id")
                            .from(AuthTokens::Table, AuthTokens::EmailId)
                            .to(UserEmails::Table, UserEmails::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_auth_tokens_user_kind")
                    .table(AuthTokens::Table)
                    .col(AuthTokens::UserId)
                    .col(AuthTokens::Kind)
                    .col((AuthTokens::CreatedAt, IndexOrder::Desc))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_auth_tokens_expires_at")
                    .table(AuthTokens::Table)
                    .col(AuthTokens::ExpiresAt)
                    .to_owned(),
            )
            .await?;

        // REFRESH TOKENS TABLE
        manager
            .create_table(
                Table::create()
                    .table(RefreshTokens::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(RefreshTokens::Id)
                            .uuid()
                            .not_null()
                            .default(Expr::cust("uuidv7()"))
                            .primary_key(),
                    )
                    .col(ColumnDef::new(RefreshTokens::UserId).uuid().not_null())
                    .col(ColumnDef::new(RefreshTokens::TokenHash).text().not_null().unique_key())
                    .col(
                        ColumnDef::new(RefreshTokens::ExpiresAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(ColumnDef::new(RefreshTokens::UserAgent).text())
                    .col(ColumnDef::new(RefreshTokens::IpAddress).inet())
                    .col(ColumnDef::new(RefreshTokens::LastUsedAt).timestamp_with_time_zone())
                    .col(
                        ColumnDef::new(RefreshTokens::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(ColumnDef::new(RefreshTokens::RevokedAt).timestamp_with_time_zone())
                    .col(ColumnDef::new(RefreshTokens::RevokedReason).text())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_refresh_tokens_user_id")
                            .from(RefreshTokens::Table, RefreshTokens::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_refresh_tokens_expires_at")
                    .table(RefreshTokens::Table)
                    .col(RefreshTokens::ExpiresAt)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_refresh_tokens_revoked_at")
                    .table(RefreshTokens::Table)
                    .col(RefreshTokens::RevokedAt)
                    .and_where(Expr::col(RefreshTokens::RevokedAt).is_not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_refresh_tokens_user_id")
                    .table(RefreshTokens::Table)
                    .col(RefreshTokens::UserId)
                    .to_owned(),
            )
            .await?;

        // INSTANCE AUDIT LOGS TABLE
        manager
            .create_table(
                Table::create()
                    .table(InstanceAuditLogs::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(InstanceAuditLogs::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(InstanceAuditLogs::ActorId).uuid())
                    .col(ColumnDef::new(InstanceAuditLogs::Action).text().not_null())
                    .col(ColumnDef::new(InstanceAuditLogs::TargetUserId).uuid())
                    .col(ColumnDef::new(InstanceAuditLogs::TargetTeamId).uuid())
                    .col(ColumnDef::new(InstanceAuditLogs::TargetWorkspaceId).uuid())
                    .col(ColumnDef::new(InstanceAuditLogs::Reason).text())
                    .col(
                        ColumnDef::new(InstanceAuditLogs::Meta)
                            .json_binary()
                            .not_null()
                            .default(Expr::cust("'{}'::jsonb")),
                    )
                    .col(ColumnDef::new(InstanceAuditLogs::IpAddress).inet())
                    .col(ColumnDef::new(InstanceAuditLogs::UserAgent).text())
                    .col(
                        ColumnDef::new(InstanceAuditLogs::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_instance_audit_logs_actor_id")
                            .from(InstanceAuditLogs::Table, InstanceAuditLogs::ActorId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_instance_audit_logs_target_user_id")
                            .from(InstanceAuditLogs::Table, InstanceAuditLogs::TargetUserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    // NOTE: The following FK are not used while the Teams and Workspaces tables are not created in this migration. They will be added in a future migration when those tables are created (with an ALTER TABLE statement).
                    // .foreign_key(
                    //     ForeignKey::create()
                    //         .name("fk_instance_audit_logs_target_team_id")
                    //         .from(InstanceAuditLogs::Table, InstanceAuditLogs::TargetTeamId)
                    //         .to(Teams::Table, Teams::Id)
                    //         .on_delete(ForeignKeyAction::SetNull),
                    // )
                    // .foreign_key(
                    //     ForeignKey::create()
                    //         .name("fk_instance_audit_logs_target_workspace_id")
                    //         .from(InstanceAuditLogs::Table, InstanceAuditLogs::TargetWorkspaceId)
                    //         .to(Workspaces::Table, Workspaces::Id)
                    //         .on_delete(ForeignKeyAction::SetNull),
                    // )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_instance_audit_actor")
                    .table(InstanceAuditLogs::Table)
                    .col(InstanceAuditLogs::ActorId)
                    .col((InstanceAuditLogs::CreatedAt, IndexOrder::Desc))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_instance_audit_target_user")
                    .table(InstanceAuditLogs::Table)
                    .col(InstanceAuditLogs::TargetUserId)
                    .col((InstanceAuditLogs::CreatedAt, IndexOrder::Desc))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_instance_audit_action")
                    .table(InstanceAuditLogs::Table)
                    .col(InstanceAuditLogs::Action)
                    .col((InstanceAuditLogs::CreatedAt, IndexOrder::Desc))
                    .to_owned(),
            )
            .await?;

        // INSTANCE ALERTS TABLE
        manager
            .create_table(
                Table::create()
                    .table(InstanceAlerts::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(InstanceAlerts::Id)
                            .uuid()
                            .not_null()
                            .default(Expr::cust("uuidv7()"))
                            .primary_key(),
                    )
                    .col(ColumnDef::new(InstanceAlerts::Kind).text().not_null())
                    .col(
                        ColumnDef::new(InstanceAlerts::Message)
                            .json_binary()
                            .not_null(),
                    )
                    .col(ColumnDef::new(InstanceAlerts::LinkUrl).text())
                    .col(
                        ColumnDef::new(InstanceAlerts::StartsAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(ColumnDef::new(InstanceAlerts::EndsAt).timestamp_with_time_zone())
                    .col(
                        ColumnDef::new(InstanceAlerts::IsActive)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(ColumnDef::new(InstanceAlerts::CreatedBy).uuid())
                    .col(
                        ColumnDef::new(InstanceAlerts::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_instance_alerts_created_by")
                            .from(InstanceAlerts::Table, InstanceAlerts::CreatedBy)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_instance_alerts_active")
                    .table(InstanceAlerts::Table)
                    .col(InstanceAlerts::IsActive)
                    .col(InstanceAlerts::StartsAt)
                    .col(InstanceAlerts::EndsAt)
                    .to_owned(),
            )
            .await?;

        // EMAILS DOMAINS TABLES
        manager
            .create_table(
                Table::create()
                    .table(BlockedEmailsDomains::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(BlockedEmailsDomains::Domain)
                            .custom("citext")
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(BlockedEmailsDomains::Source)
                            .text()
                            .not_null(),
                    )
                    .col(ColumnDef::new(BlockedEmailsDomains::Reason).text())
                    .col(ColumnDef::new(BlockedEmailsDomains::AddedBy).uuid())
                    .col(
                        ColumnDef::new(BlockedEmailsDomains::AddedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_blocked_emails_domains_added_by")
                            .from(BlockedEmailsDomains::Table, BlockedEmailsDomains::AddedBy)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(AllowedEmailsDomains::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AllowedEmailsDomains::Domain)
                            .custom("citext")
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(AllowedEmailsDomains::Reason).text())
                    .col(ColumnDef::new(AllowedEmailsDomains::AddedBy).uuid())
                    .col(
                        ColumnDef::new(AllowedEmailsDomains::AddedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_allowed_emails_domains_added_by")
                            .from(AllowedEmailsDomains::Table, AllowedEmailsDomains::AddedBy)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(ReservedUsernames::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ReservedUsernames::Username)
                            .custom("citext")
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(ReservedUsernames::Source).text().not_null())
                    .col(ColumnDef::new(ReservedUsernames::Reason).text())
                    .col(ColumnDef::new(ReservedUsernames::AddedBy).uuid())
                    .col(
                        ColumnDef::new(ReservedUsernames::AddedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_reserved_usernames_added_by")
                            .from(ReservedUsernames::Table, ReservedUsernames::AddedBy)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ReservedUsernames::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(AllowedEmailsDomains::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(BlockedEmailsDomains::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(InstanceAlerts::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(InstanceAuditLogs::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(RefreshTokens::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(AuthTokens::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(UserEmails::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Users::Table).to_owned())
            .await?;

        manager
            .get_connection()
            .execute_unprepared("DROP EXTENSION IF EXISTS citext;")
            .await?;

        Ok(())
    }
}

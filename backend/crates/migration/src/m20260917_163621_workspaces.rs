use sea_orm_migration::prelude::*;

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Teams {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Workspaces {
    Table,
    Id,
    TeamId,
    OwnerId,
    Slug,
    Name,
    Description,
    Color,
    AvatarUrl,
    Type,
    Platform,
    Mode,
    StartsAt,
    EndsAt,
    LockAfterEnd,
    DefaultWriteupVisibility,
    Archived,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum WorkspaceMembers {
    Table,
    WorkspaceId,
    UserId,
    Role,
    IsGuest,
    JoinedAt,
}

#[derive(DeriveIden)]
enum WorkspaceFavorites {
    Table,
    UserId,
    WorkspaceId,
    CreatedAt,
}

#[derive(DeriveIden)]
enum WorkspaceInvitations {
    Table,
    Id,
    WorkspaceId,
    Token,
    DefaultRole,
    CreatedBy,
    ExpiresAt,
    MaxUses,
    Uses,
    AsGuest,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Categories {
    Table,
    Slug,
    Label,
    Color,
    DisplayOrder,
}

#[derive(DeriveIden)]
enum Tags {
    Table,
    Id,
    WorkspaceId,
    Slug,
    IsSystem,
}

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260917_163621_workspaces"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // WORKSPACES TABLE
        manager
            .create_table(
                Table::create()
                    .table(Workspaces::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Workspaces::Id)
                            .uuid()
                            .not_null()
                            .default(Expr::cust("uuidv7()"))
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Workspaces::TeamId).uuid())
                    .col(ColumnDef::new(Workspaces::OwnerId).uuid())
                    // Uniqueness is carried by the named index below rather
                    // than by `unique_key()` here, so the constraint the
                    // routes depend on has a name worth reading in an error.
                    .col(ColumnDef::new(Workspaces::Slug).custom("citext").not_null())
                    .col(ColumnDef::new(Workspaces::Name).text().not_null())
                    .col(ColumnDef::new(Workspaces::Description).text())
                    .col(ColumnDef::new(Workspaces::Color).text().not_null())
                    // Two states only: NULL means no image, and the frontend
                    // derives initials from `name` for it rather than storing
                    // anything. Non-NULL is the URL of an uploaded image.
                    // There is no icon identifier any more, so nothing here to
                    // constrain to a closed set the way `color` is.
                    .col(ColumnDef::new(Workspaces::AvatarUrl).text())
                    .col(ColumnDef::new(Workspaces::Type).text().not_null())
                    .col(ColumnDef::new(Workspaces::Platform).text())
                    .col(ColumnDef::new(Workspaces::Mode).text().not_null())
                    .col(ColumnDef::new(Workspaces::StartsAt).timestamp_with_time_zone())
                    .col(ColumnDef::new(Workspaces::EndsAt).timestamp_with_time_zone())
                    .col(
                        ColumnDef::new(Workspaces::LockAfterEnd)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(Workspaces::DefaultWriteupVisibility)
                            .text()
                            .not_null()
                            .default("draft"),
                    )
                    .col(
                        ColumnDef::new(Workspaces::Archived)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(Workspaces::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::cust("now()")),
                    )
                    .col(
                        ColumnDef::new(Workspaces::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::cust("now()")),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_workspaces_team_id")
                            .from(Workspaces::Table, Workspaces::TeamId)
                            .to(Teams::Table, Teams::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    // RESTRICT, not CASCADE: a solo workspace is the only copy
                    // of its owner's work, so deleting the account must fail
                    // loudly here rather than silently taking the workspace,
                    // and everything hanging off it, with it. Account deletion
                    // has to reassign or archive first.
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_workspaces_owner_id")
                            .from(Workspaces::Table, Workspaces::OwnerId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    // A workspace belongs to a team or to a person, and `mode`
                    // must agree with whichever column is filled. Without this,
                    // `mode` is a label the application sets and nothing keeps
                    // honest: a row claiming 'team' with no team_id would read
                    // as a team workspace whose members cannot be resolved, and
                    // the permission checks would have nothing to check
                    // against.
                    .check(Check::named(
                        "ck_workspaces_ownership",
                        Expr::col(Workspaces::TeamId)
                            .is_not_null()
                            .and(Expr::col(Workspaces::Mode).eq("team"))
                            .or(Expr::col(Workspaces::OwnerId)
                                .is_not_null()
                                .and(Expr::col(Workspaces::Mode).eq("solo"))),
                    ))
                    .to_owned(),
            )
            .await?;

        // One slug per instance, on the same principle as `teams.slug`.
        // Scoping it per team would match how workspaces are owned, but not
        // how they are addressed: the routes are flat (`GET
        // /workspaces/:slug`, no team segment), so the slug is the whole
        // lookup key and two teams holding the same one would make the URL
        // ambiguous. Whatever the resolver then picked would decide which
        // team's workspace a member of neither sees.
        //
        // citext, so the uniqueness is case-insensitive: `/workspaces/pwn`
        // and `/workspaces/PWN` are one workspace, not two.
        manager
            .create_index(
                Index::create()
                    .name("idx_workspaces_unique_slug")
                    .table(Workspaces::Table)
                    .col(Workspaces::Slug)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // WORKSPACE MEMBERS TABLE
        manager
            .create_table(
                Table::create()
                    .table(WorkspaceMembers::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(WorkspaceMembers::WorkspaceId)
                            .uuid()
                            .not_null(),
                    )
                    .col(ColumnDef::new(WorkspaceMembers::UserId).uuid().not_null())
                    // Nullable on purpose: a team member with no row-level role
                    // inherits the one they hold on the team. Only a guest, who
                    // has no team role to inherit, is required to carry one
                    // here.
                    .col(ColumnDef::new(WorkspaceMembers::Role).text())
                    // "Holds a guest grant on this workspace", not "is not a
                    // team member". The two can coexist, and this does not
                    // stop being true when the membership starts: someone
                    // admitted as a guest who later joins the team keeps the
                    // row, dormant, since `effective_role` reads the override
                    // either way and `list_effective_members` reports them as
                    // the team member they have become.
                    //
                    // It is also what is left if they leave the team again,
                    // which is why `team_service::remove_member` bounds its
                    // cleanup to `is_guest = false`: a grant made
                    // independently of the team is not something leaving it
                    // asked to revoke.
                    .col(
                        ColumnDef::new(WorkspaceMembers::IsGuest)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(WorkspaceMembers::JoinedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::cust("now()")),
                    )
                    .primary_key(
                        Index::create()
                            .col(WorkspaceMembers::WorkspaceId)
                            .col(WorkspaceMembers::UserId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_workspace_members_workspace_id")
                            .from(WorkspaceMembers::Table, WorkspaceMembers::WorkspaceId)
                            .to(Workspaces::Table, Workspaces::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_workspace_members_user_id")
                            .from(WorkspaceMembers::Table, WorkspaceMembers::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    // A guest has no team role behind them, so a NULL role
                    // leaves the resolver with nothing to fall back on.
                    // Whether it then failed open or closed would be an
                    // implementation detail of whoever wrote the resolver, so
                    // the state is made unreachable instead.
                    .check(Check::named(
                        "ck_workspace_members_guest_has_role",
                        Expr::col(WorkspaceMembers::IsGuest)
                            .eq(false)
                            .or(Expr::col(WorkspaceMembers::Role).is_not_null()),
                    ))
                    .to_owned(),
            )
            .await?;

        // WORKSPACE FAVORITES TABLE
        manager
            .create_table(
                Table::create()
                    .table(WorkspaceFavorites::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(WorkspaceFavorites::UserId).uuid().not_null())
                    .col(
                        ColumnDef::new(WorkspaceFavorites::WorkspaceId)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(WorkspaceFavorites::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::cust("now()")),
                    )
                    .primary_key(
                        Index::create()
                            .col(WorkspaceFavorites::UserId)
                            .col(WorkspaceFavorites::WorkspaceId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_workspace_favorites_user_id")
                            .from(WorkspaceFavorites::Table, WorkspaceFavorites::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_workspace_favorites_workspace_id")
                            .from(WorkspaceFavorites::Table, WorkspaceFavorites::WorkspaceId)
                            .to(Workspaces::Table, Workspaces::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // WORKSPACE INVITATIONS TABLE
        manager
            .create_table(
                Table::create()
                    .table(WorkspaceInvitations::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(WorkspaceInvitations::Id)
                            .uuid()
                            .not_null()
                            .default(Expr::cust("uuidv7()"))
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(WorkspaceInvitations::WorkspaceId)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(WorkspaceInvitations::Token)
                            .text()
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(WorkspaceInvitations::DefaultRole)
                            .text()
                            .not_null(),
                    )
                    .col(ColumnDef::new(WorkspaceInvitations::CreatedBy).uuid())
                    .col(
                        ColumnDef::new(WorkspaceInvitations::ExpiresAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(ColumnDef::new(WorkspaceInvitations::MaxUses).integer())
                    .col(
                        ColumnDef::new(WorkspaceInvitations::Uses)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(WorkspaceInvitations::AsGuest)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(WorkspaceInvitations::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::cust("now()")),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_workspace_invitations_workspace_id")
                            .from(
                                WorkspaceInvitations::Table,
                                WorkspaceInvitations::WorkspaceId,
                            )
                            .to(Workspaces::Table, Workspaces::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    // SET NULL rather than CASCADE: the invitations an author
                    // handed out stay valid when their account goes, so a
                    // deletion never silently revokes access other people are
                    // already relying on. The attribution is dropped, the
                    // grant is not.
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_workspace_invitations_created_by")
                            .from(WorkspaceInvitations::Table, WorkspaceInvitations::CreatedBy)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        // CATEGORIES AND TAGS TABLES
        //
        // Both belong to the Phase 4 challenge schema and are pulled forward
        // to here on purpose. Step 3.11 seeds the system categories and tags
        // as part of workspace bootstrapping, so the tables have to exist
        // before any workspace does; leaving them in Phase 4 would mean either
        // a seed with nowhere to write or a second backfill migration once
        // Phase 4 lands.
        //
        // Only the taxonomy moves. `challenges`, `challenge_assignments` and
        // `challenge_tags` stay in Phase 4: nothing in Phase 3 reads them, and
        // until then they would be empty tables with no writer.
        manager
            .create_table(
                Table::create()
                    .table(Categories::Table)
                    .if_not_exists()
                    // The slug is the key: categories are instance-wide, few,
                    // and referenced by name from the seed and from the
                    // frontend. A surrogate id would add a lookup without
                    // adding anything to identify them by.
                    .col(
                        ColumnDef::new(Categories::Slug)
                            .custom("citext")
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Categories::Label).text().not_null())
                    .col(ColumnDef::new(Categories::Color).text().not_null())
                    .col(
                        ColumnDef::new(Categories::DisplayOrder)
                            .integer()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Tags::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Tags::Id)
                            .uuid()
                            .not_null()
                            .default(Expr::cust("uuidv7()"))
                            .primary_key(),
                    )
                    // NULL for the system tags seeded at 3.11, which belong to
                    // no workspace and are visible from all of them.
                    .col(ColumnDef::new(Tags::WorkspaceId).uuid())
                    .col(ColumnDef::new(Tags::Slug).custom("citext").not_null())
                    .col(
                        ColumnDef::new(Tags::IsSystem)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_tags_workspace_id")
                            .from(Tags::Table, Tags::WorkspaceId)
                            .to(Workspaces::Table, Workspaces::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Tags, unlike workspaces, are genuinely scoped: they are never
        // addressed by a route of their own, only read within a workspace, so
        // two workspaces may each have a `web` tag. That needs two partial
        // indexes rather than one UNIQUE (workspace_id, slug), because in a
        // multi-column unique constraint a NULL never equals another NULL:
        // every system tag (workspace_id IS NULL) would be unique against
        // every other one for free, and `web` could be seeded any number of
        // times. The predicates are exact complements, so each row is covered
        // by exactly one of the two.
        manager
            .create_index(
                Index::create()
                    .name("idx_tags_unique_per_workspace")
                    .table(Tags::Table)
                    .col(Tags::WorkspaceId)
                    .col(Tags::Slug)
                    .and_where(Expr::col(Tags::WorkspaceId).is_not_null())
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_tags_unique_system_slug")
                    .table(Tags::Table)
                    .col(Tags::Slug)
                    .and_where(Expr::col(Tags::WorkspaceId).is_null())
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Reverse creation order, so every referencing table is gone before the
        // table it points at. The indexes go with their tables.
        manager
            .drop_table(Table::drop().table(Tags::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(Categories::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(WorkspaceInvitations::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(WorkspaceFavorites::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(WorkspaceMembers::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(Workspaces::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

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
    Slug,
    Name,
    Color,
    Personal,
    CreatedAt,
    CreatedBy,
}

/// The closed palette `teams.color` is constrained to.
///
/// Kept in sync with `domain::dto::team::TeamColor` and, on the frontend side,
/// with `WS_COLORS` in `lib/ws-color.ts`. Duplicated here rather than imported
/// because a migration must keep describing the schema it created even after
/// the enum in the code moves on.
const TEAM_COLORS: [&str; 6] = ["ember", "blue", "violet", "teal", "rose", "amber"];

/// Same idea for the role columns. This one guards an authorization field, so
/// the database refusing an unknown value is worth more than the application
/// layer quietly reading it back as the least-privileged role.
const TEAM_ROLES: [&str; 4] = ["captain", "admin", "member", "viewer"];

#[derive(DeriveIden)]
enum TeamMembers {
    Table,
    TeamId,
    UserId,
    Role,
    JoinedAt,
}

#[derive(DeriveIden)]
enum TeamInvitations {
    Table,
    Id,
    TeamId,
    Token,
    DefaultRole,
    InvitedUserId,
    InvitedEmail,
    CreatedBy,
    CreatedAt,
    ExpiresAt,
    MaxUses,
    Uses,
}

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260911_141751_teams"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // TEAMS TABLE
        manager
            .create_table(
                Table::create()
                    .table(Teams::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Teams::Id)
                            .uuid()
                            .not_null()
                            .default(Expr::cust("uuidv7()"))
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Teams::Slug)
                            .custom("citext")
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(Teams::Name).text().not_null())
                    .col(
                        ColumnDef::new(Teams::Color)
                            .text()
                            .not_null()
                            .check(Expr::col(Teams::Color).is_in(TEAM_COLORS)),
                    )
                    .col(
                        ColumnDef::new(Teams::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::cust("now()")),
                    )
                    .col(
                        ColumnDef::new(Teams::Personal)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(ColumnDef::new(Teams::CreatedBy).uuid())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_teams_created_by")
                            .from(Teams::Table, Teams::CreatedBy)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        // TEAM MEMBERS TABLE
        manager
            .create_table(
                Table::create()
                    .table(TeamMembers::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(TeamMembers::TeamId).uuid().not_null())
                    .col(ColumnDef::new(TeamMembers::UserId).uuid().not_null())
                    .col(
                        ColumnDef::new(TeamMembers::Role)
                            .text()
                            .not_null()
                            .check(Expr::col(TeamMembers::Role).is_in(TEAM_ROLES)),
                    )
                    .col(
                        ColumnDef::new(TeamMembers::JoinedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::cust("now()")),
                    )
                    .primary_key(
                        Index::create()
                            .col(TeamMembers::TeamId)
                            .col(TeamMembers::UserId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_team_members_team_id")
                            .from(TeamMembers::Table, TeamMembers::TeamId)
                            .to(Teams::Table, Teams::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_team_members_user_id")
                            .from(TeamMembers::Table, TeamMembers::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // TEAM INVITATIONS TABLE
        manager
            .create_table(
                Table::create()
                    .table(TeamInvitations::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(TeamInvitations::Id)
                            .uuid()
                            .not_null()
                            .default(Expr::cust("uuidv7()"))
                            .primary_key(),
                    )
                    .col(ColumnDef::new(TeamInvitations::TeamId).uuid().not_null())
                    .col(
                        ColumnDef::new(TeamInvitations::Token)
                            .text()
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(TeamInvitations::DefaultRole)
                            .text()
                            .not_null()
                            .check(Expr::col(TeamInvitations::DefaultRole).is_in(TEAM_ROLES)),
                    )
                    .col(ColumnDef::new(TeamInvitations::InvitedUserId).uuid())
                    .col(ColumnDef::new(TeamInvitations::InvitedEmail).custom("citext"))
                    .col(ColumnDef::new(TeamInvitations::CreatedBy).uuid())
                    .col(
                        ColumnDef::new(TeamInvitations::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::cust("now()")),
                    )
                    .col(ColumnDef::new(TeamInvitations::ExpiresAt).timestamp_with_time_zone())
                    .col(ColumnDef::new(TeamInvitations::MaxUses).integer())
                    .col(
                        ColumnDef::new(TeamInvitations::Uses)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_team_invitations_team_id")
                            .from(TeamInvitations::Table, TeamInvitations::TeamId)
                            .to(Teams::Table, Teams::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_team_invitations_created_by")
                            .from(TeamInvitations::Table, TeamInvitations::CreatedBy)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_team_invitations_invited_user_id")
                            .from(TeamInvitations::Table, TeamInvitations::InvitedUserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    // At most one target, never both. The two columns are two
                    // ways of saying who an invitation is for, and a row
                    // setting both says two different things at once: the
                    // acceptance paths then disagree about which one to
                    // believe, and an invitation can read as addressed to one
                    // person while granting the team to whoever holds the
                    // mailbox.
                    //
                    // Deliberately not "exactly one": both NULL is the generic
                    // invitation link, where the token is the only credential
                    // and anyone holding it may join.
                    //
                    // Mirrors `invitee_is_named_once` on CreateInvitationRequest.
                    // The validator gives the caller a usable 422; this makes
                    // the state unreachable for every other writer: the admin
                    // CLI, a data migration, hand SQL.
                    .check(Check::named(
                        "ck_team_invitations_single_target",
                        Expr::col(TeamInvitations::InvitedUserId)
                            .is_null()
                            .or(Expr::col(TeamInvitations::InvitedEmail).is_null()),
                    ))
                    .to_owned(),
            )
            .await?;

        // One personal team per account, enforced by the database rather than
        // by the caller remembering to look first. `create_personal_team` runs
        // on every email verification, so a second verified address (or a
        // replayed token) would otherwise mint a duplicate.
        //
        // Partial on `personal` so ordinary teams are unaffected, and NULL
        // `created_by` values never conflict with each other in Postgres, which
        // is what keeps teams orphaned by an account deletion from clashing.
        manager
            .create_index(
                Index::create()
                    .name("idx_teams_one_personal_per_creator")
                    .table(Teams::Table)
                    .col(Teams::CreatedBy)
                    .and_where(Expr::col(Teams::Personal))
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_team_invitations_unique_per_user")
                    .table(TeamInvitations::Table)
                    .col(TeamInvitations::TeamId)
                    .col(TeamInvitations::InvitedUserId)
                    .and_where(Expr::col(TeamInvitations::InvitedUserId).is_not_null())
                    .unique()
                    .to_owned(),
            )
            .await?;

        // Dedupes pending email-only invitations (no invited_user_id yet,
        // account does not exist). Deliberately disjoint from the index
        // above: an invitation that already resolved to a user id is deduped
        // by that index instead, so a row is never covered by both.
        manager
            .create_index(
                Index::create()
                    .name("idx_team_invitations_unique_per_email")
                    .table(TeamInvitations::Table)
                    .col(TeamInvitations::TeamId)
                    .col(TeamInvitations::InvitedEmail)
                    .and_where(Expr::col(TeamInvitations::InvitedUserId).is_null())
                    .and_where(Expr::col(TeamInvitations::InvitedEmail).is_not_null())
                    .unique()
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(TeamInvitations::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(TeamMembers::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(Teams::Table).if_exists().to_owned())
            .await?;
        Ok(())
    }
}

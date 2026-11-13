use sea_orm_migration::prelude::*;

#[derive(DeriveIden)]
enum Workspaces {
    Table,
    Color,
    Type,
    Platform,
    DefaultWriteupVisibility,
}

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20261008_090000_workspaces_enum_checks"
    }
}

/// The closed sets the text columns of `workspaces` are constrained to.
///
/// Frozen here rather than imported from `domain`, same reasoning as
/// `TEAM_COLORS` in the teams migration: a migration must keep describing the
/// schema it created even after the enums in the code move on. Each list
/// mirrors the `as_str()` values of its enum (`AccentColor`, `WorkspaceType`,
/// `Platform`, `WriteupVisibility`), which are what the services write, not
/// the serde names the API uses ("rootme", not "root_me").
const COLORS: [&str; 6] = ["ember", "blue", "violet", "teal", "rose", "amber"];
const TYPES: [&str; 2] = ["platform", "event"];
const PLATFORMS: [&str; 5] = ["htb", "thm", "rootme", "ctftime", "other"];
const VISIBILITIES: [&str; 2] = ["draft", "team"];

const CK_COLOR: &str = "ck_workspaces_color";
const CK_TYPE: &str = "ck_workspaces_type";
const CK_PLATFORM: &str = "ck_workspaces_platform";
const CK_PLATFORM_REQUIRED: &str = "ck_workspaces_platform_required";
const CK_VISIBILITY: &str = "ck_workspaces_visibility";

/// The five checks, by name. `mode` has none of its own: `ck_workspaces_ownership`
/// already admits only 'team' (with a team) and 'solo' (with an owner), so a
/// second check on the same column could only restate it.
fn checks() -> [(&'static str, Expr); 5] {
    [
        (CK_COLOR, Expr::col(Workspaces::Color).is_in(COLORS)),
        (CK_TYPE, Expr::col(Workspaces::Type).is_in(TYPES)),
        (
            CK_PLATFORM,
            Expr::col(Workspaces::Platform)
                .is_null()
                .or(Expr::col(Workspaces::Platform).is_in(PLATFORMS)),
        ),
        // A platform workspace has to say which platform; `other` is the
        // documented way out for one that is not in the list. Optional for an
        // event.
        (
            CK_PLATFORM_REQUIRED,
            Expr::col(Workspaces::Type)
                .ne("platform")
                .or(Expr::col(Workspaces::Platform).is_not_null()),
        ),
        (
            CK_VISIBILITY,
            Expr::col(Workspaces::DefaultWriteupVisibility).is_in(VISIBILITIES),
        ),
    ]
}

/// Renders a check through sea-query, which quotes the values itself.
/// `TableAlterStatement` has no "add check" option, so the statement around
/// the expression is the only SQL written by hand, and it carries nothing but
/// a constant table name and a constant constraint name.
fn add_check_sql(name: &str, check: &Expr) -> String {
    let mut condition = String::new();
    PostgresQueryBuilder.prepare_expr(check, &mut condition);

    format!(r#"ALTER TABLE "workspaces" ADD CONSTRAINT "{name}" CHECK ({condition})"#)
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    /// Normalises the values with a known honest answer, then adds the
    /// constraints. Runs in one transaction (sea-orm-migration's default on
    /// Postgres), so a failing constraint also rolls the normalisation back.
    ///
    /// Any other invalid value makes a constraint fail and the migration with
    /// it. That is deliberate: there is no honest answer for it, and guessing
    /// one would hide the row that needs a human.
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // A colour outside the palette already renders as Ember: that is the
        // read fallback in `AccentColor`. Writing it down changes nothing the
        // user sees.
        manager
            .exec_stmt(
                Query::update()
                    .table(Workspaces::Table)
                    .value(Workspaces::Color, "ember")
                    .and_where(Expr::col(Workspaces::Color).is_not_in(COLORS))
                    .to_owned(),
            )
            .await?;

        // 'ctf' is the only legacy type known to have been written, by a test
        // fixture. A CTF is an event.
        manager
            .exec_stmt(
                Query::update()
                    .table(Workspaces::Table)
                    .value(Workspaces::Type, "event")
                    .and_where(Expr::col(Workspaces::Type).eq("ctf"))
                    .to_owned(),
            )
            .await?;

        // A platform workspace that never named its platform gets `other`,
        // the escape hatch the rule documents, rather than a guess.
        manager
            .exec_stmt(
                Query::update()
                    .table(Workspaces::Table)
                    .value(Workspaces::Platform, "other")
                    .and_where(Expr::col(Workspaces::Type).eq("platform"))
                    .and_where(Expr::col(Workspaces::Platform).is_null())
                    .to_owned(),
            )
            .await?;

        let db = manager.get_connection();
        for (name, check) in checks() {
            db.execute_unprepared(&add_check_sql(name, &check)).await?;
        }

        Ok(())
    }

    /// Drops the five constraints. The normalisations are not undone: the
    /// values they replaced were never recorded, so there is nothing to put
    /// back, and every normalised value is also valid without the constraints.
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for (name, _) in checks() {
            manager
                .alter_table(
                    Table::alter()
                        .table(Workspaces::Table)
                        .drop_constraint(name)
                        .to_owned(),
                )
                .await?;
        }

        Ok(())
    }
}

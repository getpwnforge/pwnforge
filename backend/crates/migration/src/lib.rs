// crates/migration/src/lib.rs
pub use sea_orm_migration::prelude::*;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260808_173047_auth::Migration),
            Box::new(m20260818_160804_instance_settings::Migration),
            Box::new(m20260911_141751_teams::Migration),
            Box::new(m20260917_163621_workspaces::Migration),
            Box::new(m20260922_101500_avatar_version::Migration),
            Box::new(m20260922_143000_drop_avatar_url::Migration),
            Box::new(m20261008_090000_workspaces_enum_checks::Migration),
            Box::new(m20261008_120000_personal_team_captains::Migration),
            Box::new(m20261012_090000_workspaces_exclusive_ownership::Migration),
            Box::new(m20261012_120000_teams_single_captain::Migration),
        ]
    }
}
mod m20260808_173047_auth;
mod m20260818_160804_instance_settings;
mod m20260911_141751_teams;
mod m20260917_163621_workspaces;
mod m20260922_101500_avatar_version;
mod m20260922_143000_drop_avatar_url;
mod m20261008_090000_workspaces_enum_checks;
mod m20261008_120000_personal_team_captains;
mod m20261012_090000_workspaces_exclusive_ownership;
mod m20261012_120000_teams_single_captain;

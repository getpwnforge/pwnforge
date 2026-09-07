// crates/migration/src/lib.rs
pub use sea_orm_migration::prelude::*;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260808_173047_auth::Migration),
            Box::new(m20260818_160804_instance_settings::Migration),
        ]
    }
}
mod m20260808_173047_auth;
mod m20260818_160804_instance_settings;

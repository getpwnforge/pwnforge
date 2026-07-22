pub use sea_orm_migration::prelude::*;

mod m202607021_000001_init;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m202607021_000001_init::Migration)]
    }
}

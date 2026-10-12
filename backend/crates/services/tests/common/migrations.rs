//! A migration's place in the Migrator, found by name, so a test aims at the
//! same migration whatever is appended after it.
//!
//! Included with `#[path]` by the tests that use it; see `scratch.rs`.

use migration::MigratorTrait;

/// How many migrations come before `name`, which is also how many steps
/// `Migrator::up` takes to stop just short of it.
pub fn position_of(name: &str) -> u32 {
    migration::Migrator::migrations()
        .iter()
        .position(|m| m.name() == name)
        .expect("the migration must be registered in the Migrator") as u32
}

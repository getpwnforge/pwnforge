// crates/seed/src/lib.rs
pub mod seeders;

use anyhow::{Context, Result};
use sea_orm::DatabaseConnection;

/// Embedded at compile time: the image ships with the list, nothing to mount.
const DISPOSABLE_DOMAINS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../data/disposable_email_blocklist.conf"
));

const RESERVED_USERNAMES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../data/reserved_usernames.conf"
));

/// Idempotent: safe to call on every boot.
pub async fn run(db: &DatabaseConnection) -> Result<()> {
    let count_domains =
        seeders::disposable_domains::seed_disposable_domains(db, DISPOSABLE_DOMAINS)
            .await
            .context("failed to seed disposable email domains")?;

    tracing::info!("processed {count_domains} disposable email domains");

    let count_usernames =
        seeders::reserved_usernames::seed_reserved_usernames(db, RESERVED_USERNAMES)
            .await
            .context("failed to seed reserved usernames")?;

    tracing::info!("processed {count_usernames} reserved usernames");

    Ok(())
}

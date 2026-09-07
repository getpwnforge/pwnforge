// crates/api/src/services/instance_service.rs
use domain::entities::instance_settings;
use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr, EntityTrait};
use std::sync::{LazyLock, RwLock};
use std::time::{Duration, Instant};
use thiserror::Error;

/// The settings row is a singleton created by the migration, so its primary
/// key is always 1.
pub const SETTINGS_ID: i16 = 1;

#[derive(Debug, Error)]
pub enum InstanceError {
    #[error(transparent)]
    Db(#[from] DbErr),
}

const PUBLIC_CONFIG_CACHE_TTL: Duration = Duration::from_secs(60);

struct CachedConfig {
    hide_landing_page: bool,
    cached_at: Instant,
}

static PUBLIC_CONFIG_CACHE: LazyLock<RwLock<Option<CachedConfig>>> =
    LazyLock::new(|| RwLock::new(None));

/// Reads the single settings row.
///
/// Lives here rather than in setup_service: these settings are consumed by
/// registration, by the wizard, and later by the admin console, so putting the
/// accessor in any one of them would make the others depend on it. auth_service
/// needing setup_service, while setup_service needs auth_service, is exactly
/// the cycle this avoids.
///
/// Generic over the connection so callers can read inside the transaction that
/// is about to update the row.
///
/// The row always exists: the migration inserts it, so callers never deal with
/// an "uninitialised" absence.
pub async fn settings<C: ConnectionTrait>(
    db: &C,
) -> Result<instance_settings::Model, InstanceError> {
    let settings = instance_settings::Entity::find_by_id(SETTINGS_ID)
        .one(db)
        .await?
        .ok_or_else(|| {
            DbErr::Custom("instance_settings row missing, schema out of sync".to_owned())
        })?;

    Ok(settings)
}

/// Whether the setup wizard has run.
///
/// Read from the database rather than from the in-memory token, so a setup
/// completed by another instance of the process is seen immediately.
pub async fn is_setup_completed<C: ConnectionTrait>(db: &C) -> Result<bool, InstanceError> {
    Ok(settings(db).await?.setup_completed_at.is_some())
}

/// Cached read of the single field the public landing page needs. Separate
/// from `settings()`, which stays uncached: that function is also used
/// inside the setup transaction, where a stale read would be a correctness
/// bug, not just a missed optimisation.
pub async fn cached_hide_landing_page(db: &DatabaseConnection) -> Result<bool, InstanceError> {
    {
        let cache = PUBLIC_CONFIG_CACHE.read().unwrap();
        if let Some(entry) = cache.as_ref()
            && entry.cached_at.elapsed() < PUBLIC_CONFIG_CACHE_TTL
        {
            return Ok(entry.hide_landing_page);
        }
    }

    let value = settings(db).await?.hide_landing_page;

    let mut cache = PUBLIC_CONFIG_CACHE.write().unwrap();
    *cache = Some(CachedConfig {
        hide_landing_page: value,
        cached_at: Instant::now(),
    });

    Ok(value)
}

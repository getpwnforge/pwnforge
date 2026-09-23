pub mod admin_service;
pub mod alert_service;
pub mod audit_service;
pub mod auth_service;
pub mod auth_token_service;
pub mod blocked_email_service;
pub mod config;
pub mod contact_service;
pub mod email_service;
pub mod instance_service;
pub mod jwt_service;
pub mod legal_service;
pub mod password_service;
pub mod permissions_service;
pub mod rate_limit_service;
pub mod session_service;
pub mod setup_service;
pub mod slug_service;
pub mod stats_service;
pub mod team_service;
pub mod token_service;
pub mod turnstile_service;
pub mod workspace_service;

use config::Config;
pub use redis::aio::ConnectionManager;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};

rust_i18n::i18n!("locales", fallback = "en");

/// Opens the SeaORM connection pool from `config.database_url`.
///
/// The pool is sized explicitly rather than left to SeaORM's default of 100.
/// That default is both larger than the bundled Postgres container can serve
/// comfortably (its own `max_connections` is 100, shared with migrations, the
/// admin CLI and any operator session) and invisible, which makes it useless
/// as the reference point for sizing anything downstream, and the API's
/// concurrency limit is derived from this number.
pub async fn connect_db(config: &Config) -> anyhow::Result<DatabaseConnection> {
    let mut options = ConnectOptions::new(&config.database_url);

    options
        .max_connections(config.database_max_connections)
        // One warm connection is enough: a cold instance should not hold open
        // connections it is not using, and the pool grows on demand.
        .min_connections(1)
        // Bounded rather than infinite: a caller waiting forever on a pool
        // permit ties up a request handler with no way to shed it. Kept below
        // the API request timeout so the pool gives up first and the failure
        // is attributed to the database rather than to the request as a whole.
        .acquire_timeout(std::time::Duration::from_secs(10))
        .sqlx_logging(false);

    Ok(Database::connect(options).await?)
}

/// Opens the Redis connection manager from `config.redis_url`.
pub async fn connect_redis(config: &Config) -> anyhow::Result<ConnectionManager> {
    let client = redis::Client::open(config.redis_url.as_str())?;
    Ok(client.get_connection_manager().await?)
}

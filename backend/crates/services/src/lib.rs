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
pub mod rate_limit_service;
pub mod session_service;
pub mod setup_service;
pub mod stats_service;
pub mod token_service;
pub mod turnstile_service;

use config::Config;
pub use redis::aio::ConnectionManager;
use sea_orm::{Database, DatabaseConnection};

rust_i18n::i18n!("locales", fallback = "en");

/// Opens the SeaORM connection pool from `config.database_url`.
pub async fn connect_db(config: &Config) -> anyhow::Result<DatabaseConnection> {
    Ok(Database::connect(&config.database_url).await?)
}

/// Opens the Redis connection manager from `config.redis_url`.
pub async fn connect_redis(config: &Config) -> anyhow::Result<ConnectionManager> {
    let client = redis::Client::open(config.redis_url.as_str())?;
    Ok(client.get_connection_manager().await?)
}

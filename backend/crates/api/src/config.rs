// crates/api/src/config.rs
use anyhow::{Context, Result};

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub redis_url: String,
    #[allow(dead_code)] // consumed by JWT service (not yet implemented : Phase 2)
    pub jwt_secret: String,
    #[allow(dead_code)] // consumed by encryption service (not yet implemented : Phase 2)
    pub encryption_key: String,
    pub port: u16,
    #[allow(dead_code)] // consumed by CORS middleware (not yet implemented : Phase 1)
    pub cors_origin: String,
    pub block_disposable_emails: bool, // whether to block disposable email addresses
    pub cookie_secure: bool,           // whether to set the Secure flag on cookies
    pub trusted_proxy_count: usize,    // number of trusted reverse proxies in front of the backend
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Config {
            database_url: std::env::var("DATABASE_URL").context("DATABASE_URL must be set")?,
            redis_url: std::env::var("REDIS_URL").context("REDIS_URL must be set")?,
            jwt_secret: std::env::var("JWT_SECRET").context("JWT_SECRET must be set")?,
            encryption_key: std::env::var("ENCRYPTION_KEY")
                .context("ENCRYPTION_KEY must be set")?,
            port: std::env::var("PORT")
                .context("PORT must be set")?
                .parse()
                .context("PORT must be a valid u16")?,
            cors_origin: std::env::var("CORS_ORIGIN").context("CORS_ORIGIN must be set")?,
            block_disposable_emails: std::env::var("BLOCK_DISPOSABLE_EMAILS")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .context("BLOCK_DISPOSABLE_EMAILS must be a valid bool")?,
            cookie_secure: std::env::var("COOKIE_SECURE")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .context("COOKIE_SECURE must be a valid bool")?,
            trusted_proxy_count: std::env::var("TRUSTED_PROXY_COUNT")
                .unwrap_or_else(|_| "0".to_string())
                .parse()
                .context("TRUSTED_PROXY_COUNT must be a valid usize")?,
        })
    }
}

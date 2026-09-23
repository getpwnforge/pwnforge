// crates/api/src/state.rs
use jsonwebtoken::{DecodingKey, EncodingKey};
use redis::aio::ConnectionManager;
use sea_orm::DatabaseConnection;
use services::config::Config;
use services::storage_service::ObjectStore;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub redis: ConnectionManager,
    pub config: Arc<Config>,
    pub jwt_keys: Arc<JwtKeys>,
    pub setup_token: Option<Arc<String>>,
    /// Built once at boot and shared. The S3 client holds its own connection
    /// pool, so rebuilding it per request would throw that away.
    pub storage: Arc<ObjectStore>,
}

pub struct JwtKeys {
    pub encoding: EncodingKey,
    pub decoding: DecodingKey,
}

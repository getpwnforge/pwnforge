// crates/api/src/state.rs
use crate::config::Config;
use jsonwebtoken::{DecodingKey, EncodingKey};
use redis::aio::ConnectionManager;
use sea_orm::DatabaseConnection;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub redis: ConnectionManager,
    pub config: Arc<Config>,
    pub jwt_keys: Arc<JwtKeys>,
    pub setup_token: Option<Arc<String>>,
}

pub struct JwtKeys {
    pub encoding: EncodingKey,
    pub decoding: DecodingKey,
}

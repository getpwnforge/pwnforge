use sea_orm::DatabaseConnection;
use redis::aio::ConnectionManager;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub redis: ConnectionManager,
}
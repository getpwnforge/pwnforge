// crates/api/src/main.rs
mod app;
mod config;
mod error;
mod middleware;
mod routes;
mod services;
mod state;
mod tasks;

use config::Config;
use migration::MigratorTrait;
use sea_orm::Database;
use state::AppState;
use std::net::SocketAddr;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // Initialize logging
    middleware::tracing::init_tracing();

    let config = Config::from_env()?;
    let port = config.port;

    let db = Database::connect(&config.database_url).await?;
    let redis_client = redis::Client::open(config.redis_url.as_str())?;
    let redis = redis_client.get_connection_manager().await?;

    let encoding = jsonwebtoken::EncodingKey::from_secret(config.encryption_key.as_bytes());
    let decoding = jsonwebtoken::DecodingKey::from_secret(config.encryption_key.as_bytes());

    // Run database migrations
    migration::Migrator::up(&db, None).await?;

    // Run database seeding
    seed::run(&db).await?;

    let state = AppState {
        db: db.clone(),
        redis,
        config: Arc::new(config),
        jwt_keys: Arc::new(state::JwtKeys { encoding, decoding }),
    };

    // Background cleanup: runs hourly for the lifetime of the process.
    tasks::cleanup::spawn(db);

    let app = app::build_router(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    ::tracing::info!("listening on {}", listener.local_addr()?);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}

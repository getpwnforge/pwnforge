mod app;
mod config;
mod middleware;
mod routes;
mod state;

use config::Config;
use migration::MigratorTrait;
use sea_orm::Database;
use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // Initialize logging
    middleware::tracing::init_tracing();

    let config = Config::from_env()?;

    let db = Database::connect(&config.database_url).await?;
    let redis_client = redis::Client::open(config.redis_url.as_str())?;
    let redis = redis_client.get_connection_manager().await?;

    // Run database migrations
    migration::Migrator::up(&db, None).await?;

    let state = AppState { db, redis };
    let app = app::build_router(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", config.port)).await?;
    ::tracing::info!("listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;

    Ok(())
}

// crates/api/src/main.rs
mod app;
mod error;
mod middleware;
mod routes;
mod state;
mod tasks;

use migration::MigratorTrait;
use services::config::Config;
use state::AppState;
use std::net::SocketAddr;
use std::sync::Arc;

use services::setup_service;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // Initialize logging
    middleware::tracing::init_tracing();

    let config = Config::from_env()?;
    let port = config.port;

    let db = services::connect_db(&config).await?;
    let redis = services::connect_redis(&config).await?;

    let encoding = jsonwebtoken::EncodingKey::from_secret(config.jwt_secret.as_bytes());
    let decoding = jsonwebtoken::DecodingKey::from_secret(config.jwt_secret.as_bytes());

    // Run database migrations
    migration::Migrator::up(&db, None).await?;

    // Run database seeding
    seed::run(&db).await?;

    let setup_token = setup_service::issue_boot_token(&db, config.setup_token.as_deref()).await?;

    if let Some(token) = &setup_token {
        // warn! rather than info!: the operator must not miss it, and it is
        // the one line that matters on a first boot.
        tracing::warn!("Instance not set up yet. Open /setup and use token: {token}");
    }

    let state = AppState {
        db: db.clone(),
        redis,
        config: Arc::new(config),
        jwt_keys: Arc::new(state::JwtKeys { encoding, decoding }),
        setup_token: setup_token.map(Arc::new),
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

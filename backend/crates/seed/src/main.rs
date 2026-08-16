// crates/seed/src/main.rs
use anyhow::{Context, Result};
use sea_orm::Database;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL must be set")?;
    let db = Database::connect(&database_url)
        .await
        .context("failed to connect to the database")?;

    seed::run(&db).await
}

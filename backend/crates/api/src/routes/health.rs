// crates/api/src/routes/health.rs
use crate::state::AppState;
use axum::{Json, Router, extract::State, http::StatusCode, response::IntoResponse, routing::get};
use serde_json::json;

pub fn router() -> Router<AppState> {
    Router::new().route("/health", get(health))
}

// Private: only reachable through router() above.
async fn health(State(mut state): State<AppState>) -> impl IntoResponse {
    let db_ok = state.db.ping().await.is_ok();
    let redis_ok = redis::cmd("PING")
        .query_async::<String>(&mut state.redis)
        .await
        .is_ok();
    let healthy = db_ok && redis_ok;

    let code = if healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        code,
        Json(json!({
            "status": if healthy { "ok" } else { "down" },
            "db": if db_ok { "ok" } else { "down" },
            "redis": if redis_ok { "ok" } else { "down" },
        })),
    )
}

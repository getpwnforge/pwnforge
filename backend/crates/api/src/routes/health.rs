// crates/api/src/routes/health.rs
use crate::state::AppState;
use axum::{Json, Router, extract::State, http::StatusCode, response::IntoResponse, routing::get};
use domain::dto::health::HealthResponse;

pub fn router() -> Router<AppState> {
    Router::new().route("/health", get(health))
}

// Private: only reachable through router() above.
#[utoipa::path(
    get,
    path = "/api/v1/health",
    tag = "Health",
    responses(
        (status = 200, description = "All components reachable", body = HealthResponse),
        (status = 503, description = "At least one component is down", body = HealthResponse),
    )
)]
async fn health(State(mut state): State<AppState>) -> impl IntoResponse {
    let db_ok = state.db.ping().await.is_ok();
    let redis_ok = redis::cmd("PING")
        .query_async::<String>(&mut state.redis)
        .await
        .is_ok();

    let code = if db_ok && redis_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (code, Json(HealthResponse::new(db_ok, redis_ok)))
}

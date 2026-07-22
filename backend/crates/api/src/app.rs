use axum::{routing::get, Router, middleware};
use crate::{routes::health::health, state::AppState, middleware::tracing::request_id_middleware};

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/health", get(health))
        .layer(middleware::from_fn(request_id_middleware))
        .with_state(state)
}
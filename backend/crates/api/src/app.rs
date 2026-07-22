use crate::{middleware::tracing::request_id_middleware, routes::health::health, state::AppState};
use axum::{Router, middleware, routing::get};

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/health", get(health))
        .layer(middleware::from_fn(request_id_middleware))
        .with_state(state)
}

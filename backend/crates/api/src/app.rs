// crates/api/src/app.rs
use crate::{middleware::tracing::request_id_middleware, routes, state::AppState};
use axum::{Router, middleware};

pub fn build_router(state: AppState) -> Router {
    let api_v1 = Router::new()
        .merge(routes::health::router())
        .nest("/auth", routes::auth::router());

    Router::new()
        .nest("/api/v1", api_v1)
        .layer(middleware::from_fn(request_id_middleware))
        .with_state(state)
}

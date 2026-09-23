// crates/api/src/app.rs
use crate::{middleware::tracing::request_id_middleware, routes, state::AppState};
use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::{
        Method, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
    middleware,
};
use std::time::Duration;
use tower::limit::GlobalConcurrencyLimitLayer;
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    timeout::TimeoutLayer,
};

/// Upper bound on how long a single request may run.
///
/// Sized off the slowest legitimate path rather than picked round: `register`
/// can spend 3s on the Pwned Passwords lookup, 3s on Turnstile siteverify,
/// ~100ms hashing with Argon2, plus its database work. Thirty seconds leaves
/// room for all of it on a loaded instance while still guaranteeing that no
/// handler is pinned indefinitely by an unresponsive third party.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Largest request body the API accepts.
///
/// Axum applies a 2 MiB default; this states the real figure instead of
/// inheriting one. Nothing the API takes today comes close: the biggest is
/// the contact form's 5000-character message, roughly 20 KiB at worst in
/// UTF-8. Routes that legitimately need more (writeup bodies, file uploads)
/// should raise it for themselves with their own `DefaultBodyLimit`, so the
/// generous limit stays scoped to the routes that earn it.
const MAX_BODY_BYTES: usize = 64 * 1024;

/// In-flight requests allowed per database connection in the pool.
///
/// A request holds a connection only while a query is actually running; the
/// rest of its life is spent hashing, waiting on an outbound HTTP call, or
/// rendering. Limiting one-to-one would leave the pool idle under load, so the
/// ceiling is a small multiple of the one resource that is genuinely finite.
const REQUESTS_PER_DB_CONNECTION: usize = 4;

/// Builds the API router.
///
/// Note on where WebSocket routes go when they land: they must be nested
/// *outside* `TimeoutLayer`, not merged into `api_v1`. An upgraded connection
/// is meant to stay open for as long as the client is around (nginx allows it
/// 3600s via `proxy_read_timeout`), and a 30s deadline would tear down every
/// live session on schedule.
pub fn build_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        // Unreachable: `Config::validate` rejects anything outside printable
        // ASCII at startup, which is exactly the byte range a header value
        // allows, so this parse cannot fail on a config that booted.
        .allow_origin(AllowOrigin::exact(
            state
                .config
                .cors_origin
                .parse()
                .expect("CORS_ORIGIN is validated at startup by Config::validate"),
        ))
        .allow_credentials(true)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([CONTENT_TYPE, AUTHORIZATION])
        .max_age(Duration::from_secs(3600));

    let max_concurrent_requests =
        state.config.database_max_connections as usize * REQUESTS_PER_DB_CONNECTION;

    let api_v1 = Router::new()
        .merge(routes::health::router())
        .merge(routes::alerts::router())
        .merge(routes::avatars::router())
        .merge(routes::contact::router())
        .merge(routes::teams::router())
        .nest("/instance", routes::instance::router())
        .nest("/legal", routes::legal::router())
        .nest("/auth", routes::auth::router())
        .nest("/setup", routes::setup::router());

    Router::new()
        .nest("/api/v1", api_v1)
        // Ordering matters, and axum applies the last layer added as the
        // outermost one. The timeout therefore wraps the concurrency limit, so
        // that time spent queueing for a permit counts against the deadline
        // too. The other way round, a request could wait unbounded for a
        // permit and only start its clock once it had one.
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        // `GlobalConcurrencyLimitLayer`, not `ConcurrencyLimitLayer`: the
        // latter builds a fresh semaphore on every `layer()` call, and axum
        // calls it once per route, which would hand each route its own
        // independent allowance instead of bounding the instance as a whole.
        .layer(GlobalConcurrencyLimitLayer::new(max_concurrent_requests))
        // Answers with a bare 408, outside the `AppError` envelope every other
        // response uses: the layer sits above the handlers and never reaches
        // `IntoResponse`. Acceptable for now because a client that timed out
        // has no error code to act on anyway, but it is the one response in
        // the API without a machine-readable body.
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            REQUEST_TIMEOUT,
        ))
        .layer(cors)
        .layer(middleware::from_fn(request_id_middleware))
        .with_state(state)
}

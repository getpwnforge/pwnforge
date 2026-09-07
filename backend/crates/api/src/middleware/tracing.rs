// crates/api/src/middleware/tracing.rs
use axum::{extract::Request, middleware::Next, response::Response};

use ::tracing::Instrument;

pub fn init_tracing() {
    let backend_env = std::env::var("BACKEND_ENV").unwrap_or_else(|_| "development".into());
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        if backend_env == "production" {
            tracing_subscriber::EnvFilter::new("info,sqlx=warn")
        } else {
            tracing_subscriber::EnvFilter::new("debug,sqlx=warn,hyper=info")
        }
    });

    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE);

    if backend_env == "production" {
        builder.json().init();
    } else {
        builder.pretty().init();
    }
}

pub async fn request_id_middleware(request: Request, next: Next) -> Response {
    let request_id = uuid::Uuid::new_v4().to_string();
    let method = request.method().clone();
    let path = request.uri().path().to_owned();

    // Health checks run every 10s: at info level they bury everything else.
    let span = if path == "/api/v1/health" {
        tracing::debug_span!("request", %request_id, %method, %path, status = tracing::field::Empty)
    } else {
        tracing::info_span!("request", %request_id, %method, %path, status = tracing::field::Empty)
    };

    async move {
        let response = next.run(request).await;
        tracing::Span::current().record("status", response.status().as_u16());
        response
    }
    .instrument(span)
    .await
}

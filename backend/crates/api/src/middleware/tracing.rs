// middleware/tracing.rs
use axum::{extract::Request, middleware::Next, response::Response};

use ::tracing::Instrument;

pub fn init_tracing() {
    let backend_env = std::env::var("BACKEND_ENV").unwrap_or_else(|_| "development".into());
    let filter = tracing_subscriber::EnvFilter::from_default_env();

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

    let span = ::tracing::info_span!(
        "request",
        request_id = %request_id,
        method = %request.method(),
        path = %request.uri().path(),
    );

    async move {
        let mut response = next.run(request).await;

        response
            .headers_mut()
            .insert("X-Request-ID", request_id.parse().unwrap());
        response
    }
    .instrument(span)
    .await
}

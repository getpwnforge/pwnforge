use axum::{
    Router,
    extract::{ConnectInfo, State},
    http::StatusCode,
    routing::post,
};
use domain::dto::{
    contact::ContactRequest,
    error_responses::{RateLimitedErrorResponse, SimpleErrorResponse, UnprocessableErrorResponse},
};
use services::contact_service;
use std::net::SocketAddr;
use validator::Validate;

use crate::{
    error::AppError,
    middleware::{client_ip::ClientIp, json::JsonBody},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/contact", post(submit))
}

/// Submits the public contact form.
///
/// Generates a ticket reference used as the subject of both the notification
/// and the confirmation mail, so replies stay in a single thread.
#[utoipa::path(
    post,
    path = "/api/v1/contact",
    tag = "Contact",
    request_body = ContactRequest,
    responses(
        (status = 204, description = "Contact request submitted"),
        (status = 400, description = "malformed_json or invalid_body: the body is not JSON, or could not be read; or turnstile_failed", body = SimpleErrorResponse),
        (status = 415, description = "unsupported_media_type: Content-Type is not application/json", body = SimpleErrorResponse),
        (status = 422, description = "validation_failed or invalid_body", body = UnprocessableErrorResponse),
        (status = 429, description = "Rate limit exceeded", body = RateLimitedErrorResponse),
        (status = 503, description = "service_unavailable: the rate limiter is unreachable, and the route fails closed", body = SimpleErrorResponse),
    )
)]
async fn submit(
    State(state): State<AppState>,
    ClientIp(client_ip): ClientIp,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    JsonBody(payload): JsonBody<ContactRequest>,
) -> Result<StatusCode, AppError> {
    payload.validate()?;

    let rate_limit_ip = client_ip.unwrap_or_else(|| remote.ip());

    contact_service::submit_contact(
        &state.db,
        &mut state.redis.clone(),
        &state.config,
        rate_limit_ip,
        payload,
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

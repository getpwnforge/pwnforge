use axum::{
    Router,
    extract::{ConnectInfo, State},
    http::StatusCode,
    routing::post,
};
use domain::dto::contact::ContactRequest;
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

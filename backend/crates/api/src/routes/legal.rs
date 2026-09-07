//! Legal acceptance endpoints.
//!
//! Target path: backend/crates/api/src/routes/legal.rs
//! Declare with `pub mod legal;` in routes/mod.rs and mount in app.rs:
//!
//!     .nest("/legal", routes::legal::router())
//!
//! Handlers stay thin adapters: extract, delegate, map. All logic lives in
//! the `services` crate.

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use chrono::Utc;
use domain::{
    dto::{
        error_responses::{LegalVersionStaleErrorResponse, SimpleErrorResponse},
        legal::{LegalAcceptanceInput, LegalStatusDto, LegalVersionsDto},
    },
    legal::{CURRENT_PRIVACY_VERSION, CURRENT_TERMS_VERSION},
};
use services::legal_service;

use crate::middleware::{auth::AuthUser, client_ip::ClientIp, json::JsonBody, session_context};
use crate::{error::AppError, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/status", get(status))
        .route("/accept", post(accept))
        .route("/versions", get(versions))
}

/// `GET /api/v1/legal/status`
///
/// Returns whether the caller must accept a revised document. The frontend
/// calls this after login and uses the result to choose between nothing, a
/// dismissible banner, and a blocking modal.
#[utoipa::path(
    get,
    path = "/api/v1/legal/status",
    tag = "Legal",
    security(("access_token" = [])),
    responses(
        (status = 200, description = "Legal acceptance status", body = LegalStatusDto),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
    )
)]
async fn status(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<LegalStatusDto>, AppError> {
    let status = legal_service::status(&state.db, user.id, Utc::now()).await?;
    Ok(Json(status))
}

/// `POST /api/v1/legal/accept`
///
/// Records acceptance of the currently published documents. Returns 409 with
/// `legal_version_stale` if the client submitted a superseded version.
///
/// No `ConnectInfo` fallback on the client IP, unlike the rate limit checks
/// in `routes/auth.rs`. When the trusted-proxy chain does not resolve an
/// address, the raw TCP peer is Cloudflare, not the user: recording the
/// proxy's address as evidence would be worse than recording nothing.
#[utoipa::path(
    post,
    path = "/api/v1/legal/accept",
    tag = "Legal",
    security(("access_token" = [])),
    request_body = LegalAcceptanceInput,
    responses(
        (status = 204, description = "Legal documents accepted"),
        (status = 400, description = "Invalid request", body = SimpleErrorResponse),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 409, description = "Legal version stale", body = LegalVersionStaleErrorResponse),
    )
)]
async fn accept(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    ClientIp(client_ip): ClientIp,
    JsonBody(payload): JsonBody<LegalAcceptanceInput>,
) -> Result<StatusCode, AppError> {
    legal_service::validate_input(&payload)?;

    let ctx = session_context(&headers, client_ip);
    legal_service::accept(&state.db, user.id, &payload, &ctx).await?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/legal/versions",
    tag = "Legal",
    responses(
        (status = 200, description = "Legal versions", body = LegalVersionsDto),
    )
)]
/// `GET /api/v1/legal/versions`
///
/// Public: the registration form needs it before any session exists. Read
/// straight from the constants, so it cannot drift from what the backend will
/// accept a moment later.
async fn versions() -> Json<LegalVersionsDto> {
    Json(LegalVersionsDto {
        terms_version: CURRENT_TERMS_VERSION,
        privacy_version: CURRENT_PRIVACY_VERSION,
    })
}

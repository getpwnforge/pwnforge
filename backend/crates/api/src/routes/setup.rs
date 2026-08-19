// crates/api/src/routes/setup.rs
use crate::{
    error::AppError,
    middleware::{client_ip::ClientIp, setup::SetupToken},
    services::{
        audit_service::AuditContext,
        email_service, instance_service,
        rate_limit_service::{self, RateLimitDecision},
        setup_service::{self, SetupError},
    },
    state::AppState,
};
use axum::{
    Json, Router, extract::{ConnectInfo, State}, http::{HeaderMap, StatusCode, header}, response::{IntoResponse, Response}, routing::{get, post}
};
use domain::dto::{
    auth::UserResponse,
    setup::{EmailConfigResponse, SetupRequest, SetupStatusResponse, TestEmailRequest},
};
use sea_orm::prelude::IpNetwork;
use serde_json::json;
use std::net::{IpAddr, SocketAddr};
use validator::Validate;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/status", get(status))
        .route("/email-config", get(email_config))
        .route("/test-email", post(test_email))
        .route("/", post(complete))
}

/// The only route reachable without a token: the frontend cannot know whether
/// to prompt for one before asking.
async fn status(State(state): State<AppState>) -> Result<Json<SetupStatusResponse>, AppError> {
    // 1. instance_service::is_setup_completed
    let completed = instance_service::is_setup_completed(&state.db).await?;
    // 2. Json(SetupStatusResponse { completed })
    Ok(Json(SetupStatusResponse { completed }))
}

/// Lets the operator check that the .env is what the process understood.
async fn email_config(
    State(state): State<AppState>,
    _token: SetupToken,
) -> Result<Json<EmailConfigResponse>, AppError> {
    // The extractor covers the token, not the state of the instance.
    ensure_pending(&state).await?;

    Ok(Json(setup_service::email_config(&state.config)))
}

async fn test_email(
    State(state): State<AppState>,
    _token: SetupToken,
    ClientIp(client_ip): ClientIp,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    Json(payload): Json<TestEmailRequest>,
) -> Result<Response, AppError> {
    ensure_pending(&state).await?;

    payload.validate()?;

    let rate_limit_ip = client_ip.unwrap_or_else(|| remote.ip());

    match rate_limit_service::check_setup_test_email(&mut state.redis.clone(), rate_limit_ip).await {
        Ok(RateLimitDecision::Limited { retry_after_secs }) => {
            return Err(AppError::RateLimited { retry_after_secs });
        }
        Ok(RateLimitDecision::Allowed) => {}
        Err(err) => {
            // Refuse rather than fail open: without a limit this is an open
            // mailer for as long as the setup is pending.
            tracing::error!(error = ?err, "setup test-email rate limit unavailable, refusing");
            return Err(AppError::ServiceUnavailable);
        }
    };

    if let Err(err) = email_service::send_test(&state.config, &payload.to).await {
        tracing::warn!(error = ?err, "setup test email failed");
        return Ok((
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": "email_send_failed", "detail": err.to_string() })),
        )
            .into_response());
    }

    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn complete(
    State(state): State<AppState>,
    _token: SetupToken,
    ClientIp(client_ip): ClientIp,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(payload): Json<SetupRequest>,
) -> Result<(StatusCode, Json<UserResponse>), AppError> {
    payload.validate()?;

    let rate_limit_ip = client_ip.unwrap_or_else(|| remote.ip());

    match rate_limit_service::check_setup_attempt(&mut state.redis.clone(), rate_limit_ip).await {
        Ok(RateLimitDecision::Limited { retry_after_secs }) => {
            return Err(AppError::RateLimited { retry_after_secs });
        }
        Ok(RateLimitDecision::Allowed) => {}
        Err(err) => {
            tracing::error!(error = ?err, "setup complete rate limit unavailable, refusing");
            return Err(AppError::ServiceUnavailable);
        }
    };

    let ctx = audit_context(&headers, client_ip);

    let (user, email) = setup_service::complete(&state.db, &state.config, &ctx, payload).await?;

    Ok((StatusCode::CREATED, Json(UserResponse::new(user, email))))

}

/// Request metadata for the audit entry. Same shape as the session context
/// built in routes/auth.rs, but the setup has no session to attach it to.
fn audit_context(headers: &HeaderMap, client_ip: Option<IpAddr>) -> AuditContext {
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        // Fully client-controlled: cap it before it reaches the database.
        .map(|s| s.chars().take(512).collect::<String>());

    AuditContext {
        actor_id: None,
        ip_address: client_ip.map(IpNetwork::from),
        user_agent,
    }
}

/// Refuses once the wizard has run.
///
/// 404 rather than 403: the setup surface should not be advertised at all
/// after initialisation, and the token extractor cannot know the state of the
/// instance.
async fn ensure_pending(state: &AppState) -> Result<(), AppError> {
    if instance_service::is_setup_completed(&state.db).await? {
        return Err(SetupError::AlreadyCompleted.into());
    }

    Ok(())
}

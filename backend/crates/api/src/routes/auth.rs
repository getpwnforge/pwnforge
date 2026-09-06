// crates/api/src/routes/auth.rs
use crate::middleware::{auth::AuthUser, client_ip::ClientIp, json::JsonBody, session_context};
use crate::{error::AppError, state::AppState};
use axum::{
    Json, Router, extract::ConnectInfo, extract::State, http::HeaderMap, http::StatusCode,
    routing::get, routing::post,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use domain::dto::auth::{
    EmailResendRequest, EmailVerifyRequest, LoginRequest, PasswordChangeRequest,
    PasswordForgotRequest, PasswordResetRequest, RegisterRequest, UserResponse,
};
use services::{
    auth_service, jwt_service,
    rate_limit_service::{self, RateLimitDecision},
};
use services::{config::Config, legal_service};
use std::net::SocketAddr;
use time::Duration;
use validator::Validate;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/refresh", post(refresh))
        .route("/logout", post(logout))
        .route("/me", get(me))
        .route("/password/forgot", post(password_forgot))
        .route("/password/reset", post(password_reset))
        .route("/password/change", post(password_change))
        .route("/email/verify", post(email_verify))
        .route("/email/resend", post(email_resend))
}

async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    ClientIp(client_ip): ClientIp,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    JsonBody(payload): JsonBody<RegisterRequest>,
) -> Result<(StatusCode, Json<UserResponse>), AppError> {
    payload.validate()?;
    legal_service::validate_input(&payload.legal)?;

    // Same reasoning as login: a malformed X-Forwarded-For must not strip the
    // IP that Turnstile verifies against.
    let client_ip_resolved = client_ip.unwrap_or_else(|| remote.ip());

    let ctx = session_context(&headers, client_ip);
    let registered =
        auth_service::register(&state.db, &state.config, payload, ctx, client_ip_resolved).await?;
    Ok((StatusCode::CREATED, Json(UserResponse::from(registered))))
}

async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    ClientIp(client_ip): ClientIp,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    JsonBody(payload): JsonBody<LoginRequest>,
) -> Result<(CookieJar, Json<UserResponse>), AppError> {
    // Falls back to the raw TCP peer when the trusted-proxy chain does not
    // resolve a client IP, so a malformed X-Forwarded-For header cannot be
    // used to dodge the rate limit.
    let rate_limit_ip = client_ip.unwrap_or_else(|| remote.ip());
    let identifier = payload.username_or_email.clone();
    let mut redis = state.redis.clone();
    match rate_limit_service::check_login_attempt(&mut redis, &identifier, rate_limit_ip).await {
        Ok(RateLimitDecision::Limited { retry_after_secs }) => {
            return Err(AppError::RateLimited { retry_after_secs });
        }
        Ok(RateLimitDecision::Allowed) => {}
        Err(err) => {
            tracing::error!(error = ?err, "login rate limit check failed, allowing request");
        }
    }

    let ctx = session_context(&headers, client_ip);
    let logged_in = auth_service::login(
        &state.db,
        &state.config,
        payload,
        &state.jwt_keys.encoding,
        ctx,
        rate_limit_ip,
    )
    .await?;

    if let Err(err) = rate_limit_service::reset_login_attempts(&mut redis, &identifier).await {
        tracing::error!(error = ?err, "failed to reset login rate limit after successful login");
    }

    let jar = set_auth_cookies(
        jar,
        &state.config,
        &logged_in.access_token,
        &logged_in.refresh_token,
    );
    Ok((
        jar,
        Json(UserResponse::new(logged_in.user, logged_in.email)),
    ))
}

async fn refresh(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    ClientIp(client_ip): ClientIp,
) -> Result<(CookieJar, Json<UserResponse>), AppError> {
    let refresh_token = jar
        .get("refresh_token")
        .ok_or(AppError::Unauthorized)?
        .value()
        .to_owned();

    let ctx = session_context(&headers, client_ip);
    let refreshed =
        auth_service::refresh(&state.db, &state.jwt_keys.encoding, &refresh_token, ctx).await?;

    let jar = set_auth_cookies(
        jar,
        &state.config,
        &refreshed.access_token,
        &refreshed.refresh_token,
    );
    Ok((
        jar,
        Json(UserResponse::new(refreshed.user, refreshed.email)),
    ))
}

async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<(CookieJar, StatusCode), AppError> {
    if let Some(token) = jar.get("refresh_token") {
        auth_service::logout(&state.db, token.value()).await?;
    }

    Ok((
        clear_auth_cookies(jar, &state.config),
        StatusCode::NO_CONTENT,
    ))
}

async fn me(State(state): State<AppState>, user: AuthUser) -> Result<Json<UserResponse>, AppError> {
    let (model, email) = auth_service::fetch_user(&state.db, user.id).await?;
    Ok(Json(UserResponse::new(model, email)))
}

async fn password_forgot(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    JsonBody(payload): JsonBody<PasswordForgotRequest>,
) -> Result<StatusCode, AppError> {
    payload.validate()?;

    let rate_limit_ip = ip.unwrap_or_else(|| remote.ip());

    match rate_limit_service::check_password_forgot(
        &mut state.redis.clone(),
        &payload.email,
        rate_limit_ip,
    )
    .await
    {
        Ok(RateLimitDecision::Limited { retry_after_secs }) => {
            return Err(AppError::RateLimited { retry_after_secs });
        }
        Ok(RateLimitDecision::Allowed) => {}
        Err(err) => {
            // Refuse rather than fail open: unlike login there is no password
            // check underneath, so an unlimited endpoint here is a free mailer.
            tracing::error!(error = ?err, "password reset rate limit unavailable, refusing");
            return Err(AppError::ServiceUnavailable);
        }
    }

    auth_service::password_forgot(
        &state.db,
        &state.config,
        &payload.email,
        &payload.turnstile_token,
        rate_limit_ip,
    )
    .await?;

    // Always 202: the response must never depend on whether the account exists.
    Ok(StatusCode::ACCEPTED)
}

async fn password_reset(
    State(state): State<AppState>,
    headers: HeaderMap,
    ClientIp(ip): ClientIp,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    JsonBody(payload): JsonBody<PasswordResetRequest>,
) -> Result<StatusCode, AppError> {
    payload.validate()?;

    let rate_limit_ip = ip.unwrap_or_else(|| remote.ip());

    match rate_limit_service::check_password_reset(&mut state.redis.clone(), rate_limit_ip).await {
        Ok(RateLimitDecision::Limited { retry_after_secs }) => {
            return Err(AppError::RateLimited { retry_after_secs });
        }
        Ok(RateLimitDecision::Allowed) => {}
        Err(err) => {
            tracing::error!(error = ?err, "password reset rate limit unavailable, allowing request");
        }
    }

    let ctx = session_context(&headers, ip);

    auth_service::password_reset(
        &state.db,
        &state.config,
        &payload.token,
        payload.new_password,
        ctx,
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn password_change(
    State(state): State<AppState>,
    headers: HeaderMap,
    ClientIp(ip): ClientIp,
    user: AuthUser,
    jar: CookieJar,
    JsonBody(payload): JsonBody<PasswordChangeRequest>,
) -> Result<StatusCode, AppError> {
    payload.validate()?;

    // Absent is fine: without it every session is revoked, including this one.
    let refresh_token = jar.get("refresh_token").map(|c| c.value().to_owned());

    tracing::debug!(has_refresh = refresh_token.is_some(), "password change");

    auth_service::password_change(
        &state.db,
        &state.config,
        user.id,
        refresh_token.as_deref(),
        payload.current_password,
        payload.new_password,
        session_context(&headers, ip),
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn email_verify(
    State(state): State<AppState>,
    JsonBody(payload): JsonBody<EmailVerifyRequest>,
) -> Result<StatusCode, AppError> {
    // POST, not GET: mail clients and spam scanners prefetch links, and a GET
    // would let them burn the token before the user clicks. The frontend page
    // reads the token from the URL and posts it.
    payload.validate()?;

    auth_service::verify_email(&state.db, &payload.token).await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn email_resend(
    State(state): State<AppState>,
    JsonBody(payload): JsonBody<EmailResendRequest>,
) -> Result<StatusCode, AppError> {
    payload.validate()?;
    // 1. Rate limit keyed on the opaque token: 1 per 60s, 5 per hour.
    match rate_limit_service::check_email_resend(&mut state.redis.clone(), &payload.token).await {
        Ok(RateLimitDecision::Limited { retry_after_secs }) => {
            return Err(AppError::RateLimited { retry_after_secs });
        }
        Ok(RateLimitDecision::Allowed) => {}
        Err(err) => {
            tracing::error!(error = ?err, "email resend rate limit unavailable, allowing request");
        }
    }
    // 2. auth_service::resend_verification.
    auth_service::resend_verification(&state.db, &state.config, &payload.token).await?;
    // 3. 202.
    Ok(StatusCode::ACCEPTED)
}

fn set_auth_cookies(jar: CookieJar, config: &Config, access: &str, refresh: &str) -> CookieJar {
    let access_cookie = Cookie::build(("access_token", access.to_owned()))
        .http_only(true)
        .secure(config.cookie_secure)
        .same_site(SameSite::Strict)
        .path("/")
        .max_age(Duration::seconds(jwt_service::ACCESS_TOKEN_TTL_SECS))
        .build();

    // Scoped to the refresh endpoint: it is never sent on ordinary requests.
    let refresh_cookie = Cookie::build(("refresh_token", refresh.to_owned()))
        .http_only(true)
        .secure(config.cookie_secure)
        .same_site(SameSite::Strict)
        .path("/api/v1/auth")
        .max_age(Duration::seconds(jwt_service::REFRESH_TOKEN_TTL_SECS))
        .build();

    jar.add(access_cookie).add(refresh_cookie)
}

fn clear_auth_cookies(jar: CookieJar, config: &Config) -> CookieJar {
    // Path must match the one used when setting: a cookie is identified by
    // name + domain + path.
    let access = Cookie::build(("access_token", ""))
        .path("/")
        .secure(config.cookie_secure)
        .http_only(true)
        .same_site(SameSite::Strict)
        .build();

    let refresh = Cookie::build(("refresh_token", ""))
        .path("/api/v1/auth")
        .secure(config.cookie_secure)
        .http_only(true)
        .same_site(SameSite::Strict)
        .build();

    jar.remove(access).remove(refresh)
}

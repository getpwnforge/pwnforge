// crates/api/src/routes/auth.rs
use crate::config::Config;
use crate::middleware::auth::AuthUser;
use crate::middleware::client_ip::ClientIp;
use crate::services::jwt_service;
use crate::services::rate_limit_service::{self, RateLimitDecision};
use crate::{error::AppError, services::auth_service, state::AppState};
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, header};
use axum::{Json, Router, extract::State, http::StatusCode, routing::get, routing::post};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use domain::dto::auth::{LoginRequest, RegisterRequest, UserResponse};
use sea_orm::prelude::IpNetwork;
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
}

async fn register(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<UserResponse>), AppError> {
    payload.validate()?;
    let registered = auth_service::register(&state.db, &state.config, payload).await?;
    Ok((StatusCode::CREATED, Json(UserResponse::from(registered))))
}

async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    ClientIp(client_ip): ClientIp,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    Json(payload): Json<LoginRequest>,
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

    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.chars().take(512).collect::<String>());
    let ip_address = client_ip.map(IpNetwork::from);
    let logged_in = auth_service::login(
        &state.db,
        payload,
        &state.jwt_keys.encoding,
        user_agent,
        ip_address,
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
    let ip_address = client_ip.map(IpNetwork::from);
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.chars().take(512).collect::<String>());
    let refreshed = auth_service::refresh(
        &state.db,
        &state.jwt_keys.encoding,
        &refresh_token,
        user_agent,
        ip_address,
    )
    .await?;

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
    let (model, email) = auth_service::current_user(&state.db, user.id).await?;
    Ok(Json(UserResponse::new(model, email)))
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

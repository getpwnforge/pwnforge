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
use domain::dto::error_responses::{
    EmailNotVerifiedErrorResponse, RateLimitedErrorResponse, SimpleErrorResponse,
    UnprocessableErrorResponse,
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

/// Creates an account without opening a session.
///
/// Returns 201 with the user, but no cookies: the address must be verified
/// before the account can sign in. A verification mail is sent on a best
/// effort basis, its failure does not undo the registration.
#[utoipa::path(
    post,
    path = "/api/v1/auth/register",
    tag = "Auth",
    request_body = RegisterRequest,
    responses(
        (status = 201, description = "Account created", body = UserResponse),
        (status = 400, description = "malformed_json or invalid_body: the body is not JSON, or could not be read; or turnstile_failed", body = SimpleErrorResponse),
        (status = 403, description = "Public signup is disabled", body = SimpleErrorResponse),
        (status = 409, description = "Username or email already taken, or username reserved", body = SimpleErrorResponse),
        (status = 415, description = "unsupported_media_type: Content-Type is not application/json", body = SimpleErrorResponse),
        (status = 422, description = "validation_failed, invalid_body, invalid_email, email_domain_not_allowed or password_compromised", body = UnprocessableErrorResponse),
        (status = 429, description = "Rate limited", body = RateLimitedErrorResponse),
        (status = 503, description = "Rate limit unavailable", body = SimpleErrorResponse),
    )
)]
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
    // IP that Turnstile verifies against, nor let a caller dodge the limit.
    let rate_limit_ip = client_ip.unwrap_or_else(|| remote.ip());

    // Checked after validation, like password/forgot: a mistyped form must not
    // burn one of the five hourly attempts and lock the visitor out.
    match rate_limit_service::check_register_attempt(&mut state.redis.clone(), rate_limit_ip).await
    {
        Ok(RateLimitDecision::Limited { retry_after_secs }) => {
            return Err(AppError::RateLimited { retry_after_secs });
        }
        Ok(RateLimitDecision::Allowed) => {}
        Err(err) => {
            // Refuse rather than fail open, same rule as password/forgot: there
            // is no password or token underneath this route, and every request
            // that gets through mails an address the caller never proved they
            // own. An unlimited signup endpoint is a free mailer plus unbounded
            // account creation, so a missing limiter must close the door.
            tracing::error!(error = ?err, "register rate limit unavailable, refusing");
            return Err(AppError::ServiceUnavailable);
        }
    }

    let ctx = session_context(&headers, client_ip);
    let registered =
        auth_service::register(&state.db, &state.config, payload, ctx, rate_limit_ip).await?;
    Ok((StatusCode::CREATED, Json(UserResponse::from(registered))))
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    tag = "Auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Signed in", body = UserResponse),
        (status = 400, description = "malformed_json or invalid_body: the body is not JSON, or could not be read; or turnstile_failed", body = SimpleErrorResponse),
        (status = 401, description = "Invalid credentials", body = SimpleErrorResponse),
        (status = 403, description = "Email not verified", body = EmailNotVerifiedErrorResponse),
        (status = 415, description = "unsupported_media_type: Content-Type is not application/json", body = SimpleErrorResponse),
        (status = 422, description = "invalid_body: the body is JSON of the wrong shape", body = UnprocessableErrorResponse),
        (status = 429, description = "Rate limited", body = RateLimitedErrorResponse),
    )
)]
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

/// Rotates the session and issues a fresh access token.
///
/// Consumes the `refresh_token` cookie and replaces it. The presented token
/// is revoked on every rotation, and presenting it again is treated as a
/// replay: the whole session chain is then revoked.
#[utoipa::path(
    post,
    path = "/api/v1/auth/refresh",
    tag = "Auth",
    security(("refresh_token" = [])),
    responses(
        (status = 200, description = "Tokens refreshed", body = UserResponse),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
    )
)]
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

#[utoipa::path(
    post,
    path = "/api/v1/auth/logout",
    tag = "Auth",
    security(("refresh_token" = [])),
    responses(
        (status = 204, description = "Logged out: both cookies are cleared in every case, and the refresh token revoked when the server knows it"),
    )
)]
async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<(CookieJar, StatusCode), AppError> {
    // The cookies go whatever the revocation does: an error here used to
    // leave both in the browser, the access token usable for the rest of its
    // lifetime by whoever used that browser next. The revocation is best
    // effort, and a token the server no longer knows is already logged out.
    if let Some(token) = jar.get("refresh_token")
        && let Err(err) = auth_service::logout(&state.db, token.value()).await
    {
        tracing::warn!(error = ?err, "logout could not revoke the refresh token");
    }

    Ok((
        clear_auth_cookies(jar, &state.config),
        StatusCode::NO_CONTENT,
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/auth/me",
    tag = "Auth",
    security(("access_token" = [])),
    responses(
        (status = 200, description = "Authenticated user", body = UserResponse),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
    )
)]
async fn me(State(state): State<AppState>, user: AuthUser) -> Result<Json<UserResponse>, AppError> {
    let (model, email) = auth_service::fetch_user(&state.db, user.id).await?;
    Ok(Json(UserResponse::new(model, email)))
}

/// Sends a password reset link.
///
/// Always answers 202, whether or not the address belongs to an account:
/// a different answer would turn this route into a way to test which
/// addresses are registered.
#[utoipa::path(
    post,
    path = "/api/v1/auth/password/forgot",
    tag = "Auth",
    request_body = PasswordForgotRequest,
    responses(
        (status = 202, description = "Password reset email sent"),
        (status = 400, description = "malformed_json or invalid_body: the body is not JSON, or could not be read; or turnstile_failed", body = SimpleErrorResponse),
        (status = 415, description = "unsupported_media_type: Content-Type is not application/json", body = SimpleErrorResponse),
        (status = 422, description = "validation_failed or invalid_body", body = UnprocessableErrorResponse),
        (status = 429, description = "Rate limited", body = RateLimitedErrorResponse),
    )
)]
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

#[utoipa::path(
    post,
    path = "/api/v1/auth/password/reset",
    tag = "Auth",
    request_body = PasswordResetRequest,
    responses(
        (status = 204, description = "Password reset"),
        (status = 400, description = "malformed_json or invalid_body: the body is not JSON, or could not be read", body = SimpleErrorResponse),
        (status = 415, description = "unsupported_media_type: Content-Type is not application/json", body = SimpleErrorResponse),
        (status = 422, description = "validation_failed, invalid_body, password_compromised, token_invalid, token_expired or token_consumed", body = UnprocessableErrorResponse),
        (status = 429, description = "Rate limited", body = RateLimitedErrorResponse),
    )
)]
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

#[utoipa::path(
    post,
    path = "/api/v1/auth/password/change",
    tag = "Auth",
    request_body = PasswordChangeRequest,
    security(("access_token" = [])),
    responses(
        (status = 204, description = "Password changed"),
        (status = 400, description = "malformed_json or invalid_body: the body is not JSON, or could not be read", body = SimpleErrorResponse),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 415, description = "unsupported_media_type: Content-Type is not application/json", body = SimpleErrorResponse),
        (status = 422, description = "validation_failed, invalid_body, password_compromised or password_unchanged", body = UnprocessableErrorResponse),
        (status = 429, description = "Rate limited", body = RateLimitedErrorResponse),
    )
)]
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

/// Marks an email address as verified.
///
/// POST rather than GET: mail clients and spam scanners prefetch links, and a
/// GET would let them burn the token before the user clicks. The frontend
/// page reads the token from the URL and posts it.
#[utoipa::path(
    post,
    path = "/api/v1/auth/email/verify",
    tag = "Auth",
    request_body = EmailVerifyRequest,
    responses(
        (status = 204, description = "Email verified"),
        (status = 400, description = "malformed_json or invalid_body: the body is not JSON, or could not be read", body = SimpleErrorResponse),
        (status = 415, description = "unsupported_media_type: Content-Type is not application/json", body = SimpleErrorResponse),
        (status = 422, description = "validation_failed, invalid_body, token_invalid, token_expired or token_consumed", body = UnprocessableErrorResponse),
        (status = 429, description = "Rate limited", body = RateLimitedErrorResponse),
    )
)]
async fn email_verify(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    JsonBody(payload): JsonBody<EmailVerifyRequest>,
) -> Result<StatusCode, AppError> {
    payload.validate()?;

    // Fails open, like password/reset: the 256-bit token underneath is what
    // stops a guess, this only bounds the lookups.
    match rate_limit_service::check_email_verify_ip(
        &mut state.redis.clone(),
        ip.unwrap_or_else(|| remote.ip()),
    )
    .await
    {
        Ok(RateLimitDecision::Limited { retry_after_secs }) => {
            return Err(AppError::RateLimited { retry_after_secs });
        }
        Ok(RateLimitDecision::Allowed) => {}
        Err(err) => {
            tracing::error!(error = ?err, "email verify rate limit unavailable, allowing request");
        }
    }

    auth_service::verify_email(&state.db, &payload.token).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Sends a new verification link.
///
/// Takes the opaque token returned alongside an `email_not_verified`
/// failure, not an email address: the caller has already proven it holds a
/// pending verification for that address.
#[utoipa::path(
    post,
    path = "/api/v1/auth/email/resend",
    tag = "Auth",
    request_body = EmailResendRequest,
    responses(
        (status = 202, description = "Verification email resent"),
        (status = 400, description = "malformed_json or invalid_body: the body is not JSON, or could not be read", body = SimpleErrorResponse),
        (status = 415, description = "unsupported_media_type: Content-Type is not application/json", body = SimpleErrorResponse),
        (status = 422, description = "validation_failed, invalid_body or token_invalid", body = UnprocessableErrorResponse),
        (status = 429, description = "Rate limited", body = RateLimitedErrorResponse),
    )
)]
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

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::{HeaderMap, Request, StatusCode, header};
    use domain::entities::{auth_tokens, user_emails, users};
    use redis::AsyncCommands;
    use sea_orm::{
        ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
    };
    use serde_json::{Value, json};
    use std::net::{IpAddr, Ipv6Addr, SocketAddr};
    use tower::ServiceExt;
    use uuid::Uuid;

    use crate::test_support::{app, app_with_config, config, db, env_or};

    const PASSWORD: &str = "a-long-test-password-1!";

    /// A documentation address (RFC 3849) of its own per call, so no two
    /// tests share a rate-limit bucket.
    fn fresh_ip() -> IpAddr {
        let id = Uuid::now_v7().as_u128();
        IpAddr::V6(Ipv6Addr::new(
            0x2001,
            0x0db8,
            0x0a11,
            (id >> 64) as u16,
            (id >> 48) as u16,
            (id >> 32) as u16,
            (id >> 16) as u16,
            id as u16,
        ))
    }

    async fn redis() -> redis::aio::ConnectionManager {
        let client = redis::Client::open(env_or("REDIS_URL", "redis://127.0.0.1:6379"))
            .expect("REDIS_URL must be valid");
        redis::aio::ConnectionManager::new(client)
            .await
            .expect("a Redis instance must be reachable")
    }

    /// An account with `PASSWORD`, signing in by its username, and its
    /// primary address, verified or not.
    async fn account(db: &DatabaseConnection, verified: bool) -> (Uuid, String, String) {
        let id = Uuid::now_v7();
        let username = format!("a{}", &id.simple().to_string()[1..]);
        let address = format!("{username}@example.test");
        let hash = services::password_service::hash_password(PASSWORD.to_owned())
            .await
            .expect("the password must hash");
        users::ActiveModel {
            id: Set(id),
            username: Set(username.clone()),
            password_hash: Set(hash),
            locale: Set("en".to_owned()),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("the fixture user must insert");
        user_emails::ActiveModel {
            id: Set(Uuid::now_v7()),
            user_id: Set(id),
            email: Set(address.clone()),
            is_primary: Set(true),
            verified_at: Set(verified.then(|| chrono::Utc::now().into())),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("the fixture address must insert");
        (id, username, address)
    }

    async fn post(
        app: &axum::Router,
        uri: &str,
        body: Value,
        ip: IpAddr,
        cookie: Option<&str>,
    ) -> (StatusCode, HeaderMap, Value) {
        let mut request = Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json");
        if let Some(cookie) = cookie {
            request = request.header("cookie", cookie);
        }
        let mut request = request
            .body(Body::from(body.to_string()))
            .expect("request must build");
        request
            .extensions_mut()
            .insert(ConnectInfo(SocketAddr::new(ip, 0)));

        let response = app
            .clone()
            .oneshot(request)
            .await
            .expect("router must answer");
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("the body must be readable");
        let value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).expect("the body must be JSON")
        };
        (status, headers, value)
    }

    fn login_body(identifier: &str, password: &str, turnstile: &str) -> Value {
        json!({ "username_or_email": identifier, "password": password, "turnstile_token": turnstile })
    }

    async fn verification_tokens(db: &DatabaseConnection, user: Uuid) -> Vec<auth_tokens::Model> {
        auth_tokens::Entity::find()
            .filter(auth_tokens::Column::UserId.eq(user))
            .filter(auth_tokens::Column::Kind.eq("email_verification"))
            .all(db)
            .await
            .expect("the lookup must succeed")
    }

    /// Signing in on an unverified address answers with a handle that asks
    /// for another verification mail and nothing else: it does not verify
    /// the address, so knowing the password is not knowing the mailbox.
    #[tokio::test]
    async fn the_handle_from_an_unverified_sign_in_resends_but_never_verifies() {
        let db = db().await;
        let app = app(&db).await;
        let (user, username, _) = account(&db, false).await;
        let ip = fresh_ip();

        let (status, _, body) = post(
            &app,
            "/api/v1/auth/login",
            login_body(&username, PASSWORD, ""),
            ip,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["error"], "email_not_verified");
        let handle = body["resend_token"].as_str().expect("a handle").to_owned();

        let (status, _, body) = post(
            &app,
            "/api/v1/auth/email/verify",
            json!({ "token": handle }),
            ip,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert_eq!(body["error"], "token_invalid");
        let address = user_emails::Entity::find()
            .filter(user_emails::Column::UserId.eq(user))
            .one(&db)
            .await
            .expect("the lookup must succeed")
            .expect("the address exists");
        assert!(
            address.verified_at.is_none(),
            "the address stays unverified"
        );

        assert!(verification_tokens(&db, user).await.is_empty());
        let (status, _, _) = post(
            &app,
            "/api/v1/auth/email/resend",
            json!({ "token": handle }),
            ip,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::ACCEPTED);
        assert_eq!(
            verification_tokens(&db, user).await.len(),
            1,
            "the resend issued a verification link, for the mail alone"
        );
    }

    /// Logging out clears both cookies even when the refresh token cannot be
    /// revoked, here because the server does not know it.
    #[tokio::test]
    async fn logging_out_always_clears_the_cookies() {
        let db = db().await;
        let app = app(&db).await;

        let (status, headers, _) = post(
            &app,
            "/api/v1/auth/logout",
            Value::Null,
            fresh_ip(),
            Some("refresh_token=not-a-token-the-server-knows; access_token=whatever"),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let cleared: Vec<String> = headers
            .get_all(header::SET_COOKIE)
            .iter()
            .map(|value| value.to_str().expect("ascii").to_owned())
            .collect();
        for name in ["access_token=", "refresh_token="] {
            assert!(
                cleared
                    .iter()
                    .any(|cookie| cookie.starts_with(name) && cookie.contains("Max-Age=0")),
                "{name} must be cleared, got {cleared:?}"
            );
        }
    }

    /// A made-up reset token is refused before the breach check runs: a
    /// compromised password with an unknown token is answered `token_invalid`.
    #[tokio::test]
    async fn an_unknown_reset_token_is_refused_before_the_breach_check() {
        let db = db().await;
        let mut config = config();
        config.check_pwned_passwords = true;
        let app = app_with_config(&db, config).await;

        let (status, _, body) = post(
            &app,
            "/api/v1/auth/password/reset",
            json!({ "token": "no-such-reset-token", "new_password": "Password123!" }),
            fresh_ip(),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["error"], "token_invalid");
    }

    /// Verification is limited per IP.
    #[tokio::test]
    async fn email_verification_is_limited_per_ip() {
        let db = db().await;
        let app = app(&db).await;
        let mut redis = redis().await;
        let ip = fresh_ip();
        let key = format!("ratelimit:verify:ip:{ip}");
        let _: () = redis.set_ex(&key, 30, 3600).await.expect("SET");

        let (status, _, _) = post(
            &app,
            "/api/v1/auth/email/verify",
            json!({ "token": "any-token" }),
            ip,
            None,
        )
        .await;
        let _: () = redis.del(&key).await.expect("DEL");
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    }

    /// With Turnstile on, a sign-in that carries no token is refused with
    /// 400 turnstile_failed. The refusal is local: the secret below is not a
    /// real key, and no call to Cloudflare is needed to reach the answer.
    #[tokio::test]
    async fn an_empty_turnstile_token_is_refused_when_turnstile_is_on() {
        let db = db().await;
        let mut config = config();
        config.turnstile_site_key = Some("test-site-key".to_owned());
        config.turnstile_secret_key = Some("test-secret-key-never-sent".to_owned());
        let app = app_with_config(&db, config).await;

        let body = serde_json::json!({
            "username_or_email": format!("nobody-{}", Uuid::now_v7().simple()),
            "password": "long-enough-password",
            "turnstile_token": "",
        });
        let mut request = Request::builder()
            .method("POST")
            .uri("/api/v1/auth/login")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .expect("request must build");
        // The rate limiter reads the peer address; the router normally gets it
        // from the listener.
        request
            .extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))));

        let response = app.oneshot(request).await.expect("router must answer");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("the body must be readable");
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).expect("the body must be JSON");
        assert_eq!(value["error"], "turnstile_failed");
    }
}

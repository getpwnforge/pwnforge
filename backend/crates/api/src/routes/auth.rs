// crates/api/src/routes/auth.rs
use crate::middleware::{
    address::require_mailbox, auth::AuthUser, client_ip::ClientIp, json::JsonBody, session_context,
};
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

/// Creates an account without opening a session, or does nothing visible.
///
/// Answers 202 with no body and no cookie, the same whether the address was
/// free, held by a verified account or held by a registration nobody
/// confirmed: registration never says whether an address has an account
/// (see `auth_service::register`). The address gets the mail: the
/// verification link of the new account, or a notice to its holder.
#[utoipa::path(
    post,
    path = "/api/v1/auth/register",
    tag = "Auth",
    request_body = RegisterRequest,
    responses(
        (status = 202, description = "Accepted, with no body: the same answer whether the address was free or already had an account"),
        (status = 400, description = "malformed_json or invalid_body: the body is not JSON, or could not be read; or turnstile_failed", body = SimpleErrorResponse),
        (status = 403, description = "public_signup_disabled: public signup is disabled, or the instance is not set up yet", body = SimpleErrorResponse),
        (status = 409, description = "username_taken or username_reserved, checked before anything about the address", body = SimpleErrorResponse),
        (status = 415, description = "unsupported_media_type: Content-Type is not application/json", body = SimpleErrorResponse),
        (status = 422, description = "validation_failed, invalid_body, invalid_email, email_domain_not_allowed or password_compromised; an address no mail can go to (a..b@example.test, a@[127.0.0.1]) is validation_failed, as a malformed one", body = UnprocessableErrorResponse),
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
) -> Result<StatusCode, AppError> {
    payload.validate()?;
    // On the form alone, before anything looks the address up: the answer
    // is the same whether an account holds it or not.
    require_mailbox("email", &payload.email)?;
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
    auth_service::register(
        &state.db,
        &state.redis,
        &state.config,
        &state.background,
        payload,
        ctx,
        rate_limit_ip,
    )
    .await?;
    Ok(StatusCode::ACCEPTED)
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
    let mut redis = state.redis.clone();
    // The IP window first, on every attempt. The identifier's window of
    // failed passwords is the service's, after Turnstile.
    match rate_limit_service::check_login_ip(&mut redis, rate_limit_ip).await {
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
        &mut redis,
        &state.config,
        payload,
        &state.jwt_keys.encoding,
        ctx,
        rate_limit_ip,
    )
    .await?;

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
        (status = 422, description = "validation_failed or invalid_body; an address no mail can go to (a..b@example.test, a@[127.0.0.1]) is validation_failed, as a malformed one", body = UnprocessableErrorResponse),
        (status = 429, description = "Rate limited: per IP first, then, past Turnstile, per address", body = RateLimitedErrorResponse),
        (status = 503, description = "service_unavailable: a rate limiter cannot be reached", body = SimpleErrorResponse),
    )
)]
async fn password_forgot(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    JsonBody(payload): JsonBody<PasswordForgotRequest>,
) -> Result<StatusCode, AppError> {
    payload.validate()?;
    // On the form alone, as at registration.
    require_mailbox("email", &payload.email)?;

    let rate_limit_ip = ip.unwrap_or_else(|| remote.ip());
    let mut redis = state.redis.clone();

    // The IP window first; the address's comes after Turnstile, in the
    // service, so a failed challenge does not spend it.
    match rate_limit_service::check_password_forgot_ip(&mut redis, rate_limit_ip).await {
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
        &mut redis,
        &state.config,
        &state.background,
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
        (status = 429, description = "Rate limited: 10 attempts an hour per account", body = RateLimitedErrorResponse),
        (status = 503, description = "service_unavailable: the rate limiter cannot be reached", body = SimpleErrorResponse),
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

    // Per account, before the Argon2 verification it bounds. Fails closed,
    // as the avatar upload does: any account reaches this route, and nothing
    // else bounds the work.
    match rate_limit_service::check_password_change(&mut state.redis.clone(), user.id).await {
        Ok(RateLimitDecision::Limited { retry_after_secs }) => {
            return Err(AppError::RateLimited { retry_after_secs });
        }
        Ok(RateLimitDecision::Allowed) => {}
        Err(err) => {
            tracing::error!(error = ?err, "password change rate limit unavailable, refusing");
            return Err(AppError::ServiceUnavailable);
        }
    }

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
/// Takes the handle returned alongside an `email_not_verified` failure, or
/// the token of an earlier verification link, not an email address: the
/// caller has already proven it holds the password or the mailbox. The new
/// link goes to the address on file.
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
    ClientIp(ip): ClientIp,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    JsonBody(payload): JsonBody<EmailResendRequest>,
) -> Result<StatusCode, AppError> {
    payload.validate()?;
    let mut redis = state.redis.clone();
    // 1. Per IP first: the token windows below key on whatever the caller
    //    sends, so on their own every made-up token got fresh ones.
    match rate_limit_service::check_email_resend_ip(&mut redis, ip.unwrap_or_else(|| remote.ip()))
        .await
    {
        Ok(RateLimitDecision::Limited { retry_after_secs }) => {
            return Err(AppError::RateLimited { retry_after_secs });
        }
        Ok(RateLimitDecision::Allowed) => {}
        Err(err) => {
            tracing::error!(error = ?err, "email resend rate limit unavailable, allowing request");
        }
    }
    // 2. Then keyed on the opaque token: 1 per 60s, 5 per hour.
    match rate_limit_service::check_email_resend(&mut redis, &payload.token).await {
        Ok(RateLimitDecision::Limited { retry_after_secs }) => {
            return Err(AppError::RateLimited { retry_after_secs });
        }
        Ok(RateLimitDecision::Allowed) => {}
        Err(err) => {
            tracing::error!(error = ?err, "email resend rate limit unavailable, allowing request");
        }
    }
    // 3. auth_service::resend_verification.
    auth_service::resend_verification(&state.db, &state.config, &payload.token).await?;
    // 4. 202.
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
    use services::token_service::hash_opaque_token;
    use std::net::{IpAddr, Ipv6Addr, SocketAddr};
    use tower::ServiceExt;
    use uuid::Uuid;

    use crate::test_support::{app, app_with_config, config, cookie_for, db, env_or, unique};

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

    /// An account, verified, holding `address`: one registered before an
    /// address had to make a mailbox.
    async fn account_holding(db: &DatabaseConnection, address: &str) -> Uuid {
        let (id, _, _) = account(db, true).await;
        user_emails::Entity::update_many()
            .col_expr(
                user_emails::Column::Email,
                sea_orm::sea_query::Expr::value(address),
            )
            .filter(user_emails::Column::UserId.eq(id))
            .exec(db)
            .await
            .expect("the address must be set");
        id
    }

    /// Two shapes of address the request's `email` rule takes and no mail
    /// can be addressed to, each made unique by `tag`: an empty atom, and a
    /// domain literal, which the mailbox parser refuses.
    fn undeliverable(tag: &str) -> [String; 2] {
        [
            format!("{tag}..b@example.test"),
            format!("{tag}@[127.0.0.1]"),
        ]
    }

    fn refused_as_malformed(field: &str) -> Value {
        let mut fields = serde_json::Map::new();
        fields.insert(field.to_owned(), json!(["email"]));
        json!({ "error": "validation_failed", "fields": fields })
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

    fn identifier_key(identifier: &str) -> String {
        format!(
            "ratelimit:login:identifier:{}",
            hash_opaque_token(&identifier.trim().to_lowercase())
        )
    }

    fn turnstile_on() -> services::config::Config {
        let mut config = config();
        config.turnstile_site_key = Some("test-site-key".to_owned());
        config.turnstile_secret_key = Some("test-secret-key-never-sent".to_owned());
        config
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

    /// The identifier's window counts failed passwords, and only those past
    /// Turnstile: requests refused by the challenge leave it empty, while the
    /// IP window, checked first, counts every one.
    #[tokio::test]
    async fn only_failed_passwords_past_turnstile_count_against_the_identifier() {
        let db = db().await;
        let app = app_with_config(&db, turnstile_on()).await;
        let mut redis = redis().await;
        let (_, username, _) = account(&db, true).await;
        let ip = fresh_ip();

        for _ in 0..12 {
            let (status, _, body) = post(
                &app,
                "/api/v1/auth/login",
                login_body(&username, "wrong-password-12", ""),
                ip,
                None,
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST);
            assert_eq!(body["error"], "turnstile_failed");
        }
        let counted: Option<i64> = redis.get(identifier_key(&username)).await.expect("GET");
        assert_eq!(
            counted, None,
            "a request refused by Turnstile is not a failed password"
        );
        let by_ip: Option<i64> = redis
            .get(format!("ratelimit:login:ip:{ip}"))
            .await
            .expect("GET");
        assert_eq!(by_ip, Some(12));
    }

    /// Ten failed passwords block the identifier, the right password
    /// included, until the window ends; a success before that clears it.
    #[tokio::test]
    async fn ten_failed_passwords_block_the_identifier_and_a_success_clears_it() {
        let db = db().await;
        let app = app(&db).await;
        let mut redis = redis().await;
        let (_, username, _) = account(&db, true).await;

        for _ in 0..3 {
            let (status, _, _) = post(
                &app,
                "/api/v1/auth/login",
                login_body(&username, "wrong-password-12", ""),
                fresh_ip(),
                None,
            )
            .await;
            assert_eq!(status, StatusCode::UNAUTHORIZED);
        }
        let (status, _, _) = post(
            &app,
            "/api/v1/auth/login",
            login_body(&username, PASSWORD, ""),
            fresh_ip(),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let counted: Option<i64> = redis.get(identifier_key(&username)).await.expect("GET");
        assert_eq!(counted, None, "a success clears the failures");

        for _ in 0..10 {
            let (status, _, _) = post(
                &app,
                "/api/v1/auth/login",
                login_body(&username, "wrong-password-12", ""),
                fresh_ip(),
                None,
            )
            .await;
            assert_eq!(status, StatusCode::UNAUTHORIZED);
        }
        let (status, headers, body) = post(
            &app,
            "/api/v1/auth/login",
            login_body(&username, PASSWORD, ""),
            fresh_ip(),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
        assert_eq!(body["error"], "rate_limited");
        assert!(headers.contains_key(header::RETRY_AFTER));
        let _: () = redis.del(identifier_key(&username)).await.expect("DEL");
    }

    /// The IP window comes before anything else, Turnstile included.
    #[tokio::test]
    async fn the_login_ip_window_comes_first() {
        let db = db().await;
        let app = app_with_config(&db, turnstile_on()).await;
        let mut redis = redis().await;
        let ip = fresh_ip();
        let key = format!("ratelimit:login:ip:{ip}");
        let _: () = redis.set_ex(&key, 30, 900).await.expect("SET");

        let (status, _, _) = post(
            &app,
            "/api/v1/auth/login",
            login_body("anyone", PASSWORD, ""),
            ip,
            None,
        )
        .await;
        let _: () = redis.del(&key).await.expect("DEL");
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    }

    /// A reset request refused by Turnstile counts against the IP, never
    /// against the address it names; past the challenge, the address window
    /// fails closed.
    #[tokio::test]
    async fn the_address_window_of_a_reset_request_comes_after_turnstile() {
        let db = db().await;
        let mut redis = redis().await;
        let address = format!("{}@example.test", Uuid::now_v7().simple());
        let email_key = format!(
            "ratelimit:forgot:email:{}",
            hash_opaque_token(&address.to_lowercase())
        );
        let ip = fresh_ip();

        let guarded = app_with_config(&db, turnstile_on()).await;
        for _ in 0..4 {
            let (status, _, _) = post(
                &guarded,
                "/api/v1/auth/password/forgot",
                json!({ "email": address, "turnstile_token": "" }),
                ip,
                None,
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }
        let by_address: Option<i64> = redis.get(&email_key).await.expect("GET");
        assert_eq!(
            by_address, None,
            "a failed challenge spends nothing of the address"
        );
        let by_ip: Option<i64> = redis
            .get(format!("ratelimit:forgot:ip:{ip}"))
            .await
            .expect("GET");
        assert_eq!(by_ip, Some(4));

        let open = app(&db).await;
        let _: () = redis.set(&email_key, "not-a-counter").await.expect("SET");
        let (status, _, body) = post(
            &open,
            "/api/v1/auth/password/forgot",
            json!({ "email": address, "turnstile_token": "" }),
            fresh_ip(),
            None,
        )
        .await;
        let _: () = redis.del(&email_key).await.expect("DEL");
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    }

    /// A reset request for an address no mail can go to is refused on its
    /// form alone: the same answer whether an account holds it or not, and
    /// no link is issued.
    #[tokio::test]
    async fn a_reset_request_for_an_address_no_mail_can_go_to_is_refused_on_its_form() {
        let db = db().await;
        let app = app(&db).await;
        let held = undeliverable(&unique("held"));
        let free = undeliverable(&unique("free"));

        for (held, free) in held.iter().zip(&free) {
            let holder = account_holding(&db, held).await;
            let mut answers = Vec::new();
            for address in [held, free] {
                let (status, _, body) = post(
                    &app,
                    "/api/v1/auth/password/forgot",
                    json!({ "email": address, "turnstile_token": "" }),
                    fresh_ip(),
                    None,
                )
                .await;
                assert_eq!(
                    status,
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "{address}: {body}"
                );
                answers.push(body);
            }
            assert_eq!(answers[0], refused_as_malformed("email"), "{held}");
            assert_eq!(answers[0], answers[1], "the same answer, taken or free");
            let links = auth_tokens::Entity::find()
                .filter(auth_tokens::Column::UserId.eq(holder))
                .all(&db)
                .await
                .expect("the lookup must succeed");
            assert!(links.is_empty(), "no link is issued");
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

    /// The resend IP window comes before the token windows: a refused
    /// request spends nothing of the token's.
    #[tokio::test]
    async fn the_resend_ip_window_comes_before_the_token_windows() {
        let db = db().await;
        let app = app(&db).await;
        let mut redis = redis().await;
        let ip = fresh_ip();
        let key = format!("ratelimit:resend:ip:{ip}");
        let token = format!("made-up-{}", Uuid::now_v7().simple());
        let _: () = redis.set_ex(&key, 20, 3600).await.expect("SET");

        let (status, _, _) = post(
            &app,
            "/api/v1/auth/email/resend",
            json!({ "token": token }),
            ip,
            None,
        )
        .await;
        let _: () = redis.del(&key).await.expect("DEL");
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        let by_token: Option<i64> = redis
            .get(format!(
                "ratelimit:resend:token:{}",
                hash_opaque_token(&token)
            ))
            .await
            .expect("GET");
        assert_eq!(by_token, None);
    }

    /// Password changes are limited per account, and the limit fails closed.
    #[tokio::test]
    async fn password_changes_are_limited_per_account_and_fail_closed() {
        let db = db().await;
        let app = app(&db).await;
        let mut redis = redis().await;
        let (user, _, _) = account(&db, true).await;
        let key = format!("ratelimit:password_change:user:{user}");
        let body =
            json!({ "current_password": "wrong-password-12", "new_password": "Another-pass-34!" });
        let cookie = cookie_for(user);

        let _: () = redis.set_ex(&key, 10, 3600).await.expect("SET");
        let (limited, _, _) = post(
            &app,
            "/api/v1/auth/password/change",
            body.clone(),
            fresh_ip(),
            Some(&cookie),
        )
        .await;
        let _: () = redis.set(&key, "not-a-counter").await.expect("SET");
        let (unavailable, _, _) = post(
            &app,
            "/api/v1/auth/password/change",
            body,
            fresh_ip(),
            Some(&cookie),
        )
        .await;
        let _: () = redis.del(&key).await.expect("DEL");

        assert_eq!(limited, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(unavailable, StatusCode::SERVICE_UNAVAILABLE);
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

    // ---- Registration ---------------------------------------------------

    /// Registration never says whether an address has an account: the same
    /// answer, the same work and the same timing whether the address is free,
    /// held by a verified account or held by a registration nobody
    /// confirmed, and only the address learns which, by mail. Every test runs
    /// in a throwaway database: the shared one has not run its setup, and
    /// registration is closed until it has.
    mod registration {
        use super::*;
        use crate::test_support::{MailSink, SentMail, in_scratch_database};
        use domain::legal::{CURRENT_PRIVACY_VERSION, CURRENT_TERMS_VERSION};
        use sea_orm::PaginatorTrait;
        use std::time::{Duration, Instant};

        fn unique_name() -> String {
            format!("r{}", &Uuid::now_v7().simple().to_string()[1..])
        }

        fn free_address() -> String {
            format!("{}@example.test", unique_name())
        }

        fn registration(username: &str, email: &str) -> Value {
            json!({
                "username": username,
                "email": email,
                "password": PASSWORD,
                "legal": {
                    "terms_version": CURRENT_TERMS_VERSION,
                    "privacy_version": CURRENT_PRIVACY_VERSION,
                },
                "turnstile_token": "",
            })
        }

        /// The whole answer to a registration, from an IP of its own: status,
        /// headers and the body as bytes.
        async fn register(app: &axum::Router, body: Value) -> (StatusCode, HeaderMap, Vec<u8>) {
            let mut request = Request::builder()
                .method("POST")
                .uri("/api/v1/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .expect("request must build");
            request
                .extensions_mut()
                .insert(ConnectInfo(SocketAddr::new(fresh_ip(), 0)));
            let response = app
                .clone()
                .oneshot(request)
                .await
                .expect("router must answer");
            let status = response.status();
            let headers = response.headers().clone();
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("the body must be readable");
            (status, headers, body.to_vec())
        }

        /// An address no mail can go to is refused on its form alone, before
        /// anything else runs: the same status, headers and body whether an
        /// account holds it or not, and no account, no mail.
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn an_address_no_mail_can_go_to_is_refused_whether_taken_or_free() {
            in_scratch_database(|db| async move {
                let sink = MailSink::start().await;
                let app = app_with_config(&db, sink.config()).await;
                let held = undeliverable(&unique_name());
                let free = undeliverable(&unique_name());

                for (held, free) in held.iter().zip(&free) {
                    account_holding(&db, held).await;
                    let mut answers = Vec::new();
                    for address in [held, free] {
                        let username = unique_name();
                        let (status, mut headers, body) =
                            register(&app, registration(&username, address)).await;
                        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{address}");
                        assert!(!exists(&db, &username).await, "{address}: no account");
                        headers.remove(header::DATE);
                        answers.push((headers, body));
                    }
                    let body: Value =
                        serde_json::from_slice(&answers[0].1).expect("the body must be JSON");
                    assert_eq!(body, refused_as_malformed("email"), "{held}");
                    assert_eq!(answers[0], answers[1], "the same answer, taken or free");
                    assert!(answers[0].0.get(header::SET_COOKIE).is_none(), "no cookie");
                }
                assert!(sink.settle().await.is_empty(), "no mail went out");
            })
            .await;
        }

        async fn exists(db: &DatabaseConnection, username: &str) -> bool {
            users::Entity::find()
                .filter(users::Column::Username.eq(username))
                .one(db)
                .await
                .expect("the lookup must succeed")
                .is_some()
        }

        fn to<'a>(mails: &'a [SentMail], address: &str) -> Vec<&'a SentMail> {
            mails
                .iter()
                .filter(|mail| mail.to.iter().any(|to| to == address))
                .collect()
        }

        /// The key the shared cap on unsolicited mail counts an address
        /// under, written out here so that a drift in its name shows.
        fn cap_key(address: &str) -> String {
            format!(
                "ratelimit:mail:address:{}",
                hash_opaque_token(&address.trim().to_lowercase())
            )
        }

        /// The same status, headers and body, and no cookie, whether the
        /// address is free, verified or pending; an account only for the
        /// free one.
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn the_answer_is_the_same_whether_the_address_is_free_or_taken() {
            in_scratch_database(|db| async move {
                let sink = MailSink::start().await;
                let app = app_with_config(&db, sink.config()).await;
                let (_, _, verified) = account(&db, true).await;
                let (_, _, pending) = account(&db, false).await;

                let mut answers = Vec::new();
                let mut usernames = Vec::new();
                for address in [free_address(), verified, pending] {
                    let username = unique_name();
                    answers.push(register(&app, registration(&username, &address)).await);
                    usernames.push(username);
                }

                let (status, headers, body) = &answers[0];
                assert_eq!(*status, StatusCode::ACCEPTED);
                assert!(body.is_empty(), "no body");
                assert!(headers.get(header::SET_COOKIE).is_none(), "no cookie");
                for answer in &answers[1..] {
                    assert_eq!(answer, &answers[0], "the same answer whatever the address");
                }

                assert!(
                    exists(&db, &usernames[0]).await,
                    "a free address gets its account"
                );
                assert!(
                    !exists(&db, &usernames[1]).await,
                    "a verified address gets none"
                );
                assert!(
                    !exists(&db, &usernames[2]).await,
                    "a pending address gets none"
                );

                sink.wait_for(3).await;
                db.close().await.ok();
            })
            .await;
        }

        /// The free address gets its verification link; a verified one, the
        /// way back in; a pending one, the date it is freed, and never a link
        /// that would confirm the account holding it.
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn each_address_gets_its_own_mail_and_nothing_else() {
            in_scratch_database(|db| async move {
                let sink = MailSink::start().await;
                let app = app_with_config(&db, sink.config()).await;
                let (verified_id, _, verified) = account(&db, true).await;
                let (pending_id, _, pending) = account(&db, false).await;
                let free = free_address();

                for address in [&free, &verified, &pending] {
                    register(&app, registration(&unique_name(), address)).await;
                }
                sink.wait_for(3).await;
                let mails = sink.settle().await;
                assert_eq!(mails.len(), 3, "one mail per registration, got {mails:#?}");

                let [verification] = to(&mails, &free)[..] else {
                    panic!("one mail to the free address, got {mails:#?}");
                };
                assert_eq!(verification.subject, "Confirm your email address");
                assert!(
                    verification
                        .text
                        .contains("http://localhost/auth/email/verify#token=")
                );

                let [notice] = to(&mails, &verified)[..] else {
                    panic!("one mail to the verified address, got {mails:#?}");
                };
                assert_eq!(
                    notice.subject,
                    "Someone tried to create an account with your address"
                );
                assert!(notice.text.contains("http://localhost/login"));
                assert!(notice.text.contains("http://localhost/forgot-password"));
                assert!(!notice.text.contains("token"), "no token: {}", notice.text);

                let created_at = users::Entity::find_by_id(pending_id)
                    .one(&db)
                    .await
                    .expect("the lookup must succeed")
                    .expect("the pending account is still there")
                    .created_at;
                let deleted_after = (created_at.with_timezone(&chrono::Utc)
                    + chrono::Duration::days(7))
                .format("%Y-%m-%d %H:%M UTC")
                .to_string();
                let [notice] = to(&mails, &pending)[..] else {
                    panic!("one mail to the pending address, got {mails:#?}");
                };
                assert_eq!(notice.subject, "A pending registration uses your address");
                assert!(
                    notice.text.contains(&deleted_after),
                    "the deletion date: {}",
                    notice.text
                );
                assert!(!notice.text.contains("token"), "no token: {}", notice.text);
                assert!(
                    !notice.text.contains("/auth/"),
                    "no auth link: {}",
                    notice.text
                );
                assert!(
                    !notice.text.contains("/login"),
                    "no sign-in link: {}",
                    notice.text
                );

                // Nothing was issued that could confirm, or open, either account.
                for holder in [verified_id, pending_id] {
                    let issued = auth_tokens::Entity::find()
                        .filter(auth_tokens::Column::UserId.eq(holder))
                        .count(&db)
                        .await
                        .expect("the count must succeed");
                    assert_eq!(issued, 0, "no token for the holder of a taken address");
                }

                db.close().await.ok();
            })
            .await;
        }

        /// The notice goes under the daily cap on unsolicited mail to one
        /// address, the one invitations count against: past it, or with the
        /// limiter failing, the answer is the same and no mail goes out.
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn the_daily_cap_holds_the_notice_back_and_fails_closed() {
            in_scratch_database(|db| async move {
                let sink = MailSink::start().await;
                let app = app_with_config(&db, sink.config()).await;
                let (_, _, capped) = account(&db, true).await;
                let (_, _, broken) = account(&db, true).await;
                let (_, _, control) = account(&db, true).await;
                let mut redis = super::redis().await;
                let _: () = redis
                    .set_ex(cap_key(&capped), 3, 3600)
                    .await
                    .expect("the cap must be filled");
                // A value INCR refuses: the limiter answers with an error.
                let _: () = redis
                    .set_ex(cap_key(&broken), "not-a-number", 3600)
                    .await
                    .expect("the cap must be broken");

                for address in [&capped, &broken, &control] {
                    let (status, _, _) =
                        register(&app, registration(&unique_name(), address)).await;
                    assert_eq!(
                        status,
                        StatusCode::ACCEPTED,
                        "the same answer for {address}"
                    );
                }
                sink.wait_for(1).await;
                let mails = sink.settle().await;

                let recipients: Vec<&str> = mails
                    .iter()
                    .flat_map(|mail| mail.to.iter().map(String::as_str))
                    .collect();
                assert_eq!(
                    recipients,
                    [control.as_str()],
                    "only the address under the cap gets mail"
                );

                db.close().await.ok();
            })
            .await;
        }

        /// A taken username is the one explicit refusal, and it comes first:
        /// the same `username_taken` whether its address is taken or free.
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn a_taken_username_is_refused_whatever_the_address() {
            in_scratch_database(|db| async move {
                let sink = MailSink::start().await;
                let app = app_with_config(&db, sink.config()).await;
                let (_, username, address) = account(&db, true).await;

                for email in [address, free_address()] {
                    let (status, _, body) = register(&app, registration(&username, &email)).await;
                    assert_eq!(status, StatusCode::CONFLICT, "for {email}");
                    let body: Value = serde_json::from_slice(&body).expect("the body must be JSON");
                    assert_eq!(body["error"], "username_taken", "for {email}");
                }
                assert!(
                    sink.settle().await.is_empty(),
                    "a refused registration mails nobody"
                );

                db.close().await.ok();
            })
            .await;
        }

        /// Two registrations racing for one free address: one account, and
        /// the same answer to both, the loser's taken address included.
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn two_registrations_racing_for_an_address_get_the_same_answer() {
            in_scratch_database(|db| async move {
                let sink = MailSink::start().await;
                let app = app_with_config(&db, sink.config()).await;
                let address = free_address();
                let (first, second) = (unique_name(), unique_name());

                let (a, b) = tokio::join!(
                    register(&app, registration(&first, &address)),
                    register(&app, registration(&second, &address)),
                );

                assert_eq!(a.0, StatusCode::ACCEPTED);
                assert_eq!(a, b, "the same answer to both");
                let holders = user_emails::Entity::find()
                    .filter(user_emails::Column::Email.eq(address.clone()))
                    .count(&db)
                    .await
                    .expect("the count must succeed");
                assert_eq!(holders, 1, "one account holds the address");
                assert!(
                    exists(&db, &first).await != exists(&db, &second).await,
                    "one account in all"
                );

                sink.wait_for(2).await;
                db.close().await.ok();
            })
            .await;
        }

        /// The mail goes out in a task of its own in every case: a mail server
        /// that never answers holds no registration up.
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn no_mail_holds_the_answer_up() {
            in_scratch_database(|db| async move {
                // Accepts the connection and never greets.
                let silent =
                    std::net::TcpListener::bind("127.0.0.1:0").expect("a port must be free");
                let mut config = MailSink::start().await.config();
                config.smtp_port = silent.local_addr().expect("an address").port();
                let app = app_with_config(&db, config).await;
                let (_, _, verified) = account(&db, true).await;
                let (_, _, pending) = account(&db, false).await;

                for address in [free_address(), verified, pending] {
                    let (status, _, _) = tokio::time::timeout(
                        Duration::from_secs(3),
                        register(&app, registration(&unique_name(), &address)),
                    )
                    .await
                    .expect("the answer must not wait for the mail");
                    assert_eq!(status, StatusCode::ACCEPTED);
                }

                drop(silent);
                db.close().await.ok();
            })
            .await;
        }

        /// The Argon2 hash runs whatever the address: an answer that came back
        /// faster for a taken one would say it was taken.
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn a_taken_address_takes_as_long_to_answer() {
            in_scratch_database(|db| async move {
                let sink = MailSink::start().await;
                let app = app_with_config(&db, sink.config()).await;
                let (_, _, taken) = account(&db, true).await;

                let mut free_times = Vec::new();
                let mut taken_times = Vec::new();
                for _ in 0..3 {
                    let started = Instant::now();
                    register(&app, registration(&unique_name(), &free_address())).await;
                    free_times.push(started.elapsed());
                    let started = Instant::now();
                    register(&app, registration(&unique_name(), &taken)).await;
                    taken_times.push(started.elapsed());
                }
                free_times.sort();
                taken_times.sort();

                // Generous: the hash dominates both, and a taken address that
                // skipped it would answer several times faster.
                assert!(
                    taken_times[1] * 2 >= free_times[1],
                    "a taken address answered in {:?}, a free one in {:?}",
                    taken_times,
                    free_times
                );

                sink.wait_for(6).await;
                db.close().await.ok();
            })
            .await;
        }
    }
}

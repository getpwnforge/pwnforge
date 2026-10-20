// crates/api/src/routes/setup.rs
use crate::{
    error::AppError,
    middleware::{
        address::require_mailbox, client_ip::ClientIp, json::JsonBody, setup::SetupToken,
    },
    state::AppState,
};
use axum::{
    Json, Router,
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use domain::dto::{
    auth::UserResponse,
    error_responses::{RateLimitedErrorResponse, SimpleErrorResponse, UnprocessableErrorResponse},
    setup::{
        EmailConfigResponse, SetupRequest, SetupStatusResponse, TestEmailRequest,
        ValidateAdminRequest,
    },
};
use sea_orm::prelude::IpNetwork;
use serde_json::json;
use services::{
    audit_service::AuditContext,
    email_service, instance_service,
    rate_limit_service::{self, RateLimitDecision},
    setup_service::{self, SetupError},
};
use std::net::{IpAddr, SocketAddr};
use validator::Validate;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/status", get(status))
        .route("/email-config", get(email_config))
        .route("/test-email", post(test_email))
        .route("/validate-admin", post(validate_admin))
        .route("/", post(complete))
}

/// The only route reachable without a token: the frontend cannot know whether
/// to prompt for one before asking.
#[utoipa::path(
    get,
    path = "/api/v1/setup/status",
    tag = "Setup",
    operation_id = "setup_status",
    responses(
        (status = 200, description = "Setup status", body = SetupStatusResponse),
        (status = 503, description = "Database unavailable", body = SimpleErrorResponse),
    )
)]
async fn status(State(state): State<AppState>) -> Result<Json<SetupStatusResponse>, AppError> {
    // 1. instance_service::is_setup_completed
    let completed = instance_service::is_setup_completed(&state.db).await?;
    // 2. Json(SetupStatusResponse { completed })
    Ok(Json(SetupStatusResponse { completed }))
}

/// Lets the operator check that the .env is what the process understood.
#[utoipa::path(
    get,
    path = "/api/v1/setup/email-config",
    tag = "Setup",
    responses(
        (status = 200, description = "Email configuration", body = EmailConfigResponse),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 404, description = "Setup already completed", body = SimpleErrorResponse),
    )
)]
async fn email_config(
    State(state): State<AppState>,
    _token: SetupToken,
) -> Result<Json<EmailConfigResponse>, AppError> {
    // The extractor covers the token, not the state of the instance.
    ensure_pending(&state).await?;

    Ok(Json(setup_service::email_config(&state.config)))
}

/// Sends a test message through the configured transport.
///
/// Records nothing: this only checks that the email backend the process
/// understood actually works.
#[utoipa::path(
    post,
    path = "/api/v1/setup/test-email",
    tag = "Setup",
    request_body = TestEmailRequest,
    responses(
        (status = 204, description = "Test email sent"),
        (status = 400, description = "malformed_json or invalid_body: the body is not JSON, or could not be read", body = SimpleErrorResponse),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 404, description = "Setup already completed", body = SimpleErrorResponse),
        (status = 415, description = "unsupported_media_type: Content-Type is not application/json", body = SimpleErrorResponse),
        (status = 422, description = "validation_failed or invalid_body; an address no mail can go to (a..b@example.test, a@[127.0.0.1]) is validation_failed, as a malformed one", body = UnprocessableErrorResponse),
        (status = 429, description = "Rate limited", body = RateLimitedErrorResponse),
        (status = 502, description = "Email send failed", body = SimpleErrorResponse),
        (status = 503, description = "Rate limit unavailable", body = SimpleErrorResponse),
    )
)]
async fn test_email(
    State(state): State<AppState>,
    _token: SetupToken,
    ClientIp(client_ip): ClientIp,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    JsonBody(payload): JsonBody<TestEmailRequest>,
) -> Result<Response, AppError> {
    ensure_pending(&state).await?;

    payload.validate()?;
    require_mailbox("to", &payload.to)?;

    let rate_limit_ip = client_ip.unwrap_or_else(|| remote.ip());

    match rate_limit_service::check_setup_test_email(&mut state.redis.clone(), rate_limit_ip).await
    {
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

/// Dry run of the administrator rules, so the wizard can reject a reserved
/// username or a breached password on the step that owns the field.
///
/// Creates nothing: POST /api/v1/setup runs the same checks again and stays
/// the authority.
#[utoipa::path(
    post,
    path = "/api/v1/setup/validate-admin",
    tag = "Setup",
    request_body = ValidateAdminRequest,
    responses(
        (status = 204, description = "Admin credentials valid"),
        (status = 400, description = "malformed_json or invalid_body: the body is not JSON, or could not be read", body = SimpleErrorResponse),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 404, description = "Setup already completed", body = SimpleErrorResponse),
        (status = 409, description = "username_taken, username_reserved or admin_email_taken: the username or the address is already an account's", body = SimpleErrorResponse),
        (status = 415, description = "unsupported_media_type: Content-Type is not application/json", body = SimpleErrorResponse),
        (status = 422, description = "validation_failed, invalid_body, invalid_email, email_domain_not_allowed or password_compromised; an address no mail can go to (a..b@example.test, a@[127.0.0.1]) is validation_failed, as a malformed one", body = UnprocessableErrorResponse),
        (status = 429, description = "Rate limited", body = RateLimitedErrorResponse),
        (status = 503, description = "Rate limit unavailable", body = SimpleErrorResponse),
    )
)]
async fn validate_admin(
    State(state): State<AppState>,
    _token: SetupToken,
    ClientIp(client_ip): ClientIp,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    JsonBody(payload): JsonBody<ValidateAdminRequest>,
) -> Result<StatusCode, AppError> {
    ensure_pending(&state).await?;

    payload.validate()?;
    require_mailbox("admin_email", &payload.admin_email)?;

    let rate_limit_ip = client_ip.unwrap_or_else(|| remote.ip());

    match rate_limit_service::check_setup_validate(&mut state.redis.clone(), rate_limit_ip).await {
        Ok(RateLimitDecision::Limited { retry_after_secs }) => {
            return Err(AppError::RateLimited { retry_after_secs });
        }
        Ok(RateLimitDecision::Allowed) => {}
        Err(err) => {
            // Refuse rather than fail open: every call through here reaches out
            // to the breach API on someone else's behalf.
            tracing::error!(error = ?err, "setup validate rate limit unavailable, refusing");
            return Err(AppError::ServiceUnavailable);
        }
    };

    setup_service::validate_admin(&state.db, &state.config, &payload).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Creates the first administrator and closes the wizard.
///
/// The account is created verified, the instance settings are recorded, and
/// the setup surface answers 404 from this point on.
#[utoipa::path(
    post,
    path = "/api/v1/setup",
    tag = "Setup",
    request_body = SetupRequest,
    responses(
        (status = 201, description = "Setup completed", body = UserResponse),
        (status = 400, description = "malformed_json or invalid_body: the body is not JSON, or could not be read", body = SimpleErrorResponse),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 404, description = "Setup already completed", body = SimpleErrorResponse),
        (status = 409, description = "username_taken, username_reserved or admin_email_taken: the username or the address is already an account's", body = SimpleErrorResponse),
        (status = 415, description = "unsupported_media_type: Content-Type is not application/json", body = SimpleErrorResponse),
        (status = 422, description = "validation_failed, invalid_body, invalid_timezone, invalid_email, email_domain_not_allowed or password_compromised; an address no mail can go to (a..b@example.test, a@[127.0.0.1]) is validation_failed, as a malformed one", body = UnprocessableErrorResponse),
        (status = 429, description = "Rate limited", body = RateLimitedErrorResponse),
        (status = 503, description = "Rate limit unavailable", body = SimpleErrorResponse),
    )
)]
async fn complete(
    State(state): State<AppState>,
    _token: SetupToken,
    ClientIp(client_ip): ClientIp,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    JsonBody(payload): JsonBody<SetupRequest>,
) -> Result<(StatusCode, Json<UserResponse>), AppError> {
    payload.validate()?;
    require_mailbox("admin_email", &payload.admin_email)?;

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

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::{Request, StatusCode};
    use domain::entities::{instance_settings, user_emails, users};
    use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, Set};
    use serde_json::{Value, json};
    use services::instance_service::SETTINGS_ID;
    use std::net::{IpAddr, Ipv6Addr, SocketAddr};
    use tower::ServiceExt;
    use uuid::Uuid;

    use crate::test_support::{app_with_setup_token, in_database_before_setup};

    const TOKEN: &str = "a-setup-token-for-these-tests-only";

    /// A documentation address (RFC 3849) of its own per call, so no two
    /// requests share a rate-limit bucket.
    fn fresh_ip() -> IpAddr {
        let id = Uuid::now_v7().as_u128();
        IpAddr::V6(Ipv6Addr::new(
            0x2001,
            0x0db8,
            0x5e7,
            (id >> 64) as u16,
            (id >> 48) as u16,
            (id >> 32) as u16,
            (id >> 16) as u16,
            id as u16,
        ))
    }

    /// A username of its own, at most 32 characters.
    fn name() -> String {
        format!("a{}", &Uuid::now_v7().simple().to_string()[1..])
    }

    async fn post(app: &axum::Router, uri: &str, body: Value) -> (StatusCode, Value) {
        let mut request = Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .header("x-setup-token", TOKEN)
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
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("the body must be readable");
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).expect("the body must be JSON")
        };
        (status, body)
    }

    fn admin(username: &str, email: &str) -> Value {
        json!({
            "admin_username": username,
            "admin_email": email,
            "admin_password": "a-long-test-password-1!",
        })
    }

    fn setup(username: &str, email: &str) -> Value {
        let mut body = admin(username, email);
        body["default_locale"] = json!("en");
        body["default_timezone"] = json!("Europe/Paris");
        body["allow_public_signup"] = json!(false);
        body["hide_landing_page"] = json!(false);
        body
    }

    /// An account that already holds `email`.
    async fn holder(db: &DatabaseConnection, email: &str) {
        let user = users::ActiveModel {
            id: Set(Uuid::now_v7()),
            username: Set(name()),
            password_hash: Set("not-a-real-hash".to_owned()),
            locale: Set("en".to_owned()),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("the account inserts");
        user_emails::ActiveModel {
            id: Set(Uuid::now_v7()),
            user_id: Set(user.id),
            email: Set(email.to_owned()),
            is_primary: Set(true),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("the address inserts");
    }

    async fn setup_done(db: &DatabaseConnection) -> bool {
        instance_settings::Entity::find_by_id(SETTINGS_ID)
            .one(db)
            .await
            .expect("the settings read")
            .expect("the settings row exists")
            .setup_completed_at
            .is_some()
    }

    /// An address an account already holds was a 500 on the final step;
    /// both steps now name it, so the wizard can put it under the field.
    #[tokio::test]
    async fn an_address_already_taken_is_409_admin_email_taken() {
        in_database_before_setup(|db| async move {
            holder(&db, "operator@example.test").await;
            let app = app_with_setup_token(&db, TOKEN).await;

            for (uri, body) in [
                (
                    "/api/v1/setup/validate-admin",
                    admin(&name(), "Operator@Example.test"),
                ),
                ("/api/v1/setup", setup(&name(), "Operator@Example.test")),
            ] {
                let (status, body) = post(&app, uri, body).await;
                assert_eq!(status, StatusCode::CONFLICT, "{uri}: {body}");
                assert_eq!(body, json!({ "error": "admin_email_taken" }), "{uri}");
            }
            assert!(!setup_done(&db).await, "the setup is still to run");
        })
        .await;
    }

    /// An address no mail can go to is refused on every setup route that
    /// takes one, as a malformed one, and the setup stays to run.
    #[tokio::test]
    async fn an_address_no_mail_can_go_to_is_refused_on_every_setup_route() {
        in_database_before_setup(|db| async move {
            let app = app_with_setup_token(&db, TOKEN).await;

            for address in ["a..b@example.test", "a@[127.0.0.1]"] {
                for (uri, body, field) in [
                    ("/api/v1/setup/test-email", json!({ "to": address }), "to"),
                    (
                        "/api/v1/setup/validate-admin",
                        admin(&name(), address),
                        "admin_email",
                    ),
                    ("/api/v1/setup", setup(&name(), address), "admin_email"),
                ] {
                    let (status, body) = post(&app, uri, body).await;
                    assert_eq!(
                        status,
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "{uri} {address}: {body}"
                    );
                    assert_eq!(body["error"], "validation_failed", "{uri} {address}");
                    assert_eq!(
                        body["fields"],
                        json!({ field: ["email"] }),
                        "{uri} {address}"
                    );
                }
            }
            assert!(!setup_done(&db).await, "the setup is still to run");
        })
        .await;
    }

    /// Control: a free address passes the dry run and the setup.
    #[tokio::test]
    async fn a_free_address_completes_the_setup() {
        in_database_before_setup(|db| async move {
            holder(&db, "someone-else@example.test").await;
            let app = app_with_setup_token(&db, TOKEN).await;
            let (username, email) = (name(), format!("{}@example.test", name()));

            let (status, body) = post(
                &app,
                "/api/v1/setup/validate-admin",
                admin(&username, &email),
            )
            .await;
            assert_eq!(status, StatusCode::NO_CONTENT, "{body}");

            let (status, body) = post(&app, "/api/v1/setup", setup(&username, &email)).await;
            assert_eq!(status, StatusCode::CREATED, "{body}");
            assert!(setup_done(&db).await, "the setup has run");
        })
        .await;
    }
}

// crates/api/src/error.rs
use crate::services::{auth_service::AuthError, auth_token_service::AuthTokenError, blocked_email_service::BlockedEmailError, email_service::EmailError, instance_service::InstanceError, session_service::SessionError, setup_service::SetupError};
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Validation(#[from] validator::ValidationErrors),

    #[error(transparent)]
    Auth(#[from] AuthError),

    #[error("rate limit exceeded")]
    RateLimited { retry_after_secs: u64 },

    #[error("unauthorized")]
    Unauthorized,

    #[error("service temporarily unavailable")]
    ServiceUnavailable,

    #[error(transparent)]
    Email(#[from] EmailError),

    #[error(transparent)]
    Instance(#[from] InstanceError),

    #[error(transparent)]
    Setup(#[from] SetupError),

    #[error(transparent)]
    Db(#[from] sea_orm::DbErr),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::Validation(errors) => {
                // field -> [codes], consumed by the frontend to place messages under inputs
                let fields: HashMap<_, Vec<_>> = errors
                    .field_errors()
                    .iter()
                    .map(|(field, errs)| {
                        (
                            field.to_string(),
                            errs.iter().map(|e| e.code.to_string()).collect(),
                        )
                    })
                    .collect();

                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(json!({ "error": "validation_failed", "fields": fields })),
                )
                    .into_response()
            }

            AppError::Auth(err) => {
                let (status, code) = match &err {
                    AuthError::UsernameTaken => (StatusCode::CONFLICT, "username_taken"),
                    AuthError::EmailTaken => (StatusCode::CONFLICT, "email_taken"),
                    AuthError::UsernameReserved => (StatusCode::CONFLICT, "username_reserved"),
                    AuthError::InvalidEmail => (StatusCode::UNPROCESSABLE_ENTITY, "invalid_email"),
                    AuthError::BlockedEmail(BlockedEmailError::DomainNotAllowed) => {
                        (StatusCode::UNPROCESSABLE_ENTITY, "email_domain_not_allowed")
                    }
                    AuthError::PublicSignupDisabled => (StatusCode::FORBIDDEN, "public_signup_disabled"),
                    AuthError::InvalidCredentials => {
                        (StatusCode::UNAUTHORIZED, "invalid_credentials")
                    }
                    AuthError::EmailNotVerified => (StatusCode::FORBIDDEN, "email_not_verified"),
                    AuthError::AccountSuspended { .. } => {
                        (StatusCode::FORBIDDEN, "account_suspended")
                    }
                    AuthError::InvalidRefreshToken => {
                        (StatusCode::UNAUTHORIZED, "invalid_refresh_token")
                    }
                    AuthError::Session(err) => match err {
                        // Same code for all three: the client cannot act on the distinction, and
                        // telling it apart would leak whether a token ever existed.
                        SessionError::NotFound | SessionError::Expired | SessionError::Reused => {
                            (StatusCode::UNAUTHORIZED, "invalid_refresh_token")
                        }
                        SessionError::Db(_) | SessionError::Audit(_) => {
                            tracing::error!(error = ?err, "session store failure");
                            (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
                        }
                    },
                    AuthError::PasswordCompromised => {
                        (StatusCode::UNPROCESSABLE_ENTITY, "password_compromised")
                    }
                    AuthError::PasswordUnchanged => {
                        (StatusCode::UNPROCESSABLE_ENTITY, "password_unchanged")
                    }
                    AuthError::AuthToken(err) => match err {
                        AuthTokenError::NotFound => (StatusCode::UNPROCESSABLE_ENTITY, "token_invalid"),
                        AuthTokenError::Expired => (StatusCode::UNPROCESSABLE_ENTITY, "token_expired"),
                        AuthTokenError::Consumed => (StatusCode::UNPROCESSABLE_ENTITY, "token_consumed"),
                        AuthTokenError::Db(_) => {
                            tracing::error!(error = ?err, "auth token store failure");
                            (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
                        }
                    },
                    // Internal failures: log the cause, return an opaque code.
                    _ => {
                        tracing::error!(error = ?err, "unhandled auth error");
                        (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
                    }
                };

                (status, Json(json!({ "error": code }))).into_response()
            }

            AppError::RateLimited { retry_after_secs } => {
                let mut response = (
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(json!({ "error": "rate_limited", "retry_after_secs": retry_after_secs })),
                )
                    .into_response();

                if let Ok(value) = axum::http::HeaderValue::from_str(&retry_after_secs.to_string())
                {
                    response
                        .headers_mut()
                        .insert(axum::http::header::RETRY_AFTER, value);
                }

                response
            }

            AppError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "unauthorized" })),
            )
                .into_response(),

            AppError::Db(err) => {
                tracing::error!(error = ?err, "database error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": "internal_error" })),
                )
                    .into_response()
            }

            AppError::ServiceUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error": "service_unavailable"})),
            )
                .into_response(),

            AppError::Setup(err) => {
                let (status, code) = match &err {
                    SetupError::AlreadyCompleted | SetupError::InvalidToken => {
                        (StatusCode::NOT_FOUND, "not_found")
                    }
                    SetupError::InvalidTimezone => {
                        (StatusCode::UNPROCESSABLE_ENTITY, "invalid_timezone")
                    }
                    SetupError::Email(_) => (StatusCode::BAD_GATEWAY, "email_send_failed"),

                    // Only the register rules that can actually fire here. The rest of
                    // AuthError has no meaning during setup.
                    SetupError::Auth(AuthError::UsernameTaken) => (StatusCode::CONFLICT, "username_taken"),
                    SetupError::Auth(AuthError::UsernameReserved) => {
                        (StatusCode::CONFLICT, "username_reserved")
                    }
                    SetupError::Auth(AuthError::EmailTaken) => (StatusCode::CONFLICT, "email_taken"),
                    SetupError::Auth(AuthError::PasswordCompromised) => {
                        (StatusCode::UNPROCESSABLE_ENTITY, "password_compromised")
                    }
                    SetupError::Auth(AuthError::BlockedEmail(_)) => {
                        (StatusCode::UNPROCESSABLE_ENTITY, "email_domain_not_allowed")
                    }

                    _ => {
                        tracing::error!(error = ?err, "setup failure");
                        (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
                    }
                };

                (status, Json(json!({ "error": code }))).into_response()
            }
            AppError::Instance(err) => {
                tracing::error!(error = ?err, "instance settings unavailable");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": "internal_error" })),
                )
                    .into_response()
            }
            AppError::Email(err) => {
                tracing::error!(error = ?err, "email send failed");
                (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({ "error": "email_send_failed" })),
                )
                    .into_response()
            }


        }
    }
}

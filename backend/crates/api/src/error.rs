// crates/api/src/error.rs
use crate::services::auth_service::AuthError;
use crate::services::blocked_email_service::BlockedEmailError;
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
        }
    }
}

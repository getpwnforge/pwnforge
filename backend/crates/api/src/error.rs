// crates/api/src/error.rs
use axum::{
    Json,
    extract::rejection::JsonRejection,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use services::{
    alert_service::AlertError, auth_service::AuthError, auth_token_service::AuthTokenError,
    blocked_email_service::BlockedEmailError, contact_service::ContactError,
    email_service::EmailError, instance_service::InstanceError, legal_service::LegalError,
    session_service::SessionError, setup_service::SetupError,
};
use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Validation(#[from] validator::ValidationErrors),

    #[error(transparent)]
    JsonBody(#[from] JsonRejection),

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
    Alert(#[from] AlertError),

    #[error(transparent)]
    Legal(#[from] LegalError),

    #[error(transparent)]
    Contact(#[from] ContactError),

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

            AppError::JsonBody(err) => {
                // Logged, never returned: axum's message names the fields the
                // endpoint expects, which hands a client-side schema to
                // anyone probing the API. The log covers the only legitimate
                // need for that detail, which is debugging.
                tracing::debug!(error = ?err, "request body rejected");

                let (status, code) = match &err {
                    // Valid JSON, wrong shape: unknown field, wrong type,
                    // missing required field. Same status as a validation
                    // failure, since the request is well-formed but unusable.
                    JsonRejection::JsonDataError(_) => {
                        (StatusCode::UNPROCESSABLE_ENTITY, "invalid_body")
                    }
                    // Not valid JSON at all.
                    JsonRejection::JsonSyntaxError(_) => {
                        (StatusCode::BAD_REQUEST, "malformed_json")
                    }
                    JsonRejection::MissingJsonContentType(_) => {
                        (StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported_media_type")
                    }
                    // JsonRejection is non_exhaustive: BytesRejection today,
                    // whatever axum adds tomorrow.
                    _ => (StatusCode::BAD_REQUEST, "invalid_body"),
                };

                (status, Json(json!({ "error": code }))).into_response()
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
                    AuthError::PublicSignupDisabled => {
                        (StatusCode::FORBIDDEN, "public_signup_disabled")
                    }
                    AuthError::InvalidCredentials
                    | AuthError::AccountSuspended { .. }
                    | AuthError::PendingDeletion => {
                        (StatusCode::UNAUTHORIZED, "invalid_credentials")
                    }
                    AuthError::EmailNotVerified { .. } => {
                        (StatusCode::FORBIDDEN, "email_not_verified")
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
                        AuthTokenError::NotFound => {
                            (StatusCode::UNPROCESSABLE_ENTITY, "token_invalid")
                        }
                        AuthTokenError::Expired => {
                            (StatusCode::UNPROCESSABLE_ENTITY, "token_expired")
                        }
                        AuthTokenError::Consumed => {
                            (StatusCode::UNPROCESSABLE_ENTITY, "token_consumed")
                        }
                        AuthTokenError::Db(_) => {
                            tracing::error!(error = ?err, "auth token store failure");
                            (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
                        }
                    },
                    AuthError::Legal(err) => {
                        tracing::error!(error = ?err, "legal acceptance write failed during register");
                        (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
                    }
                    AuthError::Turnstile(_) => (StatusCode::BAD_REQUEST, "turnstile_failed"),
                    // Internal failures: log the cause, return an opaque code.
                    _ => {
                        tracing::error!(error = ?err, "unhandled auth error");
                        (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
                    }
                };

                let body = match err {
                    AuthError::EmailNotVerified { resend_token } => {
                        json!({ "error": code, "resend_token": resend_token })
                    }
                    _ => json!({ "error": code }),
                };

                (status, Json(body)).into_response()
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
                    SetupError::Auth(AuthError::UsernameTaken) => {
                        (StatusCode::CONFLICT, "username_taken")
                    }
                    SetupError::Auth(AuthError::UsernameReserved) => {
                        (StatusCode::CONFLICT, "username_reserved")
                    }
                    SetupError::Auth(AuthError::EmailTaken) => {
                        (StatusCode::CONFLICT, "email_taken")
                    }
                    SetupError::Auth(AuthError::PasswordCompromised) => {
                        (StatusCode::UNPROCESSABLE_ENTITY, "password_compromised")
                    }
                    SetupError::Auth(AuthError::BlockedEmail(_)) => {
                        (StatusCode::UNPROCESSABLE_ENTITY, "email_domain_not_allowed")
                    }
                    SetupError::Auth(AuthError::InvalidEmail) => {
                        (StatusCode::UNPROCESSABLE_ENTITY, "invalid_email")
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

            AppError::Alert(err) => {
                let (status, code) = match &err {
                    AlertError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
                    AlertError::Audit(_) | AlertError::Db(_) => {
                        tracing::error!(error = ?err, "alert store failure");
                        (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
                    }
                };
                (status, Json(json!({ "error": code }))).into_response()
            }

            AppError::Legal(err) => {
                let (status, code) = match &err {
                    LegalError::VersionStale => (StatusCode::CONFLICT, "legal_version_stale"),
                    LegalError::AcceptanceRequired => {
                        (StatusCode::FORBIDDEN, "legal_acceptance_required")
                    }
                    LegalError::Db(_) => {
                        tracing::error!(error = ?err, "legal acceptance store failure");
                        (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
                    }
                };

                let body = match err {
                    // Carry the current versions so the client can re-prompt
                    // without a second round trip, same idea as the
                    // resend_token on EmailNotVerified.
                    LegalError::VersionStale => json!({
                        "error": code,
                        "current_terms_version": domain::legal::CURRENT_TERMS_VERSION,
                        "current_privacy_version": domain::legal::CURRENT_PRIVACY_VERSION,
                    }),
                    _ => json!({ "error": code }),
                };

                (status, Json(body)).into_response()
            }
            AppError::Contact(err) => match &err {
                ContactError::RateLimited { retry_after_secs } => (
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(json!({ "error": "rate_limited", "retry_after_secs": retry_after_secs })),
                )
                    .into_response(),
                ContactError::Turnstile(_) => (
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error": "turnstile_failed" })),
                )
                    .into_response(),
                ContactError::Email(_) | ContactError::Redis(_) => {
                    tracing::error!(error = ?err, "contact notification failed");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": "internal_error" })),
                    )
                        .into_response()
                }
            },
        }
    }
}

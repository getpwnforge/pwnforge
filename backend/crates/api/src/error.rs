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
    avatar_service::AvatarError, blocked_email_service::BlockedEmailError,
    contact_service::ContactError, email_service::EmailError, instance_service::InstanceError,
    legal_service::LegalError, permissions_service::PermissionsError,
    session_service::SessionError, setup_service::SetupError, slug_service,
    storage_service::StorageError, team_service::TeamError, workspace_service::WorkspaceError,
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

    /// The caller is authenticated and may act on this resource's kind, but
    /// not with the role they hold. Distinct from `NotFound`, which is what a
    /// caller with no access at all gets: see the 404/403 invariant in
    /// SECURITY.md.
    #[error("forbidden")]
    Forbidden,

    /// Either the resource does not exist, or the caller must not learn that
    /// it does.
    #[error("not found")]
    NotFound,

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
    Team(#[from] TeamError),

    #[error(transparent)]
    Avatar(#[from] AvatarError),

    #[error(transparent)]
    Permissions(#[from] PermissionsError),

    #[error(transparent)]
    Workspace(#[from] WorkspaceError),

    #[error(transparent)]
    Db(#[from] sea_orm::DbErr),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::Validation(errors) => validation_response(&errors),
            AppError::JsonBody(err) => json_body_response(&err),
            AppError::Auth(err) => auth_response(err),
            AppError::RateLimited { retry_after_secs } => rate_limited_response(retry_after_secs),

            AppError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "unauthorized" })),
            )
                .into_response(),

            AppError::Forbidden => (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "insufficient_role" })),
            )
                .into_response(),

            AppError::NotFound => {
                (StatusCode::NOT_FOUND, Json(json!({ "error": "not_found" }))).into_response()
            }

            AppError::Avatar(err) => avatar_response(&err),

            AppError::Permissions(err) => {
                tracing::error!(error = ?err, "permission resolution failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": "internal_error" })),
                )
                    .into_response()
            }

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

            AppError::Setup(err) => setup_response(&err),
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

            AppError::Alert(err) => alert_response(&err),
            AppError::Legal(err) => legal_response(&err),
            AppError::Contact(err) => contact_response(&err),
            AppError::Team(err) => team_response(&err),
            AppError::Workspace(err) => workspace_response(err),
        }
    }
}

// One function per error family with a nested mapping, so `into_response`
// stays a flat dispatch whatever the families grow into.

fn validation_response(errors: &validator::ValidationErrors) -> Response {
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

fn json_body_response(err: &JsonRejection) -> Response {
    // Logged, never returned: axum's message names the fields the
    // endpoint expects, which hands a client-side schema to
    // anyone probing the API. The log covers the only legitimate
    // need for that detail, which is debugging.
    tracing::debug!(error = ?err, "request body rejected");

    let (status, code) = match err {
        // Valid JSON, wrong shape: unknown field, wrong type,
        // missing required field. Same status as a validation
        // failure, since the request is well-formed but unusable.
        JsonRejection::JsonDataError(_) => (StatusCode::UNPROCESSABLE_ENTITY, "invalid_body"),
        // Not valid JSON at all.
        JsonRejection::JsonSyntaxError(_) => (StatusCode::BAD_REQUEST, "malformed_json"),
        JsonRejection::MissingJsonContentType(_) => {
            (StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported_media_type")
        }
        // JsonRejection is non_exhaustive: BytesRejection today,
        // whatever axum adds tomorrow.
        _ => (StatusCode::BAD_REQUEST, "invalid_body"),
    };

    (status, Json(json!({ "error": code }))).into_response()
}

fn auth_response(err: AuthError) -> Response {
    let (status, code) = match &err {
        AuthError::UsernameTaken => (StatusCode::CONFLICT, "username_taken"),
        AuthError::EmailTaken => (StatusCode::CONFLICT, "email_taken"),
        AuthError::UsernameReserved => (StatusCode::CONFLICT, "username_reserved"),
        AuthError::InvalidEmail => (StatusCode::UNPROCESSABLE_ENTITY, "invalid_email"),
        AuthError::BlockedEmail(BlockedEmailError::DomainNotAllowed) => {
            (StatusCode::UNPROCESSABLE_ENTITY, "email_domain_not_allowed")
        }
        AuthError::PublicSignupDisabled => (StatusCode::FORBIDDEN, "public_signup_disabled"),
        AuthError::InvalidCredentials
        | AuthError::AccountSuspended { .. }
        | AuthError::PendingDeletion => (StatusCode::UNAUTHORIZED, "invalid_credentials"),
        AuthError::EmailNotVerified { .. } => (StatusCode::FORBIDDEN, "email_not_verified"),
        AuthError::InvalidRefreshToken => (StatusCode::UNAUTHORIZED, "invalid_refresh_token"),
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
        AuthError::PasswordUnchanged => (StatusCode::UNPROCESSABLE_ENTITY, "password_unchanged"),
        AuthError::AuthToken(err) => match err {
            AuthTokenError::NotFound => (StatusCode::UNPROCESSABLE_ENTITY, "token_invalid"),
            AuthTokenError::Expired => (StatusCode::UNPROCESSABLE_ENTITY, "token_expired"),
            AuthTokenError::Consumed => (StatusCode::UNPROCESSABLE_ENTITY, "token_consumed"),
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

fn rate_limited_response(retry_after_secs: u64) -> Response {
    let mut response = (
        StatusCode::TOO_MANY_REQUESTS,
        Json(json!({ "error": "rate_limited", "retry_after_secs": retry_after_secs })),
    )
        .into_response();

    if let Ok(value) = axum::http::HeaderValue::from_str(&retry_after_secs.to_string()) {
        response
            .headers_mut()
            .insert(axum::http::header::RETRY_AFTER, value);
    }

    response
}

fn avatar_response(err: &AvatarError) -> Response {
    let (status, code) = match err {
        // 404 rather than 403 or 204: to a client, an entity with
        // no avatar and an entity it may not see are the same
        // answer, and the route is a plain image fetch that has to
        // fail the way a missing image does.
        AvatarError::NotFound => (StatusCode::NOT_FOUND, "not_found"),

        AvatarError::TooLarge { .. } => (StatusCode::PAYLOAD_TOO_LARGE, "avatar_too_large"),

        // 415 rather than 422: the objection is to the media type
        // of the body, not to a field in it.
        AvatarError::SvgRejected => (StatusCode::UNSUPPORTED_MEDIA_TYPE, "avatar_svg_rejected"),
        AvatarError::UnsupportedFormat => (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "avatar_unsupported_format",
        ),

        // The bytes passed the magic-byte pre-filter and the
        // decoder still refused them, which includes hitting the
        // decompression-bomb limits. Either way the caller has to
        // send a different file.
        AvatarError::Undecodable => (StatusCode::UNPROCESSABLE_ENTITY, "avatar_undecodable"),

        // Both are ours, not the caller's, and both are already
        // logged where they happen. 503 rather than 500 for the
        // timeout: it is a load condition, and retrying later is
        // the right advice.
        AvatarError::RenderTimeout => (StatusCode::SERVICE_UNAVAILABLE, "service_unavailable"),
        AvatarError::RenderPanicked | AvatarError::EncodeFailed => {
            (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
        }

        AvatarError::Storage(storage) => match storage {
            // The row named a version the bucket does not hold.
            // Already logged as an inconsistency by the service.
            StorageError::NotFound => (StatusCode::NOT_FOUND, "not_found"),

            // 507 rather than 503: the store is answering fine and
            // retrying will never help. Someone has to raise the
            // quota or delete objects, and the operator needs to
            // be able to tell this apart from an outage at a
            // glance - which is the whole reason this is not
            // folded into `Backend`.
            StorageError::QuotaExceeded { .. } => {
                tracing::error!(error = ?err, "object storage quota reached");
                (StatusCode::INSUFFICIENT_STORAGE, "storage_quota_exceeded")
            }

            // The provider refused our credential. The caller did
            // nothing wrong and can do nothing about it: this is
            // our own grant or policy being incomplete, so it is a
            // 500 with a log line that names the likely cause,
            // not a 503 that suggests waiting.
            StorageError::AccessDenied { .. } => {
                tracing::error!(
                    error = ?err,
                    "object storage refused our credential; check the key's \
                     grants on both buckets"
                );
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
            }

            StorageError::BucketUnavailable { .. } | StorageError::Backend(_) => {
                tracing::error!(error = ?err, "object storage unavailable");
                (StatusCode::SERVICE_UNAVAILABLE, "service_unavailable")
            }
            StorageError::TooLarge { .. } => {
                tracing::error!(error = ?err, "stored avatar is past the read limit");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
            }
        },

        AvatarError::Db(_) => {
            tracing::error!(error = ?err, "avatar store failure");
            (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
        }
    };

    (status, Json(json!({ "error": code }))).into_response()
}

fn setup_response(err: &SetupError) -> Response {
    let (status, code) = match err {
        SetupError::AlreadyCompleted | SetupError::InvalidToken => {
            (StatusCode::NOT_FOUND, "not_found")
        }
        SetupError::InvalidTimezone => (StatusCode::UNPROCESSABLE_ENTITY, "invalid_timezone"),
        SetupError::Email(_) => {
            tracing::error!(error = ?err, "setup email send failed");
            (StatusCode::BAD_GATEWAY, "email_send_failed")
        }

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

fn alert_response(err: &AlertError) -> Response {
    let (status, code) = match err {
        AlertError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
        AlertError::Audit(_) | AlertError::Db(_) => {
            tracing::error!(error = ?err, "alert store failure");
            (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
        }
    };
    (status, Json(json!({ "error": code }))).into_response()
}

fn legal_response(err: &LegalError) -> Response {
    let (status, code) = match err {
        LegalError::VersionStale => (StatusCode::CONFLICT, "legal_version_stale"),
        LegalError::AcceptanceRequired => (StatusCode::FORBIDDEN, "legal_acceptance_required"),
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

fn contact_response(err: &ContactError) -> Response {
    match err {
        // Same body and same Retry-After header as every other rate limit.
        ContactError::RateLimited { retry_after_secs } => rate_limited_response(*retry_after_secs),
        ContactError::Turnstile(_) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "turnstile_failed" })),
        )
            .into_response(),
        // The limiter is down. The route fails closed: nothing else bounds
        // it, and it sends mail. 503 rather than 500, because the request
        // was fine and retrying later is the right advice.
        ContactError::Redis(_) => {
            tracing::error!(error = ?err, "contact rate limiter unavailable, refusing");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "error": "service_unavailable" })),
            )
                .into_response()
        }
        ContactError::Email(_) => {
            tracing::error!(error = ?err, "contact notification failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "internal_error" })),
            )
                .into_response()
        }
    }
}

fn team_response(err: &TeamError) -> Response {
    match err {
        TeamError::AlreadyInvited => (
            StatusCode::CONFLICT,
            Json(json!({ "error": "already_invited" })),
        )
            .into_response(),
        TeamError::AlreadyMember => (
            StatusCode::CONFLICT,
            Json(json!({ "error": "already_member" })),
        )
            .into_response(),
        TeamError::InvitationExpired => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "invitation_expired" })),
        )
            .into_response(),
        TeamError::InvitationMaxUsesReached => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "invitation_max_uses_reached" })),
        )
            .into_response(),
        TeamError::InvitationNotFound => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "invitation_not_found" })),
        )
            .into_response(),
        TeamError::InsufficientRole => (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "insufficient_role" })),
        )
            .into_response(),
        TeamError::InvitationNotForUser => (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "invitation_not_for_user" })),
        )
            .into_response(),
        TeamError::InvitationEmailMismatch => (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "invitation_email_mismatch" })),
        )
            .into_response(),
        TeamError::LastCaptain => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "last_captain" })),
        )
            .into_response(),
        TeamError::MemberNotFound => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "member_not_found" })),
        )
            .into_response(),
        // Both map to 422 with a code the form can place under the
        // name field: the caller has to change the name, and the two
        // reasons are distinct enough to word differently.
        TeamError::Slug(slug_service::SlugError::Empty) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "team_name_not_sluggable" })),
        )
            .into_response(),
        TeamError::Slug(slug_service::SlugError::Reserved) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "team_name_reserved" })),
        )
            .into_response(),
        // Every candidate drawn was taken. Not the caller's fault and
        // not something a different name would fix, so it is a 500
        // rather than the 422 the two validation variants above get.
        TeamError::Slug(slug_service::SlugError::Exhausted) | TeamError::SlugGenerationFailed => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "slug_generation_failed" })),
        )
            .into_response(),
        TeamError::TeamNotFound => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "team_not_found" })),
        )
            .into_response(),
        // Generic 404: the caller named a username that matches no
        // account. This does confirm the name is unclaimed, which is
        // already what registration says with `username_taken`, so it
        // reveals nothing the signup form does not.
        TeamError::InvitedUserNotFound => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "invited_user_not_found" })),
        )
            .into_response(),
        // 422 rather than 429: the caller is not going too fast, they
        // are at a hard ceiling that only deleting a team clears, so
        // there is no delay worth handing back.
        TeamError::TeamQuotaReached => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "team_quota_reached" })),
        )
            .into_response(),
        // Everything with no client-facing meaning, `TeamError::Db`
        // chief among them. Logged whole rather than summarised: the
        // `Debug` of a `DbErr` carries the driver's own message, which
        // is the only place the violated constraint is named.
        // `sql_err()` classifies unique and foreign-key violations
        // only, and reports None for a CHECK.
        _ => {
            tracing::error!(error = ?err, "team operation failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "internal_error" })),
            )
                .into_response()
        }
    }
}

fn workspace_response(err: WorkspaceError) -> Response {
    match err {
        WorkspaceError::WorkspaceNotFound => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "workspace_not_found" })),
        )
            .into_response(),
        WorkspaceError::Team(inner) => AppError::Team(inner).into_response(),
        WorkspaceError::Permissions(inner) => AppError::Permissions(inner).into_response(),
        WorkspaceError::Db(db_err) => {
            tracing::error!(error = ?db_err, "workspace operation failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "internal_error" })),
            )
                .into_response()
        }
        WorkspaceError::Slug(slug_service::SlugError::Empty) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "workspace_name_not_sluggable" })),
        )
            .into_response(),
        WorkspaceError::Slug(slug_service::SlugError::Reserved) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "workspace_name_reserved" })),
        )
            .into_response(),
        WorkspaceError::Slug(slug_service::SlugError::Exhausted) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "slug_generation_failed" })),
        )
            .into_response(),
        WorkspaceError::Slug(slug_service::SlugError::Db(db_err)) => {
            tracing::error!(error = ?db_err, "workspace slug generation failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "internal_error" })),
            )
                .into_response()
        }
        WorkspaceError::InvalidOwnership => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "invalid_ownership" })),
        )
            .into_response(),
        WorkspaceError::TargetNotInTeam => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "target_not_in_team" })),
        )
            .into_response(),
        WorkspaceError::TargetAlreadyInTeam => (
            StatusCode::CONFLICT,
            Json(json!({ "error": "target_already_in_team" })),
        )
            .into_response(),
        WorkspaceError::SoloWorkspace => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "solo_workspace" })),
        )
            .into_response(),
        // 409 rather than 403: the caller holds the role, it is the state of
        // the workspace that refuses the write, and unarchiving lifts it.
        WorkspaceError::Archived => (
            StatusCode::CONFLICT,
            Json(json!({ "error": "workspace_archived" })),
        )
            .into_response(),
        WorkspaceError::InvalidDateRange => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "ends_before_start" })),
        )
            .into_response(),
        WorkspaceError::PlatformRequired => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "platform_required" })),
        )
            .into_response(),
        WorkspaceError::DataIntegrity(integrity_err) => {
            tracing::error!(error = ?integrity_err, "workspace operation failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "internal_error" })),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn body_of(response: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("the body must be readable");
        serde_json::from_slice(&bytes).expect("the body must be JSON")
    }

    /// The contact limiter being down is a refusal, not a crash: the route
    /// fails closed with 503, which tells the client to retry later.
    #[tokio::test]
    async fn an_unreachable_contact_limiter_is_503_service_unavailable() {
        let redis_down = redis::RedisError::from(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "redis is down",
        ));

        let response = AppError::Contact(ContactError::Redis(redis_down)).into_response();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body_of(response).await["error"], "service_unavailable");
    }

    /// Control: a failed notification mail keeps its 500.
    #[tokio::test]
    async fn a_failed_contact_mail_stays_500_internal_error() {
        let rejected = services::email_service::EmailError::Rejected("refused".to_owned());

        let response = AppError::Contact(ContactError::Email(rejected)).into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body_of(response).await["error"], "internal_error");
    }

    /// The contact form's 429 carries the same Retry-After header as every
    /// other rate limit, with the delay also in the body.
    #[tokio::test]
    async fn the_contact_rate_limit_carries_retry_after() {
        let response = AppError::Contact(ContactError::RateLimited {
            retry_after_secs: 42,
        })
        .into_response();

        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::RETRY_AFTER)
                .map(|value| value.to_str().unwrap()),
            Some("42")
        );

        let body = body_of(response).await;
        assert_eq!(body["error"], "rate_limited");
        assert_eq!(body["retry_after_secs"], 42);
    }
}

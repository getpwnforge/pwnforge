pub mod alerts;
pub mod auth;
pub mod avatars;
pub mod contact;
pub mod health;
pub mod instance;
pub mod legal;
pub mod setup;
pub mod teams;
pub mod workspace_invitations;
pub mod workspace_members;
pub mod workspaces;

use services::rate_limit_service::RateLimitDecision;

use crate::error::AppError;

/// An invitation limiter's answer as the route's: 429 past a window, and 503
/// when the limiter cannot answer. Fails closed: a session on any team is
/// all an invitation route asks for, and each invitation may send a mail, so
/// unmetered they are a mailer (see docs/security-invariants.md).
pub(crate) fn enforce_invitation_limit<E: std::fmt::Debug>(
    decision: Result<RateLimitDecision, E>,
) -> Result<(), AppError> {
    match decision {
        Ok(RateLimitDecision::Allowed) => Ok(()),
        Ok(RateLimitDecision::Limited { retry_after_secs }) => {
            Err(AppError::RateLimited { retry_after_secs })
        }
        Err(error) => {
            tracing::error!(?error, "invitation rate limiter unavailable");
            Err(AppError::ServiceUnavailable)
        }
    }
}

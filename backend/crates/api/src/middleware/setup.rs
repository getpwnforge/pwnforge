// crates/api/src/middleware/setup.rs
use crate::{services::setup_service::{self, SetupError}, state::AppState, error::AppError};
use axum::{extract::FromRequestParts, http::request::Parts};


/// Proof that the caller holds the boot token.
///
/// An extractor rather than a layer, same reasoning as AuthUser: the
/// requirement is visible in the handler signature and cannot be forgotten
/// when a route is added.
pub struct SetupToken;

impl FromRequestParts<AppState> for SetupToken {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // A header rather than the body, so GET routes are covered by the
        // same check.
        let presented = parts
            .headers
            .get("x-setup-token")
            .and_then(|v| v.to_str().ok())
            .ok_or(SetupError::InvalidToken)?;

        let expected = state.setup_token.as_ref().map(|t| t.as_str());
        setup_service::verify_token(expected, presented)?;

        Ok(SetupToken)
    }
}

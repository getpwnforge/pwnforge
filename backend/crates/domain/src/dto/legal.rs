//! Legal acceptance DTOs.
//!
//! Target path: backend/crates/domain/src/dto/legal.rs
//! Declare with `pub mod legal;` in backend/crates/domain/src/dto/mod.rs

use serde::{Deserialize, Serialize};
use validator::Validate;

/// Versions the client says it displayed. Embedded in `RegisterRequest` and
/// used as the body of `POST /api/v1/legal/accept`.
///
/// The client sends what it showed rather than a bare boolean, so that a tab
/// left open for three weeks cannot silently accept a superseded text.
#[derive(Debug, Clone, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct LegalAcceptanceInput {
    #[validate(length(min = 1, max = 32))]
    pub terms_version: String,
    #[validate(length(min = 1, max = 32))]
    pub privacy_version: String,
}

/// Versions in force, served unauthenticated so the registration form can
/// send back what it displayed rather than a bare boolean.
#[derive(Debug, Clone, Serialize)]
pub struct LegalVersionsDto {
    pub terms_version: &'static str,
    pub privacy_version: &'static str,
}

/// Where the user stands with respect to the current documents.
#[derive(Debug, Clone, Serialize)]
pub struct LegalStatusDto {
    pub state: LegalState,
    pub current_terms_version: &'static str,
    pub current_privacy_version: &'static str,
    /// Versions currently on record, `None` if never accepted.
    pub accepted_terms_version: Option<String>,
    pub accepted_privacy_version: Option<String>,
    /// RFC 3339, present only while a revision is published but not yet
    /// enforced.
    pub effective_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LegalState {
    /// Accepted versions are current. Nothing to display.
    Ok,
    /// A revision is published but not yet enforced. Show a dismissible
    /// banner; the user may accept early.
    Pending,
}

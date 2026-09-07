//! Legal document versions and discriminator.
//!
//! Target path: backend/crates/domain/src/legal.rs
//! Declare with `pub mod legal;` in backend/crates/domain/src/lib.rs
//!
//! Lives in `domain` rather than `services` because both `services` and the
//! integration tests read these constants, and `domain` is the only crate
//! sitting below both in the dependency graph.
//!
//! The version strings here are the single source of truth on the backend
//! side. They must match the `version` field in the frontmatter of
//! frontend/public/legal/{fr,en}/{terms,privacy}.md. That correspondence is
//! enforced by a test, because a silent drift would record acceptances
//! pointing at a text that no longer exists.

use serde::{Deserialize, Serialize};

/* -------------------------------------------------------------------------- */
/* Versions                                                                   */
/* -------------------------------------------------------------------------- */

pub const CURRENT_TERMS_VERSION: &str = "1.0";
pub const CURRENT_PRIVACY_VERSION: &str = "1.0";

/// Versions that are still considered valid.
///
/// A minor revision (typo, clarification) is published under a new version
/// number that is *added* to this list: nobody is prompted again. A material
/// revision removes the superseded versions, which forces every user to
/// accept again.
pub const VALID_TERMS_VERSIONS: &[&str] = &["1.0"];
pub const VALID_PRIVACY_VERSIONS: &[&str] = &["1.0"];

/// RFC 3339 instant at which a pending material revision starts being
/// enforced. `None` means no revision is pending.
///
/// Between publication and this instant the user is informed but not blocked,
/// which is what implements the 30 day notice promised in the terms.
pub const LEGAL_EFFECTIVE_AT: Option<&str> = None;

/* -------------------------------------------------------------------------- */
/* Document discriminator                                                     */
/* -------------------------------------------------------------------------- */

/// Discriminator stored in `legal_acceptances.document`.
///
/// The column is `TEXT`, consistent with `auth_tokens.kind` and
/// `instance_alerts.kind`, so the closed set is enforced here rather than by
/// a Postgres enum.
///
/// Note that the privacy policy is *acknowledged*, not consented to: the legal
/// basis for the processing is contract performance, not consent. The row is
/// evidence that the information was provided, and must never be presented to
/// the user as a consent that could be withdrawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LegalDocument {
    Terms,
    Privacy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AcceptanceMethod {
    /// Checkbox at registration.
    Explicit,
    /// Continued use after the notice period, per article 23 of the terms.
    Tacit,
    /// Instance operator, recorded at setup. Not an acceptance: the operator
    /// is the party publishing the documents, not a counterparty to them.
    /// Present so the account has a baseline version and the cache columns
    /// keep a single writer.
    Operator,
}

impl AcceptanceMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Explicit => "explicit",
            Self::Tacit => "tacit",
            Self::Operator => "operator",
        }
    }
}

impl LegalDocument {
    pub const ALL: [LegalDocument; 2] = [LegalDocument::Terms, LegalDocument::Privacy];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Terms => "terms",
            Self::Privacy => "privacy",
        }
    }

    /// Returns `None` for an unknown value rather than erroring.
    ///
    /// An unrecognised string can only come from a manual write to the
    /// database. The service treats it as an absent acceptance, which means
    /// re-acceptance is required: the safe direction.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "terms" => Some(Self::Terms),
            "privacy" => Some(Self::Privacy),
            _ => None,
        }
    }

    pub fn current_version(self) -> &'static str {
        match self {
            Self::Terms => CURRENT_TERMS_VERSION,
            Self::Privacy => CURRENT_PRIVACY_VERSION,
        }
    }

    pub fn valid_versions(self) -> &'static [&'static str] {
        match self {
            Self::Terms => VALID_TERMS_VERSIONS,
            Self::Privacy => VALID_PRIVACY_VERSIONS,
        }
    }

    pub fn is_valid(self, version: &str) -> bool {
        self.valid_versions().contains(&version)
    }
}

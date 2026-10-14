// backend/crates/domain/src/dto/invitation.rs
//
// What team and workspace invitations share at the API edge.

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::{Validate, ValidationError, ValidationErrors};

/// How long an invitation lasts when its author does not say, and the only
/// duration a workspace invitation has.
pub const INVITATION_DEFAULT_DAYS: i64 = 7;

/// The furthest a team invitation's author may set its expiry.
pub const TEAM_INVITATION_MAX_DAYS: i64 = 30;

/// `?invited_by=<user id>` on the member and invitation lists: only the rows
/// that account brought in.
///
/// Read as a string so a malformed id answers with the usual 422 body rather
/// than the plain text a query rejection would give.
#[derive(Debug, Default, Deserialize)]
pub struct InvitedByFilter {
    pub invited_by: Option<String>,
}

impl InvitedByFilter {
    pub fn parse(&self) -> Result<Option<Uuid>, ValidationErrors> {
        match self.invited_by.as_deref() {
            None => Ok(None),
            Some(raw) => Uuid::parse_str(raw).map(Some).map_err(|_| {
                let mut errors = ValidationErrors::new();
                errors.add("invited_by", ValidationError::new("uuid"));
                errors
            }),
        }
    }
}

/// An invitation token, in the body so it stays out of the request span and
/// the access logs, as `JoinTeamRequest` does.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct InvitationTokenRequest {
    #[validate(length(min = 1, max = 128))]
    pub token: String,
}

/// Whom an invitation names, as the invitation lists show it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum InvitationTarget {
    /// One account, named by username: `invited_username`.
    Username,
    /// One address, for whoever holds it verified: `invited_email`.
    Email,
    /// Nobody: a team's generic link, for whoever holds its token. A
    /// workspace has none.
    Link,
}

impl InvitationTarget {
    /// From the two targeting columns, which never both hold a value.
    pub fn of(invited_user_id: Option<Uuid>, invited_email: Option<&str>) -> Self {
        match (invited_user_id, invited_email) {
            (Some(_), _) => Self::Username,
            (None, Some(_)) => Self::Email,
            (None, None) => Self::Link,
        }
    }
}

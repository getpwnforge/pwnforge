// backend/crates/domain/src/dto/team.rs

use crate::entities::teams;
use crate::types::AccentColor;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::{Validate, ValidationError};

/// A member's role within a team.
//
// Stored in the database as raw TEXT (`team_members.role`,
// `team_invitations.default_role`). The String <-> TeamRole conversion
// happens in the service layer, not via `DeriveActiveEnum` on the generated
// entity, to avoid depending on a manual edit after
// `sea-orm-cli generate entity`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum TeamRole {
    Captain,
    Admin,
    Member,
    Viewer,
}

impl TeamRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            TeamRole::Captain => "captain",
            TeamRole::Admin => "admin",
            TeamRole::Member => "member",
            TeamRole::Viewer => "viewer",
        }
    }

    /// Ordinal used to compare roles by seniority (higher = more privileged).
    pub fn level(&self) -> u8 {
        match self {
            TeamRole::Viewer => 0,
            TeamRole::Member => 1,
            TeamRole::Admin => 2,
            TeamRole::Captain => 3,
        }
    }
}

impl std::str::FromStr for TeamRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "captain" => Ok(TeamRole::Captain),
            "admin" => Ok(TeamRole::Admin),
            "member" => Ok(TeamRole::Member),
            "viewer" => Ok(TeamRole::Viewer),
            _ => Err("Invalid team role".into()),
        }
    }
}

// ---- Teams --------------------------------------------------------------

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateTeamRequest {
    // Bounded like every other free-text field in the API (see
    // `dto/auth.rs`): the column is unbounded TEXT, so without a ceiling a
    // single request could store the whole body limit, once per team, with no
    // cap on how many teams a user creates.
    #[validate(length(min = 1, max = 60))]
    pub name: String,
    pub color: AccentColor,
}

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TeamPatch {
    #[validate(length(min = 1, max = 60))]
    pub name: Option<String>,
    pub color: Option<AccentColor>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TeamResponse {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub color: AccentColor,
    /// `null` when the team has no avatar. Otherwise the cache-busting version
    /// the client puts in the avatar URL, which it builds from the id:
    /// `/api/v1/avatars/teams/{id}/{avatar_version}/256.webp`.
    //
    // The URL is built client side rather than stored, so no row ever names
    // an object in the bucket.
    pub avatar_version: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    /// Current user's role in this team, avoids an extra frontend round trip
    /// to know which actions to show.
    pub my_role: TeamRole,
}

impl TeamResponse {
    /// Builds the response from a stored row plus the caller's role.
    ///
    /// Single place where `teams.color` is read back into `AccentColor`, so the
    /// fallback for an out-of-palette value cannot drift between the six call
    /// sites that used to construct this by hand.
    pub fn from_model(team: teams::Model, my_role: TeamRole) -> Self {
        Self {
            id: team.id,
            slug: team.slug,
            name: team.name,
            color: team.color.parse().unwrap_or_default(),
            avatar_version: team.avatar_version,
            created_at: team.created_at.into(),
            my_role,
        }
    }
}

// ---- Members --------------------------------------------------------------

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TeamMemberResponse {
    pub user_id: Uuid,
    pub username: String,
    pub first_name: String,
    pub last_name: String,
    /// Same contract as `TeamResponse.avatar_version`, under
    /// `/api/v1/avatars/users/{user_id}/{avatar_version}/64.webp`.
    pub avatar_version: Option<Uuid>,
    pub role: TeamRole,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ChangeMemberRoleRequest {
    pub role: TeamRole,
}

// ---- Invitations ----------------------------------------------------------

/// Rejects an invitation that names a person *and* an address.
///
/// The two are different ways to say who the invitation is for, and letting
/// both through produced a row whose targeting fields disagreed: the id said
/// one account, the address said another, and the three acceptance paths did
/// not consult the same one. An Admin could then issue an invitation that read
/// as addressed to a colleague while actually granting the team to whoever
/// controlled the mailbox.
///
/// There is no need for the caller to supply an address alongside a username:
/// the service looks up where to write to the named account itself.
fn invitee_is_named_once(request: &CreateInvitationRequest) -> Result<(), ValidationError> {
    if request.username.is_some() && request.invited_email.is_some() {
        return Err(ValidationError::new("invitee_named_twice"));
    }

    Ok(())
}

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
#[validate(schema(function = invitee_is_named_once))]
pub struct CreateInvitationRequest {
    pub default_role: TeamRole,
    pub expires_at: Option<DateTime<Utc>>,
    /// Ignored, and set to 1, when `username` or `invited_email` is given.
    pub max_uses: Option<i32>,
    /// The only account allowed to accept, by username. Left out for a
    /// generic invitation link, which anyone holding the token may accept.
    /// Mutually exclusive with `invited_email`.
    //
    // A username rather than a raw user id: the id is an internal handle the
    // caller has no legitimate way to obtain for someone outside their team,
    // and accepting one invited probing for valid accounts by identifier.
    // UUIDv7 encodes a timestamp, so that guessing game is cheaper than it
    // looks. A username is already a public identifier in this project,
    // since registration answers `username_taken` outright, so resolving by
    // name adds no enumeration surface that does not already exist.
    //
    // Mutual exclusion is `invitee_is_named_once`. The service finds the
    // account's own verified address to write to, so there is nothing for
    // the caller to supply here beyond the name.
    //
    // Same bounds as `RegisterRequest::username`: an unbounded string would
    // otherwise reach the lookup untouched.
    #[validate(length(min = 3, max = 32), regex(path = *crate::dto::auth::USERNAME_RE))]
    pub username: Option<String>,
    /// Invites someone who has no account here yet: they get a link to sign
    /// up, and join on verifying this address.
    ///
    /// Mutually exclusive with `username`. If the address already belongs to a
    /// verified account, the invitation is for that account, the same as
    /// naming it.
    #[validate(email)]
    pub invited_email: Option<String>,
}

/// Body of `POST /teams/join`.
//
// The token travels in the body rather than the path so it stays out of the
// `tracing` request span (which records `path` at info level on every
// request) and out of nginx's access log. Same bounds as the other opaque
// tokens in `dto/auth.rs`: a generated one is 43 characters, the ceiling only
// exists to bound what reaches the hashing step.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JoinTeamRequest {
    #[validate(length(min = 1, max = 128))]
    pub token: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TeamInvitationResponse {
    pub id: Uuid,
    /// Only in the response that creates the invitation, never returned by
    /// any listing afterwards.
    //
    // Neither `list_active_invitations` nor
    // `list_pending_invitations_for_user` fills it.
    pub token: Option<String>,
    pub default_role: TeamRole,
    pub invited_user_id: Option<Uuid>,
    pub invited_email: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub max_uses: Option<i32>,
    pub uses: i32,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Negative: deserialization refuses an unknown colour outright, which is
    /// what turns a bad request into a 422 without any explicit check.
    #[test]
    fn create_team_request_rejects_an_unknown_color() {
        let body = r##"{"name":"Team","color":"#EB5927"}"##;
        assert!(serde_json::from_str::<CreateTeamRequest>(body).is_err());
    }

    /// Negative: an unexpected field is refused rather than silently dropped,
    /// matching `deny_unknown_fields` on the auth DTOs. `icon` is the field
    /// that was just removed, so this also pins that it stays gone.
    #[test]
    fn create_team_request_rejects_an_unknown_field() {
        let body = r#"{"name":"Team","color":"ember","icon":"skull"}"#;
        assert!(serde_json::from_str::<CreateTeamRequest>(body).is_err());
    }

    /// Positive: a well-formed body deserializes and validates.
    #[test]
    fn create_team_request_accepts_a_palette_color() {
        let body = r#"{"name":"Team","color":"violet"}"#;
        let parsed: CreateTeamRequest =
            serde_json::from_str(body).expect("a palette colour must be accepted");

        assert_eq!(parsed.color, AccentColor::Violet);
        assert!(parsed.validate().is_ok());
    }

    /// Negative: the name ceiling is enforced, so an unbounded TEXT column
    /// cannot be filled from a single request.
    #[test]
    fn create_team_request_rejects_an_overlong_name() {
        let parsed = CreateTeamRequest {
            name: "a".repeat(61),
            color: AccentColor::Ember,
        };

        assert!(parsed.validate().is_err());
    }

    fn invitation(username: Option<&str>, email: Option<&str>) -> CreateInvitationRequest {
        CreateInvitationRequest {
            default_role: TeamRole::Member,
            expires_at: None,
            max_uses: None,
            username: username.map(str::to_owned),
            invited_email: email.map(str::to_owned),
        }
    }

    /// Negative: naming a person *and* an address is refused.
    ///
    /// Accepting both produced a row whose two targeting columns disagreed,
    /// and the three acceptance paths did not all consult the same one. An
    /// invitation could read as addressed to a colleague while granting the
    /// team to whoever controlled the mailbox.
    #[test]
    fn invitation_naming_both_a_username_and_an_email_is_refused() {
        let errors = invitation(Some("alice"), Some("someone@example.test"))
            .validate()
            .expect_err("naming the invitee twice must not validate");

        assert!(
            format!("{errors:?}").contains("invitee_named_twice"),
            "the rejection must carry its own code, got {errors:?}"
        );
    }

    /// Positive: each way of naming an invitee is fine on its own, and a
    /// generic link names nobody at all.
    #[test]
    fn invitation_naming_the_invitee_once_or_not_at_all_is_accepted() {
        assert!(invitation(Some("alice"), None).validate().is_ok());
        assert!(
            invitation(None, Some("someone@example.test"))
                .validate()
                .is_ok()
        );
        assert!(invitation(None, None).validate().is_ok());
    }

    /// The username bounds match `RegisterRequest`, so a name that could never
    /// have been registered is refused before it reaches the lookup.
    #[test]
    fn invitation_rejects_a_username_that_could_not_exist() {
        for bad in ["ab", &"a".repeat(33), "_leading", "has space", "has/slash"] {
            assert!(
                invitation(Some(bad), None).validate().is_err(),
                "{bad:?} must not be accepted as a username"
            );
        }
    }
}

// crates/domain/src/dto/workspace.rs

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::dto::team::TeamRole;
use crate::serde_helpers::deserialize_some;

/// Access outcome for a user on a workspace: no access, explicitly excluded,
/// sole ownership of a solo workspace, or an active role inherited/overridden
/// from team membership. Not a variant of `TeamRole` on purpose: neither
/// `Owner` nor `Excluded` corresponds to a string ever stored in
/// `team_members.role` or `workspace_members.role`, both are outcomes
/// computed by `permissions_service::effective_role`, never persisted as-is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Membership {
    None,
    Excluded,
    Owner,
    Active(TeamRole),
}

/// Payload for POST /workspaces. Deliberately has no `owner_id`: that's
/// always the authenticated actor for solo mode, never client-supplied
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateWorkspaceInput {
    pub name: String,
    pub description: Option<String>,
    pub color: String,
    pub r#type: WorkspaceType,
    pub platform: Option<Platform>,
    pub mode: WorkspaceMode,
    /// Required when `mode == WorkspaceMode::Team`, ignored for solo.
    /// Cross-field validation happens in `create_workspace`, not here.
    pub team_id: Option<Uuid>,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub lock_after_end: Option<bool>,
    pub default_writeup_visibility: Option<WriteupVisibility>,
}

/// Payload for PATCH /workspaces/:slug. No `mode`, `team_id`, or `owner_id`
/// field on purpose, a structural change can't even be expressed by this
/// type (workspace_service::update_workspace is a second guard, not
/// the only one).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateWorkspaceInput {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub description: Option<Option<String>>,
    pub color: Option<String>,
    // Pas d'avatar ici : il n'est pas modifiable par ce PATCH générique,
    // uniquement par POST /api/v1/avatars/workspaces/{id}, qui a son propre
    // contrôle de permission, son propre rate limit et son propre pipeline de
    // ré-encodage.
    pub lock_after_end: Option<bool>,
    pub default_writeup_visibility: Option<WriteupVisibility>,
}

/// One card in `WorkspaceSelector`.
#[derive(Debug, Serialize)]
pub struct WorkspaceSummary {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub color: String,
    /// Same contract as `TeamResponse::avatar_version`, under
    /// `/api/v1/avatars/workspaces/{id}/{avatar_version}/256.webp`.
    pub avatar_version: Option<Uuid>,
    pub r#type: WorkspaceType,
    pub mode: WorkspaceMode,
    pub archived: bool,
    pub favorite: bool,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
}

/// One row of `WorkspaceMembersScreen`, after `effective_role` has
/// resolved inheritance and overrides. Never built for an `Excluded`
/// membership, that case is filtered out upstream.
#[derive(Debug, Serialize)]
pub struct EffectiveMember {
    pub user_id: Uuid,
    pub handle: String,
    pub role: Membership,
    pub is_guest: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceType {
    Platform,
    Event,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceMode {
    Solo,
    Team,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Htb,
    Thm,
    RootMe,
    Ctftime,
    Other,
}

/// Only two values confirmed so far, from `writeups.visibility` : 'draft' | 'team'.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteupVisibility {
    Draft,
    Team,
}

impl WorkspaceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkspaceType::Platform => "platform",
            WorkspaceType::Event => "event",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "platform" => Some(Self::Platform),
            "event" => Some(Self::Event),
            _ => None,
        }
    }
}

impl WorkspaceMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkspaceMode::Solo => "solo",
            WorkspaceMode::Team => "team",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "solo" => Some(Self::Solo),
            "team" => Some(Self::Team),
            _ => None,
        }
    }
}

impl Platform {
    pub fn as_str(&self) -> &'static str {
        match self {
            Platform::Htb => "htb",
            Platform::Thm => "thm",
            Platform::RootMe => "rootme",
            Platform::Ctftime => "ctftime",
            Platform::Other => "other",
        }
    }
}

impl WriteupVisibility {
    pub fn as_str(&self) -> &'static str {
        match self {
            WriteupVisibility::Draft => "draft",
            WriteupVisibility::Team => "team",
        }
    }
}

impl Membership {
    pub fn as_str(&self) -> &'static str {
        match self {
            Membership::None => "none",
            Membership::Excluded => "excluded",
            Membership::Owner => "owner",
            Membership::Active(role) => role.as_str(),
        }
    }
}

/// Manual rather than derived: a derive on an enum mixing unit variants with
/// `Active(TeamRole)` would serialize as `{"Active": "captain"}`, not a flat
/// string. `EffectiveMember.role` needs the same flat shape as every other
/// role string the API returns (`"captain"`, `"owner"`, ...).
impl Serialize for Membership {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

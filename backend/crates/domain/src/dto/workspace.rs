// crates/domain/src/dto/workspace.rs

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::openapi::RefOr;
use utoipa::openapi::schema::{ObjectBuilder, Schema, Type};
use utoipa::{PartialSchema, ToSchema};
use uuid::Uuid;
use validator::Validate;

use crate::dto::team::{TeamColor, TeamRole};
use crate::entities::workspaces;
use crate::serde_helpers::deserialize_some;

/// Access outcome for a user on a workspace: no access, explicitly excluded,
/// sole ownership of a solo workspace, or an active role inherited/overridden
/// from team membership. Not a variant of `TeamRole` on purpose: neither
/// `Owner` nor `Excluded` corresponds to a string ever stored in
/// `team_members.role` or `workspace_members.role`, both are outcomes
/// computed by `permissions_service::effective_role`, never persisted as-is.
///
/// No `ToSchema` derive: it would describe serde's default enum shape
/// (`"None"`, `{"Active": ...}`), not the flat string the manual `Serialize`
/// below emits. The schema is implemented by hand next to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Membership {
    None,
    Excluded,
    Owner,
    Active(TeamRole),
}

/// Payload for POST /workspaces. Deliberately has no `owner_id`: that's
/// always the authenticated actor for solo mode, never client-supplied.
#[derive(Debug, Deserialize, Validate, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateWorkspaceInput {
    #[validate(length(min = 1, max = 100))]
    pub name: String,
    #[validate(length(max = 1000))]
    pub description: Option<String>,
    /// Closed set enforced by serde, shared with teams: see `TeamColor`.
    pub color: TeamColor,
    pub r#type: WorkspaceType,
    pub platform: Option<Platform>,
    pub mode: WorkspaceMode,
    /// Required when `mode == WorkspaceMode::Team`, rejected for solo
    /// (`InvalidOwnership`). Cross-field validation happens in `create_workspace`.
    pub team_id: Option<Uuid>,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub lock_after_end: Option<bool>,
    pub default_writeup_visibility: Option<WriteupVisibility>,
}

/// Payload for PATCH /workspaces/{slug}. No `mode`, `team_id`, or `owner_id`
/// field on purpose: a structural change can't even be expressed by this
/// type (`workspace_service::update_workspace` is a second guard, not the
/// only one).
///
/// No avatar either: it is only changed through
/// POST /api/v1/avatars/workspaces/{id}, which has its own permission check,
/// its own rate limit and its own re-encoding pipeline.
#[derive(Debug, Deserialize, Validate, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateWorkspaceInput {
    /// Same bounds as `CreateWorkspaceInput::name`, checked only when present.
    #[validate(length(min = 1, max = 100))]
    pub name: Option<String>,
    /// Absent leaves it unchanged, explicit null clears it, a string sets it.
    /// `value_type` because utoipa has no schema for the double Option; check
    /// the exported spec documents it as one nullable string. If validator
    /// refuses `length` on `Option<Option<String>>`, replace it with a
    /// `custom` function that only inspects `Some(Some(s))`.
    #[serde(default, deserialize_with = "deserialize_some")]
    #[schema(value_type = Option<String>)]
    #[validate(length(max = 1000))]
    pub description: Option<Option<String>>,
    pub color: Option<TeamColor>,
    pub lock_after_end: Option<bool>,
    pub default_writeup_visibility: Option<WriteupVisibility>,
}

/// One card in `WorkspaceSelector`.
#[derive(Debug, Serialize, ToSchema)]
pub struct WorkspaceSummary {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub color: TeamColor,
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

/// One workspace as GET, POST and PATCH return it, with the caller's own
/// access. Only built for a caller already admitted, so `my_role` is never
/// `None` or `Excluded`.
#[derive(Debug, Serialize, ToSchema)]
pub struct WorkspaceResponse {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub color: TeamColor,
    pub avatar_version: Option<Uuid>,
    pub r#type: WorkspaceType,
    pub platform: Option<Platform>,
    pub mode: WorkspaceMode,
    pub team_id: Option<Uuid>,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub lock_after_end: bool,
    pub default_writeup_visibility: WriteupVisibility,
    pub archived: bool,
    pub created_at: DateTime<Utc>,
    pub my_role: Membership,
}

impl WorkspaceResponse {
    /// `None` when a stored enum value is unrecognised; the caller turns it
    /// into `WorkspaceError::DataIntegrity`.
    pub fn try_from_model(model: workspaces::Model, my_role: Membership) -> Option<Self> {
        // A response for someone with no access is a bug upstream.
        debug_assert!(my_role.satisfies(TeamRole::Viewer));

        let r#type = WorkspaceType::parse(&model.r#type)?;
        let mode = WorkspaceMode::parse(&model.mode)?;
        let default_writeup_visibility =
            WriteupVisibility::parse(&model.default_writeup_visibility)?;

        // A NULL column is "no platform"; an unrecognised value is corrupt
        // data and fails the whole conversion rather than reading as absent.
        let platform = match model.platform.as_deref() {
            None => None,
            Some(raw) => Some(Platform::parse(raw)?),
        };

        Some(Self {
            id: model.id,
            slug: model.slug,
            name: model.name,
            description: model.description,
            // A display field: falls back to Ember like `TeamResponse::from_model`
            // instead of failing the read.
            color: model.color.parse().unwrap_or_default(),
            avatar_version: model.avatar_version,
            r#type,
            platform,
            mode,
            team_id: model.team_id,
            starts_at: model.starts_at.map(Into::into),
            ends_at: model.ends_at.map(Into::into),
            lock_after_end: model.lock_after_end,
            default_writeup_visibility,
            archived: model.archived,
            created_at: model.created_at.into(),
            my_role,
        })
    }
}

/// One row of `WorkspaceMembersScreen`, after `effective_role` has
/// resolved inheritance and overrides. Never built for an `Excluded`
/// membership, that case is filtered out upstream.
///
/// Carries the same display fields as `TeamMemberResponse` because the
/// members table needs them: `workspaces.members.columns` lists `member` and
/// `handle` as two separate columns, so the first one is a name and an
/// avatar rather than a repeat of the handle, and `joined` is a third.
/// Serving them from here is one query; leaving them out would have the page
/// fetch the team roster a second time just to fill three cells.
///
/// The one column not served from here is `solved`, an aggregate over
/// challenges that belongs to the stats path, not to membership resolution.
#[derive(Debug, Serialize, ToSchema)]
pub struct EffectiveMember {
    pub user_id: Uuid,
    pub handle: String,
    pub first_name: String,
    pub last_name: String,
    /// Same contract as `TeamResponse::avatar_version`, under
    /// `/api/v1/avatars/users/{user_id}/{avatar_version}/64.webp`.
    pub avatar_version: Option<Uuid>,
    pub role: Membership,
    pub is_guest: bool,
    /// When this person gained access to *this workspace*, which is not the
    /// same date as joining the team: a guest's is their `workspace_members`
    /// row, a team member's is their `team_members` row, and a solo owner's
    /// is the workspace's own creation date, there being no membership row
    /// behind sole ownership.
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceType {
    Platform,
    Event,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceMode {
    Solo,
    Team,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Htb,
    Thm,
    RootMe,
    Ctftime,
    Other,
}

/// Only two values confirmed so far, from `writeups.visibility`: 'draft' | 'team'.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
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

    /// Mirrors `as_str()`, not the serde names: the database holds "rootme",
    /// while the API spells it "root_me" (`rename_all = "snake_case"`).
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "htb" => Some(Self::Htb),
            "thm" => Some(Self::Thm),
            "rootme" => Some(Self::RootMe),
            "ctftime" => Some(Self::Ctftime),
            "other" => Some(Self::Other),
            _ => None,
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

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "draft" => Some(Self::Draft),
            "team" => Some(Self::Team),
            _ => None,
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

    /// Seniority rank for ordering a roster, higher = more privileged.
    /// Extends `TeamRole::level()` upwards: `Owner` outranks every team
    /// role, since it is sole ownership of a solo workspace.
    ///
    /// Deliberately a method and not an `Ord` impl: `None` and `Excluded`
    /// are absences of access rather than points on the same scale, and
    /// giving them a comparison operator would invite sorting by it
    /// somewhere it means nothing. They rank below every real role here
    /// only so that this total function has an answer for them.
    pub fn rank(&self) -> u8 {
        match self {
            Membership::None | Membership::Excluded => 0,
            Membership::Active(role) => role.level() + 1,
            Membership::Owner => u8::MAX,
        }
    }

    /// Whether this access outcome grants at least `min` on the workspace.
    ///
    /// Not built on `rank()`: that one gives `None` and `Excluded` a value only
    /// so a roster can be sorted, and a permission check must never treat an
    /// absence of access as the bottom of a scale.
    pub fn satisfies(&self, min: TeamRole) -> bool {
        match self {
            Membership::None | Membership::Excluded => false,
            Membership::Owner => true,
            Membership::Active(role) => role.level() >= min.level(),
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

/// Lists only the values a response can carry. `none` and `excluded` are
/// serializable but never served: the extractor refuses those callers and
/// `list_effective_members` skips those rows.
impl PartialSchema for Membership {
    fn schema() -> RefOr<Schema> {
        // Derived from as_str() rather than written as literals, so the schema
        // cannot drift from what Serialize emits.
        let served = [
            Membership::Owner,
            Membership::Active(TeamRole::Captain),
            Membership::Active(TeamRole::Admin),
            Membership::Active(TeamRole::Member),
            Membership::Active(TeamRole::Viewer),
        ];
        let values = served.iter().map(Membership::as_str);

        ObjectBuilder::new()
            .schema_type(Type::String)
            .enum_values(Some(values))
            .description(Some("Effective access on a workspace"))
            .into()
    }
}

/// Default name() gives "Membership"; no referenced schemas to collect.
impl ToSchema for Membership {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Positive: the rank orders a roster the way the members table reads it,
    /// most privileged first, with `Owner` above every team role.
    #[test]
    fn membership_rank_orders_by_seniority() {
        let descending = [
            Membership::Owner,
            Membership::Active(TeamRole::Captain),
            Membership::Active(TeamRole::Admin),
            Membership::Active(TeamRole::Member),
            Membership::Active(TeamRole::Viewer),
        ];

        for pair in descending.windows(2) {
            assert!(
                pair[0].rank() > pair[1].rank(),
                "{} must outrank {}",
                pair[0].as_str(),
                pair[1].as_str()
            );
        }
    }

    /// The two non-roles sort below every real one. They are never built into
    /// an `EffectiveMember`, so this only pins that the function stays total
    /// rather than describing an order anyone should rely on.
    #[test]
    fn membership_rank_puts_the_absences_of_access_last() {
        assert_eq!(Membership::None.rank(), 0);
        assert_eq!(Membership::Excluded.rank(), 0);
        assert!(Membership::Active(TeamRole::Viewer).rank() > Membership::None.rank());
    }

    /// `Membership` serializes as a flat string, the same shape as every other
    /// role the API returns, rather than as `{"Active": "captain"}`.
    #[test]
    fn membership_serializes_as_a_flat_string() {
        assert_eq!(
            serde_json::to_string(&Membership::Active(TeamRole::Captain)).unwrap(),
            r#""captain""#
        );
        assert_eq!(
            serde_json::to_string(&Membership::Owner).unwrap(),
            r#""owner""#
        );
    }

    #[test]
    fn no_access_never_satisfies_any_role() {
        assert!(
            !Membership::None.satisfies(TeamRole::Viewer),
            "none must not satisfy viewer"
        );
        assert!(
            !Membership::None.satisfies(TeamRole::Member),
            "none must not satisfy member"
        );
        assert!(
            !Membership::None.satisfies(TeamRole::Admin),
            "none must not satisfy admin"
        );
        assert!(
            !Membership::None.satisfies(TeamRole::Captain),
            "none must not satisfy captain"
        );
        assert!(
            !Membership::Excluded.satisfies(TeamRole::Viewer),
            "excluded must not satisfy viewer"
        );
        assert!(
            !Membership::Excluded.satisfies(TeamRole::Member),
            "excluded must not satisfy member"
        );
        assert!(
            !Membership::Excluded.satisfies(TeamRole::Admin),
            "excluded must not satisfy admin"
        );
        assert!(
            !Membership::Excluded.satisfies(TeamRole::Captain),
            "excluded must not satisfy captain"
        );
    }

    #[test]
    fn owner_satisfies_every_role() {
        assert!(
            Membership::Owner.satisfies(TeamRole::Viewer),
            "owner must satisfy viewer"
        );
        assert!(
            Membership::Owner.satisfies(TeamRole::Member),
            "owner must satisfy member"
        );
        assert!(
            Membership::Owner.satisfies(TeamRole::Admin),
            "owner must satisfy admin"
        );
        assert!(
            Membership::Owner.satisfies(TeamRole::Captain),
            "owner must satisfy captain"
        );
    }

    /// Literal truth table, deliberately not derived from `level()`: a test
    /// computing its expectations the same way as the code would pass even if
    /// both were wrong.
    #[test]
    fn an_active_role_satisfies_its_level_and_below_only() {
        assert!(
            Membership::Active(TeamRole::Viewer).satisfies(TeamRole::Viewer),
            "viewer must satisfy viewer"
        );
        assert!(
            !Membership::Active(TeamRole::Viewer).satisfies(TeamRole::Member),
            "viewer must not satisfy member"
        );
        assert!(
            !Membership::Active(TeamRole::Viewer).satisfies(TeamRole::Admin),
            "viewer must not satisfy admin"
        );
        assert!(
            !Membership::Active(TeamRole::Viewer).satisfies(TeamRole::Captain),
            "viewer must not satisfy captain"
        );

        assert!(
            Membership::Active(TeamRole::Member).satisfies(TeamRole::Viewer),
            "member must satisfy viewer"
        );
        assert!(
            Membership::Active(TeamRole::Member).satisfies(TeamRole::Member),
            "member must satisfy member"
        );
        assert!(
            !Membership::Active(TeamRole::Member).satisfies(TeamRole::Admin),
            "member must not satisfy admin"
        );
        assert!(
            !Membership::Active(TeamRole::Member).satisfies(TeamRole::Captain),
            "member must not satisfy captain"
        );

        assert!(
            Membership::Active(TeamRole::Admin).satisfies(TeamRole::Viewer),
            "admin must satisfy viewer"
        );
        assert!(
            Membership::Active(TeamRole::Admin).satisfies(TeamRole::Member),
            "admin must satisfy member"
        );
        assert!(
            Membership::Active(TeamRole::Admin).satisfies(TeamRole::Admin),
            "admin must satisfy admin"
        );
        assert!(
            !Membership::Active(TeamRole::Admin).satisfies(TeamRole::Captain),
            "admin must not satisfy captain"
        );

        assert!(
            Membership::Active(TeamRole::Captain).satisfies(TeamRole::Viewer),
            "captain must satisfy viewer"
        );
        assert!(
            Membership::Active(TeamRole::Captain).satisfies(TeamRole::Member),
            "captain must satisfy member"
        );
        assert!(
            Membership::Active(TeamRole::Captain).satisfies(TeamRole::Admin),
            "captain must satisfy admin"
        );
        assert!(
            Membership::Active(TeamRole::Captain).satisfies(TeamRole::Captain),
            "captain must satisfy captain"
        );
    }

    /// Pins the assumption behind the hand-written schema: five served values,
    /// never the two absences of access.
    #[test]
    fn membership_schema_lists_exactly_the_served_values() {
        let schema = serde_json::to_value(Membership::schema()).unwrap();
        let values: Vec<&str> = schema["enum"]
            .as_array()
            .expect("the schema must carry an enum array")
            .iter()
            .map(|value| value.as_str().expect("every enum value is a string"))
            .collect();

        assert_eq!(values.len(), 5, "got {values:?}");
        for served in ["owner", "captain", "admin", "member", "viewer"] {
            assert!(values.contains(&served), "{served} missing from {values:?}");
        }
        for absent in ["none", "excluded"] {
            assert!(!values.contains(&absent), "{absent} must not be served");
        }
    }
}

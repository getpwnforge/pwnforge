// backend/crates/services/src/permissions_service.rs
use domain::dto::team::TeamRole;
use domain::dto::workspace::{Membership, RoleSource};
use domain::entities::{team_members, workspace_members, workspaces};

use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum PermissionsError {
    #[error(transparent)]
    Db(#[from] sea_orm::DbErr),

    /// `mode = 'team'` with no `team_id` violates `ck_workspaces_ownership`,
    /// so this should be unreachable in practice. Kept as an explicit
    /// variant rather than an unwrap so a data-integrity bug surfaces as a
    /// named error instead of a panic.
    #[error("workspace has mode=team but no team_id")]
    WorkspaceMisconfigured,
}

/// One person's access to a workspace, and where it comes from. `source` is
/// `None` exactly when `membership` grants nothing (`None`, `Excluded`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Access {
    pub membership: Membership,
    pub source: Option<RoleSource>,
}

impl Access {
    const NONE: Self = Self {
        membership: Membership::None,
        source: None,
    };
}

/// Resolves what `user_id` can do on `workspace`, folding together solo
/// ownership, team inheritance, and workspace-level overrides.
///
/// Generic over the connection so a write can resolve it inside its own
/// transaction, on rows it has locked.
pub async fn effective_role<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    workspace: &workspaces::Model,
) -> Result<Membership, PermissionsError> {
    if workspace.team_id.is_none() {
        return Ok(resolve(workspace, user_id, None, None).membership);
    }

    let row = workspace_members::Entity::find()
        .filter(workspace_members::Column::WorkspaceId.eq(workspace.id))
        .filter(workspace_members::Column::UserId.eq(user_id))
        .one(db)
        .await?;

    let team_id = workspace
        .team_id
        .ok_or(PermissionsError::WorkspaceMisconfigured)?;

    let team_role = team_members::Entity::find()
        .filter(team_members::Column::TeamId.eq(team_id))
        .filter(team_members::Column::UserId.eq(user_id))
        .one(db)
        .await?
        .map(|tm| TeamRole::from_str(&tm.role).unwrap_or(TeamRole::Viewer));

    Ok(resolve(workspace, user_id, row.as_ref(), team_role).membership)
}

/// The rule `effective_role` applies, on rows already read: `row` is the
/// person's `workspace_members` row on this workspace, `team_role` their
/// role on its team. The workspace and member lists read those rows in bulk
/// and resolve each person here, so the rule exists once, whatever reads it.
///
/// A workspace with no team answers from `owner_id` alone. On a team
/// workspace, a row holding a role decides first, before team membership:
/// the exclusion sentinel denies, any other role grants it. A row with no
/// role overrides nothing and falls through to the team role, rather than
/// denying access just because it exists (it is not a guest: guests are
/// required to carry a role).
pub fn resolve(
    workspace: &workspaces::Model,
    user_id: Uuid,
    row: Option<&workspace_members::Model>,
    team_role: Option<TeamRole>,
) -> Access {
    if workspace.team_id.is_none() {
        return if workspace.owner_id == Some(user_id) {
            Access {
                membership: Membership::Owner,
                source: Some(RoleSource::Owner),
            }
        } else {
            Access::NONE
        };
    }

    if let Some(role) = row.and_then(|row| row.role.as_deref()) {
        // The sentinel comes from `Membership::Excluded` rather than a
        // literal here and in `workspace_service`, so the reader and the
        // writer cannot drift apart on its spelling.
        if role == Membership::Excluded.as_str() {
            return Access {
                membership: Membership::Excluded,
                source: None,
            };
        }

        // A role set here is an override for a team member, whatever the
        // row says about how it was written: someone admitted as a guest
        // who later joined the team is the team member they now are. Off
        // the team, it is a guest's role, the only such row that can be
        // written (`set_member_override`) or survive a departure from the
        // team (`team_service::remove_member`).
        let source = if team_role.is_some() {
            RoleSource::Override
        } else {
            RoleSource::Guest
        };

        return Access {
            membership: Membership::Active(TeamRole::from_str(role).unwrap_or(TeamRole::Viewer)),
            source: Some(source),
        };
    }

    match team_role {
        Some(role) => Access {
            membership: Membership::Active(role),
            source: Some(RoleSource::Team),
        },
        None => Access::NONE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(team_id: Option<Uuid>, owner_id: Option<Uuid>) -> workspaces::Model {
        let now = chrono::Utc::now().into();
        workspaces::Model {
            id: Uuid::now_v7(),
            team_id,
            owner_id,
            slug: "ws".to_owned(),
            name: "Workspace".to_owned(),
            description: None,
            color: "blue".to_owned(),
            avatar_version: None,
            r#type: "event".to_owned(),
            platform: None,
            mode: if team_id.is_some() { "team" } else { "solo" }.to_owned(),
            starts_at: None,
            ends_at: None,
            lock_after_end: false,
            default_writeup_visibility: "team".to_owned(),
            archived: false,
            created_at: now,
            updated_at: now,
        }
    }

    fn row(
        workspace: &workspaces::Model,
        user_id: Uuid,
        role: Option<&str>,
        is_guest: bool,
    ) -> workspace_members::Model {
        workspace_members::Model {
            workspace_id: workspace.id,
            user_id,
            role: role.map(str::to_owned),
            is_guest,
            joined_at: chrono::Utc::now().into(),
            invited_by: None,
        }
    }

    fn active(role: TeamRole, source: RoleSource) -> Access {
        Access {
            membership: Membership::Active(role),
            source: Some(source),
        }
    }

    /// A solo workspace answers from its owner alone, whatever rows exist.
    #[test]
    fn a_solo_workspace_has_its_owner_and_nobody_else() {
        let owner = Uuid::now_v7();
        let other = Uuid::now_v7();
        let solo = workspace(None, Some(owner));
        let stray = row(&solo, other, Some("admin"), true);

        assert_eq!(
            resolve(&solo, owner, None, None),
            Access {
                membership: Membership::Owner,
                source: Some(RoleSource::Owner),
            }
        );
        assert_eq!(
            resolve(&solo, other, Some(&stray), Some(TeamRole::Admin)),
            Access::NONE
        );
    }

    /// Literal truth table for a team workspace, written out rather than
    /// derived from the code under test.
    #[test]
    fn a_team_workspace_resolves_rows_before_team_membership() {
        let user = Uuid::now_v7();
        let ws = workspace(Some(Uuid::now_v7()), None);
        let excluded = row(&ws, user, Some(Membership::Excluded.as_str()), false);
        let overridden = row(&ws, user, Some("viewer"), false);
        let guest = row(&ws, user, Some("member"), true);
        let role_less = row(&ws, user, None, false);

        assert_eq!(
            resolve(&ws, user, None, Some(TeamRole::Admin)),
            active(TeamRole::Admin, RoleSource::Team),
        );
        assert_eq!(resolve(&ws, user, None, None), Access::NONE);
        assert_eq!(
            resolve(&ws, user, Some(&excluded), Some(TeamRole::Captain)),
            Access {
                membership: Membership::Excluded,
                source: None,
            }
        );
        assert_eq!(
            resolve(&ws, user, Some(&overridden), Some(TeamRole::Admin)),
            active(TeamRole::Viewer, RoleSource::Override),
        );
        assert_eq!(
            resolve(&ws, user, Some(&guest), None),
            active(TeamRole::Member, RoleSource::Guest),
        );
        assert_eq!(
            resolve(&ws, user, Some(&guest), Some(TeamRole::Viewer)),
            active(TeamRole::Member, RoleSource::Override),
            "a guest who joined the team keeps the row's role, as an override"
        );
        assert_eq!(
            resolve(&ws, user, Some(&role_less), Some(TeamRole::Member)),
            active(TeamRole::Member, RoleSource::Team),
            "a row with no role overrides nothing"
        );
        assert_eq!(resolve(&ws, user, Some(&role_less), None), Access::NONE);
    }
}

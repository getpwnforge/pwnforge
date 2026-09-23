// backend/crates/services/src/permissions_service.rs
use domain::dto::team::TeamRole;
use domain::dto::workspace::Membership;
use domain::entities::{team_members, workspace_members, workspaces};

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
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

/// Resolves what `user_id` can do on `workspace`, folding together solo
/// ownership, team inheritance, and workspace-level overrides.
pub async fn effective_role(
    db: &DatabaseConnection,
    user_id: Uuid,
    workspace: &workspaces::Model,
) -> Result<Membership, PermissionsError> {
    if workspace.team_id.is_none() {
        return Ok(if workspace.owner_id == Some(user_id) {
            Membership::Owner
        } else {
            Membership::None
        });
    }

    let member = workspace_members::Entity::find()
        .filter(workspace_members::Column::WorkspaceId.eq(workspace.id))
        .filter(workspace_members::Column::UserId.eq(user_id))
        .one(db)
        .await?;

    // A row whose `role` is `None` overrides nothing (it is not a guest -
    // guests are required to carry a role), so it falls through to team
    // membership below rather than denying access just because it exists.
    if let Some(member) = member
        && let Some(role) = member.role
    {
        return if role == "excluded" {
            Ok(Membership::Excluded)
        } else {
            Ok(Membership::Active(
                TeamRole::from_str(&role).unwrap_or(TeamRole::Viewer),
            ))
        };
    }

    let team_id = workspace
        .team_id
        .ok_or(PermissionsError::WorkspaceMisconfigured)?;

    let team_member = team_members::Entity::find()
        .filter(team_members::Column::TeamId.eq(team_id))
        .filter(team_members::Column::UserId.eq(user_id))
        .one(db)
        .await?;

    Ok(match team_member {
        Some(tm) => Membership::Active(TeamRole::from_str(&tm.role).unwrap_or(TeamRole::Viewer)),
        None => Membership::None,
    })
}

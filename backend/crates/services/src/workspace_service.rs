// backend/crates/services/src/workspace_service.rs

use domain::dto::team::TeamRole;
use domain::dto::workspace::{
    CreateWorkspaceInput, EffectiveMember, Membership, UpdateWorkspaceInput, WorkspaceMode,
    WorkspaceSummary, WorkspaceType, WriteupVisibility,
};
use domain::entities::{team_members, workspace_favorites, workspace_members, workspaces};

use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use uuid::Uuid;

use crate::{permissions_service, slug_service, team_service};

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error(transparent)]
    Db(#[from] sea_orm::DbErr),

    #[error(transparent)]
    Slug(#[from] slug_service::SlugError),

    #[error(transparent)]
    Team(#[from] team_service::TeamError),

    #[error(transparent)]
    Permissions(#[from] permissions_service::PermissionsError),

    #[error("workspace not found")]
    WorkspaceNotFound,

    #[error("input does not satisfy ck_workspaces_ownership")]
    InvalidOwnership,

    #[error("workspace {0} has an unrecognized type or mode value in the database")]
    DataIntegrity(Uuid),
}

/// Minimum team role required to create a workspace under that team.
/// Arbitrary threshold, easy to revisit: swap this constant if Member should
/// also be allowed to create workspaces.
const MIN_ROLE_TO_CREATE_WORKSPACE: TeamRole = TeamRole::Admin;

/// Creates a workspace, either solo (owned directly by `actor_id`) or under
/// a team, and returns the persisted row.
pub async fn create_workspace(
    db: &DatabaseConnection,
    actor_id: Uuid,
    input: CreateWorkspaceInput,
) -> Result<workspaces::Model, WorkspaceError> {
    validate_ownership(input.mode, input.team_id)?;

    let (team_id, owner_id) = match input.mode {
        WorkspaceMode::Team => (input.team_id, None),
        WorkspaceMode::Solo => (None, Some(actor_id)),
    };

    if let WorkspaceMode::Team = input.mode {
        let team_id =
            team_id.expect("validate_ownership guarantees team_id is Some for WorkspaceMode::Team");
        let role = team_service::get_user_role_in_team(db, team_id, actor_id)
            .await?
            .ok_or(WorkspaceError::WorkspaceNotFound)?;
        team_service::require_role(role, MIN_ROLE_TO_CREATE_WORKSPACE)?;
    }

    let slug = slug_service::generate_unique_slug(&input.name, move |candidate| async move {
        Ok(workspaces::Entity::find()
            .filter(workspaces::Column::Slug.eq(candidate))
            .one(db)
            .await?
            .is_some())
    })
    .await?;

    let workspace = workspaces::ActiveModel {
        name: Set(input.name),
        slug: Set(slug),
        description: Set(input.description),
        color: Set(input.color),
        r#type: Set(input.r#type.as_str().to_string()),
        platform: Set(input.platform.map(|p| p.as_str().to_string())),
        mode: Set(input.mode.as_str().to_string()),
        team_id: Set(team_id),
        owner_id: Set(owner_id),
        starts_at: Set(input.starts_at.map(Into::into)),
        ends_at: Set(input.ends_at.map(Into::into)),
        lock_after_end: Set(input.lock_after_end.unwrap_or(true)),
        default_writeup_visibility: Set(input
            .default_writeup_visibility
            .unwrap_or(WriteupVisibility::Draft)
            .as_str()
            .to_string()),
        ..Default::default()
    }
    .insert(db)
    .await?;

    Ok(workspace)
}

pub async fn get_workspace_by_slug(
    db: &DatabaseConnection,
    slug: &str,
) -> Result<workspaces::Model, WorkspaceError> {
    let workspace = workspaces::Entity::find()
        .filter(workspaces::Column::Slug.eq(slug))
        .one(db)
        .await?
        .ok_or(WorkspaceError::WorkspaceNotFound)?;

    Ok(workspace)
}

/// Resolves a workspace by slug and the caller's membership as a single
/// unit, mirroring `team_service::get_team_for_member`: a missing workspace
/// and a workspace the caller has no access to both surface as
/// `WorkspaceNotFound`, so a non-member can't tell the two apart.
pub async fn get_workspace_for_member(
    db: &DatabaseConnection,
    slug: &str,
    user_id: Uuid,
) -> Result<(workspaces::Model, Membership), WorkspaceError> {
    let workspace = get_workspace_by_slug(db, slug).await?;
    let membership = permissions_service::effective_role(db, user_id, &workspace).await?;

    match membership {
        Membership::None | Membership::Excluded => Err(WorkspaceError::WorkspaceNotFound),
        Membership::Owner | Membership::Active(_) => Ok((workspace, membership)),
    }
}

/// Every workspace `user_id` can currently see: solo workspaces they own,
/// team workspaces where they're a team member and not `Excluded` via a
/// `workspace_members` override, and workspaces where they only appear as
/// a guest.
pub async fn list_workspaces_for_user(
    db: &DatabaseConnection,
    user_id: Uuid,
) -> Result<Vec<WorkspaceSummary>, WorkspaceError> {
    // Solo workspaces owned directly.
    let solo = workspaces::Entity::find()
        .filter(workspaces::Column::OwnerId.eq(user_id))
        .all(db)
        .await?;

    // Teams this user belongs to, then the workspaces under those teams.
    let team_ids: Vec<Uuid> = team_members::Entity::find()
        .filter(team_members::Column::UserId.eq(user_id))
        .all(db)
        .await?
        .into_iter()
        .map(|tm| tm.team_id)
        .collect();

    let team_workspaces = if team_ids.is_empty() {
        Vec::new()
    } else {
        workspaces::Entity::find()
            .filter(workspaces::Column::TeamId.is_in(team_ids))
            .all(db)
            .await?
    };

    // Every override row for this user, fetched once rather than per
    // workspace, indexed by workspace id.
    let overrides_by_workspace: std::collections::HashMap<Uuid, workspace_members::Model> =
        workspace_members::Entity::find()
            .filter(workspace_members::Column::UserId.eq(user_id))
            .all(db)
            .await?
            .into_iter()
            .map(|o| (o.workspace_id, o))
            .collect();

    // Drop any team workspace where this user is explicitly excluded.
    let team_workspaces: Vec<workspaces::Model> = team_workspaces
        .into_iter()
        .filter(|w| {
            overrides_by_workspace
                .get(&w.id)
                .and_then(|o| o.role.as_deref())
                != Some("excluded")
        })
        .collect();

    // Guest workspaces: any override row with is_guest = true. A row
    // can't be both excluded and a guest (one role field per row), so no
    // extra filter needed here.
    let guest_workspace_ids: Vec<Uuid> = overrides_by_workspace
        .values()
        .filter(|o| o.is_guest)
        .map(|o| o.workspace_id)
        .collect();

    let guest_workspaces = if guest_workspace_ids.is_empty() {
        Vec::new()
    } else {
        workspaces::Entity::find()
            .filter(workspaces::Column::Id.is_in(guest_workspace_ids))
            .all(db)
            .await?
    };

    // Union, dedupe on id: a user shouldn't be both a team member and a
    // guest on the same workspace, but don't assume the data enforces it.
    let mut by_id: std::collections::HashMap<Uuid, workspaces::Model> =
        std::collections::HashMap::new();
    for w in solo
        .into_iter()
        .chain(team_workspaces)
        .chain(guest_workspaces)
    {
        by_id.entry(w.id).or_insert(w);
    }

    // Favorites for this user, one query.
    let favorite_ids: std::collections::HashSet<Uuid> = workspace_favorites::Entity::find()
        .filter(workspace_favorites::Column::UserId.eq(user_id))
        .all(db)
        .await?
        .into_iter()
        .map(|f| f.workspace_id)
        .collect();

    // Map to DTOs.
    by_id
        .into_values()
        .map(|w| {
            Ok(WorkspaceSummary {
                r#type: WorkspaceType::parse(&w.r#type)
                    .ok_or(WorkspaceError::DataIntegrity(w.id))?,
                mode: WorkspaceMode::parse(&w.mode).ok_or(WorkspaceError::DataIntegrity(w.id))?,
                favorite: favorite_ids.contains(&w.id),
                id: w.id,
                slug: w.slug,
                name: w.name,
                color: w.color,
                avatar_version: w.avatar_version,
                archived: w.archived,
                starts_at: w.starts_at.map(Into::into),
                ends_at: w.ends_at.map(Into::into),
            })
        })
        .collect()
}

/// Steps:
/// 1. `get_workspace_by_slug`-style fetch by id instead
///    (`workspaces::Entity::find_by_id(workspace_id).one(db).await?.ok_or(
///    WorkspaceError::WorkspaceNotFound)?`).
/// 2. Reject the update if `input` tries to touch `mode`, `team_id`, or
///    `owner_id` (shouldn't be on `UpdateWorkspaceInput` at all, guard here
///    too regardless).
/// 3. Build an `ActiveModel` from the fetched row, `Set` only the fields
///    present in `input`.
/// 4. `.update(db).await?`, return the updated row.
pub async fn update_workspace(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    input: UpdateWorkspaceInput,
) -> Result<workspaces::Model, WorkspaceError> {
    let workspace = workspaces::Entity::find_by_id(workspace_id)
        .one(db)
        .await?
        .ok_or(WorkspaceError::WorkspaceNotFound)?;

    // Apply the update
    let mut active_model: workspaces::ActiveModel = workspace.into();
    if let Some(name) = input.name {
        active_model.name = Set(name);
    }
    if let Some(description) = input.description {
        active_model.description = Set(description);
    }
    if let Some(color) = input.color {
        active_model.color = Set(color);
    }
    if let Some(lock_after_end) = input.lock_after_end {
        active_model.lock_after_end = Set(lock_after_end);
    }
    if let Some(default_writeup_visibility) = input.default_writeup_visibility {
        active_model.default_writeup_visibility =
            Set(default_writeup_visibility.as_str().to_string());
    }
    active_model.update(db).await.map_err(Into::into)
}

/// Steps:
/// 1. `workspaces::Entity::delete_by_id(workspace_id).exec(db).await?`.
/// 2. `if result.rows_affected == 0 { return
///    Err(WorkspaceError::WorkspaceNotFound); }`, matching
///    `team_service::delete_team`'s convention exactly.
#[allow(
    unused_variables,
    dead_code,
    reason = "TODO(Phase 3): documented stub, still todo!(). Remove this attribute with the todo!()."
)]
pub async fn delete_workspace(
    db: &DatabaseConnection,
    workspace_id: Uuid,
) -> Result<(), WorkspaceError> {
    todo!()
}

/// Steps: fetch by id (`WorkspaceNotFound` if missing), `Set(archived:
/// true)`, `.update()`. Archiving an already-archived workspace: no-op
/// success, not an error.
#[allow(
    unused_variables,
    dead_code,
    reason = "TODO(Phase 3): documented stub, still todo!(). Remove this attribute with the todo!()."
)]
pub async fn archive_workspace(
    db: &DatabaseConnection,
    workspace_id: Uuid,
) -> Result<workspaces::Model, WorkspaceError> {
    todo!()
}

/// Mirror of `archive_workspace`, `Set(archived: false)`.
#[allow(
    unused_variables,
    dead_code,
    reason = "TODO(Phase 3): documented stub, still todo!(). Remove this attribute with the todo!()."
)]
pub async fn unarchive_workspace(
    db: &DatabaseConnection,
    workspace_id: Uuid,
) -> Result<workspaces::Model, WorkspaceError> {
    todo!()
}

/// Steps:
/// 1. `workspace_favorites::ActiveModel { user_id, workspace_id, .. }.insert(db).await`.
/// 2. On a unique-violation error, treat as success. `is_unique_violation`
///    below is a private copy, same as `team_service.rs` keeps its own
///    rather than sharing one: the only thing this module needs to know is
///    whether the row already existed.
#[allow(
    unused_variables,
    dead_code,
    reason = "TODO(Phase 3): documented stub, still todo!(). Remove this attribute with the todo!()."
)]
pub async fn add_favorite(
    db: &DatabaseConnection,
    user_id: Uuid,
    workspace_id: Uuid,
) -> Result<(), WorkspaceError> {
    todo!()
}

/// Steps: `workspace_favorites::Entity::delete_by_id((user_id,
/// workspace_id)).exec(db).await?`, ignore whether a row was deleted.
#[allow(
    unused_variables,
    dead_code,
    reason = "TODO(Phase 3): documented stub, still todo!(). Remove this attribute with the todo!()."
)]
pub async fn remove_favorite(
    db: &DatabaseConnection,
    user_id: Uuid,
    workspace_id: Uuid,
) -> Result<(), WorkspaceError> {
    todo!()
}

/// Everyone with access to `workspace`, with their effective role resolved.
///
/// Steps:
/// 1. If `workspace.mode` is solo: one `EffectiveMember` for
///    `workspace.owner_id`, role `TeamRole::Captain` (the top of the
///    hierarchy, standing in for sole ownership since there's no separate
///    "Owner" tier at the `TeamRole` level, only at `Membership`), `is_guest: false`.
/// 2. If team mode: fetch all `team_members` rows for `workspace.team_id`,
///    joined with `users` (`find_also_related`, same pattern as
///    `team_service::list_team_members`) for display fields.
/// 3. Fetch all `workspace_members` override rows for `workspace.id`, index
///    by `user_id`.
/// 4. For each team member: drop entirely if override has
///    `role == "excluded"`; use the override's role if it has another one;
///    otherwise their plain team role.
/// 5. Add every `workspace_members` row with `is_guest = true` that has no
///    corresponding `team_members` row, joined with `users` too.
/// 6. Return the combined list.
#[allow(
    unused_variables,
    dead_code,
    reason = "TODO(Phase 3): documented stub, still todo!(). Remove this attribute with the todo!()."
)]
pub async fn list_effective_members(
    db: &DatabaseConnection,
    workspace: &workspaces::Model,
) -> Result<Vec<EffectiveMember>, WorkspaceError> {
    todo!()
}

/// Steps:
/// 1. Validate `ck_workspace_members_guest_has_role`: `is_guest` true
///    requires a role.
/// 2. Upsert on the composite PK (`workspace_id, user_id`). Check whether
///    `OnConflict` is used anywhere in `team_service.rs`'s sibling files
///    for this shape before writing a manual select-then-insert-or-update
///    (it isn't used in `team_service.rs` itself, `add_member` just checks
///    existence then inserts, consider mirroring that instead of reaching
///    for `OnConflict`).
/// 3. Return `Ok(())`.
///
/// `role: TeamRole` here can't express exclusion (`"excluded"` isn't a
/// `TeamRole` variant, it's a `Membership` state). Decide whether this
/// takes a raw validated string, or whether exclusion should be its own
/// function (`exclude_member`) instead of a value of this one's `role`
/// parameter.
#[allow(
    unused_variables,
    dead_code,
    reason = "TODO(Phase 3): documented stub, still todo!(). Remove this attribute with the todo!()."
)]
pub async fn set_member_override(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    target_user_id: Uuid,
    role: TeamRole,
    is_guest: bool,
) -> Result<(), WorkspaceError> {
    todo!()
}

/// Steps: `workspace_members::Entity::delete_by_id((workspace_id,
/// target_user_id)).exec(db).await?`, no error if nothing was deleted.
#[allow(
    unused_variables,
    dead_code,
    reason = "TODO(Phase 3): documented stub, still todo!(). Remove this attribute with the todo!()."
)]
pub async fn remove_member_override(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    target_user_id: Uuid,
) -> Result<(), WorkspaceError> {
    todo!()
}

/// Steps: `workspaces::Entity::find().filter(workspaces::Column::Slug.eq(candidate))
/// .one(db).await?.is_some()`. Same shape as the closure inline in
/// `team_service::create_team`, kept as a named function here since it's
/// called from more than one place (create, and later rename if that's
/// ever supported).
#[allow(
    unused_variables,
    dead_code,
    reason = "TODO(Phase 3): documented stub, still todo!(). Remove this attribute with the todo!()."
)]
async fn workspace_slug_exists(
    db: &DatabaseConnection,
    candidate: String,
) -> Result<bool, sea_orm::DbErr> {
    todo!()
}

/// Copied rather than imported, same reasoning as `team_service.rs`'s own
/// copy: the only thing this module needs is whether the row already
/// existed.
#[allow(
    unused_variables,
    dead_code,
    reason = "TODO(Phase 3): documented stub, still todo!(). Remove this attribute with the todo!()."
)]
fn is_unique_violation(err: &sea_orm::DbErr) -> bool {
    matches!(
        err.sql_err(),
        Some(sea_orm::SqlErr::UniqueConstraintViolation(_))
    )
}

fn validate_ownership(mode: WorkspaceMode, team_id: Option<Uuid>) -> Result<(), WorkspaceError> {
    match (mode, team_id) {
        (WorkspaceMode::Team, Some(_)) => Ok(()),
        (WorkspaceMode::Solo, None) => Ok(()),
        _ => Err(WorkspaceError::InvalidOwnership),
    }
}

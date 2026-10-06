// backend/crates/services/src/workspace_service.rs

use domain::dto::team::TeamRole;
use domain::dto::workspace::{
    CreateWorkspaceInput, EffectiveMember, Membership, UpdateWorkspaceInput, WorkspaceMode,
    WorkspaceSummary, WorkspaceType, WriteupVisibility,
};
use domain::entities::{team_members, users, workspace_favorites, workspace_members, workspaces};

use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use std::str::FromStr;
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

    #[error("user is not a member of this workspace's team")]
    TargetNotInTeam,

    #[error("user is already a member of this workspace's team, so cannot be a guest of it")]
    TargetAlreadyInTeam,

    #[error("a solo workspace has no members to override")]
    SoloWorkspace,
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
        color: Set(input.color.as_str().to_string()),
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
                != Some(Membership::Excluded.as_str())
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
                color: w.color.parse().unwrap_or_default(),
                avatar_version: w.avatar_version,
                archived: w.archived,
                starts_at: w.starts_at.map(Into::into),
                ends_at: w.ends_at.map(Into::into),
            })
        })
        .collect()
}

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
        active_model.color = Set(color.as_str().to_string());
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

pub async fn delete_workspace(
    db: &DatabaseConnection,
    workspace_id: Uuid,
) -> Result<(), WorkspaceError> {
    let result = workspaces::Entity::delete_by_id(workspace_id)
        .exec(db)
        .await?;

    if result.rows_affected == 0 {
        return Err(WorkspaceError::WorkspaceNotFound);
    }
    Ok(())
}

pub async fn archive_workspace(
    db: &DatabaseConnection,
    workspace_id: Uuid,
) -> Result<workspaces::Model, WorkspaceError> {
    let workspace = workspaces::Entity::find_by_id(workspace_id)
        .one(db)
        .await?
        .ok_or(WorkspaceError::WorkspaceNotFound)?;

    workspaces::ActiveModel {
        id: Set(workspace.id),
        archived: Set(true),
        ..Default::default()
    }
    .update(db)
    .await
    .map_err(Into::into)
}

pub async fn unarchive_workspace(
    db: &DatabaseConnection,
    workspace_id: Uuid,
) -> Result<workspaces::Model, WorkspaceError> {
    let workspace = workspaces::Entity::find_by_id(workspace_id)
        .one(db)
        .await?
        .ok_or(WorkspaceError::WorkspaceNotFound)?;

    workspaces::ActiveModel {
        id: Set(workspace.id),
        archived: Set(false),
        ..Default::default()
    }
    .update(db)
    .await
    .map_err(Into::into)
}

pub async fn add_favorite(
    db: &DatabaseConnection,
    user_id: Uuid,
    workspace_id: Uuid,
) -> Result<(), WorkspaceError> {
    let inserted = workspace_favorites::ActiveModel {
        user_id: Set(user_id),
        workspace_id: Set(workspace_id),
        ..Default::default()
    }
    .insert(db)
    .await;

    match inserted {
        Ok(_) => Ok(()),
        // Favouriting something already favourited is the same wish stated
        // twice, not a failure, so the caller gets the outcome it asked for.
        //
        // Caught after the fact rather than prevented by a prior SELECT: two
        // concurrent requests both pass an existence check and one still
        // loses on the insert, so the check would narrow the race without
        // closing it. The constraint is the thing that actually decides, so
        // it is the thing to read.
        Err(err) if is_unique_violation(&err) => Ok(()),
        Err(err) => Err(err.into()),
    }
}

pub async fn remove_favorite(
    db: &DatabaseConnection,
    user_id: Uuid,
    workspace_id: Uuid,
) -> Result<(), WorkspaceError> {
    workspace_favorites::Entity::delete_by_id((user_id, workspace_id))
        .exec(db)
        .await?;

    Ok(())
}

/// Everyone with access to `workspace`, with their effective role resolved.
///
/// Solo: a single entry for the owner, `Membership::Owner` rather than
/// `Active(Captain)`. `Membership` has an `Owner` tier where `TeamRole` does
/// not, and using it is what keeps this roster and
/// `permissions_service::effective_role` saying the same thing about the
/// same person.
///
/// Team: every team member, minus anyone a `workspace_members` row excludes,
/// with an override role taking precedence over the inherited team one, plus
/// every guest, who has no team membership behind them at all.
///
/// A member whose `users` row has gone is dropped rather than failing the
/// whole listing, matching `team_service::list_team_members`.
pub async fn list_effective_members(
    db: &DatabaseConnection,
    workspace: &workspaces::Model,
) -> Result<Vec<EffectiveMember>, WorkspaceError> {
    let mode =
        WorkspaceMode::parse(&workspace.mode).ok_or(WorkspaceError::DataIntegrity(workspace.id))?;

    if let WorkspaceMode::Solo = mode {
        // `ck_workspaces_ownership` makes a solo workspace with no owner
        // unreachable, so this is the same named-error-over-unwrap choice as
        // `PermissionsError::WorkspaceMisconfigured`.
        let owner_id = workspace
            .owner_id
            .ok_or(WorkspaceError::DataIntegrity(workspace.id))?;

        let owner = users::Entity::find_by_id(owner_id).one(db).await?;

        return Ok(owner
            .map(|owner| EffectiveMember {
                user_id: owner_id,
                handle: owner.username,
                first_name: owner.first_name.unwrap_or_default(),
                last_name: owner.last_name.unwrap_or_default(),
                avatar_version: owner.avatar_version,
                role: Membership::Owner,
                is_guest: false,
                // No membership row stands behind sole ownership, so the
                // workspace's own creation is the date the owner got access.
                joined_at: workspace.created_at.into(),
            })
            .into_iter()
            .collect());
    }

    // Likewise unreachable: `mode = 'team'` with no `team_id` is the other
    // half of `ck_workspaces_ownership`.
    let team_id = workspace
        .team_id
        .ok_or(WorkspaceError::DataIntegrity(workspace.id))?;

    let team_roster = team_members::Entity::find()
        .filter(team_members::Column::TeamId.eq(team_id))
        .find_also_related(users::Entity)
        .all(db)
        .await?;

    // Overrides come back with their user rows already joined, so the guest
    // pass below needs no second query.
    let mut overrides: std::collections::HashMap<
        Uuid,
        (workspace_members::Model, Option<users::Model>),
    > = workspace_members::Entity::find()
        .filter(workspace_members::Column::WorkspaceId.eq(workspace.id))
        .find_also_related(users::Entity)
        .all(db)
        .await?
        .into_iter()
        .map(|(member, user)| (member.user_id, (member, user)))
        .collect();

    let mut members = Vec::with_capacity(team_roster.len());

    for (team_member, user) in team_roster {
        let Some(user) = user else { continue };

        // Removed rather than read, so what is left in `overrides` after this
        // loop is exactly the set of rows with no team membership behind
        // them: the guests.
        let override_role = overrides
            .remove(&team_member.user_id)
            .and_then(|(member, _)| member.role);

        let role = match override_role.as_deref() {
            Some(role) if role == Membership::Excluded.as_str() => continue,
            Some(role) => TeamRole::from_str(role).unwrap_or(TeamRole::Viewer),
            // A row with no role overrides nothing and falls through to the
            // team role, the same way `permissions_service::effective_role`
            // treats it.
            None => TeamRole::from_str(&team_member.role).unwrap_or(TeamRole::Viewer),
        };

        members.push(EffectiveMember {
            user_id: team_member.user_id,
            handle: user.username,
            first_name: user.first_name.unwrap_or_default(),
            last_name: user.last_name.unwrap_or_default(),
            avatar_version: user.avatar_version,
            role: Membership::Active(role),
            is_guest: false,
            // The team_members date, not the override's: an override
            // changes what they may do here, not when they gained access.
            joined_at: team_member.joined_at.into(),
        });
    }

    for (member, user) in overrides.into_values() {
        // Everything still here belongs to someone with no team membership
        // behind it, so `is_guest = false` is not a case this filter handles
        // quietly, it is one that cannot arise. Two guards make that true,
        // one at each end of the row's life: `set_member_override` refuses to
        // write a non-guest override for someone who is not on the team, and
        // `team_service::remove_member` deletes these rows in the same
        // transaction that ends the membership.
        //
        // It matters that they cannot, because skipping such a row would deny
        // nothing: this function gates no access, and
        // `permissions_service::effective_role` reads the override before it
        // reads team membership, so it would still admit them. Leaving the
        // row possible would mean someone with access to the workspace and no
        // place on its members list.
        if !member.is_guest {
            continue;
        }

        let Some(user) = user else { continue };

        // `ck_workspace_members_guest_has_role` makes a role-less guest
        // unreachable. Unlike the orphan above, this one really is
        // unreachable rather than merely unhandled, so there is nothing to
        // fix upstream and dropping the row is only defensive.
        let Some(role) = member.role else { continue };

        if role == Membership::Excluded.as_str() {
            continue;
        }

        members.push(EffectiveMember {
            user_id: member.user_id,
            handle: user.username,
            first_name: user.first_name.unwrap_or_default(),
            last_name: user.last_name.unwrap_or_default(),
            avatar_version: user.avatar_version,
            role: Membership::Active(TeamRole::from_str(&role).unwrap_or(TeamRole::Viewer)),
            is_guest: true,
            joined_at: member.joined_at.into(),
        });
    }

    // Without this the order is whatever Postgres returns, which is not
    // stable across reads: the members table would reshuffle on every
    // refresh. Seniority first, then handle, so the people who can change
    // things are at the top.
    //
    // `to_lowercase` because the handles come from a CITEXT column: a byte
    // ordering would file `Zoe` before `alice`, which is not the order the
    // column itself considers those two to be in.
    members.sort_by_cached_key(|member| {
        (
            std::cmp::Reverse(member.role.rank()),
            member.handle.to_lowercase(),
        )
    });

    Ok(members)
}

/// Grants `target_user_id` a workspace-level role, overriding whatever they
/// inherit from the team, or admits them as a guest.
///
/// Refuses a non-guest override for someone who is not on the workspace's
/// team. That guard is the creation-side half of the one
/// `team_service::remove_member` enforces on deletion: without it, the row
/// that removal exists to clean up could simply be written directly, and
/// `permissions_service::effective_role` would honour it, since it reads the
/// override before it reads team membership.
///
/// Refuses the mirror case too, `is_guest` for someone who *is* on the team:
/// a guest is by definition someone with no team membership behind them, and
/// a row claiming otherwise would be a fact the roster then contradicts.
/// (This blocks writing that state, not reaching it: someone admitted as a
/// guest who later joins the team arrives there legitimately, and
/// `list_effective_members` reads them as the team member they now are.)
///
/// Exclusion is not expressible here and lives in `exclude_member`: it is not
/// a `TeamRole`, and a role parameter that sometimes means "no role at all"
/// is the kind that gets passed `Captain` by mistake.
pub async fn set_member_override(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    target_user_id: Uuid,
    role: TeamRole,
    is_guest: bool,
) -> Result<(), WorkspaceError> {
    let team_id = require_team_workspace(db, workspace_id).await?;
    let in_team = is_team_member(db, team_id, target_user_id).await?;

    match (is_guest, in_team) {
        (false, false) => return Err(WorkspaceError::TargetNotInTeam),
        (true, true) => return Err(WorkspaceError::TargetAlreadyInTeam),
        _ => {}
    }

    upsert_override(db, workspace_id, target_user_id, role.as_str(), is_guest).await
}

/// Removes `target_user_id`'s access to one workspace while leaving their
/// team membership intact.
///
/// Its own function rather than a value of `set_member_override`'s `role`,
/// and only for team members: an exclusion subtracts from inherited access,
/// so for someone with none it would name nothing. To drop a guest, delete
/// their row instead.
pub async fn exclude_member(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    target_user_id: Uuid,
) -> Result<(), WorkspaceError> {
    let team_id = require_team_workspace(db, workspace_id).await?;

    if !is_team_member(db, team_id, target_user_id).await? {
        return Err(WorkspaceError::TargetNotInTeam);
    }

    upsert_override(
        db,
        workspace_id,
        target_user_id,
        Membership::Excluded.as_str(),
        false,
    )
    .await
}

/// The workspace's team, or an error naming why it has none.
///
/// A `workspace_members` row on a solo workspace is inert: both
/// `permissions_service::effective_role` and `list_effective_members` answer
/// from `owner_id` and never look at the table. Writing one would be a
/// silent no-op, so it is refused rather than accepted and ignored.
async fn require_team_workspace(
    db: &DatabaseConnection,
    workspace_id: Uuid,
) -> Result<Uuid, WorkspaceError> {
    let workspace = workspaces::Entity::find_by_id(workspace_id)
        .one(db)
        .await?
        .ok_or(WorkspaceError::WorkspaceNotFound)?;

    workspace.team_id.ok_or(WorkspaceError::SoloWorkspace)
}

async fn is_team_member(
    db: &DatabaseConnection,
    team_id: Uuid,
    user_id: Uuid,
) -> Result<bool, WorkspaceError> {
    Ok(team_members::Entity::find_by_id((team_id, user_id))
        .one(db)
        .await?
        .is_some())
}

/// Writes the row whether or not one is already there.
///
/// Select-then-insert-or-update rather than `OnConflict`, mirroring
/// `team_service::add_member`'s explicit shape. `joined_at` is left `NotSet`
/// in both branches: the database fills it on insert, and an update must not
/// reset it, since changing what someone may do here is not them arriving
/// again.
async fn upsert_override(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    user_id: Uuid,
    role: &str,
    is_guest: bool,
) -> Result<(), WorkspaceError> {
    let row = workspace_members::ActiveModel {
        workspace_id: Set(workspace_id),
        user_id: Set(user_id),
        role: Set(Some(role.to_owned())),
        is_guest: Set(is_guest),
        ..Default::default()
    };

    let exists = workspace_members::Entity::find_by_id((workspace_id, user_id))
        .one(db)
        .await?
        .is_some();

    if exists {
        row.update(db).await?;
    } else {
        row.insert(db).await?;
    }

    Ok(())
}

/// Drops `target_user_id`'s row for this workspace, whatever it held: a
/// granted role, an exclusion, or a guest admission. The completion of
/// `set_member_override` and `exclude_member`, which could both write a row
/// that nothing could then take back.
///
/// One operation, three different outcomes, which a caller has to know apart
/// before putting it behind a single button:
///
/// - on a team member with a granted role, they fall back to the role they
///   inherit from the team, so this is a reset, not a removal;
/// - on an excluded team member, their inherited access comes back, so this
///   *grants*;
/// - on a guest, who has no team membership to fall back to, the row was the
///   whole of their access, so this revokes it.
///
/// Absent row is success, not `WorkspaceNotFound`: the caller asked for the
/// override to be gone and it is. Same shape as `remove_favorite`.
pub async fn remove_member_override(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    target_user_id: Uuid,
) -> Result<(), WorkspaceError> {
    workspace_members::Entity::delete_by_id((workspace_id, target_user_id))
        .exec(db)
        .await?;

    Ok(())
}

/// Copied rather than imported, same reasoning as `team_service.rs`'s own
/// copy: the only thing this module needs is whether the row already
/// existed.
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

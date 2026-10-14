// backend/crates/services/src/team_service.rs

use std::collections::HashMap;
use std::str::FromStr;

use chrono::{DateTime, Duration, Utc};
use redis::aio::ConnectionManager;
use sea_orm::sea_query::{Expr, Query};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, EntityTrait,
    ExprTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};
use uuid::Uuid;

use domain::dto::invitation::{INVITATION_DEFAULT_DAYS, TEAM_INVITATION_MAX_DAYS};
use domain::dto::team::{
    CreateInvitationRequest, CreateTeamRequest, TeamInvitationPreview, TeamInvitationResponse,
    TeamMemberResponse, TeamPatch, TeamResponse, TeamRole,
};
use domain::entities::{
    team_invitations, team_members, teams, user_emails, users, workspace_members, workspaces,
};

use crate::audit_service::{self, AuditContext, AuditError};
use crate::config::Config;
use crate::email_service::InvitationKind;
use crate::instance_service::InstanceError;
use crate::invitation_service::{self, CreatedInvitation, InvitationMail, MailOutcome};
use crate::storage_service::{AvatarKind, ObjectStore, ObjectsToPurge};
use crate::{auth_service, slug_service, token_service};

#[derive(Debug, thiserror::Error)]
pub enum TeamError {
    #[error(transparent)]
    Db(#[from] sea_orm::DbErr),

    #[error(transparent)]
    Audit(#[from] AuditError),

    #[error("team not found")]
    TeamNotFound,

    #[error("team member not found")]
    MemberNotFound,

    /// The Captain role changes hands only through `transfer_captaincy`: a
    /// role change, an invitation or a departure can neither name a Captain
    /// nor take the role from the one there is.
    #[error("the captain role changes only by transfer")]
    CaptainMustTransfer,

    /// The Captain is the team's only member. Leaving would leave an empty
    /// team behind, so the way out is deleting the team.
    #[error("the captain is the team's only member, so the team has to be deleted instead")]
    CaptainMustDeleteTeam,

    #[error("the target is already the team's captain")]
    TargetAlreadyCaptain,

    /// A transfer to an account scheduled for deletion would get around the
    /// refusal of a Captain's deletion request: request first, then receive.
    #[error("the target account is scheduled for deletion")]
    TargetPendingDeletion,

    #[error("invitation not found")]
    InvitationNotFound,

    #[error("invitation has expired")]
    InvitationExpired,

    /// The author asked for an expiry further out than
    /// `TEAM_INVITATION_MAX_DAYS`.
    #[error("invitation expiry is too far in the future")]
    InvitationExpiryTooFar,

    #[error("invitation has reached its maximum number of uses")]
    InvitationMaxUsesReached,

    #[error("this invitation is not addressed to the current user")]
    InvitationNotForUser,

    #[error("this invitation was addressed to a different, unverified or unmatched email")]
    InvitationEmailMismatch,

    #[error("user is already invited to this team")]
    AlreadyInvited,

    #[error("user is already a member of this team")]
    AlreadyMember,

    #[error("could not generate a unique slug after the maximum number of attempts")]
    SlugGenerationFailed,

    #[error(transparent)]
    Slug(#[from] slug_service::SlugError),

    #[error("caller's role does not permit this action")]
    InsufficientRole,

    #[error("user has reached the maximum number of teams they may create")]
    TeamQuotaReached,

    #[error("no account matches the invited username")]
    InvitedUserNotFound,
}

/// Ceiling on how many teams one account may create.
///
/// Team rows are unbounded otherwise: any authenticated user could create them
/// in a loop, each carrying a name and a slug, with nothing to stop it. Set
/// high enough that no honest workflow reaches it (organisers running several
/// events at once stay well under) and low enough that the abuse stops being
/// worth the effort.
///
/// Counts teams the user *created*, not teams they belong to: being invited
/// into many teams is not something the invitee controls, so charging it
/// against their quota would let anyone else exhaust it for them.
const MAX_TEAMS_PER_USER: u64 = 20;

// ---- Team CRUD ------------------------------------------------------------

/// Creates a team and its founding Captain, in a single transaction.
pub async fn create_team(
    db: &DatabaseConnection,
    creator_id: Uuid,
    payload: CreateTeamRequest,
) -> Result<TeamResponse, TeamError> {
    let created = teams::Entity::find()
        .filter(teams::Column::CreatedBy.eq(creator_id))
        .count(db)
        .await?;

    if created >= MAX_TEAMS_PER_USER {
        return Err(TeamError::TeamQuotaReached);
    }

    let slug = slug_service::generate_unique_slug(&payload.name, move |candidate| async move {
        Ok(teams::Entity::find()
            .filter(teams::Column::Slug.eq(candidate))
            .one(db)
            .await?
            .is_some())
    })
    .await?;

    let txn = db.begin().await?;

    let team = teams::ActiveModel {
        id: Set(Uuid::now_v7()),
        slug: Set(slug),
        name: Set(payload.name),
        color: Set(payload.color.as_str().to_string()),
        created_by: Set(Some(creator_id)),
        ..Default::default()
    }
    .insert(&txn)
    .await?;

    team_members::ActiveModel {
        team_id: Set(team.id),
        user_id: Set(creator_id),
        role: Set(TeamRole::Captain.as_str().to_string()),
        ..Default::default()
    }
    .insert(&txn)
    .await?;

    txn.commit().await?;

    Ok(TeamResponse::from_model(team, TeamRole::Captain))
}

pub async fn get_team_by_slug(
    db: &DatabaseConnection,
    slug: &str,
) -> Result<teams::Model, TeamError> {
    let team = teams::Entity::find()
        .filter(teams::Column::Slug.eq(slug))
        .one(db)
        .await?
        .ok_or(TeamError::TeamNotFound)?;

    Ok(team)
}

/// Resolves a team by slug and the caller's membership in it as a single
/// unit. A missing team and a team the caller does not belong to both surface
/// as `TeamNotFound`: a non-member must not be able to tell the two apart
/// (see "A 404 hides existence, a 403 admits it" in
/// docs/security-invariants.md, and the Testing instructions in AGENTS.md).
pub async fn get_team_for_member(
    db: &DatabaseConnection,
    slug: &str,
    user_id: Uuid,
) -> Result<(teams::Model, TeamRole), TeamError> {
    let team = get_team_by_slug(db, slug).await?;

    let role = get_user_role_in_team(db, team.id, user_id)
        .await?
        .ok_or(TeamError::TeamNotFound)?;

    Ok((team, role))
}

/// Gates an action behind a minimum role. Only usable once the caller is
/// already known to be a member (see `get_team_for_member`): insufficient
/// privilege on a resource you know exists is a `403`, never a `404`.
pub fn require_role(role: TeamRole, min: TeamRole) -> Result<(), TeamError> {
    if role.level() >= min.level() {
        Ok(())
    } else {
        Err(TeamError::InsufficientRole)
    }
}

/// Whether `actor` may change a member's role from `current` to `new`.
///
/// An Admin may not touch the Captain role either way: letting an Admin do
/// so would let any Admin promote themselves (or an ally) to Captain, a
/// privilege escalation. A Captain passes here and is then refused by
/// `change_member_role`, since the role only moves by transfer.
pub fn can_manage_role(actor: TeamRole, current: TeamRole, new: TeamRole) -> bool {
    if actor == TeamRole::Captain {
        return true;
    }

    actor.level() >= TeamRole::Admin.level()
        && current != TeamRole::Captain
        && new != TeamRole::Captain
}

/// Whether `actor` may remove `target` (a member with role `target_role`)
/// from the team. Removing yourself (leaving) is always allowed here; a
/// Captain's departure is refused by `remove_member`.
///
/// An Admin may not remove a Captain, for the same escalation reason as
/// `can_manage_role`.
pub fn can_remove_member(
    actor: TeamRole,
    actor_id: Uuid,
    target_id: Uuid,
    target_role: TeamRole,
) -> bool {
    if actor_id == target_id {
        return true;
    }

    if actor == TeamRole::Captain {
        return true;
    }

    actor.level() >= TeamRole::Admin.level() && target_role != TeamRole::Captain
}

pub async fn list_teams_for_user(
    db: &DatabaseConnection,
    user_id: Uuid,
) -> Result<Vec<TeamResponse>, TeamError> {
    let teams = team_members::Entity::find()
        .filter(team_members::Column::UserId.eq(user_id))
        .find_also_related(teams::Entity)
        .all(db)
        .await?;

    Ok(teams
        .into_iter()
        .filter_map(|(member, team)| {
            team.map(|team| {
                TeamResponse::from_model(
                    team,
                    TeamRole::from_str(&member.role).unwrap_or(TeamRole::Viewer),
                )
            })
        })
        .collect())
}

pub async fn update_team(
    db: &DatabaseConnection,
    team_id: Uuid,
    patch: TeamPatch,
) -> Result<teams::Model, TeamError> {
    let team = teams::Entity::find_by_id(team_id)
        .one(db)
        .await?
        .ok_or(TeamError::TeamNotFound)?;

    let mut team_am: teams::ActiveModel = team.into();

    if let Some(name) = patch.name {
        team_am.name = Set(name);
    }
    if let Some(color) = patch.color {
        team_am.color = Set(color.as_str().to_string());
    }

    let updated_team = team_am.update(db).await?;

    Ok(updated_team)
}

/// Deletes a team with its workspaces, then their objects once the deletion
/// is committed.
pub async fn delete_team(
    db: &DatabaseConnection,
    store: &ObjectStore,
    team_id: Uuid,
) -> Result<(), TeamError> {
    let txn = db.begin().await?;
    let objects = delete_team_rows(&txn, team_id).await?;
    txn.commit().await?;

    store.purge(objects).await;
    Ok(())
}

/// The database half of `delete_team`, for a caller that deletes more in the
/// same transaction. Returns the objects to purge once the caller has
/// committed: the team's avatar and those of all its workspaces.
///
/// The workspace ids are read before the delete because the cascade removes
/// those rows, and with them the only record of which objects were theirs.
/// The team row is locked first, so a workspace created meanwhile waits on
/// it rather than landing between the read and the cascade.
pub(crate) async fn delete_team_rows<C: ConnectionTrait>(
    conn: &C,
    team_id: Uuid,
) -> Result<ObjectsToPurge, TeamError> {
    teams::Entity::find_by_id(team_id)
        .lock_exclusive()
        .one(conn)
        .await?
        .ok_or(TeamError::TeamNotFound)?;

    let workspace_ids: Vec<Uuid> = workspaces::Entity::find()
        .select_only()
        .column(workspaces::Column::Id)
        .filter(workspaces::Column::TeamId.eq(team_id))
        .into_tuple()
        .all(conn)
        .await?;

    teams::Entity::delete_by_id(team_id).exec(conn).await?;

    let mut objects = ObjectsToPurge::avatar(AvatarKind::Team, team_id);
    for workspace_id in workspace_ids {
        objects.add(ObjectsToPurge::avatar(AvatarKind::Workspace, workspace_id));
    }

    Ok(objects)
}

// ---- Members ----------------------------------------------------------

pub async fn list_team_members(
    db: &DatabaseConnection,
    team_id: Uuid,
) -> Result<Vec<TeamMemberResponse>, TeamError> {
    let members = team_members::Entity::find()
        .filter(team_members::Column::TeamId.eq(team_id))
        .find_also_related(users::Entity)
        .all(db)
        .await?;

    Ok(members
        .into_iter()
        .filter_map(|(member, user)| {
            user.map(|user| TeamMemberResponse {
                user_id: member.user_id,
                username: user.username,
                first_name: user.first_name.unwrap_or_default(),
                last_name: user.last_name.unwrap_or_default(),
                avatar_version: user.avatar_version,
                role: TeamRole::from_str(&member.role).unwrap_or(TeamRole::Viewer),
                joined_at: member.joined_at.into(),
                invited_by: member.invited_by,
            })
        })
        .collect())
}

/// A single member, joined with its user row. Used to build a
/// `TeamMemberResponse` after a targeted write (role change) without
/// re-fetching the whole team roster.
pub async fn get_team_member(
    db: &DatabaseConnection,
    team_id: Uuid,
    user_id: Uuid,
) -> Result<TeamMemberResponse, TeamError> {
    let (member, user) = team_members::Entity::find_by_id((team_id, user_id))
        .find_also_related(users::Entity)
        .one(db)
        .await?
        .ok_or(TeamError::MemberNotFound)?;

    let user = user.ok_or(TeamError::MemberNotFound)?;

    Ok(TeamMemberResponse {
        user_id: member.user_id,
        username: user.username,
        first_name: user.first_name.unwrap_or_default(),
        last_name: user.last_name.unwrap_or_default(),
        avatar_version: user.avatar_version,
        role: TeamRole::from_str(&member.role).unwrap_or(TeamRole::Viewer),
        joined_at: member.joined_at.into(),
        invited_by: member.invited_by,
    })
}

/// Inserts a `team_members` row. Used both by direct addition and by
/// invitation acceptance (token or id), which records the invitation's
/// author as `invited_by`.
///
/// Never as Captain. A team gets its Captain when it is created and changes
/// it by transfer; joining as one would hand the team to a second person
/// without the first ever giving it up, and the database allows one.
pub async fn add_member<C: ConnectionTrait>(
    db: &C,
    team_id: Uuid,
    user_id: Uuid,
    role: TeamRole,
    invited_by: Option<Uuid>,
) -> Result<team_members::Model, TeamError> {
    if role == TeamRole::Captain {
        return Err(TeamError::CaptainMustTransfer);
    }

    let existing_member = team_members::Entity::find_by_id((team_id, user_id))
        .one(db)
        .await?;

    if existing_member.is_some() {
        return Err(TeamError::AlreadyMember);
    }

    let new_member = team_members::ActiveModel {
        team_id: Set(team_id),
        user_id: Set(user_id),
        role: Set(role.as_str().to_string()),
        invited_by: Set(invited_by),
        ..Default::default()
    }
    .insert(db)
    .await?;

    Ok(new_member)
}

/// Changes a member's role, never to or from Captain: that role only changes
/// hands through `transfer_captaincy`.
pub async fn change_member_role(
    db: &DatabaseConnection,
    team_id: Uuid,
    user_id: Uuid,
    new_role: TeamRole,
) -> Result<team_members::Model, TeamError> {
    if new_role == TeamRole::Captain {
        return Err(TeamError::CaptainMustTransfer);
    }

    let txn = db.begin().await?;

    // Locked, and read inside the transaction that writes it: a transfer
    // naming this member Captain meanwhile is either seen here and refused,
    // or waits and lands on the role written here. Read outside, the write
    // would demote the Captain the transfer had just named.
    let member = team_members::Entity::find_by_id((team_id, user_id))
        .lock_exclusive()
        .one(&txn)
        .await?
        .ok_or(TeamError::MemberNotFound)?;

    if member.role == TeamRole::Captain.as_str() {
        return Err(TeamError::CaptainMustTransfer);
    }

    let mut member_am: team_members::ActiveModel = member.into();
    member_am.role = Set(new_role.as_str().to_string());

    let updated_member = member_am.update(&txn).await?;
    txn.commit().await?;

    Ok(updated_member)
}

pub async fn remove_member(
    db: &DatabaseConnection,
    team_id: Uuid,
    user_id: Uuid,
) -> Result<(), TeamError> {
    // One transaction, because the two deletes are one fact: this person is
    // no longer on this team. Committing the first alone leaves an override
    // row with no membership behind it, which
    // `permissions_service::effective_role` still reads and still honours,
    // since it consults the override before it consults team membership. The
    // person would keep their access to the team's workspaces after being
    // removed from the team, and would not appear on any members list while
    // holding it.
    let txn = db.begin().await?;

    // Locked for the same reason as in `change_member_role`: a transfer
    // naming this member Captain meanwhile is either seen here, or waits and
    // then finds no member to name.
    let member = team_members::Entity::find_by_id((team_id, user_id))
        .lock_exclusive()
        .one(&txn)
        .await?
        .ok_or(TeamError::MemberNotFound)?;

    // Nobody removes the Captain, the Captain included: the role goes first,
    // by transfer, or the team goes with them. Leaving would otherwise
    // strand the others without a Captain, or leave an empty team behind.
    if member.role == TeamRole::Captain.as_str() {
        let others = team_members::Entity::find()
            .filter(team_members::Column::TeamId.eq(team_id))
            .filter(team_members::Column::UserId.ne(user_id))
            .count(&txn)
            .await?;

        return Err(if others > 0 {
            TeamError::CaptainMustTransfer
        } else {
            TeamError::CaptainMustDeleteTeam
        });
    }

    let result = team_members::Entity::delete_by_id((team_id, user_id))
        .exec(&txn)
        .await?;

    if result.rows_affected == 0 {
        return Err(TeamError::MemberNotFound);
    }

    let workspace_ids: Vec<Uuid> = workspaces::Entity::find()
        .filter(workspaces::Column::TeamId.eq(team_id))
        .all(&txn)
        .await?
        .into_iter()
        .map(|workspace| workspace.id)
        .collect();

    // Exclusions go too, not just granted roles. An exclusion subtracts from
    // inherited access, so without the inheritance it names nothing: keeping
    // it would make this a silent tombstone, and someone who rejoined later
    // would be the only person not seeing a workspace their colleagues see,
    // with nothing left to say why.
    //
    // Guest rows stay, and that is what bounds this delete to the rows team
    // membership actually created. A guest grant is made independently of the
    // team and is not something leaving it asked to revoke: someone admitted
    // as a guest, who then joined the team and later left, must come out the
    // far side with the access they walked in with. The orphan this cleanup
    // exists to prevent is a row with no team membership behind it, which is
    // `is_guest = false` by definition, so excluding guests costs the
    // guarantee nothing.
    if !workspace_ids.is_empty() {
        workspace_members::Entity::delete_many()
            .filter(workspace_members::Column::UserId.eq(user_id))
            .filter(workspace_members::Column::WorkspaceId.is_in(workspace_ids))
            .filter(workspace_members::Column::IsGuest.eq(false))
            .exec(&txn)
            .await?;
    }

    txn.commit().await?;

    Ok(())
}

pub async fn get_user_role_in_team(
    db: &DatabaseConnection,
    team_id: Uuid,
    user_id: Uuid,
) -> Result<Option<TeamRole>, TeamError> {
    let member = team_members::Entity::find_by_id((team_id, user_id))
        .one(db)
        .await?;

    Ok(member.map(|m| TeamRole::from_str(&m.role).unwrap_or(TeamRole::Viewer)))
}

// ---- Captaincy ------------------------------------------------------------

/// Who hands a team's Captain role over.
pub enum TransferBy<'a> {
    /// The Captain, through the API. They must still be the Captain once the
    /// team is locked: of two transfers racing from the same Captain, the
    /// second finds it is not one any more. Not audited, like every other
    /// team action.
    Captain(Uuid),
    /// An instance administrator, from the CLI, who needs no seat in the
    /// team; a team with no Captain then simply gets one. Audited, as every
    /// other administrator action is.
    InstanceAdmin {
        ctx: &'a AuditContext,
        reason: Option<&'a str>,
    },
}

/// Hands the Captain role to another member: the Captain becomes an Admin
/// and the target becomes Captain, in one transaction with the team row
/// locked.
///
/// Refused for a target who is not a member or is already the Captain, and
/// for a target scheduled for deletion.
pub async fn transfer_captaincy(
    db: &DatabaseConnection,
    team_id: Uuid,
    target_user_id: Uuid,
    by: TransferBy<'_>,
) -> Result<(), TeamError> {
    let txn = db.begin().await?;

    teams::Entity::find_by_id(team_id)
        .lock_exclusive()
        .one(&txn)
        .await?
        .ok_or(TeamError::TeamNotFound)?;

    let captain = current_captain(&txn, team_id).await?;

    if let TransferBy::Captain(acting_captain) = by
        && captain != Some(acting_captain)
    {
        return Err(TeamError::InsufficientRole);
    }

    // Locked so the member cannot leave or change role between this read
    // and the promotion below (see `remove_member`).
    let target = team_members::Entity::find_by_id((team_id, target_user_id))
        .lock_exclusive()
        .one(&txn)
        .await?
        .ok_or(TeamError::MemberNotFound)?;

    if target.role == TeamRole::Captain.as_str() {
        return Err(TeamError::TargetAlreadyCaptain);
    }

    // Shared lock on the account: a deletion request for it, which locks the
    // same row before checking whether it is a Captain, either lands first
    // and is seen here, or waits and then sees the role this commits.
    let target_account = users::Entity::find_by_id(target_user_id)
        .lock_shared()
        .one(&txn)
        .await?
        .ok_or(TeamError::MemberNotFound)?;

    if target_account.purge_scheduled_at.is_some() {
        return Err(TeamError::TargetPendingDeletion);
    }

    // The Captain steps down before the target steps up: the index allows
    // one Captain per team after every statement, not only at the commit.
    if let Some(captain) = captain {
        set_role(&txn, team_id, captain, TeamRole::Admin).await?;
    }
    set_role(&txn, team_id, target_user_id, TeamRole::Captain).await?;

    if let TransferBy::InstanceAdmin { ctx, reason } = by {
        audit_service::record(
            &txn,
            domain::types::AuditAction::TeamCaptaincyTransferred,
            ctx,
            audit_service::AuditTarget {
                user_id: Some(target_user_id),
                team_id: Some(team_id),
                workspace_id: None,
            },
            reason,
            // Identifiers only, all three in the entry's own data: the user
            // target is set to NULL if that account is deleted later.
            serde_json::json!({
                "team_id": team_id,
                "previous_captain": captain,
                "new_captain": target_user_id,
            }),
        )
        .await?;
    }

    txn.commit().await?;

    Ok(())
}

/// What became of a team whose Captain's account is being deleted.
pub(crate) enum Succession {
    /// The Captain was the only member, and the team went with them: these
    /// are its objects to purge once the deletion is committed.
    TeamDeleted(ObjectsToPurge),
    /// This member is the Captain now.
    Promoted(Uuid),
    /// The role moved by transfer after the team was listed: there is no
    /// succession to arrange, and the membership goes with the account.
    NoLongerCaptain,
}

/// The team side of deleting an account that is still a team's Captain, run
/// in the deletion's own transaction.
///
/// A team with no other member is deleted with its workspaces. Otherwise its
/// longest-standing Admin becomes Captain, or its longest-standing member
/// when it has no Admin, ties broken by id: among the active members first,
/// neither suspended nor scheduled for deletion, and among all of them when
/// none is active, so the team keeps a Captain either way. The departing
/// Captain's own row goes first, since the index allows one Captain per
/// team at every statement; the account's deletion would have removed it
/// anyway.
pub(crate) async fn succeed_captain<C: ConnectionTrait>(
    conn: &C,
    team_id: Uuid,
    captain_id: Uuid,
) -> Result<Succession, TeamError> {
    // Locked so a transfer, or a member joining (its foreign key check takes
    // this row too), waits for the deletion to commit. A member leaving does
    // not wait: if it is the one chosen below, the promotion finds no row
    // and fails the whole deletion rather than leave the team without a
    // Captain, and the purge tries again on its next pass.
    teams::Entity::find_by_id(team_id)
        .lock_exclusive()
        .one(conn)
        .await?
        .ok_or(TeamError::TeamNotFound)?;

    if current_captain(conn, team_id).await? != Some(captain_id) {
        return Ok(Succession::NoLongerCaptain);
    }

    let active = successor_among(conn, team_id, captain_id, true).await?;
    let successor = match active {
        Some(active) => Some(active),
        None => successor_among(conn, team_id, captain_id, false).await?,
    };

    let Some(successor) = successor else {
        return Ok(Succession::TeamDeleted(
            delete_team_rows(conn, team_id).await?,
        ));
    };

    team_members::Entity::delete_by_id((team_id, captain_id))
        .exec(conn)
        .await?;
    set_role(conn, team_id, successor, TeamRole::Captain).await?;

    Ok(Succession::Promoted(successor))
}

/// The team's Captain, if it has one.
async fn current_captain<C: ConnectionTrait>(
    conn: &C,
    team_id: Uuid,
) -> Result<Option<Uuid>, TeamError> {
    Ok(team_members::Entity::find()
        .select_only()
        .column(team_members::Column::UserId)
        .filter(team_members::Column::TeamId.eq(team_id))
        .filter(team_members::Column::Role.eq(TeamRole::Captain.as_str()))
        .into_tuple()
        .one(conn)
        .await?)
}

/// The longest-standing Admin other than `except`, or failing one the
/// longest-standing member, among the active members only when
/// `active_only`.
async fn successor_among<C: ConnectionTrait>(
    conn: &C,
    team_id: Uuid,
    except: Uuid,
    active_only: bool,
) -> Result<Option<Uuid>, TeamError> {
    let admin = longest_standing(conn, team_id, except, Some(TeamRole::Admin), active_only).await?;
    match admin {
        Some(admin) => Ok(Some(admin)),
        None => longest_standing(conn, team_id, except, None, active_only).await,
    }
}

/// The member who joined first, other than `except`, with `role` when one is
/// given. Ties on the date go to the lower id, so the answer never depends on
/// the order Postgres happens to return rows in.
///
/// With `active_only`, an account scheduled for deletion or suspended is
/// left out. Suspended reads as `auth_service::suspension_error` reads it: a
/// suspension whose end date has passed no longer counts.
async fn longest_standing<C: ConnectionTrait>(
    conn: &C,
    team_id: Uuid,
    except: Uuid,
    role: Option<TeamRole>,
    active_only: bool,
) -> Result<Option<Uuid>, TeamError> {
    let mut query = team_members::Entity::find()
        .select_only()
        .column(team_members::Column::UserId)
        .filter(team_members::Column::TeamId.eq(team_id))
        .filter(team_members::Column::UserId.ne(except));

    if let Some(role) = role {
        query = query.filter(team_members::Column::Role.eq(role.as_str()));
    }

    if active_only {
        query = query
            .inner_join(users::Entity)
            .filter(users::Column::PurgeScheduledAt.is_null())
            .filter(
                Condition::any()
                    .add(users::Column::SuspendedAt.is_null())
                    .add(users::Column::SuspendedUntil.lte(chrono::Utc::now())),
            );
    }

    Ok(query
        .order_by_asc(team_members::Column::JoinedAt)
        .order_by_asc(team_members::Column::UserId)
        .into_tuple()
        .one(conn)
        .await?)
}

async fn set_role<C: ConnectionTrait>(
    conn: &C,
    team_id: Uuid,
    user_id: Uuid,
    role: TeamRole,
) -> Result<(), TeamError> {
    let result = team_members::Entity::update_many()
        .col_expr(team_members::Column::Role, Expr::value(role.as_str()))
        .filter(team_members::Column::TeamId.eq(team_id))
        .filter(team_members::Column::UserId.eq(user_id))
        .exec(conn)
        .await?;

    if result.rows_affected == 0 {
        return Err(TeamError::MemberNotFound);
    }

    Ok(())
}

/// The teams whose Captain is `user_id` and that have at least one other
/// member: the teams a deletion request for that account would leave
/// without a Captain.
pub(crate) async fn captained_teams_with_others<C: ConnectionTrait>(
    conn: &C,
    user_id: Uuid,
) -> Result<Vec<teams::Model>, TeamError> {
    let captained = teams::Entity::find()
        .inner_join(team_members::Entity)
        .filter(team_members::Column::UserId.eq(user_id))
        .filter(team_members::Column::Role.eq(TeamRole::Captain.as_str()))
        .order_by_asc(teams::Column::Slug)
        .all(conn)
        .await?;

    let mut with_others = Vec::new();
    for team in captained {
        let others = team_members::Entity::find()
            .filter(team_members::Column::TeamId.eq(team.id))
            .filter(team_members::Column::UserId.ne(user_id))
            .count(conn)
            .await?;

        if others > 0 {
            with_others.push(team);
        }
    }

    Ok(with_others)
}

/// The teams whose Captain is `user_id`, in a fixed order.
pub(crate) async fn captained_teams<C: ConnectionTrait>(
    conn: &C,
    user_id: Uuid,
) -> Result<Vec<Uuid>, TeamError> {
    Ok(teams::Entity::find()
        .select_only()
        .column(teams::Column::Id)
        .inner_join(team_members::Entity)
        .filter(team_members::Column::UserId.eq(user_id))
        .filter(team_members::Column::Role.eq(TeamRole::Captain.as_str()))
        .order_by_asc(teams::Column::Id)
        .into_tuple()
        .all(conn)
        .await?)
}

// ---- Invitations --------------------------------------------------------

/// Matches invitations that are still usable.
///
/// Written once because every read of this table needs it: an expired row
/// must neither be offered to the invitee, nor block a replacement being
/// issued, nor be acceptable. Leaving it out of the dedupe checks meant an
/// expired invitation locked a person out of ever being re-invited to that
/// team, since the unique indexes still counted the dead row. Every row has
/// an expiry since `m20261014_120000_team_invitations_expire`.
fn not_expired() -> Expr {
    team_invitations::Column::ExpiresAt.gt(chrono::Utc::now())
}

/// When a new team invitation expires: the date its author asked for, or
/// seven days from `now` when they did not say. Never more than thirty days
/// out, so no link stays redeemable indefinitely.
fn invitation_expiry(
    requested: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Result<DateTime<Utc>, TeamError> {
    let expires_at = requested.unwrap_or(now + Duration::days(INVITATION_DEFAULT_DAYS));
    if expires_at > now + Duration::days(TEAM_INVITATION_MAX_DAYS) {
        return Err(TeamError::InvitationExpiryTooFar);
    }
    Ok(expires_at)
}

/// Whether a member of the team has `email` verified: inviting that address
/// would invite someone already there.
async fn member_holds_address<C: ConnectionTrait>(
    conn: &C,
    team_id: Uuid,
    email: &str,
) -> Result<bool, TeamError> {
    let members = Query::select()
        .column((team_members::Entity, team_members::Column::UserId))
        .from(team_members::Entity)
        .and_where(Expr::col((team_members::Entity, team_members::Column::TeamId)).eq(team_id))
        .to_owned();

    Ok(user_emails::Entity::find()
        .filter(user_emails::Column::Email.eq(email))
        .filter(user_emails::Column::VerifiedAt.is_not_null())
        .filter(user_emails::Column::UserId.in_subquery(members))
        .one(conn)
        .await?
        .is_some())
}

/// Names the invitee of a new invitation and refuses the ones that cannot be
/// issued. Returns the account named by username, if any, and the address,
/// normalized, if any; both `None` is a generic link.
///
/// A username names an account, and one that matches nobody is refused
/// rather than quietly downgraded to a generic link: the caller asked for a
/// specific person, and silently minting a token anyone could redeem is not
/// the same invitation.
///
/// An address names nobody. It is not resolved to an account here, so the
/// answer is the same whether one holds it or not; the invitation reaches
/// whoever has it verified, at the time they read it (see
/// `invitation_service::addressed_to`). The DTO guarantees the two are never
/// both set (see `invitee_is_named_once`).
///
/// Inviting someone already on the team is a mistake to reject up front
/// rather than let them discover it only at acceptance (see `add_member`,
/// still kept as the last line of defense for the race window between these
/// checks and the insert). By address, that is a member who has it verified:
/// telling the author so reveals nothing they cannot read off the roster.
async fn resolve_invitee(
    db: &DatabaseConnection,
    team_id: Uuid,
    payload: &CreateInvitationRequest,
) -> Result<(Option<Uuid>, Option<String>), TeamError> {
    let invited_email = payload
        .invited_email
        .as_deref()
        .map(auth_service::normalize_email);

    let invited_user_id = match &payload.username {
        Some(username) => Some(
            auth_service::find_user_by_username(db, username)
                .await?
                .ok_or(TeamError::InvitedUserNotFound)?,
        ),
        None => None,
    };

    match (invited_user_id, &invited_email) {
        (Some(invited_user_id), _) => {
            if team_members::Entity::find_by_id((team_id, invited_user_id))
                .one(db)
                .await?
                .is_some()
            {
                return Err(TeamError::AlreadyMember);
            }

            let already_pending = team_invitations::Entity::find()
                .filter(team_invitations::Column::TeamId.eq(team_id))
                .filter(team_invitations::Column::InvitedUserId.eq(invited_user_id))
                .filter(not_expired())
                .one(db)
                .await?
                .is_some();
            if already_pending {
                return Err(TeamError::AlreadyInvited);
            }
        }
        (None, Some(email)) => {
            if member_holds_address(db, team_id, email).await? {
                return Err(TeamError::AlreadyMember);
            }

            // Same constraint as idx_team_invitations_unique_per_email.
            let already_pending = team_invitations::Entity::find()
                .filter(team_invitations::Column::TeamId.eq(team_id))
                .filter(team_invitations::Column::InvitedUserId.is_null())
                .filter(team_invitations::Column::InvitedEmail.eq(email.clone()))
                .filter(not_expired())
                .one(db)
                .await?
                .is_some();
            if already_pending {
                return Err(TeamError::AlreadyInvited);
            }
        }
        (None, None) => {}
    }

    Ok((invited_user_id, invited_email))
}

pub async fn create_invitation(
    db: &DatabaseConnection,
    redis: &mut ConnectionManager,
    config: &Config,
    team_id: Uuid,
    creator_id: Uuid,
    payload: CreateInvitationRequest,
) -> Result<CreatedInvitation<TeamInvitationResponse>, TeamError> {
    // Joining never makes a Captain (see `add_member`). Refused here, where
    // the Captain asking can be told to transfer, rather than at acceptance,
    // where the invitee would be the one to find out.
    if payload.default_role == TeamRole::Captain {
        return Err(TeamError::CaptainMustTransfer);
    }

    let expires_at = invitation_expiry(payload.expires_at, Utc::now())?;
    let (invited_user_id, invited_email) = resolve_invitee(db, team_id, &payload).await?;

    let team = teams::Entity::find_by_id(team_id)
        .one(db)
        .await?
        .ok_or(TeamError::TeamNotFound)?;

    let token = token_service::generate_opaque_token();

    let max_uses = if invited_user_id.is_some() || invited_email.is_some() {
        Some(1)
    } else {
        payload.max_uses
    };

    let invitation = team_invitations::ActiveModel {
        id: Set(Uuid::now_v7()),
        team_id: Set(team_id),
        created_by: Set(Some(creator_id)),
        default_role: Set(payload.default_role.as_str().to_string()),
        invited_user_id: Set(invited_user_id),
        invited_email: Set(invited_email.clone()),
        expires_at: Set(expires_at.into()),
        max_uses: Set(max_uses),
        token: Set(token.hash),
        ..Default::default()
    }
    .insert(db)
    .await?;

    // Where to write is the account's business when it is named, not the
    // caller's: an address taken from the request would let an Admin send
    // the invitation to any mailbox they liked, over an invitation that names
    // someone else entirely. The mail is the same either way, and never
    // undoes the invitation (see `invitation_service::send_mail`).
    let recipient = match (invited_user_id, &invited_email) {
        (Some(user_id), _) => Some(invitation_service::account_recipient(db, user_id).await),
        (None, Some(email)) => Some(
            invitation_service::address_recipient(db, email)
                .await
                .map(Some)
                .map_err(|InstanceError::Db(err)| err),
        ),
        (None, None) => None,
    };
    let mail = match recipient {
        None => None,
        Some(Ok(None)) => Some(MailOutcome::NoAddress),
        Some(Ok(Some(recipient))) => Some(
            invitation_service::send_mail(
                redis,
                config,
                &recipient,
                InvitationMail {
                    kind: InvitationKind::Team,
                    name: &team.name,
                    token: &token.secret,
                    expires_at,
                },
            )
            .await,
        ),
        Some(Err(error)) => {
            tracing::error!(?error, invitation_id = %invitation.id, "could not address the team invitation email");
            Some(MailOutcome::Failed)
        }
    };

    let authors = invitation_service::usernames(db, [creator_id]).await?;

    Ok(CreatedInvitation {
        invitation: TeamInvitationResponse {
            id: invitation.id,
            token: Some(token.secret),
            team_id,
            team_name: team.name,
            default_role: payload.default_role,
            invited_user_id,
            invited_email,
            invited_by: Some(creator_id),
            invited_by_username: authors.get(&creator_id).cloned(),
            expires_at,
            max_uses,
            uses: 0,
            created_at: invitation.created_at.into(),
        },
        mail,
    })
}

/// The listing shape of a row: never its token, which only the creation
/// response carries.
fn invitation_response(
    invitation: team_invitations::Model,
    team_name: String,
    usernames: &HashMap<Uuid, String>,
) -> TeamInvitationResponse {
    TeamInvitationResponse {
        id: invitation.id,
        token: None,
        team_id: invitation.team_id,
        team_name,
        default_role: TeamRole::from_str(&invitation.default_role).unwrap_or(TeamRole::Viewer),
        invited_user_id: invitation.invited_user_id,
        invited_email: invitation.invited_email,
        invited_by: invitation.created_by,
        invited_by_username: invitation
            .created_by
            .and_then(|id| usernames.get(&id).cloned()),
        expires_at: invitation.expires_at.into(),
        max_uses: invitation.max_uses,
        uses: invitation.uses,
        created_at: invitation.created_at.into(),
    }
}

/// The team's active invitations that name no account: generic links and
/// invitations by address, not expired, with their author.
pub async fn list_active_invitations(
    db: &DatabaseConnection,
    team_id: Uuid,
) -> Result<Vec<TeamInvitationResponse>, TeamError> {
    let team = teams::Entity::find_by_id(team_id)
        .one(db)
        .await?
        .ok_or(TeamError::TeamNotFound)?;

    let invitations = team_invitations::Entity::find()
        .filter(team_invitations::Column::TeamId.eq(team_id))
        .filter(team_invitations::Column::InvitedUserId.is_null())
        .filter(not_expired())
        .order_by_asc(team_invitations::Column::CreatedAt)
        .all(db)
        .await?;

    let usernames =
        invitation_service::usernames(db, invitations.iter().filter_map(|row| row.created_by))
            .await?;

    Ok(invitations
        .into_iter()
        .map(|invitation| invitation_response(invitation, team.name.clone(), &usernames))
        .collect())
}

/// Pending targeted invitations for this user, across all teams: those that
/// name the account, and those sent to an address it has verified, matched
/// now rather than when they were sent.
pub async fn list_pending_invitations_for_user(
    db: &DatabaseConnection,
    user_id: Uuid,
) -> Result<Vec<TeamInvitationResponse>, TeamError> {
    let invitations = team_invitations::Entity::find()
        .filter(invitation_service::addressed_to(
            team_invitations::Column::InvitedUserId,
            team_invitations::Column::InvitedEmail,
            user_id,
        ))
        .filter(not_expired())
        .order_by_asc(team_invitations::Column::CreatedAt)
        .find_also_related(teams::Entity)
        .all(db)
        .await?;

    let usernames =
        invitation_service::usernames(db, invitations.iter().filter_map(|(row, _)| row.created_by))
            .await?;

    Ok(invitations
        .into_iter()
        .filter_map(|(invitation, team)| {
            team.map(|team| invitation_response(invitation, team.name, &usernames))
        })
        .collect())
}

/// Scoped by `team_id` so a member of team A cannot revoke an invitation
/// belonging to team B by guessing its id.
pub async fn revoke_invitation(
    db: &DatabaseConnection,
    team_id: Uuid,
    invitation_id: Uuid,
) -> Result<(), TeamError> {
    let result = team_invitations::Entity::delete_many()
        .filter(team_invitations::Column::Id.eq(invitation_id))
        .filter(team_invitations::Column::TeamId.eq(team_id))
        .exec(db)
        .await?;

    if result.rows_affected == 0 {
        return Err(TeamError::InvitationNotFound);
    }

    Ok(())
}

/// Whether `user_id` may answer this invitation. A generic link is for
/// whoever holds it; an addressed one, for the account it names or for an
/// account with its address verified. The account id wins when both are
/// set: it is the stronger statement of the two, and consulting only the
/// address is what once let a third party redeem a link issued in someone
/// else's name.
async fn check_invitee<C: ConnectionTrait>(
    conn: &C,
    invitation: &team_invitations::Model,
    user_id: Uuid,
) -> Result<(), TeamError> {
    match (&invitation.invited_user_id, &invitation.invited_email) {
        (Some(invited_user_id), _) if *invited_user_id != user_id => {
            Err(TeamError::InvitationNotForUser)
        }
        (Some(_), _) | (None, None) => Ok(()),
        (None, Some(invited_email)) => {
            let matches = user_emails::Entity::find()
                .filter(user_emails::Column::UserId.eq(user_id))
                .filter(user_emails::Column::Email.eq(invited_email.clone()))
                .filter(user_emails::Column::VerifiedAt.is_not_null())
                .one(conn)
                .await?
                .is_some();

            if matches {
                Ok(())
            } else {
                Err(TeamError::InvitationEmailMismatch)
            }
        }
    }
}

/// Uses the invitation up, inside the acceptance's transaction. An addressed
/// one is deleted: whichever of two concurrent acceptances deletes it first
/// joins, the other finds nothing. A generic link reserves one of its uses,
/// under the same guarantee from the conditional update.
async fn consume<C: ConnectionTrait>(
    conn: &C,
    invitation: &team_invitations::Model,
) -> Result<(), TeamError> {
    if invitation.invited_user_id.is_some() || invitation.invited_email.is_some() {
        let deleted = team_invitations::Entity::delete_by_id(invitation.id)
            .exec(conn)
            .await?;
        if deleted.rows_affected == 0 {
            return Err(TeamError::InvitationNotFound);
        }
        return Ok(());
    }

    let reserved = team_invitations::Entity::update_many()
        .col_expr(
            team_invitations::Column::Uses,
            Expr::col(team_invitations::Column::Uses).add(1),
        )
        .filter(team_invitations::Column::Id.eq(invitation.id))
        .filter(
            team_invitations::Column::MaxUses
                .is_null()
                .or(Expr::col(team_invitations::Column::Uses)
                    .lt(Expr::col(team_invitations::Column::MaxUses))),
        )
        .exec(conn)
        .await?;

    if reserved.rows_affected == 0 {
        return Err(TeamError::InvitationMaxUsesReached);
    }

    Ok(())
}

/// Joins `user_id` to the invitation's team, recording its author, in one
/// transaction with the invitation being used up.
async fn join_by(
    db: &DatabaseConnection,
    invitation: &team_invitations::Model,
    user_id: Uuid,
) -> Result<TeamResponse, TeamError> {
    let txn = db.begin().await?;

    consume(&txn, invitation).await?;

    let role = TeamRole::from_str(&invitation.default_role).unwrap_or(TeamRole::Viewer);
    add_member(
        &txn,
        invitation.team_id,
        user_id,
        role,
        invitation.created_by,
    )
    .await?;

    let team = teams::Entity::find_by_id(invitation.team_id)
        .one(&txn)
        .await?
        .ok_or(TeamError::TeamNotFound)?;

    txn.commit().await?;

    Ok(TeamResponse::from_model(team, role))
}

/// Acceptance by token (`POST /teams/join`): a generic link, or the link of
/// an addressed invitation followed by the account it is for.
pub async fn accept_invitation_by_token(
    db: &DatabaseConnection,
    token: &str,
    user_id: Uuid,
) -> Result<TeamResponse, TeamError> {
    let token_hash = token_service::hash_opaque_token(token);

    let invitation = team_invitations::Entity::find()
        .filter(team_invitations::Column::Token.eq(token_hash))
        .one(db)
        .await?
        .ok_or(TeamError::InvitationNotFound)?;

    if invitation.expires_at < chrono::Utc::now() {
        return Err(TeamError::InvitationExpired);
    }

    // An addressed link must not be usable by whoever else it ends up with
    // (forwarded, leaked, shared inbox).
    check_invitee(db, &invitation, user_id).await?;

    join_by(db, &invitation, user_id).await
}

/// Acceptance from the invitee's pending list.
pub async fn accept_invitation_by_id(
    db: &DatabaseConnection,
    invitation_id: Uuid,
    user_id: Uuid,
) -> Result<TeamResponse, TeamError> {
    let invitation = addressed_invitation(db, invitation_id, user_id).await?;

    if invitation.expires_at < chrono::Utc::now() {
        return Err(TeamError::InvitationExpired);
    }

    join_by(db, &invitation, user_id).await
}

pub async fn reject_invitation_by_id(
    db: &DatabaseConnection,
    invitation_id: Uuid,
    user_id: Uuid,
) -> Result<(), TeamError> {
    let invitation = addressed_invitation(db, invitation_id, user_id).await?;

    team_invitations::Entity::delete_by_id(invitation.id)
        .exec(db)
        .await?;

    Ok(())
}

/// An invitation reached by id is answerable only by its invitee; a generic
/// link, which names nobody, is not reached this way at all.
async fn addressed_invitation(
    db: &DatabaseConnection,
    invitation_id: Uuid,
    user_id: Uuid,
) -> Result<team_invitations::Model, TeamError> {
    let invitation = team_invitations::Entity::find_by_id(invitation_id)
        .one(db)
        .await?
        .ok_or(TeamError::InvitationNotFound)?;

    if invitation.invited_user_id.is_none() && invitation.invited_email.is_none() {
        return Err(TeamError::InvitationNotForUser);
    }
    check_invitee(db, &invitation, user_id).await?;

    Ok(invitation)
}

/// What the invitee sees behind a link before answering: the team's name,
/// the author's username, the role and the expiry.
///
/// Only to the account the invitation is for (anyone, for a generic link).
/// An unknown token, an expired or used-up invitation and someone else's all
/// get the same `InvitationNotFound`, so the preview tells nobody else
/// anything, not even that the token exists.
pub async fn preview_invitation(
    db: &DatabaseConnection,
    token: &str,
    user_id: Uuid,
) -> Result<TeamInvitationPreview, TeamError> {
    let token_hash = token_service::hash_opaque_token(token);

    let (invitation, team) = team_invitations::Entity::find()
        .filter(team_invitations::Column::Token.eq(token_hash))
        .filter(not_expired())
        .filter(
            team_invitations::Column::MaxUses
                .is_null()
                .or(Expr::col(team_invitations::Column::Uses)
                    .lt(Expr::col(team_invitations::Column::MaxUses))),
        )
        .find_also_related(teams::Entity)
        .one(db)
        .await?
        .ok_or(TeamError::InvitationNotFound)?;
    let team = team.ok_or(TeamError::InvitationNotFound)?;

    check_invitee(db, &invitation, user_id)
        .await
        .map_err(|error| match error {
            TeamError::InvitationNotForUser | TeamError::InvitationEmailMismatch => {
                TeamError::InvitationNotFound
            }
            other => other,
        })?;

    let authors = invitation_service::usernames(db, invitation.created_by).await?;

    Ok(TeamInvitationPreview {
        team_name: team.name,
        invited_by_username: invitation
            .created_by
            .and_then(|id| authors.get(&id).cloned()),
        role: TeamRole::from_str(&invitation.default_role).unwrap_or(TeamRole::Viewer),
        expires_at: invitation.expires_at.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::dto::team::CreateInvitationRequest;
    use domain::types::AccentColor;
    use sea_orm::{DatabaseBackend, MockDatabase, Value};
    use std::collections::BTreeMap;

    /// Shapes a row the way `PaginatorTrait::count` reads it back: it issues
    /// `COUNT(*) AS num_items` and pulls that one column out as an i64.
    fn count_row(n: i64) -> BTreeMap<String, Value> {
        BTreeMap::from([("num_items".to_owned(), Value::BigInt(Some(n)))])
    }

    fn verified_email_row(user_id: Uuid) -> user_emails::Model {
        user_emails::Model {
            id: Uuid::now_v7(),
            user_id,
            email: "existing@example.test".to_owned(),
            is_primary: true,
            created_at: chrono::Utc::now().into(),
            updated_at: chrono::Utc::now().into(),
            verified_at: Some(chrono::Utc::now().into()),
        }
    }

    /// Only the identity fields matter here: `create_invitation` looks this
    /// row up to confirm the account exists and reads nothing else from it.
    fn user_row(user_id: Uuid) -> users::Model {
        let now = chrono::Utc::now().into();

        users::Model {
            id: user_id,
            username: "invitee".to_owned(),
            password_hash: "not-a-real-hash".to_owned(),
            first_name: None,
            last_name: None,
            bio: None,
            avatar_version: None,
            theme: "dark".to_owned(),
            locale: "en".to_owned(),
            is_instance_admin: false,
            suspended_at: None,
            suspended_until: None,
            deletion_requested_by: None,
            purge_scheduled_at: None,
            suspended_reason: None,
            suspended_by: None,
            legal_terms_version: None,
            legal_privacy_version: None,
            created_at: now,
            updated_at: now,
            last_activity_at: None,
        }
    }

    fn team_member_row(team_id: Uuid, user_id: Uuid) -> team_members::Model {
        team_members::Model {
            team_id,
            user_id,
            role: TeamRole::Member.as_str().to_owned(),
            joined_at: chrono::Utc::now().into(),
            invited_by: None,
        }
    }

    fn pending_invitation_row() -> team_invitations::Model {
        team_invitations::Model {
            id: Uuid::now_v7(),
            team_id: Uuid::now_v7(),
            token: "hash".to_owned(),
            default_role: TeamRole::Member.as_str().to_owned(),
            invited_user_id: None,
            invited_email: None,
            created_by: None,
            created_at: chrono::Utc::now().into(),
            expires_at: (chrono::Utc::now() + Duration::days(1)).into(),
            max_uses: Some(1),
            uses: 0,
        }
    }

    fn base_invitation_payload() -> CreateInvitationRequest {
        CreateInvitationRequest {
            default_role: TeamRole::Member,
            expires_at: None,
            max_uses: None,
            username: None,
            invited_email: None,
        }
    }

    /// The expiry an author leaves out is seven days; thirty is the most
    /// they may ask for, to the second.
    #[test]
    fn invitation_expiry_defaults_to_seven_days_and_caps_at_thirty() {
        let now = Utc::now();

        assert_eq!(
            invitation_expiry(None, now).ok(),
            Some(now + Duration::days(7))
        );
        assert_eq!(
            invitation_expiry(Some(now + Duration::days(30)), now).ok(),
            Some(now + Duration::days(30))
        );
        assert!(matches!(
            invitation_expiry(Some(now + Duration::days(30) + Duration::seconds(1)), now),
            Err(TeamError::InvitationExpiryTooFar)
        ));
    }

    #[tokio::test]
    async fn create_invitation_rejects_a_second_pending_invite_to_the_same_email() {
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            // 1: no member of the team has the address verified.
            .append_query_results([Vec::<user_emails::Model>::new()])
            // 2: a pending invitation for that address already exists.
            .append_query_results([vec![pending_invitation_row()]])
            .into_connection();

        let payload = CreateInvitationRequest {
            invited_email: Some("new@example.test".to_owned()),
            ..base_invitation_payload()
        };

        let result = resolve_invitee(&db, Uuid::now_v7(), &payload).await;

        assert!(matches!(result, Err(TeamError::AlreadyInvited)));
    }

    #[tokio::test]
    async fn create_invitation_rejects_inviting_by_username_someone_already_a_member() {
        let team_id = Uuid::now_v7();
        let invited_user_id = Uuid::now_v7();

        let db = MockDatabase::new(DatabaseBackend::Postgres)
            // 1: the username resolves to an account.
            .append_query_results([vec![user_row(invited_user_id)]])
            // 2: they are already a member of the team.
            .append_query_results([vec![team_member_row(team_id, invited_user_id)]])
            .into_connection();

        let payload = CreateInvitationRequest {
            username: Some("invitee".to_owned()),
            ..base_invitation_payload()
        };

        let result = resolve_invitee(&db, team_id, &payload).await;

        assert!(matches!(result, Err(TeamError::AlreadyMember)));
    }

    /// Negative: an address a member of the team has verified is refused as
    /// `already_member`, which is all the lookup of its holder is for.
    #[tokio::test]
    async fn create_invitation_rejects_inviting_by_email_someone_already_a_member() {
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            // 1: a member of the team has the address verified.
            .append_query_results([vec![verified_email_row(Uuid::now_v7())]])
            .into_connection();

        let payload = CreateInvitationRequest {
            invited_email: Some("existing@example.test".to_owned()),
            ..base_invitation_payload()
        };

        let result = resolve_invitee(&db, Uuid::now_v7(), &payload).await;

        assert!(matches!(result, Err(TeamError::AlreadyMember)));
    }

    /// Negative: a username that matches no account is refused outright.
    ///
    /// The alternative would be to fall through and mint a generic link, which
    /// is a different invitation than the one asked for: anyone holding the
    /// token could redeem it, instead of the one person named.
    #[tokio::test]
    async fn create_invitation_rejects_an_unknown_username() {
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results([Vec::<users::Model>::new()])
            .into_connection();

        let payload = CreateInvitationRequest {
            username: Some("nobody".to_owned()),
            ..base_invitation_payload()
        };

        let result = resolve_invitee(&db, Uuid::now_v7(), &payload).await;

        assert!(matches!(result, Err(TeamError::InvitedUserNotFound)));
    }

    #[tokio::test]
    async fn accept_invitation_by_token_rejects_when_verified_email_does_not_match() {
        let user_id = Uuid::now_v7();
        let invitation = team_invitations::Model {
            invited_email: Some("target@example.test".to_owned()),
            ..pending_invitation_row()
        };

        let db = MockDatabase::new(DatabaseBackend::Postgres)
            // 1: the invitation itself, found by token.
            .append_query_results([vec![invitation]])
            // 2: the accepting user has no verified row matching that email.
            .append_query_results([Vec::<user_emails::Model>::new()])
            .into_connection();

        let result = accept_invitation_by_token(&db, "some-token", user_id).await;

        assert!(matches!(result, Err(TeamError::InvitationEmailMismatch)));
    }

    /// Negative: an invitation pinned to an account is checked against that
    /// account, not against whatever address the row also happens to carry.
    ///
    /// Reading the address instead is what let a third party redeem a link
    /// issued in someone else's name: the id said one person, the address said
    /// another, and this path consulted the wrong one.
    #[tokio::test]
    async fn accept_invitation_by_token_rejects_a_caller_who_is_not_the_named_account() {
        let named_account = Uuid::now_v7();
        let someone_else = Uuid::now_v7();

        let invitation = team_invitations::Model {
            invited_user_id: Some(named_account),
            invited_email: Some("target@example.test".to_owned()),
            ..pending_invitation_row()
        };

        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results([vec![invitation]])
            .into_connection();

        let result = accept_invitation_by_token(&db, "some-token", someone_else).await;

        assert!(matches!(result, Err(TeamError::InvitationNotForUser)));
    }

    /// Positive: the named account may redeem its own link, and is not asked
    /// to also prove it owns the address the row carries.
    #[tokio::test]
    async fn accept_invitation_by_token_admits_the_named_account() {
        let named_account = Uuid::now_v7();

        let invitation = team_invitations::Model {
            invited_user_id: Some(named_account),
            invited_email: Some("target@example.test".to_owned()),
            ..pending_invitation_row()
        };

        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results([vec![invitation]])
            .into_connection();

        let result = accept_invitation_by_token(&db, "some-token", named_account).await;

        // The mock runs out of canned answers once the identity check passes,
        // which is all this test is pinning: the caller is not turned away.
        assert!(
            !matches!(
                result,
                Err(TeamError::InvitationNotForUser) | Err(TeamError::InvitationEmailMismatch)
            ),
            "the named account must clear the identity check"
        );
    }

    fn create_payload() -> CreateTeamRequest {
        CreateTeamRequest {
            name: "Team".to_owned(),
            color: AccentColor::Ember,
        }
    }

    /// Negative: a user already at the ceiling is refused, before any row is
    /// written and before a slug is even generated.
    #[tokio::test]
    async fn create_team_refuses_a_user_at_the_quota() {
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results([vec![count_row(MAX_TEAMS_PER_USER as i64)]])
            .into_connection();

        let result = create_team(&db, Uuid::now_v7(), create_payload()).await;

        assert!(matches!(result, Err(TeamError::TeamQuotaReached)));
    }

    /// Negative: being past the ceiling is refused too, not just landing
    /// exactly on it. A team deleted out from under the count must not let a
    /// user who is over budget slip through on an equality check.
    #[tokio::test]
    async fn create_team_refuses_a_user_past_the_quota() {
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results([vec![count_row(MAX_TEAMS_PER_USER as i64 + 5)]])
            .into_connection();

        let result = create_team(&db, Uuid::now_v7(), create_payload()).await;

        assert!(matches!(result, Err(TeamError::TeamQuotaReached)));
    }

    /// Positive: one slot below the ceiling, the quota gate lets the request
    /// through. It goes on to the slug lookup, which the mock answers with an
    /// empty set, so the assertion is that the call is not turned away *here*.
    /// Whatever happens further down, it is not a quota refusal.
    #[tokio::test]
    async fn create_team_lets_a_user_below_the_quota_through() {
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results([vec![count_row(MAX_TEAMS_PER_USER as i64 - 1)]])
            .append_query_results([Vec::<teams::Model>::new()])
            .into_connection();

        let result = create_team(&db, Uuid::now_v7(), create_payload()).await;

        assert!(
            !matches!(result, Err(TeamError::TeamQuotaReached)),
            "a user below the ceiling must clear the quota gate"
        );
    }
}

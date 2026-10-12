// backend/crates/services/src/team_service.rs

use std::str::FromStr;

use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DatabaseTransaction,
    EntityTrait, ExprTrait, PaginatorTrait, QueryFilter, QuerySelect, Set, TransactionTrait,
};
use uuid::Uuid;

use domain::dto::team::{
    CreateInvitationRequest, CreateTeamRequest, TeamInvitationResponse, TeamMemberResponse,
    TeamPatch, TeamResponse, TeamRole,
};
use domain::entities::{
    team_invitations, team_members, teams, user_emails, users, workspace_members, workspaces,
};
use domain::types::AccentColor;

use crate::audit_service::{self, AuditContext, AuditError};
use crate::config::Config;
use crate::storage_service::{AvatarKind, ObjectStore, ObjectsToPurge};
use crate::{auth_service, email_service, instance_service, slug_service, token_service};

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

    #[error("a personal team cannot be transferred")]
    PersonalTeamNotTransferable,

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
    })
}

/// Inserts a `team_members` row. Used both by direct addition and by
/// invitation acceptance (token or id).
///
/// Never as Captain. A team gets its Captain when it is created and changes
/// it by transfer; joining as one would hand the team to a second person
/// without the first ever giving it up, and the database allows one.
pub async fn add_member<C: ConnectionTrait>(
    db: &C,
    team_id: Uuid,
    user_id: Uuid,
    role: TeamRole,
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
/// Refused for a personal team, for a target who is not a member or is
/// already the Captain, and for a target scheduled for deletion.
pub async fn transfer_captaincy(
    db: &DatabaseConnection,
    team_id: Uuid,
    target_user_id: Uuid,
    by: TransferBy<'_>,
) -> Result<(), TeamError> {
    let txn = db.begin().await?;

    let team = teams::Entity::find_by_id(team_id)
        .lock_exclusive()
        .one(&txn)
        .await?
        .ok_or(TeamError::TeamNotFound)?;

    if team.personal {
        return Err(TeamError::PersonalTeamNotTransferable);
    }

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

// ---- Personal team auto-creation (3.6) -------------------------------

/// Creates the account's personal team, or does nothing if it already has one.
///
/// Called from `auth_service::verify_email`, inside the same transaction that
/// marks the address verified. That path is not once-per-account: a second
/// verified address runs it again, and so would any replay that reached the
/// verification step twice. Creating a duplicate team each time is both a
/// correctness bug and an unbounded row source driven from an unauthenticated
/// endpoint.
///
/// Guarded twice on purpose. The lookup answers the ordinary case without
/// raising an error, and the unique index catches the two transactions that
/// interleave past it. A check alone is a race, and a constraint alone turns
/// a normal second verification into a failed request.
///
/// The creator becomes the team's Captain in the same step, as `create_team`
/// does: a team nobody belongs to is invisible to its own creator. Both guards
/// return before that insert, so a second pass creates no second member either.
pub async fn create_personal_team(
    txn: &DatabaseTransaction,
    user: &users::Model,
) -> Result<Option<teams::Model>, TeamError> {
    // No personal team for an instance admin, whichever path gets here. Setup
    // creates that account with its address already verified and never calls
    // this, but a second address verified later does. The admin account is
    // for administering the instance; everyday use belongs to a separate
    // account, and a team of its own would invite exactly that use.
    if user.is_instance_admin {
        return Ok(None);
    }

    let existing = teams::Entity::find()
        .filter(teams::Column::CreatedBy.eq(user.id))
        .filter(teams::Column::Personal.eq(true))
        .one(txn)
        .await?;

    if existing.is_some() {
        return Ok(None);
    }

    let name = format!("{}'s team", user.username);

    // The username is already constrained to `[a-zA-Z0-9][a-zA-Z0-9_-]*`, so
    // the slug source can never come out empty here. Propagated rather than
    // unwrapped anyway: the guarantee lives in another module's validator.
    let slug = slug_service::generate_unique_slug(&name, move |candidate| async move {
        Ok(teams::Entity::find()
            .filter(teams::Column::Slug.eq(candidate))
            .one(txn)
            .await?
            .is_some())
    })
    .await?;

    // A savepoint around the inserts. When the unique index refuses the team,
    // Postgres aborts the enclosing transaction unless the failure is
    // contained: every later statement would fail, and the caller's commit
    // would silently roll back the email verification it carries. Rolling
    // back to the savepoint keeps the rest of that transaction alive.
    let savepoint = txn.begin().await?;

    let team = teams::ActiveModel {
        id: Set(Uuid::now_v7()),
        slug: Set(slug),
        name: Set(name),
        color: Set(AccentColor::default().as_str().to_string()),
        personal: Set(true),
        created_by: Set(Some(user.id)),
        ..Default::default()
    }
    .insert(&savepoint)
    .await;

    let team = match team {
        Ok(team) => team,
        // Another transaction created it between the lookup and here. The goal
        // state is reached either way, so this is a success, not a conflict,
        // and that transaction's call inserts the Captain.
        Err(err) if is_unique_violation(&err) => {
            savepoint.rollback().await?;
            return Ok(None);
        }
        Err(err) => return Err(err.into()),
    };

    team_members::ActiveModel {
        team_id: Set(team.id),
        user_id: Set(user.id),
        role: Set(TeamRole::Captain.as_str().to_string()),
        ..Default::default()
    }
    .insert(&savepoint)
    .await?;

    savepoint.commit().await?;

    Ok(Some(team))
}

/// Whether a database error is a unique-constraint violation.
///
/// Mirrors `auth_service::map_unique_violation`, kept local because the only
/// thing this module needs to know is whether the row already existed.
fn is_unique_violation(err: &sea_orm::DbErr) -> bool {
    matches!(
        err.sql_err(),
        Some(sea_orm::SqlErr::UniqueConstraintViolation(_))
    )
}

// ---- Invitations --------------------------------------------------------

/// Matches invitations that are still usable.
///
/// A NULL `expires_at` means the invitation never expires. Written once
/// because every read of this table needs it: an expired row must neither be
/// offered to the invitee, nor block a replacement being issued, nor be
/// acceptable. Leaving it out of the dedupe checks meant an expired invitation
/// locked a person out of ever being re-invited to that team, since the unique
/// indexes still counted the dead row.
fn not_expired() -> Expr {
    team_invitations::Column::ExpiresAt
        .is_null()
        .or(team_invitations::Column::ExpiresAt.gt(chrono::Utc::now()))
}

pub async fn create_invitation(
    db: &DatabaseConnection,
    config: &Config,
    team_id: Uuid,
    creator_id: Uuid,
    payload: CreateInvitationRequest,
) -> Result<TeamInvitationResponse, TeamError> {
    // Joining never makes a Captain (see `add_member`). Refused here, where
    // the Captain asking can be told to transfer, rather than at acceptance,
    // where the invitee would be the one to find out.
    if payload.default_role == TeamRole::Captain {
        return Err(TeamError::CaptainMustTransfer);
    }

    let invited_email = payload
        .invited_email
        .as_deref()
        .map(auth_service::normalize_email);

    // Both ways of naming an invitee collapse to an account id here, so the
    // rest of this function works on one identifier. The DTO guarantees they
    // are never both set (see `invitee_is_named_once`), which is what keeps
    // the row's two targeting columns from ever disagreeing.
    //
    // A username that matches nobody is refused rather than quietly downgraded
    // to a generic link: the caller asked for a specific person, and silently
    // minting a token anyone could redeem is not the same invitation.
    let invited_user_id = match (&payload.username, &invited_email) {
        (Some(username), _) => Some(
            auth_service::find_user_by_username(db, username)
                .await?
                .ok_or(TeamError::InvitedUserNotFound)?,
        ),
        (None, Some(email)) => auth_service::find_verified_user_by_email(db, email).await?,
        (None, None) => None,
    };

    if let Some(invited_user_id) = invited_user_id {
        // Resolved either from a username or from
        // invited_email matching an existing account: either way, inviting
        // someone already on the team is a mistake to reject up front rather
        // than let them discover it only at acceptance (see add_member,
        // still kept as the last line of defense for the race window
        // between this check and the insert below).
        let already_member = team_members::Entity::find_by_id((team_id, invited_user_id))
            .one(db)
            .await?
            .is_some();

        if already_member {
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
    } else if let Some(email) = &invited_email {
        // No matching account yet: dedupe on the email itself instead, same
        // constraint as idx_team_invitations_unique_per_email.
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
        expires_at: Set(payload.expires_at.map(Into::into)),
        max_uses: Set(max_uses),
        token: Set(token.hash),
        ..Default::default()
    }
    .insert(db)
    .await?;

    // Best effort: a mail failure must not undo a completed invitation.
    if let Err(err) = send_invitation_email(
        db,
        config,
        &invitation,
        invited_user_id,
        invited_email.as_deref(),
    )
    .await
    {
        tracing::error!(error = ?err, invitation_id = %invitation.id, "failed to send team invitation email");
    }

    Ok(TeamInvitationResponse {
        id: invitation.id,
        token: Some(token.secret),
        default_role: payload.default_role,
        invited_user_id,
        invited_email,
        expires_at: payload.expires_at,
        max_uses,
        uses: 0,
        created_at: invitation.created_at.into(),
    })
}

/// Picks the right of the two invitation emails: an already-existing,
/// verified account gets told it was added; anyone else gets a link to sign
/// up (the account-less case, no user locale to translate into yet, so the
/// instance's default is used).
async fn send_invitation_email(
    db: &DatabaseConnection,
    config: &Config,
    invitation: &team_invitations::Model,
    invited_user_id: Option<Uuid>,
    invited_email: Option<&str>,
) -> anyhow::Result<()> {
    let team = teams::Entity::find_by_id(invitation.team_id)
        .one(db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("team not found while sending invitation email"))?;

    match (invited_user_id, invited_email) {
        // Pinned to an account. Where to write is the account's business, not
        // the caller's: an address taken from the request would let an Admin
        // send "you were added to <team>" to any mailbox they liked, over an
        // invitation that names someone else entirely.
        (Some(user_id), _) => {
            let user = users::Entity::find_by_id(user_id)
                .one(db)
                .await?
                .ok_or_else(|| {
                    anyhow::anyhow!("invited user not found while sending invitation email")
                })?;

            // No verified primary address is not an error: the invitation is
            // valid and already visible in-app, there is simply nowhere to
            // write to yet.
            let Some(primary) = auth_service::primary_email(db, user_id).await? else {
                tracing::info!(
                    invitation_id = %invitation.id,
                    "invited account has no primary address, skipping invitation email"
                );
                return Ok(());
            };

            if primary.verified_at.is_none() {
                tracing::info!(
                    invitation_id = %invitation.id,
                    "invited account's primary address is unverified, skipping invitation email"
                );
                return Ok(());
            }

            email_service::send_team_invitation_added(
                config,
                &primary.email,
                &user.locale,
                &team.name,
            )
            .await?;
        }
        // No account behind the address yet: invite them to create one. No
        // user locale to translate into, so the instance default is used.
        (None, Some(email)) => {
            let settings = instance_service::settings(db).await?;
            email_service::send_team_invitation_signup(
                config,
                email,
                &settings.default_locale,
                &team.name,
            )
            .await?;
        }
        // A generic link. Nobody in particular to tell about it.
        (None, None) => {}
    }

    Ok(())
}

/// Active generic links for the team (invited_user_id IS NULL), not expired.
pub async fn list_active_invitations(
    db: &DatabaseConnection,
    team_id: Uuid,
) -> Result<Vec<TeamInvitationResponse>, TeamError> {
    let invitations = team_invitations::Entity::find()
        .filter(team_invitations::Column::TeamId.eq(team_id))
        .filter(team_invitations::Column::InvitedUserId.is_null())
        .filter(not_expired())
        .all(db)
        .await?;

    Ok(invitations
        .into_iter()
        .map(|invitation| TeamInvitationResponse {
            id: invitation.id,
            token: None, // never returned in this listing
            default_role: TeamRole::from_str(&invitation.default_role).unwrap_or(TeamRole::Viewer),
            invited_user_id: invitation.invited_user_id,
            invited_email: invitation.invited_email,
            expires_at: invitation.expires_at.map(Into::into),
            max_uses: invitation.max_uses,
            uses: invitation.uses,
            created_at: invitation.created_at.into(),
        })
        .collect())
}

/// Pending targeted invitations for this user, across all teams.
pub async fn list_pending_invitations_for_user(
    db: &DatabaseConnection,
    user_id: Uuid,
) -> Result<Vec<TeamInvitationResponse>, TeamError> {
    let invitations = team_invitations::Entity::find()
        .filter(team_invitations::Column::InvitedUserId.eq(user_id))
        .filter(not_expired())
        .all(db)
        .await?;

    Ok(invitations
        .into_iter()
        .map(|invitation| TeamInvitationResponse {
            id: invitation.id,
            token: None, // never returned in this listing
            default_role: TeamRole::from_str(&invitation.default_role).unwrap_or(TeamRole::Viewer),
            invited_user_id: invitation.invited_user_id,
            invited_email: invitation.invited_email,
            expires_at: invitation.expires_at.map(Into::into),
            max_uses: invitation.max_uses,
            uses: invitation.uses,
            created_at: invitation.created_at.into(),
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

/// Acceptance via a generic link (token in the URL, `POST /teams/join/:token`).
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

    if let Some(expires_at) = invitation.expires_at
        && expires_at < chrono::Utc::now()
    {
        return Err(TeamError::InvitationExpired);
    }

    // An addressed link must not be usable by whoever else it ends up with
    // (forwarded, leaked, shared inbox). Which check applies depends on how
    // the invitation named its invitee, and the account id wins when both are
    // set: it is the stronger statement of the two, and consulting only the
    // address here is what once let a third party redeem a link issued in
    // someone else's name.
    match (&invitation.invited_user_id, &invitation.invited_email) {
        (Some(invited_user_id), _) => {
            if *invited_user_id != user_id {
                return Err(TeamError::InvitationNotForUser);
            }
        }
        (None, Some(invited_email)) => {
            let matches = user_emails::Entity::find()
                .filter(user_emails::Column::UserId.eq(user_id))
                .filter(user_emails::Column::Email.eq(invited_email.clone()))
                .filter(user_emails::Column::VerifiedAt.is_not_null())
                .one(db)
                .await?
                .is_some();

            if !matches {
                return Err(TeamError::InvitationEmailMismatch);
            }
        }
        // A generic link: the token is the only credential it carries.
        (None, None) => {}
    }

    let txn = db.begin().await?;

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
        .exec(&txn)
        .await?;

    if reserved.rows_affected == 0 {
        txn.rollback().await?;
        return Err(TeamError::InvitationMaxUsesReached);
    }

    let role = TeamRole::from_str(&invitation.default_role).unwrap_or(TeamRole::Viewer);
    add_member(&txn, invitation.team_id, user_id, role).await?;

    let team = teams::Entity::find_by_id(invitation.team_id)
        .one(&txn)
        .await?
        .ok_or(TeamError::TeamNotFound)?;

    txn.commit().await?;

    Ok(TeamResponse::from_model(team, role))
}

/// Acceptance via notification (targeted invitation).
pub async fn accept_invitation_by_id(
    db: &DatabaseConnection,
    invitation_id: Uuid,
    user_id: Uuid,
) -> Result<TeamResponse, TeamError> {
    let invitation = team_invitations::Entity::find_by_id(invitation_id)
        .one(db)
        .await?
        .ok_or(TeamError::InvitationNotFound)?;

    if invitation.invited_user_id != Some(user_id) {
        return Err(TeamError::InvitationNotForUser);
    }

    if let Some(expires_at) = invitation.expires_at
        && expires_at < chrono::Utc::now()
    {
        return Err(TeamError::InvitationExpired);
    }

    let txn = db.begin().await?;

    let role = TeamRole::from_str(&invitation.default_role).unwrap_or(TeamRole::Viewer);
    add_member(&txn, invitation.team_id, user_id, role).await?;

    // Delete the invitation
    team_invitations::Entity::delete_by_id(invitation_id)
        .exec(&txn)
        .await?;

    let team = teams::Entity::find_by_id(invitation.team_id)
        .one(&txn)
        .await?
        .ok_or(TeamError::TeamNotFound)?;

    txn.commit().await?;

    Ok(TeamResponse::from_model(team, role))
}

pub async fn reject_invitation_by_id(
    db: &DatabaseConnection,
    invitation_id: Uuid,
    user_id: Uuid,
) -> Result<(), TeamError> {
    let invitation = team_invitations::Entity::find_by_id(invitation_id)
        .one(db)
        .await?
        .ok_or(TeamError::InvitationNotFound)?;

    if invitation.invited_user_id != Some(user_id) {
        return Err(TeamError::InvitationNotForUser);
    }

    team_invitations::Entity::delete_by_id(invitation_id)
        .exec(db)
        .await?;

    Ok(())
}

/// Joins `user_id` to every team with a pending email invitation for `email`,
/// then removes those invitations. Meant to run inside the same transaction
/// as marking that address verified (see `auth_service::verify_email`), the
/// same way `create_personal_team` is called from `register`: an address
/// that is not actually verified yet must not be able to join a team by it.
pub async fn accept_pending_email_invitations(
    txn: &DatabaseTransaction,
    email: &str,
    user_id: Uuid,
) -> Result<(), TeamError> {
    // Only rows that name nobody but this address. An invitation pinned to an
    // account is that account's to accept, through the invitation list.
    //
    // `ck_team_invitations_single_target` now makes a row carrying both an
    // account and an address impossible, which makes the `is_null` filter
    // redundant on paper: any row with an address has no account on it, and
    // `InvitedEmail.eq` already skips the NULL ones. Kept as the second lock
    // rather than removed: it costs nothing, and it is what still holds if
    // the constraint is ever dropped or data arrives around it.
    let invitations = team_invitations::Entity::find()
        .filter(team_invitations::Column::InvitedEmail.eq(email))
        .filter(team_invitations::Column::InvitedUserId.is_null())
        .filter(not_expired())
        .all(txn)
        .await?;

    for invitation in invitations {
        let role = TeamRole::from_str(&invitation.default_role).unwrap_or(TeamRole::Viewer);

        // Already a member through another route in the meantime: drop the
        // now-redundant invitation rather than fail the whole verification.
        match add_member(txn, invitation.team_id, user_id, role).await {
            Ok(_) | Err(TeamError::AlreadyMember) => {}
            Err(err) => return Err(err),
        }

        team_invitations::Entity::delete_by_id(invitation.id)
            .exec(txn)
            .await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, EmailBackend, SmtpTls, StorageConfig};
    use domain::dto::team::CreateInvitationRequest;
    use domain::types::AccentColor;
    use sea_orm::{DatabaseBackend, MockDatabase, Value};
    use std::collections::BTreeMap;

    /// Shapes a row the way `PaginatorTrait::count` reads it back: it issues
    /// `COUNT(*) AS num_items` and pulls that one column out as an i64.
    fn count_row(n: i64) -> BTreeMap<String, Value> {
        BTreeMap::from([("num_items".to_owned(), Value::BigInt(Some(n)))])
    }

    /// Never read on the paths under test here: every test below returns
    /// before `create_invitation` reaches the point where it would send mail.
    fn unused_config() -> Config {
        Config {
            port: 3000,
            public_url: String::new(),
            cors_origin: String::new(),
            setup_token: None,
            database_url: String::new(),
            database_max_connections: 1,
            redis_url: String::new(),
            jwt_secret: String::new(),
            encryption_key: String::new(),
            turnstile_site_key: None,
            turnstile_secret_key: None,
            cookie_secure: false,
            trusted_proxy_count: 0,
            block_disposable_emails: false,
            check_pwned_passwords: false,
            email_backend: EmailBackend::Console,
            email_from: String::new(),
            support_email: None,
            smtp_host: String::new(),
            smtp_port: 0,
            smtp_username: String::new(),
            smtp_password: String::new(),
            smtp_tls: SmtpTls::None,
            resend_api_key: String::new(),
            storage: StorageConfig {
                endpoint: None,
                region: "us-east-1".to_owned(),
                access_key_id: String::new(),
                secret_access_key: String::new(),
                force_path_style: true,
                bucket_attachments: String::new(),
                bucket_avatars: String::new(),
                key_prefix: None,
                public_base_url: None,
            },
        }
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
            expires_at: None,
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

    #[tokio::test]
    async fn create_invitation_rejects_a_second_pending_invite_to_the_same_unregistered_email() {
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            // 1: email resolution finds no verified account.
            .append_query_results([Vec::<user_emails::Model>::new()])
            // 2: a pending invitation for that email already exists.
            .append_query_results([vec![pending_invitation_row()]])
            .into_connection();

        let payload = CreateInvitationRequest {
            invited_email: Some("new@example.test".to_owned()),
            ..base_invitation_payload()
        };

        let result = create_invitation(
            &db,
            &unused_config(),
            Uuid::now_v7(),
            Uuid::now_v7(),
            payload,
        )
        .await;

        assert!(matches!(result, Err(TeamError::AlreadyInvited)));
    }

    #[tokio::test]
    async fn create_invitation_resolves_invited_email_to_an_existing_verified_account() {
        let existing_user_id = Uuid::now_v7();
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            // 1: email resolution matches an existing verified account.
            .append_query_results([vec![verified_email_row(existing_user_id)]])
            // 2: that account is not already a member of the team.
            .append_query_results([Vec::<team_members::Model>::new()])
            // 3: that account already has a pending invitation to this team,
            // proving the invitation was pinned to the resolved account (this
            // dedupe branch only runs once an account id is known).
            .append_query_results([vec![pending_invitation_row()]])
            .into_connection();

        let payload = CreateInvitationRequest {
            invited_email: Some("existing@example.test".to_owned()),
            ..base_invitation_payload()
        };

        let result = create_invitation(
            &db,
            &unused_config(),
            Uuid::now_v7(),
            Uuid::now_v7(),
            payload,
        )
        .await;

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

        let result =
            create_invitation(&db, &unused_config(), team_id, Uuid::now_v7(), payload).await;

        assert!(matches!(result, Err(TeamError::AlreadyMember)));
    }

    #[tokio::test]
    async fn create_invitation_rejects_inviting_by_email_someone_already_a_member() {
        let team_id = Uuid::now_v7();
        let existing_user_id = Uuid::now_v7();

        let db = MockDatabase::new(DatabaseBackend::Postgres)
            // 1: email resolution matches an existing verified account.
            .append_query_results([vec![verified_email_row(existing_user_id)]])
            // 2: that account is already a member of the team.
            .append_query_results([vec![team_member_row(team_id, existing_user_id)]])
            .into_connection();

        let payload = CreateInvitationRequest {
            invited_email: Some("existing@example.test".to_owned()),
            ..base_invitation_payload()
        };

        let result =
            create_invitation(&db, &unused_config(), team_id, Uuid::now_v7(), payload).await;

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

        let result = create_invitation(
            &db,
            &unused_config(),
            Uuid::now_v7(),
            Uuid::now_v7(),
            payload,
        )
        .await;

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

    #[tokio::test]
    async fn accept_pending_email_invitations_is_a_no_op_without_matching_invitations() {
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results([Vec::<team_invitations::Model>::new()])
            .into_connection();

        let txn = db.begin().await.expect("mock transaction must open");

        let result =
            accept_pending_email_invitations(&txn, "nobody@example.test", Uuid::now_v7()).await;

        assert!(result.is_ok());
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

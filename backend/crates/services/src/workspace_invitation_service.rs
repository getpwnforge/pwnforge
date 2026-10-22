// backend/crates/services/src/workspace_invitation_service.rs
//
// Guest invitations to a team workspace. Each names one account or one
// address, lasts seven days, is answered only by its invitee, and stays as
// history once answered: accepted, declined, cancelled or expired, with the
// address erased as soon as it is no longer pending.

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use redis::aio::ConnectionManager;
use sea_orm::sea_query::{Expr, LockType};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DbErr, EntityTrait,
    QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};
use uuid::Uuid;

use domain::dto::invitation::{INVITATION_DEFAULT_DAYS, InvitationTarget};
use domain::dto::team::TeamRole;
use domain::dto::workspace::{
    CancelReason, CreateWorkspaceInvitationRequest, GuestRole, InvitationStatus, Membership,
    WorkspaceInvitationPreview, WorkspaceInvitationResponse,
};
use domain::entities::{team_members, teams, workspace_invitations, workspace_members, workspaces};

use crate::background::Shutdown;
use crate::config::Config;
use crate::email_service::InvitationKind;
use crate::invitation_service::{self, CreatedInvitation, InvitationMail, MailOutcome};
use crate::workspace_service::{self, WorkspaceError};
use crate::{auth_service, permissions_service, token_service};

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceInvitationError {
    #[error(transparent)]
    Db(#[from] DbErr),

    /// Solo, archived, no access, or a role too low: the workspace's own
    /// refusals, answered as the member routes answer them.
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),

    #[error("no account matches the invited username")]
    InvitedUserNotFound,

    /// The invitee is on the workspace's team, or already a guest of it.
    #[error("the invitee already has a place in this workspace")]
    AlreadyMember,

    #[error("an invitation for this invitee is already pending")]
    AlreadyPending,

    /// Unknown, or not the caller's to see: the two are one answer.
    #[error("invitation not found")]
    InvitationNotFound,

    #[error("invitation has expired")]
    InvitationExpired,

    /// Already accepted, declined or cancelled.
    #[error("invitation is no longer pending")]
    NotPending,
}

type Result<T> = std::result::Result<T, WorkspaceInvitationError>;

/// How long the cleanup keeps an invitation once it stopped being pending.
pub const HISTORY_RETENTION_DAYS: i64 = 90;

/// Batched, like the other cleanups: one unbounded statement on a large
/// table holds its locks for too long.
const BATCH_SIZE: u64 = 1000;

/// The team of a workspace that may receive guests: a team workspace. A solo
/// one is private to its owner, so every invitation route refuses it.
fn team_of(workspace: &workspaces::Model) -> Result<Uuid> {
    Ok(workspace.team_id.ok_or(WorkspaceError::SoloWorkspace)?)
}

/// Who may invite: a member of the workspace's team whose access here is at
/// least Member. A Viewer, by team role or by override, may not, nor may a
/// guest, whatever their role: inviting is the team's to do.
async fn require_inviter<C: ConnectionTrait>(
    conn: &C,
    workspace: &workspaces::Model,
    team_id: Uuid,
    user_id: Uuid,
) -> Result<Membership> {
    let on_team = team_members::Entity::find_by_id((team_id, user_id))
        .one(conn)
        .await?
        .is_some();
    let membership = permissions_service::effective_role(conn, user_id, workspace)
        .await
        .map_err(WorkspaceError::from)?;

    if !on_team || !membership.satisfies(TeamRole::Member) {
        return Err(WorkspaceError::InsufficientRole.into());
    }

    Ok(membership)
}

fn is_admin(membership: Membership) -> bool {
    membership.satisfies(TeamRole::Admin)
}

/// The status a row reads as: a pending one past its date is expired, even
/// before the cleanup records it.
fn effective_status(row: &workspace_invitations::Model, now: DateTime<Utc>) -> InvitationStatus {
    let stored = InvitationStatus::parse(&row.status).unwrap_or(InvitationStatus::Expired);
    if stored == InvitationStatus::Pending && row.expires_at <= now {
        return InvitationStatus::Expired;
    }
    stored
}

/// `with_access` says whether the reader has access to the workspace, the
/// only readers its avatar is served to: anyone else is announced none.
fn to_response(
    row: workspace_invitations::Model,
    workspace: workspaces::Model,
    with_access: bool,
    team_name: String,
    usernames: &HashMap<Uuid, String>,
    now: DateTime<Utc>,
) -> WorkspaceInvitationResponse {
    let status = effective_status(&row, now);
    let name_of = |id: Option<Uuid>| id.and_then(|id| usernames.get(&id).cloned());

    WorkspaceInvitationResponse {
        id: row.id,
        workspace_id: row.workspace_id,
        workspace_name: workspace.name,
        workspace_color: workspace.color.parse().unwrap_or_default(),
        workspace_avatar_version: workspace.avatar_version.filter(|_| with_access),
        team_name,
        status,
        role: GuestRole::parse(&row.role).unwrap_or(GuestRole::Viewer),
        // Not from the address: it is erased once the invitation is closed,
        // and a workspace has no generic link it could be mistaken for.
        target: if row.invited_user_id.is_some() {
            InvitationTarget::Username
        } else {
            InvitationTarget::Email
        },
        invited_user_id: row.invited_user_id,
        invited_username: name_of(row.invited_user_id),
        // Only while pending. Erased from the row on every way out of that
        // state; withheld here too for a row past its date that the cleanup
        // has not reached yet.
        invited_email: if status == InvitationStatus::Pending {
            row.invited_email
        } else {
            None
        },
        accepted_by: row.accepted_by,
        accepted_username: name_of(row.accepted_by),
        invited_by: row.invited_by,
        invited_by_username: name_of(row.invited_by),
        cancel_reason: row.cancel_reason.as_deref().and_then(CancelReason::parse),
        expires_at: row.expires_at.into(),
        created_at: row.created_at.into(),
        resolved_at: row
            .resolved_at
            .map(Into::into)
            .or((status == InvitationStatus::Expired).then(|| row.expires_at.into())),
    }
}

/// The rows with their workspace and whether the reader has access to it,
/// as answers: the names they show read in two queries, the accounts and the
/// teams, however many rows there are.
async fn responses<C: ConnectionTrait>(
    conn: &C,
    rows: Vec<(workspace_invitations::Model, workspaces::Model, bool)>,
) -> Result<Vec<WorkspaceInvitationResponse>> {
    let usernames = invitation_service::usernames(
        conn,
        rows.iter().flat_map(|(row, _, _)| {
            [row.invited_user_id, row.accepted_by, row.invited_by]
                .into_iter()
                .flatten()
        }),
    )
    .await?;
    let team_ids: Vec<Uuid> = rows.iter().filter_map(|(_, w, _)| w.team_id).collect();
    let team_names: HashMap<Uuid, String> = if team_ids.is_empty() {
        HashMap::new()
    } else {
        teams::Entity::find()
            .filter(teams::Column::Id.is_in(team_ids))
            .all(conn)
            .await?
            .into_iter()
            .map(|team| (team.id, team.name))
            .collect()
    };
    let now = Utc::now();

    rows.into_iter()
        .map(|(row, workspace, with_access)| {
            // Only a team workspace takes invitations (`team_of`), and its
            // team goes only with it.
            let team_name = workspace
                .team_id
                .and_then(|id| team_names.get(&id).cloned())
                .ok_or(WorkspaceError::DataIntegrity(workspace.id))?;
            Ok(to_response(
                row,
                workspace,
                with_access,
                team_name,
                &usernames,
                now,
            ))
        })
        .collect()
}

/// Whether `user_id` already has a place in the workspace: on its team, or
/// a guest of it.
async fn has_place<C: ConnectionTrait>(
    conn: &C,
    workspace_id: Uuid,
    team_id: Uuid,
    user_id: Uuid,
) -> Result<bool> {
    if team_members::Entity::find_by_id((team_id, user_id))
        .one(conn)
        .await?
        .is_some()
    {
        return Ok(true);
    }

    Ok(
        workspace_members::Entity::find_by_id((workspace_id, user_id))
            .filter(workspace_members::Column::IsGuest.eq(true))
            .one(conn)
            .await?
            .is_some(),
    )
}

/// Records as expired the pending rows of `workspace_id` past their date that
/// `matching` selects, so they stop holding the one-pending-per-invitee index
/// and stop reading as pending anywhere.
async fn expire_pending<C: ConnectionTrait>(
    conn: &C,
    workspace_id: Uuid,
    matching: sea_orm::Condition,
    now: DateTime<Utc>,
) -> std::result::Result<(), DbErr> {
    workspace_invitations::Entity::update_many()
        .col_expr(
            workspace_invitations::Column::Status,
            Expr::value(InvitationStatus::Expired.as_str()),
        )
        .col_expr(
            workspace_invitations::Column::InvitedEmail,
            Expr::value(Option::<String>::None),
        )
        .col_expr(
            workspace_invitations::Column::ResolvedAt,
            Expr::col(workspace_invitations::Column::ExpiresAt),
        )
        .filter(workspace_invitations::Column::WorkspaceId.eq(workspace_id))
        .filter(workspace_invitations::Column::Status.eq(InvitationStatus::Pending.as_str()))
        .filter(workspace_invitations::Column::ExpiresAt.lte(now))
        .filter(matching)
        .exec(conn)
        .await?;
    Ok(())
}

/// The refusals `create_invitation` opens with, for the route to run before
/// it spends the caller's rate-limit budget: a solo workspace, an archived
/// one, a caller who may not invite. `create_invitation` checks them again
/// under the workspace lock.
pub async fn ensure_can_invite(
    db: &DatabaseConnection,
    workspace: &workspaces::Model,
    actor_id: Uuid,
) -> Result<()> {
    let team_id = team_of(workspace)?;
    workspace_service::ensure_writable(workspace)?;
    require_inviter(db, workspace, team_id, actor_id).await?;
    Ok(())
}

/// Invites one account or one address into a team workspace as a guest.
///
/// Refusals in order: a solo workspace, an archived one, a caller who may
/// not invite, an unknown username, an invitee who already has a place here
/// (on the team or a guest, by account or by an account holding the address
/// verified), and a pending invitation for the same invitee.
///
/// The answer for an address is the same whether an account holds it or
/// not, and the mail goes out either way (see `invitation_service`). Run in
/// one transaction with the workspace row locked, as `archive_workspace`
/// locks it: an invitation is either created before an archive, and then
/// cancelled by it, or sees the archive and is refused.
pub async fn create_invitation(
    db: &DatabaseConnection,
    redis: &mut ConnectionManager,
    config: &Config,
    workspace_id: Uuid,
    actor_id: Uuid,
    input: CreateWorkspaceInvitationRequest,
) -> Result<CreatedInvitation<WorkspaceInvitationResponse>> {
    let txn = db.begin().await?;

    let workspace = workspaces::Entity::find_by_id(workspace_id)
        .lock(LockType::NoKeyUpdate)
        .one(&txn)
        .await?
        .ok_or(WorkspaceError::WorkspaceNotFound)?;
    let team_id = team_of(&workspace)?;
    workspace_service::ensure_writable(&workspace)?;
    require_inviter(&txn, &workspace, team_id, actor_id).await?;

    let invited_email = input
        .invited_email
        .as_deref()
        .map(auth_service::normalize_email);
    let invited_user_id = match &input.username {
        Some(username) => Some(
            auth_service::find_user_by_username(db, username)
                .await?
                .ok_or(WorkspaceInvitationError::InvitedUserNotFound)?,
        ),
        None => None,
    };

    // By address, the account that has it verified, if any. Looked up only
    // to refuse inviting someone who is already here, which the author can
    // tell from the members list anyway; the answer otherwise never depends
    // on it.
    let holder = match (invited_user_id, &invited_email) {
        (Some(user_id), _) => Some(user_id),
        (None, Some(email)) => auth_service::find_verified_user_by_email(db, email).await?,
        (None, None) => None,
    };
    if let Some(holder) = holder
        && has_place(&txn, workspace_id, team_id, holder).await?
    {
        return Err(WorkspaceInvitationError::AlreadyMember);
    }

    let now = Utc::now();
    let same_invitee = match (invited_user_id, &invited_email) {
        (Some(user_id), _) => workspace_invitations::Column::InvitedUserId.eq(user_id),
        (None, Some(email)) => workspace_invitations::Column::InvitedEmail.eq(email.clone()),
        // `names_one_invitee` refuses a request naming nobody.
        (None, None) => return Err(WorkspaceInvitationError::InvitedUserNotFound),
    };
    expire_pending(
        &txn,
        workspace_id,
        sea_orm::Condition::all().add(same_invitee.clone()),
        now,
    )
    .await?;

    let already_pending = workspace_invitations::Entity::find()
        .filter(workspace_invitations::Column::WorkspaceId.eq(workspace_id))
        .filter(workspace_invitations::Column::Status.eq(InvitationStatus::Pending.as_str()))
        .filter(same_invitee)
        .one(&txn)
        .await?
        .is_some();
    if already_pending {
        return Err(WorkspaceInvitationError::AlreadyPending);
    }

    let token = token_service::generate_opaque_token();
    let expires_at = now + Duration::days(INVITATION_DEFAULT_DAYS);

    let row = workspace_invitations::ActiveModel {
        id: Set(Uuid::now_v7()),
        workspace_id: Set(workspace_id),
        invited_user_id: Set(invited_user_id),
        invited_email: Set(invited_email.clone()),
        role: Set(input.role.as_str().to_owned()),
        token: Set(token.hash),
        invited_by: Set(Some(actor_id)),
        expires_at: Set(expires_at.into()),
        ..Default::default()
    }
    .insert(&txn)
    .await?;

    txn.commit().await?;

    // After the commit: the mail must never name an invitation that a
    // rollback took back.
    let recipient = match (invited_user_id, &invited_email) {
        (Some(user_id), _) => invitation_service::account_recipient(db, user_id).await,
        (None, Some(email)) => invitation_service::address_recipient(db, email)
            .await
            .map(Some)
            .map_err(|crate::instance_service::InstanceError::Db(err)| err),
        (None, None) => Ok(None),
    };
    let mail = match recipient {
        Ok(None) => MailOutcome::NoAddress,
        Ok(Some(recipient)) => {
            invitation_service::send_mail(
                redis,
                config,
                &recipient,
                InvitationMail {
                    kind: InvitationKind::Workspace,
                    name: &workspace.name,
                    token: &token.secret,
                    expires_at,
                },
            )
            .await
        }
        Err(error) => {
            tracing::error!(?error, invitation_id = %row.id, "could not address the workspace invitation email");
            MailOutcome::Failed
        }
    };

    // The token went to the invitee's mail, and goes nowhere else: the
    // author holding it would hold the invitee's link.
    // The author has access: `require_inviter` saw to it.
    let invitation = responses(db, vec![(row, workspace, true)])
        .await?
        .pop()
        .ok_or(WorkspaceInvitationError::InvitationNotFound)?;

    Ok(CreatedInvitation {
        invitation,
        mail: Some(mail),
    })
}

/// The workspace's invitations, every status, newest first. The Captain and
/// the Admins see all of them; any other member who may invite sees only
/// their own. Those who may not invite see none (`InsufficientRole`).
pub async fn list_invitations(
    db: &DatabaseConnection,
    workspace: &workspaces::Model,
    actor_id: Uuid,
) -> Result<Vec<WorkspaceInvitationResponse>> {
    let team_id = team_of(workspace)?;
    let membership = require_inviter(db, workspace, team_id, actor_id).await?;

    let mut query = workspace_invitations::Entity::find()
        .filter(workspace_invitations::Column::WorkspaceId.eq(workspace.id))
        .order_by_desc(workspace_invitations::Column::CreatedAt);
    if !is_admin(membership) {
        query = query.filter(workspace_invitations::Column::InvitedBy.eq(actor_id));
    }

    let rows = query.all(db).await?;
    responses(
        db,
        // Read by those who may invite, all with access.
        rows.into_iter()
            .map(|row| (row, workspace.clone(), true))
            .collect(),
    )
    .await
}

/// Withdraws a pending invitation: by the Captain, an Admin, or its author.
///
/// Anyone else on the workspace is told it does not exist, which is what
/// their own list says: they see only the invitations they sent.
pub async fn cancel_invitation(
    db: &DatabaseConnection,
    workspace: &workspaces::Model,
    actor_id: Uuid,
    invitation_id: Uuid,
) -> Result<()> {
    team_of(workspace)?;
    let txn = db.begin().await?;

    let row = workspace_invitations::Entity::find_by_id(invitation_id)
        .filter(workspace_invitations::Column::WorkspaceId.eq(workspace.id))
        .lock_exclusive()
        .one(&txn)
        .await?
        .ok_or(WorkspaceInvitationError::InvitationNotFound)?;

    let membership = permissions_service::effective_role(&txn, actor_id, workspace)
        .await
        .map_err(WorkspaceError::from)?;
    if row.invited_by != Some(actor_id) && !is_admin(membership) {
        return Err(WorkspaceInvitationError::InvitationNotFound);
    }

    if effective_status(&row, Utc::now()) != InvitationStatus::Pending {
        return Err(WorkspaceInvitationError::NotPending);
    }

    close(
        &txn,
        row,
        InvitationStatus::Cancelled,
        Some(CancelReason::Revoked),
        None,
    )
    .await?;
    txn.commit().await?;
    Ok(())
}

/// Ends a pending row: its status, why, who accepted it, and when. The
/// address goes in the same write, as `ck_workspace_invitations_email_while_pending`
/// requires.
async fn close<C: ConnectionTrait>(
    conn: &C,
    row: workspace_invitations::Model,
    status: InvitationStatus,
    reason: Option<CancelReason>,
    accepted_by: Option<Uuid>,
) -> Result<()> {
    workspace_invitations::ActiveModel {
        id: Set(row.id),
        status: Set(status.as_str().to_owned()),
        cancel_reason: Set(reason.map(|reason| reason.as_str().to_owned())),
        accepted_by: Set(accepted_by),
        invited_email: Set(None),
        resolved_at: Set(Some(Utc::now().into())),
        ..Default::default()
    }
    .update(conn)
    .await?;
    Ok(())
}

/// Cancels every pending invitation of a workspace being archived, in the
/// archive's own transaction. Those already past their date are recorded as
/// expired instead, which is what they were. Unarchiving brings none back.
pub(crate) async fn close_on_archive<C: ConnectionTrait>(
    conn: &C,
    workspace_id: Uuid,
) -> std::result::Result<(), DbErr> {
    let now = Utc::now();
    expire_pending(conn, workspace_id, sea_orm::Condition::all(), now).await?;

    workspace_invitations::Entity::update_many()
        .col_expr(
            workspace_invitations::Column::Status,
            Expr::value(InvitationStatus::Cancelled.as_str()),
        )
        .col_expr(
            workspace_invitations::Column::CancelReason,
            Expr::value(CancelReason::WorkspaceArchived.as_str()),
        )
        .col_expr(
            workspace_invitations::Column::InvitedEmail,
            Expr::value(Option::<String>::None),
        )
        .col_expr(workspace_invitations::Column::ResolvedAt, Expr::value(now))
        .filter(workspace_invitations::Column::WorkspaceId.eq(workspace_id))
        .filter(workspace_invitations::Column::Status.eq(InvitationStatus::Pending.as_str()))
        .exec(conn)
        .await?;
    Ok(())
}

/// Pending invitations for `user_id`, across workspaces: those naming the
/// account, and those sent to an address it has verified, matched now.
pub async fn list_pending_for_user(
    db: &DatabaseConnection,
    user_id: Uuid,
) -> Result<Vec<WorkspaceInvitationResponse>> {
    let rows = workspace_invitations::Entity::find()
        .filter(invitation_service::addressed_to(
            workspace_invitations::Column::InvitedUserId,
            workspace_invitations::Column::InvitedEmail,
            user_id,
        ))
        .filter(workspace_invitations::Column::Status.eq(InvitationStatus::Pending.as_str()))
        .filter(workspace_invitations::Column::ExpiresAt.gt(Utc::now()))
        .order_by_asc(workspace_invitations::Column::CreatedAt)
        .find_also_related(workspaces::Entity)
        .all(db)
        .await?;

    let rows: Vec<_> = rows
        .into_iter()
        .filter_map(|(row, workspace)| workspace.map(|workspace| (row, workspace)))
        .collect();

    // Whether the invitee can already reach each workspace, resolved as every
    // route resolves access, on their rows read in two queries: an invitation
    // stays pending when its invitee joins the team meanwhile.
    let workspace_ids: Vec<Uuid> = rows.iter().map(|(_, w)| w.id).collect();
    let own_rows: HashMap<Uuid, workspace_members::Model> = if workspace_ids.is_empty() {
        HashMap::new()
    } else {
        workspace_members::Entity::find()
            .filter(workspace_members::Column::UserId.eq(user_id))
            .filter(workspace_members::Column::WorkspaceId.is_in(workspace_ids))
            .all(db)
            .await?
            .into_iter()
            .map(|member| (member.workspace_id, member))
            .collect()
    };
    let team_ids: Vec<Uuid> = rows.iter().filter_map(|(_, w)| w.team_id).collect();
    let team_roles: HashMap<Uuid, TeamRole> = if team_ids.is_empty() {
        HashMap::new()
    } else {
        team_members::Entity::find()
            .filter(team_members::Column::UserId.eq(user_id))
            .filter(team_members::Column::TeamId.is_in(team_ids))
            .all(db)
            .await?
            .into_iter()
            .map(|member| {
                (
                    member.team_id,
                    member.role.parse().unwrap_or(TeamRole::Viewer),
                )
            })
            .collect()
    };

    let rows = rows
        .into_iter()
        .map(|(row, workspace)| {
            let team_role = workspace
                .team_id
                .and_then(|id| team_roles.get(&id).copied());
            let access = permissions_service::resolve(
                &workspace,
                user_id,
                own_rows.get(&workspace.id),
                team_role,
            );
            let with_access = access.membership.satisfies(TeamRole::Viewer);
            (row, workspace, with_access)
        })
        .collect();

    responses(db, rows).await
}

/// How an invitee names the invitation they answer.
#[derive(Debug, Clone, Copy)]
pub enum InvitationRef<'a> {
    Id(Uuid),
    Token(&'a str),
}

/// The invitation `by` names, if `user_id` is its invitee: named by account,
/// or holding its address verified. Anything else, an unknown id or token
/// included, is `InvitationNotFound`, and the same one.
async fn find_for_invitee<C: ConnectionTrait>(
    conn: &C,
    by: InvitationRef<'_>,
    user_id: Uuid,
    lock: bool,
) -> Result<workspace_invitations::Model> {
    // Or the account that accepted it: an invitation by address no longer
    // carries the address once answered, and its invitee asking again is
    // told it is answered rather than that it does not exist.
    let mut query = workspace_invitations::Entity::find().filter(
        sea_orm::Condition::any()
            .add(invitation_service::addressed_to(
                workspace_invitations::Column::InvitedUserId,
                workspace_invitations::Column::InvitedEmail,
                user_id,
            ))
            .add(workspace_invitations::Column::AcceptedBy.eq(user_id)),
    );
    query = match by {
        InvitationRef::Id(id) => query.filter(workspace_invitations::Column::Id.eq(id)),
        InvitationRef::Token(token) => query.filter(
            workspace_invitations::Column::Token.eq(token_service::hash_opaque_token(token)),
        ),
    };
    if lock {
        query = query.lock_exclusive();
    }

    query
        .one(conn)
        .await?
        .ok_or(WorkspaceInvitationError::InvitationNotFound)
}

/// Refuses an invitation that can no longer be answered.
fn ensure_answerable(row: &workspace_invitations::Model) -> Result<()> {
    match effective_status(row, Utc::now()) {
        InvitationStatus::Pending => Ok(()),
        InvitationStatus::Expired => Err(WorkspaceInvitationError::InvitationExpired),
        _ => Err(WorkspaceInvitationError::NotPending),
    }
}

/// Accepts an invitation as its invitee, from the pending list or by the
/// mail's link: adds `user_id` as a guest with the invited role, recording
/// the author, and closes the invitation as accepted by them.
///
/// The workspace row is locked first, as creation and archiving lock it,
/// then the invitation, so of two acceptances only one finds it pending,
/// and an archive is either seen (the invitation is cancelled) or waits.
/// Refused when the invitee has meanwhile joined the team or become a guest.
pub async fn accept(
    db: &DatabaseConnection,
    by: InvitationRef<'_>,
    user_id: Uuid,
) -> Result<(workspaces::Model, Membership)> {
    let found = find_for_invitee(db, by, user_id, false).await?;

    let txn = db.begin().await?;
    let workspace = workspaces::Entity::find_by_id(found.workspace_id)
        .lock(LockType::NoKeyUpdate)
        .one(&txn)
        .await?
        .ok_or(WorkspaceInvitationError::InvitationNotFound)?;
    let row = find_for_invitee(&txn, InvitationRef::Id(found.id), user_id, true).await?;
    ensure_answerable(&row)?;
    let team_id = team_of(&workspace)?;
    workspace_service::ensure_writable(&workspace)?;

    if team_members::Entity::find_by_id((team_id, user_id))
        .one(&txn)
        .await?
        .is_some()
        || workspace_members::Entity::find_by_id((workspace.id, user_id))
            .one(&txn)
            .await?
            .is_some()
    {
        return Err(WorkspaceInvitationError::AlreadyMember);
    }

    let role = GuestRole::parse(&row.role).unwrap_or(GuestRole::Viewer);
    workspace_members::ActiveModel {
        workspace_id: Set(workspace.id),
        user_id: Set(user_id),
        role: Set(Some(role.as_str().to_owned())),
        is_guest: Set(true),
        invited_by: Set(row.invited_by),
        ..Default::default()
    }
    .insert(&txn)
    .await?;

    close(&txn, row, InvitationStatus::Accepted, None, Some(user_id)).await?;
    txn.commit().await?;

    Ok((workspace, Membership::Active(role.into())))
}

/// Declines an invitation as its invitee. An address with no account behind
/// it has nobody to decline: that invitation simply expires.
pub async fn decline(db: &DatabaseConnection, invitation_id: Uuid, user_id: Uuid) -> Result<()> {
    let txn = db.begin().await?;
    let row = find_for_invitee(&txn, InvitationRef::Id(invitation_id), user_id, true).await?;
    ensure_answerable(&row)?;
    close(&txn, row, InvitationStatus::Declined, None, None).await?;
    txn.commit().await?;
    Ok(())
}

/// What the invitee sees behind the mail's link before answering: the
/// workspace's name, the author's username, the role and the expiry.
///
/// Only for the account it is for, and only while it can be answered: an
/// unknown token, someone else's, and one expired or already answered all
/// get the same `InvitationNotFound`.
pub async fn preview(
    db: &DatabaseConnection,
    token: &str,
    user_id: Uuid,
) -> Result<WorkspaceInvitationPreview> {
    let row = find_for_invitee(db, InvitationRef::Token(token), user_id, false).await?;
    if effective_status(&row, Utc::now()) != InvitationStatus::Pending {
        return Err(WorkspaceInvitationError::InvitationNotFound);
    }

    let workspace = workspaces::Entity::find_by_id(row.workspace_id)
        .one(db)
        .await?
        .ok_or(WorkspaceInvitationError::InvitationNotFound)?;
    let authors = invitation_service::usernames(db, row.invited_by).await?;

    Ok(WorkspaceInvitationPreview {
        workspace_name: workspace.name,
        invited_by_username: row.invited_by.and_then(|id| authors.get(&id).cloned()),
        role: GuestRole::parse(&row.role).unwrap_or(GuestRole::Viewer),
        expires_at: row.expires_at.into(),
    })
}

/// Records as expired every pending invitation past its date, erasing its
/// address. Returns how many. Run by the cleanup task, which stops between
/// two batches once `shutdown` is requested.
pub async fn expire_stale(
    db: &DatabaseConnection,
    now: DateTime<Utc>,
    shutdown: &Shutdown,
) -> std::result::Result<u64, DbErr> {
    let mut total = 0;

    loop {
        // A shutdown starts no new batch; the one before it has finished.
        if shutdown.requested() {
            break;
        }
        let ids: Vec<Uuid> = workspace_invitations::Entity::find()
            .select_only()
            .column(workspace_invitations::Column::Id)
            .filter(workspace_invitations::Column::Status.eq(InvitationStatus::Pending.as_str()))
            .filter(workspace_invitations::Column::ExpiresAt.lte(now))
            .limit(BATCH_SIZE)
            .into_tuple()
            .all(db)
            .await?;
        if ids.is_empty() {
            break;
        }

        let result = workspace_invitations::Entity::update_many()
            .col_expr(
                workspace_invitations::Column::Status,
                Expr::value(InvitationStatus::Expired.as_str()),
            )
            .col_expr(
                workspace_invitations::Column::InvitedEmail,
                Expr::value(Option::<String>::None),
            )
            .col_expr(
                workspace_invitations::Column::ResolvedAt,
                Expr::col(workspace_invitations::Column::ExpiresAt),
            )
            .filter(workspace_invitations::Column::Id.is_in(ids))
            .filter(workspace_invitations::Column::Status.eq(InvitationStatus::Pending.as_str()))
            .exec(db)
            .await?;
        total += result.rows_affected;
    }

    Ok(total)
}

/// Deletes the invitations that stopped being pending more than
/// `HISTORY_RETENTION_DAYS` ago. A pending one is never deleted, whatever
/// its age: `expire_stale` closes it first, and the retention runs from
/// then. Returns how many. Run by the cleanup task, which stops between two
/// batches once `shutdown` is requested.
pub async fn purge_history(
    db: &DatabaseConnection,
    now: DateTime<Utc>,
    shutdown: &Shutdown,
) -> std::result::Result<u64, DbErr> {
    let cutoff = now - Duration::days(HISTORY_RETENTION_DAYS);
    let mut total = 0;

    loop {
        // A shutdown starts no new batch; the one before it has finished.
        if shutdown.requested() {
            break;
        }
        let ids: Vec<Uuid> = workspace_invitations::Entity::find()
            .select_only()
            .column(workspace_invitations::Column::Id)
            .filter(workspace_invitations::Column::Status.ne(InvitationStatus::Pending.as_str()))
            .filter(workspace_invitations::Column::ResolvedAt.lt(cutoff))
            .limit(BATCH_SIZE)
            .into_tuple()
            .all(db)
            .await?;
        if ids.is_empty() {
            break;
        }

        let result = workspace_invitations::Entity::delete_many()
            .filter(workspace_invitations::Column::Id.is_in(ids))
            .filter(workspace_invitations::Column::Status.ne(InvitationStatus::Pending.as_str()))
            .exec(db)
            .await?;
        total += result.rows_affected;
    }

    Ok(total)
}

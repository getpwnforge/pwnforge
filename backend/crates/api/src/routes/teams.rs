// backend/crates/api/src/routes/teams.rs

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, patch, post},
};
use uuid::Uuid;
use validator::Validate;

use crate::middleware::{address::require_mailbox, auth::AuthUser, json::JsonBody};
use crate::routes::enforce_invitation_limit;
use crate::{error::AppError, state::AppState};
use domain::dto::error_responses::{
    RateLimitedErrorResponse, SimpleErrorResponse, UnprocessableErrorResponse,
};
use domain::dto::invitation::{InvitationTokenRequest, InvitedByFilter};
use domain::dto::team::{
    ChangeMemberRoleRequest, CreateInvitationRequest, CreateTeamRequest, CreatedTeamInvitation,
    JoinTeamRequest, TeamInvitationPreview, TeamInvitationResponse, TeamMemberResponse, TeamPatch,
    TeamResponse, TeamRole, TransferCaptaincyRequest,
};
use services::rate_limit_service;
use services::team_service::{self, TeamError, TransferBy};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/teams", get(list_teams).post(create_team))
        .route(
            "/teams/{slug}",
            get(get_team).patch(update_team).delete(delete_team),
        )
        .route("/teams/{slug}/members", get(list_members))
        .route("/teams/{slug}/transfer", post(transfer_captaincy))
        .route(
            "/teams/{slug}/members/{user_id}",
            patch(change_member_role).delete(remove_member),
        )
        .route(
            "/teams/{slug}/invitations",
            get(list_invitations).post(create_invitation),
        )
        .route("/teams/{slug}/invitations/{id}", delete(revoke_invitation))
        .route("/teams/join", post(accept_invitation_by_token))
        .route("/teams/invitations/pending", get(list_pending_invitations))
        .route("/teams/invitations/preview", post(preview_invitation))
        .route(
            "/teams/invitations/{id}/accept",
            post(accept_invitation_by_id),
        )
        .route(
            "/teams/invitations/{id}/reject",
            post(reject_invitation_by_id),
        )
}

#[utoipa::path(
    get,
    path = "/api/v1/teams",
    tag = "Teams",
    security(("access_token" = [])),
    responses(
        (status = 200, description = "Teams the user belongs to", body = [TeamResponse]),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
    )
)]
async fn list_teams(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<TeamResponse>>, AppError> {
    let teams = team_service::list_teams_for_user(&state.db, user.id).await?;
    Ok(Json(teams))
}

#[utoipa::path(
    post,
    path = "/api/v1/teams",
    tag = "Teams",
    request_body = CreateTeamRequest,
    security(("access_token" = [])),
    responses(
        (status = 201, description = "Team created", body = TeamResponse),
        (status = 400, description = "malformed_json or invalid_body: the body is not JSON, or could not be read", body = SimpleErrorResponse),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 415, description = "unsupported_media_type: Content-Type is not application/json", body = SimpleErrorResponse),
        (status = 422, description = "validation_failed, invalid_body, team_name_not_sluggable, team_name_reserved or team_quota_reached", body = UnprocessableErrorResponse),
    )
)]
async fn create_team(
    State(state): State<AppState>,
    user: AuthUser,
    JsonBody(payload): JsonBody<CreateTeamRequest>,
) -> Result<(StatusCode, Json<TeamResponse>), AppError> {
    payload.validate()?;
    let team = team_service::create_team(&state.db, user.id, payload).await?;
    Ok((StatusCode::CREATED, Json(team)))
}

#[utoipa::path(
    get,
    path = "/api/v1/teams/{slug}",
    tag = "Teams",
    security(("access_token" = [])),
    responses(
        (status = 200, description = "Team detail", body = TeamResponse),
        (status = 404, description = "Team not found or not a member", body = SimpleErrorResponse),
    )
)]
async fn get_team(
    State(state): State<AppState>,
    user: AuthUser,
    Path(slug): Path<String>,
) -> Result<Json<TeamResponse>, AppError> {
    let (team, my_role) = team_service::get_team_for_member(&state.db, &slug, user.id).await?;
    let count = team_service::member_count(&state.db, team.id).await?;

    Ok(Json(TeamResponse::from_model(team, my_role, count)))
}

#[utoipa::path(
    patch,
    path = "/api/v1/teams/{slug}",
    tag = "Teams",
    request_body = TeamPatch,
    security(("access_token" = [])),
    responses(
        (status = 200, description = "Team updated", body = TeamResponse),
        (status = 400, description = "malformed_json or invalid_body: the body is not JSON, or could not be read", body = SimpleErrorResponse),
        (status = 403, description = "insufficient_role: below Admin, whichever field is sent, members_can_create_workspaces included", body = SimpleErrorResponse),
        (status = 404, description = "Team not found or not a member", body = SimpleErrorResponse),
        (status = 415, description = "unsupported_media_type: Content-Type is not application/json", body = SimpleErrorResponse),
        (status = 422, description = "validation_failed (fields.name, or fields.description beyond 500 characters once trimmed) or invalid_body", body = UnprocessableErrorResponse),
    )
)]
async fn update_team(
    State(state): State<AppState>,
    user: AuthUser,
    Path(slug): Path<String>,
    JsonBody(patch): JsonBody<TeamPatch>,
) -> Result<Json<TeamResponse>, AppError> {
    patch.validate()?;

    let (team, my_role) = team_service::get_team_for_member(&state.db, &slug, user.id).await?;
    team_service::require_role(my_role, TeamRole::Admin)?;

    let updated = team_service::update_team(&state.db, team.id, patch).await?;
    let count = team_service::member_count(&state.db, updated.id).await?;

    Ok(Json(TeamResponse::from_model(updated, my_role, count)))
}

#[utoipa::path(
    delete,
    path = "/api/v1/teams/{slug}",
    tag = "Teams",
    security(("access_token" = [])),
    responses(
        (status = 204, description = "Team deleted"),
        (status = 403, description = "Captain only", body = SimpleErrorResponse),
        (status = 404, description = "Team not found or not a member", body = SimpleErrorResponse),
    )
)]
async fn delete_team(
    State(state): State<AppState>,
    user: AuthUser,
    Path(slug): Path<String>,
) -> Result<StatusCode, AppError> {
    let (team, my_role) = team_service::get_team_for_member(&state.db, &slug, user.id).await?;
    team_service::require_role(my_role, TeamRole::Captain)?;

    team_service::delete_team(&state.db, &state.storage, team.id).await?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/teams/{slug}/members",
    tag = "Teams",
    operation_id = "list_team_members",
    params(
        ("slug" = String, Path, description = "Team slug"),
        ("invited_by" = Option<Uuid>, Query, description = "Only the members who joined by an invitation from this account"),
    ),
    security(("access_token" = [])),
    responses(
        (status = 200, description = "Team members, each with the author of the invitation they joined by (`invited_by`, null when none is recorded)", body = [TeamMemberResponse]),
        (status = 404, description = "Team not found or not a member", body = SimpleErrorResponse),
        (status = 422, description = "validation_failed: invited_by is not a uuid", body = UnprocessableErrorResponse),
    )
)]
async fn list_members(
    State(state): State<AppState>,
    user: AuthUser,
    Path(slug): Path<String>,
    Query(filter): Query<InvitedByFilter>,
) -> Result<Json<Vec<TeamMemberResponse>>, AppError> {
    let invited_by = filter.parse()?;
    let (team, _my_role) = team_service::get_team_for_member(&state.db, &slug, user.id).await?;
    let mut members = team_service::list_team_members(&state.db, team.id).await?;
    if let Some(author) = invited_by {
        members.retain(|member| member.invited_by == Some(author));
    }
    Ok(Json(members))
}

#[utoipa::path(
    patch,
    path = "/api/v1/teams/{slug}/members/{user_id}",
    tag = "Teams",
    request_body = ChangeMemberRoleRequest,
    security(("access_token" = [])),
    responses(
        (status = 200, description = "Member role updated", body = TeamMemberResponse),
        (status = 400, description = "malformed_json or invalid_body: the body is not JSON, or could not be read", body = SimpleErrorResponse),
        (status = 403, description = "Insufficient role, or an Admin touching the Captain role", body = SimpleErrorResponse),
        (status = 404, description = "Team, target member not found, or caller not a member", body = SimpleErrorResponse),
        (status = 409, description = "captain_must_transfer: the Captain role is named or given up only through POST /api/v1/teams/{slug}/transfer", body = SimpleErrorResponse),
        (status = 415, description = "unsupported_media_type: Content-Type is not application/json", body = SimpleErrorResponse),
        (status = 422, description = "invalid_body", body = UnprocessableErrorResponse),
    )
)]
async fn change_member_role(
    State(state): State<AppState>,
    user: AuthUser,
    Path((slug, target_user_id)): Path<(String, Uuid)>,
    JsonBody(payload): JsonBody<ChangeMemberRoleRequest>,
) -> Result<Json<TeamMemberResponse>, AppError> {
    let (team, my_role) = team_service::get_team_for_member(&state.db, &slug, user.id).await?;
    team_service::require_role(my_role, TeamRole::Admin)?;

    let target_role = team_service::get_user_role_in_team(&state.db, team.id, target_user_id)
        .await?
        .ok_or(TeamError::MemberNotFound)?;

    if !team_service::can_manage_role(my_role, target_role, payload.role) {
        return Err(TeamError::InsufficientRole.into());
    }

    team_service::change_member_role(&state.db, team.id, target_user_id, payload.role).await?;
    let member = team_service::get_team_member(&state.db, team.id, target_user_id).await?;

    Ok(Json(member))
}

#[utoipa::path(
    delete,
    path = "/api/v1/teams/{slug}/members/{user_id}",
    tag = "Teams",
    security(("access_token" = [])),
    responses(
        (status = 204, description = "Member removed"),
        (status = 403, description = "Insufficient role, or an Admin removing the Captain", body = SimpleErrorResponse),
        (status = 404, description = "Team, target member not found, or caller not a member", body = SimpleErrorResponse),
        (status = 409, description = "The Captain cannot leave. captain_must_transfer: the team has other members, transfer the role first. captain_must_delete_team: the Captain is the only member, delete the team instead", body = SimpleErrorResponse),
    )
)]
async fn remove_member(
    State(state): State<AppState>,
    user: AuthUser,
    Path((slug, target_user_id)): Path<(String, Uuid)>,
) -> Result<StatusCode, AppError> {
    let (team, my_role) = team_service::get_team_for_member(&state.db, &slug, user.id).await?;

    let target_role = team_service::get_user_role_in_team(&state.db, team.id, target_user_id)
        .await?
        .ok_or(TeamError::MemberNotFound)?;

    if !team_service::can_remove_member(my_role, user.id, target_user_id, target_role) {
        return Err(TeamError::InsufficientRole.into());
    }

    team_service::remove_member(&state.db, team.id, target_user_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/teams/{slug}/transfer",
    tag = "Teams",
    request_body = TransferCaptaincyRequest,
    security(("access_token" = [])),
    responses(
        (status = 204, description = "The member is Captain, and the previous Captain an Admin"),
        (status = 400, description = "malformed_json or invalid_body: the body is not JSON, or could not be read", body = SimpleErrorResponse),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 403, description = "insufficient_role: Captain only", body = SimpleErrorResponse),
        (status = 404, description = "team_not_found: team not found or caller not a member. member_not_found: the target is not a member of the team", body = SimpleErrorResponse),
        (status = 409, description = "target_already_captain, or target_pending_deletion: the target's account is scheduled for deletion", body = SimpleErrorResponse),
        (status = 415, description = "unsupported_media_type: Content-Type is not application/json", body = SimpleErrorResponse),
        (status = 422, description = "invalid_body", body = UnprocessableErrorResponse),
    )
)]
async fn transfer_captaincy(
    State(state): State<AppState>,
    user: AuthUser,
    Path(slug): Path<String>,
    JsonBody(payload): JsonBody<TransferCaptaincyRequest>,
) -> Result<StatusCode, AppError> {
    let (team, my_role) = team_service::get_team_for_member(&state.db, &slug, user.id).await?;
    team_service::require_role(my_role, TeamRole::Captain)?;

    team_service::transfer_captaincy(
        &state.db,
        team.id,
        payload.user_id,
        TransferBy::Captain(user.id),
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/teams/{slug}/invitations",
    tag = "Teams",
    operation_id = "list_team_invitations",
    params(
        ("slug" = String, Path, description = "Team slug"),
        ("invited_by" = Option<Uuid>, Query, description = "Only the invitations this account created, whatever they name"),
    ),
    security(("access_token" = [])),
    responses(
        (status = 200, description = "Every pending invitation of the team, oldest first: by username, by address, and generic links (`target`), each with its invitee (`invited_username` or `invited_email`), its author, its creation and its expiry", body = [TeamInvitationResponse]),
        (status = 403, description = "Insufficient role", body = SimpleErrorResponse),
        (status = 404, description = "Team not found or not a member", body = SimpleErrorResponse),
        (status = 422, description = "validation_failed: invited_by is not a uuid", body = UnprocessableErrorResponse),
    )
)]
async fn list_invitations(
    State(state): State<AppState>,
    user: AuthUser,
    Path(slug): Path<String>,
    Query(filter): Query<InvitedByFilter>,
) -> Result<Json<Vec<TeamInvitationResponse>>, AppError> {
    let invited_by = filter.parse()?;
    let (team, my_role) = team_service::get_team_for_member(&state.db, &slug, user.id).await?;
    team_service::require_role(my_role, TeamRole::Admin)?;

    let mut invitations = team_service::list_active_invitations(&state.db, team.id).await?;
    if let Some(author) = invited_by {
        invitations.retain(|invitation| invitation.invited_by == Some(author));
    }
    Ok(Json(invitations))
}

#[utoipa::path(
    post,
    path = "/api/v1/teams/{slug}/invitations",
    tag = "Teams",
    operation_id = "create_team_invitation",
    request_body = CreateInvitationRequest,
    security(("access_token" = [])),
    responses(
        (status = 201, description = "Invitation created. A generic link is answered with its `token` (TeamLinkResponse); a targeted invitation has no token field at all (TeamInvitationResponse), since its token goes to the invitee's mail alone. By address, the answer is the same whether an account holds it or not (`invited_user_id` is null), and the mail goes out either way, at most three a day to one address across every team and workspace; past that the invitation is still created, without a mail", body = CreatedTeamInvitation),
        (status = 400, description = "malformed_json or invalid_body: the body is not JSON, or could not be read", body = SimpleErrorResponse),
        (status = 403, description = "Insufficient role, or a non-Captain tried to invite as Captain", body = SimpleErrorResponse),
        (status = 404, description = "Team not found or not a member. invited_user_not_found: no account has this username", body = SimpleErrorResponse),
        (status = 409, description = "already_invited, or already_member: the account, or a member holding the address verified, is on the team. captain_must_transfer: an invitation cannot grant Captain, the role moves only by transfer", body = SimpleErrorResponse),
        (status = 415, description = "unsupported_media_type: Content-Type is not application/json", body = SimpleErrorResponse),
        (status = 422, description = "validation_failed, invalid_body, or invitation_expiry_too_far: expires_at more than 30 days out (7 days when absent); an address no mail can go to (a..b@example.test, a@[127.0.0.1]) is validation_failed, as a malformed one", body = UnprocessableErrorResponse),
        (status = 429, description = "rate_limited: by username 20 an hour per author and 50 per team, by address 10 and 30. Generic links are not limited", body = RateLimitedErrorResponse),
        (status = 503, description = "service_unavailable: the rate limiter cannot be reached", body = SimpleErrorResponse),
    )
)]
async fn create_invitation(
    State(state): State<AppState>,
    user: AuthUser,
    Path(slug): Path<String>,
    JsonBody(payload): JsonBody<CreateInvitationRequest>,
) -> Result<(StatusCode, Json<CreatedTeamInvitation>), AppError> {
    payload.validate()?;
    if let Some(address) = &payload.invited_email {
        require_mailbox("invited_email", address)?;
    }

    let (team, my_role) = team_service::get_team_for_member(&state.db, &slug, user.id).await?;
    team_service::require_role(my_role, TeamRole::Admin)?;

    // An Admin asking for a Captain invitation is refused as before, same
    // reasoning as `can_manage_role`. A Captain asking is refused by the
    // service: nobody joins as Captain, the role moves only by transfer.
    if payload.default_role == TeamRole::Captain && my_role != TeamRole::Captain {
        return Err(TeamError::InsufficientRole.into());
    }

    // After authorization, so nobody spends a team's budget without being an
    // Admin of it; before the service, so a refused request creates nothing.
    // Every attempt counts, refused ones included. A generic link mails
    // nobody, so it is not limited.
    let mut redis = state.redis.clone();
    if payload.username.is_some() {
        enforce_invitation_limit(
            rate_limit_service::check_team_invitation_by_username(&mut redis, user.id, team.id)
                .await,
        )?;
    } else if payload.invited_email.is_some() {
        enforce_invitation_limit(
            rate_limit_service::check_team_invitation_by_email(&mut redis, user.id, team.id).await,
        )?;
    }

    let created = team_service::create_invitation(
        &state.db,
        &mut redis,
        &state.config,
        team.id,
        user.id,
        payload,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(created.invitation)))
}

#[utoipa::path(
    delete,
    path = "/api/v1/teams/{slug}/invitations/{id}",
    tag = "Teams",
    security(("access_token" = [])),
    responses(
        (status = 204, description = "Invitation revoked"),
        (status = 403, description = "Insufficient role", body = SimpleErrorResponse),
        (status = 404, description = "Team, invitation not found, or caller not a member", body = SimpleErrorResponse),
    )
)]
async fn revoke_invitation(
    State(state): State<AppState>,
    user: AuthUser,
    Path((slug, invitation_id)): Path<(String, Uuid)>,
) -> Result<StatusCode, AppError> {
    let (team, my_role) = team_service::get_team_for_member(&state.db, &slug, user.id).await?;
    team_service::require_role(my_role, TeamRole::Admin)?;

    team_service::revoke_invitation(&state.db, team.id, invitation_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/teams/join",
    tag = "Teams",
    operation_id = "accept_team_invitation_by_token",
    request_body = JoinTeamRequest,
    security(("access_token" = [])),
    responses(
        (status = 200, description = "Joined the team", body = TeamResponse),
        (status = 400, description = "malformed_json or invalid_body: the body is not JSON, or could not be read", body = SimpleErrorResponse),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 404, description = "invitation_not_found: one body for an unknown token and for an invitation addressed to another account (or to an address the caller has not verified), expired or not", body = SimpleErrorResponse),
        (status = 409, description = "already_member", body = SimpleErrorResponse),
        (status = 415, description = "unsupported_media_type: Content-Type is not application/json", body = SimpleErrorResponse),
        (status = 422, description = "validation_failed, invalid_body, or, to the invitee or the holder of a generic link, invitation_expired or invitation_max_uses_reached", body = UnprocessableErrorResponse),
    )
)]
async fn accept_invitation_by_token(
    State(state): State<AppState>,
    user: AuthUser,
    JsonBody(payload): JsonBody<JoinTeamRequest>,
) -> Result<Json<TeamResponse>, AppError> {
    payload.validate()?;

    let team = team_service::accept_invitation_by_token(&state.db, &payload.token, user.id).await?;
    Ok(Json(team))
}

#[utoipa::path(
    get,
    path = "/api/v1/teams/invitations/pending",
    tag = "Teams",
    operation_id = "list_pending_team_invitations",
    security(("access_token" = [])),
    responses(
        (status = 200, description = "Pending invitations for the current user: those naming the account, and those sent to an address it has verified. None is ever accepted on its behalf", body = [TeamInvitationResponse]),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
    )
)]
async fn list_pending_invitations(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<TeamInvitationResponse>>, AppError> {
    let invitations = team_service::list_pending_invitations_for_user(&state.db, user.id).await?;
    Ok(Json(invitations))
}

#[utoipa::path(
    post,
    path = "/api/v1/teams/invitations/preview",
    tag = "Teams",
    operation_id = "preview_team_invitation",
    request_body = InvitationTokenRequest,
    security(("access_token" = [])),
    responses(
        (status = 200, description = "What the invitation behind a link offers: the team's name, the author, the role and the expiry", body = TeamInvitationPreview),
        (status = 400, description = "malformed_json or invalid_body: the body is not JSON, or could not be read", body = SimpleErrorResponse),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 404, description = "invitation_not_found: the same for an unknown token, an expired or used-up invitation, and one addressed to another account", body = SimpleErrorResponse),
        (status = 415, description = "unsupported_media_type: Content-Type is not application/json", body = SimpleErrorResponse),
        (status = 422, description = "validation_failed or invalid_body", body = UnprocessableErrorResponse),
    )
)]
async fn preview_invitation(
    State(state): State<AppState>,
    user: AuthUser,
    JsonBody(payload): JsonBody<InvitationTokenRequest>,
) -> Result<Json<TeamInvitationPreview>, AppError> {
    payload.validate()?;

    let preview = team_service::preview_invitation(&state.db, &payload.token, user.id).await?;
    Ok(Json(preview))
}

#[utoipa::path(
    post,
    path = "/api/v1/teams/invitations/{id}/accept",
    tag = "Teams",
    operation_id = "accept_team_invitation_by_id",
    security(("access_token" = [])),
    responses(
        (status = 200, description = "Invitation accepted", body = TeamResponse),
        (status = 404, description = "invitation_not_found: one body for an unknown id and for an invitation addressed to another account (or to an address the caller has not verified), expired or not. A generic link is not reached by id", body = SimpleErrorResponse),
        (status = 409, description = "already_member", body = SimpleErrorResponse),
        (status = 422, description = "invitation_expired, to the invitee alone", body = UnprocessableErrorResponse),
    )
)]
async fn accept_invitation_by_id(
    State(state): State<AppState>,
    user: AuthUser,
    Path(invitation_id): Path<Uuid>,
) -> Result<Json<TeamResponse>, AppError> {
    let team = team_service::accept_invitation_by_id(&state.db, invitation_id, user.id).await?;
    Ok(Json(team))
}

#[utoipa::path(
    post,
    path = "/api/v1/teams/invitations/{id}/reject",
    tag = "Teams",
    operation_id = "reject_team_invitation_by_id",
    security(("access_token" = [])),
    responses(
        (status = 204, description = "Invitation rejected"),
        (status = 404, description = "invitation_not_found: one body for an unknown id and for an invitation addressed to another account (or to an address the caller has not verified), expired or not. A generic link is not reached by id", body = SimpleErrorResponse),
    )
)]
async fn reject_invitation_by_id(
    State(state): State<AppState>,
    user: AuthUser,
    Path(invitation_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    team_service::reject_invitation_by_id(&state.db, invitation_id, user.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use crate::test_support::{
        add_team_member, app, avatar_status, avatar_stored, create_team, create_team_workspace,
        create_user, db, request_as, upload_avatar,
    };
    use axum::http::StatusCode;
    use domain::dto::team::TeamRole;
    use domain::entities::{team_members, teams, users};
    use sea_orm::sea_query::Expr;
    use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
    use serde_json::json;
    use services::storage_service::AvatarKind;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use uuid::Uuid;

    async fn slug_of(db: &DatabaseConnection, team_id: Uuid) -> String {
        teams::Entity::find_by_id(team_id)
            .one(db)
            .await
            .expect("the lookup must succeed")
            .expect("the team must exist")
            .slug
    }

    /// The member's role, `None` once they are off the team.
    async fn role_of(db: &DatabaseConnection, team_id: Uuid, user_id: Uuid) -> Option<String> {
        team_members::Entity::find_by_id((team_id, user_id))
            .one(db)
            .await
            .expect("the lookup must succeed")
            .map(|member| member.role)
    }

    /// A team with `captain` as Captain and one member per role given, in
    /// that order, and the members' ids.
    async fn team_with(
        db: &DatabaseConnection,
        captain: Uuid,
        roles: &[TeamRole],
    ) -> (Uuid, Vec<Uuid>) {
        let team_id = create_team(db, captain).await;
        let mut members = Vec::new();
        for role in roles {
            let member = create_user(db).await;
            add_team_member(db, team_id, member, *role).await;
            members.push(member);
        }
        (team_id, members)
    }

    async fn transfer(
        app: &axum::Router,
        db: &DatabaseConnection,
        team_id: Uuid,
        caller: Uuid,
        target: Uuid,
    ) -> (StatusCode, serde_json::Value) {
        request_as(
            app,
            "POST",
            &format!("/api/v1/teams/{}/transfer", slug_of(db, team_id).await),
            Some(caller),
            Some(json!({ "user_id": target })),
        )
        .await
    }

    /// The Captain hands the role over: the target is Captain, and the
    /// Captain an Admin.
    #[tokio::test]
    async fn a_transfer_makes_the_target_captain_and_the_captain_an_admin() {
        let db = db().await;
        let app = app(&db).await;
        let captain = create_user(&db).await;
        let (team_id, members) = team_with(&db, captain, &[TeamRole::Member]).await;

        let (status, _) = transfer(&app, &db, team_id, captain, members[0]).await;

        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(
            role_of(&db, team_id, members[0]).await.as_deref(),
            Some("captain")
        );
        assert_eq!(
            role_of(&db, team_id, captain).await.as_deref(),
            Some("admin")
        );
    }

    /// Captain only: a member of any other role knows the team and gets a
    /// 403, someone outside it a 404. Nothing changes either way.
    #[tokio::test]
    async fn a_transfer_by_anyone_but_the_captain_is_refused() {
        let db = db().await;
        let app = app(&db).await;
        let captain = create_user(&db).await;
        let (team_id, members) =
            team_with(&db, captain, &[TeamRole::Admin, TeamRole::Member]).await;
        let outsider = create_user(&db).await;

        for caller in [members[0], members[1]] {
            let (status, body) = transfer(&app, &db, team_id, caller, caller).await;
            assert_eq!(status, StatusCode::FORBIDDEN);
            assert_eq!(body["error"], "insufficient_role");
        }

        let (status, body) = transfer(&app, &db, team_id, outsider, members[0]).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"], "team_not_found");

        assert_eq!(
            role_of(&db, team_id, captain).await.as_deref(),
            Some("captain")
        );
        assert_eq!(
            role_of(&db, team_id, members[0]).await.as_deref(),
            Some("admin")
        );
    }

    /// The target has to be on the team, and not be its Captain already.
    #[tokio::test]
    async fn a_transfer_to_a_non_member_or_to_the_captain_is_refused() {
        let db = db().await;
        let app = app(&db).await;
        let captain = create_user(&db).await;
        let (team_id, _) = team_with(&db, captain, &[TeamRole::Member]).await;
        let outsider = create_user(&db).await;

        let (status, body) = transfer(&app, &db, team_id, captain, outsider).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"], "member_not_found");

        let (status, body) = transfer(&app, &db, team_id, captain, captain).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"], "target_already_captain");

        assert_eq!(
            role_of(&db, team_id, captain).await.as_deref(),
            Some("captain")
        );
        assert_eq!(role_of(&db, team_id, outsider).await, None);
    }

    /// An account scheduled for deletion cannot receive the role: its own
    /// request would have been refused had it been Captain then.
    #[tokio::test]
    async fn a_transfer_to_an_account_pending_deletion_is_refused() {
        let db = db().await;
        let app = app(&db).await;
        let captain = create_user(&db).await;
        let (team_id, members) = team_with(&db, captain, &[TeamRole::Admin]).await;
        users::Entity::update_many()
            .col_expr(
                users::Column::PurgeScheduledAt,
                Expr::value(chrono::Utc::now().fixed_offset() + chrono::Duration::days(30)),
            )
            .filter(users::Column::Id.eq(members[0]))
            .exec(&db)
            .await
            .expect("the deletion must be scheduled");

        let (status, body) = transfer(&app, &db, team_id, captain, members[0]).await;

        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"], "target_pending_deletion");
        assert_eq!(
            role_of(&db, team_id, captain).await.as_deref(),
            Some("captain")
        );
        assert_eq!(
            role_of(&db, team_id, members[0]).await.as_deref(),
            Some("admin")
        );
    }

    /// A role change neither names a Captain nor demotes the one there is,
    /// whoever asks: the Captain is told to transfer, an Admin is refused
    /// outright as before.
    #[tokio::test]
    async fn a_role_change_can_neither_name_nor_demote_the_captain() {
        let db = db().await;
        let app = app(&db).await;
        let captain = create_user(&db).await;
        let (team_id, members) =
            team_with(&db, captain, &[TeamRole::Admin, TeamRole::Member]).await;
        let slug = slug_of(&db, team_id).await;
        let patch = |target: Uuid, role: &str| {
            (
                format!("/api/v1/teams/{slug}/members/{target}"),
                Some(json!({ "role": role })),
            )
        };

        for (caller, target, role, expected, code) in [
            (
                captain,
                members[1],
                "captain",
                StatusCode::CONFLICT,
                "captain_must_transfer",
            ),
            (
                captain,
                captain,
                "admin",
                StatusCode::CONFLICT,
                "captain_must_transfer",
            ),
            (
                members[0],
                captain,
                "member",
                StatusCode::FORBIDDEN,
                "insufficient_role",
            ),
            (
                members[0],
                members[1],
                "captain",
                StatusCode::FORBIDDEN,
                "insufficient_role",
            ),
        ] {
            let (uri, body) = patch(target, role);
            let (status, answer) = request_as(&app, "PATCH", &uri, Some(caller), body).await;
            assert_eq!(status, expected, "{role} for {target}");
            assert_eq!(answer["error"], code, "{role} for {target}");
        }

        assert_eq!(
            role_of(&db, team_id, captain).await.as_deref(),
            Some("captain")
        );
        assert_eq!(
            role_of(&db, team_id, members[0]).await.as_deref(),
            Some("admin")
        );
        assert_eq!(
            role_of(&db, team_id, members[1]).await.as_deref(),
            Some("member")
        );
    }

    /// An Admin cannot remove the Captain from the team.
    #[tokio::test]
    async fn an_admin_cannot_remove_the_captain() {
        let db = db().await;
        let app = app(&db).await;
        let captain = create_user(&db).await;
        let (team_id, members) = team_with(&db, captain, &[TeamRole::Admin]).await;

        let (status, body) = request_as(
            &app,
            "DELETE",
            &format!(
                "/api/v1/teams/{}/members/{captain}",
                slug_of(&db, team_id).await
            ),
            Some(members[0]),
            None,
        )
        .await;

        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["error"], "insufficient_role");
        assert_eq!(
            role_of(&db, team_id, captain).await.as_deref(),
            Some("captain")
        );
    }

    /// The Captain cannot leave: with others on the team they transfer
    /// first, alone they delete the team instead of leaving it empty.
    #[tokio::test]
    async fn the_captain_cannot_leave() {
        let db = db().await;
        let app = app(&db).await;
        let captain = create_user(&db).await;
        let (shared, _) = team_with(&db, captain, &[TeamRole::Member]).await;
        let (alone, _) = team_with(&db, captain, &[]).await;

        for (team_id, code) in [
            (shared, "captain_must_transfer"),
            (alone, "captain_must_delete_team"),
        ] {
            let (status, body) = request_as(
                &app,
                "DELETE",
                &format!(
                    "/api/v1/teams/{}/members/{captain}",
                    slug_of(&db, team_id).await
                ),
                Some(captain),
                None,
            )
            .await;

            assert_eq!(status, StatusCode::CONFLICT);
            assert_eq!(body["error"], code);
            assert_eq!(
                role_of(&db, team_id, captain).await.as_deref(),
                Some("captain")
            );
        }
    }

    /// Joining never makes a Captain, so the Captain cannot mint an
    /// invitation that would.
    #[tokio::test]
    async fn an_invitation_cannot_grant_captain() {
        let db = db().await;
        let app = app(&db).await;
        let captain = create_user(&db).await;
        let (team_id, _) = team_with(&db, captain, &[]).await;

        let (status, body) = request_as(
            &app,
            "POST",
            &format!("/api/v1/teams/{}/invitations", slug_of(&db, team_id).await),
            Some(captain),
            Some(json!({ "default_role": "captain" })),
        )
        .await;

        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"], "captain_must_transfer");
    }

    /// The two settings as stored.
    async fn settings_of(db: &DatabaseConnection, team_id: Uuid) -> (Option<String>, bool) {
        let team = teams::Entity::find_by_id(team_id)
            .one(db)
            .await
            .expect("the lookup must succeed")
            .expect("the team must exist");
        (team.description, team.members_can_create_workspaces)
    }

    /// An Admin sets the description trimmed, clears it with null or with a
    /// blank string, and is refused past 500 characters with the field named
    /// and the stored value left alone.
    #[tokio::test]
    async fn patching_the_description_sets_clears_and_bounds_it() {
        let db = db().await;
        let app = app(&db).await;
        let captain = create_user(&db).await;
        let (team_id, members) = team_with(&db, captain, &[TeamRole::Admin]).await;
        let uri = format!("/api/v1/teams/{}", slug_of(&db, team_id).await);
        let patch = |description: serde_json::Value| {
            request_as(
                &app,
                "PATCH",
                &uri,
                Some(members[0]),
                Some(json!({ "description": description })),
            )
        };

        let (status, body) = patch(json!("  Red team, Thursdays  ")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["description"], "Red team, Thursdays");
        assert_eq!(
            settings_of(&db, team_id).await.0.as_deref(),
            Some("Red team, Thursdays")
        );

        let (status, body) = patch(json!(null)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["description"], json!(null));
        assert_eq!(settings_of(&db, team_id).await.0, None);

        assert_eq!(patch(json!("Red team")).await.0, StatusCode::OK);
        let (status, body) = patch(json!("")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["description"], json!(null));
        assert_eq!(
            settings_of(&db, team_id).await.0,
            None,
            "an empty string is stored as NULL, not as blank text"
        );

        assert_eq!(patch(json!("Red team")).await.0, StatusCode::OK);
        let (status, body) = patch(json!("a".repeat(501))).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["error"], "validation_failed");
        assert!(
            body["fields"]["description"].is_array(),
            "the refusal must name the field, got {body}"
        );
        assert_eq!(
            settings_of(&db, team_id).await.0.as_deref(),
            Some("Red team")
        );
    }

    /// Who may create workspaces is changed like the rest of the team: an
    /// Admin at least. A Member knows the team and gets a 403, someone
    /// outside it a 404, and the setting stays off for both.
    #[tokio::test]
    async fn only_an_admin_changes_whether_members_create_workspaces() {
        let db = db().await;
        let app = app(&db).await;
        let captain = create_user(&db).await;
        let (team_id, members) =
            team_with(&db, captain, &[TeamRole::Admin, TeamRole::Member]).await;
        let outsider = create_user(&db).await;
        let uri = format!("/api/v1/teams/{}", slug_of(&db, team_id).await);
        let body = Some(json!({ "members_can_create_workspaces": true }));

        let (status, answer) =
            request_as(&app, "PATCH", &uri, Some(members[1]), body.clone()).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(answer["error"], "insufficient_role");

        let (status, answer) = request_as(&app, "PATCH", &uri, Some(outsider), body.clone()).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(answer["error"], "team_not_found");

        assert!(!settings_of(&db, team_id).await.1, "nothing changed yet");

        let (status, answer) = request_as(&app, "PATCH", &uri, Some(members[0]), body).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(answer["members_can_create_workspaces"], true);
        assert!(settings_of(&db, team_id).await.1);
    }

    /// Deleting a team takes its avatar out of the bucket, and those of every
    /// workspace the cascade deletes with it.
    #[tokio::test]
    async fn deleting_a_team_purges_its_avatar_and_its_workspaces_avatars() {
        let db = db().await;
        let app = app(&db).await;
        let captain = create_user(&db).await;
        let team_id = create_team(&db, captain).await;
        let first = create_team_workspace(&db, team_id, "First").await;
        let second = create_team_workspace(&db, team_id, "Second").await;

        let mut avatars = Vec::new();
        for (kind, id) in [
            (AvatarKind::Team, team_id),
            (AvatarKind::Workspace, first.id),
            (AvatarKind::Workspace, second.id),
        ] {
            let version = upload_avatar(&db, kind, id).await;
            assert!(
                avatar_stored(kind, id, version).await,
                "control: the {kind:?} avatar is in the bucket"
            );
            avatars.push((kind, id, version));
        }

        let slug = teams::Entity::find_by_id(team_id)
            .one(&db)
            .await
            .expect("the lookup must succeed")
            .expect("the team must exist")
            .slug;
        let (status, _) = request_as(
            &app,
            "DELETE",
            &format!("/api/v1/teams/{slug}"),
            Some(captain),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);

        for (kind, id, version) in avatars {
            assert!(
                !avatar_stored(kind, id, version).await,
                "the {kind:?} avatar of {id} must be purged"
            );
        }
    }

    // ---- Invitations ----------------------------------------------------

    fn fresh_address() -> String {
        format!("{}@example.test", crate::test_support::unique("teaminv"))
    }

    async fn verified_address(db: &DatabaseConnection, user: Uuid) -> String {
        let address = fresh_address();
        add_address(db, user, &address, true).await;
        address
    }

    async fn add_address(db: &DatabaseConnection, user: Uuid, address: &str, verified: bool) {
        domain::entities::user_emails::ActiveModel {
            id: sea_orm::Set(Uuid::now_v7()),
            user_id: sea_orm::Set(user),
            email: sea_orm::Set(address.to_owned()),
            is_primary: sea_orm::Set(true),
            verified_at: sea_orm::Set(verified.then(|| chrono::Utc::now().into())),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("the fixture address must insert");
    }

    /// Gives an invitation a token the test knows, the one its mail would
    /// carry: only the hash is stored, so nothing else holds it.
    async fn known_token(db: &DatabaseConnection, invitation: &serde_json::Value) -> String {
        let token = services::token_service::generate_opaque_token();
        domain::entities::team_invitations::ActiveModel {
            id: sea_orm::Set(serde_json::from_value(invitation["id"].clone()).expect("an id")),
            token: sea_orm::Set(token.hash),
            ..Default::default()
        }
        .update(db)
        .await
        .expect("the token must be set");
        token.secret
    }

    async fn post(
        app: &axum::Router,
        path: &str,
        caller: Uuid,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value) {
        request_as(app, "POST", path, Some(caller), body).await
    }

    /// Whoever is not the invitee gets, on every route that answers an
    /// invitation, the very answer an unknown id or token gets: an invitation
    /// by username or by address, live or expired, and a generic link
    /// reached by id. None of it is used up, and the invitee still accepts.
    #[tokio::test]
    async fn anyone_but_the_invitee_is_answered_as_for_an_unknown_invitation() {
        let db = db().await;
        let app = app(&db).await;
        let captain = create_user(&db).await;
        let (team_id, _) = team_with(&db, captain, &[]).await;
        let invitee = create_user(&db).await;
        let late = create_user(&db).await;
        let stranger = create_user(&db).await;
        let unverified = create_user(&db).await;
        let address = fresh_address();
        add_address(&db, unverified, &address, false).await;

        let mut created = Vec::new();
        for body in [
            json!({ "default_role": "member", "username": username_of(&db, invitee).await }),
            json!({ "default_role": "member", "invited_email": address }),
            json!({ "default_role": "member", "username": username_of(&db, late).await }),
            json!({ "default_role": "member" }),
        ] {
            let (status, answer) = invite(&app, &db, team_id, captain, body).await;
            assert_eq!(status, StatusCode::CREATED, "{answer}");
            created.push(answer);
        }
        let [by_name, by_address, expired, link] =
            <[serde_json::Value; 4]>::try_from(created).expect("four invitations");
        domain::entities::team_invitations::ActiveModel {
            id: sea_orm::Set(serde_json::from_value(expired["id"].clone()).expect("an id")),
            expires_at: sea_orm::Set((chrono::Utc::now() - chrono::Duration::minutes(1)).into()),
            ..Default::default()
        }
        .update(&db)
        .await
        .expect("the fixture expires");
        let name_token = known_token(&db, &by_name).await;
        let address_token = known_token(&db, &by_address).await;
        let expired_token = known_token(&db, &expired).await;

        for action in ["accept", "reject"] {
            let unknown = post(
                &app,
                &format!("/api/v1/teams/invitations/{}/{action}", Uuid::now_v7()),
                stranger,
                None,
            )
            .await;
            assert_eq!(unknown.0, StatusCode::NOT_FOUND);
            for (caller, invitation) in [
                (stranger, &by_name),
                (stranger, &by_address),
                (unverified, &by_address),
                (stranger, &expired),
                (stranger, &link),
                (invitee, &link),
            ] {
                let path = format!(
                    "/api/v1/teams/invitations/{}/{action}",
                    invitation["id"].as_str().expect("an id")
                );
                assert_eq!(post(&app, &path, caller, None).await, unknown, "{path}");
            }
        }

        for path in ["/api/v1/teams/join", "/api/v1/teams/invitations/preview"] {
            let unknown = post(
                &app,
                path,
                stranger,
                Some(json!({ "token": "no-such-token" })),
            )
            .await;
            assert_eq!(unknown.0, StatusCode::NOT_FOUND);
            for (caller, token) in [
                (stranger, &name_token),
                (stranger, &address_token),
                (unverified, &address_token),
                (stranger, &expired_token),
            ] {
                let answer = post(&app, path, caller, Some(json!({ "token": token }))).await;
                assert_eq!(answer, unknown, "{path}");
            }
        }

        let (status, _) = post(
            &app,
            "/api/v1/teams/join",
            invitee,
            Some(json!({ "token": name_token })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "the invitee still accepts");
        assert_eq!(
            role_of(&db, team_id, invitee).await.as_deref(),
            Some("member")
        );
    }

    async fn username_of(db: &DatabaseConnection, user: Uuid) -> String {
        users::Entity::find_by_id(user)
            .one(db)
            .await
            .expect("the lookup must succeed")
            .expect("the user exists")
            .username
    }

    async fn invite(
        app: &axum::Router,
        db: &DatabaseConnection,
        team_id: Uuid,
        caller: Uuid,
        body: serde_json::Value,
    ) -> (StatusCode, serde_json::Value) {
        request_as(
            app,
            "POST",
            &format!("/api/v1/teams/{}/invitations", slug_of(db, team_id).await),
            Some(caller),
            Some(body),
        )
        .await
    }

    /// Without the fields that differ between any two invitations.
    fn shape(mut body: serde_json::Value) -> serde_json::Value {
        for field in ["id", "token", "created_at", "expires_at", "invited_email"] {
            body.as_object_mut().expect("an object").remove(field);
        }
        body
    }

    /// The author is answered the same whether an account holds the address
    /// or not, and the invitation names no account: it is not pinned to one
    /// at creation any more.
    #[tokio::test]
    async fn an_invitation_by_address_is_answered_the_same_with_or_without_an_account() {
        let db = db().await;
        let app = app(&db).await;
        let captain = create_user(&db).await;
        let (team_id, _) = team_with(&db, captain, &[]).await;
        let holder = create_user(&db).await;
        let held = verified_address(&db, holder).await;

        let (held_status, held_body) = invite(
            &app,
            &db,
            team_id,
            captain,
            json!({ "default_role": "member", "invited_email": held }),
        )
        .await;
        let (free_status, free_body) = invite(
            &app,
            &db,
            team_id,
            captain,
            json!({ "default_role": "member", "invited_email": fresh_address() }),
        )
        .await;

        assert_eq!(held_status, StatusCode::CREATED);
        assert_eq!(free_status, StatusCode::CREATED);
        assert_eq!(held_body["invited_user_id"], serde_json::Value::Null);
        assert_eq!(shape(held_body), shape(free_body));
    }

    /// The account holding the address verified finds the invitation in its
    /// pending list, and is on the team only once it accepts.
    #[tokio::test]
    async fn an_invitation_by_address_waits_for_its_holder_to_accept() {
        let db = db().await;
        let app = app(&db).await;
        let captain = create_user(&db).await;
        let (team_id, _) = team_with(&db, captain, &[]).await;
        let holder = create_user(&db).await;
        let address = verified_address(&db, holder).await;
        let (_, invitation) = invite(
            &app,
            &db,
            team_id,
            captain,
            json!({ "default_role": "viewer", "invited_email": address }),
        )
        .await;

        let (_, pending) = request_as(
            &app,
            "GET",
            "/api/v1/teams/invitations/pending",
            Some(holder),
            None,
        )
        .await;
        let pending = pending.as_array().expect("a list");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0]["id"], invitation["id"]);
        assert_eq!(pending[0]["invited_by"], json!(captain));
        assert_eq!(
            role_of(&db, team_id, holder).await,
            None,
            "not before accepting"
        );

        let (status, _) = request_as(
            &app,
            "POST",
            &format!(
                "/api/v1/teams/invitations/{}/accept",
                invitation["id"].as_str().expect("an id")
            ),
            Some(holder),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            role_of(&db, team_id, holder).await.as_deref(),
            Some("viewer")
        );
    }

    /// An invitation lasts seven days unless its author says otherwise, and
    /// at most thirty.
    #[tokio::test]
    async fn an_invitation_expires_in_seven_days_by_default_and_thirty_at_most() {
        let db = db().await;
        let app = app(&db).await;
        let captain = create_user(&db).await;
        let (team_id, _) = team_with(&db, captain, &[]).await;
        let now = chrono::Utc::now();

        let (status, body) = invite(
            &app,
            &db,
            team_id,
            captain,
            json!({ "default_role": "member" }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        let expires_at: chrono::DateTime<chrono::Utc> =
            serde_json::from_value(body["expires_at"].clone()).expect("a date");
        assert!(
            (expires_at - (now + chrono::Duration::days(7)))
                .num_seconds()
                .abs()
                < 60
        );

        let (status, _) = invite(
            &app,
            &db,
            team_id,
            captain,
            json!({ "default_role": "member", "expires_at": now + chrono::Duration::days(29) }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);

        let (status, body) = invite(
            &app,
            &db,
            team_id,
            captain,
            json!({ "default_role": "member", "expires_at": now + chrono::Duration::days(31) }),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["error"], "invitation_expiry_too_far");
    }

    /// Negative: an address a member of the team has verified is
    /// `already_member`.
    #[tokio::test]
    async fn inviting_the_address_of_a_member_is_already_member() {
        let db = db().await;
        let app = app(&db).await;
        let captain = create_user(&db).await;
        let (team_id, members) = team_with(&db, captain, &[TeamRole::Member]).await;
        let address = verified_address(&db, members[0]).await;

        let (status, body) = invite(
            &app,
            &db,
            team_id,
            captain,
            json!({ "default_role": "member", "invited_email": address }),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"], "already_member");
    }

    /// Accepting records the author on the membership; the member list
    /// shows it and filters on it, and so does the invitation list.
    #[tokio::test]
    async fn the_author_is_recorded_and_listed() {
        let db = db().await;
        let app = app(&db).await;
        let captain = create_user(&db).await;
        let (team_id, members) = team_with(&db, captain, &[TeamRole::Admin]).await;
        let admin = members[0];
        let slug = slug_of(&db, team_id).await;
        let address = fresh_address();
        let (_, by_admin) = invite(
            &app,
            &db,
            team_id,
            admin,
            json!({ "default_role": "member", "invited_email": address }),
        )
        .await;
        let invitee = create_user(&db).await;
        let (status, joined) = invite(
            &app,
            &db,
            team_id,
            captain,
            json!({ "default_role": "member", "username": username_of(&db, invitee).await }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        let (status, _) = request_as(
            &app,
            "POST",
            &format!(
                "/api/v1/teams/invitations/{}/accept",
                joined["id"].as_str().expect("an id")
            ),
            Some(invitee),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (_, roster) = request_as(
            &app,
            "GET",
            &format!("/api/v1/teams/{slug}/members?invited_by={captain}"),
            Some(admin),
            None,
        )
        .await;
        let roster = roster.as_array().expect("a list");
        assert_eq!(roster.len(), 1);
        assert_eq!(roster[0]["user_id"], json!(invitee));
        assert_eq!(roster[0]["invited_by"], json!(captain));

        let (_, listed) = request_as(
            &app,
            "GET",
            &format!("/api/v1/teams/{slug}/invitations?invited_by={admin}"),
            Some(captain),
            None,
        )
        .await;
        let listed = listed.as_array().expect("a list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0]["id"], by_admin["id"]);
        assert_eq!(
            listed[0]["invited_by_username"],
            json!(username_of(&db, admin).await)
        );

        let (status, body) = request_as(
            &app,
            "GET",
            &format!("/api/v1/teams/{slug}/members?invited_by=nope"),
            Some(admin),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["error"], "validation_failed");
    }

    async fn list_invitations(
        app: &axum::Router,
        db: &DatabaseConnection,
        team_id: Uuid,
        caller: Uuid,
        query: &str,
    ) -> Vec<serde_json::Value> {
        let (status, body) = request_as(
            app,
            "GET",
            &format!(
                "/api/v1/teams/{}/invitations{query}",
                slug_of(db, team_id).await
            ),
            Some(caller),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body.as_array().expect("a list").clone()
    }

    /// The Admin list shows every pending invitation, by username included,
    /// each with its target, its invitee, its author and its dates; and
    /// `?invited_by=` narrows it whatever the invitations name.
    #[tokio::test]
    async fn the_admin_list_shows_every_pending_invitation_with_its_target() {
        let db = db().await;
        let app = app(&db).await;
        let captain = create_user(&db).await;
        let (team_id, members) = team_with(&db, captain, &[TeamRole::Admin]).await;
        let admin = members[0];
        let invitee = create_user(&db).await;
        let other_invitee = create_user(&db).await;
        let address = fresh_address();

        let mut ids = Vec::new();
        for (caller, body) in [
            (
                admin,
                json!({ "default_role": "member", "username": username_of(&db, invitee).await }),
            ),
            (
                captain,
                json!({ "default_role": "viewer", "username": username_of(&db, other_invitee).await }),
            ),
            (
                captain,
                json!({ "default_role": "member", "invited_email": address }),
            ),
            (admin, json!({ "default_role": "member" })),
        ] {
            let (status, answer) = invite(&app, &db, team_id, caller, body).await;
            assert_eq!(status, StatusCode::CREATED, "{answer}");
            ids.push(answer["id"].clone());
        }

        let listed = list_invitations(&app, &db, team_id, captain, "").await;
        let entry = |id: &serde_json::Value| {
            listed
                .iter()
                .find(|entry| &entry["id"] == id)
                .cloned()
                .expect("the invitation is listed")
        };
        assert_eq!(listed.len(), 4);
        let by_name = entry(&ids[0]);
        assert_eq!(by_name["target"], "username");
        assert_eq!(by_name["invited_user_id"], json!(invitee));
        assert_eq!(
            by_name["invited_username"],
            json!(username_of(&db, invitee).await)
        );
        assert_eq!(by_name["invited_email"], serde_json::Value::Null);
        assert_eq!(by_name["invited_by"], json!(admin));
        assert_eq!(
            by_name["invited_by_username"],
            json!(username_of(&db, admin).await)
        );
        assert!(by_name["created_at"].is_string() && by_name["expires_at"].is_string());
        let by_address = entry(&ids[2]);
        assert_eq!(by_address["target"], "email");
        assert_eq!(by_address["invited_email"], json!(address));
        assert_eq!(by_address["invited_username"], serde_json::Value::Null);
        assert_eq!(entry(&ids[3])["target"], "link");

        let only = |entries: Vec<serde_json::Value>| {
            let mut found: Vec<String> = entries
                .iter()
                .map(|entry| entry["id"].as_str().expect("an id").to_owned())
                .collect();
            found.sort();
            found
        };
        let sorted = |picked: &[usize]| {
            let mut wanted: Vec<String> = picked
                .iter()
                .map(|&i| ids[i].as_str().expect("an id").to_owned())
                .collect();
            wanted.sort();
            wanted
        };
        assert_eq!(
            only(
                list_invitations(&app, &db, team_id, admin, &format!("?invited_by={admin}")).await
            ),
            sorted(&[0, 3])
        );
        assert_eq!(
            only(
                list_invitations(&app, &db, team_id, admin, &format!("?invited_by={captain}"))
                    .await
            ),
            sorted(&[1, 2])
        );
    }

    /// The Captain, an Admin and the author, an Admin too since only they
    /// invite, may revoke an invitation by username; a Member or a Viewer is
    /// refused (403), someone off the team is told it does not exist (404).
    #[tokio::test]
    async fn the_captain_an_admin_or_the_author_revokes_an_invitation_by_username() {
        let db = db().await;
        let app = app(&db).await;
        let captain = create_user(&db).await;
        let (team_id, members) = team_with(
            &db,
            captain,
            &[
                TeamRole::Admin,
                TeamRole::Admin,
                TeamRole::Member,
                TeamRole::Viewer,
            ],
        )
        .await;
        let (author, other_admin, member, viewer) =
            (members[0], members[1], members[2], members[3]);
        let outsider = create_user(&db).await;
        let slug = slug_of(&db, team_id).await;

        for revoker in [captain, other_admin, author] {
            let invitee = create_user(&db).await;
            let (_, invitation) = invite(
                &app,
                &db,
                team_id,
                author,
                json!({ "default_role": "member", "username": username_of(&db, invitee).await }),
            )
            .await;
            let path = format!(
                "/api/v1/teams/{slug}/invitations/{}",
                invitation["id"].as_str().expect("an id")
            );

            for (refused, status) in [
                (member, StatusCode::FORBIDDEN),
                (viewer, StatusCode::FORBIDDEN),
                (outsider, StatusCode::NOT_FOUND),
            ] {
                assert_eq!(
                    request_as(&app, "DELETE", &path, Some(refused), None)
                        .await
                        .0,
                    status
                );
            }
            let (status, _) = request_as(&app, "DELETE", &path, Some(revoker), None).await;
            assert_eq!(status, StatusCode::NO_CONTENT);
            let (_, pending) = request_as(
                &app,
                "GET",
                "/api/v1/teams/invitations/pending",
                Some(invitee),
                None,
            )
            .await;
            assert_eq!(pending, json!([]), "the invitee no longer has it");
        }
    }

    /// The preview answers the invitee, and gives anyone else, or an unknown
    /// token, the one same 404.
    #[tokio::test]
    async fn the_preview_answers_the_invitee_alone() {
        let db = db().await;
        let app = app(&db).await;
        let captain = create_user(&db).await;
        let (team_id, _) = team_with(&db, captain, &[]).await;
        let invitee = create_user(&db).await;
        let (_, invitation) = invite(
            &app,
            &db,
            team_id,
            captain,
            json!({ "default_role": "viewer", "username": username_of(&db, invitee).await }),
        )
        .await;
        let token = known_token(&db, &invitation).await;
        let preview = |user: Uuid, token: String| {
            let app = app.clone();
            async move {
                request_as(
                    &app,
                    "POST",
                    "/api/v1/teams/invitations/preview",
                    Some(user),
                    Some(json!({ "token": token })),
                )
                .await
            }
        };

        let (status, body) = preview(invitee, token.clone()).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["team_name"], "Test Team");
        assert_eq!(body["role"], "viewer");
        assert_eq!(
            body["invited_by_username"],
            json!(username_of(&db, captain).await)
        );

        let stranger = preview(create_user(&db).await, token).await;
        let unknown = preview(invitee, "no-such-token".to_owned()).await;
        assert_eq!(stranger.0, StatusCode::NOT_FOUND);
        assert_eq!(stranger, unknown);
    }

    /// A targeted invitation's answer has no token field at all, by username
    /// or by address: its token goes to the invitee's mail alone. A generic
    /// link's answer carries its token, which is the author's to hand out,
    /// and it works.
    #[tokio::test]
    async fn only_a_generic_link_is_answered_with_its_token() {
        let db = db().await;
        let app = app(&db).await;
        let captain = create_user(&db).await;
        let (team_id, _) = team_with(&db, captain, &[]).await;

        for body in [
            json!({ "default_role": "member", "username": username_of(&db, create_user(&db).await).await }),
            json!({ "default_role": "member", "invited_email": fresh_address() }),
        ] {
            let (status, answer) = invite(&app, &db, team_id, captain, body).await;
            assert_eq!(status, StatusCode::CREATED, "{answer}");
            assert!(answer.get("token").is_none(), "{answer}");
        }

        let (status, link) = invite(
            &app,
            &db,
            team_id,
            captain,
            json!({ "default_role": "viewer" }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(link["target"], "link");
        let token = link["token"].as_str().expect("a link carries its token");
        let holder = create_user(&db).await;
        let (status, _) = post(
            &app,
            "/api/v1/teams/join",
            holder,
            Some(json!({ "token": token })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            role_of(&db, team_id, holder).await.as_deref(),
            Some("viewer")
        );
    }

    /// 429 past each window, per author and per team, on both targeted
    /// paths, and 503 when the limiter cannot answer.
    #[tokio::test]
    async fn targeted_invitations_are_rate_limited_per_author_and_per_team() {
        use redis::AsyncCommands;

        let db = db().await;
        let app = app(&db).await;
        let captain = create_user(&db).await;
        let (team_id, _) = team_with(&db, captain, &[]).await;
        let client = redis::Client::open(crate::test_support::env_or(
            "REDIS_URL",
            "redis://127.0.0.1:6379",
        ))
        .expect("REDIS_URL must be valid");
        let mut redis = redis::aio::ConnectionManager::new(client)
            .await
            .expect("a Redis instance must be reachable");

        for (path, suffix, limit) in [
            ("username", format!("author:{captain}"), 20),
            ("username", format!("team:{team_id}"), 50),
            ("email", format!("author:{captain}"), 10),
            ("email", format!("team:{team_id}"), 30),
        ] {
            let key = format!("ratelimit:invite:team:{path}:{suffix}");
            let body = if path == "username" {
                json!({ "default_role": "member", "username": username_of(&db, create_user(&db).await).await })
            } else {
                json!({ "default_role": "member", "invited_email": fresh_address() })
            };

            let _: () = redis
                .set_ex(&key, limit, 3600)
                .await
                .expect("SET must answer");
            let (limited, answer) = invite(&app, &db, team_id, captain, body.clone()).await;
            let _: () = redis
                .set(&key, "not-a-counter")
                .await
                .expect("SET must answer");
            let (unavailable, _) = invite(&app, &db, team_id, captain, body).await;
            let _: () = redis.del(&key).await.expect("DEL must answer");

            assert_eq!(limited, StatusCode::TOO_MANY_REQUESTS, "{key}: {answer}");
            assert_eq!(unavailable, StatusCode::SERVICE_UNAVAILABLE, "{key}");
        }
        for path in ["username", "email"] {
            for suffix in [format!("author:{captain}"), format!("team:{team_id}")] {
                let _: () = redis
                    .del(format!("ratelimit:invite:team:{path}:{suffix}"))
                    .await
                    .expect("DEL must answer");
            }
        }
    }

    /// An address no mail can go to is refused as a malformed one, and no
    /// invitation is written.
    #[tokio::test]
    async fn an_address_no_mail_can_go_to_invites_nobody() {
        let db = db().await;
        let app = app(&db).await;
        let captain = create_user(&db).await;
        let (team_id, _) = team_with(&db, captain, &[]).await;

        for address in ["a..b@example.test", "a@[127.0.0.1]"] {
            let (status, body) = invite(
                &app,
                &db,
                team_id,
                captain,
                json!({ "default_role": "member", "invited_email": address }),
            )
            .await;
            assert_eq!(
                status,
                StatusCode::UNPROCESSABLE_ENTITY,
                "{address}: {body}"
            );
            assert_eq!(
                body,
                json!({ "error": "validation_failed", "fields": { "invited_email": ["email"] } }),
                "{address}"
            );
        }
        let written = domain::entities::team_invitations::Entity::find()
            .filter(domain::entities::team_invitations::Column::TeamId.eq(team_id))
            .all(&db)
            .await
            .expect("the lookup must succeed");
        assert!(written.is_empty(), "no invitation is written");
    }

    /// `member_count` as GET /teams/{slug} and GET /teams both give it.
    async fn counts(
        app: &axum::Router,
        db: &DatabaseConnection,
        team_id: Uuid,
        caller: Uuid,
    ) -> (serde_json::Value, serde_json::Value) {
        let (status, team) = request_as(
            app,
            "GET",
            &format!("/api/v1/teams/{}", slug_of(db, team_id).await),
            Some(caller),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (status, teams) = request_as(app, "GET", "/api/v1/teams", Some(caller), None).await;
        assert_eq!(status, StatusCode::OK);
        let listed = teams
            .as_array()
            .expect("a list")
            .iter()
            .find(|t| t["id"] == team_id.to_string())
            .expect("the team is listed")["member_count"]
            .clone();
        (team["member_count"].clone(), listed)
    }

    /// The count is the team's members, its Captain included, from its
    /// creation on, and follows every arrival and departure.
    #[tokio::test]
    async fn the_member_count_follows_arrivals_and_departures() {
        let db = db().await;
        let app = app(&db).await;
        let captain = create_user(&db).await;

        let (status, created) = request_as(
            &app,
            "POST",
            "/api/v1/teams",
            Some(captain),
            Some(json!({ "name": "Counted", "color": "teal" })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{created}");
        assert_eq!(created["member_count"], 1);
        let team_id: Uuid = created["id"].as_str().unwrap().parse().unwrap();
        assert_eq!(
            counts(&app, &db, team_id, captain).await,
            (json!(1), json!(1))
        );

        let member = create_user(&db).await;
        add_team_member(&db, team_id, member, TeamRole::Member).await;
        assert_eq!(
            counts(&app, &db, team_id, captain).await,
            (json!(2), json!(2))
        );

        let (status, link) = invite(
            &app,
            &db,
            team_id,
            captain,
            json!({ "default_role": "viewer" }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{link}");
        let joiner = create_user(&db).await;
        let (status, joined) = post(
            &app,
            "/api/v1/teams/join",
            joiner,
            Some(json!({ "token": link["token"] })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{joined}");
        assert_eq!(
            joined["member_count"], 3,
            "the joiner counted in the answer"
        );
        assert_eq!(
            counts(&app, &db, team_id, member).await,
            (json!(3), json!(3))
        );

        let (status, _) = request_as(
            &app,
            "DELETE",
            &format!(
                "/api/v1/teams/{}/members/{member}",
                slug_of(&db, team_id).await
            ),
            Some(member),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "the member leaves");
        assert_eq!(
            counts(&app, &db, team_id, captain).await,
            (json!(2), json!(2))
        );

        let (status, renamed) = request_as(
            &app,
            "PATCH",
            &format!("/api/v1/teams/{}", slug_of(&db, team_id).await),
            Some(captain),
            Some(json!({ "name": "Recounted" })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{renamed}");
        assert_eq!(renamed["member_count"], 2);
    }

    /// GET /teams costs two queries, the teams and their counts, for one
    /// team as for four of different sizes.
    #[tokio::test]
    async fn the_team_list_costs_a_fixed_number_of_queries() {
        let db = db().await;
        let user = create_user(&db).await;
        let queries = Arc::new(AtomicUsize::new(0));
        let mut metered = crate::test_support::db().await;
        let counter = queries.clone();
        metered.set_metric_callback(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
        });

        let mut costs = Vec::new();
        for size in 0..4 {
            let (team_id, _) =
                team_with(&db, create_user(&db).await, &vec![TeamRole::Member; size]).await;
            add_team_member(&db, team_id, user, TeamRole::Viewer).await;

            queries.store(0, Ordering::SeqCst);
            let listed = services::team_service::list_teams_for_user(&metered, user)
                .await
                .expect("the list must load");
            costs.push(queries.load(Ordering::SeqCst));

            let entry = listed.iter().find(|t| t.id == team_id).expect("listed");
            // The Captain, `size` members, and the user.
            assert_eq!(entry.member_count, size as u64 + 2);
        }

        assert!(costs.iter().all(|cost| *cost == costs[0]), "{costs:?}");
        assert_eq!(costs[0], 2, "{costs:?}");
    }

    /// The invitee's pending list carries the team's colour next to its
    /// name, and nothing else than before: the same fields, two added. The
    /// avatar version goes only to a reader the team's avatar is served to:
    /// the author's answer and the Admins' list carry it, an invitee off the
    /// team gets `null`, and the same invitee once on the team gets it.
    #[tokio::test]
    async fn a_pending_team_invitation_announces_only_an_avatar_its_reader_can_load() {
        let db = db().await;
        let app = app(&db).await;
        let captain = create_user(&db).await;
        let (team_id, _) = team_with(&db, captain, &[]).await;
        teams::ActiveModel {
            id: sea_orm::Set(team_id),
            color: sea_orm::Set("amber".to_owned()),
            ..Default::default()
        }
        .update(&db)
        .await
        .expect("the team must be updated");
        let avatar = upload_avatar(&db, AvatarKind::Team, team_id).await;
        let invitee = create_user(&db).await;

        let (status, created) = invite(
            &app,
            &db,
            team_id,
            captain,
            json!({ "default_role": "member", "username": username_of(&db, invitee).await }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{created}");
        let (status, listed) = request_as(
            &app,
            "GET",
            &format!("/api/v1/teams/{}/invitations", slug_of(&db, team_id).await),
            Some(captain),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let listed = listed
            .as_array()
            .expect("a list")
            .iter()
            .find(|entry| entry["id"] == created["id"])
            .expect("listed for the Captain")
            .clone();
        for answer in [&created, &listed] {
            assert_eq!(answer["team_name"], "Test Team", "{answer}");
            assert_eq!(answer["team_color"], "amber", "{answer}");
            assert_eq!(answer["team_avatar_version"], json!(avatar), "{answer}");
        }
        assert_eq!(
            avatar_status(&app, captain, "teams", team_id, avatar).await,
            StatusCode::OK,
            "the Captain loads the avatar announced"
        );

        let pending = |user: Uuid| {
            let app = app.clone();
            let id = created["id"].clone();
            async move {
                let (status, pending) = request_as(
                    &app,
                    "GET",
                    "/api/v1/teams/invitations/pending",
                    Some(user),
                    None,
                )
                .await;
                assert_eq!(status, StatusCode::OK);
                pending
                    .as_array()
                    .expect("a list")
                    .iter()
                    .find(|entry| entry["id"] == id)
                    .expect("the invitation is pending for its invitee")
                    .clone()
            }
        };

        let entry = pending(invitee).await;
        assert_eq!(entry["team_name"], "Test Team");
        assert_eq!(entry["team_color"], "amber");
        assert_eq!(entry["team_avatar_version"], serde_json::Value::Null);
        assert_eq!(
            avatar_status(&app, invitee, "teams", team_id, avatar).await,
            StatusCode::NOT_FOUND,
            "the version withheld is one the invitee could not load"
        );
        let fields: std::collections::BTreeSet<&str> = entry
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        let expected: std::collections::BTreeSet<&str> = [
            "id",
            "team_id",
            "team_name",
            "team_color",
            "team_avatar_version",
            "default_role",
            "target",
            "invited_user_id",
            "invited_username",
            "invited_email",
            "invited_by",
            "invited_by_username",
            "expires_at",
            "max_uses",
            "uses",
            "created_at",
        ]
        .into_iter()
        .collect();
        assert_eq!(fields, expected);

        // Joined by another way, the invitation still pending: now a member,
        // the invitee is announced the avatar, and loads it.
        add_team_member(&db, team_id, invitee, TeamRole::Viewer).await;
        let entry = pending(invitee).await;
        assert_eq!(entry["team_avatar_version"], json!(avatar));
        assert_eq!(
            avatar_status(&app, invitee, "teams", team_id, avatar).await,
            StatusCode::OK
        );
    }
}

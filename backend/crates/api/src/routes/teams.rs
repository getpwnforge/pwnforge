// backend/crates/api/src/routes/teams.rs

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, patch, post},
};
use uuid::Uuid;
use validator::Validate;

use crate::middleware::{auth::AuthUser, json::JsonBody};
use crate::{error::AppError, state::AppState};
use domain::dto::error_responses::{SimpleErrorResponse, ValidationErrorResponse};
use domain::dto::team::{
    ChangeMemberRoleRequest, CreateInvitationRequest, CreateTeamRequest, JoinTeamRequest,
    TeamInvitationResponse, TeamMemberResponse, TeamPatch, TeamResponse, TeamRole,
};
use services::team_service::{self, TeamError};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/teams", get(list_teams).post(create_team))
        .route(
            "/teams/{slug}",
            get(get_team).patch(update_team).delete(delete_team),
        )
        .route("/teams/{slug}/members", get(list_members))
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
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 422, description = "Validation failed", body = ValidationErrorResponse),
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

    Ok(Json(TeamResponse::from_model(team, my_role)))
}

#[utoipa::path(
    patch,
    path = "/api/v1/teams/{slug}",
    tag = "Teams",
    request_body = TeamPatch,
    security(("access_token" = [])),
    responses(
        (status = 200, description = "Team updated", body = TeamResponse),
        (status = 403, description = "Insufficient role", body = SimpleErrorResponse),
        (status = 404, description = "Team not found or not a member", body = SimpleErrorResponse),
        (status = 422, description = "Validation failed", body = ValidationErrorResponse),
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

    Ok(Json(TeamResponse::from_model(updated, my_role)))
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

    team_service::delete_team(&state.db, team.id).await?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/teams/{slug}/members",
    tag = "Teams",
    security(("access_token" = [])),
    responses(
        (status = 200, description = "Team members", body = [TeamMemberResponse]),
        (status = 404, description = "Team not found or not a member", body = SimpleErrorResponse),
    )
)]
async fn list_members(
    State(state): State<AppState>,
    user: AuthUser,
    Path(slug): Path<String>,
) -> Result<Json<Vec<TeamMemberResponse>>, AppError> {
    let (team, _my_role) = team_service::get_team_for_member(&state.db, &slug, user.id).await?;
    let members = team_service::list_team_members(&state.db, team.id).await?;
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
        (status = 403, description = "Insufficient role, or attempts to grant/revoke Captain without being one", body = SimpleErrorResponse),
        (status = 404, description = "Team, target member not found, or caller not a member", body = SimpleErrorResponse),
        (status = 422, description = "Would leave the team without a Captain", body = SimpleErrorResponse),
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
        (status = 403, description = "Insufficient role", body = SimpleErrorResponse),
        (status = 404, description = "Team, target member not found, or caller not a member", body = SimpleErrorResponse),
        (status = 422, description = "Would leave the team without a Captain", body = SimpleErrorResponse),
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
    get,
    path = "/api/v1/teams/{slug}/invitations",
    tag = "Teams",
    security(("access_token" = [])),
    responses(
        (status = 200, description = "Active generic invitation links for the team", body = [TeamInvitationResponse]),
        (status = 403, description = "Insufficient role", body = SimpleErrorResponse),
        (status = 404, description = "Team not found or not a member", body = SimpleErrorResponse),
    )
)]
async fn list_invitations(
    State(state): State<AppState>,
    user: AuthUser,
    Path(slug): Path<String>,
) -> Result<Json<Vec<TeamInvitationResponse>>, AppError> {
    let (team, my_role) = team_service::get_team_for_member(&state.db, &slug, user.id).await?;
    team_service::require_role(my_role, TeamRole::Admin)?;

    let invitations = team_service::list_active_invitations(&state.db, team.id).await?;
    Ok(Json(invitations))
}

#[utoipa::path(
    post,
    path = "/api/v1/teams/{slug}/invitations",
    tag = "Teams",
    request_body = CreateInvitationRequest,
    security(("access_token" = [])),
    responses(
        (status = 201, description = "Invitation created", body = TeamInvitationResponse),
        (status = 403, description = "Insufficient role, or a non-Captain tried to invite as Captain", body = SimpleErrorResponse),
        (status = 404, description = "Team not found or not a member", body = SimpleErrorResponse),
        (status = 409, description = "User is already invited or already a member", body = SimpleErrorResponse),
        (status = 422, description = "Validation failed", body = ValidationErrorResponse),
    )
)]
async fn create_invitation(
    State(state): State<AppState>,
    user: AuthUser,
    Path(slug): Path<String>,
    JsonBody(payload): JsonBody<CreateInvitationRequest>,
) -> Result<(StatusCode, Json<TeamInvitationResponse>), AppError> {
    payload.validate()?;

    let (team, my_role) = team_service::get_team_for_member(&state.db, &slug, user.id).await?;
    team_service::require_role(my_role, TeamRole::Admin)?;

    // Only a Captain may mint an invitation that grants Captain on
    // acceptance, same reasoning as `can_manage_role`.
    if payload.default_role == TeamRole::Captain && my_role != TeamRole::Captain {
        return Err(TeamError::InsufficientRole.into());
    }

    let invitation =
        team_service::create_invitation(&state.db, &state.config, team.id, user.id, payload)
            .await?;
    Ok((StatusCode::CREATED, Json(invitation)))
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
    request_body = JoinTeamRequest,
    security(("access_token" = [])),
    responses(
        (status = 200, description = "Joined the team", body = TeamResponse),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 403, description = "Invitation is addressed to a different email than the caller's verified one", body = SimpleErrorResponse),
        (status = 404, description = "Invitation not found", body = SimpleErrorResponse),
        (status = 409, description = "Already a member", body = SimpleErrorResponse),
        (status = 422, description = "Validation failed, or invitation expired or exhausted", body = ValidationErrorResponse),
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
    security(("access_token" = [])),
    responses(
        (status = 200, description = "Pending targeted invitations for the current user", body = [TeamInvitationResponse]),
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
    path = "/api/v1/teams/invitations/{id}/accept",
    tag = "Teams",
    security(("access_token" = [])),
    responses(
        (status = 200, description = "Invitation accepted", body = TeamResponse),
        (status = 403, description = "Invitation is not addressed to the current user", body = SimpleErrorResponse),
        (status = 404, description = "Invitation not found", body = SimpleErrorResponse),
        (status = 409, description = "Already a member", body = SimpleErrorResponse),
        (status = 422, description = "Invitation expired", body = SimpleErrorResponse),
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
    security(("access_token" = [])),
    responses(
        (status = 204, description = "Invitation rejected"),
        (status = 403, description = "Invitation is not addressed to the current user", body = SimpleErrorResponse),
        (status = 404, description = "Invitation not found", body = SimpleErrorResponse),
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

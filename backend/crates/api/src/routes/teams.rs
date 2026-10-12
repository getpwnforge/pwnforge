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
use domain::dto::error_responses::{SimpleErrorResponse, UnprocessableErrorResponse};
use domain::dto::team::{
    ChangeMemberRoleRequest, CreateInvitationRequest, CreateTeamRequest, JoinTeamRequest,
    TeamInvitationResponse, TeamMemberResponse, TeamPatch, TeamResponse, TeamRole,
    TransferCaptaincyRequest,
};
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
        (status = 400, description = "malformed_json or invalid_body: the body is not JSON, or could not be read", body = SimpleErrorResponse),
        (status = 403, description = "Insufficient role", body = SimpleErrorResponse),
        (status = 404, description = "Team not found or not a member", body = SimpleErrorResponse),
        (status = 415, description = "unsupported_media_type: Content-Type is not application/json", body = SimpleErrorResponse),
        (status = 422, description = "validation_failed or invalid_body", body = UnprocessableErrorResponse),
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

    team_service::delete_team(&state.db, &state.storage, team.id).await?;

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
        (status = 409, description = "personal_team_not_transferable, target_already_captain, or target_pending_deletion: the target's account is scheduled for deletion", body = SimpleErrorResponse),
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
        (status = 400, description = "malformed_json or invalid_body: the body is not JSON, or could not be read", body = SimpleErrorResponse),
        (status = 403, description = "Insufficient role, or a non-Captain tried to invite as Captain", body = SimpleErrorResponse),
        (status = 404, description = "Team not found or not a member", body = SimpleErrorResponse),
        (status = 409, description = "already_invited or already_member, or captain_must_transfer: an invitation cannot grant Captain, the role moves only by transfer", body = SimpleErrorResponse),
        (status = 415, description = "unsupported_media_type: Content-Type is not application/json", body = SimpleErrorResponse),
        (status = 422, description = "validation_failed or invalid_body", body = UnprocessableErrorResponse),
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

    // An Admin asking for a Captain invitation is refused as before, same
    // reasoning as `can_manage_role`. A Captain asking is refused by the
    // service: nobody joins as Captain, the role moves only by transfer.
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
        (status = 400, description = "malformed_json or invalid_body: the body is not JSON, or could not be read", body = SimpleErrorResponse),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 403, description = "Invitation is addressed to a different email than the caller's verified one", body = SimpleErrorResponse),
        (status = 404, description = "Invitation not found", body = SimpleErrorResponse),
        (status = 409, description = "Already a member", body = SimpleErrorResponse),
        (status = 415, description = "unsupported_media_type: Content-Type is not application/json", body = SimpleErrorResponse),
        (status = 422, description = "validation_failed, invalid_body, invitation_expired or invitation_max_uses_reached", body = UnprocessableErrorResponse),
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
        (status = 422, description = "invitation_expired", body = UnprocessableErrorResponse),
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

#[cfg(test)]
mod tests {
    use crate::test_support::{
        add_team_member, app, avatar_stored, create_team, create_team_workspace, create_user, db,
        request_as, unique, upload_avatar,
    };
    use axum::http::StatusCode;
    use domain::dto::team::TeamRole;
    use domain::entities::{team_members, teams, users};
    use domain::types::AccentColor;
    use sea_orm::sea_query::Expr;
    use sea_orm::{
        ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
    };
    use serde_json::json;
    use services::storage_service::AvatarKind;
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

    /// A personal team stays with the account it belongs to.
    #[tokio::test]
    async fn a_personal_team_is_not_transferable() {
        let db = db().await;
        let app = app(&db).await;
        let captain = create_user(&db).await;
        let team = teams::ActiveModel {
            id: Set(Uuid::now_v7()),
            slug: Set(unique("test-personal")),
            name: Set("Personal".to_owned()),
            color: Set(AccentColor::Blue.as_str().to_owned()),
            personal: Set(true),
            created_by: Set(Some(captain)),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("personal team fixture must insert");
        add_team_member(&db, team.id, captain, TeamRole::Captain).await;
        let member = create_user(&db).await;
        add_team_member(&db, team.id, member, TeamRole::Member).await;

        let (status, body) = transfer(&app, &db, team.id, captain, member).await;

        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"], "personal_team_not_transferable");
        assert_eq!(
            role_of(&db, team.id, captain).await.as_deref(),
            Some("captain")
        );
        assert_eq!(
            role_of(&db, team.id, member).await.as_deref(),
            Some("member")
        );
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
}

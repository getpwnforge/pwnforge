// backend/crates/api/src/routes/workspaces.rs

use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
};
use validator::Validate;

use crate::middleware::workspace::{WorkspaceMember, role};
use crate::middleware::{auth::AuthUser, json::JsonBody};
use crate::{error::AppError, state::AppState};
use domain::dto::error_responses::{SimpleErrorResponse, UnprocessableErrorResponse};
use domain::dto::invitation::InvitedByFilter;
use domain::dto::workspace::{
    CreateWorkspaceInput, EffectiveMember, Membership, UpdateWorkspaceInput, WorkspaceResponse,
    WorkspaceSummary,
};
use domain::entities::workspaces;
use services::workspace_service::{self, WorkspaceError};

/// Every route naming a workspace takes a `WorkspaceMember<_>`, whose marker
/// is the minimum role: the requirement is in the signature, not in a check
/// a handler could forget. Archived-state checks live in the service, on the
/// writes they guard.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/workspaces", get(list_workspaces).post(create_workspace))
        .route(
            "/workspaces/{slug}",
            get(get_workspace)
                .patch(update_workspace)
                .delete(delete_workspace),
        )
        .route("/workspaces/{slug}/archive", post(archive_workspace))
        .route("/workspaces/{slug}/unarchive", post(unarchive_workspace))
        // POST and not PUT: the CORS layer does not allow PUT.
        .route(
            "/workspaces/{slug}/favorite",
            post(add_favorite).delete(remove_favorite),
        )
        .route("/workspaces/{slug}/members", get(list_members))
}

/// A stored enum value the DTO cannot represent is corrupt data, not the
/// caller's fault.
fn to_response(
    workspace: workspaces::Model,
    membership: Membership,
) -> Result<WorkspaceResponse, AppError> {
    let id = workspace.id;
    WorkspaceResponse::try_from_model(workspace, membership)
        .ok_or_else(|| WorkspaceError::DataIntegrity(id).into())
}

#[utoipa::path(
    get,
    path = "/api/v1/workspaces",
    tag = "Workspaces",
    security(("access_token" = [])),
    responses(
        (status = 200, description = "Workspaces the user can see: favourites first, archived last, then by name", body = [WorkspaceSummary]),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
    )
)]
async fn list_workspaces(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<WorkspaceSummary>>, AppError> {
    let workspaces = workspace_service::list_workspaces_for_user(&state.db, user.id).await?;
    Ok(Json(workspaces))
}

#[utoipa::path(
    post,
    path = "/api/v1/workspaces",
    tag = "Workspaces",
    request_body = CreateWorkspaceInput,
    security(("access_token" = [])),
    responses(
        (status = 201, description = "Workspace created", body = WorkspaceResponse),
        (status = 400, description = "malformed_json or invalid_body: the body is not JSON, or could not be read", body = SimpleErrorResponse),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 403, description = "insufficient_role: a team member below Admin, or below Member when the team's members_can_create_workspaces is on. A Viewer is refused either way", body = SimpleErrorResponse),
        (status = 404, description = "Team not found or not a member", body = SimpleErrorResponse),
        (status = 415, description = "unsupported_media_type: Content-Type is not application/json", body = SimpleErrorResponse),
        (status = 422, description = "validation_failed, invalid_body, invalid_ownership, platform_required, ends_before_start, workspace_name_not_sluggable or workspace_name_reserved", body = UnprocessableErrorResponse),
    )
)]
async fn create_workspace(
    State(state): State<AppState>,
    user: AuthUser,
    JsonBody(input): JsonBody<CreateWorkspaceInput>,
) -> Result<(StatusCode, Json<WorkspaceResponse>), AppError> {
    input.validate()?;
    let (workspace, membership) =
        workspace_service::create_workspace(&state.db, user.id, input).await?;
    let response = to_response(workspace, membership)?;
    Ok((StatusCode::CREATED, Json(response)))
}

#[utoipa::path(
    get,
    path = "/api/v1/workspaces/{slug}",
    tag = "Workspaces",
    params(("slug" = String, Path, description = "Workspace slug")),
    security(("access_token" = [])),
    responses(
        (status = 200, description = "Workspace detail", body = WorkspaceResponse),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 404, description = "Workspace not found, or no access to it", body = SimpleErrorResponse),
    )
)]
async fn get_workspace(
    member: WorkspaceMember<role::Viewer>,
) -> Result<Json<WorkspaceResponse>, AppError> {
    Ok(Json(to_response(member.workspace, member.membership)?))
}

#[utoipa::path(
    patch,
    path = "/api/v1/workspaces/{slug}",
    tag = "Workspaces",
    params(("slug" = String, Path, description = "Workspace slug")),
    request_body = UpdateWorkspaceInput,
    security(("access_token" = [])),
    responses(
        (status = 200, description = "Workspace updated", body = WorkspaceResponse),
        (status = 400, description = "malformed_json or invalid_body: the body is not JSON, or could not be read", body = SimpleErrorResponse),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 403, description = "Below Admin", body = SimpleErrorResponse),
        (status = 404, description = "Workspace not found, or no access to it", body = SimpleErrorResponse),
        (status = 409, description = "The workspace is archived", body = SimpleErrorResponse),
        (status = 415, description = "unsupported_media_type: Content-Type is not application/json", body = SimpleErrorResponse),
        (status = 422, description = "validation_failed, invalid_body, invalid_event_url, or ends_before_start", body = UnprocessableErrorResponse),
    )
)]
async fn update_workspace(
    State(state): State<AppState>,
    member: WorkspaceMember<role::Admin>,
    JsonBody(input): JsonBody<UpdateWorkspaceInput>,
) -> Result<Json<WorkspaceResponse>, AppError> {
    input.validate()?;
    let updated =
        workspace_service::update_workspace(&state.db, member.workspace.id, input).await?;
    Ok(Json(to_response(updated, member.membership)?))
}

#[utoipa::path(
    delete,
    path = "/api/v1/workspaces/{slug}",
    tag = "Workspaces",
    params(("slug" = String, Path, description = "Workspace slug")),
    security(("access_token" = [])),
    responses(
        (status = 204, description = "Workspace deleted"),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 403, description = "Captain only, or the owner of a solo workspace", body = SimpleErrorResponse),
        (status = 404, description = "Workspace not found, or no access to it", body = SimpleErrorResponse),
    )
)]
async fn delete_workspace(
    State(state): State<AppState>,
    member: WorkspaceMember<role::Captain>,
) -> Result<StatusCode, AppError> {
    workspace_service::delete_workspace(&state.db, &state.storage, member.workspace.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/workspaces/{slug}/archive",
    tag = "Workspaces",
    params(("slug" = String, Path, description = "Workspace slug")),
    security(("access_token" = [])),
    responses(
        (status = 200, description = "Workspace archived; idempotent. Every pending invitation to it is cancelled in the same transaction (cancel_reason workspace_archived), and unarchiving brings none back", body = WorkspaceResponse),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 403, description = "Captain only, or the owner of a solo workspace", body = SimpleErrorResponse),
        (status = 404, description = "Workspace not found, or no access to it", body = SimpleErrorResponse),
    )
)]
async fn archive_workspace(
    State(state): State<AppState>,
    member: WorkspaceMember<role::Captain>,
) -> Result<Json<WorkspaceResponse>, AppError> {
    let archived = workspace_service::archive_workspace(&state.db, member.workspace.id).await?;
    Ok(Json(to_response(archived, member.membership)?))
}

#[utoipa::path(
    post,
    path = "/api/v1/workspaces/{slug}/unarchive",
    tag = "Workspaces",
    params(("slug" = String, Path, description = "Workspace slug")),
    security(("access_token" = [])),
    responses(
        (status = 200, description = "Workspace unarchived; idempotent", body = WorkspaceResponse),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 403, description = "Captain only, or the owner of a solo workspace", body = SimpleErrorResponse),
        (status = 404, description = "Workspace not found, or no access to it", body = SimpleErrorResponse),
    )
)]
async fn unarchive_workspace(
    State(state): State<AppState>,
    member: WorkspaceMember<role::Captain>,
) -> Result<Json<WorkspaceResponse>, AppError> {
    let restored = workspace_service::unarchive_workspace(&state.db, member.workspace.id).await?;
    Ok(Json(to_response(restored, member.membership)?))
}

#[utoipa::path(
    post,
    path = "/api/v1/workspaces/{slug}/favorite",
    tag = "Workspaces",
    params(("slug" = String, Path, description = "Workspace slug")),
    security(("access_token" = [])),
    responses(
        (status = 204, description = "Marked as a favourite; idempotent"),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 404, description = "Workspace not found, or no access to it", body = SimpleErrorResponse),
    )
)]
async fn add_favorite(
    State(state): State<AppState>,
    member: WorkspaceMember<role::Viewer>,
) -> Result<StatusCode, AppError> {
    workspace_service::add_favorite(&state.db, member.user_id, member.workspace.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/v1/workspaces/{slug}/favorite",
    tag = "Workspaces",
    params(("slug" = String, Path, description = "Workspace slug")),
    security(("access_token" = [])),
    responses(
        (status = 204, description = "No longer a favourite; idempotent"),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 404, description = "Workspace not found, or no access to it", body = SimpleErrorResponse),
    )
)]
async fn remove_favorite(
    State(state): State<AppState>,
    member: WorkspaceMember<role::Viewer>,
) -> Result<StatusCode, AppError> {
    workspace_service::remove_favorite(&state.db, member.user_id, member.workspace.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/workspaces/{slug}/members",
    tag = "Workspaces",
    operation_id = "list_workspace_members",
    params(
        ("slug" = String, Path, description = "Workspace slug"),
        ("invited_by" = Option<Uuid>, Query, description = "Only the guests who joined by an invitation from this account"),
    ),
    security(("access_token" = [])),
    responses(
        (status = 200, description = "Everyone with access, most senior first; excluded members are not listed. A guest carries the author of the invitation they accepted (`invited_by`)", body = [EffectiveMember]),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 404, description = "Workspace not found, or no access to it", body = SimpleErrorResponse),
        (status = 422, description = "validation_failed: invited_by is not a uuid", body = UnprocessableErrorResponse),
    )
)]
async fn list_members(
    State(state): State<AppState>,
    member: WorkspaceMember<role::Viewer>,
    Query(filter): Query<InvitedByFilter>,
) -> Result<Json<Vec<EffectiveMember>>, AppError> {
    let invited_by = filter.parse()?;
    let mut members =
        workspace_service::list_effective_members(&state.db, &member.workspace).await?;
    if let Some(author) = invited_by {
        members.retain(|member| member.invited_by == Some(author));
    }
    Ok(Json(members))
}

#[cfg(test)]
mod tests {
    use crate::test_support::{
        add_team_member, app, avatar_status, avatar_stored, create_solo_workspace, create_team,
        create_team_workspace, create_user, db, request_as, unique, upload_avatar,
    };
    use axum::http::StatusCode;
    use domain::dto::team::TeamRole;
    use domain::entities::{teams, workspace_favorites, workspace_members, workspaces};
    use sea_orm::{
        ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait,
        QueryFilter, Set,
    };
    use serde_json::{Value, json};
    use services::storage_service::AvatarKind;
    use services::workspace_service;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use uuid::Uuid;

    fn ws_uri(slug: &str) -> String {
        format!("/api/v1/workspaces/{slug}")
    }

    /// A team workspace with one member per role, all created for the test.
    struct TeamFixture {
        workspace: workspaces::Model,
        team_id: Uuid,
        captain: Uuid,
        admin: Uuid,
        member: Uuid,
        viewer: Uuid,
    }

    async fn team_fixture(db: &DatabaseConnection) -> TeamFixture {
        let captain = create_user(db).await;
        let team_id = create_team(db, captain).await;

        let admin = create_user(db).await;
        let member = create_user(db).await;
        let viewer = create_user(db).await;
        add_team_member(db, team_id, admin, TeamRole::Admin).await;
        add_team_member(db, team_id, member, TeamRole::Member).await;
        add_team_member(db, team_id, viewer, TeamRole::Viewer).await;

        let workspace = create_team_workspace(db, team_id, "Team Workspace").await;

        TeamFixture {
            workspace,
            team_id,
            captain,
            admin,
            member,
            viewer,
        }
    }

    async fn stored(db: &DatabaseConnection, id: Uuid) -> workspaces::Model {
        workspaces::Entity::find_by_id(id)
            .one(db)
            .await
            .expect("the lookup must succeed")
            .expect("the workspace must exist")
    }

    /// The three answers to the same GET: no session, no access, access. The
    /// member's 200 is what makes the stranger's 404 mean something.
    #[tokio::test]
    async fn get_is_401_anonymous_404_for_a_stranger_and_200_for_a_member() {
        let db = db().await;
        let app = app(&db).await;
        let f = team_fixture(&db).await;
        let stranger = create_user(&db).await;
        let uri = ws_uri(&f.workspace.slug);

        let (anonymous, _) = request_as(&app, "GET", &uri, None, None).await;
        assert_eq!(anonymous, StatusCode::UNAUTHORIZED);

        let (outsider, _) = request_as(&app, "GET", &uri, Some(stranger), None).await;
        assert_eq!(
            outsider,
            StatusCode::NOT_FOUND,
            "a stranger gets 404, never 403"
        );

        let (status, body) = request_as(&app, "GET", &uri, Some(f.captain), None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["my_role"], "captain");
    }

    /// An excluded team member counts as a non-member: their 404 must be
    /// byte for byte the one a slug that never existed gets.
    #[tokio::test]
    async fn an_excluded_member_gets_the_same_404_as_a_missing_slug() {
        let db = db().await;
        let app = app(&db).await;
        let f = team_fixture(&db).await;

        workspace_service::exclude_member(&db, f.workspace.id, f.member)
            .await
            .expect("the exclusion must be written");

        let (excluded, excluded_body) = request_as(
            &app,
            "GET",
            &ws_uri(&f.workspace.slug),
            Some(f.member),
            None,
        )
        .await;
        let (missing, missing_body) = request_as(
            &app,
            "GET",
            &ws_uri(&unique("no-such")),
            Some(f.member),
            None,
        )
        .await;

        assert_eq!(excluded, StatusCode::NOT_FOUND);
        assert_eq!(missing, StatusCode::NOT_FOUND);
        assert_eq!(excluded_body, missing_body);

        // Control: the rest of the team still sees it.
        let (viewer, _) = request_as(
            &app,
            "GET",
            &ws_uri(&f.workspace.slug),
            Some(f.viewer),
            None,
        )
        .await;
        assert_eq!(viewer, StatusCode::OK);
    }

    #[tokio::test]
    async fn patch_is_403_for_a_viewer_and_200_for_an_admin() {
        let db = db().await;
        let app = app(&db).await;
        let f = team_fixture(&db).await;
        let uri = ws_uri(&f.workspace.slug);
        let patch = json!({ "name": "Renamed" });

        let (viewer, _) =
            request_as(&app, "PATCH", &uri, Some(f.viewer), Some(patch.clone())).await;
        assert_eq!(viewer, StatusCode::FORBIDDEN);

        let (admin, body) = request_as(&app, "PATCH", &uri, Some(f.admin), Some(patch)).await;
        assert_eq!(admin, StatusCode::OK);
        assert_eq!(body["name"], "Renamed");
    }

    /// An archived workspace is read-only. The role is checked first, so a
    /// Viewer still learns only that they may not write, not the state.
    #[tokio::test]
    async fn patch_on_an_archived_workspace_is_409_for_an_admin_and_403_for_a_viewer() {
        let db = db().await;
        let app = app(&db).await;
        let f = team_fixture(&db).await;
        let uri = ws_uri(&f.workspace.slug);

        let (archived, _) = request_as(
            &app,
            "POST",
            &format!("{uri}/archive"),
            Some(f.captain),
            None,
        )
        .await;
        assert_eq!(archived, StatusCode::OK);

        let patch = json!({ "name": "Renamed" });
        let (admin, body) =
            request_as(&app, "PATCH", &uri, Some(f.admin), Some(patch.clone())).await;
        assert_eq!(admin, StatusCode::CONFLICT);
        assert_eq!(body["error"], "workspace_archived");

        let (viewer, _) = request_as(&app, "PATCH", &uri, Some(f.viewer), Some(patch)).await;
        assert_eq!(viewer, StatusCode::FORBIDDEN);

        assert_eq!(stored(&db, f.workspace.id).await.name, "Team Workspace");
    }

    #[tokio::test]
    async fn delete_is_captain_only_or_the_solo_owner() {
        let db = db().await;
        let app = app(&db).await;
        let f = team_fixture(&db).await;
        let uri = ws_uri(&f.workspace.slug);

        let (admin, _) = request_as(&app, "DELETE", &uri, Some(f.admin), None).await;
        assert_eq!(admin, StatusCode::FORBIDDEN);

        let (captain, _) = request_as(&app, "DELETE", &uri, Some(f.captain), None).await;
        assert_eq!(captain, StatusCode::NO_CONTENT);

        let owner = create_user(&db).await;
        let solo = create_solo_workspace(&db, owner, "Solo").await;
        let (status, _) = request_as(&app, "DELETE", &ws_uri(&solo.slug), Some(owner), None).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn archive_and_unarchive_are_captain_only_and_idempotent() {
        let db = db().await;
        let app = app(&db).await;
        let f = team_fixture(&db).await;
        let archive = format!("{}/archive", ws_uri(&f.workspace.slug));
        let unarchive = format!("{}/unarchive", ws_uri(&f.workspace.slug));

        let (admin_archive, _) = request_as(&app, "POST", &archive, Some(f.admin), None).await;
        assert_eq!(admin_archive, StatusCode::FORBIDDEN);
        let (admin_unarchive, _) = request_as(&app, "POST", &unarchive, Some(f.admin), None).await;
        assert_eq!(admin_unarchive, StatusCode::FORBIDDEN);

        for _ in 0..2 {
            let (status, body) = request_as(&app, "POST", &archive, Some(f.captain), None).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(body["archived"], true);
        }
        for _ in 0..2 {
            let (status, body) = request_as(&app, "POST", &unarchive, Some(f.captain), None).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(body["archived"], false);
        }
    }

    /// Favouriting is behind the same access rule as reading: someone with no
    /// access learns nothing, and nothing is written for them.
    #[tokio::test]
    async fn favoriting_without_access_is_an_indistinguishable_404_and_writes_nothing() {
        let db = db().await;
        let app = app(&db).await;
        let f = team_fixture(&db).await;
        let stranger = create_user(&db).await;
        let real = format!("{}/favorite", ws_uri(&f.workspace.slug));
        let random = format!("{}/favorite", ws_uri(&unique("no-such")));

        let (real_status, real_body) = request_as(&app, "POST", &real, Some(stranger), None).await;
        let (random_status, random_body) =
            request_as(&app, "POST", &random, Some(stranger), None).await;

        assert_eq!(real_status, StatusCode::NOT_FOUND);
        assert_eq!(random_status, StatusCode::NOT_FOUND);
        assert_eq!(real_body, random_body);

        let row = workspace_favorites::Entity::find_by_id((stranger, f.workspace.id))
            .one(&db)
            .await
            .expect("the lookup must succeed");
        assert!(row.is_none(), "no favourite may be written for a stranger");

        // Control: a member can favourite, twice, and the row exists.
        for _ in 0..2 {
            let (status, _) = request_as(&app, "POST", &real, Some(f.viewer), None).await;
            assert_eq!(status, StatusCode::NO_CONTENT);
        }
        let row = workspace_favorites::Entity::find_by_id((f.viewer, f.workspace.id))
            .one(&db)
            .await
            .expect("the lookup must succeed");
        assert!(row.is_some());
    }

    #[tokio::test]
    async fn creating_a_team_workspace_needs_admin_and_hides_foreign_teams() {
        let db = db().await;
        let app = app(&db).await;
        let f = team_fixture(&db).await;
        let stranger = create_user(&db).await;
        let body = json!({
            "name": "New Team Workspace",
            "color": "violet",
            "type": "event",
            "mode": "team",
            "team_id": f.team_id,
        });

        let (member, _) = request_as(
            &app,
            "POST",
            "/api/v1/workspaces",
            Some(f.member),
            Some(body.clone()),
        )
        .await;
        assert_eq!(member, StatusCode::FORBIDDEN);

        let (outsider, outsider_body) = request_as(
            &app,
            "POST",
            "/api/v1/workspaces",
            Some(stranger),
            Some(body.clone()),
        )
        .await;
        assert_eq!(outsider, StatusCode::NOT_FOUND);
        assert_eq!(outsider_body["error"], "team_not_found");

        let (captain, created) = request_as(
            &app,
            "POST",
            "/api/v1/workspaces",
            Some(f.captain),
            Some(body),
        )
        .await;
        assert_eq!(captain, StatusCode::CREATED);
        assert_eq!(created["my_role"], "captain");
    }

    async fn create_in_team(
        app: &axum::Router,
        caller: Uuid,
        team_id: Uuid,
    ) -> (StatusCode, Value) {
        let body = json!({
            "name": "New Team Workspace",
            "color": "violet",
            "type": "event",
            "mode": "team",
            "team_id": team_id,
        });
        request_as(app, "POST", "/api/v1/workspaces", Some(caller), Some(body)).await
    }

    /// Sets the team's `members_can_create_workspaces` directly.
    async fn let_members_create(db: &DatabaseConnection, team_id: Uuid, allowed: bool) {
        teams::ActiveModel {
            id: Set(team_id),
            members_can_create_workspaces: Set(allowed),
            ..Default::default()
        }
        .update(db)
        .await
        .expect("the setting must be written");
    }

    async fn team_workspace_count(db: &DatabaseConnection, team_id: Uuid) -> u64 {
        workspaces::Entity::find()
            .filter(workspaces::Column::TeamId.eq(team_id))
            .count(db)
            .await
            .expect("the count must succeed")
    }

    fn id_of(created: &Value) -> Uuid {
        created["id"]
            .as_str()
            .and_then(|id| id.parse().ok())
            .expect("the answer must carry the workspace id")
    }

    /// With the setting off, a Member is refused and nothing is created.
    #[tokio::test]
    async fn a_member_cannot_create_a_team_workspace_when_the_team_does_not_allow_it() {
        let db = db().await;
        let app = app(&db).await;
        let f = team_fixture(&db).await;
        let_members_create(&db, f.team_id, false).await;

        let (status, body) = create_in_team(&app, f.member, f.team_id).await;

        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["error"], "insufficient_role");
        assert_eq!(
            team_workspace_count(&db, f.team_id).await,
            1,
            "only the fixture's workspace"
        );
    }

    /// With the setting on, a Member creates one and holds their team role on
    /// it, nothing more: no override is written for the creator.
    #[tokio::test]
    async fn a_member_creates_a_team_workspace_when_the_team_allows_it() {
        let db = db().await;
        let app = app(&db).await;
        let f = team_fixture(&db).await;
        let_members_create(&db, f.team_id, true).await;

        let (status, created) = create_in_team(&app, f.member, f.team_id).await;

        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(created["my_role"], "member");
        let row = workspace_members::Entity::find_by_id((id_of(&created), f.member))
            .one(&db)
            .await
            .expect("the lookup must succeed");
        assert!(row.is_none(), "the creator gets no override");
    }

    /// A Viewer is refused whatever the setting says.
    #[tokio::test]
    async fn a_viewer_cannot_create_a_team_workspace_even_when_the_team_allows_it() {
        let db = db().await;
        let app = app(&db).await;
        let f = team_fixture(&db).await;
        let_members_create(&db, f.team_id, true).await;

        let (status, body) = create_in_team(&app, f.viewer, f.team_id).await;

        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["error"], "insufficient_role");
        assert_eq!(team_workspace_count(&db, f.team_id).await, 1);
    }

    /// Turning the setting off touches no workspace created under it: the row
    /// is as it was and its creator still reaches it as a Member. Only the
    /// next creation is refused.
    #[tokio::test]
    async fn turning_the_setting_off_leaves_the_workspaces_created_under_it() {
        let db = db().await;
        let app = app(&db).await;
        let f = team_fixture(&db).await;
        let_members_create(&db, f.team_id, true).await;
        let (status, created) = create_in_team(&app, f.member, f.team_id).await;
        assert_eq!(status, StatusCode::CREATED);
        let before = stored(&db, id_of(&created)).await;

        // Through the team route, as an Admin turns it off.
        let (status, _) = request_as(
            &app,
            "PATCH",
            &format!("/api/v1/teams/{}", team_row(&db, f.team_id).await.slug),
            Some(f.admin),
            Some(json!({ "members_can_create_workspaces": false })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        assert_eq!(stored(&db, before.id).await, before);
        let (status, body) =
            request_as(&app, "GET", &ws_uri(&before.slug), Some(f.member), None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["my_role"], "member");

        let (status, _) = create_in_team(&app, f.member, f.team_id).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "the next creation is refused"
        );
    }

    async fn create_solo(app: &axum::Router, owner: Uuid, body: Value) -> (StatusCode, Value) {
        request_as(app, "POST", "/api/v1/workspaces", Some(owner), Some(body)).await
    }

    #[tokio::test]
    async fn a_color_outside_the_palette_is_an_invalid_body() {
        let db = db().await;
        let app = app(&db).await;
        let owner = create_user(&db).await;

        let (status, body) = create_solo(
            &app,
            owner,
            json!({ "name": "Hex", "color": "#EB5927", "type": "event", "mode": "solo" }),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["error"], "invalid_body");

        let (status, body) = create_solo(
            &app,
            owner,
            json!({ "name": "Teal", "color": "teal", "type": "event", "mode": "solo" }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(body["my_role"], "owner");
    }

    #[tokio::test]
    async fn a_platform_workspace_must_name_its_platform() {
        let db = db().await;
        let app = app(&db).await;
        let owner = create_user(&db).await;

        let (status, body) = create_solo(
            &app,
            owner,
            json!({ "name": "No platform", "color": "blue", "type": "platform", "mode": "solo" }),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["error"], "platform_required");

        let (status, body) = create_solo(
            &app,
            owner,
            json!({
                "name": "Root-Me",
                "color": "blue",
                "type": "platform",
                "platform": "root_me",
                "mode": "solo",
            }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(body["platform"], "root_me");

        let (status, _) = create_solo(
            &app,
            owner,
            json!({ "name": "Event", "color": "blue", "type": "event", "mode": "solo" }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
    }

    #[tokio::test]
    async fn the_date_window_is_checked_on_create_and_on_merged_patch_values() {
        let db = db().await;
        let app = app(&db).await;
        let owner = create_user(&db).await;
        let starts = "2026-11-01T10:00:00Z";
        let ends = "2026-11-03T10:00:00Z";

        // A zero-length window is refused, not only a backwards one.
        let (status, body) = create_solo(
            &app,
            owner,
            json!({
                "name": "Empty window", "color": "rose", "type": "event", "mode": "solo",
                "starts_at": starts, "ends_at": starts,
            }),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["error"], "ends_before_start");

        let (status, created) = create_solo(
            &app,
            owner,
            json!({
                "name": "Event", "color": "rose", "type": "event", "mode": "solo",
                "starts_at": starts, "ends_at": ends,
            }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        let uri = ws_uri(created["slug"].as_str().unwrap());
        let id: Uuid = created["id"].as_str().unwrap().parse().unwrap();
        let before = stored(&db, id).await.updated_at;

        // Only one bound moves, and it is the stored start it now precedes.
        let (status, body) = request_as(
            &app,
            "PATCH",
            &uri,
            Some(owner),
            Some(json!({ "ends_at": "2026-10-31T10:00:00Z" })),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["error"], "ends_before_start");
        assert_eq!(
            stored(&db, id).await.updated_at,
            before,
            "a refused PATCH writes nothing"
        );

        // Null clears the date; absent leaves the other one alone.
        let (status, body) = request_as(
            &app,
            "PATCH",
            &uri,
            Some(owner),
            Some(json!({ "starts_at": null })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["starts_at"], Value::Null);
        assert!(body["ends_at"].is_string(), "ends_at must be untouched");

        let after = stored(&db, id).await.updated_at;
        assert!(
            after > before,
            "updated_at must move: {before} then {after}"
        );
    }

    /// Archived last, favourites included, then favourites first, then by
    /// name ignoring case. `echo` is the case that tells the two possible keys
    /// apart: a favourite, but archived, so it comes after every active
    /// workspace, favourite or not. Seven workspaces owned by a fresh user, so
    /// the list is exactly these and a shuffled order cannot pass by chance
    /// (1 in 5040).
    #[tokio::test]
    async fn the_list_is_sorted_archived_last_then_favorites_first_then_by_name() {
        let db = db().await;
        let app = app(&db).await;
        let owner = create_user(&db).await;

        // (name, favourite, archived)
        let rows = [
            ("golf", false, false),
            ("charlie", false, true),
            ("echo", true, true),
            ("Foxtrot", false, false),
            ("delta", true, false),
            ("alpha", false, false),
            ("bravo", true, false),
        ];

        for (name, favorite, archived) in rows {
            let workspace = create_solo_workspace(&db, owner, name).await;
            if favorite {
                workspace_service::add_favorite(&db, owner, workspace.id)
                    .await
                    .expect("the favourite must be written");
            }
            if archived {
                workspaces::ActiveModel {
                    id: Set(workspace.id),
                    archived: Set(true),
                    ..Default::default()
                }
                .update(&db)
                .await
                .expect("the archive flag must be written");
            }
        }

        let (status, body) = request_as(&app, "GET", "/api/v1/workspaces", Some(owner), None).await;
        assert_eq!(status, StatusCode::OK);

        let names: Vec<&str> = body
            .as_array()
            .expect("the list is an array")
            .iter()
            .map(|summary| summary["name"].as_str().unwrap())
            .collect();

        assert_eq!(
            names,
            [
                "bravo", "delta", "alpha", "Foxtrot", "golf", "echo", "charlie"
            ]
        );

        let echo = names.iter().position(|name| *name == "echo").unwrap();
        let alpha = names.iter().position(|name| *name == "alpha").unwrap();
        assert!(
            echo > alpha,
            "an archived favourite must come after an active non-favourite"
        );
    }

    #[tokio::test]
    async fn a_viewer_can_list_the_members_without_the_excluded_one() {
        let db = db().await;
        let app = app(&db).await;
        let f = team_fixture(&db).await;

        workspace_service::exclude_member(&db, f.workspace.id, f.member)
            .await
            .expect("the exclusion must be written");

        let (status, body) = request_as(
            &app,
            "GET",
            &format!("{}/members", ws_uri(&f.workspace.slug)),
            Some(f.viewer),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let ids: Vec<Uuid> = body
            .as_array()
            .expect("the roster is an array")
            .iter()
            .map(|member| member["user_id"].as_str().unwrap().parse().unwrap())
            .collect();

        for listed in [f.captain, f.admin, f.viewer] {
            assert!(ids.contains(&listed), "{listed} must be listed");
        }
        assert!(
            !ids.contains(&f.member),
            "the excluded member must not be listed"
        );
        assert_eq!(ids.len(), 3);
    }

    /// Deleting a workspace takes its avatar out of the bucket too: once the
    /// row is gone, nothing would name those objects again.
    #[tokio::test]
    async fn deleting_a_workspace_purges_its_avatar() {
        let db = db().await;
        let app = app(&db).await;
        let f = team_fixture(&db).await;
        let version = upload_avatar(&db, AvatarKind::Workspace, f.workspace.id).await;
        assert!(
            avatar_stored(AvatarKind::Workspace, f.workspace.id, version).await,
            "control: the avatar is in the bucket"
        );

        let (status, _) = request_as(
            &app,
            "DELETE",
            &ws_uri(&f.workspace.slug),
            Some(f.captain),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);

        assert!(
            !avatar_stored(AvatarKind::Workspace, f.workspace.id, version).await,
            "the deleted workspace's avatar must be purged"
        );
    }

    #[tokio::test]
    async fn the_list_needs_a_session() {
        let db = db().await;
        let app = app(&db).await;

        let (status, _) = request_as(&app, "GET", "/api/v1/workspaces", None, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    /// Archiving makes a workspace read-only, not unreachable: reading it,
    /// listing its members, favouriting it and deleting it all stay open.
    #[tokio::test]
    async fn an_archived_workspace_can_still_be_read_favorited_and_deleted() {
        let db = db().await;
        let app = app(&db).await;
        let f = team_fixture(&db).await;
        let uri = ws_uri(&f.workspace.slug);
        let favorite = format!("{uri}/favorite");

        let (archived, _) = request_as(
            &app,
            "POST",
            &format!("{uri}/archive"),
            Some(f.captain),
            None,
        )
        .await;
        assert_eq!(archived, StatusCode::OK);

        let (read, body) = request_as(&app, "GET", &uri, Some(f.viewer), None).await;
        assert_eq!(read, StatusCode::OK);
        assert_eq!(body["archived"], true);

        let (members, _) =
            request_as(&app, "GET", &format!("{uri}/members"), Some(f.viewer), None).await;
        assert_eq!(members, StatusCode::OK);

        let (added, _) = request_as(&app, "POST", &favorite, Some(f.viewer), None).await;
        assert_eq!(added, StatusCode::NO_CONTENT);
        let (removed, _) = request_as(&app, "DELETE", &favorite, Some(f.viewer), None).await;
        assert_eq!(removed, StatusCode::NO_CONTENT);

        let (deleted, _) = request_as(&app, "DELETE", &uri, Some(f.captain), None).await;
        assert_eq!(deleted, StatusCode::NO_CONTENT);
        let gone = workspaces::Entity::find_by_id(f.workspace.id)
            .one(&db)
            .await
            .expect("the lookup must succeed");
        assert!(gone.is_none(), "the archived workspace must be deleted");
    }

    /// The other half of favouriting: someone with no access gets the 404 of
    /// a workspace that does not exist, and a member takes their favourite
    /// back, twice over.
    #[tokio::test]
    async fn unfavoriting_needs_access_and_removes_the_row() {
        let db = db().await;
        let app = app(&db).await;
        let f = team_fixture(&db).await;
        let stranger = create_user(&db).await;
        let real = format!("{}/favorite", ws_uri(&f.workspace.slug));
        let random = format!("{}/favorite", ws_uri(&unique("no-such")));

        workspace_service::add_favorite(&db, f.viewer, f.workspace.id)
            .await
            .expect("the favourite must be written");

        let (real_status, real_body) =
            request_as(&app, "DELETE", &real, Some(stranger), None).await;
        let (random_status, random_body) =
            request_as(&app, "DELETE", &random, Some(stranger), None).await;
        assert_eq!(real_status, StatusCode::NOT_FOUND);
        assert_eq!(random_status, StatusCode::NOT_FOUND);
        assert_eq!(real_body, random_body);

        for _ in 0..2 {
            let (status, _) = request_as(&app, "DELETE", &real, Some(f.viewer), None).await;
            assert_eq!(status, StatusCode::NO_CONTENT);
        }
        let row = workspace_favorites::Entity::find_by_id((f.viewer, f.workspace.id))
            .one(&db)
            .await
            .expect("the lookup must succeed");
        assert!(row.is_none(), "the favourite must be gone");
    }

    #[tokio::test]
    async fn archiving_and_unarchiving_move_updated_at() {
        let db = db().await;
        let app = app(&db).await;
        let f = team_fixture(&db).await;
        let uri = ws_uri(&f.workspace.slug);
        let created = stored(&db, f.workspace.id).await.updated_at;

        let (status, _) = request_as(
            &app,
            "POST",
            &format!("{uri}/archive"),
            Some(f.captain),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let archived = stored(&db, f.workspace.id).await.updated_at;
        assert!(
            archived > created,
            "archiving must move updated_at: {created} then {archived}"
        );

        let (status, _) = request_as(
            &app,
            "POST",
            &format!("{uri}/unarchive"),
            Some(f.captain),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let unarchived = stored(&db, f.workspace.id).await.updated_at;
        assert!(
            unarchived > archived,
            "unarchiving must move updated_at: {archived} then {unarchived}"
        );
    }

    /// A segment that does not decode to UTF-8 names no workspace: the same
    /// 404, body included, as a slug that names none.
    #[tokio::test]
    async fn an_undecodable_slug_gets_the_404_of_a_missing_workspace() {
        let db = db().await;
        let app = app(&db).await;
        let user = create_user(&db).await;

        let (undecodable, undecodable_body) =
            request_as(&app, "GET", "/api/v1/workspaces/%FF", Some(user), None).await;
        let (missing, missing_body) =
            request_as(&app, "GET", &ws_uri(&unique("no-such")), Some(user), None).await;

        assert_eq!(undecodable, StatusCode::NOT_FOUND);
        assert_eq!(missing, StatusCode::NOT_FOUND);
        assert_eq!(undecodable_body, missing_body);
    }

    /// Names are not unique. Equal ones, case aside, come back in id order
    /// whatever order the map produced. Six of them, so a shuffled order
    /// cannot pass by chance (1 in 720).
    #[tokio::test]
    async fn workspaces_with_the_same_name_are_listed_by_id() {
        let db = db().await;
        let app = app(&db).await;
        let owner = create_user(&db).await;

        let mut created = Vec::new();
        for name in ["Same", "same", "SAME", "sAme", "saMe", "samE"] {
            created.push(create_solo_workspace(&db, owner, name).await.id);
        }
        created.sort();

        let (status, body) = request_as(&app, "GET", "/api/v1/workspaces", Some(owner), None).await;
        assert_eq!(status, StatusCode::OK);

        let listed: Vec<Uuid> = body
            .as_array()
            .expect("the list is an array")
            .iter()
            .map(|summary| summary["id"].as_str().unwrap().parse().unwrap())
            .collect();
        assert_eq!(listed, created);
    }

    /// The summary GET /workspaces lists for `workspace`, as `user` sees it,
    /// or `None` when it is not listed.
    async fn summary_of(app: &axum::Router, user: Uuid, workspace: Uuid) -> Option<Value> {
        let (status, body) = request_as(app, "GET", "/api/v1/workspaces", Some(user), None).await;
        assert_eq!(status, StatusCode::OK);

        body.as_array()
            .expect("the list is an array")
            .iter()
            .find(|summary| summary["id"] == workspace.to_string())
            .cloned()
    }

    async fn team_row(db: &DatabaseConnection, team_id: Uuid) -> teams::Model {
        teams::Entity::find_by_id(team_id)
            .one(db)
            .await
            .expect("the lookup must succeed")
            .expect("the team must exist")
    }

    /// Each kind of access lists its own role: an override set on the
    /// workspace wins over the team role, a guest has the role they were
    /// admitted with, an excluded member is not listed at all. Every one of
    /// them sees the team, and a solo workspace has none.
    #[tokio::test]
    async fn the_list_gives_the_callers_role_the_team_and_the_platform() {
        let db = db().await;
        let app = app(&db).await;
        let f = team_fixture(&db).await;
        let avatar = Uuid::now_v7();
        teams::ActiveModel {
            id: Set(f.team_id),
            avatar_version: Set(Some(avatar)),
            ..Default::default()
        }
        .update(&db)
        .await
        .expect("the team avatar must be set");
        let team = team_row(&db, f.team_id).await;

        workspace_service::set_member_override(
            &db,
            f.workspace.id,
            f.member,
            TeamRole::Viewer,
            false,
        )
        .await
        .expect("the override must be written");
        let guest = create_user(&db).await;
        workspace_service::set_member_override(&db, f.workspace.id, guest, TeamRole::Member, true)
            .await
            .expect("the guest must be admitted");
        let excluded = create_user(&db).await;
        add_team_member(&db, f.team_id, excluded, TeamRole::Member).await;
        workspace_service::exclude_member(&db, f.workspace.id, excluded)
            .await
            .expect("the exclusion must be written");

        // The team's avatar goes to its members; a guest is not one.
        let expected_team = |avatar_version: Option<Uuid>| {
            json!({
                "id": team.id,
                "slug": team.slug,
                "name": team.name,
                "color": "blue",
                "avatar_version": avatar_version,
            })
        };
        for (user, role, avatar_version) in [
            (f.captain, "captain", Some(avatar)),
            (f.admin, "admin", Some(avatar)),
            (f.member, "viewer", Some(avatar)),
            (f.viewer, "viewer", Some(avatar)),
            (guest, "member", None),
        ] {
            let summary = summary_of(&app, user, f.workspace.id)
                .await
                .unwrap_or_else(|| panic!("listed for the {role}"));
            assert_eq!(summary["my_role"], role, "{summary}");
            assert_eq!(summary["team"], expected_team(avatar_version), "{summary}");
            assert_eq!(summary["platform"], Value::Null, "an event without one");
        }
        assert_eq!(summary_of(&app, excluded, f.workspace.id).await, None);

        let solo = create_solo_workspace(&db, f.captain, "Solo").await;
        let summary = summary_of(&app, f.captain, solo.id)
            .await
            .expect("the solo workspace is listed for its owner");
        assert_eq!(summary["my_role"], "owner");
        assert_eq!(summary["team"], Value::Null);
        assert_eq!(summary["platform"], "htb");

        // The column's spelling is not the API's.
        workspaces::ActiveModel {
            id: Set(solo.id),
            platform: Set(Some("rootme".to_owned())),
            ..Default::default()
        }
        .update(&db)
        .await
        .expect("the platform must be set");
        let summary = summary_of(&app, f.captain, solo.id).await.expect("listed");
        assert_eq!(summary["platform"], "root_me");
    }

    /// A guest is shown the team's name in the list, and nothing more: every
    /// route of the team answers them exactly as it answers a stranger, and
    /// the team is not among theirs.
    #[tokio::test]
    async fn a_guest_sees_the_team_in_the_list_and_none_of_its_routes() {
        let db = db().await;
        let app = app(&db).await;
        let f = team_fixture(&db).await;
        let guest = create_user(&db).await;
        workspace_service::set_member_override(&db, f.workspace.id, guest, TeamRole::Viewer, true)
            .await
            .expect("the guest must be admitted");
        let stranger = create_user(&db).await;
        let team = team_row(&db, f.team_id).await;

        let summary = summary_of(&app, guest, f.workspace.id)
            .await
            .expect("the workspace is listed for its guest");
        assert_eq!(summary["team"]["name"], team.name);
        assert_eq!(summary["team"]["slug"], team.slug);

        for suffix in ["", "/members", "/invitations"] {
            let uri = format!("/api/v1/teams/{}{suffix}", team.slug);
            let (status, body) = request_as(&app, "GET", &uri, Some(guest), None).await;
            let (stranger_status, stranger_body) =
                request_as(&app, "GET", &uri, Some(stranger), None).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "GET {uri}");
            assert_eq!(
                (status, &body),
                (stranger_status, &stranger_body),
                "GET {uri}"
            );
        }
        // Control: the same routes answer a member of the team.
        let (status, _) = request_as(
            &app,
            "GET",
            &format!("/api/v1/teams/{}", team.slug),
            Some(f.viewer),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, teams) = request_as(&app, "GET", "/api/v1/teams", Some(guest), None).await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            !teams
                .as_array()
                .expect("a list")
                .iter()
                .any(|t| t["id"] == f.team_id.to_string()),
            "the team is not among the guest's teams"
        );
    }

    /// The list announces an avatar only to a reader who can load it: the
    /// team's to its members and not to a guest, whose request for it is
    /// refused, and the workspace's own to everyone it lists it for, a guest
    /// included.
    #[tokio::test]
    async fn the_list_announces_only_the_avatars_its_reader_can_load() {
        let db = db().await;
        let app = app(&db).await;
        let f = team_fixture(&db).await;
        let guest = create_user(&db).await;
        workspace_service::set_member_override(&db, f.workspace.id, guest, TeamRole::Viewer, true)
            .await
            .expect("the guest must be admitted");
        let team_avatar = upload_avatar(&db, AvatarKind::Team, f.team_id).await;
        let workspace_avatar = upload_avatar(&db, AvatarKind::Workspace, f.workspace.id).await;

        for (reader, who) in [(f.captain, "the Captain"), (f.viewer, "a Viewer")] {
            let summary = summary_of(&app, reader, f.workspace.id)
                .await
                .expect("listed");
            assert_eq!(
                summary["team"]["avatar_version"],
                json!(team_avatar),
                "{who}"
            );
            assert_eq!(
                avatar_status(&app, reader, "teams", f.team_id, team_avatar).await,
                StatusCode::OK,
                "{who} loads the team avatar announced"
            );
        }

        let summary = summary_of(&app, guest, f.workspace.id)
            .await
            .expect("listed");
        assert_eq!(summary["team"]["avatar_version"], Value::Null);
        assert_eq!(
            avatar_status(&app, guest, "teams", f.team_id, team_avatar).await,
            StatusCode::NOT_FOUND,
            "the version withheld is one the guest could not load"
        );
        assert_eq!(summary["avatar_version"], json!(workspace_avatar));
        assert_eq!(
            avatar_status(&app, guest, "workspaces", f.workspace.id, workspace_avatar).await,
            StatusCode::OK,
            "the guest loads the workspace avatar announced"
        );
    }

    /// One more of each kind of workspace for `user`: a solo one, one on a
    /// team they are a Member of with an override, one excluded there, and
    /// one where they are a guest of another team.
    async fn one_of_each_kind(db: &DatabaseConnection, user: Uuid) {
        create_solo_workspace(db, user, "Solo").await;

        let captain = create_user(db).await;
        let own_team = create_team(db, captain).await;
        add_team_member(db, own_team, user, TeamRole::Member).await;
        let overridden = create_team_workspace(db, own_team, "Overridden").await;
        workspace_service::set_member_override(db, overridden.id, user, TeamRole::Viewer, false)
            .await
            .expect("the override must be written");
        create_team_workspace(db, own_team, "Inherited").await;
        let excluded = create_team_workspace(db, own_team, "Excluded").await;
        workspace_service::exclude_member(db, excluded.id, user)
            .await
            .expect("the exclusion must be written");

        let other_team = create_team(db, captain).await;
        let hosted = create_team_workspace(db, other_team, "Hosted").await;
        workspace_service::set_member_override(db, hosted.id, user, TeamRole::Member, true)
            .await
            .expect("the guest must be admitted");
    }

    /// The list costs as many queries for four workspaces as for sixteen,
    /// teams and guests included: every row is read in bulk and each
    /// workspace resolved in memory.
    #[tokio::test]
    async fn the_list_costs_a_fixed_number_of_queries() {
        let db = db().await;
        let user = create_user(&db).await;
        let queries = Arc::new(AtomicUsize::new(0));
        let mut metered = crate::test_support::db().await;
        let counter = queries.clone();
        metered.set_metric_callback(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
        });

        let mut costs = Vec::new();
        for round in 0..4 {
            one_of_each_kind(&db, user).await;
            queries.store(0, Ordering::SeqCst);
            let listed = workspace_service::list_workspaces_for_user(&metered, user)
                .await
                .expect("the list must load");
            costs.push(queries.load(Ordering::SeqCst));
            assert_eq!(listed.len(), 4 * (round + 1), "the excluded one left out");
        }

        assert!(costs.iter().all(|cost| *cost == costs[0]), "{costs:?}");
        // Solo, team memberships, team workspaces, rows, guest workspaces,
        // their teams, favourites.
        assert_eq!(costs[0], 7, "{costs:?}");
    }
}

// backend/crates/api/src/routes/workspaces.rs

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use validator::Validate;

use crate::middleware::workspace::{WorkspaceMember, role};
use crate::middleware::{auth::AuthUser, json::JsonBody};
use crate::{error::AppError, state::AppState};
use domain::dto::error_responses::{SimpleErrorResponse, ValidationErrorResponse};
use domain::dto::workspace::{
    CreateWorkspaceInput, EffectiveMember, Membership, UpdateWorkspaceInput, WorkspaceResponse,
    WorkspaceSummary,
};
use domain::entities::workspaces;
use services::permissions_service;
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
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 403, description = "Team member below Admin", body = SimpleErrorResponse),
        (status = 404, description = "Team not found or not a member", body = SimpleErrorResponse),
        (status = 422, description = "Validation failed, invalid ownership, platform required, or ends before start", body = ValidationErrorResponse),
    )
)]
async fn create_workspace(
    State(state): State<AppState>,
    user: AuthUser,
    JsonBody(input): JsonBody<CreateWorkspaceInput>,
) -> Result<(StatusCode, Json<WorkspaceResponse>), AppError> {
    input.validate()?;
    let workspace = workspace_service::create_workspace(&state.db, user.id, input).await?;
    let membership = permissions_service::effective_role(&state.db, user.id, &workspace).await?;
    Ok((
        StatusCode::CREATED,
        Json(to_response(workspace, membership)?),
    ))
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
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 403, description = "Below Admin", body = SimpleErrorResponse),
        (status = 404, description = "Workspace not found, or no access to it", body = SimpleErrorResponse),
        (status = 409, description = "The workspace is archived", body = SimpleErrorResponse),
        (status = 422, description = "Validation failed, or ends before start", body = ValidationErrorResponse),
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
    workspace_service::delete_workspace(&state.db, member.workspace.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/workspaces/{slug}/archive",
    tag = "Workspaces",
    params(("slug" = String, Path, description = "Workspace slug")),
    security(("access_token" = [])),
    responses(
        (status = 200, description = "Workspace archived; idempotent", body = WorkspaceResponse),
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
    params(("slug" = String, Path, description = "Workspace slug")),
    security(("access_token" = [])),
    responses(
        (status = 200, description = "Everyone with access, most senior first; excluded members are not listed", body = [EffectiveMember]),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 404, description = "Workspace not found, or no access to it", body = SimpleErrorResponse),
    )
)]
async fn list_members(
    State(state): State<AppState>,
    member: WorkspaceMember<role::Viewer>,
) -> Result<Json<Vec<EffectiveMember>>, AppError> {
    let members = workspace_service::list_effective_members(&state.db, &member.workspace).await?;
    Ok(Json(members))
}

#[cfg(test)]
mod tests {
    use crate::test_support::{
        add_team_member, app, create_solo_workspace, create_team, create_team_workspace,
        create_user, db, request_as, unique,
    };
    use axum::http::StatusCode;
    use domain::dto::team::TeamRole;
    use domain::entities::{workspace_favorites, workspaces};
    use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, Set};
    use serde_json::{Value, json};
    use services::workspace_service;
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
}

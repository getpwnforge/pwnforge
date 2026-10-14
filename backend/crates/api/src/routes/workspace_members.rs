// backend/crates/api/src/routes/workspace_members.rs
//
// The writes on a team workspace's members: a role for one workspace, an
// exclusion from it, a guest's removal. Every rule on who may do what to whom
// is in `workspace_service::write_member`; these handlers only name the
// write. Guests are not added here: they arrive through an accepted
// invitation.

use std::collections::HashMap;

use axum::{
    Router,
    extract::{FromRequestParts, Path, State},
    http::{StatusCode, request::Parts},
    routing::{delete, patch, post},
};
use uuid::Uuid;

use crate::middleware::json::JsonBody;
use crate::middleware::workspace::{WorkspaceMember, role};
use crate::{error::AppError, state::AppState};
use domain::dto::error_responses::{SimpleErrorResponse, UnprocessableErrorResponse};
use domain::dto::workspace::SetMemberRoleRequest;
use services::workspace_service::{self, MemberWrite};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/workspaces/{slug}/members/{user_id}", patch(set_role))
        .route(
            "/workspaces/{slug}/members/{user_id}/role-override",
            delete(clear_role_override),
        )
        .route(
            "/workspaces/{slug}/members/{user_id}/exclusion",
            post(exclude).delete(lift_exclusion),
        )
        .route("/workspaces/{slug}/guests/{user_id}", delete(remove_guest))
        .route("/workspaces/{slug}/leave", post(leave))
}

/// The `{user_id}` segment, read without assuming it is the route's only
/// parameter, as the `WorkspaceMember` extractor reads `{slug}`.
///
/// One that does not parse names no member, so it is handed on as the nil
/// id, which names no account: it gets the answer an unknown member gets,
/// after the workspace's own refusals, rather than a 400 of its own.
struct Target(Uuid);

impl FromRequestParts<AppState> for Target {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let params = Path::<HashMap<String, String>>::from_request_parts(parts, state)
            .await
            .map(|Path(params)| params)
            .unwrap_or_default();

        Ok(Self(
            params
                .get("user_id")
                .and_then(|id| Uuid::parse_str(id).ok())
                .unwrap_or(Uuid::nil()),
        ))
    }
}

async fn write(
    state: &AppState,
    member: &WorkspaceMember<role::Admin>,
    Target(target): Target,
    write: MemberWrite,
) -> Result<StatusCode, AppError> {
    workspace_service::write_member(
        &state.db,
        member.workspace.id,
        member.user_id,
        target,
        write,
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    patch,
    path = "/api/v1/workspaces/{slug}/members/{user_id}",
    tag = "Workspaces",
    params(
        ("slug" = String, Path, description = "Workspace slug"),
        ("user_id" = Uuid, Path, description = "The member or guest"),
    ),
    request_body = SetMemberRoleRequest,
    security(("access_token" = [])),
    responses(
        (status = 204, description = "Role set: an override of the team role for a team member, the role itself for a guest"),
        (status = 400, description = "malformed_json or invalid_body: the body is not JSON, or could not be read", body = SimpleErrorResponse),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 403, description = "insufficient_role: below Admin, or an Admin whose target is at or above Admin before or after the change. target_is_self, target_is_captain", body = SimpleErrorResponse),
        (status = 404, description = "Workspace not found, or no access to it. member_not_found: neither on the team nor a guest", body = SimpleErrorResponse),
        (status = 409, description = "workspace_is_solo or workspace_archived", body = SimpleErrorResponse),
        (status = 415, description = "unsupported_media_type: Content-Type is not application/json", body = SimpleErrorResponse),
        (status = 422, description = "invalid_body (captain or an unknown role), or guest_role_above_member", body = UnprocessableErrorResponse),
    )
)]
async fn set_role(
    State(state): State<AppState>,
    member: WorkspaceMember<role::Admin>,
    target: Target,
    JsonBody(input): JsonBody<SetMemberRoleRequest>,
) -> Result<StatusCode, AppError> {
    write(
        &state,
        &member,
        target,
        MemberWrite::SetRole(input.role.into()),
    )
    .await
}

#[utoipa::path(
    delete,
    path = "/api/v1/workspaces/{slug}/members/{user_id}/role-override",
    tag = "Workspaces",
    params(
        ("slug" = String, Path, description = "Workspace slug"),
        ("user_id" = Uuid, Path, description = "The team member"),
    ),
    security(("access_token" = [])),
    responses(
        (status = 204, description = "Back to the team role; idempotent, and an exclusion is left in place"),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 403, description = "insufficient_role: below Admin, or an Admin whose target is at or above Admin before or after the change. target_is_self, target_is_captain", body = SimpleErrorResponse),
        (status = 404, description = "Workspace not found, or no access to it. member_not_found: neither on the team nor a guest", body = SimpleErrorResponse),
        (status = 409, description = "workspace_is_solo or workspace_archived", body = SimpleErrorResponse),
        (status = 422, description = "target_not_in_team: a guest, who has no team role to go back to", body = UnprocessableErrorResponse),
    )
)]
async fn clear_role_override(
    State(state): State<AppState>,
    member: WorkspaceMember<role::Admin>,
    target: Target,
) -> Result<StatusCode, AppError> {
    write(&state, &member, target, MemberWrite::ClearRoleOverride).await
}

#[utoipa::path(
    post,
    path = "/api/v1/workspaces/{slug}/members/{user_id}/exclusion",
    tag = "Workspaces",
    params(
        ("slug" = String, Path, description = "Workspace slug"),
        ("user_id" = Uuid, Path, description = "The team member"),
    ),
    security(("access_token" = [])),
    responses(
        (status = 204, description = "Excluded from this workspace, team membership kept; idempotent. Allowed on an archived workspace, since it only takes access away"),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 403, description = "insufficient_role: below Admin, or an Admin whose target is at or above Admin before the change. target_is_self, target_is_captain", body = SimpleErrorResponse),
        (status = 404, description = "Workspace not found, or no access to it. member_not_found: neither on the team nor a guest", body = SimpleErrorResponse),
        (status = 409, description = "workspace_is_solo", body = SimpleErrorResponse),
        (status = 422, description = "target_not_in_team: a guest, who is removed rather than excluded", body = UnprocessableErrorResponse),
    )
)]
async fn exclude(
    State(state): State<AppState>,
    member: WorkspaceMember<role::Admin>,
    target: Target,
) -> Result<StatusCode, AppError> {
    write(&state, &member, target, MemberWrite::Exclude).await
}

#[utoipa::path(
    delete,
    path = "/api/v1/workspaces/{slug}/members/{user_id}/exclusion",
    tag = "Workspaces",
    params(
        ("slug" = String, Path, description = "Workspace slug"),
        ("user_id" = Uuid, Path, description = "The team member"),
    ),
    security(("access_token" = [])),
    responses(
        (status = 204, description = "Exclusion lifted, back to the team role; idempotent, and a role override is left in place"),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 403, description = "insufficient_role: below Admin, or an Admin whose target is at or above Admin after the change. target_is_self, target_is_captain", body = SimpleErrorResponse),
        (status = 404, description = "Workspace not found, or no access to it. member_not_found: neither on the team nor a guest", body = SimpleErrorResponse),
        (status = 409, description = "workspace_is_solo or workspace_archived", body = SimpleErrorResponse),
        (status = 422, description = "target_not_in_team: a guest", body = UnprocessableErrorResponse),
    )
)]
async fn lift_exclusion(
    State(state): State<AppState>,
    member: WorkspaceMember<role::Admin>,
    target: Target,
) -> Result<StatusCode, AppError> {
    write(&state, &member, target, MemberWrite::LiftExclusion).await
}

#[utoipa::path(
    delete,
    path = "/api/v1/workspaces/{slug}/guests/{user_id}",
    tag = "Workspaces",
    params(
        ("slug" = String, Path, description = "Workspace slug"),
        ("user_id" = Uuid, Path, description = "The guest"),
    ),
    security(("access_token" = [])),
    responses(
        (status = 204, description = "The guest no longer has access. Allowed on an archived workspace, since it only takes access away"),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 403, description = "insufficient_role: below Admin. target_is_self", body = SimpleErrorResponse),
        (status = 404, description = "Workspace not found, or no access to it. member_not_found: neither on the team nor a guest", body = SimpleErrorResponse),
        (status = 409, description = "workspace_is_solo, or target_already_in_team: a team member, excluded rather than removed", body = SimpleErrorResponse),
    )
)]
async fn remove_guest(
    State(state): State<AppState>,
    member: WorkspaceMember<role::Admin>,
    target: Target,
) -> Result<StatusCode, AppError> {
    write(&state, &member, target, MemberWrite::RemoveGuest).await
}

#[utoipa::path(
    post,
    path = "/api/v1/workspaces/{slug}/leave",
    tag = "Workspaces",
    params(("slug" = String, Path, description = "Workspace slug")),
    security(("access_token" = [])),
    responses(
        (status = 204, description = "The caller no longer has access: a guest's place is removed, a team member is excluded, their team membership kept, until the Captain or an Admin lifts it. The one member write made on oneself, allowed on an archived workspace"),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 404, description = "Workspace not found, or no access to it", body = SimpleErrorResponse),
        (status = 409, description = "workspace_is_solo, or captain_must_transfer: the Captain holds every workspace of the team and cannot leave one", body = SimpleErrorResponse),
    )
)]
async fn leave(
    State(state): State<AppState>,
    member: WorkspaceMember<role::Viewer>,
) -> Result<StatusCode, AppError> {
    workspace_service::leave_workspace(&state.db, member.workspace.id, member.user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use crate::test_support::{
        add_team_member, app, create_solo_workspace, create_team, create_team_workspace,
        create_user, db, request_as,
    };
    use axum::http::StatusCode;
    use domain::dto::team::TeamRole;
    use domain::entities::workspaces;
    use sea_orm::DatabaseConnection;
    use serde_json::{Value, json};
    use services::workspace_service;
    use uuid::Uuid;

    /// A team workspace with its Captain, two Admins, a Member, a Viewer and
    /// a guest admitted as Member, all created for the test.
    struct Fixture {
        workspace: workspaces::Model,
        captain: Uuid,
        admin: Uuid,
        other_admin: Uuid,
        member: Uuid,
        viewer: Uuid,
        guest: Uuid,
    }

    async fn fixture(db: &DatabaseConnection) -> Fixture {
        let captain = create_user(db).await;
        let team_id = create_team(db, captain).await;
        let mut roles = Vec::new();
        for role in [
            TeamRole::Admin,
            TeamRole::Admin,
            TeamRole::Member,
            TeamRole::Viewer,
        ] {
            let user = create_user(db).await;
            add_team_member(db, team_id, user, role).await;
            roles.push(user);
        }
        let workspace = create_team_workspace(db, team_id, "Members").await;
        let guest = admit_guest(db, &workspace).await;

        Fixture {
            workspace,
            captain,
            admin: roles[0],
            other_admin: roles[1],
            member: roles[2],
            viewer: roles[3],
            guest,
        }
    }

    async fn admit_guest(db: &DatabaseConnection, workspace: &workspaces::Model) -> Uuid {
        let guest = create_user(db).await;
        workspace_service::set_member_override(db, workspace.id, guest, TeamRole::Member, true)
            .await
            .expect("the guest must be admitted");
        guest
    }

    fn member_uri(f: &Fixture, target: impl std::fmt::Display, suffix: &str) -> String {
        format!(
            "/api/v1/workspaces/{}/members/{target}{suffix}",
            f.workspace.slug
        )
    }

    fn guest_uri(f: &Fixture, target: Uuid) -> String {
        format!("/api/v1/workspaces/{}/guests/{target}", f.workspace.slug)
    }

    async fn set_role(
        app: &axum::Router,
        f: &Fixture,
        caller: Uuid,
        target: Uuid,
        role: &str,
    ) -> (StatusCode, Value) {
        request_as(
            app,
            "PATCH",
            &member_uri(f, target, ""),
            Some(caller),
            Some(json!({ "role": role })),
        )
        .await
    }

    async fn send(
        app: &axum::Router,
        method: &str,
        uri: &str,
        caller: Uuid,
    ) -> (StatusCode, Value) {
        request_as(app, method, uri, Some(caller), None).await
    }

    /// The role GET /members lists for `user`, with whether they are a guest,
    /// or `None` when they are not listed.
    async fn listed(app: &axum::Router, f: &Fixture, user: Uuid) -> Option<(String, bool)> {
        let (status, body) = send(
            app,
            "GET",
            &format!("/api/v1/workspaces/{}/members", f.workspace.slug),
            f.captain,
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        body.as_array()
            .expect("the roster is a list")
            .iter()
            .find(|entry| entry["user_id"] == user.to_string())
            .map(|entry| {
                (
                    entry["role"].as_str().expect("a role").to_owned(),
                    entry["is_guest"].as_bool().expect("a guest flag"),
                )
            })
    }

    fn role(name: &str, is_guest: bool) -> Option<(String, bool)> {
        Some((name.to_owned(), is_guest))
    }

    #[tokio::test]
    async fn the_captain_overrides_a_member_up_and_down() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;

        let (status, _) = set_role(&app, &f, f.captain, f.member, "admin").await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(listed(&app, &f, f.member).await, role("admin", false));

        let (status, _) = set_role(&app, &f, f.captain, f.member, "viewer").await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(listed(&app, &f, f.member).await, role("viewer", false));
    }

    #[tokio::test]
    async fn an_admin_overrides_below_admin_up_and_down() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;

        let (status, _) = set_role(&app, &f, f.admin, f.viewer, "member").await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(listed(&app, &f, f.viewer).await, role("member", false));

        let (status, _) = set_role(&app, &f, f.admin, f.viewer, "viewer").await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(listed(&app, &f, f.viewer).await, role("viewer", false));
    }

    #[tokio::test]
    async fn clearing_an_override_gives_back_the_team_role() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;

        for (caller, target, team_role) in [
            (f.captain, f.member, "member"),
            (f.admin, f.viewer, "viewer"),
        ] {
            let overridden = if team_role == "member" {
                "viewer"
            } else {
                "member"
            };
            let (status, _) = set_role(&app, &f, f.captain, target, overridden).await;
            assert_eq!(status, StatusCode::NO_CONTENT);

            let (status, _) = send(
                &app,
                "DELETE",
                &member_uri(&f, target, "/role-override"),
                caller,
            )
            .await;
            assert_eq!(status, StatusCode::NO_CONTENT);
            assert_eq!(listed(&app, &f, target).await, role(team_role, false));
        }
    }

    #[tokio::test]
    async fn excluding_then_lifting_the_exclusion() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;

        for (caller, target, team_role) in [
            (f.captain, f.member, "member"),
            (f.admin, f.viewer, "viewer"),
        ] {
            let (status, _) =
                send(&app, "POST", &member_uri(&f, target, "/exclusion"), caller).await;
            assert_eq!(status, StatusCode::NO_CONTENT);
            assert_eq!(
                listed(&app, &f, target).await,
                None,
                "an excluded member is not listed"
            );
            let (status, _) = send(
                &app,
                "GET",
                &format!("/api/v1/workspaces/{}", f.workspace.slug),
                target,
            )
            .await;
            assert_eq!(
                status,
                StatusCode::NOT_FOUND,
                "and no longer sees the workspace"
            );

            let (status, _) = send(
                &app,
                "DELETE",
                &member_uri(&f, target, "/exclusion"),
                caller,
            )
            .await;
            assert_eq!(status, StatusCode::NO_CONTENT);
            assert_eq!(listed(&app, &f, target).await, role(team_role, false));
        }
    }

    #[tokio::test]
    async fn a_guest_role_is_capped_at_member_and_a_guest_can_be_removed() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;

        let (status, _) = set_role(&app, &f, f.captain, f.guest, "viewer").await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(listed(&app, &f, f.guest).await, role("viewer", true));
        let (status, _) = set_role(&app, &f, f.admin, f.guest, "member").await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(listed(&app, &f, f.guest).await, role("member", true));

        let (status, body) = set_role(&app, &f, f.captain, f.guest, "admin").await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["error"], "guest_role_above_member");
        assert_eq!(listed(&app, &f, f.guest).await, role("member", true));

        let other_guest = admit_guest(&db, &f.workspace).await;
        for (caller, guest) in [(f.admin, f.guest), (f.captain, other_guest)] {
            let (status, _) = send(&app, "DELETE", &guest_uri(&f, guest), caller).await;
            assert_eq!(status, StatusCode::NO_CONTENT);
            assert_eq!(listed(&app, &f, guest).await, None);
            let (status, _) = send(
                &app,
                "GET",
                &format!("/api/v1/workspaces/{}", f.workspace.slug),
                guest,
            )
            .await;
            assert_eq!(
                status,
                StatusCode::NOT_FOUND,
                "a removed guest has no access left"
            );
        }
    }

    #[tokio::test]
    async fn an_admin_cannot_give_admin() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;

        let (status, body) = set_role(&app, &f, f.admin, f.member, "admin").await;

        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["error"], "insufficient_role");
        assert_eq!(listed(&app, &f, f.member).await, role("member", false));
    }

    #[tokio::test]
    async fn an_admin_cannot_act_on_another_admin() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;

        let (status, body) = set_role(&app, &f, f.admin, f.other_admin, "viewer").await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["error"], "insufficient_role");
        let (status, body) = send(
            &app,
            "POST",
            &member_uri(&f, f.other_admin, "/exclusion"),
            f.admin,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["error"], "insufficient_role");

        assert_eq!(listed(&app, &f, f.other_admin).await, role("admin", false));
    }

    /// An Admin team member held below Admin here stays there for an Admin:
    /// clearing the override or lifting the exclusion would hand the role
    /// back. The Captain can.
    #[tokio::test]
    async fn an_admin_cannot_give_back_an_admin_team_role() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;

        let (status, _) = set_role(&app, &f, f.captain, f.other_admin, "viewer").await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (status, body) = send(
            &app,
            "DELETE",
            &member_uri(&f, f.other_admin, "/role-override"),
            f.admin,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["error"], "insufficient_role");
        assert_eq!(listed(&app, &f, f.other_admin).await, role("viewer", false));

        let (status, _) = send(
            &app,
            "POST",
            &member_uri(&f, f.other_admin, "/exclusion"),
            f.captain,
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (status, body) = send(
            &app,
            "DELETE",
            &member_uri(&f, f.other_admin, "/exclusion"),
            f.admin,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["error"], "insufficient_role");
        assert_eq!(
            listed(&app, &f, f.other_admin).await,
            None,
            "still excluded"
        );

        let (status, _) = send(
            &app,
            "DELETE",
            &member_uri(&f, f.other_admin, "/exclusion"),
            f.captain,
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "the Captain can");
        assert_eq!(listed(&app, &f, f.other_admin).await, role("admin", false));
    }

    #[tokio::test]
    async fn nobody_acts_on_the_captain_or_on_themselves() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;

        let (status, body) = set_role(&app, &f, f.admin, f.captain, "viewer").await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["error"], "target_is_captain");
        let (status, body) = send(
            &app,
            "POST",
            &member_uri(&f, f.captain, "/exclusion"),
            f.admin,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["error"], "target_is_captain");

        for (caller, uri, method) in [
            (f.captain, member_uri(&f, f.captain, "/exclusion"), "POST"),
            (f.admin, member_uri(&f, f.admin, "/exclusion"), "POST"),
            (f.admin, member_uri(&f, f.admin, "/role-override"), "DELETE"),
        ] {
            let (status, body) = send(&app, method, &uri, caller).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}");
            assert_eq!(body["error"], "target_is_self", "{method} {uri}");
        }
        let (status, body) = set_role(&app, &f, f.captain, f.captain, "admin").await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["error"], "target_is_self");

        assert_eq!(listed(&app, &f, f.captain).await, role("captain", false));
        assert_eq!(listed(&app, &f, f.admin).await, role("admin", false));
    }

    /// The Captain is out of reach by who they are, not by rank: held below
    /// Admin here by an override written outside these routes, they still
    /// cannot be touched by an Admin, whom the rank rule alone would let
    /// through.
    #[tokio::test]
    async fn the_captain_stays_out_of_reach_when_held_below_admin() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;
        workspace_service::set_member_override(
            &db,
            f.workspace.id,
            f.captain,
            TeamRole::Viewer,
            false,
        )
        .await
        .expect("the override must be written");

        let (status, body) = set_role(&app, &f, f.admin, f.captain, "member").await;

        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["error"], "target_is_captain");
        assert_eq!(listed(&app, &f, f.captain).await, role("viewer", false));
    }

    #[tokio::test]
    async fn members_viewers_and_guests_have_no_access() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;

        for caller in [f.member, f.viewer, f.guest] {
            let (status, body) = set_role(&app, &f, caller, f.viewer, "member").await;
            assert_eq!(status, StatusCode::FORBIDDEN);
            assert_eq!(body["error"], "insufficient_role");
            let (status, _) = send(
                &app,
                "POST",
                &member_uri(&f, f.member, "/exclusion"),
                caller,
            )
            .await;
            assert_eq!(status, StatusCode::FORBIDDEN);
            let (status, _) = send(&app, "DELETE", &guest_uri(&f, f.guest), caller).await;
            assert_eq!(status, StatusCode::FORBIDDEN);
        }

        assert_eq!(listed(&app, &f, f.viewer).await, role("viewer", false));
        assert_eq!(listed(&app, &f, f.guest).await, role("member", true));
    }

    /// Someone off the team and not a guest, an id that names no account and
    /// one that does not parse all get the same answer.
    #[tokio::test]
    async fn an_unknown_target_is_member_not_found_whoever_it_is() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;
        let outsider = create_user(&db).await;

        let mut bodies = Vec::new();
        for target in [
            outsider.to_string(),
            Uuid::now_v7().to_string(),
            "not-a-uuid".to_owned(),
        ] {
            let (status, body) = send(
                &app,
                "POST",
                &member_uri(&f, &target, "/exclusion"),
                f.captain,
            )
            .await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{target}");
            bodies.push(body);
        }
        assert!(
            bodies
                .iter()
                .all(|body| body == &json!({ "error": "member_not_found" }))
        );
    }

    #[tokio::test]
    async fn a_solo_workspace_refuses_every_member_route() {
        let db = db().await;
        let app = app(&db).await;
        let owner = create_user(&db).await;
        let solo = create_solo_workspace(&db, owner, "Solo").await;
        let other = create_user(&db).await;
        let base = format!("/api/v1/workspaces/{}", solo.slug);

        let (status, body) = request_as(
            &app,
            "PATCH",
            &format!("{base}/members/{other}"),
            Some(owner),
            Some(json!({ "role": "viewer" })),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"], "workspace_is_solo");

        for (method, uri) in [
            ("DELETE", format!("{base}/members/{other}/role-override")),
            ("POST", format!("{base}/members/{other}/exclusion")),
            ("DELETE", format!("{base}/members/{other}/exclusion")),
            ("DELETE", format!("{base}/guests/{other}")),
        ] {
            let (status, body) = send(&app, method, &uri, owner).await;
            assert_eq!(status, StatusCode::CONFLICT, "{method} {uri}");
            assert_eq!(body["error"], "workspace_is_solo", "{method} {uri}");
        }
    }

    /// On an archived workspace, read-only protects the content, not who
    /// reaches it: removing a guest and excluding a member go through, while
    /// a write that can grant or change access is refused.
    #[tokio::test]
    async fn an_archived_workspace_allows_only_taking_access_away() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;
        workspace_service::exclude_member(&db, f.workspace.id, f.viewer)
            .await
            .expect("the exclusion must be written");
        workspace_service::archive_workspace(&db, f.workspace.id)
            .await
            .expect("the workspace must archive");

        let (status, _) = send(&app, "DELETE", &guest_uri(&f, f.guest), f.admin).await;
        assert_eq!(status, StatusCode::NO_CONTENT, "a guest is removed");
        assert_eq!(listed(&app, &f, f.guest).await, None);
        let (status, _) = send(
            &app,
            "POST",
            &member_uri(&f, f.member, "/exclusion"),
            f.admin,
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "a member is excluded");
        assert_eq!(listed(&app, &f, f.member).await, None);

        let (status, body) = set_role(&app, &f, f.captain, f.other_admin, "viewer").await;
        assert_eq!(status, StatusCode::CONFLICT, "PATCH");
        assert_eq!(body["error"], "workspace_archived");
        for (uri, target) in [
            (
                member_uri(&f, f.other_admin, "/role-override"),
                "a role override",
            ),
            (member_uri(&f, f.viewer, "/exclusion"), "an exclusion"),
        ] {
            let (status, body) = send(&app, "DELETE", &uri, f.captain).await;
            assert_eq!(status, StatusCode::CONFLICT, "removing {target}");
            assert_eq!(body["error"], "workspace_archived", "removing {target}");
        }

        assert_eq!(listed(&app, &f, f.other_admin).await, role("admin", false));
        assert_eq!(listed(&app, &f, f.viewer).await, None, "still excluded");
    }

    #[tokio::test]
    async fn an_invalid_role_is_422() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;

        for invalid in ["captain", "superuser"] {
            let (status, body) = set_role(&app, &f, f.captain, f.member, invalid).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{invalid}");
            assert_eq!(body["error"], "invalid_body", "{invalid}");
        }
        assert_eq!(listed(&app, &f, f.member).await, role("member", false));
    }

    /// Exclusions and role overrides are for team members, removal for
    /// guests.
    #[tokio::test]
    async fn a_write_for_the_other_kind_of_member_is_refused() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;

        for suffix in ["/exclusion", "/role-override"] {
            let method = if suffix == "/exclusion" {
                "POST"
            } else {
                "DELETE"
            };
            let (status, body) =
                send(&app, method, &member_uri(&f, f.guest, suffix), f.admin).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{suffix}");
            assert_eq!(body["error"], "target_not_in_team", "{suffix}");
        }
        let (status, body) = send(&app, "DELETE", &guest_uri(&f, f.member), f.admin).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"], "target_already_in_team");

        assert_eq!(listed(&app, &f, f.guest).await, role("member", true));
        assert_eq!(listed(&app, &f, f.member).await, role("member", false));
    }

    /// On one route: no session, then no access, then a role below Admin,
    /// then the workspace's state, and only then the target. Lifting an
    /// exclusion can grant access, so an archived workspace refuses it.
    #[tokio::test]
    async fn refusals_come_in_order() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;
        let stranger = create_user(&db).await;
        let unknown = Uuid::now_v7();
        let uri = member_uri(&f, unknown, "/exclusion");

        let (status, _) = request_as(&app, "DELETE", &uri, None, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        let (status, body) = send(&app, "DELETE", &uri, f.admin).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(
            body["error"], "member_not_found",
            "control before archiving"
        );

        workspace_service::archive_workspace(&db, f.workspace.id)
            .await
            .expect("the workspace must archive");

        let (status, body) = send(&app, "DELETE", &uri, stranger).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"], "workspace_not_found");
        let (status, _) = send(&app, "DELETE", &uri, f.viewer).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, body) = send(&app, "DELETE", &uri, f.admin).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"], "workspace_archived");
    }

    async fn leave(app: &axum::Router, f: &Fixture, caller: Uuid) -> (StatusCode, Value) {
        send(
            app,
            "POST",
            &format!("/api/v1/workspaces/{}/leave", f.workspace.slug),
            caller,
        )
        .await
    }

    /// A guest who leaves is gone; a team member who leaves is excluded,
    /// their team membership kept, and only the Captain or an Admin brings
    /// them back. The Captain cannot leave.
    #[tokio::test]
    async fn leaving_takes_the_caller_out_and_the_captain_cannot() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;

        let (status, _) = leave(&app, &f, f.guest).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(listed(&app, &f, f.guest).await, None);
        assert_eq!(leave(&app, &f, f.guest).await.0, StatusCode::NOT_FOUND);

        let (status, _) = leave(&app, &f, f.admin).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(
            listed(&app, &f, f.admin).await,
            None,
            "excluded, so not listed"
        );
        let lift = member_uri(&f, f.admin, "/exclusion");
        assert_eq!(
            send(&app, "DELETE", &lift, f.admin).await.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            send(&app, "DELETE", &lift, f.other_admin).await.0,
            StatusCode::FORBIDDEN,
            "an Admin cannot give back an Admin's access"
        );
        assert_eq!(
            send(&app, "DELETE", &lift, f.captain).await.0,
            StatusCode::NO_CONTENT
        );
        assert_eq!(listed(&app, &f, f.admin).await, role("admin", false));

        let (status, body) = leave(&app, &f, f.captain).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"], "captain_must_transfer");
        assert_eq!(listed(&app, &f, f.captain).await, role("captain", false));
    }

    /// Leaving only takes access away, so an archived workspace allows it. A
    /// solo workspace has nobody to leave it.
    #[tokio::test]
    async fn leaving_is_allowed_on_an_archived_workspace_and_not_on_a_solo_one() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;
        workspace_service::archive_workspace(&db, f.workspace.id)
            .await
            .expect("the workspace archives");

        assert_eq!(leave(&app, &f, f.member).await.0, StatusCode::NO_CONTENT);
        assert_eq!(leave(&app, &f, f.guest).await.0, StatusCode::NO_CONTENT);
        assert_eq!(listed(&app, &f, f.member).await, None);

        let owner = create_user(&db).await;
        let solo = create_solo_workspace(&db, owner, "Solo").await;
        let (status, body) = send(
            &app,
            "POST",
            &format!("/api/v1/workspaces/{}/leave", solo.slug),
            owner,
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"], "workspace_is_solo");
    }
}

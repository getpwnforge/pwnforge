// backend/crates/api/src/routes/workspace_invitations.rs
//
// Guest invitations to a team workspace, on the model of the team ones: the
// workspace's own routes under `/workspaces/{slug}/invitations`, the
// invitee's under `/workspaces/invitations` and `/workspaces/join`. Every
// rule is in `workspace_invitation_service`; these handlers add the rate
// limits and the `?invited_by` filter.

use std::collections::HashMap;

use axum::{
    Json, Router,
    extract::{FromRequestParts, Path, Query, State},
    http::{StatusCode, request::Parts},
    routing::{delete, get, post},
};
use uuid::Uuid;
use validator::Validate;

use crate::middleware::workspace::{WorkspaceMember, role};
use crate::middleware::{address::require_mailbox, auth::AuthUser, json::JsonBody};
use crate::routes::enforce_invitation_limit;
use crate::{error::AppError, state::AppState};
use domain::dto::error_responses::{
    RateLimitedErrorResponse, SimpleErrorResponse, UnprocessableErrorResponse,
};
use domain::dto::invitation::{InvitationTokenRequest, InvitedByFilter};
use domain::dto::workspace::{
    CreateWorkspaceInvitationRequest, Membership, WorkspaceInvitationPreview,
    WorkspaceInvitationResponse, WorkspaceResponse,
};
use domain::entities::workspaces;
use services::rate_limit_service;
use services::workspace_invitation_service::{self as invitations, InvitationRef};
use services::workspace_service::WorkspaceError;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/workspaces/{slug}/invitations",
            get(list_invitations).post(create_invitation),
        )
        .route(
            "/workspaces/{slug}/invitations/{id}",
            delete(cancel_invitation),
        )
        .route("/workspaces/join", post(accept_invitation_by_token))
        .route(
            "/workspaces/invitations/pending",
            get(list_pending_invitations),
        )
        .route("/workspaces/invitations/preview", post(preview_invitation))
        .route(
            "/workspaces/invitations/{id}/accept",
            post(accept_invitation_by_id),
        )
        .route(
            "/workspaces/invitations/{id}/reject",
            post(reject_invitation_by_id),
        )
}

/// The `{id}` segment next to `{slug}`, read as `workspace_members::Target`
/// reads `{user_id}`: one that does not parse names no invitation, so it
/// gets the answer an unknown one gets, after the workspace's own refusals.
struct InvitationId(Uuid);

impl FromRequestParts<AppState> for InvitationId {
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
                .get("id")
                .and_then(|id| Uuid::parse_str(id).ok())
                .unwrap_or(Uuid::nil()),
        ))
    }
}

/// A stored enum value the DTO cannot represent is corrupt data, as in
/// `workspaces::to_response`.
fn to_response(
    workspace: workspaces::Model,
    membership: Membership,
) -> Result<WorkspaceResponse, AppError> {
    let id = workspace.id;
    WorkspaceResponse::try_from_model(workspace, membership)
        .ok_or_else(|| WorkspaceError::DataIntegrity(id).into())
}

#[utoipa::path(
    post,
    path = "/api/v1/workspaces/{slug}/invitations",
    tag = "Workspaces",
    operation_id = "create_workspace_invitation",
    params(("slug" = String, Path, description = "Workspace slug")),
    request_body = CreateWorkspaceInvitationRequest,
    security(("access_token" = [])),
    responses(
        (status = 201, description = "Invitation created, pending for seven days. By address, the answer is the same whether an account holds it or not, and the mail with the link goes out either way, at most three a day to one address across every team and workspace; past that the invitation is still created, without a mail. The answer carries no token: the link goes to the invitee's mail alone", body = WorkspaceInvitationResponse),
        (status = 400, description = "malformed_json or invalid_body: the body is not JSON, or could not be read", body = SimpleErrorResponse),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 403, description = "insufficient_role: a Viewer, by team role or override, or a guest", body = SimpleErrorResponse),
        (status = 404, description = "Workspace not found, or no access to it. invited_user_not_found: no account has this username", body = SimpleErrorResponse),
        (status = 409, description = "workspace_is_solo, workspace_archived, already_member (on the team or a guest, by account or by an account holding the address verified), or invitation_already_pending", body = SimpleErrorResponse),
        (status = 415, description = "unsupported_media_type: Content-Type is not application/json", body = SimpleErrorResponse),
        (status = 422, description = "validation_failed (invitee_required, invitee_named_twice, or a malformed username or address), or invalid_body (a role other than member or viewer); an address no mail can go to (a..b@example.test, a@[127.0.0.1]) is validation_failed, as a malformed one", body = UnprocessableErrorResponse),
        (status = 429, description = "rate_limited: by username 20 an hour per author and 50 per workspace, by address 10 and 30", body = RateLimitedErrorResponse),
        (status = 503, description = "service_unavailable: the rate limiter cannot be reached", body = SimpleErrorResponse),
    )
)]
async fn create_invitation(
    State(state): State<AppState>,
    member: WorkspaceMember<role::Viewer>,
    JsonBody(input): JsonBody<CreateWorkspaceInvitationRequest>,
) -> Result<(StatusCode, Json<WorkspaceInvitationResponse>), AppError> {
    input.validate()?;
    if let Some(address) = &input.invited_email {
        require_mailbox("invited_email", address)?;
    }

    // After authorization, so nobody spends a workspace's budget without
    // being allowed to invite into it; before the service, so a refused
    // request creates nothing. Every attempt counts, refused ones included.
    invitations::ensure_can_invite(&state.db, &member.workspace, member.user_id).await?;
    let mut redis = state.redis.clone();
    let decision = if input.username.is_some() {
        rate_limit_service::check_workspace_invitation_by_username(
            &mut redis,
            member.user_id,
            member.workspace.id,
        )
        .await
    } else {
        rate_limit_service::check_workspace_invitation_by_email(
            &mut redis,
            member.user_id,
            member.workspace.id,
        )
        .await
    };
    enforce_invitation_limit(decision)?;

    let created = invitations::create_invitation(
        &state.db,
        &mut redis,
        &state.config,
        member.workspace.id,
        member.user_id,
        input,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(created.invitation)))
}

#[utoipa::path(
    get,
    path = "/api/v1/workspaces/{slug}/invitations",
    tag = "Workspaces",
    operation_id = "list_workspace_invitations",
    params(
        ("slug" = String, Path, description = "Workspace slug"),
        ("invited_by" = Option<Uuid>, Query, description = "Only the invitations this account created"),
    ),
    security(("access_token" = [])),
    responses(
        (status = 200, description = "The workspace's invitations, every status, newest first, with their author. The Captain and the Admins see all of them, any other member who may invite only their own. The address is shown only while an invitation is pending; an accepted one names the account that accepted it", body = [WorkspaceInvitationResponse]),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 403, description = "insufficient_role: a Viewer, by team role or override, or a guest", body = SimpleErrorResponse),
        (status = 404, description = "Workspace not found, or no access to it", body = SimpleErrorResponse),
        (status = 409, description = "workspace_is_solo", body = SimpleErrorResponse),
        (status = 422, description = "validation_failed: invited_by is not a uuid", body = UnprocessableErrorResponse),
    )
)]
async fn list_invitations(
    State(state): State<AppState>,
    member: WorkspaceMember<role::Viewer>,
    Query(filter): Query<InvitedByFilter>,
) -> Result<Json<Vec<WorkspaceInvitationResponse>>, AppError> {
    let invited_by = filter.parse()?;
    let mut list =
        invitations::list_invitations(&state.db, &member.workspace, member.user_id).await?;
    if let Some(author) = invited_by {
        list.retain(|invitation| invitation.invited_by == Some(author));
    }
    Ok(Json(list))
}

#[utoipa::path(
    delete,
    path = "/api/v1/workspaces/{slug}/invitations/{id}",
    tag = "Workspaces",
    params(
        ("slug" = String, Path, description = "Workspace slug"),
        ("id" = Uuid, Path, description = "The invitation"),
    ),
    security(("access_token" = [])),
    responses(
        (status = 204, description = "Cancelled: kept in the history as cancelled (revoked), its address erased"),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 404, description = "Workspace not found, or no access to it. invitation_not_found: unknown here, or neither the caller's own nor theirs to see (the Captain and the Admins see them all)", body = SimpleErrorResponse),
        (status = 409, description = "workspace_is_solo, or invitation_not_pending: already accepted, declined, cancelled or expired", body = SimpleErrorResponse),
    )
)]
async fn cancel_invitation(
    State(state): State<AppState>,
    member: WorkspaceMember<role::Viewer>,
    InvitationId(id): InvitationId,
) -> Result<StatusCode, AppError> {
    invitations::cancel_invitation(&state.db, &member.workspace, member.user_id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/workspaces/invitations/pending",
    tag = "Workspaces",
    operation_id = "list_pending_workspace_invitations",
    security(("access_token" = [])),
    responses(
        (status = 200, description = "Pending workspace invitations for the current user: those naming the account, and those sent to an address it has verified. None is ever accepted on its behalf", body = [WorkspaceInvitationResponse]),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
    )
)]
async fn list_pending_invitations(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<WorkspaceInvitationResponse>>, AppError> {
    let list = invitations::list_pending_for_user(&state.db, user.id).await?;
    Ok(Json(list))
}

#[utoipa::path(
    post,
    path = "/api/v1/workspaces/invitations/preview",
    tag = "Workspaces",
    operation_id = "preview_workspace_invitation",
    request_body = InvitationTokenRequest,
    security(("access_token" = [])),
    responses(
        (status = 200, description = "What the invitation behind a link offers: the workspace's name, the author, the role and the expiry", body = WorkspaceInvitationPreview),
        (status = 400, description = "malformed_json or invalid_body: the body is not JSON, or could not be read", body = SimpleErrorResponse),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 404, description = "invitation_not_found: the same for an unknown token, one addressed to another account (or to an address the caller has not verified), and one expired or already answered", body = SimpleErrorResponse),
        (status = 415, description = "unsupported_media_type: Content-Type is not application/json", body = SimpleErrorResponse),
        (status = 422, description = "validation_failed or invalid_body", body = UnprocessableErrorResponse),
    )
)]
async fn preview_invitation(
    State(state): State<AppState>,
    user: AuthUser,
    JsonBody(payload): JsonBody<InvitationTokenRequest>,
) -> Result<Json<WorkspaceInvitationPreview>, AppError> {
    payload.validate()?;
    let preview = invitations::preview(&state.db, &payload.token, user.id).await?;
    Ok(Json(preview))
}

#[utoipa::path(
    post,
    path = "/api/v1/workspaces/join",
    tag = "Workspaces",
    operation_id = "accept_workspace_invitation_by_token",
    request_body = InvitationTokenRequest,
    security(("access_token" = [])),
    responses(
        (status = 200, description = "Joined as a guest with the invited role", body = WorkspaceResponse),
        (status = 400, description = "malformed_json or invalid_body: the body is not JSON, or could not be read", body = SimpleErrorResponse),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 404, description = "invitation_not_found: the same for an unknown token and one addressed to another account (or to an address the caller has not verified)", body = SimpleErrorResponse),
        (status = 409, description = "invitation_not_pending (accepted, declined or cancelled), already_member (joined the team or became a guest meanwhile), or workspace_archived", body = SimpleErrorResponse),
        (status = 415, description = "unsupported_media_type: Content-Type is not application/json", body = SimpleErrorResponse),
        (status = 422, description = "validation_failed, invalid_body, or invitation_expired", body = UnprocessableErrorResponse),
    )
)]
async fn accept_invitation_by_token(
    State(state): State<AppState>,
    user: AuthUser,
    JsonBody(payload): JsonBody<InvitationTokenRequest>,
) -> Result<Json<WorkspaceResponse>, AppError> {
    payload.validate()?;
    let (workspace, membership) =
        invitations::accept(&state.db, InvitationRef::Token(&payload.token), user.id).await?;
    Ok(Json(to_response(workspace, membership)?))
}

#[utoipa::path(
    post,
    path = "/api/v1/workspaces/invitations/{id}/accept",
    tag = "Workspaces",
    operation_id = "accept_workspace_invitation_by_id",
    params(("id" = Uuid, Path, description = "The invitation")),
    security(("access_token" = [])),
    responses(
        (status = 200, description = "Joined as a guest with the invited role", body = WorkspaceResponse),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 404, description = "invitation_not_found: the same for an unknown invitation and one addressed to another account (or to an address the caller has not verified)", body = SimpleErrorResponse),
        (status = 409, description = "invitation_not_pending (accepted, declined or cancelled), already_member (joined the team or became a guest meanwhile), or workspace_archived", body = SimpleErrorResponse),
        (status = 422, description = "invitation_expired", body = UnprocessableErrorResponse),
    )
)]
async fn accept_invitation_by_id(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<WorkspaceResponse>, AppError> {
    let (workspace, membership) =
        invitations::accept(&state.db, InvitationRef::Id(id), user.id).await?;
    Ok(Json(to_response(workspace, membership)?))
}

#[utoipa::path(
    post,
    path = "/api/v1/workspaces/invitations/{id}/reject",
    tag = "Workspaces",
    operation_id = "reject_workspace_invitation_by_id",
    params(("id" = Uuid, Path, description = "The invitation")),
    security(("access_token" = [])),
    responses(
        (status = 204, description = "Declined: kept in the history as declined, its address erased"),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 404, description = "invitation_not_found: the same for an unknown invitation and one addressed to another account (or to an address the caller has not verified)", body = SimpleErrorResponse),
        (status = 409, description = "invitation_not_pending: already accepted, declined or cancelled", body = SimpleErrorResponse),
        (status = 422, description = "invitation_expired", body = UnprocessableErrorResponse),
    )
)]
async fn reject_invitation_by_id(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    invitations::decline(&state.db, id, user.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use crate::test_support::{
        add_team_member, app, avatar_status, create_solo_workspace, create_team,
        create_team_workspace, create_user, db, env_or, request_as, unique, upload_avatar,
    };
    use axum::http::StatusCode;
    use chrono::{Duration, Utc};
    use domain::dto::team::TeamRole;
    use domain::entities::{user_emails, users, workspace_invitations, workspaces};
    use redis::AsyncCommands;
    use sea_orm::{
        ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
    };
    use serde_json::{Value, json};
    use services::storage_service::AvatarKind;
    use services::{auth_service, auth_token_service, token_service, workspace_service};
    use uuid::Uuid;

    /// A team workspace with its Captain, an Admin, a Member, a Viewer, a
    /// Member whose access here is overridden to Viewer, a guest admitted as
    /// Member, and an outsider.
    struct Fixture {
        workspace: workspaces::Model,
        captain: Uuid,
        admin: Uuid,
        member: Uuid,
        viewer: Uuid,
        overridden: Uuid,
        guest: Uuid,
        outsider: Uuid,
    }

    async fn fixture(db: &DatabaseConnection) -> Fixture {
        let captain = create_user(db).await;
        let team_id = create_team(db, captain).await;
        let mut users = Vec::new();
        for role in [
            TeamRole::Admin,
            TeamRole::Member,
            TeamRole::Viewer,
            TeamRole::Member,
        ] {
            let user = create_user(db).await;
            add_team_member(db, team_id, user, role).await;
            users.push(user);
        }
        let workspace = create_team_workspace(db, team_id, "Invitations").await;
        workspace_service::set_member_override(db, workspace.id, users[3], TeamRole::Viewer, false)
            .await
            .expect("the override must be written");
        let guest = create_user(db).await;
        workspace_service::set_member_override(db, workspace.id, guest, TeamRole::Member, true)
            .await
            .expect("the guest must be admitted");

        Fixture {
            workspace,
            captain,
            admin: users[0],
            member: users[1],
            viewer: users[2],
            overridden: users[3],
            guest,
            outsider: create_user(db).await,
        }
    }

    fn fresh_address() -> String {
        format!("{}@example.test", unique("wsinv"))
    }

    /// Gives `user` a verified address and returns it.
    async fn verified_address(db: &DatabaseConnection, user: Uuid) -> String {
        let address = fresh_address();
        add_address(db, user, &address, true).await;
        address
    }

    async fn add_address(
        db: &DatabaseConnection,
        user: Uuid,
        address: &str,
        verified: bool,
    ) -> Uuid {
        user_emails::ActiveModel {
            id: Set(Uuid::now_v7()),
            user_id: Set(user),
            email: Set(address.to_owned()),
            is_primary: Set(true),
            verified_at: Set(verified.then(|| Utc::now().into())),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("the fixture address must insert")
        .id
    }

    /// Gives an invitation a token the test knows, the one its mail would
    /// carry: only the hash is stored, and the answer holds none.
    async fn known_token(db: &DatabaseConnection, invitation: &Value) -> String {
        let token = token_service::generate_opaque_token();
        workspace_invitations::ActiveModel {
            id: Set(serde_json::from_value(invitation["id"].clone()).expect("an id")),
            token: Set(token.hash),
            ..Default::default()
        }
        .update(db)
        .await
        .expect("the token must be set");
        token.secret
    }

    async fn username_of(db: &DatabaseConnection, user: Uuid) -> String {
        users::Entity::find_by_id(user)
            .one(db)
            .await
            .expect("the lookup must succeed")
            .expect("the user exists")
            .username
    }

    fn uri(f: &Fixture, suffix: &str) -> String {
        format!("/api/v1/workspaces/{}{suffix}", f.workspace.slug)
    }

    async fn invite(
        app: &axum::Router,
        f: &Fixture,
        caller: Uuid,
        body: Value,
    ) -> (StatusCode, Value) {
        request_as(
            app,
            "POST",
            &uri(f, "/invitations"),
            Some(caller),
            Some(body),
        )
        .await
    }

    async fn invite_username(
        app: &axum::Router,
        db: &DatabaseConnection,
        f: &Fixture,
        caller: Uuid,
        invitee: Uuid,
        role: &str,
    ) -> Value {
        let username = username_of(db, invitee).await;
        let (status, body) = invite(
            app,
            f,
            caller,
            json!({ "role": role, "username": username }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        body
    }

    async fn list(app: &axum::Router, f: &Fixture, caller: Uuid, query: &str) -> Vec<Value> {
        let (status, body) = request_as(
            app,
            "GET",
            &uri(f, &format!("/invitations{query}")),
            Some(caller),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body.as_array().expect("a list").clone()
    }

    fn id_of(invitation: &Value) -> String {
        invitation["id"].as_str().expect("an id").to_owned()
    }

    async fn pending_ids(app: &axum::Router, user: Uuid) -> Vec<String> {
        let (status, body) = request_as(
            app,
            "GET",
            "/api/v1/workspaces/invitations/pending",
            Some(user),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        body.as_array().expect("a list").iter().map(id_of).collect()
    }

    async fn sees_workspace(app: &axum::Router, f: &Fixture, user: Uuid) -> bool {
        request_as(app, "GET", &uri(f, ""), Some(user), None)
            .await
            .0
            == StatusCode::OK
    }

    async fn redis() -> redis::aio::ConnectionManager {
        let client = redis::Client::open(env_or("REDIS_URL", "redis://127.0.0.1:6379"))
            .expect("REDIS_URL must be valid");
        redis::aio::ConnectionManager::new(client)
            .await
            .expect("a Redis instance must be reachable")
    }

    /// Without the fields that differ between any two invitations.
    fn shape(mut body: Value) -> Value {
        for field in ["id", "token", "created_at", "expires_at", "invited_email"] {
            body.as_object_mut().expect("an object").remove(field);
        }
        body
    }

    /// The author is answered the same whether an account holds the address
    /// or not: nothing in the answer names one.
    #[tokio::test]
    async fn an_invitation_by_address_is_answered_the_same_with_or_without_an_account() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;
        let held = verified_address(&db, f.outsider).await;

        let (held_status, held_body) = invite(
            &app,
            &f,
            f.member,
            json!({ "role": "member", "invited_email": held }),
        )
        .await;
        let (free_status, free_body) = invite(
            &app,
            &f,
            f.member,
            json!({ "role": "member", "invited_email": fresh_address() }),
        )
        .await;

        assert_eq!(held_status, StatusCode::CREATED);
        assert_eq!(free_status, StatusCode::CREATED);
        assert_eq!(held_body["invited_user_id"], Value::Null);
        assert_eq!(held_body["invited_email"], json!(held));
        assert_eq!(shape(held_body), shape(free_body));
    }

    /// Positive: a Member, an Admin and the Captain may invite, and the
    /// invitation is pending for seven days, with its token in this answer
    /// alone and stored hashed.
    #[tokio::test]
    async fn a_member_an_admin_and_the_captain_may_invite() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;

        for caller in [f.member, f.admin, f.captain] {
            let invitee = create_user(&db).await;
            let body = invite_username(&app, &db, &f, caller, invitee, "viewer").await;
            assert_eq!(body["status"], "pending");
            assert_eq!(body["invited_by"], json!(caller));
            let expires_at: chrono::DateTime<Utc> =
                serde_json::from_value(body["expires_at"].clone()).expect("a date");
            assert!(
                (expires_at - (Utc::now() + Duration::days(7)))
                    .num_seconds()
                    .abs()
                    < 60
            );

            assert!(body.get("token").is_none(), "the answer carries no token");
            let id: Uuid = serde_json::from_value(body["id"].clone()).expect("an id");
            let stored = workspace_invitations::Entity::find_by_id(id)
                .one(&db)
                .await
                .expect("the lookup must succeed")
                .expect("the row exists");
            // A SHA-256 in hex, never the 43-character secret the mail carries.
            assert_eq!(stored.token.len(), 64, "the token is stored hashed");
            assert!(
                stored
                    .token
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            );
        }

        let listed = list(&app, &f, f.captain, "").await;
        assert!(
            listed
                .iter()
                .all(|invitation| invitation.get("token").is_none())
        );
    }

    /// A workspace invitation is always targeted, so its answer has no token
    /// field at all, by username or by address: the token goes to the
    /// invitee's mail alone.
    #[tokio::test]
    async fn a_workspace_invitation_is_answered_without_its_token() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;
        let username = username_of(&db, create_user(&db).await).await;

        for body in [
            json!({ "role": "viewer", "username": username }),
            json!({ "role": "viewer", "invited_email": fresh_address() }),
        ] {
            let (status, answer) = invite(&app, &f, f.member, body).await;
            assert_eq!(status, StatusCode::CREATED, "{answer}");
            assert!(answer.get("token").is_none(), "{answer}");
        }
    }

    /// Negative: a Viewer by team role or by override, and a guest, may not
    /// invite (403); an outsider is told the workspace does not exist (404).
    #[tokio::test]
    async fn viewers_guests_and_outsiders_may_not_invite() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;
        let username = username_of(&db, create_user(&db).await).await;

        for (caller, expected) in [
            (f.viewer, StatusCode::FORBIDDEN),
            (f.overridden, StatusCode::FORBIDDEN),
            (f.guest, StatusCode::FORBIDDEN),
            (f.outsider, StatusCode::NOT_FOUND),
        ] {
            let (status, body) = invite(
                &app,
                &f,
                caller,
                json!({ "role": "viewer", "username": username }),
            )
            .await;
            assert_eq!(status, expected, "{body}");
        }
        assert!(
            list(&app, &f, f.captain, "").await.is_empty(),
            "nothing was created"
        );
    }

    /// Negative: the body names one invitee, who exists, with a guest's role.
    #[tokio::test]
    async fn an_invitation_names_one_existing_invitee_with_a_guest_role() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;
        let username = username_of(&db, create_user(&db).await).await;

        for (body, status, code) in [
            (
                json!({ "role": "admin", "username": username }),
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_body",
            ),
            (
                json!({ "role": "captain", "username": username }),
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_body",
            ),
            (
                json!({ "role": "member", "username": format!("n{}", &Uuid::now_v7().simple().to_string()[1..]) }),
                StatusCode::NOT_FOUND,
                "invited_user_not_found",
            ),
            (
                json!({ "role": "member", "username": username, "invited_email": fresh_address() }),
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation_failed",
            ),
            (
                json!({ "role": "member" }),
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation_failed",
            ),
        ] {
            let (got, answer) = invite(&app, &f, f.captain, body).await;
            assert_eq!(
                (got, answer["error"].as_str()),
                (status, Some(code)),
                "{answer}"
            );
        }
    }

    /// Negative: someone already here, on the team or as a guest, named by
    /// account or by an address they have verified, is `already_member`,
    /// and nothing is created.
    #[tokio::test]
    async fn inviting_someone_already_here_is_already_member() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;
        let member_address = verified_address(&db, f.member).await;
        let guest_address = verified_address(&db, f.guest).await;

        for body in [
            json!({ "role": "viewer", "username": username_of(&db, f.member).await }),
            json!({ "role": "viewer", "username": username_of(&db, f.guest).await }),
            json!({ "role": "viewer", "invited_email": member_address }),
            json!({ "role": "viewer", "invited_email": guest_address }),
        ] {
            let (status, answer) = invite(&app, &f, f.captain, body).await;
            assert_eq!(status, StatusCode::CONFLICT);
            assert_eq!(answer["error"], "already_member");
        }
        assert!(
            list(&app, &f, f.captain, "").await.is_empty(),
            "nothing was created"
        );
    }

    /// Negative: one pending invitation per invitee, whatever the case of
    /// the address.
    #[tokio::test]
    async fn a_second_pending_invitation_for_the_same_invitee_is_refused() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;
        let invitee = create_user(&db).await;
        invite_username(&app, &db, &f, f.member, invitee, "viewer").await;
        let address = fresh_address();
        let (status, _) = invite(
            &app,
            &f,
            f.member,
            json!({ "role": "member", "invited_email": address }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);

        for body in [
            json!({ "role": "member", "username": username_of(&db, invitee).await }),
            json!({ "role": "viewer", "invited_email": address.to_uppercase() }),
        ] {
            let (status, answer) = invite(&app, &f, f.admin, body).await;
            assert_eq!(status, StatusCode::CONFLICT);
            assert_eq!(answer["error"], "invitation_already_pending");
        }
    }

    /// Negative: a solo workspace has no guests to invite, on any invitation
    /// route, and an archived one takes no new invitation.
    #[tokio::test]
    async fn a_solo_or_archived_workspace_takes_no_invitation() {
        let db = db().await;
        let app = app(&db).await;
        let owner = create_user(&db).await;
        let solo = create_solo_workspace(&db, owner, "Solo").await;
        let username = username_of(&db, create_user(&db).await).await;
        let base = format!("/api/v1/workspaces/{}/invitations", solo.slug);

        for (method, path, body) in [
            (
                "POST",
                base.clone(),
                Some(json!({ "role": "viewer", "username": username })),
            ),
            ("GET", base.clone(), None),
            ("DELETE", format!("{base}/{}", Uuid::now_v7()), None),
        ] {
            let (status, answer) = request_as(&app, method, &path, Some(owner), body).await;
            assert_eq!(status, StatusCode::CONFLICT, "{method} {answer}");
            assert_eq!(answer["error"], "workspace_is_solo");
        }

        let f = fixture(&db).await;
        workspace_service::archive_workspace(&db, f.workspace.id)
            .await
            .expect("the workspace archives");
        let (status, answer) = invite(
            &app,
            &f,
            f.captain,
            json!({ "role": "viewer", "username": username }),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(answer["error"], "workspace_archived");
    }

    /// Positive: the invitee named by username sees the invitation in their
    /// pending list and accepts it there; they are a guest with the invited
    /// role, the author recorded and listed. Accepting again is refused.
    #[tokio::test]
    async fn the_invitee_accepts_from_the_pending_list() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;
        let invitee = create_user(&db).await;
        let invitation = invite_username(&app, &db, &f, f.member, invitee, "viewer").await;
        let id = id_of(&invitation);

        assert_eq!(pending_ids(&app, invitee).await, vec![id.clone()]);
        assert!(!sees_workspace(&app, &f, invitee).await);

        let accept = format!("/api/v1/workspaces/invitations/{id}/accept");
        let (status, body) = request_as(&app, "POST", &accept, Some(invitee), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["slug"], json!(f.workspace.slug));
        assert!(sees_workspace(&app, &f, invitee).await);

        let (_, roster) = request_as(&app, "GET", &uri(&f, "/members"), Some(f.viewer), None).await;
        let entry = roster
            .as_array()
            .expect("a list")
            .iter()
            .find(|entry| entry["user_id"] == json!(invitee))
            .cloned()
            .expect("the guest is listed");
        assert_eq!(entry["is_guest"], true);
        assert_eq!(entry["role"], "viewer");
        assert_eq!(entry["invited_by"], json!(f.member));

        let (status, body) = request_as(&app, "POST", &accept, Some(invitee), None).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"], "invitation_not_pending");
        assert!(pending_ids(&app, invitee).await.is_empty());
    }

    /// Positive: the mail's link accepts for the account holding the address
    /// verified. Negative: anyone else is told it does not exist, by token
    /// or by id, and so is the account holding the address unverified.
    #[tokio::test]
    async fn the_link_accepts_for_the_invitee_alone() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;
        let address = fresh_address();
        let (_, invitation) = invite(
            &app,
            &f,
            f.member,
            json!({ "role": "member", "invited_email": address }),
        )
        .await;
        let token = known_token(&db, &invitation).await;
        let id = id_of(&invitation);

        let unverified = create_user(&db).await;
        add_address(&db, unverified, &address, false).await;
        let stranger = create_user(&db).await;
        for caller in [unverified, stranger] {
            assert!(pending_ids(&app, caller).await.is_empty());
            for (path, body) in [
                (
                    "/api/v1/workspaces/join".to_owned(),
                    Some(json!({ "token": token })),
                ),
                (format!("/api/v1/workspaces/invitations/{id}/accept"), None),
                (format!("/api/v1/workspaces/invitations/{id}/reject"), None),
            ] {
                let (status, answer) = request_as(&app, "POST", &path, Some(caller), body).await;
                assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
                assert_eq!(answer["error"], "invitation_not_found");
            }
        }
        let (status, _) = request_as(
            &app,
            "POST",
            "/api/v1/workspaces/join",
            Some(stranger),
            Some(json!({ "token": "no-such-token" })),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        // The unverified holder is not the invitee: the address is free to
        // be verified by the account that holds it, here a fresh one.
        user_emails::Entity::delete_many()
            .filter(user_emails::Column::UserId.eq(unverified))
            .exec(&db)
            .await
            .expect("the unverified address goes");
        let invitee = create_user(&db).await;
        add_address(&db, invitee, &address, true).await;
        let (status, body) = request_as(
            &app,
            "POST",
            "/api/v1/workspaces/join",
            Some(invitee),
            Some(json!({ "token": token })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(sees_workspace(&app, &f, invitee).await);
    }

    /// Negative: an invitation expired, cancelled or overtaken (the invitee
    /// joined the team meanwhile) is not accepted.
    #[tokio::test]
    async fn an_invitation_that_cannot_be_answered_is_refused() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;

        let late = create_user(&db).await;
        let expired = invite_username(&app, &db, &f, f.member, late, "viewer").await;
        workspace_invitations::ActiveModel {
            id: Set(serde_json::from_value(expired["id"].clone()).expect("an id")),
            expires_at: Set((Utc::now() - Duration::minutes(1)).into()),
            ..Default::default()
        }
        .update(&db)
        .await
        .expect("the fixture expires");

        let withdrawn_for = create_user(&db).await;
        let withdrawn = invite_username(&app, &db, &f, f.member, withdrawn_for, "viewer").await;
        let (status, _) = request_as(
            &app,
            "DELETE",
            &uri(&f, &format!("/invitations/{}", id_of(&withdrawn))),
            Some(f.member),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);

        let joiner = create_user(&db).await;
        let overtaken = invite_username(&app, &db, &f, f.member, joiner, "viewer").await;
        add_team_member(
            &db,
            f.workspace.team_id.expect("a team"),
            joiner,
            TeamRole::Viewer,
        )
        .await;

        for (caller, invitation, status, code) in [
            (
                late,
                &expired,
                StatusCode::UNPROCESSABLE_ENTITY,
                "invitation_expired",
            ),
            (
                withdrawn_for,
                &withdrawn,
                StatusCode::CONFLICT,
                "invitation_not_pending",
            ),
            (joiner, &overtaken, StatusCode::CONFLICT, "already_member"),
        ] {
            let path = format!(
                "/api/v1/workspaces/invitations/{}/accept",
                id_of(invitation)
            );
            let (got, answer) = request_as(&app, "POST", &path, Some(caller), None).await;
            assert_eq!((got, answer["error"].as_str()), (status, Some(code)));
        }
        assert!(!sees_workspace(&app, &f, late).await);
        assert!(!sees_workspace(&app, &f, withdrawn_for).await);
    }

    /// The preview tells the invitee what the link offers, and nothing else;
    /// an unknown token, someone else's and an expired one get one answer.
    #[tokio::test]
    async fn the_preview_is_for_the_invitee_alone() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;
        let invitee = create_user(&db).await;
        let invitation = invite_username(&app, &db, &f, f.member, invitee, "member").await;
        let token = known_token(&db, &invitation).await;
        let preview = |user: Uuid, token: String| {
            let app = app.clone();
            async move {
                request_as(
                    &app,
                    "POST",
                    "/api/v1/workspaces/invitations/preview",
                    Some(user),
                    Some(json!({ "token": token })),
                )
                .await
            }
        };

        let (status, body) = preview(invitee, token.clone()).await;
        assert_eq!(status, StatusCode::OK);
        let mut keys: Vec<_> = body
            .as_object()
            .expect("an object")
            .keys()
            .cloned()
            .collect();
        keys.sort();
        assert_eq!(
            keys,
            [
                "expires_at",
                "invited_by_username",
                "role",
                "workspace_name"
            ]
        );
        assert_eq!(body["workspace_name"], "Invitations");
        assert_eq!(
            body["invited_by_username"],
            json!(username_of(&db, f.member).await)
        );
        assert_eq!(body["role"], "member");

        let refused = [
            preview(f.outsider, token.clone()).await,
            preview(invitee, "no-such-token".to_owned()).await,
        ];
        workspace_invitations::ActiveModel {
            id: Set(serde_json::from_value(invitation["id"].clone()).expect("an id")),
            expires_at: Set((Utc::now() - Duration::minutes(1)).into()),
            ..Default::default()
        }
        .update(&db)
        .await
        .expect("the fixture expires");
        let expired = preview(invitee, token).await;
        for answer in refused.iter().chain([&expired]) {
            assert_eq!(answer.0, StatusCode::NOT_FOUND);
            assert_eq!(answer.1, expired.1, "one body for every refusal");
        }
    }

    /// Signing up with an invited address and verifying it joins nothing:
    /// the invitation waits in the account's pending list for an explicit
    /// answer.
    #[tokio::test]
    async fn verifying_an_invited_address_accepts_nothing() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;
        let address = fresh_address();
        let (_, invitation) = invite(
            &app,
            &f,
            f.member,
            json!({ "role": "viewer", "invited_email": address }),
        )
        .await;

        let newcomer = create_user(&db).await;
        let email_id = add_address(&db, newcomer, &address, false).await;
        let secret = auth_token_service::issue(
            &db,
            newcomer,
            email_id,
            domain::types::TokenKind::EmailVerification,
        )
        .await
        .expect("a verification token must be issued");
        auth_service::verify_email(&db, &secret)
            .await
            .expect("the address verifies");

        assert!(
            !sees_workspace(&app, &f, newcomer).await,
            "nothing was accepted"
        );
        assert_eq!(pending_ids(&app, newcomer).await, vec![id_of(&invitation)]);

        let path = format!(
            "/api/v1/workspaces/invitations/{}/accept",
            id_of(&invitation)
        );
        let (status, _) = request_as(&app, "POST", &path, Some(newcomer), None).await;
        assert_eq!(status, StatusCode::OK);
        assert!(sees_workspace(&app, &f, newcomer).await);
    }

    /// The Captain, an Admin and the author may cancel a pending invitation;
    /// anyone else on the workspace is told it does not exist. Once
    /// cancelled it is no longer pending.
    #[tokio::test]
    async fn the_captain_an_admin_or_the_author_cancels() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;
        let other_member = create_user(&db).await;
        add_team_member(
            &db,
            f.workspace.team_id.expect("a team"),
            other_member,
            TeamRole::Member,
        )
        .await;

        for caller in [f.captain, f.admin, f.member] {
            let invitation =
                invite_username(&app, &db, &f, f.member, create_user(&db).await, "viewer").await;
            let path = uri(&f, &format!("/invitations/{}", id_of(&invitation)));

            for refused in [other_member, f.viewer, f.guest] {
                let (status, body) = request_as(&app, "DELETE", &path, Some(refused), None).await;
                assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
                assert_eq!(body["error"], "invitation_not_found");
            }
            let (status, _) = request_as(&app, "DELETE", &path, Some(caller), None).await;
            assert_eq!(status, StatusCode::NO_CONTENT);
            let (status, body) = request_as(&app, "DELETE", &path, Some(caller), None).await;
            assert_eq!(status, StatusCode::CONFLICT);
            assert_eq!(body["error"], "invitation_not_pending");
        }
    }

    /// The Captain and the Admins see every invitation with its author; a
    /// member sees only their own; `?invited_by` narrows either. The address
    /// shows while pending and is gone once accepted, which names the
    /// account that accepted.
    #[tokio::test]
    async fn the_invitation_list_shows_each_reader_what_is_theirs_to_see() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;
        let address = fresh_address();
        let (_, by_member) = invite(
            &app,
            &f,
            f.member,
            json!({ "role": "viewer", "invited_email": address }),
        )
        .await;
        let by_admin =
            invite_username(&app, &db, &f, f.admin, create_user(&db).await, "viewer").await;

        let all: Vec<String> = list(&app, &f, f.captain, "")
            .await
            .iter()
            .map(id_of)
            .collect();
        assert_eq!(all.len(), 2);
        assert_eq!(list(&app, &f, f.admin, "").await.len(), 2);
        let own: Vec<String> = list(&app, &f, f.member, "")
            .await
            .iter()
            .map(id_of)
            .collect();
        assert_eq!(own, vec![id_of(&by_member)]);
        let filtered: Vec<String> = list(&app, &f, f.captain, &format!("?invited_by={}", f.admin))
            .await
            .iter()
            .map(id_of)
            .collect();
        assert_eq!(filtered, vec![id_of(&by_admin)]);
        assert_eq!(
            list(&app, &f, f.captain, &format!("?invited_by={}", f.admin)).await[0]["target"],
            "username"
        );
        assert!(
            list(&app, &f, f.member, &format!("?invited_by={}", f.admin))
                .await
                .is_empty()
        );
        let (status, body) = request_as(
            &app,
            "GET",
            &uri(&f, "/invitations?invited_by=nope"),
            Some(f.captain),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["error"], "validation_failed");
        for caller in [f.viewer, f.guest] {
            let (status, _) =
                request_as(&app, "GET", &uri(&f, "/invitations"), Some(caller), None).await;
            assert_eq!(status, StatusCode::FORBIDDEN);
        }

        let listed = list(&app, &f, f.captain, &format!("?invited_by={}", f.member)).await;
        assert_eq!(listed[0]["invited_email"], json!(address));
        assert_eq!(listed[0]["invited_by"], json!(f.member));
        assert_eq!(
            listed[0]["invited_by_username"],
            json!(username_of(&db, f.member).await)
        );

        let invitee = create_user(&db).await;
        add_address(&db, invitee, &address, true).await;
        let path = format!(
            "/api/v1/workspaces/invitations/{}/accept",
            id_of(&by_member)
        );
        let (status, _) = request_as(&app, "POST", &path, Some(invitee), None).await;
        assert_eq!(status, StatusCode::OK);

        let listed = list(&app, &f, f.captain, &format!("?invited_by={}", f.member)).await;
        assert_eq!(listed[0]["status"], "accepted");
        assert_eq!(
            listed[0]["target"], "email",
            "still an invitation by address"
        );
        assert_eq!(
            listed[0]["invited_email"],
            Value::Null,
            "the address is gone"
        );
        assert_eq!(listed[0]["accepted_by"], json!(invitee));
        assert_eq!(
            listed[0]["accepted_username"],
            json!(username_of(&db, invitee).await)
        );

        let (_, roster) = request_as(
            &app,
            "GET",
            &uri(&f, &format!("/members?invited_by={}", f.member)),
            Some(f.viewer),
            None,
        )
        .await;
        let ids: Vec<&Value> = roster
            .as_array()
            .expect("a list")
            .iter()
            .map(|entry| &entry["user_id"])
            .collect();
        assert_eq!(ids, vec![&json!(invitee)]);
    }

    /// A pending invitation past its date reads as expired, address withheld,
    /// before the cleanup has recorded it, and leaves the invitee's list.
    #[tokio::test]
    async fn a_pending_invitation_past_its_date_reads_as_expired_without_its_address() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;
        let invitee = create_user(&db).await;
        let address = verified_address(&db, invitee).await;
        let (_, invitation) = invite(
            &app,
            &f,
            f.member,
            json!({ "role": "viewer", "invited_email": address }),
        )
        .await;
        assert_eq!(pending_ids(&app, invitee).await, vec![id_of(&invitation)]);

        workspace_invitations::ActiveModel {
            id: Set(serde_json::from_value(invitation["id"].clone()).expect("an id")),
            expires_at: Set((Utc::now() - Duration::minutes(1)).into()),
            ..Default::default()
        }
        .update(&db)
        .await
        .expect("the fixture expires");

        let listed = list(&app, &f, f.captain, "").await;
        assert_eq!(listed[0]["status"], "expired");
        assert_eq!(listed[0]["invited_email"], Value::Null);
        assert_eq!(listed[0]["resolved_at"], listed[0]["expires_at"]);
        assert!(pending_ids(&app, invitee).await.is_empty());
    }

    /// 429 past each window, per author and per workspace, on both paths,
    /// and 503 when the limiter cannot answer. Nothing is created then.
    #[tokio::test]
    async fn invitations_are_rate_limited_per_author_and_per_workspace() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;
        let mut redis = redis().await;
        let workspace = f.workspace.id;

        let cases = [
            ("username", format!("author:{}", f.member), 20),
            ("username", format!("workspace:{workspace}"), 50),
            ("email", format!("author:{}", f.member), 10),
            ("email", format!("workspace:{workspace}"), 30),
        ];
        for (path, suffix, limit) in cases {
            let key = format!("ratelimit:invite:workspace:{path}:{suffix}");
            let body = if path == "username" {
                json!({ "role": "viewer", "username": username_of(&db, create_user(&db).await).await })
            } else {
                json!({ "role": "viewer", "invited_email": fresh_address() })
            };

            let _: () = redis
                .set_ex(&key, limit, 3600)
                .await
                .expect("SET must answer");
            let (limited, answer) = invite(&app, &f, f.member, body.clone()).await;
            let _: () = redis
                .set(&key, "not-a-counter")
                .await
                .expect("SET must answer");
            let (unavailable, _) = invite(&app, &f, f.member, body).await;
            let _: () = redis.del(&key).await.expect("DEL must answer");

            assert_eq!(limited, StatusCode::TOO_MANY_REQUESTS, "{key}: {answer}");
            assert_eq!(answer["error"], "rate_limited");
            assert_eq!(unavailable, StatusCode::SERVICE_UNAVAILABLE, "{key}");
        }
        for path in ["username", "email"] {
            for suffix in [
                format!("author:{}", f.member),
                format!("workspace:{workspace}"),
            ] {
                let _: () = redis
                    .del(format!("ratelimit:invite:workspace:{path}:{suffix}"))
                    .await
                    .expect("DEL must answer");
            }
        }
        assert!(
            list(&app, &f, f.captain, "").await.is_empty(),
            "nothing was created"
        );
    }

    /// An address no mail can go to is refused as a malformed one, and no
    /// invitation is written.
    #[tokio::test]
    async fn an_address_no_mail_can_go_to_invites_nobody() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;

        for address in ["a..b@example.test", "a@[127.0.0.1]"] {
            let (status, body) = invite(
                &app,
                &f,
                f.captain,
                json!({ "role": "member", "invited_email": address }),
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
        let written = workspace_invitations::Entity::find()
            .filter(workspace_invitations::Column::WorkspaceId.eq(f.workspace.id))
            .all(&db)
            .await
            .expect("the lookup must succeed");
        assert!(written.is_empty(), "no invitation is written");
    }

    /// The invitee's pending list carries what a monogram and its context
    /// need, the workspace's name and colour and its team's name, and
    /// nothing else than before: the same fields, three added. The avatar
    /// version goes only to a reader the workspace's avatar is served to: the
    /// author's answer and the list of those who may invite carry it, an
    /// invitee with no access gets `null`, and the same invitee once on the
    /// team gets it.
    #[tokio::test]
    async fn a_pending_invitation_announces_only_an_avatar_its_reader_can_load() {
        let db = db().await;
        let app = app(&db).await;
        let f = fixture(&db).await;
        workspaces::ActiveModel {
            id: Set(f.workspace.id),
            color: Set("rose".to_owned()),
            ..Default::default()
        }
        .update(&db)
        .await
        .expect("the workspace must be updated");
        let avatar = upload_avatar(&db, AvatarKind::Workspace, f.workspace.id).await;
        let invitee = create_user(&db).await;

        let created = invite_username(&app, &db, &f, f.admin, invitee, "viewer").await;
        let (status, listed) =
            request_as(&app, "GET", &uri(&f, "/invitations"), Some(f.admin), None).await;
        assert_eq!(status, StatusCode::OK);
        let listed = listed
            .as_array()
            .expect("a list")
            .iter()
            .find(|entry| entry["id"] == created["id"])
            .expect("listed for the Admin")
            .clone();
        for answer in [&created, &listed] {
            assert_eq!(answer["workspace_name"], "Invitations", "{answer}");
            assert_eq!(answer["workspace_color"], "rose", "{answer}");
            assert_eq!(
                answer["workspace_avatar_version"],
                json!(avatar),
                "{answer}"
            );
            assert_eq!(answer["team_name"], "Test Team", "{answer}");
        }
        assert_eq!(
            avatar_status(&app, f.admin, "workspaces", f.workspace.id, avatar).await,
            StatusCode::OK,
            "the Admin loads the avatar announced"
        );

        let pending = |user: Uuid| {
            let app = app.clone();
            let id = created["id"].clone();
            async move {
                let (status, pending) = request_as(
                    &app,
                    "GET",
                    "/api/v1/workspaces/invitations/pending",
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
        assert_eq!(entry["workspace_name"], "Invitations");
        assert_eq!(entry["workspace_color"], "rose");
        assert_eq!(entry["team_name"], "Test Team");
        assert_eq!(entry["workspace_avatar_version"], Value::Null);
        assert_eq!(
            avatar_status(&app, invitee, "workspaces", f.workspace.id, avatar).await,
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
            "workspace_id",
            "workspace_name",
            "workspace_color",
            "workspace_avatar_version",
            "team_name",
            "status",
            "role",
            "target",
            "invited_user_id",
            "invited_username",
            "invited_email",
            "accepted_by",
            "accepted_username",
            "invited_by",
            "invited_by_username",
            "cancel_reason",
            "expires_at",
            "created_at",
            "resolved_at",
        ]
        .into_iter()
        .collect();
        assert_eq!(fields, expected);

        // On the team now, the invitation still pending: the invitee has
        // access, is announced the avatar, and loads it.
        let team_id = f.workspace.team_id.expect("a team workspace");
        add_team_member(&db, team_id, invitee, TeamRole::Viewer).await;
        let entry = pending(invitee).await;
        assert_eq!(entry["workspace_avatar_version"], json!(avatar));
        assert_eq!(
            avatar_status(&app, invitee, "workspaces", f.workspace.id, avatar).await,
            StatusCode::OK
        );
    }
}

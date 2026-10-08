// backend/crates/api/src/routes/avatars.rs
use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::json;
use uuid::Uuid;

use crate::middleware::auth::AuthUser;
use crate::{error::AppError, state::AppState};
use domain::dto::error_responses::{SimpleErrorResponse, UnprocessableErrorResponse};
use domain::dto::team::TeamRole;
use services::avatar_service::{self, AvatarError, MAX_UPLOAD_BYTES, OUTPUT_CONTENT_TYPE};
use services::rate_limit_service::{self, RateLimitDecision};
use services::storage_service::{AvatarKind, AvatarSize};
use services::team_service;
use services::workspace_service::{self, WorkspaceError};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/avatars/{kind}/{id}/{version}/{size}", get(read_avatar))
        .route(
            "/avatars/{kind}/{id}",
            axum::routing::post(upload_avatar).delete(delete_avatar),
        )
        // Only this router gets the raised limit. The global
        // `DefaultBodyLimit` in `app.rs` is 64 KiB, sized for JSON, and the
        // note there says routes that legitimately need more should raise it
        // for themselves rather than everyone inheriting the generous figure.
        //
        // This is the bound that matters in practice: it refuses an oversized
        // body while it is still being read, so nothing past the limit is ever
        // buffered. `avatar_service::render` checks the same figure again for
        // the callers that are not this route.
        .layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES))
}

/// Serves one rendition of an avatar.
///
/// Every request re-checks the caller's permission. Nothing here is served by
/// a presigned URL: a presigned URL is a bearer token with a lifetime of its
/// own, and once issued it cannot be withdrawn when someone is removed from a
/// team. The object store is not reachable from outside the internal network
/// at all, so this handler is the only way in.
#[utoipa::path(
    get,
    path = "/api/v1/avatars/{kind}/{id}/{version}/{size}",
    tag = "Avatars",
    security(("access_token" = [])),
    params(
        ("kind" = String, Path, description = "users, teams or workspaces"),
        ("id" = Uuid, Path, description = "Id of the entity the avatar belongs to"),
        ("version" = Uuid, Path, description = "Current avatar_version; a stale one is a 404"),
        ("size" = String, Path, description = "256.webp or 64.webp"),
    ),
    responses(
        (status = 200, description = "The avatar, always image/webp", content_type = "image/webp"),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 404, description = "No avatar, or the caller may not know this entity exists", body = SimpleErrorResponse),
    )
)]
async fn read_avatar(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((kind, id, version, size)): Path<(String, Uuid, Uuid, String)>,
) -> Result<Response, AppError> {
    let kind = parse_kind(&kind)?;
    let size = parse_size(&size)?;

    authorize_read(&state, &auth, kind, id).await?;

    // The version in the URL is checked against the database rather than used
    // to build a key. A key built from a caller-supplied uuid would be a read
    // of any object under the entity's prefix; checking instead means the URL
    // is a cache key and nothing more.
    let (current, bytes) =
        avatar_service::read_current(&state.db, &state.storage, kind, id, size).await?;

    if current != version {
        // The caller is asking for a superseded version. Those objects are
        // deleted on upload, so there is nothing to serve and nothing to say
        // beyond "not here".
        return Err(AvatarError::NotFound.into());
    }

    Ok(rendition_response(bytes))
}

/// Replaces an avatar.
///
/// The body is the raw image, not a multipart form. That is deliberate: it
/// removes a parser that would otherwise run on attacker-controlled input
/// ahead of the one that has to run anyway, and there is exactly one file, so
/// a multipart envelope carries no information. The request's `Content-Type`
/// is ignored entirely - magic bytes decide, and the uploader controls the
/// header.
#[utoipa::path(
    post,
    path = "/api/v1/avatars/{kind}/{id}",
    tag = "Avatars",
    security(("access_token" = [])),
    request_body(
        content = Vec<u8>,
        description = "Raw PNG, JPEG or WebP, 5 MB maximum. Not multipart; the                        request Content-Type is ignored and the magic bytes decide.",
    ),
    params(
        ("kind" = String, Path, description = "users, teams or workspaces"),
        ("id" = Uuid, Path, description = "Id of the entity the avatar belongs to"),
    ),
    responses(
        (status = 200, description = "Stored; returns the new avatar_version"),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 403, description = "Member, but the role is insufficient", body = SimpleErrorResponse),
        (status = 404, description = "The caller may not know this entity exists", body = SimpleErrorResponse),
        (status = 413, description = "Past the 5 MB upload limit", body = SimpleErrorResponse),
        (status = 415, description = "Not PNG, JPEG or WebP (SVG has its own code)", body = SimpleErrorResponse),
        (status = 422, description = "avatar_undecodable: the magic bytes matched but the image would not decode", body = UnprocessableErrorResponse),
        (status = 429, description = "Rate limited", body = SimpleErrorResponse),
        (status = 503, description = "Rate limiter or object store unavailable", body = SimpleErrorResponse),
    )
)]
async fn upload_avatar(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((kind, id)): Path<(String, Uuid)>,
    body: Bytes,
) -> Result<Response, AppError> {
    let kind = parse_kind(&kind)?;

    authorize_write(&state, &auth, kind, id).await?;

    // After authorization, so an unauthorized caller cannot consume the quota
    // of a user they are not. Before decoding, which is the expensive part
    // this limit exists to bound.
    let mut redis = state.redis.clone();
    match rate_limit_service::check_avatar_upload(&mut redis, auth.id).await {
        Ok(RateLimitDecision::Allowed) => {}
        Ok(RateLimitDecision::Limited { retry_after_secs }) => {
            return Err(AppError::RateLimited { retry_after_secs });
        }
        // Fail closed. Nothing else bounds this route: it is reachable by any
        // authenticated account and it is the only one that hands
        // attacker-chosen bytes to an image decoder, so an unmetered one is a
        // CPU sink. That places it in the second row of the table in
        // SECURITY.md, with `register` and `contact`.
        Err(error) => {
            tracing::error!(?error, "avatar rate limiter unavailable");
            return Err(AppError::ServiceUnavailable);
        }
    }

    let version =
        avatar_service::upload(&state.db, &state.storage, kind, id, body.to_vec()).await?;

    Ok((StatusCode::OK, Json(json!({ "avatar_version": version }))).into_response())
}

/// Clears an avatar and purges every version of it.
#[utoipa::path(
    delete,
    path = "/api/v1/avatars/{kind}/{id}",
    tag = "Avatars",
    security(("access_token" = [])),
    params(
        ("kind" = String, Path, description = "users, teams or workspaces"),
        ("id" = Uuid, Path, description = "Id of the entity the avatar belongs to"),
    ),
    responses(
        (status = 204, description = "Cleared, and every stored version purged"),
        (status = 401, description = "Unauthorized", body = SimpleErrorResponse),
        (status = 403, description = "Member, but the role is insufficient", body = SimpleErrorResponse),
        (status = 404, description = "The caller may not know this entity exists", body = SimpleErrorResponse),
    )
)]
async fn delete_avatar(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((kind, id)): Path<(String, Uuid)>,
) -> Result<Response, AppError> {
    let kind = parse_kind(&kind)?;

    authorize_write(&state, &auth, kind, id).await?;

    avatar_service::remove(&state.db, &state.storage, kind, id).await?;

    Ok(StatusCode::NO_CONTENT.into_response())
}

/// The response headers that make serving user-supplied imagery inline safe.
fn rendition_response(bytes: Vec<u8>) -> Response {
    let mut response = (StatusCode::OK, bytes).into_response();
    let headers = response.headers_mut();

    // Fixed, never derived from the upload: every object in this bucket was
    // written by our own WebP encoder.
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(OUTPUT_CONTENT_TYPE),
    );

    // Belt and braces with the fixed Content-Type above. Without it a browser
    // may sniff the body and act on what it finds rather than on what we said.
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );

    // `inline` is acceptable here, and only here, because the bytes below are
    // our own re-encode of a decoded pixel buffer: a polyglot that is both a
    // valid image and a valid HTML document does not survive that round trip.
    // Challenge attachments are the opposite case - arbitrary operator-supplied
    // files, served as `application/octet-stream` with `attachment` - and this
    // must not be taken as a precedent for them.
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("inline"),
    );

    // `immutable` is truthful because the URL names a version: a new upload
    // gets a new uuid and therefore a new URL, and the superseded objects are
    // deleted. Were the version not in the path, this would pin a stale avatar
    // in every cache for a year.
    //
    // `private`, not `public`. A shared cache holding the bytes would make the
    // permission check above a guard on *discovering* the URL rather than on
    // the bytes - which is exactly the URL-as-capability model rejected when
    // presigned URLs were ruled out, reintroduced one header at a time. The
    // browser cache behaves identically under `private`, so the only thing
    // given up is a shared cache we do not have.
    //
    // If a CDN is put in front of avatars later, `public` becomes a deliberate
    // choice to make there, and only for entities whose avatars are public
    // anyway. See the caching note in
    // docs/adr/0001-object-storage-and-avatars.md.
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=31536000, immutable"),
    );

    response
}

/// Who may see an avatar.
///
/// A non-member gets `404` rather than `403`: answering `403` would confirm
/// that the team or workspace id they tried is real, which is the enumeration
/// oracle SECURITY.md rules out.
async fn authorize_read(
    state: &AppState,
    auth: &AuthUser,
    kind: AvatarKind,
    id: Uuid,
) -> Result<(), AppError> {
    match kind {
        // Any authenticated caller. A user's avatar is shown next to their
        // name in member lists, activity feeds and writeups, which span teams
        // and workspaces; scoping this to "shares a context with the viewer"
        // would mean a membership query per image on every page that renders
        // one, for a picture the account chose to publish.
        AvatarKind::User => Ok(()),

        AvatarKind::Team => {
            team_membership(state, auth.id, id).await?;
            Ok(())
        }

        AvatarKind::Workspace => {
            workspace_service::get_workspace_for_member_by_id(&state.db, id, auth.id)
                .await
                .map_err(hide_workspace)?;
            Ok(())
        }
    }
}

/// Who may replace or clear an avatar.
///
/// The split between `404` and `403` follows the same rule as everywhere else:
/// someone with no access at all must not learn the resource exists, while a
/// member whose role is too low already knows it does.
async fn authorize_write(
    state: &AppState,
    auth: &AuthUser,
    kind: AvatarKind,
    id: Uuid,
) -> Result<(), AppError> {
    match kind {
        // Only the account itself. A different caller already knows the
        // account exists - they can read its avatar - so this is a `403`, not
        // a `404`.
        AvatarKind::User => {
            if auth.id == id {
                Ok(())
            } else {
                Err(AppError::Forbidden)
            }
        }

        AvatarKind::Team => {
            let role = team_membership(state, auth.id, id).await?;
            team_service::require_role(role, TeamRole::Admin)?;
            Ok(())
        }

        // Access, then role, then state: 404 for no access, 403 for a role
        // too low, 409 for an archived workspace. A member below Admin learns
        // only that they may not write, not whether the workspace is archived.
        AvatarKind::Workspace => {
            let (workspace, membership) =
                workspace_service::get_workspace_for_member_by_id(&state.db, id, auth.id)
                    .await
                    .map_err(hide_workspace)?;

            if !membership.satisfies(TeamRole::Admin) {
                return Err(AppError::Forbidden);
            }

            workspace_service::ensure_writable(&workspace)?;
            Ok(())
        }
    }
}

/// Resolves the caller's role in a team, or `404` if they are not in it.
///
/// Resolved as one step, so "no such team" and "not your team" cannot be told
/// apart from the outside.
async fn team_membership(
    state: &AppState,
    user_id: Uuid,
    team_id: Uuid,
) -> Result<TeamRole, AppError> {
    team_service::get_user_role_in_team(&state.db, team_id, user_id)
        .await?
        .ok_or(AppError::NotFound)
}

/// The workspace access answer, translated for the avatar routes.
///
/// Access is resolved by `workspace_service`, the same function the workspace
/// routes use, so the rule exists once. Only the shape of the refusal changes:
/// these routes have always answered a bare `not_found`, the same body as an
/// entity with no avatar, and `workspace_not_found` would tell a hidden
/// workspace apart from a missing image.
fn hide_workspace(err: WorkspaceError) -> AppError {
    match err {
        WorkspaceError::WorkspaceNotFound => AppError::NotFound,
        other => other.into(),
    }
}

/// The path segment, back into the closed set. Anything else is a `404`: the
/// route does not exist for that spelling.
fn parse_kind(raw: &str) -> Result<AvatarKind, AppError> {
    match raw {
        "users" => Ok(AvatarKind::User),
        "teams" => Ok(AvatarKind::Team),
        "workspaces" => Ok(AvatarKind::Workspace),
        _ => Err(AppError::NotFound),
    }
}

fn parse_size(raw: &str) -> Result<AvatarSize, AppError> {
    match raw {
        "256.webp" => Ok(AvatarSize::Large),
        "64.webp" => Ok(AvatarSize::Small),
        _ => Err(AppError::NotFound),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        add_team_member, app, cookie_for, create_user, create_workspace, db, request_as,
    };
    use axum::body::Body;
    use axum::http::Request;
    use domain::entities::team_members;
    use sea_orm::{ActiveModelTrait, Set};
    use tower::ServiceExt;

    /// A real 2x2 RGBA PNG. Small enough to inline, real enough that the
    /// decoder accepts it, which is the point: a hand-waved byte string would
    /// be rejected by the allowlist and the upload under test would never run.
    const TINY_PNG: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x08, 0x06, 0x00, 0x00, 0x00, 0x72,
        0xb6, 0x0d, 0x24, 0x00, 0x00, 0x00, 0x16, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x60,
        0x60, 0x68, 0xf8, 0x9f, 0x02, 0xc4, 0x0c, 0x0c, 0x29, 0x40, 0x06, 0x10, 0x03, 0x00, 0x39,
        0x80, 0x07, 0x8d, 0xd8, 0xcd, 0x2b, 0xa8, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44,
        0xae, 0x42, 0x60, 0x82,
    ];

    async fn upload_avatar_as(
        app: &axum::Router,
        user_id: Uuid,
        kind: &str,
        id: Uuid,
    ) -> StatusCode {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/avatars/{kind}/{id}"))
                    .header("cookie", cookie_for(user_id))
                    .body(Body::from(TINY_PNG))
                    .expect("request must build"),
            )
            .await
            .expect("router must answer");

        response.status()
    }

    async fn read_avatar_as(
        app: &axum::Router,
        user_id: Option<Uuid>,
        kind: &str,
        id: Uuid,
        version: Uuid,
    ) -> StatusCode {
        read_avatar_response(app, user_id, kind, id, version)
            .await
            .status()
    }

    async fn read_avatar_response(
        app: &axum::Router,
        user_id: Option<Uuid>,
        kind: &str,
        id: Uuid,
        version: Uuid,
    ) -> Response {
        let mut request = Request::builder()
            .method("GET")
            .uri(format!("/api/v1/avatars/{kind}/{id}/{version}/256.webp"));

        if let Some(user_id) = user_id {
            request = request.header("cookie", cookie_for(user_id));
        }

        app.clone()
            .oneshot(request.body(Body::empty()).expect("request must build"))
            .await
            .expect("router must answer")
    }

    /// The test this whole design exists for.
    ///
    /// An authenticated stranger asking for a private workspace's avatar must
    /// get `404`, not `403` and not the image. `403` would confirm the
    /// workspace id is real, which is the enumeration oracle SECURITY.md rules
    /// out; the image would mean the permission check is decorative.
    ///
    /// The member's `200` in the same test is what makes the `404` mean
    /// something. Without it, a route that answered `404` unconditionally -
    /// because the avatar failed to upload, because the version was wrong,
    /// because the whole feature was broken - would pass just as well.
    #[tokio::test]
    async fn a_non_member_gets_404_for_a_private_workspace_avatar() {
        let db = db().await;
        let app = app(&db).await;

        let member = create_user(&db).await;
        let stranger = create_user(&db).await;
        let (workspace_id, _team_id) = create_workspace(&db, member).await;

        assert_eq!(
            upload_avatar_as(&app, member, "workspaces", workspace_id).await,
            StatusCode::OK,
            "the captain must be able to set the workspace avatar"
        );

        let version = services::avatar_service::current_version(
            &db,
            services::storage_service::AvatarKind::Workspace,
            workspace_id,
        )
        .await
        .expect("version lookup must succeed")
        .expect("the upload must have recorded a version");

        // The member can read it: the avatar exists and is served.
        assert_eq!(
            read_avatar_as(&app, Some(member), "workspaces", workspace_id, version).await,
            StatusCode::OK,
            "a member must be served the avatar"
        );

        // The stranger, holding a perfectly valid session and the exact URL,
        // must not.
        assert_eq!(
            read_avatar_as(&app, Some(stranger), "workspaces", workspace_id, version).await,
            StatusCode::NOT_FOUND,
            "a non-member must get 404, never 403 and never the image"
        );

        // And unauthenticated is 401, which is a different failure from the
        // one above and must not be confused with it.
        assert_eq!(
            read_avatar_as(&app, None, "workspaces", workspace_id, version).await,
            StatusCode::UNAUTHORIZED,
            "no session at all is 401"
        );
    }

    /// The headers that make serving user-supplied imagery inline safe, pinned
    /// so none of them can be dropped silently.
    ///
    /// `private` is the one that matters most and is the easiest to regress: a
    /// shared cache holding these bytes would turn the permission check above
    /// into a guard on discovering the URL rather than on the bytes, which is
    /// the URL-as-capability model that ruling out presigned URLs was meant to
    /// avoid.
    #[tokio::test]
    async fn the_avatar_response_carries_its_safety_headers() {
        let db = db().await;
        let app = app(&db).await;

        let member = create_user(&db).await;
        let (workspace_id, _team_id) = create_workspace(&db, member).await;

        assert_eq!(
            upload_avatar_as(&app, member, "workspaces", workspace_id).await,
            StatusCode::OK
        );

        let version = services::avatar_service::current_version(
            &db,
            services::storage_service::AvatarKind::Workspace,
            workspace_id,
        )
        .await
        .expect("version lookup must succeed")
        .expect("the upload must have recorded a version");

        let response =
            read_avatar_response(&app, Some(member), "workspaces", workspace_id, version).await;

        assert_eq!(response.status(), StatusCode::OK);

        let headers = response.headers();

        let cache_control = headers
            .get(header::CACHE_CONTROL)
            .expect("Cache-Control must be set")
            .to_str()
            .expect("Cache-Control must be ASCII");

        assert!(
            cache_control.contains("private"),
            "Cache-Control must be private, got {cache_control:?}"
        );
        assert!(
            !cache_control.contains("public"),
            "Cache-Control must not be public, got {cache_control:?}"
        );
        assert!(
            cache_control.contains("immutable"),
            "the URL names a version, so immutable is correct: got {cache_control:?}"
        );

        assert_eq!(
            headers
                .get(header::CONTENT_TYPE)
                .map(|v| v.to_str().unwrap()),
            Some("image/webp"),
            "the Content-Type is fixed, never derived from the upload"
        );
        assert_eq!(
            headers
                .get(header::X_CONTENT_TYPE_OPTIONS)
                .map(|v| v.to_str().unwrap()),
            Some("nosniff"),
        );
        assert_eq!(
            headers
                .get(header::CONTENT_DISPOSITION)
                .map(|v| v.to_str().unwrap()),
            Some("inline"),
            "inline is only acceptable because we re-encoded the bytes"
        );
    }

    /// The write side of the same rule: a stranger must not be able to replace
    /// a workspace's avatar, and must not learn the workspace exists by trying.
    #[tokio::test]
    async fn a_non_member_gets_404_when_writing_a_private_workspace_avatar() {
        let db = db().await;
        let app = app(&db).await;

        let member = create_user(&db).await;
        let stranger = create_user(&db).await;
        let (workspace_id, _team_id) = create_workspace(&db, member).await;

        assert_eq!(
            upload_avatar_as(&app, stranger, "workspaces", workspace_id).await,
            StatusCode::NOT_FOUND,
            "a non-member writing must get 404, not 403"
        );
    }

    /// A member whose role is too low is the other half of the 404/403 rule:
    /// they can see the workspace, so hiding it from them would be pointless,
    /// and they get `403`.
    #[tokio::test]
    async fn a_viewer_gets_403_when_writing_a_workspace_avatar() {
        let db = db().await;
        let app = app(&db).await;

        let captain = create_user(&db).await;
        let viewer = create_user(&db).await;
        let (workspace_id, team_id) = create_workspace(&db, captain).await;

        team_members::ActiveModel {
            team_id: Set(team_id),
            user_id: Set(viewer),
            role: Set("viewer".to_owned()),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("viewer membership must insert");

        assert_eq!(
            upload_avatar_as(&app, viewer, "workspaces", workspace_id).await,
            StatusCode::FORBIDDEN,
            "a member with an insufficient role already knows it exists: 403"
        );
    }

    /// A user may only replace their own avatar. A different account is `403`,
    /// not `404`: every authenticated caller can already read any user's
    /// avatar, so the account's existence is not a secret to hide.
    #[tokio::test]
    async fn a_user_may_not_write_another_users_avatar() {
        let db = db().await;
        let app = app(&db).await;

        let owner = create_user(&db).await;
        let other = create_user(&db).await;

        assert_eq!(
            upload_avatar_as(&app, owner, "users", owner).await,
            StatusCode::OK,
            "a user must be able to set their own avatar"
        );

        assert_eq!(
            upload_avatar_as(&app, other, "users", owner).await,
            StatusCode::FORBIDDEN,
            "writing someone else's avatar is 403"
        );
    }

    /// An archived workspace is read-only, and its avatar is part of it. The
    /// write is refused with 409 for a member who holds the role, after the
    /// access check (404 for a stranger) and the role check (403 for a
    /// Viewer), so neither learns the state. Reading stays open.
    #[tokio::test]
    async fn an_archived_workspace_avatar_can_be_read_but_not_written() {
        let db = db().await;
        let app = app(&db).await;

        let captain = create_user(&db).await;
        let admin = create_user(&db).await;
        let viewer = create_user(&db).await;
        let stranger = create_user(&db).await;
        let (workspace_id, team_id) = create_workspace(&db, captain).await;
        add_team_member(&db, team_id, admin, TeamRole::Admin).await;
        add_team_member(&db, team_id, viewer, TeamRole::Viewer).await;

        // Control: before archiving, the Admin may write.
        assert_eq!(
            upload_avatar_as(&app, admin, "workspaces", workspace_id).await,
            StatusCode::OK,
            "an Admin must be able to set the avatar of a live workspace"
        );

        workspace_service::archive_workspace(&db, workspace_id)
            .await
            .expect("the archive must be written");

        let version = services::avatar_service::current_version(
            &db,
            services::storage_service::AvatarKind::Workspace,
            workspace_id,
        )
        .await
        .expect("version lookup must succeed")
        .expect("the upload must have recorded a version");

        assert_eq!(
            upload_avatar_as(&app, admin, "workspaces", workspace_id).await,
            StatusCode::CONFLICT,
            "an archived workspace refuses a new avatar"
        );

        let uri = format!("/api/v1/avatars/workspaces/{workspace_id}");
        let (deleted, body) = request_as(&app, "DELETE", &uri, Some(admin), None).await;
        assert_eq!(deleted, StatusCode::CONFLICT, "nor may it be cleared");
        assert_eq!(body["error"], "workspace_archived");

        // The order of the refusals: access, then role, then state.
        assert_eq!(
            upload_avatar_as(&app, viewer, "workspaces", workspace_id).await,
            StatusCode::FORBIDDEN,
            "the role is checked before the archived state"
        );
        assert_eq!(
            upload_avatar_as(&app, stranger, "workspaces", workspace_id).await,
            StatusCode::NOT_FOUND,
            "a stranger learns nothing, archived or not"
        );

        // The body a stranger gets is the bare `not_found` of a missing image,
        // not `workspace_not_found`, which would set the two apart.
        let image = format!("/api/v1/avatars/workspaces/{workspace_id}/{version}/256.webp");
        let (hidden, hidden_body) = request_as(&app, "GET", &image, Some(stranger), None).await;
        assert_eq!(hidden, StatusCode::NOT_FOUND);
        assert_eq!(hidden_body["error"], "not_found");

        // Reading is not a write: the avatar is still served, unchanged.
        assert_eq!(
            read_avatar_as(&app, Some(viewer), "workspaces", workspace_id, version).await,
            StatusCode::OK,
            "an archived workspace's avatar must still be readable"
        );
        assert_eq!(
            services::avatar_service::current_version(
                &db,
                services::storage_service::AvatarKind::Workspace,
                workspace_id,
            )
            .await
            .expect("version lookup must succeed"),
            Some(version),
            "the refused writes must not have touched the stored version"
        );
    }

    /// Setting an avatar is an update of the entity's row, so it stamps
    /// `updated_at`: the version is written through a partial ActiveModel,
    /// which only carries the columns it names. Both kinds that have the
    /// column are checked; `teams` has none.
    #[tokio::test]
    async fn setting_an_avatar_moves_updated_at() {
        use domain::entities::{users, workspaces};
        use sea_orm::EntityTrait;

        let db = db().await;
        let app = app(&db).await;

        let captain = create_user(&db).await;
        let (workspace_id, _team_id) = create_workspace(&db, captain).await;

        let user_before = users::Entity::find_by_id(captain)
            .one(&db)
            .await
            .expect("the lookup must succeed")
            .expect("the user must exist")
            .updated_at;
        let workspace_before = workspaces::Entity::find_by_id(workspace_id)
            .one(&db)
            .await
            .expect("the lookup must succeed")
            .expect("the workspace must exist")
            .updated_at;

        assert_eq!(
            upload_avatar_as(&app, captain, "users", captain).await,
            StatusCode::OK
        );
        assert_eq!(
            upload_avatar_as(&app, captain, "workspaces", workspace_id).await,
            StatusCode::OK
        );

        let user_after = users::Entity::find_by_id(captain)
            .one(&db)
            .await
            .expect("the lookup must succeed")
            .expect("the user must exist")
            .updated_at;
        let workspace_after = workspaces::Entity::find_by_id(workspace_id)
            .one(&db)
            .await
            .expect("the lookup must succeed")
            .expect("the workspace must exist")
            .updated_at;

        assert!(
            user_after > user_before,
            "the user's updated_at must move: {user_before} then {user_after}"
        );
        assert!(
            workspace_after > workspace_before,
            "the workspace's updated_at must move: {workspace_before} then {workspace_after}"
        );
    }
}

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
use domain::dto::error_responses::SimpleErrorResponse;
use domain::dto::team::TeamRole;
use domain::dto::workspace::Membership;
use domain::entities::workspaces;
use sea_orm::EntityTrait;
use services::avatar_service::{self, AvatarError, MAX_UPLOAD_BYTES, OUTPUT_CONTENT_TYPE};
use services::rate_limit_service::{self, RateLimitDecision};
use services::storage_service::{AvatarKind, AvatarSize};
use services::{permissions_service, team_service};

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
        (status = 422, description = "Magic bytes matched but the image would not decode", body = SimpleErrorResponse),
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
            workspace_membership(state, auth.id, id).await?;
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

        AvatarKind::Workspace => {
            let role = workspace_membership(state, auth.id, id).await?;
            team_service::require_role(role, TeamRole::Admin)?;
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

/// Same, for a workspace, folding in solo ownership and workspace-level
/// overrides through `permissions_service`.
async fn workspace_membership(
    state: &AppState,
    user_id: Uuid,
    workspace_id: Uuid,
) -> Result<TeamRole, AppError> {
    let workspace = workspaces::Entity::find_by_id(workspace_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    match permissions_service::effective_role(&state.db, user_id, &workspace).await? {
        Membership::Owner => Ok(TeamRole::Captain),
        Membership::Active(role) => Ok(role),
        // Excluded is an explicit removal from a workspace the caller's team
        // can otherwise see. They must not be able to tell that apart from a
        // workspace that does not exist.
        Membership::Excluded | Membership::None => Err(AppError::NotFound),
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
    use axum::body::Body;
    use axum::http::Request;
    use domain::entities::{team_members, teams, users, workspaces};
    use migration::MigratorTrait;
    use sea_orm::{ActiveModelTrait, ConnectionTrait, Database, Set};
    use services::config::{Config, EmailBackend, SmtpTls, StorageConfig};
    use services::storage_service::{ObjectStore, S3Store};
    use std::sync::Arc;
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

    const JWT_SECRET: &str = "test-secret-at-least-thirty-two-characters-long";

    fn env_or(key: &str, fallback: &str) -> String {
        std::env::var(key).unwrap_or_else(|_| fallback.to_owned())
    }

    fn storage_config() -> StorageConfig {
        StorageConfig {
            endpoint: Some(env_or("PWNFORGE_S3_ENDPOINT", "http://127.0.0.1:9000")),
            region: env_or("PWNFORGE_S3_REGION", "us-east-1"),
            access_key_id: env_or("PWNFORGE_S3_ACCESS_KEY_ID", "pwnforge-app"),
            secret_access_key: std::env::var("PWNFORGE_S3_SECRET_ACCESS_KEY")
                .expect("PWNFORGE_S3_SECRET_ACCESS_KEY must be set; see crates/services/tests/object_storage.rs"),
            force_path_style: true,
            bucket_attachments: env_or("PWNFORGE_S3_BUCKET_ATTACHMENTS", "pwnforge-attachments"),
            bucket_avatars: env_or("PWNFORGE_S3_BUCKET_AVATARS", "pwnforge-avatars"),
            key_prefix: None,
            public_base_url: None,
        }
    }

    /// Only the fields these tests reach. Everything else is inert: no mail is
    /// sent and no outbound call is made on the avatar routes.
    fn config() -> Config {
        Config {
            port: 0,
            public_url: "http://localhost".to_owned(),
            cors_origin: "http://localhost:5173".to_owned(),
            setup_token: None,
            database_url: String::new(),
            database_max_connections: 5,
            redis_url: String::new(),
            jwt_secret: JWT_SECRET.to_owned(),
            encryption_key: "x".repeat(32),
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
            storage: storage_config(),
        }
    }

    /// Advisory-lock key for the migration step. Arbitrary but fixed; the only
    /// requirement is that nothing else in the instance picks the same one.
    const MIGRATION_LOCK_KEY: i64 = 0x7077_6e66_6f72_6761;

    /// A connection of this test's own, with the schema guaranteed to be up.
    ///
    /// Each `#[tokio::test]` builds its own runtime, and a sqlx pool is bound
    /// to the runtime that created it: a pool shared through a `OnceCell`
    /// stops handing out connections as soon as the first test's runtime is
    /// dropped, and every later test fails with an acquire timeout. So each
    /// test gets its own pool.
    ///
    /// That leaves the migration itself, which `Migrator::up` does not lock:
    /// two tests racing it both read a migration as unapplied and both run its
    /// DDL, and the loser fails on a column that is already gone. A Postgres
    /// advisory lock serialises it - across runtimes and across processes,
    /// which a Rust-side mutex could not do.
    async fn db() -> sea_orm::DatabaseConnection {
        let db = Database::connect(env_or(
            "DATABASE_URL",
            "postgresql://pwnforge:changeme@127.0.0.1:5432/pwnforge",
        ))
        .await
        .expect("a Postgres instance must be reachable");

        db.execute_unprepared(&format!("SELECT pg_advisory_lock({MIGRATION_LOCK_KEY})"))
            .await
            .expect("advisory lock must be acquirable");

        let migrated = migration::Migrator::up(&db, None).await;

        db.execute_unprepared(&format!("SELECT pg_advisory_unlock({MIGRATION_LOCK_KEY})"))
            .await
            .expect("advisory lock must be releasable");

        migrated.expect("migrations must apply");

        db
    }

    async fn app(db: &sea_orm::DatabaseConnection) -> axum::Router {
        let db = db.clone();

        let redis_url = env_or("REDIS_URL", "redis://127.0.0.1:6379");
        let redis = redis::aio::ConnectionManager::new(
            redis::Client::open(redis_url).expect("REDIS_URL must be valid"),
        )
        .await
        .expect("a Redis instance must be reachable");

        let config = config();
        let storage = ObjectStore::S3(S3Store::new(&config.storage));

        crate::app::build_router(AppState {
            db,
            redis,
            config: Arc::new(config),
            jwt_keys: Arc::new(crate::state::JwtKeys {
                encoding: jsonwebtoken::EncodingKey::from_secret(JWT_SECRET.as_bytes()),
                decoding: jsonwebtoken::DecodingKey::from_secret(JWT_SECRET.as_bytes()),
            }),
            setup_token: None,
            storage: Arc::new(storage),
        })
    }

    fn unique(prefix: &str) -> String {
        format!("{prefix}-{}", Uuid::now_v7().simple())
    }

    async fn create_user(db: &sea_orm::DatabaseConnection) -> Uuid {
        users::ActiveModel {
            id: Set(Uuid::now_v7()),
            username: Set(unique("avatar-test")),
            password_hash: Set("not-a-real-hash".to_owned()),
            locale: Set("en".to_owned()),
            theme: Set("dark".to_owned()),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("user fixture must insert")
        .id
    }

    /// A team-mode workspace owned by a fresh team, with `captain` as the
    /// team's Captain. Team mode rather than solo because that is the shape
    /// where "member" and "non-member" are actually distinct.
    async fn create_workspace(db: &sea_orm::DatabaseConnection, captain: Uuid) -> (Uuid, Uuid) {
        let team = teams::ActiveModel {
            id: Set(Uuid::now_v7()),
            slug: Set(unique("avatar-team")),
            name: Set("Avatar Test Team".to_owned()),
            color: Set("blue".to_owned()),
            created_by: Set(Some(captain)),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("team fixture must insert");

        team_members::ActiveModel {
            team_id: Set(team.id),
            user_id: Set(captain),
            role: Set("captain".to_owned()),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("membership fixture must insert");

        let workspace = workspaces::ActiveModel {
            id: Set(Uuid::now_v7()),
            team_id: Set(Some(team.id)),
            slug: Set(unique("avatar-ws")),
            name: Set("Private Workspace".to_owned()),
            color: Set("blue".to_owned()),
            r#type: Set("ctf".to_owned()),
            mode: Set("team".to_owned()),
            default_writeup_visibility: Set("team".to_owned()),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("workspace fixture must insert");

        (workspace.id, team.id)
    }

    fn cookie_for(user_id: Uuid) -> String {
        let token = services::jwt_service::generate_access_token(
            user_id,
            &jsonwebtoken::EncodingKey::from_secret(JWT_SECRET.as_bytes()),
        )
        .expect("token must mint");

        format!("access_token={token}")
    }

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
}

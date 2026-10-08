// crates/api/src/test_support.rs
//
// Shared fixtures for the tests that drive the real router. Every helper
// creates rows of its own: the dev database is shared and grows with each
// run, so a test must only ever assert on what it created.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use domain::dto::team::TeamRole;
use domain::entities::{team_members, teams, users, workspaces};
use domain::types::AccentColor;
use migration::MigratorTrait;
use sea_orm::{ActiveModelTrait, ConnectionTrait, Database, DbBackend, Set, Statement};
use services::config::{Config, EmailBackend, SmtpTls, StorageConfig};
use services::storage_service::{ObjectStore, S3Store};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

use crate::state::AppState;

pub(crate) const JWT_SECRET: &str = "test-secret-at-least-thirty-two-characters-long";

pub(crate) fn env_or(key: &str, fallback: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| fallback.to_owned())
}

pub(crate) fn storage_config() -> StorageConfig {
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
pub(crate) fn config() -> Config {
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
        turnstile_site_key: None,
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
pub(crate) const MIGRATION_LOCK_KEY: i64 = 0x7077_6e66_6f72_6761;

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
pub(crate) async fn db() -> sea_orm::DatabaseConnection {
    let db = Database::connect(env_or(
        "DATABASE_URL",
        "postgresql://pwnforge:changeme@127.0.0.1:5432/pwnforge",
    ))
    .await
    .expect("a Postgres instance must be reachable");

    db.execute_raw(advisory_lock_statement("SELECT pg_advisory_lock($1)"))
        .await
        .expect("advisory lock must be acquirable");

    let migrated = migration::Migrator::up(&db, None).await;

    db.execute_raw(advisory_lock_statement("SELECT pg_advisory_unlock($1)"))
        .await
        .expect("advisory lock must be releasable");

    migrated.expect("migrations must apply");

    db
}

/// The key travels as a bound parameter, never spliced into the SQL text.
fn advisory_lock_statement(sql: &str) -> Statement {
    Statement::from_sql_and_values(DbBackend::Postgres, sql, [MIGRATION_LOCK_KEY.into()])
}

pub(crate) async fn app(db: &sea_orm::DatabaseConnection) -> axum::Router {
    app_with_config(db, config()).await
}

/// `app`, with a configuration of the test's own (Turnstile on, for one).
pub(crate) async fn app_with_config(
    db: &sea_orm::DatabaseConnection,
    config: Config,
) -> axum::Router {
    let db = db.clone();

    let redis_url = env_or("REDIS_URL", "redis://127.0.0.1:6379");
    let mut redis = redis::aio::ConnectionManager::new(
        redis::Client::open(redis_url).expect("REDIS_URL must be valid"),
    )
    .await
    .expect("a Redis instance must be reachable");

    // Reachability is not usability, and the constructor above only
    // proves the former: a Redis started with `requirepass`, which the dev
    // compose does, accepts the connection and then refuses every command
    // with NOAUTH. Without this PING the first sign of trouble is the rate
    // limiter answering 503, asserted against an expected 200 hundreds of
    // lines below, naming neither Redis nor the variable that is wrong.
    // CI's service has no password and passes this as-is.
    redis::cmd("PING")
        .query_async::<String>(&mut redis)
        .await
        .expect(
            "Redis must answer PING; if this is NOAUTH the URL needs the \
             password, as REDIS_URL=redis://:$REDIS_PASSWORD@127.0.0.1:6379 \
             (see .env.example)",
        );

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

pub(crate) fn unique(prefix: &str) -> String {
    format!("{prefix}-{}", Uuid::now_v7().simple())
}

pub(crate) async fn create_user(db: &sea_orm::DatabaseConnection) -> Uuid {
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

pub(crate) fn cookie_for(user_id: Uuid) -> String {
    let token = services::jwt_service::generate_access_token(
        user_id,
        &jsonwebtoken::EncodingKey::from_secret(JWT_SECRET.as_bytes()),
    )
    .expect("token must mint");

    format!("access_token={token}")
}

/// A team with `captain` as its Captain.
pub(crate) async fn create_team(db: &sea_orm::DatabaseConnection, captain: Uuid) -> Uuid {
    let team = teams::ActiveModel {
        id: Set(Uuid::now_v7()),
        slug: Set(unique("test-team")),
        name: Set("Test Team".to_owned()),
        color: Set(AccentColor::Blue.as_str().to_owned()),
        created_by: Set(Some(captain)),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("team fixture must insert");

    add_team_member(db, team.id, captain, TeamRole::Captain).await;

    team.id
}

pub(crate) async fn add_team_member(
    db: &sea_orm::DatabaseConnection,
    team_id: Uuid,
    user_id: Uuid,
    role: TeamRole,
) {
    team_members::ActiveModel {
        team_id: Set(team_id),
        user_id: Set(user_id),
        role: Set(role.as_str().to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("membership fixture must insert");
}

/// A team-mode event workspace under `team_id`, satisfying every constraint.
pub(crate) async fn create_team_workspace(
    db: &sea_orm::DatabaseConnection,
    team_id: Uuid,
    name: &str,
) -> workspaces::Model {
    workspaces::ActiveModel {
        id: Set(Uuid::now_v7()),
        team_id: Set(Some(team_id)),
        slug: Set(unique("test-ws")),
        name: Set(name.to_owned()),
        color: Set(AccentColor::Blue.as_str().to_owned()),
        r#type: Set("event".to_owned()),
        mode: Set("team".to_owned()),
        default_writeup_visibility: Set("team".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("team workspace fixture must insert")
}

/// A solo platform workspace owned by `owner_id`.
pub(crate) async fn create_solo_workspace(
    db: &sea_orm::DatabaseConnection,
    owner_id: Uuid,
    name: &str,
) -> workspaces::Model {
    workspaces::ActiveModel {
        id: Set(Uuid::now_v7()),
        owner_id: Set(Some(owner_id)),
        slug: Set(unique("test-solo")),
        name: Set(name.to_owned()),
        color: Set(AccentColor::Ember.as_str().to_owned()),
        r#type: Set("platform".to_owned()),
        platform: Set(Some("htb".to_owned())),
        mode: Set("solo".to_owned()),
        default_writeup_visibility: Set("draft".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("solo workspace fixture must insert")
}

/// A team-mode workspace owned by a fresh team, with `captain` as the
/// team's Captain. Team mode rather than solo because that is the shape
/// where "member" and "non-member" are actually distinct.
pub(crate) async fn create_workspace(
    db: &sea_orm::DatabaseConnection,
    captain: Uuid,
) -> (Uuid, Uuid) {
    let team_id = create_team(db, captain).await;
    let workspace = create_team_workspace(db, team_id, "Private Workspace").await;

    (workspace.id, team_id)
}

/// Sends one request through the real router, as `user` when given, with a
/// JSON body when given, and returns the status and the decoded body
/// (`Null` for an empty one).
pub(crate) async fn request_as(
    app: &axum::Router,
    method: &str,
    uri: &str,
    user: Option<Uuid>,
    json: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut request = Request::builder().method(method).uri(uri);

    if let Some(user) = user {
        request = request.header("cookie", cookie_for(user));
    }

    let body = match json {
        Some(json) => {
            request = request.header("content-type", "application/json");
            Body::from(json.to_string())
        }
        None => Body::empty(),
    };

    let response = app
        .clone()
        .oneshot(request.body(body).expect("request must build"))
        .await
        .expect("router must answer");

    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the body must be readable");

    let value = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("the body must be JSON")
    };

    (status, value)
}

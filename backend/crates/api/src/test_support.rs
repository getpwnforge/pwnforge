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
use sea_orm::sqlx::{self, Connection, PgConnection};
use sea_orm::{ActiveModelTrait, ConnectionTrait, Database, Set};
use services::avatar_service;
use services::background::BackgroundTasks;
use services::config::{Config, EmailBackend, SmtpTls, StorageConfig};
use services::storage_service::{
    AvatarKind, AvatarSize, Bucket, ObjectStore, S3Store, StorageError, avatar_key,
};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

use crate::state::AppState;

pub(crate) const JWT_SECRET: &str = "test-secret-at-least-thirty-two-characters-long";

/// A real 2x2 RGBA PNG. Small enough to inline, real enough that the decoder
/// accepts it, which is the point: a hand-waved byte string would be rejected
/// by the allowlist and the upload under test would never run.
pub(crate) const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x08, 0x06, 0x00, 0x00, 0x00, 0x72, 0xb6, 0x0d,
    0x24, 0x00, 0x00, 0x00, 0x16, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x60, 0x60, 0x68, 0xf8,
    0x9f, 0x02, 0xc4, 0x0c, 0x0c, 0x29, 0x40, 0x06, 0x10, 0x03, 0x00, 0x39, 0x80, 0x07, 0x8d, 0xd8,
    0xcd, 0x2b, 0xa8, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

/// The bucket the router under test writes to, for checking what it holds.
pub(crate) fn store() -> ObjectStore {
    ObjectStore::S3(S3Store::new(&storage_config()))
}

/// Gives an entity an avatar through the service, and returns its version.
pub(crate) async fn upload_avatar(
    db: &sea_orm::DatabaseConnection,
    kind: AvatarKind,
    id: Uuid,
) -> Uuid {
    avatar_service::upload(db, &store(), kind, id, TINY_PNG.to_vec())
        .await
        .expect("the avatar upload must succeed")
}

/// Whether the bucket still holds the large rendition of that version.
pub(crate) async fn avatar_stored(kind: AvatarKind, id: Uuid, version: Uuid) -> bool {
    let key = avatar_key(kind, id, version, AvatarSize::Large);
    match store()
        .get(Bucket::Avatars, &key, avatar_service::MAX_RENDITION_BYTES)
        .await
    {
        Ok(_) => true,
        Err(StorageError::NotFound) => false,
        Err(error) => panic!("reading {key} failed: {error}"),
    }
}

/// What GET /avatars answers `user` for the large rendition of `version`:
/// whether an avatar an answer announced is one they can load.
pub(crate) async fn avatar_status(
    app: &axum::Router,
    user: Uuid,
    kind: &str,
    id: Uuid,
    version: Uuid,
) -> StatusCode {
    let request = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/avatars/{kind}/{id}/{version}/256.webp"))
        .header("cookie", cookie_for(user))
        .body(Body::empty())
        .expect("request must build");

    app.clone()
        .oneshot(request)
        .await
        .expect("router must answer")
        .status()
}

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
        shutdown_grace_secs: 25,
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
    let url = env_or(
        "DATABASE_URL",
        "postgresql://pwnforge:changeme@127.0.0.1:5432/pwnforge",
    );
    let db = Database::connect(&url)
        .await
        .expect("a Postgres instance must be reachable");

    // On a connection of its own, not one of the pool: a session lock
    // belongs to the connection that took it, and through the pool its
    // release could land on another one. It then stayed held by an idle
    // pooled connection for as long as the pool lived, and a second `db()`
    // in the same test waited on it for good. The key travels as a bound
    // parameter, never spliced into the SQL text.
    let mut lock = PgConnection::connect(&url)
        .await
        .expect("a Postgres instance must be reachable");
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(MIGRATION_LOCK_KEY)
        .execute(&mut lock)
        .await
        .expect("advisory lock must be acquirable");

    let migrated = migration::Migrator::up(&db, None).await;

    sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(MIGRATION_LOCK_KEY)
        .execute(&mut lock)
        .await
        .expect("advisory lock must be releasable");
    lock.close().await.expect("the lock connection must close");

    migrated.expect("migrations must apply");

    db
}

pub(crate) async fn app(db: &sea_orm::DatabaseConnection) -> axum::Router {
    app_with_config(db, config()).await
}

/// `app`, with a configuration of the test's own (Turnstile on, for one).
pub(crate) async fn app_with_config(
    db: &sea_orm::DatabaseConnection,
    config: Config,
) -> axum::Router {
    app_with_background(db, config, BackgroundTasks::new()).await
}

/// `app_with_config`, its background tasks counted by `background`: for the
/// tests that stop the server and wait for them.
pub(crate) async fn app_with_background(
    db: &sea_orm::DatabaseConnection,
    config: Config,
    background: BackgroundTasks,
) -> axum::Router {
    build_app(db, config, background, None).await
}

/// `app`, on an instance whose setup still runs and takes `token`.
pub(crate) async fn app_with_setup_token(
    db: &sea_orm::DatabaseConnection,
    token: &str,
) -> axum::Router {
    build_app(
        db,
        config(),
        BackgroundTasks::new(),
        Some(Arc::new(token.to_owned())),
    )
    .await
}

async fn build_app(
    db: &sea_orm::DatabaseConnection,
    config: Config,
    background: BackgroundTasks,
    setup_token: Option<Arc<String>>,
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
        setup_token,
        storage: Arc::new(storage),
        background,
    })
}

pub(crate) fn unique(prefix: &str) -> String {
    format!("{prefix}-{}", Uuid::now_v7().simple())
}

pub(crate) async fn create_user(db: &sea_orm::DatabaseConnection) -> Uuid {
    users::ActiveModel {
        id: Set(Uuid::now_v7()),
        // A username the API itself accepts (at most 32 characters), so a
        // test can invite the account by it. The first digit of a v7 uuid is
        // the top of its timestamp, the same for every account made today,
        // so dropping it keeps the name unique.
        username: Set(format!("u{}", &Uuid::now_v7().simple().to_string()[1..])),
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

// ---- Throwaway databases ----------------------------------------------------

/// Swaps the database name in a connection URL, keeping everything else.
fn with_database(url: &str, database: &str) -> String {
    let (prefix, _old) = url
        .rsplit_once('/')
        .expect("DATABASE_URL must contain a database name");
    format!("{prefix}/{database}")
}

/// Runs `body` against a fresh database, migrated and with its setup marked
/// as run, and drops it afterwards, even when an assertion fails. For the
/// tests that need a state the shared database must not be put in, an
/// instance open to registration among them. The same helper as the
/// services tests' `common/scratch.rs`, which this crate cannot include.
pub(crate) async fn in_scratch_database<F, Fut>(body: F)
where
    F: FnOnce(sea_orm::DatabaseConnection) -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    in_fresh_database(true, body).await;
}

/// `in_scratch_database`, with the setup still to run: for the tests of the
/// setup itself.
pub(crate) async fn in_database_before_setup<F, Fut>(body: F)
where
    F: FnOnce(sea_orm::DatabaseConnection) -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    in_fresh_database(false, body).await;
}

async fn in_fresh_database<F, Fut>(set_up: bool, body: F)
where
    F: FnOnce(sea_orm::DatabaseConnection) -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    let base = env_or(
        "DATABASE_URL",
        "postgresql://pwnforge:changeme@127.0.0.1:5432/pwnforge",
    );
    let admin = Database::connect(with_database(&base, "postgres"))
        .await
        .expect("a Postgres instance must be reachable");
    // Unique per run, and a valid unquoted identifier.
    let name = format!("pwnforge_scratch_test_{}", Uuid::now_v7().simple());
    admin
        .execute_unprepared(&format!("CREATE DATABASE {name}"))
        .await
        .expect("must be able to create a scratch database");

    let db = Database::connect(with_database(&base, &name))
        .await
        .expect("the scratch database must be reachable");
    migration::Migrator::up(&db, None)
        .await
        .expect("migrations must apply");
    if set_up {
        domain::entities::instance_settings::ActiveModel {
            id: Set(services::instance_service::SETTINGS_ID),
            setup_completed_at: Set(Some(chrono::Utc::now().into())),
            ..Default::default()
        }
        .update(&db)
        .await
        .expect("the setup must be marked as run");
    }

    let outcome = tokio::spawn(body(db)).await;

    // `WITH (FORCE)`: a pooled connection, or a mail task still running,
    // can outlive the body by a moment.
    admin
        .execute_unprepared(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"))
        .await
        .expect("must be able to drop the scratch database");

    if let Err(join_error) = outcome {
        std::panic::resume_unwind(join_error.into_panic());
    }
}

// ---- Mail -----------------------------------------------------------------

/// One message as the SMTP server received it: its recipients, its subject
/// and its text part, decoded.
#[derive(Debug, Clone)]
pub(crate) struct SentMail {
    pub(crate) to: Vec<String>,
    pub(crate) subject: String,
    pub(crate) text: String,
}

/// A local SMTP server that accepts every message and keeps it, for the
/// tests that need to know which mail went out, if any. It speaks just
/// enough of the protocol for lettre, without TLS or authentication.
pub(crate) struct MailSink {
    port: u16,
    mails: Arc<std::sync::Mutex<Vec<SentMail>>>,
}

impl MailSink {
    pub(crate) async fn start() -> Self {
        Self::greeting_after(Some(std::time::Duration::ZERO)).await
    }

    /// A server that keeps every client waiting `delay` for its greeting:
    /// a mail takes at least that long to go out.
    pub(crate) async fn slow(delay: std::time::Duration) -> Self {
        Self::greeting_after(Some(delay)).await
    }

    /// A server that accepts the connection and never greets: a mail to it
    /// hangs until the client gives up.
    pub(crate) async fn silent() -> Self {
        Self::greeting_after(None).await
    }

    async fn greeting_after(delay: Option<std::time::Duration>) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a port must be free");
        let port = listener.local_addr().expect("an address").port();
        let mails = Arc::new(std::sync::Mutex::new(Vec::new()));
        let store = Arc::clone(&mails);
        tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                let store = Arc::clone(&store);
                tokio::spawn(async move {
                    match delay {
                        Some(delay) => tokio::time::sleep(delay).await,
                        // Held open, never answered.
                        None => {
                            let _socket = socket;
                            return std::future::pending().await;
                        }
                    }
                    let _ = serve_smtp(socket, store).await;
                });
            }
        });
        Self { port, mails }
    }

    /// The test configuration, with its mail going to this server.
    pub(crate) fn config(&self) -> Config {
        let mut config = config();
        config.email_backend = EmailBackend::Smtp;
        config.email_from = "noreply@ctf.example.test".to_owned();
        config.smtp_host = "127.0.0.1".to_owned();
        config.smtp_port = self.port;
        config.smtp_tls = SmtpTls::None;
        config
    }

    /// What arrived so far.
    pub(crate) fn mails(&self) -> Vec<SentMail> {
        self.mails.lock().expect("the sink lock").clone()
    }

    /// Waits for `count` messages, up to ten seconds, and returns what
    /// arrived.
    pub(crate) async fn wait_for(&self, count: usize) -> Vec<SentMail> {
        for _ in 0..200 {
            if self.mails().len() >= count {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        self.mails()
    }

    /// Lets the mail tasks of the requests sent so far finish, then returns
    /// what arrived: for the tests asserting that a mail did not go out.
    pub(crate) async fn settle(&self) -> Vec<SentMail> {
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        self.mails()
    }
}

async fn serve_smtp(
    socket: tokio::net::TcpStream,
    store: Arc<std::sync::Mutex<Vec<SentMail>>>,
) -> std::io::Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let (read, mut write) = socket.into_split();
    let mut reader = BufReader::new(read);
    write.write_all(b"220 sink ESMTP\r\n").await?;

    let mut to = Vec::new();
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).await? == 0 {
            return Ok(());
        }
        let command = line.trim_end().to_ascii_uppercase();
        if command.starts_with("EHLO") || command.starts_with("HELO") {
            write.write_all(b"250 sink\r\n").await?;
        } else if command.starts_with("RCPT TO:") {
            let address = line.trim_end()[8..].trim().trim_start_matches('<');
            to.push(address.split('>').next().unwrap_or_default().to_owned());
            write.write_all(b"250 OK\r\n").await?;
        } else if command == "DATA" {
            write.write_all(b"354 end with a dot\r\n").await?;
            let mut raw = String::new();
            loop {
                line.clear();
                if reader.read_line(&mut line).await? == 0 {
                    return Ok(());
                }
                if line == ".\r\n" || line == ".\n" {
                    break;
                }
                // Dot-stuffing (RFC 5321, 4.5.2).
                raw.push_str(line.strip_prefix('.').unwrap_or(&line));
            }
            store
                .lock()
                .expect("the sink lock")
                .push(parse_mail(std::mem::take(&mut to), &raw));
            write.write_all(b"250 OK\r\n").await?;
        } else if command == "QUIT" {
            write.write_all(b"221 bye\r\n").await?;
            return Ok(());
        } else {
            write.write_all(b"250 OK\r\n").await?;
        }
    }
}

/// The subject and the decoded text/plain part of a raw message.
fn parse_mail(to: Vec<String>, raw: &str) -> SentMail {
    let raw = raw.replace("\r\n", "\n");
    let (headers, _) = raw.split_once("\n\n").unwrap_or((&raw, ""));
    // Unfolded: a header continues on the lines that start with a space.
    let headers = headers.replace("\n ", " ").replace("\n\t", " ");
    let subject = headers
        .lines()
        .find_map(|line| line.strip_prefix("Subject: "))
        .map(decode_encoded_words)
        .unwrap_or_default();

    let text =
        raw.find("Content-Type: text/plain")
            .map(|start| {
                let part = &raw[start..];
                let (part_headers, content) = part.split_once("\n\n").unwrap_or((part, ""));
                let content = content
                    .split("\n--")
                    .next()
                    .unwrap_or_default()
                    .trim_end_matches('\n');
                let encoding = part_headers
                    .lines()
                    .find_map(|line| line.strip_prefix("Content-Transfer-Encoding: "))
                    .unwrap_or("7bit")
                    .trim()
                    .to_ascii_lowercase();
                match encoding.as_str() {
                    "base64" => String::from_utf8_lossy(&decode_base64(&content.replace('\n', "")))
                        .into_owned(),
                    "quoted-printable" => decode_quoted_printable(content),
                    _ => content.to_owned(),
                }
            })
            .unwrap_or_default();

    SentMail { to, subject, text }
}

/// `=?utf-8?b?...?=` and `=?utf-8?q?...?=` words, as lettre writes a subject
/// that is not ASCII. The space between two encoded words is dropped, every
/// other one kept (RFC 2047, 6.2).
fn decode_encoded_words(value: &str) -> String {
    let mut out = String::new();
    let mut previous_encoded = false;
    for (i, word) in value.split(' ').enumerate() {
        let decoded = word
            .strip_prefix("=?")
            .and_then(|w| w.strip_suffix("?="))
            .and_then(|inner| {
                let mut parts = inner.splitn(3, '?');
                let (_charset, kind, data) = (parts.next(), parts.next(), parts.next()?);
                match kind?.to_ascii_lowercase().as_str() {
                    "b" => Some(String::from_utf8_lossy(&decode_base64(data)).into_owned()),
                    "q" => Some(decode_quoted_printable(&data.replace('_', " "))),
                    _ => None,
                }
            });
        if i > 0 && !(previous_encoded && decoded.is_some()) {
            out.push(' ');
        }
        previous_encoded = decoded.is_some();
        out.push_str(&decoded.unwrap_or_else(|| word.to_owned()));
    }
    out
}

fn decode_base64(data: &str) -> Vec<u8> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bits = 0u32;
    let mut count = 0;
    let mut out = Vec::new();
    for byte in data.bytes() {
        let Some(value) = ALPHABET.iter().position(|&c| c == byte) else {
            continue;
        };
        bits = (bits << 6) | value as u32;
        count += 6;
        if count >= 8 {
            count -= 8;
            out.push((bits >> count) as u8);
        }
    }
    out
}

fn decode_quoted_printable(data: &str) -> String {
    let joined = data.replace("=\n", "");
    let bytes = joined.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'='
            && let Some(hex) = joined.get(i + 1..i + 3)
            && let Ok(value) = u8::from_str_radix(hex, 16)
        {
            out.push(value);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Two `db()` in one test both come back: the first one left the
    /// migration lock free. The second call is bounded, so a lock left held
    /// fails this test within seconds instead of hanging the suite. The lock
    /// used to go through the pool, and stayed held only on the runs where
    /// its release landed on another pooled connection than the one holding
    /// it.
    #[tokio::test]
    async fn db_can_be_called_twice_in_one_test() {
        let first = db().await;
        let second = tokio::time::timeout(Duration::from_secs(5), db())
            .await
            .expect("a second db() must not wait on a lock the first one left held");

        for connection in [&first, &second] {
            connection
                .ping()
                .await
                .expect("both connections must answer");
        }
    }
}

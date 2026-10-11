//! Integration tests for the personal team: created on email verification with
//! its creator as Captain, never twice, never for an instance admin, and the
//! migration that backfills the Captains the old code never inserted.
//!
//! Real Postgres throughout: the guards under test are a unique index, a
//! savepoint and a transaction boundary, none of which `MockDatabase`
//! executes. The setup and migration tests each work in a throwaway database
//! of their own, because both need a state the shared database is past (an
//! instance not set up yet, a schema one migration behind). CI provisions the
//! instance and sets `DATABASE_URL`; locally the dev compose exposes one.

use std::sync::atomic::{AtomicU32, Ordering};

use domain::dto::setup::SetupRequest;
use domain::dto::team::TeamRole;
use domain::entities::{team_members, teams, user_emails, users};
use domain::types::{AccentColor, TokenKind};
use migration::MigratorTrait;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, Database, DatabaseConnection, EntityTrait,
    IsolationLevel, PaginatorTrait, QueryFilter, Set, TransactionTrait,
};
use services::audit_service::AuditContext;
use services::config::{Config, EmailBackend, SmtpTls, StorageConfig};
use services::{auth_service, auth_token_service, setup_service, team_service};
use uuid::Uuid;

fn base_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://pwnforge:changeme@127.0.0.1:5432/pwnforge".to_owned())
}

async fn connect() -> DatabaseConnection {
    let db = Database::connect(base_url())
        .await
        .expect("a Postgres instance must be reachable (see the module docs)");

    migration::Migrator::up(&db, None)
        .await
        .expect("migrations must apply");

    db
}

fn unique_suffix() -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nth = COUNTER.fetch_add(1, Ordering::Relaxed);

    format!("{}{nth}", &Uuid::now_v7().simple().to_string()[20..])
}

async fn create_user(db: &impl ConnectionTrait, is_instance_admin: bool) -> users::Model {
    users::ActiveModel {
        id: Set(Uuid::now_v7()),
        username: Set(format!("pt-{}", unique_suffix())),
        password_hash: Set("not-a-real-hash".to_owned()),
        locale: Set("en".to_owned()),
        is_instance_admin: Set(is_instance_admin),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture user must insert")
}

async fn create_address(db: &impl ConnectionTrait, user_id: Uuid) -> user_emails::Model {
    user_emails::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(user_id),
        email: Set(format!("pt-{}@example.test", unique_suffix())),
        is_primary: Set(false),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture address must insert")
}

/// Issues a verification token for the address and redeems it, the way the
/// verification link does.
async fn verify(db: &DatabaseConnection, user_id: Uuid, email_id: Uuid) {
    let secret = auth_token_service::issue(db, user_id, email_id, TokenKind::EmailVerification)
        .await
        .expect("a verification token must be issued");

    auth_service::verify_email(db, &secret)
        .await
        .expect("the address must verify");
}

async fn personal_teams_of(db: &DatabaseConnection, user_id: Uuid) -> Vec<teams::Model> {
    teams::Entity::find()
        .filter(teams::Column::CreatedBy.eq(user_id))
        .filter(teams::Column::Personal.eq(true))
        .all(db)
        .await
        .expect("the lookup must succeed")
}

async fn members_of(db: &DatabaseConnection, team_id: Uuid) -> Vec<team_members::Model> {
    team_members::Entity::find()
        .filter(team_members::Column::TeamId.eq(team_id))
        .all(db)
        .await
        .expect("the lookup must succeed")
}

/// Verifying an address creates the personal team, makes its owner the
/// Captain, and so puts it in the owner's own team list.
#[tokio::test]
async fn verifying_an_address_makes_the_user_captain_of_their_personal_team() {
    let db = connect().await;
    let user = create_user(&db, false).await;
    let email = create_address(&db, user.id).await;

    verify(&db, user.id, email.id).await;

    let teams = personal_teams_of(&db, user.id).await;
    assert_eq!(teams.len(), 1, "exactly one personal team");
    let team = &teams[0];

    let members = members_of(&db, team.id).await;
    assert_eq!(members.len(), 1, "the creator is the only member");
    assert_eq!(members[0].user_id, user.id);
    assert_eq!(members[0].role, TeamRole::Captain.as_str());

    let listed = team_service::list_teams_for_user(&db, user.id)
        .await
        .expect("the list must load");
    let mine = listed
        .iter()
        .find(|listed| listed.id == team.id)
        .expect("the personal team must be in its owner's list");
    assert_eq!(mine.my_role, TeamRole::Captain);
}

/// A second verified address runs the same path again and must add neither a
/// team nor a member.
#[tokio::test]
async fn a_second_verification_creates_no_second_team_or_member() {
    let db = connect().await;
    let user = create_user(&db, false).await;
    let first = create_address(&db, user.id).await;
    let second = create_address(&db, user.id).await;

    verify(&db, user.id, first.id).await;
    verify(&db, user.id, second.id).await;

    let teams = personal_teams_of(&db, user.id).await;
    assert_eq!(teams.len(), 1, "still exactly one personal team");
    assert_eq!(
        members_of(&db, teams[0].id).await.len(),
        1,
        "still exactly one member"
    );
}

/// An instance admin who verifies an address gets no personal team, and the
/// verification itself still lands.
#[tokio::test]
async fn an_instance_admin_who_verifies_an_address_gets_no_personal_team() {
    let db = connect().await;
    let admin = create_user(&db, true).await;
    let email = create_address(&db, admin.id).await;

    verify(&db, admin.id, email.id).await;

    assert!(
        personal_teams_of(&db, admin.id).await.is_empty(),
        "an instance admin must not get a personal team"
    );
    let verified = user_emails::Entity::find_by_id(email.id)
        .one(&db)
        .await
        .expect("the lookup must succeed")
        .expect("the address must exist");
    assert!(
        verified.verified_at.is_some(),
        "the address is still verified"
    );
}

/// The interleaving the unique index exists for: a transaction that looked
/// before another one committed the team, and inserts after. Its insert is
/// refused; that must cost it nothing but the team.
///
/// Made deterministic with a REPEATABLE READ snapshot: `late` takes its
/// snapshot, `early` creates and commits the team, and `late`'s lookup still
/// sees no team. The write `late` made before the call stands in for the
/// address verification it carries in production, and must survive the
/// commit.
#[tokio::test]
async fn a_concurrent_creation_loses_only_the_team_not_the_transaction() {
    let db = connect().await;
    let user = create_user(&db, false).await;
    let email = create_address(&db, user.id).await;

    let late = db
        .begin_with_config(Some(IsolationLevel::RepeatableRead), None)
        .await
        .expect("the late transaction must begin");
    user_emails::ActiveModel {
        id: Set(email.id),
        is_primary: Set(true),
        ..Default::default()
    }
    .update(&late)
    .await
    .expect("the late transaction's own write must succeed");

    let early = db.begin().await.expect("the early transaction must begin");
    let created = team_service::create_personal_team(&early, &user)
        .await
        .expect("the early creation must succeed");
    assert!(created.is_some(), "the early transaction creates the team");
    early
        .commit()
        .await
        .expect("the early transaction must commit");

    let raced = team_service::create_personal_team(&late, &user)
        .await
        .expect("losing the race is not an error");
    assert!(raced.is_none(), "the late transaction creates nothing");
    late.commit()
        .await
        .expect("the late transaction must still commit");

    let address = user_emails::Entity::find_by_id(email.id)
        .one(&db)
        .await
        .expect("the lookup must succeed")
        .expect("the address must exist");
    assert!(
        address.is_primary,
        "the late transaction's earlier write must have been committed"
    );

    let teams = personal_teams_of(&db, user.id).await;
    assert_eq!(teams.len(), 1, "one team");
    assert_eq!(members_of(&db, teams[0].id).await.len(), 1, "one member");
}

// ---- Throwaway databases ------------------------------------------------

fn with_database(url: &str, database: &str) -> String {
    let (prefix, _old) = url
        .rsplit_once('/')
        .expect("DATABASE_URL must contain a database name");

    format!("{prefix}/{database}")
}

async fn admin_connection() -> DatabaseConnection {
    Database::connect(with_database(&base_url(), "postgres"))
        .await
        .expect("a Postgres instance must be reachable (see the module docs)")
}

/// Runs `body` against a fresh, empty database and drops it afterwards, even
/// when an assertion fails (the body runs in a task so a panic comes back as
/// a `JoinError` instead of unwinding past the cleanup).
async fn in_scratch_database<F, Fut>(body: F)
where
    F: FnOnce(DatabaseConnection) -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    let name = format!("pwnforge_personal_team_test_{}", Uuid::now_v7().simple());
    admin_connection()
        .await
        .execute_unprepared(&format!("CREATE DATABASE {name}"))
        .await
        .expect("must be able to create a scratch database");

    let db = Database::connect(with_database(&base_url(), &name))
        .await
        .expect("the scratch database must be reachable");

    let outcome = tokio::spawn(body(db)).await;

    admin_connection()
        .await
        .execute_unprepared(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"))
        .await
        .expect("must be able to drop the scratch database");

    if let Err(join_error) = outcome {
        std::panic::resume_unwind(join_error.into_panic());
    }
}

fn setup_config() -> Config {
    Config {
        port: 0,
        public_url: "http://localhost".to_owned(),
        cors_origin: "http://localhost:5173".to_owned(),
        setup_token: None,
        database_url: String::new(),
        database_max_connections: 5,
        redis_url: String::new(),
        jwt_secret: "x".repeat(32),
        encryption_key: "x".repeat(32),
        turnstile_site_key: None,
        turnstile_secret_key: None,
        cookie_secure: false,
        trusted_proxy_count: 0,
        block_disposable_emails: false,
        // No network call to the breach database from a test.
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
        // Not used by setup.
        storage: StorageConfig {
            endpoint: None,
            region: "garage".to_owned(),
            access_key_id: "unused".to_owned(),
            secret_access_key: "unused".to_owned(),
            force_path_style: true,
            bucket_attachments: "unused-attachments".to_owned(),
            bucket_avatars: "unused-avatars".to_owned(),
            key_prefix: None,
            public_base_url: None,
        },
    }
}

/// The administrator created by the setup wizard is an administration
/// account, and gets no personal team.
#[tokio::test]
async fn setup_creates_no_personal_team_for_the_admin() {
    in_scratch_database(|db| async move {
        migration::Migrator::up(&db, None)
            .await
            .expect("migrations must apply");

        let ctx = AuditContext {
            actor_id: None,
            ip_address: None,
            user_agent: None,
        };
        let payload = SetupRequest {
            admin_username: "instanceadmin".to_owned(),
            admin_email: "admin@example.test".to_owned(),
            admin_password: "Correct-Horse-Battery-9".to_owned(),
            default_locale: "en".to_owned(),
            default_timezone: "Europe/Paris".to_owned(),
            allow_public_signup: false,
            hide_landing_page: false,
        };

        let (admin, _email) = setup_service::complete(&db, &setup_config(), &ctx, payload)
            .await
            .expect("setup must complete on a fresh instance");

        assert!(admin.is_instance_admin);
        assert_eq!(
            teams::Entity::find()
                .count(&db)
                .await
                .expect("count must succeed"),
            0,
            "setup must create no team at all"
        );

        db.close().await.ok();
    })
    .await;
}

const BACKFILL: &str = "m20261008_120000_personal_team_captains";

/// The backfill inserts the missing Captain of a personal team and nothing
/// else: a personal team with a member, an ordinary team, an orphaned
/// personal team and an instance admin's personal team are left alone.
/// `down()` runs and removes nothing.
#[tokio::test]
async fn the_backfill_adds_missing_captains_and_nothing_else() {
    in_scratch_database(|db| async move {
        // Every migration before the backfill, found by name: an index would
        // point at another migration as soon as one is appended.
        let before_backfill = migration::Migrator::migrations()
            .iter()
            .position(|m| m.name() == BACKFILL)
            .expect("the backfill must be registered in the Migrator");
        migration::Migrator::up(&db, Some(before_backfill as u32))
            .await
            .expect("the earlier migrations must apply");

        let team = |slug: &str, personal: bool, created_by: Option<Uuid>| teams::ActiveModel {
            id: Set(Uuid::now_v7()),
            slug: Set(slug.to_owned()),
            name: Set(slug.to_owned()),
            color: Set(AccentColor::default().as_str().to_owned()),
            personal: Set(personal),
            created_by: Set(created_by),
            ..Default::default()
        };

        let empty_owner = create_user(&db, false).await;
        let empty = team("empty-personal", true, Some(empty_owner.id))
            .insert(&db)
            .await
            .expect("fixture team");

        let staffed_owner = create_user(&db, false).await;
        let staffed = team("staffed-personal", true, Some(staffed_owner.id))
            .insert(&db)
            .await
            .expect("fixture team");
        // Someone already holds it, and not as Captain: it must stay as is.
        team_members::ActiveModel {
            team_id: Set(staffed.id),
            user_id: Set(staffed_owner.id),
            role: Set(TeamRole::Admin.as_str().to_owned()),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("fixture member");

        let ordinary_owner = create_user(&db, false).await;
        let ordinary = team("ordinary", false, Some(ordinary_owner.id))
            .insert(&db)
            .await
            .expect("fixture team");

        let orphan = team("orphan-personal", true, None)
            .insert(&db)
            .await
            .expect("fixture team");

        // Made by the old code before it refused instance admins.
        let admin = create_user(&db, true).await;
        let admins = team("admin-personal", true, Some(admin.id))
            .insert(&db)
            .await
            .expect("fixture team");

        // Exactly one step: the backfill and nothing appended after it.
        migration::Migrator::up(&db, Some(1))
            .await
            .expect("the backfill must apply");

        let members = |team_id: Uuid| {
            let db = &db;
            async move {
                team_members::Entity::find()
                    .filter(team_members::Column::TeamId.eq(team_id))
                    .all(db)
                    .await
                    .expect("the lookup must succeed")
            }
        };

        let backfilled = members(empty.id).await;
        assert_eq!(
            backfilled.len(),
            1,
            "the empty personal team gets one member"
        );
        assert_eq!(backfilled[0].user_id, empty_owner.id);
        assert_eq!(backfilled[0].role, TeamRole::Captain.as_str());

        let kept = members(staffed.id).await;
        assert_eq!(kept.len(), 1, "a staffed team gains no member");
        assert_eq!(kept[0].role, TeamRole::Admin.as_str(), "nor changes role");

        assert!(
            members(ordinary.id).await.is_empty(),
            "ordinary teams are ignored"
        );
        assert!(
            members(orphan.id).await.is_empty(),
            "an orphan has no one to add"
        );
        assert!(
            members(admins.id).await.is_empty(),
            "an instance admin is not made Captain of a personal team"
        );

        // The backfill is the last migration applied here, whatever comes
        // after it in the Migrator.
        migration::Migrator::down(&db, Some(1))
            .await
            .expect("down must run");
        assert_eq!(
            members(empty.id).await.len(),
            1,
            "down removes nothing it cannot tell apart"
        );

        db.close().await.ok();
    })
    .await;
}

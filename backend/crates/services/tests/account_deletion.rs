//! Deleting an account: the scheduled purge and the immediate deletion both
//! take the account's solo workspaces and its personal team with it, objects
//! included, leave a team the account merely belongs to alone, go on past an
//! account that cannot be deleted, and keep the deletion when the object
//! store does not answer.
//!
//! Every test works in a throwaway database: the purge deletes every account
//! past its deadline, and the shared database holds the accounts of other
//! tests, and of whoever develops on it. The objects go to the bucket the
//! backend is configured with, under ids no other test uses.

use chrono::Utc;
use domain::entities::{team_members, teams, users, workspaces};
use migration::MigratorTrait;
use redis::aio::ConnectionManager;
use sea_orm::{ActiveModelTrait, ConnectionTrait, DatabaseConnection, EntityTrait, Set};
use services::admin_service::{self, DeletionKind, DeletionRequest, PurgeOutcome};
use services::audit_service::AuditContext;
use services::avatar_service;
use services::config::{Config, EmailBackend, SmtpTls};
use services::storage_service::{
    AvatarKind, AvatarSize, Bucket, ObjectStore, ObjectsToPurge, S3Store, StorageError, avatar_key,
};
use uuid::Uuid;

#[path = "common/scratch.rs"]
mod scratch;
#[path = "common/storage.rs"]
mod storage;

use scratch::in_scratch_database;
use storage::storage_config;

/// A real 2x2 RGBA PNG, so the upload goes through the decoder.
const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x08, 0x06, 0x00, 0x00, 0x00, 0x72, 0xb6, 0x0d,
    0x24, 0x00, 0x00, 0x00, 0x16, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x60, 0x60, 0x68, 0xf8,
    0x9f, 0x02, 0xc4, 0x0c, 0x0c, 0x29, 0x40, 0x06, 0x10, 0x03, 0x00, 0x39, 0x80, 0x07, 0x8d, 0xd8,
    0xcd, 0x2b, 0xa8, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

fn store() -> ObjectStore {
    ObjectStore::S3(S3Store::new(&storage_config()))
}

/// The same bucket behind a credential it does not know: every request is
/// refused, so every purge fails.
fn refusing_store() -> ObjectStore {
    let mut config = storage_config();
    config.access_key_id = "GK000000000000000000000000".to_owned();
    config.secret_access_key = "0".repeat(64);
    ObjectStore::S3(S3Store::new(&config))
}

async fn migrated(db: &DatabaseConnection) {
    migration::Migrator::up(db, None)
        .await
        .expect("migrations must apply");
}

/// An account, past its purge deadline when `scheduled`.
async fn create_user(db: &DatabaseConnection, scheduled: bool) -> Uuid {
    create_user_with(
        db,
        Uuid::now_v7(),
        &format!("del-{}", Uuid::now_v7().simple()),
        scheduled,
    )
    .await
}

async fn create_user_with(
    db: &DatabaseConnection,
    id: Uuid,
    username: &str,
    scheduled: bool,
) -> Uuid {
    let due = (Utc::now() - chrono::Duration::days(1)).fixed_offset();

    users::ActiveModel {
        id: Set(id),
        username: Set(username.to_owned()),
        password_hash: Set("not-a-real-hash".to_owned()),
        locale: Set("en".to_owned()),
        purge_scheduled_at: Set(scheduled.then_some(due)),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture user must insert")
    .id
}

async fn create_team(db: &DatabaseConnection, creator: Uuid, personal: bool) -> Uuid {
    let team = teams::ActiveModel {
        id: Set(Uuid::now_v7()),
        slug: Set(format!("del-{}", Uuid::now_v7().simple())),
        name: Set("Deletion".to_owned()),
        color: Set("ember".to_owned()),
        personal: Set(personal),
        created_by: Set(Some(creator)),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture team must insert");

    add_member(db, team.id, creator, "captain").await;
    team.id
}

async fn add_member(db: &DatabaseConnection, team_id: Uuid, user_id: Uuid, role: &str) {
    team_members::ActiveModel {
        team_id: Set(team_id),
        user_id: Set(user_id),
        role: Set(role.to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture membership must insert");
}

async fn create_workspace(
    db: &DatabaseConnection,
    team_id: Option<Uuid>,
    owner_id: Option<Uuid>,
) -> Uuid {
    workspaces::ActiveModel {
        id: Set(Uuid::now_v7()),
        team_id: Set(team_id),
        owner_id: Set(owner_id),
        slug: Set(format!("del-{}", Uuid::now_v7().simple())),
        name: Set("Deletion".to_owned()),
        color: Set("ember".to_owned()),
        r#type: Set("event".to_owned()),
        mode: Set(if team_id.is_some() { "team" } else { "solo" }.to_owned()),
        default_writeup_visibility: Set("draft".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture workspace must insert")
    .id
}

/// Gives an entity an avatar in the real bucket, and returns what `stored`
/// needs to look for it.
async fn upload(db: &DatabaseConnection, kind: AvatarKind, id: Uuid) -> (AvatarKind, Uuid, Uuid) {
    let version = avatar_service::upload(db, &store(), kind, id, TINY_PNG.to_vec())
        .await
        .expect("the avatar upload must succeed");
    assert!(
        stored(kind, id, version).await,
        "control: the avatar is stored"
    );
    (kind, id, version)
}

async fn stored(kind: AvatarKind, id: Uuid, version: Uuid) -> bool {
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

async fn user_exists(db: &DatabaseConnection, id: Uuid) -> bool {
    users::Entity::find_by_id(id)
        .one(db)
        .await
        .expect("the lookup must succeed")
        .is_some()
}

async fn team_exists(db: &DatabaseConnection, id: Uuid) -> bool {
    teams::Entity::find_by_id(id)
        .one(db)
        .await
        .expect("the lookup must succeed")
        .is_some()
}

async fn workspace_exists(db: &DatabaseConnection, id: Uuid) -> bool {
    workspaces::Entity::find_by_id(id)
        .one(db)
        .await
        .expect("the lookup must succeed")
        .is_some()
}

async fn purge(db: &DatabaseConnection, store: &ObjectStore) -> PurgeOutcome {
    admin_service::purge_scheduled_deletions(db, store)
        .await
        .expect("the purge pass must run")
}

#[tokio::test]
async fn the_purge_deletes_an_account_with_its_solo_workspace_and_their_objects() {
    in_scratch_database(|db| async move {
        migrated(&db).await;
        let user = create_user(&db, true).await;
        let workspace = create_workspace(&db, None, Some(user)).await;
        let avatars = [
            upload(&db, AvatarKind::User, user).await,
            upload(&db, AvatarKind::Workspace, workspace).await,
        ];

        assert_eq!(
            purge(&db, &store()).await,
            PurgeOutcome {
                purged: 1,
                failed: 0
            }
        );

        assert!(!user_exists(&db, user).await, "the account must be gone");
        assert!(
            !workspace_exists(&db, workspace).await,
            "its solo workspace must be gone"
        );
        for (kind, id, version) in avatars {
            assert!(
                !stored(kind, id, version).await,
                "the {kind:?} avatar must be purged"
            );
        }

        db.close().await.ok();
    })
    .await;
}

#[tokio::test]
async fn an_immediate_deletion_deletes_the_solo_workspace_and_their_objects() {
    in_scratch_database(|db| async move {
        migrated(&db).await;
        let user = create_user(&db, false).await;
        let workspace = create_workspace(&db, None, Some(user)).await;
        let avatars = [
            upload(&db, AvatarKind::User, user).await,
            upload(&db, AvatarKind::Workspace, workspace).await,
        ];

        let client = redis::Client::open(
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_owned()),
        )
        .expect("REDIS_URL must be a valid Redis URL");
        let mut redis = ConnectionManager::new(client)
            .await
            .expect("a Redis instance must be reachable");
        let ctx = AuditContext {
            actor_id: None,
            ip_address: None,
            user_agent: None,
        };

        admin_service::delete_user(
            &db,
            &mut redis,
            &store(),
            &config(),
            &ctx,
            user,
            DeletionRequest {
                kind: DeletionKind::IllicitContent,
                reason: Some("account deletion test"),
            },
        )
        .await
        .expect("the immediate deletion must succeed");

        assert!(!user_exists(&db, user).await, "the account must be gone");
        assert!(
            !workspace_exists(&db, workspace).await,
            "its solo workspace must be gone"
        );
        for (kind, id, version) in avatars {
            assert!(
                !stored(kind, id, version).await,
                "the {kind:?} avatar must be purged"
            );
        }

        db.close().await.ok();
    })
    .await;
}

#[tokio::test]
async fn the_purge_deletes_the_personal_team_with_its_workspaces_and_objects() {
    in_scratch_database(|db| async move {
        migrated(&db).await;
        let user = create_user(&db, true).await;
        let team = create_team(&db, user, true).await;
        let workspace = create_workspace(&db, Some(team), None).await;
        let avatars = [
            upload(&db, AvatarKind::Team, team).await,
            upload(&db, AvatarKind::Workspace, workspace).await,
        ];

        assert_eq!(
            purge(&db, &store()).await,
            PurgeOutcome {
                purged: 1,
                failed: 0
            }
        );

        assert!(
            !team_exists(&db, team).await,
            "the personal team must be gone"
        );
        assert!(
            !workspace_exists(&db, workspace).await,
            "its workspace must be gone"
        );
        for (kind, id, version) in avatars {
            assert!(
                !stored(kind, id, version).await,
                "the {kind:?} avatar must be purged"
            );
        }

        db.close().await.ok();
    })
    .await;
}

/// A team the account belongs to without having made it personal stays,
/// with its workspace, its objects and its other members.
#[tokio::test]
async fn a_team_the_account_only_belongs_to_survives_the_purge() {
    in_scratch_database(|db| async move {
        migrated(&db).await;
        let captain = create_user(&db, false).await;
        let team = create_team(&db, captain, false).await;
        let workspace = create_workspace(&db, Some(team), None).await;
        let (kind, id, version) = upload(&db, AvatarKind::Workspace, workspace).await;

        let user = create_user(&db, true).await;
        add_member(&db, team, user, "member").await;

        assert_eq!(
            purge(&db, &store()).await,
            PurgeOutcome {
                purged: 1,
                failed: 0
            }
        );

        assert!(!user_exists(&db, user).await, "the account must be gone");
        assert!(team_exists(&db, team).await, "the shared team must stay");
        assert!(
            workspace_exists(&db, workspace).await,
            "its workspace must stay"
        );
        assert!(
            stored(kind, id, version).await,
            "its workspace's avatar must stay"
        );
        assert!(
            team_members::Entity::find_by_id((team, captain))
                .one(&db)
                .await
                .expect("the lookup must succeed")
                .is_some(),
            "the other member must stay"
        );

        store()
            .purge(ObjectsToPurge::avatar(AvatarKind::Workspace, workspace))
            .await;
        db.close().await.ok();
    })
    .await;
}

/// The first account in the pass cannot be deleted: a trigger in this
/// throwaway database refuses it, and nothing in the service knows. The two
/// after it are purged anyway, and the pass reports both counts.
#[tokio::test]
async fn an_account_that_fails_does_not_stop_the_purge_of_the_next() {
    in_scratch_database(|db| async move {
        migrated(&db).await;
        db.execute_unprepared(
            "CREATE FUNCTION refuse_this_deletion() RETURNS trigger LANGUAGE plpgsql AS $$ \
             BEGIN RAISE EXCEPTION 'deletion refused by the test'; END $$",
        )
        .await
        .expect("the trigger function must be created");
        db.execute_unprepared(
            "CREATE TRIGGER refuse_deletion BEFORE DELETE ON users FOR EACH ROW \
             WHEN (OLD.username = 'refused-by-the-test') EXECUTE FUNCTION refuse_this_deletion()",
        )
        .await
        .expect("the trigger must be created");

        // Fixed ids, so the refused account is the first the pass meets.
        let refused = create_user_with(&db, Uuid::from_u128(1), "refused-by-the-test", true).await;
        let second = create_user_with(&db, Uuid::from_u128(2), "purged-second", true).await;
        let third = create_user_with(&db, Uuid::from_u128(3), "purged-third", true).await;

        assert_eq!(
            purge(&db, &store()).await,
            PurgeOutcome {
                purged: 2,
                failed: 1
            }
        );

        assert!(
            user_exists(&db, refused).await,
            "the refused account stays for the next pass"
        );
        assert!(!user_exists(&db, second).await, "the second is purged");
        assert!(!user_exists(&db, third).await, "the third is purged");

        db.close().await.ok();
    })
    .await;
}

/// The object store refuses everything: the deletion stays committed and
/// the objects stay where they were.
#[tokio::test]
async fn an_object_store_failure_keeps_the_deletion() {
    in_scratch_database(|db| async move {
        migrated(&db).await;
        let user = create_user(&db, true).await;
        let workspace = create_workspace(&db, None, Some(user)).await;
        let (kind, id, version) = upload(&db, AvatarKind::Workspace, workspace).await;

        assert_eq!(
            purge(&db, &refusing_store()).await,
            PurgeOutcome {
                purged: 1,
                failed: 0
            }
        );

        assert!(!user_exists(&db, user).await, "the account stays deleted");
        assert!(
            !workspace_exists(&db, workspace).await,
            "the workspace stays deleted"
        );
        assert!(
            stored(kind, id, version).await,
            "the refused purge left the avatar in the bucket"
        );

        store()
            .purge(ObjectsToPurge::avatar(AvatarKind::Workspace, workspace))
            .await;
        db.close().await.ok();
    })
    .await;
}

/// Only what `delete_user` reaches: mail goes to the console, and the
/// account has no address to send to anyway.
fn config() -> Config {
    Config {
        port: 0,
        public_url: "http://localhost".to_owned(),
        cors_origin: "http://localhost:5173".to_owned(),
        setup_token: None,
        database_url: String::new(),
        database_max_connections: 1,
        redis_url: String::new(),
        jwt_secret: "x".repeat(32),
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

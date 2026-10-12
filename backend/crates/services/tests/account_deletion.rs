//! Deleting an account: the scheduled purge and the immediate deletion both
//! take the account's solo workspaces and its personal team with it, objects
//! included, hand each other team it is still Captain of to a successor,
//! active members first, or delete it when nobody else is left, leave a team
//! the account merely belongs to alone, go on past an account that cannot be
//! deleted, keep the deletion when the object store does not answer, and
//! name the account by id only in their audit entries. A deletion request
//! is refused while the account captains a team with other members.
//!
//! Every test works in a throwaway database: the purge deletes every account
//! past its deadline, and the shared database holds the accounts of other
//! tests, and of whoever develops on it. The objects go to the bucket the
//! backend is configured with, under ids no other test uses.

use chrono::{DateTime, Duration, Utc};
use domain::entities::{instance_audit_logs, team_members, teams, users, workspaces};
use migration::MigratorTrait;
use redis::aio::ConnectionManager;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, Set,
};
use services::admin_service::{self, AdminError, DeletionKind, DeletionRequest, PurgeOutcome};
use services::audit_service::AuditContext;
use services::avatar_service;
use services::storage_service::{
    AvatarKind, AvatarSize, Bucket, ObjectStore, ObjectsToPurge, S3Store, StorageError, avatar_key,
};
use services::team_service::{self, TransferBy};
use uuid::Uuid;

#[path = "common/config.rs"]
mod config;
#[path = "common/scratch.rs"]
mod scratch;
#[path = "common/storage.rs"]
mod storage;

use config::config;
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

/// No actor: what the CLI records without `--actor`.
fn ctx() -> AuditContext {
    AuditContext {
        actor_id: None,
        ip_address: None,
        user_agent: None,
    }
}

/// `admin_service::delete_user`, the way the admin CLI calls it.
async fn delete(db: &DatabaseConnection, user: Uuid, kind: DeletionKind) -> Result<(), AdminError> {
    let client = redis::Client::open(
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_owned()),
    )
    .expect("REDIS_URL must be a valid Redis URL");
    let mut redis = ConnectionManager::new(client)
        .await
        .expect("a Redis instance must be reachable");

    admin_service::delete_user(
        db,
        &mut redis,
        &store(),
        &config(),
        &ctx(),
        user,
        DeletionRequest {
            kind,
            reason: Some("account deletion test"),
        },
    )
    .await
}

async fn add_member_at(
    db: &DatabaseConnection,
    team_id: Uuid,
    user_id: Uuid,
    role: &str,
    joined_at: DateTime<Utc>,
) {
    team_members::ActiveModel {
        team_id: Set(team_id),
        user_id: Set(user_id),
        role: Set(role.to_owned()),
        joined_at: Set(joined_at.fixed_offset()),
    }
    .insert(db)
    .await
    .expect("the fixture membership must insert");
}

/// The member's role, `None` once they are off the team.
async fn role_of(db: &DatabaseConnection, team_id: Uuid, user_id: Uuid) -> Option<String> {
    team_members::Entity::find_by_id((team_id, user_id))
        .one(db)
        .await
        .expect("the lookup must succeed")
        .map(|member| member.role)
}

async fn scheduled(db: &DatabaseConnection, user: Uuid) -> bool {
    users::Entity::find_by_id(user)
        .one(db)
        .await
        .expect("the lookup must succeed")
        .expect("the account must exist")
        .purge_scheduled_at
        .is_some()
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

        delete(&db, user, DeletionKind::IllicitContent)
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

/// One account, Captain of three teams other than its own, each a case of
/// the succession.
struct Captaincies {
    captain: Uuid,
    /// An Admin is preferred over a member who joined earlier.
    with_admins: Uuid,
    veteran_member: Uuid,
    oldest_admin: Uuid,
    newer_admin: Uuid,
    /// No Admin: the longest-standing member, and on a tie the lower id.
    without_admins: Uuid,
    first_by_id: Uuid,
    second_by_id: Uuid,
    /// The account is the only member: the team goes with it.
    alone: Uuid,
    alone_workspace: Uuid,
    alone_avatars: [(AvatarKind, Uuid, Uuid); 2],
}

async fn captain_of_three_teams(db: &DatabaseConnection, scheduled: bool) -> Captaincies {
    let now = Utc::now();
    let captain = create_user(db, scheduled).await;

    let with_admins = create_team(db, captain, false).await;
    let veteran_member = create_user(db, false).await;
    let oldest_admin = create_user(db, false).await;
    let newer_admin = create_user(db, false).await;
    add_member_at(
        db,
        with_admins,
        veteran_member,
        "member",
        now - Duration::days(30),
    )
    .await;
    add_member_at(
        db,
        with_admins,
        oldest_admin,
        "admin",
        now - Duration::days(20),
    )
    .await;
    add_member_at(
        db,
        with_admins,
        newer_admin,
        "admin",
        now - Duration::days(10),
    )
    .await;

    // Fixed ids below every v7 one, so the tie is broken by id and nothing
    // else; the newcomer joined later and never comes into it.
    let without_admins = create_team(db, captain, false).await;
    let first_by_id = create_user_with(db, Uuid::from_u128(0x10), "tie-first", false).await;
    let second_by_id = create_user_with(db, Uuid::from_u128(0x20), "tie-second", false).await;
    let newcomer = create_user(db, false).await;
    let tie = now - Duration::days(5);
    add_member_at(db, without_admins, second_by_id, "viewer", tie).await;
    add_member_at(db, without_admins, first_by_id, "member", tie).await;
    add_member_at(
        db,
        without_admins,
        newcomer,
        "member",
        now - Duration::days(1),
    )
    .await;

    let alone = create_team(db, captain, false).await;
    let alone_workspace = create_workspace(db, Some(alone), None).await;
    let alone_avatars = [
        upload(db, AvatarKind::Team, alone).await,
        upload(db, AvatarKind::Workspace, alone_workspace).await,
    ];

    Captaincies {
        captain,
        with_admins,
        veteran_member,
        oldest_admin,
        newer_admin,
        without_admins,
        first_by_id,
        second_by_id,
        alone,
        alone_workspace,
        alone_avatars,
    }
}

/// What both deletion paths must leave behind, audit entries included.
async fn assert_succession(db: &DatabaseConnection, c: &Captaincies) {
    assert!(
        !user_exists(db, c.captain).await,
        "the account must be gone"
    );

    assert_eq!(
        role_of(db, c.with_admins, c.oldest_admin).await.as_deref(),
        Some("captain"),
        "the longest-standing Admin must be Captain"
    );
    assert_eq!(
        role_of(db, c.with_admins, c.newer_admin).await.as_deref(),
        Some("admin")
    );
    assert_eq!(
        role_of(db, c.with_admins, c.veteran_member)
            .await
            .as_deref(),
        Some("member"),
        "a member who joined earlier does not pass an Admin"
    );

    assert_eq!(
        role_of(db, c.without_admins, c.first_by_id)
            .await
            .as_deref(),
        Some("captain"),
        "with no Admin, the longest-standing member, the lower id on a tie"
    );
    assert_eq!(
        role_of(db, c.without_admins, c.second_by_id)
            .await
            .as_deref(),
        Some("viewer")
    );

    assert!(
        !team_exists(db, c.alone).await,
        "the team it was alone in must be gone"
    );
    assert!(
        !workspace_exists(db, c.alone_workspace).await,
        "with its workspace"
    );
    for (kind, id, version) in c.alone_avatars {
        assert!(
            !stored(kind, id, version).await,
            "the {kind:?} avatar must be purged"
        );
    }

    let promotions = instance_audit_logs::Entity::find()
        .filter(instance_audit_logs::Column::Action.eq("team.captain_promoted"))
        .order_by_asc(instance_audit_logs::Column::Id)
        .all(db)
        .await
        .expect("the audit lookup must succeed");
    let mut recorded: Vec<(Option<Uuid>, Option<Uuid>)> = promotions
        .iter()
        .map(|entry| (entry.target_team_id, entry.target_user_id))
        .collect();
    recorded.sort();
    let mut expected = vec![
        (Some(c.with_admins), Some(c.oldest_admin)),
        (Some(c.without_admins), Some(c.first_by_id)),
    ];
    expected.sort();
    assert_eq!(recorded, expected, "each promotion must be audited");
    for entry in promotions {
        assert_eq!(
            entry.meta,
            serde_json::json!({ "previous_captain": c.captain }),
            "the entry names the departing Captain by id, and nothing else"
        );
    }
}

#[tokio::test]
async fn an_immediate_deletion_hands_each_captained_team_to_a_successor() {
    in_scratch_database(|db| async move {
        migrated(&db).await;
        let captaincies = captain_of_three_teams(&db, false).await;

        delete(&db, captaincies.captain, DeletionKind::IllicitContent)
            .await
            .expect("the immediate deletion must succeed");

        assert_succession(&db, &captaincies).await;
        db.close().await.ok();
    })
    .await;
}

/// The account was scheduled while it captained these teams, past the
/// refusal of its request (written here directly): the purge is the safety
/// net and does the same.
#[tokio::test]
async fn the_purge_hands_each_captained_team_to_a_successor() {
    in_scratch_database(|db| async move {
        migrated(&db).await;
        let captaincies = captain_of_three_teams(&db, true).await;

        assert_eq!(
            purge(&db, &store()).await,
            PurgeOutcome {
                purged: 1,
                failed: 0
            }
        );

        assert_succession(&db, &captaincies).await;
        db.close().await.ok();
    })
    .await;
}

/// A deletion request is refused while the account is Captain of a team
/// with other members, naming it; a team it is alone in and its personal
/// team hold nothing back. Once the role is transferred, the same request
/// goes through.
#[tokio::test]
async fn a_deletion_request_from_the_captain_of_a_team_with_members_is_refused() {
    in_scratch_database(|db| async move {
        migrated(&db).await;
        let captain = create_user(&db, false).await;
        let shared = create_team(&db, captain, false).await;
        let member = create_user(&db, false).await;
        add_member(&db, shared, member, "member").await;
        create_team(&db, captain, false).await;
        create_team(&db, captain, true).await;
        let shared_slug = teams::Entity::find_by_id(shared)
            .one(&db)
            .await
            .expect("the lookup must succeed")
            .expect("the team must exist")
            .slug;

        match delete(&db, captain, DeletionKind::Standard).await {
            Err(AdminError::CaptainMustTransfer { teams }) => {
                assert_eq!(teams, vec![shared_slug], "the refusal names that team only")
            }
            other => panic!("the request must be refused, got {other:?}"),
        }
        assert!(
            !scheduled(&db, captain).await,
            "the account must not be scheduled"
        );

        team_service::transfer_captaincy(&db, shared, member, TransferBy::Captain(captain))
            .await
            .expect("the transfer must succeed");
        delete(&db, captain, DeletionKind::Standard)
            .await
            .expect("the request must go through once the role is transferred");
        assert!(
            scheduled(&db, captain).await,
            "the account must be scheduled"
        );

        db.close().await.ok();
    })
    .await;
}

async fn username_of(db: &DatabaseConnection, user: Uuid) -> String {
    users::Entity::find_by_id(user)
        .one(db)
        .await
        .expect("the lookup must succeed")
        .expect("the account must exist")
        .username
}

/// The entries a deletion writes keep the account's id and the kind of
/// deletion in their own data, since their target is set to NULL with the
/// account, and never its username: the purge's entry and an immediate
/// deletion's alike.
#[tokio::test]
async fn deletion_audit_entries_keep_the_account_id_and_kind() {
    in_scratch_database(|db| async move {
        migrated(&db).await;
        let purged = create_user(&db, true).await;
        let deleted = create_user(&db, false).await;
        let usernames = [
            username_of(&db, purged).await,
            username_of(&db, deleted).await,
        ];

        assert_eq!(
            purge(&db, &store()).await,
            PurgeOutcome {
                purged: 1,
                failed: 0
            }
        );
        delete(&db, deleted, DeletionKind::SecurityThreat)
            .await
            .expect("the immediate deletion must succeed");

        for (action, user, kind, username) in [
            ("user.purged", purged, "standard", &usernames[0]),
            (
                "user.delete_by_admin",
                deleted,
                "security_threat",
                &usernames[1],
            ),
        ] {
            let entry = instance_audit_logs::Entity::find()
                .filter(instance_audit_logs::Column::Action.eq(action))
                .one(&db)
                .await
                .expect("the audit lookup must succeed")
                .unwrap_or_else(|| panic!("the {action} entry must exist"));

            assert_eq!(
                entry.target_user_id, None,
                "{action}: the target goes with the account"
            );
            assert_eq!(
                entry.meta["user_id"],
                user.to_string(),
                "{action}: the account's id"
            );
            assert_eq!(entry.meta["kind"], kind, "{action}: the kind of deletion");
            assert!(
                !entry.meta.to_string().contains(username.as_str()),
                "{action}: no username in the entry, got {}",
                entry.meta
            );
        }

        db.close().await.ok();
    })
    .await;
}

/// Suspends an account, until `until` when given, for good otherwise.
async fn suspend(
    db: &DatabaseConnection,
    user: Uuid,
    since: DateTime<Utc>,
    until: Option<DateTime<Utc>>,
) {
    users::ActiveModel {
        id: Set(user),
        suspended_at: Set(Some(since.fixed_offset())),
        suspended_until: Set(until.map(|until| until.fixed_offset())),
        ..Default::default()
    }
    .update(db)
    .await
    .expect("the suspension must be written");
}

/// The successor is chosen among the active members first, with the same
/// rule, and among everyone only when nobody is active.
#[tokio::test]
async fn the_successor_is_chosen_among_active_members_first() {
    in_scratch_database(|db| async move {
        migrated(&db).await;
        let now = Utc::now();
        let captain = create_user(&db, false).await;

        // A newer active Admin passes an older suspended one.
        let past_suspended = create_team(&db, captain, false).await;
        let suspended_admin = create_user(&db, false).await;
        let active_admin = create_user(&db, false).await;
        suspend(&db, suspended_admin, now - Duration::days(1), None).await;
        add_member_at(
            &db,
            past_suspended,
            suspended_admin,
            "admin",
            now - Duration::days(20),
        )
        .await;
        add_member_at(
            &db,
            past_suspended,
            active_admin,
            "admin",
            now - Duration::days(10),
        )
        .await;

        // An active member passes an Admin scheduled for deletion.
        let past_leaving = create_team(&db, captain, false).await;
        let leaving_admin = create_user(&db, true).await;
        let active_member = create_user(&db, false).await;
        add_member_at(
            &db,
            past_leaving,
            leaving_admin,
            "admin",
            now - Duration::days(20),
        )
        .await;
        add_member_at(
            &db,
            past_leaving,
            active_member,
            "member",
            now - Duration::days(10),
        )
        .await;

        // Nobody active: the same rule over everyone, the Admin first.
        let nobody_active = create_team(&db, captain, false).await;
        let inactive_admin = create_user(&db, true).await;
        let inactive_member = create_user(&db, false).await;
        suspend(
            &db,
            inactive_member,
            now - Duration::days(1),
            Some(now + Duration::days(7)),
        )
        .await;
        add_member_at(
            &db,
            nobody_active,
            inactive_member,
            "member",
            now - Duration::days(20),
        )
        .await;
        add_member_at(
            &db,
            nobody_active,
            inactive_admin,
            "admin",
            now - Duration::days(5),
        )
        .await;

        // A suspension that has ended no longer counts.
        let suspension_over = create_team(&db, captain, false).await;
        let returned_admin = create_user(&db, false).await;
        let newer_admin = create_user(&db, false).await;
        suspend(
            &db,
            returned_admin,
            now - Duration::days(9),
            Some(now - Duration::days(2)),
        )
        .await;
        add_member_at(
            &db,
            suspension_over,
            returned_admin,
            "admin",
            now - Duration::days(20),
        )
        .await;
        add_member_at(
            &db,
            suspension_over,
            newer_admin,
            "admin",
            now - Duration::days(10),
        )
        .await;

        delete(&db, captain, DeletionKind::IllicitContent)
            .await
            .expect("the immediate deletion must succeed");

        for (team, successor, case) in [
            (
                past_suspended,
                active_admin,
                "an active Admin before a suspended one",
            ),
            (
                past_leaving,
                active_member,
                "an active member before an Admin scheduled for deletion",
            ),
            (
                nobody_active,
                inactive_admin,
                "with nobody active, the Admin first among everyone",
            ),
            (
                suspension_over,
                returned_admin,
                "a suspension that has ended no longer counts",
            ),
        ] {
            assert_eq!(
                role_of(&db, team, successor).await.as_deref(),
                Some("captain"),
                "{case}"
            );
        }

        db.close().await.ok();
    })
    .await;
}

//! The migration that constrains the enum columns of `workspaces`: what it
//! normalises before adding its checks, the checks themselves, `down()`, and
//! the refusal of a value it has no honest answer for.
//!
//! Each test works in a throwaway database of its own, migrated up to the
//! migration before this one, because rows that break the checks can only be
//! written to a schema that does not have them yet. CI provisions the
//! instance and sets `DATABASE_URL`; locally the dev compose exposes one.

use domain::entities::{users, workspaces};
use migration::MigratorTrait;
use sea_orm::{
    ActiveModelTrait, ConnectionTrait, Database, DatabaseConnection, DbBackend, EntityTrait, Set,
    Statement,
};
use uuid::Uuid;

const MIGRATION: &str = "m20261008_090000_workspaces_enum_checks";

/// The checks the migration adds, in name order.
const CHECKS: [&str; 5] = [
    "ck_workspaces_color",
    "ck_workspaces_platform",
    "ck_workspaces_platform_required",
    "ck_workspaces_type",
    "ck_workspaces_visibility",
];

fn base_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://pwnforge:changeme@127.0.0.1:5432/pwnforge".to_owned())
}

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
    let name = format!("pwnforge_enum_checks_test_{}", Uuid::now_v7().simple());
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

/// Applies every migration before the one under test, found by name.
async fn migrate_to_just_before(db: &DatabaseConnection) {
    let position = migration::Migrator::migrations()
        .iter()
        .position(|m| m.name() == MIGRATION)
        .expect("the migration under test must be registered in the Migrator");

    migration::Migrator::up(db, Some(position as u32))
        .await
        .expect("the earlier migrations must apply");
}

async fn create_owner(db: &DatabaseConnection) -> Uuid {
    users::ActiveModel {
        id: Set(Uuid::now_v7()),
        username: Set(format!("enum-{}", Uuid::now_v7().simple())),
        password_hash: Set("not-a-real-hash".to_owned()),
        locale: Set("en".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture user must insert")
    .id
}

async fn insert_workspace(
    db: &DatabaseConnection,
    owner: Uuid,
    color: &str,
    r#type: &str,
    platform: Option<&str>,
) -> Result<workspaces::Model, sea_orm::DbErr> {
    workspaces::ActiveModel {
        id: Set(Uuid::now_v7()),
        owner_id: Set(Some(owner)),
        slug: Set(format!("enum-{}", Uuid::now_v7().simple())),
        name: Set("Enum checks".to_owned()),
        color: Set(color.to_owned()),
        r#type: Set(r#type.to_owned()),
        platform: Set(platform.map(str::to_owned)),
        mode: Set("solo".to_owned()),
        default_writeup_visibility: Set("draft".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
}

async fn stored(db: &DatabaseConnection, id: Uuid) -> workspaces::Model {
    workspaces::Entity::find_by_id(id)
        .one(db)
        .await
        .expect("the lookup must succeed")
        .expect("the workspace must exist")
}

/// Which of the five checks exist on `workspaces`, in name order.
async fn present_checks(db: &DatabaseConnection) -> Vec<String> {
    db.query_all_raw(Statement::from_string(
        DbBackend::Postgres,
        "SELECT conname FROM pg_constraint \
         WHERE conrelid = 'workspaces'::regclass AND contype = 'c' \
         ORDER BY conname",
    ))
    .await
    .expect("the catalog query must succeed")
    .into_iter()
    .map(|row| {
        row.try_get::<String>("", "conname")
            .expect("conname is text")
    })
    .filter(|name| CHECKS.contains(&name.as_str()))
    .collect()
}

/// The values with a known honest answer are rewritten, a valid row is left
/// alone, the five checks exist and refuse, and `down()` drops them again
/// without putting the old values back, which were never recorded.
#[tokio::test]
async fn the_migration_normalises_constrains_and_rolls_back() {
    in_scratch_database(|db| async move {
        migrate_to_just_before(&db).await;
        let owner = create_owner(&db).await;

        let off_palette = insert_workspace(&db, owner, "red", "event", None)
            .await
            .expect("an off-palette colour inserts before the checks");
        let legacy_ctf = insert_workspace(&db, owner, "blue", "ctf", None)
            .await
            .expect("the legacy type inserts before the checks");
        let no_platform = insert_workspace(&db, owner, "blue", "platform", None)
            .await
            .expect("a platform workspace without a platform inserts before the checks");
        let valid = insert_workspace(&db, owner, "teal", "platform", Some("htb"))
            .await
            .expect("a valid workspace inserts");
        assert!(present_checks(&db).await.is_empty(), "no check yet");

        // Exactly one step: the migration under test.
        migration::Migrator::up(&db, Some(1))
            .await
            .expect("the migration must apply");

        assert_eq!(stored(&db, off_palette.id).await.color, "ember");
        assert_eq!(stored(&db, legacy_ctf.id).await.r#type, "event");
        assert_eq!(
            stored(&db, no_platform.id).await.platform.as_deref(),
            Some("other")
        );
        let kept = stored(&db, valid.id).await;
        assert_eq!(
            (
                kept.color.as_str(),
                kept.r#type.as_str(),
                kept.platform.as_deref()
            ),
            ("teal", "platform", Some("htb")),
            "a valid row is left alone"
        );

        assert_eq!(present_checks(&db).await, CHECKS);
        assert!(
            insert_workspace(&db, owner, "red", "event", None)
                .await
                .is_err(),
            "the checks refuse an off-palette colour"
        );

        migration::Migrator::down(&db, Some(1))
            .await
            .expect("down() must roll the migration back");

        assert!(
            present_checks(&db).await.is_empty(),
            "down() drops the five checks"
        );
        assert_eq!(
            stored(&db, off_palette.id).await.color,
            "ember",
            "down() does not restore the values it never recorded"
        );
        insert_workspace(&db, owner, "red", "event", None)
            .await
            .expect("without the checks an off-palette colour inserts again");

        db.close().await.ok();
    })
    .await;
}

/// A value with no honest answer fails the migration on purpose, and the
/// migration's own transaction takes the normalisations down with it.
#[tokio::test]
async fn an_unknown_value_fails_the_migration_and_undoes_it() {
    in_scratch_database(|db| async move {
        migrate_to_just_before(&db).await;
        let owner = create_owner(&db).await;

        let off_palette = insert_workspace(&db, owner, "red", "event", None)
            .await
            .expect("an off-palette colour inserts before the checks");
        insert_workspace(&db, owner, "blue", "weird", None)
            .await
            .expect("an unknown type inserts before the checks");

        assert!(
            migration::Migrator::up(&db, Some(1)).await.is_err(),
            "an unknown type must fail the migration"
        );

        assert_eq!(
            stored(&db, off_palette.id).await.color,
            "red",
            "the colour normalisation is rolled back with the failure"
        );
        assert!(present_checks(&db).await.is_empty(), "no check is left");

        db.close().await.ok();
    })
    .await;
}

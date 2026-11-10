//! Integration tests for `workspace_service::update_workspace` on the two
//! fields the settings page added: `event_url` and an editable `platform`.
//!
//! These need a real Postgres, for the same reason as
//! `workspace_membership_writes.rs`: what is under test includes a check
//! constraint (`ck_workspaces_event_url`) that `MockDatabase` never executes.
//! The inputs are built from JSON on purpose, so the three states of a PATCH
//! field (absent, null, value) go through the real `deserialize_some`.
//!
//! CI provisions the instance and sets `DATABASE_URL`; locally the dev compose
//! exposes one.

use std::sync::atomic::{AtomicU32, Ordering};

use domain::dto::workspace::UpdateWorkspaceInput;
use domain::entities::{users, workspaces};
use migration::MigratorTrait;
use sea_orm::{ActiveModelTrait, Database, DatabaseConnection, EntityTrait, Set};
use serde_json::{Value, json};
use services::workspace_service::{self, WorkspaceError};
use uuid::Uuid;

async fn connect() -> DatabaseConnection {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://pwnforge:changeme@127.0.0.1:5432/pwnforge".to_owned());

    let db = Database::connect(url)
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

    format!("{}-{nth}", Uuid::now_v7().simple())
}

async fn create_user(db: &DatabaseConnection) -> Uuid {
    users::ActiveModel {
        id: Set(Uuid::now_v7()),
        username: Set(format!("ws-update-test-{}", unique_suffix())),
        password_hash: Set("not-a-real-hash".to_owned()),
        locale: Set("en".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture user must insert")
    .id
}

/// A solo workspace of the given type. A `platform` one gets a platform, as
/// `create_workspace` requires; an `event` one starts without.
async fn create_workspace(db: &DatabaseConnection, r#type: &str) -> workspaces::Model {
    let suffix = unique_suffix();
    let owner_id = create_user(db).await;

    workspaces::ActiveModel {
        id: Set(Uuid::now_v7()),
        owner_id: Set(Some(owner_id)),
        slug: Set(format!("ws-update-test-{suffix}")),
        name: Set(format!("Workspace Update Test {suffix}")),
        color: Set("ember".to_owned()),
        r#type: Set(r#type.to_owned()),
        platform: Set((r#type == "platform").then(|| "htb".to_owned())),
        mode: Set("solo".to_owned()),
        default_writeup_visibility: Set("draft".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture workspace must insert")
}

fn patch(body: Value) -> UpdateWorkspaceInput {
    serde_json::from_value(body).expect("the test body must be a valid PATCH")
}

async fn stored(db: &DatabaseConnection, id: Uuid) -> workspaces::Model {
    workspaces::Entity::find_by_id(id)
        .one(db)
        .await
        .expect("the lookup must succeed")
        .expect("the workspace must still exist")
}

// ---- event_url ------------------------------------------------------------

/// Positive: the URL is stored in its parsed form, scheme and host lowercased.
#[tokio::test]
async fn an_event_url_is_stored_normalised() {
    let db = connect().await;
    let workspace = create_workspace(&db, "event").await;

    let updated = workspace_service::update_workspace(
        &db,
        workspace.id,
        patch(json!({ "event_url": "  HTTPS://Hexagon.CTF/scoreboard  " })),
    )
    .await
    .expect("a valid URL must be accepted");

    assert_eq!(
        updated.event_url.as_deref(),
        Some("https://hexagon.ctf/scoreboard")
    );
    assert_eq!(
        stored(&db, workspace.id).await.event_url,
        updated.event_url,
        "what is returned is what is stored"
    );
}

/// Anything that is not an absolute http(s) URL is refused, and nothing is
/// written: the frontend renders this as a link.
#[tokio::test]
async fn an_unsafe_or_relative_event_url_is_refused() {
    let db = connect().await;
    let workspace = create_workspace(&db, "event").await;

    for raw in [
        "javascript:alert(1)",
        "data:text/html,<script>alert(1)</script>",
        "ftp://ctf.example",
        "https://user:secret@ctf.example",
        "/scoreboard",
        "ctf.example",
    ] {
        let refused = workspace_service::update_workspace(
            &db,
            workspace.id,
            patch(json!({ "event_url": raw })),
        )
        .await;
        assert!(
            matches!(refused, Err(WorkspaceError::InvalidEventUrl)),
            "{raw}: expected InvalidEventUrl, got {refused:?}"
        );
    }
    assert_eq!(stored(&db, workspace.id).await.event_url, None);
}

/// Null clears it, and so does a blank string, which is what an emptied
/// field sends.
#[tokio::test]
async fn null_or_blank_clears_the_event_url() {
    let db = connect().await;
    let workspace = create_workspace(&db, "event").await;

    for clearing in [json!(null), json!("   ")] {
        workspace_service::update_workspace(
            &db,
            workspace.id,
            patch(json!({ "event_url": "https://ctf.example" })),
        )
        .await
        .expect("setting must succeed");

        let updated = workspace_service::update_workspace(
            &db,
            workspace.id,
            patch(json!({ "event_url": clearing.clone() })),
        )
        .await
        .expect("clearing must succeed");
        assert_eq!(updated.event_url, None, "{clearing} must clear the URL");
    }
}

/// Absent leaves it alone: a PATCH of the name must not wipe the URL.
#[tokio::test]
async fn a_patch_without_event_url_keeps_it() {
    let db = connect().await;
    let workspace = create_workspace(&db, "event").await;
    workspace_service::update_workspace(
        &db,
        workspace.id,
        patch(json!({ "event_url": "https://ctf.example/" })),
    )
    .await
    .expect("setting must succeed");

    let updated =
        workspace_service::update_workspace(&db, workspace.id, patch(json!({ "name": "Renamed" })))
            .await
            .expect("renaming must succeed");

    assert_eq!(updated.event_url.as_deref(), Some("https://ctf.example/"));
}

/// The constraint refuses what the service would: a writer that bypasses it
/// cannot store a non-http(s) URL either.
#[tokio::test]
async fn the_check_constraint_refuses_a_non_http_url() {
    let db = connect().await;
    let workspace = create_workspace(&db, "event").await;

    let mut active: workspaces::ActiveModel = workspace.into();
    active.event_url = Set(Some("javascript:alert(1)".to_owned()));

    assert!(
        active.update(&db).await.is_err(),
        "ck_workspaces_event_url must refuse a direct write"
    );
}

// ---- platform -------------------------------------------------------------

/// Positive: a platform workspace can change its platform.
#[tokio::test]
async fn a_platform_workspace_can_change_its_platform() {
    let db = connect().await;
    let workspace = create_workspace(&db, "platform").await;

    let updated = workspace_service::update_workspace(
        &db,
        workspace.id,
        patch(json!({ "platform": "root_me" })),
    )
    .await
    .expect("changing the platform must succeed");

    // Stored as Platform::as_str() writes it, which is not the JSON spelling.
    assert_eq!(updated.platform.as_deref(), Some("rootme"));
}

/// The rule from creation still holds: a platform workspace cannot drop its
/// platform, and the refused PATCH writes nothing.
#[tokio::test]
async fn a_platform_workspace_cannot_clear_its_platform() {
    let db = connect().await;
    let workspace = create_workspace(&db, "platform").await;

    let refused =
        workspace_service::update_workspace(&db, workspace.id, patch(json!({ "platform": null })))
            .await;

    assert!(
        matches!(refused, Err(WorkspaceError::PlatformRequired)),
        "expected PlatformRequired, got {refused:?}"
    );
    assert_eq!(
        stored(&db, workspace.id).await.platform.as_deref(),
        Some("htb")
    );
}

/// On an event the platform is optional: it can be set, then cleared.
#[tokio::test]
async fn an_event_can_set_and_clear_its_platform() {
    let db = connect().await;
    let workspace = create_workspace(&db, "event").await;

    let set = workspace_service::update_workspace(
        &db,
        workspace.id,
        patch(json!({ "platform": "ctftime" })),
    )
    .await
    .expect("setting the platform must succeed");
    assert_eq!(set.platform.as_deref(), Some("ctftime"));

    let cleared =
        workspace_service::update_workspace(&db, workspace.id, patch(json!({ "platform": null })))
            .await
            .expect("clearing the platform must succeed");
    assert_eq!(cleared.platform, None);
}

/// Absent leaves it alone, even on a platform workspace.
#[tokio::test]
async fn a_patch_without_platform_keeps_it() {
    let db = connect().await;
    let workspace = create_workspace(&db, "platform").await;

    let updated =
        workspace_service::update_workspace(&db, workspace.id, patch(json!({ "name": "Renamed" })))
            .await
            .expect("renaming must succeed");

    assert_eq!(updated.platform.as_deref(), Some("htb"));
}

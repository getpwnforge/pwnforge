//! Verifying an address creates no team: an account starts with none, and a
//! solo workspace, which belongs to its owner through `owner_id`, needs none.
//!
//! Real Postgres: verification is a transaction over several tables. CI
//! provisions the instance and sets `DATABASE_URL`; locally the dev compose
//! exposes one.

use domain::dto::workspace::{CreateWorkspaceInput, Membership, WorkspaceMode, WorkspaceType};
use domain::entities::{teams, user_emails, users};
use domain::types::{AccentColor, TokenKind};
use migration::MigratorTrait;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Database, DatabaseConnection, EntityTrait, QueryFilter, Set,
};
use services::{auth_service, auth_token_service, team_service, workspace_service};
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

#[tokio::test]
async fn verifying_an_address_creates_no_team_and_the_account_can_work_solo() {
    let db = connect().await;
    let user = users::ActiveModel {
        id: Set(Uuid::now_v7()),
        username: Set(format!("ev-{}", Uuid::now_v7().simple())),
        password_hash: Set("not-a-real-hash".to_owned()),
        locale: Set("en".to_owned()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("the fixture user must insert");
    let address = user_emails::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(user.id),
        email: Set(format!("ev-{}@example.test", Uuid::now_v7().simple())),
        is_primary: Set(true),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("the fixture address must insert");

    let secret = auth_token_service::issue(&db, user.id, address.id, TokenKind::EmailVerification)
        .await
        .expect("a verification token must be issued");
    auth_service::verify_email(&db, &secret)
        .await
        .expect("the address must verify");

    let verified = user_emails::Entity::find_by_id(address.id)
        .one(&db)
        .await
        .expect("the lookup must succeed")
        .expect("the address must exist");
    assert!(
        verified.verified_at.is_some(),
        "control: the address is verified"
    );

    let created = teams::Entity::find()
        .filter(teams::Column::CreatedBy.eq(user.id))
        .all(&db)
        .await
        .expect("the lookup must succeed");
    assert!(created.is_empty(), "verification must create no team");
    assert!(
        team_service::list_teams_for_user(&db, user.id)
            .await
            .expect("the listing must succeed")
            .is_empty(),
        "and the account belongs to none"
    );

    let (workspace, membership) = workspace_service::create_workspace(
        &db,
        user.id,
        CreateWorkspaceInput {
            name: "Solo".to_owned(),
            description: None,
            color: AccentColor::default(),
            r#type: WorkspaceType::Event,
            platform: None,
            mode: WorkspaceMode::Solo,
            team_id: None,
            starts_at: None,
            ends_at: None,
            lock_after_end: None,
            default_writeup_visibility: None,
        },
    )
    .await
    .expect("a new account can create a solo workspace");

    assert_eq!(workspace.mode, WorkspaceMode::Solo.as_str());
    assert_eq!(workspace.owner_id, Some(user.id));
    assert_eq!(workspace.team_id, None);
    assert_eq!(membership, Membership::Owner);
}

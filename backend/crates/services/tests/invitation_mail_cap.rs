//! At most three invitation mails a day to one address, counted across every
//! team and every workspace. Past that, the invitation is still created and
//! the answer is the same; only the mail is not sent. The cap fails closed:
//! when it cannot be read, no mail goes out, and the invitation is still
//! created.
//!
//! A real Postgres and a real Redis, made to fail on one key as in
//! `contact_rate_limit.rs`: INCR refuses a key that holds something other
//! than an integer. CI provides both; locally `make test-backend` points at
//! the dev compose.

use domain::dto::team::{CreateInvitationRequest, TeamRole};
use domain::dto::workspace::{CreateWorkspaceInvitationRequest, GuestRole};
use domain::entities::{team_members, teams, users, workspace_invitations, workspaces};
use migration::MigratorTrait;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use sea_orm::{ActiveModelTrait, Database, DatabaseConnection, EntityTrait, Set};
use services::invitation_service::MailOutcome;
use services::{team_service, token_service, workspace_invitation_service};
use uuid::Uuid;

#[path = "common/config.rs"]
mod config;

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

async fn connect_redis() -> ConnectionManager {
    let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_owned());
    let client = redis::Client::open(url).expect("REDIS_URL must be a valid Redis URL");
    ConnectionManager::new(client)
        .await
        .expect("a Redis instance must be reachable")
}

fn unique(prefix: &str) -> String {
    format!("{prefix}-{}", Uuid::now_v7().simple())
}

/// The key the cap counts an address under, written out here rather than
/// imported: a drift in its name would make the poison below miss.
fn cap_key(address: &str) -> String {
    format!(
        "ratelimit:invite:mail:address:{}",
        token_service::hash_opaque_token(&address.trim().to_lowercase())
    )
}

async fn create_user(db: &DatabaseConnection) -> Uuid {
    users::ActiveModel {
        id: Set(Uuid::now_v7()),
        username: Set(unique("cap")),
        password_hash: Set("not-a-real-hash".to_owned()),
        locale: Set("en".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture user must insert")
    .id
}

/// A team with `captain` as Captain, and one workspace of it.
async fn team_and_workspace(db: &DatabaseConnection, captain: Uuid) -> (Uuid, Uuid) {
    let team = teams::ActiveModel {
        id: Set(Uuid::now_v7()),
        slug: Set(unique("cap-team")),
        name: Set("Cap".to_owned()),
        color: Set("ember".to_owned()),
        created_by: Set(Some(captain)),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture team must insert");
    team_members::ActiveModel {
        team_id: Set(team.id),
        user_id: Set(captain),
        role: Set(TeamRole::Captain.as_str().to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture captain must insert");
    let workspace = workspaces::ActiveModel {
        id: Set(Uuid::now_v7()),
        team_id: Set(Some(team.id)),
        slug: Set(unique("cap-ws")),
        name: Set("Cap".to_owned()),
        color: Set("ember".to_owned()),
        r#type: Set("event".to_owned()),
        mode: Set("team".to_owned()),
        default_writeup_visibility: Set("team".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture workspace must insert");
    (team.id, workspace.id)
}

async fn invite_to_team(
    db: &DatabaseConnection,
    redis: &mut ConnectionManager,
    team_id: Uuid,
    captain: Uuid,
    address: &str,
) -> Option<MailOutcome> {
    team_service::create_invitation(
        db,
        redis,
        &config::config(),
        team_id,
        captain,
        CreateInvitationRequest {
            default_role: TeamRole::Member,
            expires_at: None,
            max_uses: None,
            username: None,
            invited_email: Some(address.to_owned()),
        },
    )
    .await
    .expect("the team invitation must be created")
    .mail
}

async fn invite_to_workspace(
    db: &DatabaseConnection,
    redis: &mut ConnectionManager,
    workspace_id: Uuid,
    captain: Uuid,
    address: &str,
) -> (serde_json::Value, Option<MailOutcome>) {
    let created = workspace_invitation_service::create_invitation(
        db,
        redis,
        &config::config(),
        workspace_id,
        captain,
        CreateWorkspaceInvitationRequest {
            role: GuestRole::Member,
            username: None,
            invited_email: Some(address.to_owned()),
        },
    )
    .await
    .expect("the workspace invitation must be created");

    // What the author is answered, minus what differs between any two
    // invitations whatever the mail did.
    let mut answer = serde_json::to_value(&created.invitation).expect("the answer serializes");
    for field in [
        "id",
        "workspace_id",
        "workspace_name",
        "created_at",
        "expires_at",
        "invited_by",
        "invited_by_username",
    ] {
        answer.as_object_mut().expect("an object").remove(field);
    }
    (answer, created.mail)
}

/// Two team invitations and a workspace one reach the address; the fourth,
/// to another workspace, is created and answered like the third, without a
/// mail.
#[tokio::test]
async fn the_fourth_invitation_mail_to_an_address_in_a_day_is_not_sent() {
    let db = connect().await;
    let mut redis = connect_redis().await;
    let address = format!("{}@example.test", unique("capped"));

    let mut outcomes = Vec::new();
    for _ in 0..2 {
        let captain = create_user(&db).await;
        let (team, _) = team_and_workspace(&db, captain).await;
        outcomes.push(invite_to_team(&db, &mut redis, team, captain, &address).await);
    }
    let captain = create_user(&db).await;
    let (_, first) = team_and_workspace(&db, captain).await;
    let (third_answer, third) =
        invite_to_workspace(&db, &mut redis, first, captain, &address).await;
    outcomes.push(third);
    let other = create_user(&db).await;
    let (_, second) = team_and_workspace(&db, other).await;
    let (fourth_answer, fourth) =
        invite_to_workspace(&db, &mut redis, second, other, &address).await;
    outcomes.push(fourth);

    let _: () = redis.del(cap_key(&address)).await.expect("DEL must answer");

    assert_eq!(
        outcomes,
        vec![
            Some(MailOutcome::Sent),
            Some(MailOutcome::Sent),
            Some(MailOutcome::Sent),
            Some(MailOutcome::AddressCapReached),
        ]
    );
    assert_eq!(
        third_answer, fourth_answer,
        "the author is answered the same whether the mail went out or not"
    );
}

/// The cap cannot be read: no mail, and the invitation is still created.
#[tokio::test]
async fn an_unreadable_cap_sends_no_mail_and_still_creates_the_invitation() {
    let db = connect().await;
    let mut redis = connect_redis().await;
    let address = format!("{}@example.test", unique("poisoned"));
    let key = cap_key(&address);
    let _: () = redis
        .set(&key, "not-a-counter")
        .await
        .expect("SET must answer");

    let captain = create_user(&db).await;
    let (_, workspace) = team_and_workspace(&db, captain).await;
    let created = workspace_invitation_service::create_invitation(
        &db,
        &mut redis,
        &config::config(),
        workspace,
        captain,
        CreateWorkspaceInvitationRequest {
            role: GuestRole::Viewer,
            username: None,
            invited_email: Some(address.clone()),
        },
    )
    .await;

    let _: () = redis.del(&key).await.expect("DEL must answer");

    let created = created.expect("the invitation is created whatever the cap says");
    assert_eq!(created.mail, Some(MailOutcome::LimiterUnavailable));
    assert!(
        workspace_invitations::Entity::find_by_id(created.invitation.id)
            .one(&db)
            .await
            .expect("the lookup must succeed")
            .is_some()
    );
}

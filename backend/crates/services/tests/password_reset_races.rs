//! The reset link against what can happen around it: its expiry, a forced
//! reset by an administrator, and the time the request for it takes.
//!
//! A real Postgres, and for the request a real Redis: what is under test is
//! the order of locks and statements between overlapping transactions. The
//! tests that need one transaction to wait on another find the wait in
//! `pg_stat_activity` rather than sleeping a guessed amount. CI provides
//! both; locally `make test-backend` points at the dev compose.

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use chrono::Utc;
use domain::dto::auth::SessionContext;
use domain::entities::{auth_tokens, user_emails, users};
use domain::types::TokenKind;
use migration::MigratorTrait;
use redis::aio::ConnectionManager;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, Database, DatabaseConnection, DbBackend,
    EntityTrait, QueryFilter, Set, Statement, TransactionTrait,
};
use services::audit_service::AuditContext;
use services::auth_service::{self, AuthError};
use services::auth_token_service::{self, AuthTokenError};
use services::background::BackgroundTasks;
use services::config::{EmailBackend, SmtpTls};
use services::{admin_service, token_service};
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

/// An account with a verified primary address, and that address.
async fn account(db: &DatabaseConnection) -> (Uuid, String) {
    let id = Uuid::now_v7();
    let address = format!("reset-{}@example.test", id.simple());
    users::ActiveModel {
        id: Set(id),
        username: Set(format!("r{}", &id.simple().to_string()[1..])),
        password_hash: Set("original-hash".to_owned()),
        locale: Set("en".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture user must insert");
    user_emails::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(id),
        email: Set(address.clone()),
        is_primary: Set(true),
        verified_at: Set(Some(Utc::now().into())),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture address must insert");
    (id, address)
}

/// A reset link for the account, expiring at `expires_in` from now, and its
/// secret.
async fn reset_link(
    db: &DatabaseConnection,
    user_id: Uuid,
    expires_in: chrono::Duration,
) -> (Uuid, String) {
    let email = user_emails::Entity::find()
        .filter(user_emails::Column::UserId.eq(user_id))
        .one(db)
        .await
        .expect("the lookup must succeed")
        .expect("the address exists");
    let token = token_service::generate_opaque_token();
    let row = auth_tokens::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(user_id),
        email_id: Set(Some(email.id)),
        kind: Set(TokenKind::PasswordReset.as_str().to_owned()),
        token_hash: Set(token.hash),
        expires_at: Set((Utc::now() + expires_in).into()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture link must insert");
    (row.id, token.secret)
}

async fn password_hash(db: &DatabaseConnection, user_id: Uuid) -> String {
    users::Entity::find_by_id(user_id)
        .one(db)
        .await
        .expect("the lookup must succeed")
        .expect("the user exists")
        .password_hash
}

async fn link_used(db: &DatabaseConnection, id: Uuid) -> bool {
    auth_tokens::Entity::find_by_id(id)
        .one(db)
        .await
        .expect("the lookup must succeed")
        .expect("the link exists")
        .used_at
        .is_some()
}

/// Waits until a statement matching `pattern` waits on a lock.
async fn wait_for_lock_wait(db: &DatabaseConnection, pattern: &str) {
    for _ in 0..200 {
        let waiting = db
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT count(*) AS n FROM pg_stat_activity \
                 WHERE wait_event_type = 'Lock' AND query ILIKE $1",
                [pattern.into()],
            ))
            .await
            .expect("the activity query must succeed")
            .and_then(|row| row.try_get::<i64>("", "n").ok())
            .unwrap_or(0);
        if waiting > 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("no statement matching {pattern} ever waited on a lock");
}

fn ctx() -> SessionContext {
    SessionContext {
        user_agent: Some("reset-race-test".to_owned()),
        ip_address: None,
    }
}

/// An expired link is refused and stays unspent: the claim itself requires
/// the link unexpired, not only a check made before it.
#[tokio::test]
async fn an_expired_link_is_refused_by_the_claim_itself() {
    let db = connect().await;
    let (user, _) = account(&db).await;
    let (id, secret) = reset_link(&db, user, chrono::Duration::minutes(-1)).await;

    let outcome = auth_token_service::consume(&db, &secret, TokenKind::PasswordReset).await;

    assert!(
        matches!(outcome, Err(AuthTokenError::Expired)),
        "got {outcome:?}"
    );
    assert!(!link_used(&db, id).await, "an expired link is not spent");
}

/// A forced reset spends the pending links in its own transaction: while a
/// pending link cannot be written, the forced reset does not commit, rather
/// than committing and leaving the link to a later step that may never run.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_forced_reset_spends_pending_links_before_it_commits() {
    let db = connect().await;
    let (user, _) = account(&db).await;
    let (link, _) = reset_link(&db, user, chrono::Duration::hours(1)).await;

    let holder = db.begin().await.expect("the transaction opens");
    holder
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT id FROM auth_tokens WHERE id = $1 FOR UPDATE",
            [link.into()],
        ))
        .await
        .expect("the link locks");

    let forced = {
        let db = db.clone();
        tokio::spawn(async move {
            admin_service::force_password_reset(
                &db,
                &config::config(),
                &AuditContext {
                    actor_id: None,
                    ip_address: None,
                    user_agent: None,
                },
                user,
                None,
            )
            .await
        })
    };
    wait_for_lock_wait(&db, "%UPDATE \"auth_tokens\"%").await;

    assert_eq!(
        password_hash(&db, user).await,
        "original-hash",
        "the forced reset waits on the link before committing anything"
    );
    holder.commit().await.expect("the holder commits");

    forced
        .await
        .expect("the forced reset must not panic")
        .expect("the forced reset must succeed");
    assert_ne!(password_hash(&db, user).await, "original-hash");
    assert!(link_used(&db, link).await, "the pending link is spent");
}

/// A reset by link spends the link inside the transaction that writes the
/// password, after taking the account: a forced reset that holds the account
/// meanwhile and spends the link wins, instead of being overwritten by a
/// request that spent the link before it and wrote after it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_reset_in_flight_cannot_overwrite_a_forced_reset() {
    let db = connect().await;
    let (user, _) = account(&db).await;
    let (_, secret) = reset_link(&db, user, chrono::Duration::hours(1)).await;

    let forced = db.begin().await.expect("the transaction opens");
    forced
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT id FROM users WHERE id = $1 FOR NO KEY UPDATE",
            [user.into()],
        ))
        .await
        .expect("the account locks");

    let reset = {
        let db = db.clone();
        let secret = secret.clone();
        tokio::spawn(async move {
            auth_service::password_reset(
                &db,
                &config::config(),
                &secret,
                "Another-password-42!".to_owned(),
                ctx(),
            )
            .await
        })
    };
    wait_for_lock_wait(&db, "%FROM \"users\"%FOR NO KEY UPDATE%").await;

    // What the forced reset does, under the account's lock.
    for sql in [
        "UPDATE users SET password_hash = 'forced-hash' WHERE id = $1",
        "UPDATE auth_tokens SET used_at = now() \
         WHERE user_id = $1 AND kind = 'password_reset' AND used_at IS NULL",
    ] {
        forced
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                sql,
                [user.into()],
            ))
            .await
            .expect("the forced reset writes");
    }
    forced.commit().await.expect("the forced reset commits");

    let outcome = reset.await.expect("the reset must not panic");
    assert!(
        matches!(outcome, Err(AuthError::AuthToken(AuthTokenError::Consumed))),
        "the reset in flight must find its link spent, got {outcome:?}"
    );
    assert_eq!(password_hash(&db, user).await, "forced-hash");
}

/// The reset link is issued and mailed in a task of its own: the request
/// answers at once even when the mail server never does, and the link is
/// still issued.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_reset_request_answers_before_its_mail_is_sent() {
    let db = connect().await;
    let mut redis = connect_redis().await;
    let (user, address) = account(&db).await;

    // A mail server that accepts the connection and never greets.
    let silent = std::net::TcpListener::bind("127.0.0.1:0").expect("a port must be free");
    let mut config = config::config();
    config.email_backend = EmailBackend::Smtp;
    config.email_from = "noreply@example.test".to_owned();
    config.smtp_host = "127.0.0.1".to_owned();
    config.smtp_port = silent.local_addr().expect("an address").port();
    config.smtp_tls = SmtpTls::None;

    let answered = tokio::time::timeout(
        Duration::from_secs(3),
        auth_service::password_forgot(
            &db,
            &mut redis,
            &config,
            &BackgroundTasks::new(),
            &address,
            "",
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        ),
    )
    .await
    .expect("the request must answer without waiting for the mail");
    answered.expect("the request must succeed");

    let mut issued = false;
    for _ in 0..60 {
        issued = auth_tokens::Entity::find()
            .filter(auth_tokens::Column::UserId.eq(user))
            .filter(auth_tokens::Column::Kind.eq(TokenKind::PasswordReset.as_str()))
            .one(&db)
            .await
            .expect("the lookup must succeed")
            .is_some();
        if issued {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    drop(silent);
    assert!(issued, "the link is issued in the background");
}

//! An acceptance sent on `POST /legal/accept`: a document whose latest
//! acceptance is already the submitted version gets no new row, whether the
//! request comes again later or twice at once.
//!
//! A real Postgres: two requests at once are ordered by a row lock, and a
//! mock would only assert what we believe about it. Every test works on an
//! account of its own. CI provisions the instance and sets `DATABASE_URL`;
//! locally the dev compose exposes one.

use domain::dto::auth::SessionContext;
use domain::dto::legal::LegalAcceptanceInput;
use domain::entities::{legal_acceptances, users};
use domain::legal::{
    AcceptanceMethod, CURRENT_PRIVACY_VERSION, CURRENT_TERMS_VERSION, LegalDocument,
};
use migration::MigratorTrait;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, Database, DatabaseConnection, DbBackend,
    EntityTrait, QueryFilter, Set, Statement, TransactionTrait,
};
use services::legal_service;
use uuid::Uuid;

#[path = "common/locks.rs"]
mod locks;

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

async fn create_user(db: &DatabaseConnection) -> Uuid {
    let id = Uuid::now_v7();
    users::ActiveModel {
        id: Set(id),
        username: Set(format!("l{}", &id.simple().to_string()[1..])),
        password_hash: Set("not-a-real-hash".to_owned()),
        locale: Set("en".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture user must insert");
    id
}

fn ctx() -> SessionContext {
    SessionContext {
        user_agent: Some("legal-acceptance-test".to_owned()),
        ip_address: None,
    }
}

fn current() -> LegalAcceptanceInput {
    LegalAcceptanceInput {
        terms_version: CURRENT_TERMS_VERSION.to_owned(),
        privacy_version: CURRENT_PRIVACY_VERSION.to_owned(),
    }
}

/// The versions on record for one document of the account, oldest first.
async fn recorded(db: &DatabaseConnection, user: Uuid, document: LegalDocument) -> Vec<String> {
    let mut rows = legal_acceptances::Entity::find()
        .filter(legal_acceptances::Column::UserId.eq(user))
        .filter(legal_acceptances::Column::Document.eq(document.as_str()))
        .all(db)
        .await
        .expect("the lookup must succeed");
    rows.sort_by_key(|row| row.accepted_at);
    rows.into_iter().map(|row| row.version).collect()
}

/// The current versions accepted a second time: nothing more is recorded.
#[tokio::test]
async fn accepting_the_same_versions_again_records_nothing() {
    let db = connect().await;
    let user = create_user(&db).await;

    for _ in 0..2 {
        legal_service::accept(&db, user, &current(), &ctx())
            .await
            .expect("the acceptance must succeed");
    }

    assert_eq!(
        recorded(&db, user, LegalDocument::Terms).await,
        [CURRENT_TERMS_VERSION]
    );
    assert_eq!(
        recorded(&db, user, LegalDocument::Privacy).await,
        [CURRENT_PRIVACY_VERSION]
    );
}

/// Each document is compared on its own: only the one whose latest
/// acceptance is another version gets a row, and its cache column follows.
#[tokio::test]
async fn only_a_document_on_another_version_is_recorded() {
    let db = connect().await;
    let user = create_user(&db).await;
    for (document, version) in [
        (LegalDocument::Terms, CURRENT_TERMS_VERSION),
        (LegalDocument::Privacy, "2000-01-01"),
    ] {
        legal_service::record(
            &db,
            user,
            document,
            version,
            AcceptanceMethod::Explicit,
            &ctx(),
        )
        .await
        .expect("the fixture acceptance must insert");
    }

    legal_service::accept(&db, user, &current(), &ctx())
        .await
        .expect("the acceptance must succeed");

    assert_eq!(
        recorded(&db, user, LegalDocument::Terms).await,
        [CURRENT_TERMS_VERSION]
    );
    assert_eq!(
        recorded(&db, user, LegalDocument::Privacy).await,
        ["2000-01-01", CURRENT_PRIVACY_VERSION]
    );
    let cached = users::Entity::find_by_id(user)
        .one(&db)
        .await
        .expect("the lookup must succeed")
        .expect("the user exists")
        .legal_privacy_version;
    assert_eq!(cached.as_deref(), Some(CURRENT_PRIVACY_VERSION));
}

/// Two acceptances at once are ordered by the account's row, locked `FOR NO
/// KEY UPDATE` like every session write: the second reads what the first
/// recorded, and records nothing. The test holds the row until both wait on
/// it, so they cannot simply run one after the other.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_acceptances_at_once_record_each_document_once() {
    let db = connect().await;
    let user = create_user(&db).await;

    let holder = db.begin().await.expect("the transaction opens");
    holder
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT id FROM users WHERE id = $1 FOR NO KEY UPDATE",
            [user.into()],
        ))
        .await
        .expect("the account locks");
    let holder_pid = locks::backend_pid(&holder).await;

    let requests: Vec<_> = (0..2)
        .map(|_| {
            let db = db.clone();
            tokio::spawn(async move { legal_service::accept(&db, user, &current(), &ctx()).await })
        })
        .collect();
    locks::wait_for_blocked(&db, holder_pid, 2).await;

    // What the requests wait on is the account's row, taken in the mode
    // session writes take it: `FOR NO KEY UPDATE`, which leaves inserts that
    // reference the account free.
    let waiting = db
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT query FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid))",
            [holder_pid.into()],
        ))
        .await
        .expect("the activity query must succeed");
    let queries: Vec<String> = waiting
        .iter()
        .map(|row| row.try_get("", "query").expect("a query text"))
        .collect();
    assert!(
        !queries.is_empty()
            && queries
                .iter()
                .all(|query| query.contains(r#"FROM "users""#)
                    && query.contains("FOR NO KEY UPDATE")),
        "the requests wait on the account's row, locked FOR NO KEY UPDATE, got {queries:?}"
    );

    holder.commit().await.expect("the holder commits");

    for request in requests {
        request
            .await
            .expect("the acceptance must not panic")
            .expect("the acceptance must succeed");
    }
    assert_eq!(
        recorded(&db, user, LegalDocument::Terms).await,
        [CURRENT_TERMS_VERSION]
    );
    assert_eq!(
        recorded(&db, user, LegalDocument::Privacy).await,
        [CURRENT_PRIVACY_VERSION]
    );
}

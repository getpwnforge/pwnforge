// crates/api/src/tasks/cleanup.rs
use chrono::Utc;
use domain::entities::{auth_tokens, refresh_tokens, team_invitations};
use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait, QueryFilter, QuerySelect,
};
use services::admin_service::{self, PurgeOutcome};
use services::background::Shutdown;
use services::jwt_service;
use services::storage_service::ObjectStore;
use services::workspace_invitation_service;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

/// Revoked tokens are kept for the full refresh lifetime: deleting them sooner
/// would turn a replayed token into a plain "unknown token" and defeat reuse
/// detection.
const REVOKED_RETENTION_SECS: i64 = jwt_service::REFRESH_TOKEN_TTL_SECS;

/// Expired tokens are kept for a week: this is long enough to catch most
/// accidental replays, but short enough to avoid bloating the database.
const AUTH_TOKEN_RETENTION_SECS: i64 = 7 * 24 * 60 * 60;

/// Deletes in batches: a single unbounded DELETE on a large table holds locks
/// long enough to stall concurrent logins.
const BATCH_SIZE: u64 = 1000;

/// The cleanup loop: a pass every hour until the shutdown is requested. Run
/// as one of the server's background tasks, so that a shutdown waits for it:
/// a pass under way starts no new batch once it is requested, and the batch
/// in progress, one statement or one account's transaction, ends as it
/// would have.
pub async fn run(db: DatabaseConnection, storage: Arc<ObjectStore>, mut shutdown: Shutdown) {
    let mut ticker = tokio::time::interval(Duration::from_secs(3600));
    loop {
        tokio::select! {
            // A shutdown first: a pass longer than the interval leaves a
            // tick ready at once.
            biased;
            () = shutdown.wait() => return,
            _ = ticker.tick() => {}
        }
        run_once(&db, &storage, &shutdown).await;
    }
}

/// Each table is cleaned independently: a failure on one must not skip the
/// other. Errors are logged and retried on the next tick, never propagated:
/// cleanup must not take the server down.
async fn run_once(db: &DatabaseConnection, storage: &ObjectStore, shutdown: &Shutdown) {
    match clean_refresh_tokens(db, shutdown).await {
        Ok(n) if n > 0 => tracing::info!(deleted = n, "refresh tokens cleaned"),
        Ok(_) => {}
        Err(err) => tracing::error!(error = ?err, "refresh token cleanup failed"),
    }

    match clean_auth_tokens(db, shutdown).await {
        Ok(n) if n > 0 => tracing::info!(deleted = n, "auth tokens cleaned"),
        Ok(_) => {}
        Err(err) => tracing::error!(error = ?err, "auth token cleanup failed"),
    }

    match clean_team_invitations(db, shutdown).await {
        Ok(n) if n > 0 => tracing::info!(deleted = n, "expired team invitations cleaned"),
        Ok(_) => {}
        Err(err) => tracing::error!(error = ?err, "team invitation cleanup failed"),
    }

    // Expired first, so an invitation closing now starts its retention now.
    match workspace_invitation_service::expire_stale(db, Utc::now(), shutdown).await {
        Ok(n) if n > 0 => tracing::info!(expired = n, "workspace invitations expired"),
        Ok(_) => {}
        Err(err) => tracing::error!(error = ?err, "workspace invitation expiry failed"),
    }

    match workspace_invitation_service::purge_history(db, Utc::now(), shutdown).await {
        Ok(n) if n > 0 => tracing::info!(deleted = n, "workspace invitation history purged"),
        Ok(_) => {}
        Err(err) => tracing::error!(error = ?err, "workspace invitation purge failed"),
    }

    // Each account that failed is already logged with its id by the purge.
    match admin_service::purge_scheduled_deletions(db, storage, shutdown).await {
        Ok(PurgeOutcome {
            purged: 0,
            failed: 0,
        }) => {}
        Ok(PurgeOutcome { purged, failed }) => {
            tracing::info!(purged, failed, "scheduled account deletions purged")
        }
        Err(err) => tracing::error!(error = ?err, "scheduled deletion purge failed"),
    }

    match admin_service::purge_unverified_accounts(db, storage, shutdown).await {
        Ok(PurgeOutcome {
            purged: 0,
            failed: 0,
        }) => {}
        Ok(PurgeOutcome { purged, failed }) => {
            tracing::info!(purged, failed, "accounts never verified purged")
        }
        Err(err) => tracing::error!(error = ?err, "unverified account purge failed"),
    }
}

async fn clean_refresh_tokens(db: &DatabaseConnection, shutdown: &Shutdown) -> Result<u64, DbErr> {
    let mut total = 0;
    let now = Utc::now();
    let cutoff = now - chrono::Duration::seconds(REVOKED_RETENTION_SECS);

    loop {
        // A shutdown starts no new batch; the one before it has finished.
        if shutdown.requested() {
            break;
        }

        let ids: Vec<Uuid> = refresh_tokens::Entity::find()
            .select_only()
            .column(refresh_tokens::Column::Id)
            .filter(
                Condition::any()
                    .add(refresh_tokens::Column::ExpiresAt.lt(now))
                    .add(refresh_tokens::Column::RevokedAt.lt(cutoff)),
            )
            .limit(BATCH_SIZE)
            .into_tuple()
            .all(db)
            .await?;

        if ids.is_empty() {
            break;
        }

        let result = refresh_tokens::Entity::delete_many()
            .filter(refresh_tokens::Column::Id.is_in(ids))
            .exec(db)
            .await?;

        total += result.rows_affected;

        // Yield between batches so cleanup never monopolises a connection.
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    Ok(total)
}

async fn clean_auth_tokens(db: &DatabaseConnection, shutdown: &Shutdown) -> Result<u64, DbErr> {
    let mut total = 0;
    let cutoff = Utc::now() - chrono::Duration::seconds(AUTH_TOKEN_RETENTION_SECS);

    loop {
        // A shutdown starts no new batch; the one before it has finished.
        if shutdown.requested() {
            break;
        }

        let ids: Vec<Uuid> = auth_tokens::Entity::find()
            .select_only()
            .column(auth_tokens::Column::Id)
            .filter(auth_tokens::Column::ExpiresAt.lt(cutoff))
            .limit(BATCH_SIZE)
            .into_tuple()
            .all(db)
            .await?;

        if ids.is_empty() {
            break;
        }

        let result = auth_tokens::Entity::delete_many()
            .filter(auth_tokens::Column::Id.is_in(ids))
            .exec(db)
            .await?;

        total += result.rows_affected;

        // Yield between batches so cleanup never monopolises a connection.
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    Ok(total)
}

/// Drops invitations whose deadline has passed.
///
/// Not just housekeeping: the unique indexes on `team_invitations` count
/// expired rows, so a dead invitation kept a person from ever being re-invited
/// to that team. The read paths now filter on the deadline, which fixes the
/// visible symptom; this removes the rows so the indexes stop counting them.
///
/// Deleted outright rather than retained like revoked refresh tokens: there is
/// no reuse detection to support here, an expired invitation carries no
/// evidence value, and the audit log already records who issued it.
async fn clean_team_invitations(
    db: &DatabaseConnection,
    shutdown: &Shutdown,
) -> Result<u64, DbErr> {
    let mut total = 0;
    let now = Utc::now();

    loop {
        // A shutdown starts no new batch; the one before it has finished.
        if shutdown.requested() {
            break;
        }

        let ids: Vec<Uuid> = team_invitations::Entity::find()
            .select_only()
            .column(team_invitations::Column::Id)
            .filter(team_invitations::Column::ExpiresAt.lt(now))
            .limit(BATCH_SIZE)
            .into_tuple()
            .all(db)
            .await?;

        if ids.is_empty() {
            break;
        }

        let result = team_invitations::Entity::delete_many()
            .filter(team_invitations::Column::Id.is_in(ids))
            .exec(db)
            .await?;

        total += result.rows_affected;

        // Yield between batches so cleanup never monopolises a connection.
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        create_team, create_team_workspace, create_user, in_scratch_database, store,
    };
    use domain::dto::team::TeamRole;
    use domain::dto::workspace::InvitationStatus;
    use domain::entities::{users, workspace_invitations};
    use domain::types::TokenKind;
    use sea_orm::{
        ActiveModelTrait, ActiveValue::Set, ConnectionTrait, PaginatorTrait, TransactionTrait,
    };

    /// Three batches' worth.
    const ROWS: u64 = 3 * BATCH_SIZE;

    /// `ROWS` refresh tokens past their expiry, all the scratch database
    /// holds.
    async fn expired_tokens(db: &DatabaseConnection) {
        let user = create_user(db).await;
        let expired = Utc::now() - chrono::Duration::days(1);
        let rows = (0..ROWS).map(|_| refresh_tokens::ActiveModel {
            user_id: Set(user),
            token_hash: Set(Uuid::now_v7().simple().to_string()),
            expires_at: Set(expired.into()),
            ..Default::default()
        });
        refresh_tokens::Entity::insert_many(rows)
            .exec(db)
            .await
            .expect("the tokens insert");
    }

    async fn remaining(db: &DatabaseConnection) -> u64 {
        refresh_tokens::Entity::find()
            .count(db)
            .await
            .expect("the count succeeds")
    }

    /// Waits until a statement of this database waits on a row lock.
    async fn wait_for_a_blocked_statement(db: &DatabaseConnection) {
        for _ in 0..200 {
            let blocked = db
                .query_one_raw(sea_orm::Statement::from_string(
                    sea_orm::DbBackend::Postgres,
                    "SELECT count(*) AS n FROM pg_stat_activity \
                     WHERE datname = current_database() AND wait_event_type = 'Lock'",
                ))
                .await
                .expect("the activity query succeeds")
                .and_then(|row| row.try_get::<i64>("", "n").ok())
                .unwrap_or(0);
            if blocked > 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("the batch never waited on the lock");
    }

    #[tokio::test]
    async fn a_shutdown_already_requested_starts_no_batch() {
        in_scratch_database(|db| async move {
            expired_tokens(&db).await;
            let (trigger, shutdown) = Shutdown::channel();
            trigger.trigger();

            let deleted = clean_refresh_tokens(&db, &shutdown)
                .await
                .expect("the cleanup runs");

            assert_eq!(deleted, 0);
            assert_eq!(remaining(&db).await, ROWS);
        })
        .await;
    }

    /// The shutdown comes while the first batch's DELETE waits on a row
    /// lock: that batch ends whole once the lock goes, and the next one
    /// never starts.
    #[tokio::test]
    async fn a_shutdown_lets_the_batch_under_way_end_and_starts_no_other() {
        in_scratch_database(|db| async move {
            expired_tokens(&db).await;
            let locker = db.begin().await.expect("a transaction");
            locker
                .execute_unprepared("SELECT id FROM refresh_tokens FOR UPDATE")
                .await
                .expect("the rows lock");

            let (trigger, shutdown) = Shutdown::channel();
            let pass = {
                let db = db.clone();
                tokio::spawn(async move { clean_refresh_tokens(&db, &shutdown).await })
            };
            wait_for_a_blocked_statement(&db).await;
            trigger.trigger();
            locker.rollback().await.expect("the lock goes");

            let deleted = tokio::time::timeout(Duration::from_secs(10), pass)
                .await
                .expect("the pass must end")
                .expect("the pass must not panic")
                .expect("the cleanup runs");

            assert_eq!(
                deleted, BATCH_SIZE,
                "the batch under way ended, no other began"
            );
            assert_eq!(remaining(&db).await, ROWS - BATCH_SIZE);
        })
        .await;
    }

    /// The two accounts `one_of_each` makes for the purges.
    struct Accounts([Uuid; 2]);

    /// One row of each kind a pass deletes or closes: an expired refresh
    /// token, an auth token past its retention, an expired team invitation,
    /// a workspace invitation still pending past its deadline, one declined
    /// past its history's retention, an account past its scheduled deletion
    /// and an account never verified past its grace.
    async fn one_of_each(db: &DatabaseConnection) -> Accounts {
        let long_ago = Utc::now() - chrono::Duration::days(30);
        let user = create_user(db).await;
        refresh_tokens::ActiveModel {
            user_id: Set(user),
            token_hash: Set(Uuid::now_v7().simple().to_string()),
            expires_at: Set(long_ago.into()),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("the refresh token inserts");
        auth_tokens::ActiveModel {
            id: Set(Uuid::now_v7()),
            user_id: Set(user),
            kind: Set(TokenKind::PasswordReset.as_str().to_owned()),
            token_hash: Set(Uuid::now_v7().simple().to_string()),
            expires_at: Set(long_ago.into()),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("the auth token inserts");
        team_invitations::ActiveModel {
            id: Set(Uuid::now_v7()),
            team_id: Set(create_team(db, user).await),
            token: Set(Uuid::now_v7().simple().to_string()),
            default_role: Set(TeamRole::Member.as_str().to_owned()),
            expires_at: Set(long_ago.into()),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("the team invitation inserts");
        let team = create_team(db, user).await;
        let workspace = create_team_workspace(db, team, "Cleaned").await.id;
        for (status, resolved_days_ago) in [
            (InvitationStatus::Pending, None),
            (InvitationStatus::Declined, Some(100)),
        ] {
            let pending = resolved_days_ago.is_none();
            workspace_invitations::ActiveModel {
                id: Set(Uuid::now_v7()),
                workspace_id: Set(workspace),
                invited_user_id: Set((!pending).then_some(user)),
                invited_email: Set(pending.then(|| "invited@example.test".to_owned())),
                role: Set("member".to_owned()),
                token: Set(Uuid::now_v7().simple().to_string()),
                status: Set(status.as_str().to_owned()),
                expires_at: Set(long_ago.into()),
                resolved_at: Set(resolved_days_ago
                    .map(|days| (Utc::now() - chrono::Duration::days(days)).into())),
                ..Default::default()
            }
            .insert(db)
            .await
            .expect("the workspace invitation inserts");
        }
        let scheduled = create_user(db).await;
        users::ActiveModel {
            id: Set(scheduled),
            purge_scheduled_at: Set(Some(long_ago.into())),
            ..Default::default()
        }
        .update(db)
        .await
        .expect("the deletion is scheduled");
        let unverified = create_user(db).await;
        users::ActiveModel {
            id: Set(unverified),
            created_at: Set(long_ago.into()),
            ..Default::default()
        }
        .update(db)
        .await
        .expect("the account is aged");

        Accounts([scheduled, unverified])
    }

    /// How many of `one_of_each`'s rows are left: refresh tokens, auth
    /// tokens, team invitations, workspace invitations still pending,
    /// workspace invitations, accounts.
    async fn left(db: &DatabaseConnection, accounts: &Accounts) -> [u64; 6] {
        let refresh = refresh_tokens::Entity::find().count(db).await;
        let auth = auth_tokens::Entity::find().count(db).await;
        let team = team_invitations::Entity::find().count(db).await;
        let pending = workspace_invitations::Entity::find()
            .filter(workspace_invitations::Column::Status.eq(InvitationStatus::Pending.as_str()))
            .count(db)
            .await;
        let workspace = workspace_invitations::Entity::find().count(db).await;
        let accounts = users::Entity::find()
            .filter(users::Column::Id.is_in(accounts.0))
            .count(db)
            .await;
        [refresh, auth, team, pending, workspace, accounts].map(|n| n.expect("the count succeeds"))
    }

    /// Every step of a pass checks the shutdown before its first batch: a
    /// pass that starts after it deletes nothing. Then, as a control, a pass
    /// with no shutdown deletes every row.
    #[tokio::test]
    async fn a_pass_after_the_shutdown_deletes_nothing() {
        in_scratch_database(|db| async move {
            let accounts = one_of_each(&db).await;
            let storage = store();
            let (trigger, shutdown) = Shutdown::channel();
            trigger.trigger();

            run_once(&db, &storage, &shutdown).await;
            assert_eq!(
                left(&db, &accounts).await,
                [1, 1, 1, 1, 2, 2],
                "nothing is deleted or closed"
            );

            run_once(&db, &storage, &Shutdown::never()).await;
            // The expired invitation stays, as history.
            assert_eq!(
                left(&db, &accounts).await,
                [0, 0, 0, 0, 1, 0],
                "a pass deletes or closes every row"
            );
        })
        .await;
    }

    #[tokio::test]
    async fn the_cleanup_loop_ends_once_the_shutdown_is_requested() {
        in_scratch_database(|db| async move {
            let (trigger, shutdown) = Shutdown::channel();
            let task = tokio::spawn(run(db, Arc::new(store()), shutdown));
            // Past the first pass, which starts at once, into the wait for
            // the next tick.
            tokio::time::sleep(Duration::from_millis(500)).await;

            trigger.trigger();

            tokio::time::timeout(Duration::from_secs(10), task)
                .await
                .expect("the loop must end")
                .expect("the loop must not panic");
        })
        .await;
    }
}

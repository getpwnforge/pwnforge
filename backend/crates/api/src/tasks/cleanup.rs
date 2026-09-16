// crates/api/src/tasks/cleanup.rs
use chrono::Utc;
use domain::entities::{auth_tokens, refresh_tokens, team_invitations};
use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait, QueryFilter, QuerySelect,
};
use services::{admin_service, jwt_service};
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

/// Spawns the background cleanup loop. Returns immediately: the task runs for
/// the lifetime of the process.
pub fn spawn(db: DatabaseConnection) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(3600));
        loop {
            ticker.tick().await;
            run_once(&db).await;
        }
    });
}

/// Each table is cleaned independently: a failure on one must not skip the
/// other. Errors are logged and retried on the next tick, never propagated:
/// cleanup must not take the server down.
async fn run_once(db: &DatabaseConnection) {
    match clean_refresh_tokens(db).await {
        Ok(n) if n > 0 => tracing::info!(deleted = n, "refresh tokens cleaned"),
        Ok(_) => {}
        Err(err) => tracing::error!(error = ?err, "refresh token cleanup failed"),
    }

    match clean_auth_tokens(db).await {
        Ok(n) if n > 0 => tracing::info!(deleted = n, "auth tokens cleaned"),
        Ok(_) => {}
        Err(err) => tracing::error!(error = ?err, "auth token cleanup failed"),
    }

    match clean_team_invitations(db).await {
        Ok(n) if n > 0 => tracing::info!(deleted = n, "expired team invitations cleaned"),
        Ok(_) => {}
        Err(err) => tracing::error!(error = ?err, "team invitation cleanup failed"),
    }

    match admin_service::purge_scheduled_deletions(db).await {
        Ok(n) if n > 0 => tracing::info!(purged = n, "scheduled account deletions purged"),
        Ok(_) => {}
        Err(err) => tracing::error!(error = ?err, "scheduled deletion purge failed"),
    }
}

async fn clean_refresh_tokens(db: &DatabaseConnection) -> Result<u64, DbErr> {
    let mut total = 0;
    let now = Utc::now();
    let cutoff = now - chrono::Duration::seconds(REVOKED_RETENTION_SECS);

    loop {
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

async fn clean_auth_tokens(db: &DatabaseConnection) -> Result<u64, DbErr> {
    let mut total = 0;
    let cutoff = Utc::now() - chrono::Duration::seconds(AUTH_TOKEN_RETENTION_SECS);

    loop {
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
async fn clean_team_invitations(db: &DatabaseConnection) -> Result<u64, DbErr> {
    let mut total = 0;
    let now = Utc::now();

    loop {
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

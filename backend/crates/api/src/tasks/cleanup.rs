// crates/api/src/tasks/cleanup.rs
use crate::services::jwt_service;
use chrono::Utc;
use domain::entities::refresh_tokens;
use sea_orm::DbErr;
use sea_orm::{ColumnTrait, Condition, DatabaseConnection, EntityTrait, QueryFilter, QuerySelect};
use std::time::Duration;
use uuid::Uuid;

/// Revoked tokens are kept for the full refresh lifetime: deleting them sooner
/// would turn a replayed token into a plain "unknown token" and defeat reuse
/// detection.
const REVOKED_RETENTION_SECS: i64 = jwt_service::REFRESH_TOKEN_TTL_SECS;

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

            // Failures are logged and retried on the next tick: cleanup must
            // never take the server down.
            if let Err(err) = run_once(&db).await {
                tracing::error!(error = ?err, "refresh token cleanup failed");
            }
        }
    });
}

async fn run_once(db: &DatabaseConnection) -> Result<u64, DbErr> {
    let now = Utc::now();
    let revoked_cutoff = now - chrono::Duration::seconds(REVOKED_RETENTION_SECS);
    let mut total = 0;

    loop {
        let ids: Vec<Uuid> = refresh_tokens::Entity::find()
            .select_only()
            .column(refresh_tokens::Column::Id)
            .filter(
                Condition::any()
                    .add(refresh_tokens::Column::ExpiresAt.lt(now))
                    .add(refresh_tokens::Column::RevokedAt.lt(revoked_cutoff)),
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

    if total > 0 {
        tracing::info!(deleted = total, "cleaned up refresh tokens");
    }

    Ok(total)
}

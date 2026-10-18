// crates/services/src/suspension_cache.rs
//! The request gate's cache of each account's suspension state, in Redis.
//!
//! Two writers, and the order between them is the point. A request that
//! misses reads the database and fills the entry, but only an empty one
//! (`SET NX`). A change that blocks an account, a suspension or a deletion,
//! writes the blocked state over whatever the entry holds, instead of
//! deleting it. A request that read the account just before the change can
//! then no longer put the old state back for the rest of the TTL: its fill
//! finds the entry taken.

use crate::auth_service::{self, AuthError};
use chrono::Utc;
use domain::entities::users;
use redis::{AsyncCommands, ExistenceCheck, SetExpiry, SetOptions, aio::ConnectionManager};
use sea_orm::{ConnectionTrait, DbErr, EntityTrait, QuerySelect, prelude::DateTimeWithTimeZone};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Short enough that a suspension takes effect within seconds even if the
/// write on the admin side fails.
pub const SUSPENSION_CACHE_TTL_SECS: u64 = 30;

pub fn suspension_cache_key(user_id: Uuid) -> String {
    format!("user:{user_id}:suspension")
}

/// Cached suspension state for a user. All fields `None` means active.
#[derive(Debug, Serialize, Deserialize)]
pub struct SuspensionState {
    pub suspended_at: Option<DateTimeWithTimeZone>,
    pub suspended_until: Option<DateTimeWithTimeZone>,
    pub suspended_reason: Option<String>,
    pub purge_scheduled_at: Option<DateTimeWithTimeZone>,
}

impl SuspensionState {
    pub fn as_error(&self) -> Option<AuthError> {
        auth_service::suspension_error(
            self.suspended_at,
            self.suspended_until,
            self.suspended_reason.clone(),
            self.purge_scheduled_at,
        )
    }
}

type SuspensionRow = (
    Option<DateTimeWithTimeZone>,
    Option<DateTimeWithTimeZone>,
    Option<String>,
    Option<DateTimeWithTimeZone>,
);

/// The account's state as the database has it, or `None` when the account
/// does not exist.
pub async fn read<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
) -> Result<Option<SuspensionState>, DbErr> {
    let row: Option<SuspensionRow> = users::Entity::find_by_id(user_id)
        .select_only()
        .column(users::Column::SuspendedAt)
        .column(users::Column::SuspendedUntil)
        .column(users::Column::SuspendedReason)
        .column(users::Column::PurgeScheduledAt)
        .into_tuple()
        .one(db)
        .await?;

    Ok(row.map(
        |(suspended_at, suspended_until, suspended_reason, purge_scheduled_at)| SuspensionState {
            suspended_at,
            suspended_until,
            suspended_reason,
            purge_scheduled_at,
        },
    ))
}

/// Fills the entry from a read of the database, if it is still empty. Best
/// effort: a failed write only means the next request reads again.
pub async fn fill(redis: &mut ConnectionManager, user_id: Uuid, state: &SuspensionState) {
    let Ok(payload) = serde_json::to_string(state) else {
        return;
    };
    let options = SetOptions::default()
        .conditional_set(ExistenceCheck::NX)
        .with_expiration(SetExpiry::EX(SUSPENSION_CACHE_TTL_SECS));
    let _: Result<Option<String>, _> = redis
        .set_options(suspension_cache_key(user_id), payload, options)
        .await;
}

/// Writes the state an account was just put in, read back after the commit,
/// over whatever the entry holds. For a change that blocks the account: a
/// suspension, a deletion request, a deletion. An account deleted outright
/// has no row left to read, and its entry says pending deletion, which the
/// gate refuses the same way.
///
/// Best effort, like the cache: on a failure, the entry is dropped if it can
/// be, and the TTL bounds what is left.
pub(crate) async fn store_blocked<C: ConnectionTrait>(
    db: &C,
    redis: &mut ConnectionManager,
    user_id: Uuid,
) {
    let key = suspension_cache_key(user_id);

    let state = match read(db, user_id).await {
        Ok(Some(state)) => state,
        Ok(None) => SuspensionState {
            suspended_at: None,
            suspended_until: None,
            suspended_reason: None,
            purge_scheduled_at: Some(Utc::now().into()),
        },
        Err(err) => {
            tracing::warn!(error = ?err, %user_id, "failed to read the state to cache, dropping the entry");
            clear(redis, user_id).await;
            return;
        }
    };

    let Ok(payload) = serde_json::to_string(&state) else {
        clear(redis, user_id).await;
        return;
    };

    if let Err(err) = redis
        .set_ex::<_, _, ()>(&key, payload, SUSPENSION_CACHE_TTL_SECS)
        .await
    {
        tracing::warn!(error = ?err, %user_id, "failed to write the suspension cache");
    }
}

/// Drops the entry, so the next request reads the database. For a change
/// that lifts a block: a stale fill can then only keep the account blocked a
/// little longer, never let it through.
pub(crate) async fn clear(redis: &mut ConnectionManager, user_id: Uuid) {
    if let Err(err) = redis.del::<_, ()>(suspension_cache_key(user_id)).await {
        tracing::warn!(error = ?err, %user_id, "failed to invalidate suspension cache");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(
        at: Option<DateTimeWithTimeZone>,
        until: Option<DateTimeWithTimeZone>,
        purge_scheduled_at: Option<DateTimeWithTimeZone>,
    ) -> SuspensionState {
        SuspensionState {
            suspended_at: at,
            suspended_until: until,
            suspended_reason: None,
            purge_scheduled_at,
        }
    }

    #[test]
    fn active_account_is_not_rejected() {
        assert!(state(None, None, None).as_error().is_none());
    }

    #[test]
    fn permanent_suspension_is_rejected() {
        let at = Utc::now().into();
        assert!(state(Some(at), None, None).as_error().is_some());
    }

    #[test]
    fn future_deadline_is_rejected() {
        let at = Utc::now().into();
        let until = (Utc::now() + chrono::Duration::days(1)).into();
        assert!(state(Some(at), Some(until), None).as_error().is_some());
    }

    #[test]
    fn expired_suspension_does_not_block() {
        let at = (Utc::now() - chrono::Duration::days(2)).into();
        let until = (Utc::now() - chrono::Duration::days(1)).into();
        assert!(state(Some(at), Some(until), None).as_error().is_none());
    }
}

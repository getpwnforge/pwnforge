// crates/api/src/middleware/auth.rs
use crate::{
    error::AppError,
    services::{auth_service::{self, AuthError}, jwt_service},
    state::AppState,
};
use axum::{extract::FromRequestParts, http::request::Parts};
use axum_extra::extract::CookieJar;
use domain::entities::users;
use redis::AsyncCommands;
use sea_orm::{EntityTrait, QuerySelect, prelude::DateTimeWithTimeZone};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Short enough that a suspension takes effect within seconds even if the
/// explicit invalidation on the admin side fails.
const SUSPENSION_CACHE_TTL_SECS: u64 = 30;

/// An authenticated caller.
///
/// Handlers that take this parameter are protected: the requirement is visible
/// in the signature rather than in a layer that can be forgotten when adding a
/// route. Identity only, no permissions: those are resolved per context by the
/// workspace and team extractors so they can never go stale.
pub struct AuthUser {
    pub id: Uuid,
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);

        let token = jar
            .get("access_token")
            .map(|cookie| cookie.value())
            .ok_or(AppError::Unauthorized)?;

        let claims = jwt_service::verify_access_token(token, &state.jwt_keys.decoding)
            .map_err(|_| AppError::Unauthorized)?;

        let user_id = claims.sub;

        if let Some(err) = resolve_suspension(state, user_id).await?.as_error() {
            tracing::warn!(%user_id, "suspended user attempted access");
            return Err(err.into());
        }

        Ok(AuthUser { id: user_id })
    }
}

/// Cached suspension state for a user. All fields `None` means active.
#[derive(Debug, Serialize, Deserialize)]
struct SuspensionState {
    suspended_at: Option<DateTimeWithTimeZone>,
    suspended_until: Option<DateTimeWithTimeZone>,
    suspended_reason: Option<String>,
}

impl SuspensionState {
    fn as_error(&self) -> Option<AuthError> {
        auth_service::suspension_error(
            self.suspended_at,
            self.suspended_until,
            self.suspended_reason.clone(),
        )
    }
}

pub(crate) fn suspension_cache_key(user_id: Uuid) -> String {
    format!("user:{user_id}:suspension")
}

/// Resolves suspension state, reading through a short-lived Redis cache.
///
/// Redis is an optimisation, not the source of truth: any Redis failure falls
/// through to the database rather than guessing in either direction. A failed
/// cache write is ignored, since the next request will simply query again.
///
/// The negative case is cached too: without it, the overwhelming majority of
/// requests would miss and the cache would buy nothing.
async fn resolve_suspension(state: &AppState, user_id: Uuid) -> Result<SuspensionState, AppError> {
    let mut redis = state.redis.clone();
    let key = suspension_cache_key(user_id);

    let cached: Option<String> = redis.get(&key).await.unwrap_or(None);

    if let Some(data) = cached
        && let Ok(parsed) = serde_json::from_str::<SuspensionState>(&data)
    {
        return Ok(parsed);
    }

    let row: Option<(
        Option<DateTimeWithTimeZone>,
        Option<DateTimeWithTimeZone>,
        Option<String>,
    )> = users::Entity::find_by_id(user_id)
        .select_only()
        .column(users::Column::SuspendedAt)
        .column(users::Column::SuspendedUntil)
        .column(users::Column::SuspendedReason)
        .into_tuple()
        .one(&state.db)
        .await?;

    // A valid token pointing at a row that no longer exists: the account was
    // deleted while the access token was still live.
    let (suspended_at, suspended_until, suspended_reason) = row.ok_or(AppError::Unauthorized)?;

    let suspension = SuspensionState {
        suspended_at,
        suspended_until,
        suspended_reason,
    };

    if let Ok(payload) = serde_json::to_string(&suspension) {
        let _: Result<(), _> = redis.set_ex(&key, payload, SUSPENSION_CACHE_TTL_SECS).await;
    }

    Ok(suspension)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn state(
        at: Option<DateTimeWithTimeZone>,
        until: Option<DateTimeWithTimeZone>,
    ) -> SuspensionState {
        SuspensionState {
            suspended_at: at,
            suspended_until: until,
            suspended_reason: None,
        }
    }

    #[test]
    fn active_account_is_not_rejected() {
        assert!(state(None, None).as_error().is_none());
    }

    #[test]
    fn permanent_suspension_is_rejected() {
        let at = Utc::now().into();
        assert!(state(Some(at), None).as_error().is_some());
    }

    #[test]
    fn future_deadline_is_rejected() {
        let at = Utc::now().into();
        let until = (Utc::now() + chrono::Duration::days(1)).into();
        assert!(state(Some(at), Some(until)).as_error().is_some());
    }

    #[test]
    fn expired_suspension_does_not_block() {
        let at = (Utc::now() - chrono::Duration::days(2)).into();
        let until = (Utc::now() - chrono::Duration::days(1)).into();
        assert!(state(Some(at), Some(until)).as_error().is_none());
    }
}

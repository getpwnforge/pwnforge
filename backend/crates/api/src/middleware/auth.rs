// crates/api/src/middleware/auth.rs
use crate::{error::AppError, state::AppState};
use axum::{extract::FromRequestParts, http::request::Parts};
use axum_extra::extract::CookieJar;
use redis::AsyncCommands;
use services::{
    jwt_service,
    suspension_cache::{self, SuspensionState, suspension_cache_key},
};
use uuid::Uuid;

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

/// Resolves suspension state, reading through a short-lived Redis cache.
///
/// Redis is an optimisation, not the source of truth: any Redis failure falls
/// through to the database rather than guessing in either direction. A failed
/// cache write is ignored, since the next request will simply query again.
///
/// The negative case is cached too: without it, the overwhelming majority of
/// requests would miss and the cache would buy nothing. The fill only takes
/// an empty entry (see `suspension_cache`): a suspension landing between the
/// read below and the fill has written its own state, which must win.
async fn resolve_suspension(state: &AppState, user_id: Uuid) -> Result<SuspensionState, AppError> {
    let mut redis = state.redis.clone();
    let key = suspension_cache_key(user_id);

    let cached: Option<String> = redis.get(&key).await.unwrap_or(None);

    if let Some(data) = cached
        && let Ok(parsed) = serde_json::from_str::<SuspensionState>(&data)
    {
        return Ok(parsed);
    }

    // A valid token pointing at a row that no longer exists: the account was
    // deleted while the access token was still live.
    let suspension = suspension_cache::read(&state.db, user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;

    suspension_cache::fill(&mut redis, user_id, &suspension).await;

    Ok(suspension)
}

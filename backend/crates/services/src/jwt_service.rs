// crates/api/src/services/jwt_service.rs
use chrono::Utc;
pub use jsonwebtoken::EncodingKey;
use jsonwebtoken::{Algorithm, DecodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

// Access token lifetime (15 minutes)
// Security boundary: access tokens are not revocable, this window bounds the damage.
pub const ACCESS_TOKEN_TTL_SECS: i64 = 15 * 60;

// Refresh token lifetime (30 days)
// Security boundary: refresh tokens are revocable, but we don't want to force users to log in too often. This is a tradeoff between security and usability.
pub const REFRESH_TOKEN_TTL_SECS: i64 = 30 * 24 * 60 * 60; // 30 days

#[derive(Debug, Error)]
pub enum JwtError {
    #[error("token is invalid or expired")]
    Invalid(#[from] jsonwebtoken::errors::Error),
}

/// Claims carried by an access token. Identity only, never permissions.
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,
    pub iat: i64,
    pub exp: i64,
}

/// Issues a short-lived access token for the given user.
pub fn generate_access_token(user_id: Uuid, key: &EncodingKey) -> Result<String, JwtError> {
    let now = Utc::now().timestamp();
    let claims = Claims {
        sub: user_id,
        iat: now,
        exp: now + ACCESS_TOKEN_TTL_SECS,
    };
    Ok(encode(&Header::new(Algorithm::HS256), &claims, key)?)
}

/// Verifies an access token's signature and expiry.
///
/// # Errors
///
/// Returns [`JwtError::Invalid`] for any failure (bad signature, wrong
/// algorithm, expired, or malformed). Callers must not distinguish these.
pub fn verify_access_token(token: &str, key: &DecodingKey) -> Result<Claims, JwtError> {
    let validation = Validation::new(Algorithm::HS256);
    Ok(decode::<Claims>(token, key, &validation)?.claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_and_verify_access_token() {
        let user_id = Uuid::now_v7();
        let key = EncodingKey::from_secret(b"secret");
        let token = generate_access_token(user_id, &key).unwrap();
        let decoded = verify_access_token(&token, &DecodingKey::from_secret(b"secret")).unwrap();
        assert_eq!(decoded.sub, user_id);
    }
}

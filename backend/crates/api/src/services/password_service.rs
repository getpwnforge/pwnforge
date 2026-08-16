// crates/api/src/services/auth_service.rs
use argon2::password_hash::{
    PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng,
};
use argon2::{Algorithm, Argon2, Params, Version};
use std::sync::{LazyLock, OnceLock};
use thiserror::Error;

// OWASP recommends the following parameters for Argon2id:
// - Memory cost: 19 MiB
// - Iteration count: 2
// - Parallelism: 1
const ARGON2_MEMORY_COST: u32 = 19 * 1024; // 19 MiB
const ARGON2_ITERATIONS: u32 = 2;
const ARGON2_PARALLELISM: u32 = 1;
const OUTPUT_HASH_LENGTH: usize = 32; // 32 bytes

/// Errors that can occur during password hashing and verification.
#[derive(Debug, Error)]
pub enum PasswordError {
    /// The Argon2 backend failed, or a stored hash could not be parsed.
    #[error("password hashing failed: {0}")]
    Hashing(#[from] argon2::password_hash::Error),

    /// The blocking task was cancelled or panicked.
    #[error("password task failed to complete")]
    Join(#[from] tokio::task::JoinError),
}

/// Returns a reference to a static Argon2 instance with the recommended parameters.
fn hasher() -> &'static Argon2<'static> {
    static HASHER: OnceLock<Argon2<'static>> = OnceLock::new();
    HASHER.get_or_init(|| {
        Argon2::new(
            Algorithm::Argon2id,
            Version::V0x13,
            Params::new(
                ARGON2_MEMORY_COST,
                ARGON2_ITERATIONS,
                ARGON2_PARALLELISM,
                Some(OUTPUT_HASH_LENGTH),
            )
            .expect("Invalid Argon2 parameters"),
        )
    })
}

/// Hashes a plaintext password using Argon2id.
///
/// Runs on a blocking thread pool: hashing takes 50-100ms of CPU with the
/// configured parameters and would otherwise stall the async executor.
///
/// # Errors
///
/// Returns [`PasswordError::Hashing`] if the Argon2 backend fails, or
/// [`PasswordError::Join`] if the blocking task panicked.
pub async fn hash_password(password: String) -> Result<String, PasswordError> {
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut OsRng);
        hasher()
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(PasswordError::from)
    })
    .await?
}

/// Verifies a plaintext password against a hashed password using Argon2id.
///
/// Runs on a blocking thread pool: verification takes 50-100ms of CPU with the
/// configured parameters and would otherwise stall the async executor.
///
/// # Errors
///
/// Returns [`PasswordError::Hashing`] if the Argon2 backend fails, or
/// [`PasswordError::Join`] if the blocking task panicked.
pub async fn verify_password(password: String, hash: String) -> Result<bool, PasswordError> {
    tokio::task::spawn_blocking(move || {
        let parsed_hash = PasswordHash::new(&hash).map_err(PasswordError::from)?;
        Ok(hasher()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok())
    })
    .await?
}

/// A valid Argon2 hash of a random secret, used to keep the login timing
/// identical whether or not the account exists. Computed once, on first use.
pub static DUMMY_HASH: LazyLock<String> = LazyLock::new(|| {
    let salt = SaltString::generate(&mut OsRng);
    hasher()
        .hash_password(b"dummy-password-never-matches", &salt)
        .expect("failed to build the dummy hash")
        .to_string()
});

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn hash_then_verify_succeeds() {
        let hash = hash_password("correct horse battery".into()).await.unwrap();
        assert!(
            verify_password("correct horse battery".into(), hash)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn wrong_password_fails() {
        let hash = hash_password("correct horse battery".into()).await.unwrap();
        assert!(!verify_password("wrong".into(), hash).await.unwrap());
    }

    #[tokio::test]
    async fn same_password_produces_different_hashes() {
        let a = hash_password("same".into()).await.unwrap();
        let b = hash_password("same".into()).await.unwrap();
        assert_ne!(a, b); // random salt per hash
    }

    #[tokio::test]
    async fn malformed_hash_returns_error() {
        let result = verify_password("whatever".into(), "not-a-phc-string".into()).await;
        assert!(matches!(result, Err(PasswordError::Hashing(_))));
    }
}

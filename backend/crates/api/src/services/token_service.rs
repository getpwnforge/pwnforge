// crates/api/src/services/token_service.rs
use argon2::password_hash::rand_core::{OsRng, RngCore};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

// Length of an opaque token, in bytes. 256 bits of entropy, brute-forcing
// is infeasible, which is why a fast hash is sufficient for storage.
const TOKEN_BYTES: usize = 32;

/// A freshly generated opaque token: the secret to hand out, and the hash to store.
pub struct OpaqueToken {
    // Sent to the client. Never persisted.
    pub secret: String,
    // Stored in the database. Never sent.
    pub hash: String,
}

/// Generates a cryptographically random opaque token.
pub fn generate_opaque_token() -> OpaqueToken {
    let mut bytes = [0u8; TOKEN_BYTES];
    OsRng.fill_bytes(&mut bytes);

    let secret = URL_SAFE_NO_PAD.encode(bytes);
    let hash = hash_opaque_token(&secret);

    OpaqueToken { secret, hash }
}

/// Hashes a token for storage or lookup.
///
/// SHA-256 rather than Argon2: the input is 256 bits of uniform randomness,
/// not a low-entropy human password, so key stretching serves no purpose.
pub fn hash_opaque_token(secret: &str) -> String {
    let digest = Sha256::digest(secret.as_bytes());
    hex::encode(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_tokens_are_unique() {
        let a = generate_opaque_token();
        let b = generate_opaque_token();
        assert_ne!(a.secret, b.secret);
        assert_ne!(a.hash, b.hash);
    }

    #[test]
    fn hash_is_deterministic() {
        let token = generate_opaque_token();
        assert_eq!(hash_opaque_token(&token.secret), token.hash);
    }

    #[test]
    fn secret_is_url_safe() {
        let token = generate_opaque_token();
        assert!(
            token
                .secret
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
    }
}

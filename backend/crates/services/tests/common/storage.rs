//! The object store the backend is configured with, for the tests that
//! talk to it. Locally `make test-backend` points the endpoint at the dev
//! compose's Garage; CI provides its own.
//!
//! Included with `#[path]` by the tests that use it; see `scratch.rs`.

use services::config::StorageConfig;

/// Reads the same variables the backend reads, so a test run exercises the
/// configuration path rather than a hand-built struct.
///
/// Every field comes from the environment, including the region. That is not
/// incidental: Garage signs against `garage` and Cloudflare R2 against `auto`,
/// so a hardcoded `us-east-1` anywhere here would make the suite pass on one
/// and fail on a signature mismatch everywhere else - which is exactly the
/// portability this layer exists to have.
pub fn storage_config() -> StorageConfig {
    StorageConfig {
        endpoint: Some(
            std::env::var("PWNFORGE_S3_ENDPOINT")
                .unwrap_or_else(|_| "http://127.0.0.1:3900".to_owned()),
        ),
        region: std::env::var("PWNFORGE_S3_REGION").unwrap_or_else(|_| "us-east-1".to_owned()),
        access_key_id: std::env::var("PWNFORGE_S3_ACCESS_KEY_ID")
            .unwrap_or_else(|_| "pwnforge-app".to_owned()),
        secret_access_key: std::env::var("PWNFORGE_S3_SECRET_ACCESS_KEY")
            .expect("PWNFORGE_S3_SECRET_ACCESS_KEY must be set; see the module docs"),
        force_path_style: std::env::var("PWNFORGE_S3_FORCE_PATH_STYLE")
            .map(|raw| raw != "false")
            .unwrap_or(true),
        bucket_attachments: std::env::var("PWNFORGE_S3_BUCKET_ATTACHMENTS")
            .unwrap_or_else(|_| "pwnforge-attachments".to_owned()),
        bucket_avatars: std::env::var("PWNFORGE_S3_BUCKET_AVATARS")
            .unwrap_or_else(|_| "pwnforge-avatars".to_owned()),
        key_prefix: None,
        public_base_url: None,
    }
}

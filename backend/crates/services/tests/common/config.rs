//! A `Config` for the tests that call a service taking one: mail goes to the
//! console, Turnstile and the breach check are off, and nothing in it points
//! at a real store or server. A test that needs a field set builds on it with
//! struct update syntax.
//!
//! Included with `#[path]` by the tests that use it; see `scratch.rs`.

use services::config::{Config, EmailBackend, SmtpTls, StorageConfig};

pub fn config() -> Config {
    Config {
        port: 0,
        public_url: "http://localhost".to_owned(),
        cors_origin: "http://localhost:5173".to_owned(),
        setup_token: None,
        database_url: String::new(),
        database_max_connections: 1,
        redis_url: String::new(),
        jwt_secret: "x".repeat(32),
        encryption_key: "x".repeat(32),
        turnstile_site_key: None,
        turnstile_secret_key: None,
        cookie_secure: false,
        trusted_proxy_count: 0,
        block_disposable_emails: false,
        // No network call to the breach database from a test.
        check_pwned_passwords: false,
        email_backend: EmailBackend::Console,
        email_from: String::new(),
        support_email: None,
        smtp_host: String::new(),
        smtp_port: 0,
        smtp_username: String::new(),
        smtp_password: String::new(),
        smtp_tls: SmtpTls::None,
        resend_api_key: String::new(),
        // No service reads it: the ones that touch the bucket take the store
        // as an argument.
        storage: StorageConfig {
            endpoint: None,
            region: "unused".to_owned(),
            access_key_id: "unused".to_owned(),
            secret_access_key: "unused".to_owned(),
            force_path_style: true,
            bucket_attachments: "unused".to_owned(),
            bucket_avatars: "unused".to_owned(),
            key_prefix: None,
            public_base_url: None,
        },
    }
}

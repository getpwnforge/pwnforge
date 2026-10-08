//! `TURNSTILE_SECRET_KEY` read through `Config::from_env`: a blank value is no
//! key at all.
//!
//! Its own test binary, holding a single test, because it has to write the
//! process environment: `set_var` is only sound while no other thread reads
//! it, and the test harness runs every test of a binary on its own thread.
//! Every variable `from_env` validates is set here, so the outcome does not
//! depend on the `.env` that `make test-backend` exports.

use services::config::Config;

const VALID: &[(&str, &str)] = &[
    ("PORT", "3000"),
    ("PUBLIC_URL", "http://localhost:5173"),
    ("CORS_ORIGIN", "http://localhost:5173"),
    ("DATABASE_URL", "postgresql://user:pass@127.0.0.1:5432/db"),
    ("REDIS_URL", "redis://127.0.0.1:6379/0"),
    ("JWT_SECRET", "0123456789abcdef0123456789abcdef"),
    ("ENCRYPTION_KEY", "0123456789abcdef0123456789abcdef"),
    ("EMAIL_BACKEND", "console"),
    ("PWNFORGE_S3_REGION", "garage"),
    ("PWNFORGE_S3_ACCESS_KEY_ID", "test-access-key"),
    ("PWNFORGE_S3_SECRET_ACCESS_KEY", "test-secret-key"),
    ("PWNFORGE_S3_BUCKET_ATTACHMENTS", "attachments"),
    ("PWNFORGE_S3_BUCKET_AVATARS", "avatars"),
];

const UNSET: &[&str] = &[
    "SETUP_TOKEN",
    "DATABASE_MAX_CONNECTIONS",
    "PWNFORGE_S3_ENDPOINT",
    "PWNFORGE_S3_KEY_PREFIX",
    "PWNFORGE_S3_PUBLIC_BASE_URL",
];

fn turnstile_key_for(value: &str) -> Option<String> {
    // SAFETY: the only test in this binary, so no other thread reads or
    // writes the environment while it changes.
    unsafe { std::env::set_var("TURNSTILE_SECRET_KEY", value) };

    Config::from_env()
        .expect("the test configuration must be valid")
        .turnstile_secret_key
}

#[test]
fn a_blank_turnstile_secret_is_treated_as_unset() {
    for (key, value) in VALID {
        // SAFETY: see turnstile_key_for.
        unsafe { std::env::set_var(key, value) };
    }
    for key in UNSET {
        // SAFETY: see turnstile_key_for.
        unsafe { std::env::remove_var(key) };
    }

    assert_eq!(turnstile_key_for(""), None, "an empty value is no key");
    assert_eq!(turnstile_key_for("   "), None, "nor is whitespace");
    assert_eq!(
        turnstile_key_for("1x0000000000000000000000000000000AA"),
        Some("1x0000000000000000000000000000000AA".to_owned()),
        "a real value is kept"
    );
}

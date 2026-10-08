//! The two Turnstile keys read through `Config::from_env`: a blank value is no
//! key at all, and the instance refuses to start with only one of them.
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

/// The Turnstile keys `from_env` reads, after setting both.
fn turnstile_keys(site: &str, secret: &str) -> anyhow::Result<(Option<String>, Option<String>)> {
    // SAFETY: the only test in this binary, so no other thread reads or
    // writes the environment while it changes.
    unsafe {
        std::env::set_var("TURNSTILE_SITE_KEY", site);
        std::env::set_var("TURNSTILE_SECRET_KEY", secret);
    }

    Config::from_env().map(|config| (config.turnstile_site_key, config.turnstile_secret_key))
}

#[test]
fn turnstile_keys_are_read_together_or_not_at_all() {
    for (key, value) in VALID {
        // SAFETY: see turnstile_keys.
        unsafe { std::env::set_var(key, value) };
    }
    for key in UNSET {
        // SAFETY: see turnstile_keys.
        unsafe { std::env::remove_var(key) };
    }

    assert_eq!(
        turnstile_keys("", "").expect("both empty must boot"),
        (None, None),
        "empty values are no keys: Turnstile is off"
    );
    assert_eq!(
        turnstile_keys("  ", "   ").expect("both blank must boot"),
        (None, None),
        "nor is whitespace"
    );
    assert_eq!(
        turnstile_keys(
            "1x00000000000000000000AA",
            "1x0000000000000000000000000000000AA"
        )
        .expect("both set must boot"),
        (
            Some("1x00000000000000000000AA".to_owned()),
            Some("1x0000000000000000000000000000000AA".to_owned())
        ),
        "real values are kept"
    );

    for (site, secret, which) in [
        ("1x00000000000000000000AA", "", "a site key alone"),
        (
            "",
            "1x0000000000000000000000000000000AA",
            "a secret key alone",
        ),
    ] {
        let error = turnstile_keys(site, secret)
            .expect_err(&format!("{which} must refuse to boot"))
            .to_string();
        assert!(
            error.contains("TURNSTILE_SITE_KEY") && error.contains("TURNSTILE_SECRET_KEY"),
            "the message for {which} must name both variables, got: {error}"
        );
    }
}

// crates/api/src/config.rs
use anyhow::{Context, Result};
use std::fmt;

/// Where outgoing mail goes.
///
/// `Console` logs the rendered message instead of sending it, so local
/// development never touches a provider. `Smtp` covers self-hosted instances,
/// which may point at any relay. `Resend` is used by the hosted service.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EmailBackend {
    Console,
    Smtp,
    Resend,
}

impl std::str::FromStr for EmailBackend {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "console" => Ok(EmailBackend::Console),
            "smtp" => Ok(EmailBackend::Smtp),
            "resend" => Ok(EmailBackend::Resend),
            other => anyhow::bail!("invalid email backend: {other}"),
        }
    }
}

/// How the SMTP connection is secured. Implicit TLS is the norm on port 465,
/// STARTTLS on 587. `None` only makes sense for a relay on the same host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SmtpTls {
    Implicit,
    StartTls,
    None,
}

impl std::str::FromStr for SmtpTls {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "implicit" => Ok(SmtpTls::Implicit),
            "starttls" => Ok(SmtpTls::StartTls),
            "none" => Ok(SmtpTls::None),
            other => anyhow::bail!(
                "invalid SMTP TLS mode: {other} (expected `implicit`, `starttls` or `none`)"
            ),
        }
    }
}

#[derive(Clone)]
pub struct Config {
    // --- Server ---
    pub port: u16,
    /// Base URL the instance is reached at, used to build links in emails.
    pub public_url: String,
    pub cors_origin: String,
    pub setup_token: Option<String>,

    // --- Datastores ---
    pub database_url: String,
    /// Upper bound on the SeaORM pool. Also the reference the API's
    /// concurrency limit is derived from, so it is a real capacity knob and
    /// not just a database detail.
    pub database_max_connections: u32,
    pub redis_url: String,

    // --- Secrets ---
    pub jwt_secret: String,
    #[allow(dead_code)] // consumed by flag encryption (Phase 4)
    pub encryption_key: String,
    pub turnstile_secret_key: Option<String>,

    // --- Security ---
    /// Set the Secure flag on cookies. Must be false on plain HTTP in local
    /// development, or browsers silently drop them.
    pub cookie_secure: bool,
    /// Number of trusted reverse proxies in front of the backend. Zero means
    /// X-Forwarded-For is ignored entirely.
    pub trusted_proxy_count: usize,
    pub block_disposable_emails: bool,
    pub check_pwned_passwords: bool,

    // --- Email ---
    pub email_backend: EmailBackend,
    pub email_from: String,
    pub support_email: Option<String>,

    // --- Email: SMTP ---
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_username: String,
    pub smtp_password: String,
    pub smtp_tls: SmtpTls,

    // --- Email: Resend ---
    pub resend_api_key: String,
}

/// Hand written so secrets never reach the logs: a single `debug!(?config)`
/// would otherwise leak every credential the instance holds.
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("port", &self.port)
            .field("public_url", &self.public_url)
            .field("cors_origin", &self.cors_origin)
            .field("setup_token", &"[redacted]")
            .field("database_url", &"[redacted]")
            .field("database_max_connections", &self.database_max_connections)
            .field("redis_url", &"[redacted]")
            .field("jwt_secret", &"[redacted]")
            .field("encryption_key", &"[redacted]")
            .field("cookie_secure", &self.cookie_secure)
            .field("trusted_proxy_count", &self.trusted_proxy_count)
            .field("block_disposable_emails", &self.block_disposable_emails)
            .field("email_backend", &self.email_backend)
            .field("email_from", &self.email_from)
            .field("smtp_host", &self.smtp_host)
            .field("smtp_port", &self.smtp_port)
            .field("smtp_username", &self.smtp_username)
            .field("smtp_password", &"[redacted]")
            .field("smtp_tls", &self.smtp_tls)
            .field("resend_api_key", &"[redacted]")
            .finish()
    }
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let config = Config {
            // --- Server ---
            port: parse_var("PORT", "3000")?,
            public_url: required("PUBLIC_URL")?,
            cors_origin: required("CORS_ORIGIN")?,
            setup_token: std::env::var("SETUP_TOKEN").ok(),

            // --- Datastores ---
            database_url: required("DATABASE_URL")?,
            database_max_connections: parse_var("DATABASE_MAX_CONNECTIONS", "20")?,
            redis_url: required("REDIS_URL")?,

            // --- Secrets ---
            jwt_secret: required("JWT_SECRET")?,
            turnstile_secret_key: std::env::var("TURNSTILE_SECRET_KEY").ok(),
            encryption_key: required("ENCRYPTION_KEY")?,

            // --- Security ---
            cookie_secure: parse_var("COOKIE_SECURE", "true")?,
            trusted_proxy_count: parse_var("TRUSTED_PROXY_COUNT", "0")?,
            block_disposable_emails: parse_var("BLOCK_DISPOSABLE_EMAILS", "true")?,
            check_pwned_passwords: parse_var("CHECK_PWNED_PASSWORDS", "true")?,
            // --- Email ---
            email_backend: parse_var("EMAIL_BACKEND", "console")?,
            email_from: optional("EMAIL_FROM"),
            support_email: std::env::var("SUPPORT_EMAIL").ok(),

            // --- Email: SMTP ---
            smtp_host: optional("SMTP_HOST"),
            smtp_port: parse_var("SMTP_PORT", "587")?,
            smtp_username: optional("SMTP_USERNAME"),
            smtp_password: optional("SMTP_PASSWORD"),
            smtp_tls: parse_var("SMTP_TLS", "starttls")?,

            // --- Email: Resend ---
            resend_api_key: optional("RESEND_API_KEY"),
        };

        config.validate()?;

        Ok(config)
    }

    /// Cross-field checks that cannot be expressed field by field. Failing at
    /// boot is deliberate: an instance that silently sends no mail is worse
    /// than one that refuses to start.
    fn validate(&self) -> Result<()> {
        match self.email_backend {
            EmailBackend::Console => {}

            EmailBackend::Smtp => {
                if self.smtp_host.is_empty() {
                    anyhow::bail!("SMTP_HOST must be set when EMAIL_BACKEND is 'smtp'");
                }
                // Credentials stay optional: a local relay often accepts
                // unauthenticated mail from the host it runs on.
            }

            EmailBackend::Resend => {
                if self.resend_api_key.is_empty() {
                    anyhow::bail!("RESEND_API_KEY must be set when EMAIL_BACKEND is 'resend'");
                }
            }
        }

        if self.email_backend != EmailBackend::Console && self.email_from.is_empty() {
            anyhow::bail!("EMAIL_FROM must be set unless EMAIL_BACKEND is 'console'");
        }

        validate_cors_origin(&self.cors_origin)?;

        // A zero-sized pool deadlocks on the first query instead of failing at
        // boot, and the API derives its concurrency limit from this, so a zero
        // would also wedge the router.
        if self.database_max_connections == 0 {
            anyhow::bail!("DATABASE_MAX_CONNECTIONS must be at least 1");
        }

        // A short token weakens the only thing standing between a fresh instance and
        // whoever reaches it first.
        if let Some(token) = &self.setup_token
            && token.len() < 32
        {
            anyhow::bail!("SETUP_TOKEN must be at least 32 characters");
        }

        // Same reasoning as SETUP_TOKEN: a short signing key is the weakest point
        // of every session the instance will ever issue.
        if self.jwt_secret.len() < 32 {
            anyhow::bail!("JWT_SECRET must be at least 32 characters");
        }

        // Flag encryption (Phase 4) derives an AES-256-GCM key from this, which
        // requires exactly 32 bytes. Checked at boot rather than at first use, so a
        // malformed key fails on startup instead of on a user's first flag reveal.
        if self.encryption_key.len() < 32 {
            anyhow::bail!("ENCRYPTION_KEY must be at least 32 characters");
        }

        Ok(())
    }
}

/// Checks that `CORS_ORIGIN` is something a browser could actually match.
///
/// An origin is a scheme, a host and an optional port, and nothing more: no
/// path, no trailing slash, no query. A browser compares the `Origin` header
/// against this byte for byte, so `https://app.example.test/` matches nothing
/// and quietly breaks every cross-origin request. Caught here it is one line in
/// the startup log instead of a support thread.
///
/// Rejecting anything outside visible ASCII is also what lets the CORS layer
/// parse this into a `HeaderValue` without a fallible path of its own: that
/// character set is exactly what a header value allows. The check lives here
/// rather than in the router so a bad value produces a readable startup error,
/// the way every other rule in `validate` does, rather than a panic and a
/// backtrace.
fn validate_cors_origin(raw: &str) -> Result<()> {
    if raw.is_empty() {
        anyhow::bail!("CORS_ORIGIN must be set to the origin the frontend is served from");
    }

    if raw.chars().any(|c| !c.is_ascii_graphic()) {
        anyhow::bail!(
            "CORS_ORIGIN must contain only printable ASCII, with no spaces or control characters (got {raw:?})"
        );
    }

    let Some((scheme, rest)) = raw.split_once("://") else {
        anyhow::bail!(
            "CORS_ORIGIN must include a scheme, as in https://ctf.example.com (got {raw:?})"
        );
    };

    if scheme != "http" && scheme != "https" {
        anyhow::bail!("CORS_ORIGIN scheme must be http or https (got {scheme:?})");
    }

    if rest.is_empty() {
        anyhow::bail!("CORS_ORIGIN must name a host after the scheme (got {raw:?})");
    }

    // Covers a trailing slash and a path in one check: both leave a `/` behind,
    // and a trailing slash is the mistake people actually make.
    if rest.contains('/') {
        anyhow::bail!(
            "CORS_ORIGIN must be a bare origin, with no path or trailing slash (got {raw:?})"
        );
    }

    if rest.contains('?') || rest.contains('#') {
        anyhow::bail!("CORS_ORIGIN must not carry a query or a fragment (got {raw:?})");
    }

    Ok(())
}

fn required(key: &str) -> Result<String> {
    std::env::var(key).with_context(|| format!("{key} must be set"))
}

fn optional(key: &str) -> String {
    std::env::var(key).unwrap_or_default()
}

fn parse_var<T>(key: &str, default: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let raw = std::env::var(key).unwrap_or_else(|_| default.to_owned());
    raw.parse::<T>()
        .map_err(|err| anyhow::anyhow!("{key} is invalid: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Positive: the shapes an operator legitimately configures, from local
    /// development to a host on a non-standard port.
    #[test]
    fn a_bare_origin_is_accepted() {
        for origin in [
            "http://localhost:5173",
            "https://ctf.example.test",
            "https://ctf.example.test:8443",
            "http://127.0.0.1:3000",
        ] {
            assert!(
                validate_cors_origin(origin).is_ok(),
                "{origin} must be accepted"
            );
        }
    }

    /// Negative: the trailing slash, which is the mistake that actually
    /// happens. A browser matches the `Origin` header byte for byte, so this
    /// would match nothing and break every cross-origin request without
    /// anything in the logs pointing at the cause.
    #[test]
    fn a_trailing_slash_is_refused() {
        let err = validate_cors_origin("https://ctf.example.test/")
            .expect_err("a trailing slash must be refused");

        assert!(
            err.to_string().contains("trailing slash"),
            "the message must name the problem, got {err}"
        );
    }

    /// Negative: everything else that is not an origin.
    #[test]
    fn a_malformed_origin_is_refused() {
        for origin in [
            "",                              // unset
            "ctf.example.test",              // no scheme
            "ftp://ctf.example.test",        // wrong scheme
            "https://",                      // no host
            "https://ctf.example.test/app",  // path
            "https://ctf.example.test?a=1",  // query
            "https://ctf.example.test#frag", // fragment
            "https://ctf.example.test ",     // trailing space
            "https://ctf example.test",      // inner space
            "https://ctf.example.test\n",    // control character
        ] {
            assert!(
                validate_cors_origin(origin).is_err(),
                "{origin:?} must be refused"
            );
        }
    }

    /// Whatever passes here is safe to turn into a `HeaderValue` in the router,
    /// which is what lets that conversion stay infallible in practice. Pinned
    /// as a property rather than trusted: the router's `expect` is only
    /// unreachable for as long as this holds.
    #[test]
    fn an_accepted_origin_is_a_usable_header_value() {
        for origin in ["http://localhost:5173", "https://ctf.example.test:8443"] {
            assert!(validate_cors_origin(origin).is_ok());
            assert!(
                origin.bytes().all(|b| (0x21..=0x7e).contains(&b)),
                "{origin} must be within the byte range a header value allows"
            );
        }
    }
}

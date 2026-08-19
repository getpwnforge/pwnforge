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
            other => anyhow::bail!("invalid SMTP TLS mode: {other}"),
        }
    }
}

#[derive(Clone)]
pub struct Config {
    // --- Server ---
    pub port: u16,
    /// Base URL the instance is reached at, used to build links in emails.
    pub public_url: String,
    #[allow(dead_code)] // consumed by the CORS layer (1.x)
    pub cors_origin: String,
    pub setup_token: Option<String>,

    // --- Datastores ---
    pub database_url: String,
    pub redis_url: String,

    // --- Secrets ---
    pub jwt_secret: String,
    #[allow(dead_code)] // consumed by flag encryption (Phase 4)
    pub encryption_key: String,

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
            redis_url: required("REDIS_URL")?,

            // --- Secrets ---
            jwt_secret: required("JWT_SECRET")?,
            encryption_key: required("ENCRYPTION_KEY")?,

            // --- Security ---
            cookie_secure: parse_var("COOKIE_SECURE", "true")?,
            trusted_proxy_count: parse_var("TRUSTED_PROXY_COUNT", "0")?,
            block_disposable_emails: parse_var("BLOCK_DISPOSABLE_EMAILS", "true")?,
            check_pwned_passwords: parse_var("CHECK_PWNED_PASSWORDS", "true")?,
            // --- Email ---
            email_backend: parse_var("EMAIL_BACKEND", "console")?,
            email_from: optional("EMAIL_FROM"),

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

        // A short token weakens the only thing standing between a fresh instance and
        // whoever reaches it first.
        if let Some(token) = &self.setup_token
            && token.len() < 32
        {
            anyhow::bail!("SETUP_TOKEN must be at least 32 characters");
        }

        Ok(())
    }
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

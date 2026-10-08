// crates/services/src/config.rs
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

/// Region assumed when `PWNFORGE_S3_REGION` is unset.
///
/// `garage`, not `us-east-1`: the bundled object store is Garage, which signs
/// SigV4 against the region named in its own `garage.toml`, and the bundled
/// store is the only configuration that ever reaches this default. Every
/// other provider sets the variable explicitly, from the presets in
/// `.env.example`. Pinned against the example config by
/// `the_default_region_matches_the_bundled_store_config`.
const DEFAULT_S3_REGION: &str = "garage";

/// Object storage settings.
///
/// There is no `PWNFORGE_STORAGE_BACKEND`: S3 is the only runtime backend and
/// no filesystem mode exists, so a "which backend" switch would have exactly
/// one legal value and would only invite someone to add a second one.
///
/// Nothing here is read from the object store's own admin credentials. Those
/// provision the buckets and then belong to the operator, not to this process;
/// the backend only ever holds the scoped application key. That separation is
/// the whole point of `ops/garage/bootstrap.py`, and reading the admin token
/// here would quietly undo it.
#[derive(Clone)]
pub struct StorageConfig {
    /// Empty for AWS S3, where the endpoint is derived from the region.
    /// Set for Garage, R2, Scaleway and anything else S3-compatible.
    pub endpoint: Option<String>,
    pub region: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    /// Path style (`host/bucket/key`) is the default because that is what the
    /// bundled Garage serves. AWS and Scaleway want virtual-hosted style
    /// (`bucket.host/key`) instead, so this has to be reachable per provider.
    pub force_path_style: bool,
    pub bucket_attachments: String,
    pub bucket_avatars: String,
    /// Prefixes every key. Lets one bucket be shared with another tenant or
    /// another application, which is common on managed S3 where buckets are
    /// billed or quota'd.
    pub key_prefix: Option<String>,
    /// Public CDN origin for avatars. Avatars only: see
    /// `storage_service::avatar_public_url`, which takes no bucket parameter
    /// precisely so that an attachment cannot be handed a public URL.
    pub public_base_url: Option<String>,
}

/// Same reasoning as `Config`'s: the secret access key must never reach a log
/// line, and a derived `Debug` would print it in full.
impl fmt::Debug for StorageConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StorageConfig")
            .field("endpoint", &self.endpoint)
            .field("region", &self.region)
            .field("access_key_id", &"[redacted]")
            .field("secret_access_key", &"[redacted]")
            .field("force_path_style", &self.force_path_style)
            .field("bucket_attachments", &self.bucket_attachments)
            .field("bucket_avatars", &self.bucket_avatars)
            .field("key_prefix", &self.key_prefix)
            .field("public_base_url", &self.public_base_url)
            .finish()
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

    // --- Object storage ---
    pub storage: StorageConfig,
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
            .field("storage", &self.storage)
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
            // Blank counts as unset, like the S3 settings: a compose file that
            // passes an empty value would otherwise enable verification with an
            // empty secret, and every route behind Turnstile would fail.
            turnstile_secret_key: non_empty("TURNSTILE_SECRET_KEY"),
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

            // --- Object storage ---
            storage: StorageConfig::from_env()?,
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

impl StorageConfig {
    /// Fail-fast like the rest of `Config::from_env`: an instance that boots
    /// with unusable storage settings would only discover it on a user's first
    /// upload, which is the worst possible place to find out.
    fn from_env() -> Result<Self> {
        let config = StorageConfig {
            endpoint: non_empty("PWNFORGE_S3_ENDPOINT"),
            region: parse_var("PWNFORGE_S3_REGION", DEFAULT_S3_REGION)?,
            access_key_id: required("PWNFORGE_S3_ACCESS_KEY_ID")?,
            secret_access_key: required("PWNFORGE_S3_SECRET_ACCESS_KEY")?,
            force_path_style: parse_var("PWNFORGE_S3_FORCE_PATH_STYLE", "true")?,
            bucket_attachments: required("PWNFORGE_S3_BUCKET_ATTACHMENTS")?,
            bucket_avatars: required("PWNFORGE_S3_BUCKET_AVATARS")?,
            key_prefix: non_empty("PWNFORGE_S3_KEY_PREFIX"),
            public_base_url: non_empty("PWNFORGE_S3_PUBLIC_BASE_URL"),
        };

        config.validate()?;

        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        if self.region.trim().is_empty() {
            anyhow::bail!("PWNFORGE_S3_REGION must not be empty");
        }

        // Credentials are `required`, so they exist, but an operator who
        // exported them as empty strings gets the same anonymous-request
        // failure far downstream. Name it here instead.
        if self.access_key_id.trim().is_empty() {
            anyhow::bail!("PWNFORGE_S3_ACCESS_KEY_ID must not be empty");
        }
        if self.secret_access_key.trim().is_empty() {
            anyhow::bail!("PWNFORGE_S3_SECRET_ACCESS_KEY must not be empty");
        }

        validate_bucket_name("PWNFORGE_S3_BUCKET_ATTACHMENTS", &self.bucket_attachments)?;
        validate_bucket_name("PWNFORGE_S3_BUCKET_AVATARS", &self.bucket_avatars)?;

        // Pointing both names at one bucket is legal S3 and would mostly work,
        // which is exactly why it is worth refusing: the two buckets carry
        // different access regimes (avatars are re-encoded and may be served
        // from a CDN, attachments are never public), and collapsing them makes
        // `Bucket::Avatars` and `Bucket::Attachments` resolve to the same
        // place while the type system still claims they are separate.
        if self.bucket_attachments == self.bucket_avatars {
            anyhow::bail!(
                "PWNFORGE_S3_BUCKET_ATTACHMENTS and PWNFORGE_S3_BUCKET_AVATARS must name different buckets (both are {:?})",
                self.bucket_attachments
            );
        }

        if let Some(endpoint) = &self.endpoint {
            validate_http_url("PWNFORGE_S3_ENDPOINT", endpoint)?;
        }

        if let Some(base) = &self.public_base_url {
            validate_http_url("PWNFORGE_S3_PUBLIC_BASE_URL", base)?;
        }

        if let Some(prefix) = &self.key_prefix {
            validate_key_prefix(prefix)?;
        }

        Ok(())
    }

    /// The prefix as it is joined onto a key: no leading slash, exactly one
    /// trailing slash, or empty when unset. Normalising here rather than at
    /// every call site is what keeps `a/b` and `a/b/` from producing two
    /// different keyspaces for the same configured value.
    pub fn normalized_key_prefix(&self) -> String {
        match &self.key_prefix {
            Some(prefix) => format!("{}/", prefix.trim_matches('/')),
            None => String::new(),
        }
    }
}

/// Bucket names are concatenated into a request path in path-style mode, so a
/// name carrying a `/` would silently move part of itself into the key. The
/// rules below are S3's own naming restrictions, enforced here because a
/// provider's rejection of a bad name arrives as an opaque signing or 400
/// error at first use rather than as a sentence naming the variable.
fn validate_bucket_name(key: &str, name: &str) -> Result<()> {
    if !(3..=63).contains(&name.len()) {
        anyhow::bail!("{key} must be between 3 and 63 characters (got {name:?})");
    }

    if !name
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.')
    {
        anyhow::bail!(
            "{key} must contain only lowercase letters, digits, '-' and '.' (got {name:?})"
        );
    }

    let first = name.as_bytes()[0];
    let last = name.as_bytes()[name.len() - 1];
    if !(first.is_ascii_lowercase() || first.is_ascii_digit())
        || !(last.is_ascii_lowercase() || last.is_ascii_digit())
    {
        anyhow::bail!("{key} must start and end with a letter or a digit (got {name:?})");
    }

    Ok(())
}

/// Shared by `PWNFORGE_S3_ENDPOINT` and `PWNFORGE_S3_PUBLIC_BASE_URL`: both are
/// origins the process will concatenate onto, so both have to be an origin and
/// nothing more.
fn validate_http_url(key: &str, raw: &str) -> Result<()> {
    if raw.chars().any(|c| !c.is_ascii_graphic()) {
        anyhow::bail!("{key} must contain only printable ASCII, with no spaces (got {raw:?})");
    }

    let Some((scheme, rest)) = raw.split_once("://") else {
        anyhow::bail!("{key} must include a scheme, as in https://s3.example.com (got {raw:?})");
    };

    if scheme != "http" && scheme != "https" {
        anyhow::bail!("{key} scheme must be http or https (got {scheme:?})");
    }

    if rest.is_empty() {
        anyhow::bail!("{key} must name a host after the scheme (got {raw:?})");
    }

    // A trailing slash is the mistake people make, and it is the one that
    // produces `https://cdn.example.com//avatars/...` - which some CDNs treat
    // as a distinct path and others normalise, so it breaks inconsistently.
    if rest.ends_with('/') {
        anyhow::bail!("{key} must not end with a slash (got {raw:?})");
    }

    Ok(())
}

/// The prefix is attacker-adjacent only in the sense that an operator typing
/// `../` into it would quietly reach outside the intended keyspace. S3 keys
/// have no path semantics, so `..` is not traversal the way it is on a
/// filesystem, but it still produces keys nothing else in the system can name,
/// which is its own kind of unreachable data.
fn validate_key_prefix(prefix: &str) -> Result<()> {
    if prefix.chars().any(|c| c.is_control() || c.is_whitespace()) {
        anyhow::bail!(
            "PWNFORGE_S3_KEY_PREFIX must not contain whitespace or control characters (got {prefix:?})"
        );
    }

    if prefix.split('/').any(|segment| segment == "..") {
        anyhow::bail!("PWNFORGE_S3_KEY_PREFIX must not contain a '..' segment (got {prefix:?})");
    }

    if prefix.trim_matches('/').is_empty() {
        anyhow::bail!("PWNFORGE_S3_KEY_PREFIX must name at least one segment (got {prefix:?})");
    }

    Ok(())
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

/// `None` for both an unset variable and one set to the empty string.
///
/// The distinction matters for the storage settings specifically: Compose
/// substitutes an unset `${PWNFORGE_S3_ENDPOINT}` into an empty string rather
/// than omitting the variable, so `std::env::var().ok()` would hand back
/// `Some("")` and the AWS client would try to reach an endpoint with no host.
/// Empty means "not configured" here, which is also how `.env.example`
/// documents the AWS preset.
fn non_empty(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
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

    /// The default region and the bundled store's configured region must be
    /// the same string, checked against the actual file rather than a copy.
    ///
    /// Garage signs SigV4 against the region named in its own `garage.toml`.
    /// If these two drift apart, every request fails with a signature
    /// mismatch, which names neither the region nor the file. Nothing else in
    /// the system would catch it, because both values are individually valid.
    #[test]
    fn the_default_region_matches_the_bundled_store_config() {
        const EXAMPLE: &str = include_str!("../../../../ops/garage/garage.toml.example");

        let configured = EXAMPLE
            .lines()
            .find_map(|line| {
                let line = line.trim();
                let rest = line.strip_prefix("s3_region")?.trim_start();
                let rest = rest.strip_prefix('=')?.trim();
                rest.trim_matches('"').to_owned().into()
            })
            .expect("garage.toml.example must set s3_region");

        assert_eq!(
            configured, DEFAULT_S3_REGION,
            "ops/garage/garage.toml.example sets s3_region={configured:?} but the \
             PWNFORGE_S3_REGION default is {DEFAULT_S3_REGION:?}; a mismatch fails \
             every request with an opaque signature error"
        );
    }

    /// Positive: the bucket names people actually use, across providers.
    #[test]
    fn a_valid_bucket_name_is_accepted() {
        for name in [
            "pwnforge-attachments",
            "pwnforge-avatars",
            "ctf",
            "my.bucket.name",
            "bucket123",
            "a-very-long-but-still-legal-bucket-name-under-sixty-three-ch",
        ] {
            assert!(
                validate_bucket_name("PWNFORGE_S3_BUCKET_AVATARS", name).is_ok(),
                "{name} must be accepted"
            );
        }
    }

    /// Negative: the one that matters is the slash.
    ///
    /// In path-style addressing the bucket name is concatenated into the
    /// request path, so `evil/../../other` would move part of itself into the
    /// key and address a bucket the configuration does not name. The rest are
    /// S3's own naming rules, caught here so the failure is a sentence naming
    /// the variable rather than an opaque signing error at first use.
    #[test]
    fn a_malformed_bucket_name_is_refused() {
        for name in [
            "with/slash",     // escapes into the key namespace
            "a/b",            // same, shorter
            "..",             // too short and meaningless
            "UPPERCASE",      // S3 refuses uppercase
            "under_score",    // not a legal character
            "-leading-dash",  // must start alphanumeric
            "trailing-dash-", // must end alphanumeric
            ".leading-dot",   //
            "ab",             // under three characters
            "",               // unset
            "has space",      //
            "back\\slash",    //
            "a-very-long-and-definitely-illegal-bucket-name-that-runs-past-sixty-three",
        ] {
            assert!(
                validate_bucket_name("PWNFORGE_S3_BUCKET_AVATARS", name).is_err(),
                "{name:?} must be refused"
            );
        }
    }

    /// Positive: the endpoint shapes from the four documented presets.
    #[test]
    fn a_valid_endpoint_is_accepted() {
        for url in [
            "http://garage:3900",
            "https://s3.fr-par.scw.cloud",
            "https://abc123.r2.cloudflarestorage.com",
            "http://127.0.0.1:9000",
            "https://cdn.example.test",
        ] {
            assert!(
                validate_http_url("PWNFORGE_S3_ENDPOINT", url).is_ok(),
                "{url} must be accepted"
            );
        }
    }

    /// Negative: chiefly the trailing slash, which produces a double slash
    /// once something is concatenated onto it - normalised by some CDNs,
    /// treated as a distinct path by others, so it breaks inconsistently.
    #[test]
    fn a_malformed_endpoint_is_refused() {
        for url in [
            "http://garage:3900/",       // trailing slash
            "https://cdn.example.test/", // same
            "garage:3900",               // no scheme
            "ftp://garage:3900",         // wrong scheme
            "https://",                  // no host
            "http://garage :3900",       // inner space
            "http://garage:3900\n",      // control character
        ] {
            assert!(
                validate_http_url("PWNFORGE_S3_ENDPOINT", url).is_err(),
                "{url:?} must be refused"
            );
        }
    }

    /// Negative: a prefix that would address keys nothing else in the system
    /// can name. S3 keys have no path semantics, so `..` is not traversal the
    /// way it is on a filesystem, but the objects it writes are unreachable
    /// by every other code path, which is its own kind of lost data.
    #[test]
    fn a_malformed_key_prefix_is_refused() {
        for prefix in [
            "..",
            "a/../b",
            "../escape",
            "with space",
            "tab\there",
            "/",
            "//",
        ] {
            assert!(
                validate_key_prefix(prefix).is_err(),
                "{prefix:?} must be refused"
            );
        }
    }

    #[test]
    fn a_valid_key_prefix_is_accepted() {
        for prefix in ["tenant-1", "a/b", "/tenant-1/", "deep/nested/prefix"] {
            assert!(
                validate_key_prefix(prefix).is_ok(),
                "{prefix:?} must be accepted"
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

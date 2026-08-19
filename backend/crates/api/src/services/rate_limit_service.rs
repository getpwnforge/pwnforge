// crates/api/src/services/rate_limit_service.rs
use crate::services::token_service::hash_opaque_token;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use std::net::IpAddr;
use uuid::Uuid;

// --- Login ---
const LOGIN_IDENTIFIER_LIMIT: isize = 10;
const LOGIN_IP_LIMIT: isize = 30;
const LOGIN_WINDOW_SECS: i64 = 15 * 60;

// --- Password reset request ---
// Lower and longer than login: every request that gets through sends a mail to
// an address the caller does not have to prove they own.
const FORGOT_EMAIL_LIMIT: isize = 3;
const FORGOT_IP_LIMIT: isize = 10;
const FORGOT_WINDOW_SECS: i64 = 60 * 60;

// --- Password reset submission ---
// Only bounds the Argon2 cost an unauthenticated caller can trigger: the token
// itself is 256 bits, so this is not what stops a brute force.
const RESET_IP_LIMIT: isize = 20;
const RESET_WINDOW_SECS: i64 = 60 * 60;

// --- Verification email resend ---
// Two windows on the same key: a short one so the button cannot be hammered,
// a long one so the daily volume stays bounded.
const RESEND_BURST_LIMIT: isize = 1;
const RESEND_BURST_WINDOW_SECS: i64 = 60;
const RESEND_HOURLY_LIMIT: isize = 5;
const RESEND_HOURLY_WINDOW_SECS: i64 = 60 * 60;

// --- Setup ---
// The setup wizard is a single endpoint that creates the first administrator and instance settings.
const SETUP_IP_LIMIT: isize = 5;
const SETUP_WINDOW_SECS: i64 = 60 * 60;
const SETUP_TEST_EMAIL_IP_LIMIT: isize = 5;
const SETUP_TEST_EMAIL_WINDOW_SECS: i64 = 60 * 60;

pub enum RateLimitDecision {
    Allowed,
    Limited { retry_after_secs: u64 },
}

/// Login rate limit, checked on two independent fixed windows: one for the
/// submitted identifier (username or email, lowercased so a case-varying
/// attacker cannot spread attempts across buckets), one per IP. Either window
/// tripping blocks the request; both are always incremented so the counts stay
/// accurate regardless of which one is checked first.
///
/// The identifier window keys on the submitted string, not on the resolved
/// account: someone who knows both the username and the email of a target gets
/// two separate quotas. Resolving the account first would close that gap but
/// would mean a database round-trip on every attempt, including the ones we
/// want to reject cheaply. The IP window covers the remainder.
pub async fn check_login_attempt(
    redis: &mut ConnectionManager,
    identifier: &str,
    ip: IpAddr,
) -> Result<RateLimitDecision, redis::RedisError> {
    let by_identifier = check_window(
        redis,
        &login_identifier_key(identifier),
        LOGIN_IDENTIFIER_LIMIT,
        LOGIN_WINDOW_SECS,
    )
    .await?;

    let by_ip = check_window(redis, &login_ip_key(ip), LOGIN_IP_LIMIT, LOGIN_WINDOW_SECS).await?;

    Ok(most_restrictive(by_identifier, by_ip))
}

/// Clears the per-identifier window after a successful login.
///
/// Only the identifier bucket is cleared, not the per-IP one: resetting the IP
/// bucket on any success would let an attacker who owns one valid account buy a
/// fresh IP-wide budget by logging into it, defeating the point of that window,
/// which is to catch a single IP spraying attempts across many identifiers.
pub async fn reset_login_attempts(
    redis: &mut ConnectionManager,
    identifier: &str,
) -> Result<(), redis::RedisError> {
    let _: () = redis.del(login_identifier_key(identifier)).await?;
    Ok(())
}

/// Rate limit for password reset requests.
///
/// The caller must treat a Redis failure as a refusal rather than swallowing
/// it: unlike login, there is no password check underneath this endpoint, so
/// failing open turns it into a free mailer aimed at arbitrary addresses.
pub async fn check_password_forgot(
    redis: &mut ConnectionManager,
    email: &str,
    ip: IpAddr,
) -> Result<RateLimitDecision, redis::RedisError> {
    let by_email = check_window(
        redis,
        &forgot_email_key(email),
        FORGOT_EMAIL_LIMIT,
        FORGOT_WINDOW_SECS,
    )
    .await?;

    let by_ip = check_window(
        redis,
        &forgot_ip_key(ip),
        FORGOT_IP_LIMIT,
        FORGOT_WINDOW_SECS,
    )
    .await?;

    Ok(most_restrictive(by_email, by_ip))
}

pub async fn check_password_reset(
    redis: &mut ConnectionManager,
    ip: IpAddr,
) -> Result<RateLimitDecision, redis::RedisError> {
    check_window(redis, &reset_ip_key(ip), RESET_IP_LIMIT, RESET_WINDOW_SECS).await
}

pub async fn check_email_resend(
    redis: &mut ConnectionManager,
    user_id: Uuid,
) -> Result<RateLimitDecision, redis::RedisError> {
    let by_user = check_window(
        redis,
        &email_resend_key(user_id),
        RESEND_HOURLY_LIMIT,
        RESEND_HOURLY_WINDOW_SECS,
    )
    .await?;

    let by_user_min = check_window(
        redis,
        &email_resend_burst_key(user_id),
        RESEND_BURST_LIMIT,
        RESEND_BURST_WINDOW_SECS,
    )
    .await?;

    Ok(most_restrictive(by_user, by_user_min))
}

pub async fn check_setup_attempt(
    redis: &mut ConnectionManager,
    ip: IpAddr,
) -> Result<RateLimitDecision, redis::RedisError> {
    check_window(redis, &setup_ip_key(ip), SETUP_IP_LIMIT, SETUP_WINDOW_SECS).await
}

pub async fn check_setup_test_email(
    redis: &mut ConnectionManager,
    ip: IpAddr,
) -> Result<RateLimitDecision, redis::RedisError> {
    check_window(
        redis,
        &setup_test_email_ip_key(ip),
        SETUP_TEST_EMAIL_IP_LIMIT,
        SETUP_TEST_EMAIL_WINDOW_SECS,
    )
    .await
}

/// Returns the blocking decision when either window tripped, keeping the
/// longest wait so a client that retries at the returned delay is not refused
/// again by the other window.
fn most_restrictive(a: RateLimitDecision, b: RateLimitDecision) -> RateLimitDecision {
    use RateLimitDecision::*;

    match (a, b) {
        (
            Limited {
                retry_after_secs: x,
            },
            Limited {
                retry_after_secs: y,
            },
        ) => Limited {
            retry_after_secs: x.max(y),
        },
        (Limited { retry_after_secs }, _) | (_, Limited { retry_after_secs }) => {
            Limited { retry_after_secs }
        }
        _ => Allowed,
    }
}

/// Hashed rather than stored raw: the identifier is often an email address, and
/// Redis key names should not become another place PII sits at rest.
fn login_identifier_key(identifier: &str) -> String {
    format!(
        "ratelimit:login:identifier:{}",
        hash_opaque_token(&identifier.trim().to_lowercase())
    )
}

fn login_ip_key(ip: IpAddr) -> String {
    format!("ratelimit:login:ip:{ip}")
}

fn forgot_email_key(email: &str) -> String {
    format!(
        "ratelimit:forgot:email:{}",
        hash_opaque_token(&email.trim().to_lowercase())
    )
}

fn forgot_ip_key(ip: IpAddr) -> String {
    format!("ratelimit:forgot:ip:{ip}")
}

fn reset_ip_key(ip: IpAddr) -> String {
    format!("ratelimit:reset:ip:{ip}")
}

fn email_resend_key(user_id: Uuid) -> String {
    format!("ratelimit:resend:user:{user_id}")
}

fn email_resend_burst_key(user_id: Uuid) -> String {
    format!("ratelimit:resend:burst:user:{user_id}")
}

fn setup_ip_key(ip: IpAddr) -> String {
    format!("ratelimit:setup:ip:{ip}")
}

fn setup_test_email_ip_key(ip: IpAddr) -> String {
    format!("ratelimit:setup:test_email:ip:{ip}")
}

/// Increments a single fixed window and reports whether it is over `limit`.
///
/// A plain INCR followed by an EXPIRE set only on the first request of the
/// window. INCR is atomic, so the count itself is always correct under
/// concurrent requests; the one known gap is a crash between the INCR and the
/// EXPIRE call, which would leave that key without a TTL. That is an
/// acceptable, self-healing risk here and not worth a Lua script for.
async fn check_window(
    redis: &mut ConnectionManager,
    key: &str,
    limit: isize,
    window_secs: i64,
) -> Result<RateLimitDecision, redis::RedisError> {
    let count: isize = redis.incr(key, 1).await?;
    if count == 1 {
        let _: () = redis.expire(key, window_secs).await?;
    }

    if count > limit {
        let ttl: i64 = redis.ttl(key).await?;
        return Ok(RateLimitDecision::Limited {
            retry_after_secs: ttl.max(0) as u64,
        });
    }

    Ok(RateLimitDecision::Allowed)
}

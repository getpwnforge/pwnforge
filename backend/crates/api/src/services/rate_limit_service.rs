// crates/api/src/services/rate_limit_service.rs
use crate::services::token_service::hash_opaque_token;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use std::net::IpAddr;

const IDENTIFIER_LIMIT: isize = 10;
const IP_LIMIT: isize = 30;
const WINDOW_SECS: i64 = 15 * 60;

pub enum RateLimitDecision {
    Allowed,
    Limited { retry_after_secs: u64 },
}

/// Login rate limit, checked on two independent fixed windows: 10 attempts
/// per 15 minutes for the submitted login identifier (username or email,
/// lowercased so a case-varying attacker cannot spread attempts across
/// buckets), and 30 attempts per 15 minutes per IP. Either window tripping
/// blocks the request; both are always incremented so the counts stay
/// accurate regardless of which one is checked first.
///
/// The identifier window keys on the submitted string, not on the resolved
/// account: someone who knows both the username and the email of a target
/// gets two separate quotas. Resolving the account first would close that
/// gap but would mean a database round-trip on every attempt, including the
/// ones we want to reject cheaply. The IP window covers the remainder.
pub async fn check_login_attempt(
    redis: &mut ConnectionManager,
    identifier: &str,
    ip: IpAddr,
) -> Result<RateLimitDecision, redis::RedisError> {
    let by_identifier = check_window(redis, &identifier_key(identifier), IDENTIFIER_LIMIT).await?;
    let by_ip = check_window(redis, &ip_key(ip), IP_LIMIT).await?;

    Ok(match (by_identifier, by_ip) {
        (
            RateLimitDecision::Limited {
                retry_after_secs: a,
            },
            RateLimitDecision::Limited {
                retry_after_secs: b,
            },
        ) => RateLimitDecision::Limited {
            retry_after_secs: a.max(b),
        },
        (RateLimitDecision::Limited { retry_after_secs }, _)
        | (_, RateLimitDecision::Limited { retry_after_secs }) => {
            RateLimitDecision::Limited { retry_after_secs }
        }
        _ => RateLimitDecision::Allowed,
    })
}

/// Clears the per-identifier window after a successful login.
///
/// Only the identifier bucket is cleared, not the per-IP bucket: resetting
/// the IP bucket on any success would let an attacker who owns one valid
/// account "buy" a fresh IP-wide budget by logging into it, defeating the
/// point of the IP limit, which is to catch a single IP spraying attempts
/// across many different identifiers.
pub async fn reset_login_attempts(
    redis: &mut ConnectionManager,
    identifier: &str,
) -> Result<(), redis::RedisError> {
    let _: () = redis.del(identifier_key(identifier)).await?;
    Ok(())
}

/// Hashed rather than stored raw: the identifier is often an email address,
/// and Redis key names should not become another place PII sits at rest.
fn identifier_key(identifier: &str) -> String {
    let normalized = identifier.trim().to_lowercase();
    format!(
        "ratelimit:login:identifier:{}",
        hash_opaque_token(&normalized)
    )
}

fn ip_key(ip: IpAddr) -> String {
    format!("ratelimit:login:ip:{ip}")
}

/// Increments a single fixed window and reports whether it is over `limit`.
///
/// A plain INCR followed by an EXPIRE set only on the first request of the
/// window. INCR is atomic, so the count itself is always correct under
/// concurrent requests; the one known gap is a crash between the INCR and
/// the EXPIRE call, which would leave that key without a TTL. That is an
/// acceptable, self-healing risk here (a Redis restart or a later
/// successful window clears it) and not worth a Lua script for.
async fn check_window(
    redis: &mut ConnectionManager,
    key: &str,
    limit: isize,
) -> Result<RateLimitDecision, redis::RedisError> {
    let count: isize = redis.incr(key, 1).await?;
    if count == 1 {
        let _: () = redis.expire(key, WINDOW_SECS).await?;
    }

    if count > limit {
        let ttl: i64 = redis.ttl(key).await?;
        return Ok(RateLimitDecision::Limited {
            retry_after_secs: ttl.max(0) as u64,
        });
    }

    Ok(RateLimitDecision::Allowed)
}

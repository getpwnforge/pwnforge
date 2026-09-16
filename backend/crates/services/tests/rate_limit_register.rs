//! Integration tests for the registration rate limit.
//!
//! These talk to a real Redis rather than a mock: `check_window` leans on
//! INCR/EXPIRE/TTL semantics, and a hand-rolled mock would assert our
//! assumptions about Redis instead of Redis itself. CI provides the instance
//! (`REDIS_URL` is set for `cargo test` in ci.yml); locally the dev compose
//! exposes one on the default port.

use std::net::{IpAddr, Ipv6Addr};
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use redis::aio::ConnectionManager;
use services::rate_limit_service::{self, RateLimitDecision};

/// Mirrors `REGISTER_IP_LIMIT` in rate_limit_service. Kept as a literal so a
/// change to the production limit fails these tests loudly rather than
/// silently re-deriving whatever the new value happens to be.
const REGISTER_IP_LIMIT: u32 = 5;

/// Mirrors `REGISTER_WINDOW_SECS`, same reasoning.
const REGISTER_WINDOW_SECS: u64 = 60 * 60;

async fn connect_redis() -> ConnectionManager {
    let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_owned());
    let client = redis::Client::open(url).expect("REDIS_URL must be a valid Redis URL");

    // The dev compose sets `requirepass`, so a bare `redis://host:port` is
    // refused with NOAUTH. CI's service has no password and works as-is.
    ConnectionManager::new(client).await.expect(
        "a Redis instance must be reachable; against the dev compose set \
         REDIS_URL=redis://:$REDIS_PASSWORD@127.0.0.1:6379",
    )
}

/// A fresh address per call, unique across runs as well as within one.
///
/// The limiter keys on the address and the window is an hour, so a counter
/// that restarts at zero every run would hand the second run of the day the
/// exact buckets the first one already filled, so the tests would pass once
/// and then fail for the next hour. Uniqueness has to outlive the process, so the
/// address mixes in the wall clock and the pid rather than just a counter.
///
/// 2001:db8::/32 is the RFC 3849 documentation prefix: reserved, never
/// routable, and it leaves 96 bits to fill, which is room enough that two runs
/// colliding is not a thing that happens.
fn unique_ip() -> IpAddr {
    static COUNTER: AtomicU16 = AtomicU16::new(0);
    let nth = COUNTER.fetch_add(1, Ordering::Relaxed);

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the system clock must be past the epoch")
        .as_nanos() as u64;

    let pid = std::process::id();

    IpAddr::V6(Ipv6Addr::new(
        0x2001,
        0x0db8,
        (pid >> 16) as u16,
        pid as u16,
        (nanos >> 48) as u16,
        (nanos >> 32) as u16,
        (nanos >> 16) as u16,
        nth,
    ))
}

/// Positive: a caller under the limit is let through every time.
#[tokio::test]
async fn register_under_the_limit_is_allowed() {
    let mut redis = connect_redis().await;
    let ip = unique_ip();

    for attempt in 1..=REGISTER_IP_LIMIT {
        let decision = rate_limit_service::check_register_attempt(&mut redis, ip)
            .await
            .expect("the limiter must answer while Redis is up");

        assert!(
            matches!(decision, RateLimitDecision::Allowed),
            "attempt {attempt} of {REGISTER_IP_LIMIT} must be allowed"
        );
    }
}

/// Negative: the attempt past the limit is refused, and carries a retry delay
/// the caller can actually wait on.
#[tokio::test]
async fn register_over_the_limit_is_refused_with_a_retry_delay() {
    let mut redis = connect_redis().await;
    let ip = unique_ip();

    for _ in 0..REGISTER_IP_LIMIT {
        rate_limit_service::check_register_attempt(&mut redis, ip)
            .await
            .expect("the limiter must answer while Redis is up");
    }

    let decision = rate_limit_service::check_register_attempt(&mut redis, ip)
        .await
        .expect("the limiter must answer while Redis is up");

    let RateLimitDecision::Limited { retry_after_secs } = decision else {
        panic!("attempt {} must be refused", REGISTER_IP_LIMIT + 1);
    };

    // Bounded by the window itself: a delay of zero would tell the client to
    // retry immediately into another refusal, and anything above the window
    // would mean the key outlived its TTL.
    assert!(
        retry_after_secs > 0 && retry_after_secs <= REGISTER_WINDOW_SECS,
        "retry_after_secs must fall within the window, got {retry_after_secs}"
    );
}

/// The limit is per address, not global: one caller exhausting their quota must
/// not lock everyone else out. This is the property that broke in production
/// when every request resolved to the reverse proxy's address.
#[tokio::test]
async fn register_limit_is_scoped_to_one_address() {
    let mut redis = connect_redis().await;
    let exhausted = unique_ip();
    let untouched = unique_ip();

    for _ in 0..=REGISTER_IP_LIMIT {
        rate_limit_service::check_register_attempt(&mut redis, exhausted)
            .await
            .expect("the limiter must answer while Redis is up");
    }

    let decision = rate_limit_service::check_register_attempt(&mut redis, untouched)
        .await
        .expect("the limiter must answer while Redis is up");

    assert!(
        matches!(decision, RateLimitDecision::Allowed),
        "a second address must keep its own quota"
    );
}

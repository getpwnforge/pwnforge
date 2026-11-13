//! The contact form fails closed when its rate limiter cannot answer: a
//! limiter error is the request's error, never a pass. The route turns that
//! error into 503 (see `api/src/error.rs`); this checks the service hands it
//! over instead of swallowing it.
//!
//! A real Redis, made to fail on one key: the limiter starts with INCR, and
//! INCR refuses a key that holds something other than an integer. No closed
//! port: a `ConnectionManager` connects when it is built, so it cannot be
//! handed over already broken. CI provides the instance (`REDIS_URL` is set
//! for `cargo test` in ci.yml); locally `make test-backend` points at the dev
//! compose.

use std::net::{IpAddr, Ipv6Addr};
use std::time::{SystemTime, UNIX_EPOCH};

use domain::dto::contact::ContactRequest;
use domain::types::ContactCategory;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use sea_orm::Database;
use services::config::Config;
use services::contact_service::{self, ContactError};
use services::rate_limit_service;

#[path = "common/config.rs"]
mod config;

async fn connect_redis() -> ConnectionManager {
    let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_owned());
    let client = redis::Client::open(url).expect("REDIS_URL must be a valid Redis URL");

    ConnectionManager::new(client)
        .await
        .expect("a Redis instance must be reachable")
}

/// A documentation address (RFC 3849) unique to this run, so the key below
/// is nobody else's.
fn fresh_address() -> IpAddr {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the system clock must be past the epoch")
        .as_nanos() as u64;
    let pid = std::process::id();

    IpAddr::V6(Ipv6Addr::new(
        0x2001,
        0x0db8,
        0xc0,
        pid as u16,
        (nanos >> 48) as u16,
        (nanos >> 32) as u16,
        (nanos >> 16) as u16,
        nanos as u16,
    ))
}

/// Turnstile off and mail on the console: if the limiter let the request
/// through, nothing else would stop it.
fn config() -> Config {
    Config {
        support_email: Some("support@example.test".to_owned()),
        ..config::config()
    }
}

#[tokio::test]
async fn a_limiter_that_cannot_answer_refuses_the_message() {
    let mut redis = connect_redis().await;
    let ip = fresh_address();
    let key = format!("ratelimit:contact:ip:{ip}");

    // The key the limiter counts under, checked rather than assumed: if its
    // name drifted, the poison below would miss and the request would pass.
    rate_limit_service::check_contact_attempt(&mut redis, ip)
        .await
        .expect("the limiter must answer while the key holds a count");
    let count: Option<String> = redis.get(&key).await.expect("GET must answer");
    assert_eq!(
        count.as_deref(),
        Some("1"),
        "the contact limiter must count under {key}"
    );

    // INCR refuses a value that is not an integer, so the next check errors.
    let _: () = redis
        .set(&key, "not-a-counter")
        .await
        .expect("SET must answer");

    let db =
        Database::connect(std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://pwnforge:changeme@127.0.0.1:5432/pwnforge".to_owned()
        }))
        .await
        .expect("a Postgres instance must be reachable");
    let request = ContactRequest {
        name: "Fail closed".to_owned(),
        email: "visitor@example.test".to_owned(),
        category: ContactCategory::General,
        message: "The limiter cannot answer this one.".to_owned(),
        turnstile_token: String::new(),
        locale: "en".to_owned(),
    };

    let outcome = contact_service::submit_contact(&db, &mut redis, &config(), ip, request).await;

    let _: () = redis.del(&key).await.expect("DEL must answer");

    assert!(
        matches!(outcome, Err(ContactError::Redis(_))),
        "a limiter error must refuse the message, got {outcome:?}"
    );
}

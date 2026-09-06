use std::net::IpAddr;

use domain::dto::contact::ContactRequest;
use rand::RngExt;
use redis::aio::ConnectionManager;
use sea_orm::DatabaseConnection;
use thiserror::Error;

use crate::config::Config;
use crate::email_service::{self, EmailError};
use crate::rate_limit_service::{self, RateLimitDecision};
use crate::turnstile_service::{self, TurnstileError};

#[derive(Debug, Error)]
pub enum ContactError {
    #[error("too many requests, try again in {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    #[error(transparent)]
    Turnstile(#[from] TurnstileError),

    #[error(transparent)]
    Email(#[from] EmailError),

    #[error(transparent)]
    Redis(#[from] redis::RedisError),
}

const REFERENCE_ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

fn generate_ticket_reference() -> String {
    let mut rng = rand::rng();
    let code: String = (0..7)
        .map(|_| {
            let idx = rng.random_range(0..REFERENCE_ALPHABET.len());
            REFERENCE_ALPHABET[idx] as char
        })
        .collect();
    format!("PF{code}")
}
pub async fn submit_contact(
    _db: &DatabaseConnection,
    redis: &mut ConnectionManager,
    config: &Config,
    client_ip: IpAddr,
    request: ContactRequest,
) -> Result<(), ContactError> {
    if let RateLimitDecision::Limited { retry_after_secs } =
        rate_limit_service::check_contact_attempt(redis, client_ip).await?
    {
        return Err(ContactError::RateLimited { retry_after_secs });
    }

    turnstile_service::verify(config, &request.turnstile_token, client_ip).await?;

    let reference = generate_ticket_reference();

    email_service::send_contact_notification(config, &request, &reference).await?;

    if let Err(err) = email_service::send_contact_confirmation(config, &request, &reference).await {
        tracing::warn!(error = ?err, %reference, "failed to send contact confirmation mail");
    }

    Ok(())
}

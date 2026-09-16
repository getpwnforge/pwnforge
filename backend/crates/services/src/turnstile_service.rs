use std::net::IpAddr;
use std::time::Duration;

use serde::Deserialize;
use thiserror::Error;

use crate::config::Config;

#[derive(Debug, Error)]
pub enum TurnstileError {
    #[error("anti-spam verification failed")]
    Failed,
}

#[derive(Deserialize)]
struct TurnstileResponse {
    success: bool,
}

/// Same budget, and the same mechanism, as the Pwned Passwords lookup in
/// `password_service`: set on the request builder before `send`, so it covers
/// connect, TLS handshake and the full response body rather than just one of
/// them. Without it an unresponsive siteverify endpoint pins a request handler
/// for as long as the OS keeps the socket open.
const TURNSTILE_TIMEOUT: Duration = Duration::from_secs(3);

/// Verifies a Turnstile token against Cloudflare's siteverify endpoint.
///
/// Returns Ok(()) when no secret is configured: a self-hosted instance that
/// has not set up Turnstile must stay usable, and opting out is a deliberate
/// operator choice, not a failure. The frontend mirrors this by not mounting
/// the widget when VITE_TURNSTILE_SITE_KEY is absent.
///
/// Every other failure is fail-closed, including a timeout: this is the
/// anti-bot gate itself, so a siteverify outage must not turn every protected
/// route into an unprotected one. That is the opposite of the Redis rate-limit
/// calls on `login`, which fail open because a password check still sits
/// underneath them. Here there is nothing underneath.
pub async fn verify(config: &Config, token: &str, remote_ip: IpAddr) -> Result<(), TurnstileError> {
    let Some(secret) = &config.turnstile_secret_key else {
        return Ok(());
    };

    let response = reqwest::Client::new()
        .post("https://challenges.cloudflare.com/turnstile/v0/siteverify")
        .json(&serde_json::json!({
            "secret": secret,
            "response": token,
            "remoteip": remote_ip.to_string(),
        }))
        .timeout(TURNSTILE_TIMEOUT)
        .send()
        .await
        .map_err(|_| TurnstileError::Failed)?;

    let result: TurnstileResponse = response.json().await.map_err(|_| TurnstileError::Failed)?;

    if result.success {
        Ok(())
    } else {
        Err(TurnstileError::Failed)
    }
}

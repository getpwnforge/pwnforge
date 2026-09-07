use std::net::IpAddr;

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

/// Verifies a Turnstile token against Cloudflare's siteverify endpoint.
///
/// Returns Ok(()) when no secret is configured: a self-hosted instance that
/// has not set up Turnstile must stay usable, and opting out is a deliberate
/// operator choice, not a failure. The frontend mirrors this by not mounting
/// the widget when VITE_TURNSTILE_SITE_KEY is absent.
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

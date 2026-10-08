use std::net::IpAddr;
use std::sync::LazyLock;
use std::time::Duration;

use serde::Deserialize;
use thiserror::Error;

use crate::config::Config;

#[derive(Debug, Error)]
pub enum TurnstileError {
    #[error("anti-spam verification failed")]
    Failed,
}

/// The widget each protected form renders, and the action the token it
/// produces must carry. The frontend declares the same strings. Cloudflare
/// accepts up to 32 characters among letters, digits, `_` and `-`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnstileAction {
    Login,
    Register,
    ForgotPassword,
    Contact,
}

impl TurnstileAction {
    pub fn as_str(self) -> &'static str {
        match self {
            TurnstileAction::Login => "login",
            TurnstileAction::Register => "register",
            TurnstileAction::ForgotPassword => "forgot_password",
            TurnstileAction::Contact => "contact",
        }
    }
}

/// The fields of a siteverify answer this module reads. A failed answer
/// carries only `success` and `error-codes`, hence the defaults.
#[derive(Debug, Default, Deserialize)]
struct SiteverifyResponse {
    success: bool,
    #[serde(default)]
    action: Option<String>,
    #[serde(default, rename = "error-codes")]
    error_codes: Vec<String>,
}

/// Why a token was refused.
#[derive(Debug, PartialEq, Eq)]
enum Rejection {
    /// Turnstile is on and the request carried no token: refused before any
    /// call, since Cloudflare could only say the same.
    EmptyToken,
    /// Cloudflare accepted the token, but it was issued for another form, or
    /// for none. A token solved on one page is not a pass for another.
    ActionMismatch { received: Option<String> },
    /// Cloudflare refused the token.
    Failed {
        kind: FailureKind,
        codes: Vec<String>,
    },
}

#[derive(Debug, PartialEq, Eq)]
enum FailureKind {
    /// The operator's secret key is missing or invalid: every request fails
    /// the same way until the configuration is fixed.
    Configuration,
    /// Anything about the token itself: invalid, expired, already spent.
    Ordinary,
}

/// Siteverify codes that mean the secret key is wrong rather than the token,
/// per Cloudflare's server-side validation reference.
const CONFIGURATION_CODES: [&str; 2] = ["missing-input-secret", "invalid-input-secret"];

/// What to do before any network call.
#[derive(Debug, PartialEq, Eq)]
enum Precheck<'a> {
    /// No secret configured: Turnstile is off, the request goes through.
    Disabled,
    Refused(Rejection),
    Call {
        secret: &'a str,
    },
}

fn precheck<'a>(secret: Option<&'a str>, token: &str) -> Precheck<'a> {
    match secret {
        None => Precheck::Disabled,
        Some(_) if token.trim().is_empty() => Precheck::Refused(Rejection::EmptyToken),
        Some(secret) => Precheck::Call { secret },
    }
}

/// Judges a decoded siteverify answer for the form that expects `expected`.
fn decide(response: &SiteverifyResponse, expected: TurnstileAction) -> Result<(), Rejection> {
    if !response.success {
        let kind = if response
            .error_codes
            .iter()
            .any(|code| CONFIGURATION_CODES.contains(&code.as_str()))
        {
            FailureKind::Configuration
        } else {
            FailureKind::Ordinary
        };

        return Err(Rejection::Failed {
            kind,
            codes: response.error_codes.clone(),
        });
    }

    if response.action.as_deref() != Some(expected.as_str()) {
        return Err(Rejection::ActionMismatch {
            received: response.action.clone(),
        });
    }

    Ok(())
}

/// Same budget, and the same mechanism, as the Pwned Passwords lookup in
/// `password_service`: set on the request builder before `send`, so it covers
/// connect, TLS handshake and the full response body rather than just one of
/// them. Without it an unresponsive siteverify endpoint pins a request handler
/// for as long as the OS keeps the socket open.
const TURNSTILE_TIMEOUT: Duration = Duration::from_secs(3);

const SITEVERIFY_URL: &str = "https://challenges.cloudflare.com/turnstile/v0/siteverify";

/// One client for the process: it keeps its connection pool and TLS sessions
/// between verifications instead of rebuilding them on every request. A
/// static rather than a field of the application state, so no service
/// signature changes to carry it.
static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);

/// Verifies a Turnstile token against Cloudflare's siteverify endpoint, for
/// the form identified by `action`.
///
/// Returns Ok(()) when no secret is configured: a self-hosted instance that
/// has not set up Turnstile must stay usable, and opting out is a deliberate
/// operator choice, not a failure. The frontend mirrors this by not mounting
/// the widget when the instance publishes no site key.
///
/// Every other failure is fail-closed, including a timeout: this is the
/// anti-bot gate itself, so a siteverify outage must not turn every protected
/// route into an unprotected one. That is the opposite of the Redis rate-limit
/// calls on `login`, which fail open because a password check still sits
/// underneath them. Here there is nothing underneath.
///
/// Logs never carry the token or the secret.
pub async fn verify(
    config: &Config,
    token: &str,
    action: TurnstileAction,
    remote_ip: IpAddr,
) -> Result<(), TurnstileError> {
    let secret = match precheck(config.turnstile_secret_key.as_deref(), token) {
        Precheck::Disabled => return Ok(()),
        Precheck::Refused(rejection) => return Err(report(rejection, action)),
        Precheck::Call { secret } => secret,
    };

    let response = CLIENT
        .post(SITEVERIFY_URL)
        .json(&serde_json::json!({
            "secret": secret,
            "response": token,
            "remoteip": remote_ip.to_string(),
        }))
        .timeout(TURNSTILE_TIMEOUT)
        .send()
        .await
        .map_err(|error| unreachable(&error, action))?;

    // Cloudflare answers a refused token with 200 and `success: false`. Any
    // other status says the call itself went wrong; the body is still read,
    // so a secret-key error it carries is reported as one.
    let status = response.status();
    if !status.is_success() {
        tracing::warn!(
            action = action.as_str(),
            status = status.as_u16(),
            "Turnstile siteverify answered with an error status, refusing"
        );
    }

    let answer: SiteverifyResponse = response.json().await.map_err(|error| {
        // The 3 second budget also covers the body.
        if error.is_timeout() {
            return unreachable(&error, action);
        }
        tracing::warn!(
            action = action.as_str(),
            status = status.as_u16(),
            "Turnstile siteverify answer is not readable, refusing"
        );
        TurnstileError::Failed
    })?;

    decide(&answer, action).map_err(|rejection| report(rejection, action))
}

/// Logs a siteverify call that got no usable answer. Fail-closed means every
/// protected form is refused while this lasts, so the operator needs the
/// cause: on a host without outbound access this is the only trace. The
/// request body, which holds the token and the secret, is never part of a
/// reqwest error; only the URL is.
fn unreachable(error: &reqwest::Error, action: TurnstileAction) -> TurnstileError {
    let cause = if error.is_timeout() {
        "no answer within the 3 second timeout"
    } else if error.is_connect() {
        "could not connect"
    } else {
        "the request failed"
    };

    tracing::warn!(
        action = action.as_str(),
        cause,
        %error,
        "Turnstile siteverify unreachable, refusing: the backend must reach \
         challenges.cloudflare.com over HTTPS while Turnstile is on"
    );

    TurnstileError::Failed
}

/// Logs a refusal at the level it deserves and turns it into the error the
/// routes map to `400 turnstile_failed`.
fn report(rejection: Rejection, action: TurnstileAction) -> TurnstileError {
    match &rejection {
        Rejection::Failed {
            kind: FailureKind::Configuration,
            codes,
        } => tracing::error!(
            action = action.as_str(),
            ?codes,
            "Turnstile refused the secret key: TURNSTILE_SECRET_KEY is missing or invalid, \
             or does not belong to the widget TURNSTILE_SITE_KEY names"
        ),
        Rejection::Failed {
            kind: FailureKind::Ordinary,
            codes,
        } => tracing::debug!(
            action = action.as_str(),
            ?codes,
            "Turnstile refused the token"
        ),
        Rejection::ActionMismatch { received } => tracing::debug!(
            expected = action.as_str(),
            ?received,
            "Turnstile token issued for another action"
        ),
        Rejection::EmptyToken => {
            tracing::debug!(
                action = action.as_str(),
                "request carried no Turnstile token"
            )
        }
    }

    TurnstileError::Failed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(success: bool, action: Option<&str>, codes: &[&str]) -> SiteverifyResponse {
        SiteverifyResponse {
            success,
            action: action.map(str::to_owned),
            error_codes: codes.iter().map(|code| (*code).to_owned()).collect(),
        }
    }

    #[test]
    fn a_success_for_the_expected_action_is_accepted() {
        let response = answer(true, Some("login"), &[]);
        assert_eq!(decide(&response, TurnstileAction::Login), Ok(()));
    }

    #[test]
    fn a_success_for_another_action_is_refused() {
        let response = answer(true, Some("contact"), &[]);
        assert_eq!(
            decide(&response, TurnstileAction::Login),
            Err(Rejection::ActionMismatch {
                received: Some("contact".to_owned())
            })
        );
    }

    #[test]
    fn a_success_without_an_action_is_refused() {
        let response = answer(true, None, &[]);
        assert_eq!(
            decide(&response, TurnstileAction::Register),
            Err(Rejection::ActionMismatch { received: None })
        );
    }

    #[test]
    fn a_failure_is_refused() {
        let response = answer(false, None, &[]);
        assert_eq!(
            decide(&response, TurnstileAction::Contact),
            Err(Rejection::Failed {
                kind: FailureKind::Ordinary,
                codes: vec![]
            })
        );
    }

    #[test]
    fn a_secret_key_error_is_classed_as_configuration() {
        for code in ["missing-input-secret", "invalid-input-secret"] {
            let response = answer(false, None, &[code]);
            assert_eq!(
                decide(&response, TurnstileAction::ForgotPassword),
                Err(Rejection::Failed {
                    kind: FailureKind::Configuration,
                    codes: vec![code.to_owned()]
                }),
                "{code} is the operator's configuration"
            );
        }
    }

    #[test]
    fn any_other_error_code_is_ordinary() {
        for code in [
            "missing-input-response",
            "invalid-input-response",
            "bad-request",
            "timeout-or-duplicate",
            "internal-error",
        ] {
            let response = answer(false, None, &[code]);
            assert_eq!(
                decide(&response, TurnstileAction::Login),
                Err(Rejection::Failed {
                    kind: FailureKind::Ordinary,
                    codes: vec![code.to_owned()]
                }),
                "{code} is about the token"
            );
        }
    }

    #[test]
    fn a_failed_answer_decodes_without_action() {
        let response: SiteverifyResponse =
            serde_json::from_str(r#"{"success":false,"error-codes":["invalid-input-secret"]}"#)
                .expect("a failed answer must decode");
        assert!(!response.success);
        assert_eq!(response.action, None);
        assert_eq!(response.error_codes, vec!["invalid-input-secret"]);
    }

    #[test]
    fn an_empty_token_is_refused_locally_when_turnstile_is_on() {
        assert_eq!(
            precheck(Some("secret"), ""),
            Precheck::Refused(Rejection::EmptyToken)
        );
        assert_eq!(
            precheck(Some("secret"), "   "),
            Precheck::Refused(Rejection::EmptyToken)
        );
    }

    #[test]
    fn turnstile_off_accepts_without_a_call() {
        assert_eq!(precheck(None, ""), Precheck::Disabled);
        assert_eq!(precheck(None, "some-token"), Precheck::Disabled);
    }

    #[test]
    fn a_token_with_turnstile_on_needs_a_call() {
        assert_eq!(
            precheck(Some("secret"), "a-token"),
            Precheck::Call { secret: "secret" }
        );
    }

    #[test]
    fn the_actions_match_cloudflares_format() {
        for action in [
            TurnstileAction::Login,
            TurnstileAction::Register,
            TurnstileAction::ForgotPassword,
            TurnstileAction::Contact,
        ] {
            let value = action.as_str();
            assert!(value.len() <= 32, "{value} is too long");
            assert!(
                value
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
                "{value} has a character Cloudflare refuses"
            );
        }
    }
}

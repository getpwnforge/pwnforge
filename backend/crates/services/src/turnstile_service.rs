use std::net::IpAddr;
use std::sync::LazyLock;
use std::time::Duration;

use serde::Deserialize;
use thiserror::Error;

use crate::config::Config;

/// Both variants answer the same `400 turnstile_failed`: the distinction is
/// for the server, which must know whether Cloudflare was asked at all.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TurnstileError {
    /// Turnstile is on and the request carried no token. Refused before any
    /// call to Cloudflare, which could only say the same.
    #[error("anti-spam verification failed: the request carried no token")]
    EmptyToken,

    /// Cloudflare refused the token, or gave no usable answer.
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
    #[serde(default)]
    metadata: Option<SiteverifyMetadata>,
}

/// The part of `metadata` this module reads.
#[derive(Debug, Default, Deserialize)]
struct SiteverifyMetadata {
    /// Set by Cloudflare when the answer comes from one of its dummy secret
    /// keys, whose tokens carry no action.
    #[serde(default)]
    result_with_testing_key: bool,
}

impl SiteverifyResponse {
    fn is_from_testing_key(&self) -> bool {
        self.metadata
            .as_ref()
            .is_some_and(|metadata| metadata.result_with_testing_key)
    }
}

/// Why a token was refused.
#[derive(Debug, PartialEq, Eq)]
enum Rejection {
    /// Turnstile is on and the request carried no token: refused before any
    /// call, since Cloudflare could only say the same.
    EmptyToken,
    /// Siteverify answered with a status other than 2xx. Refused whatever the
    /// body says; its codes are only kept for the log.
    ErrorStatus {
        status: u16,
        kind: FailureKind,
        codes: Vec<String>,
    },
    /// Cloudflare accepted the token, but it was issued for another form, or
    /// for none. A token solved on one page is not a pass for another.
    ActionMismatch { received: Option<String> },
    /// Cloudflare refused the token.
    Failed {
        kind: FailureKind,
        codes: Vec<String>,
    },
}

/// What an accepted answer means.
#[derive(Debug, PartialEq, Eq)]
enum Acceptance {
    /// A real token, issued for the form that expects it.
    Verified,
    /// An answer to one of Cloudflare's dummy keys: it passes, and nothing is
    /// being protected.
    TestingKey,
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

/// Judges a siteverify answer, its HTTP status included, for the form that
/// expects `expected`.
fn decide(
    status: u16,
    response: &SiteverifyResponse,
    expected: TurnstileAction,
) -> Result<Acceptance, Rejection> {
    // Cloudflare answers a refused token with 200 and `success: false`, so any
    // other status means the call itself went wrong, and no body can turn it
    // into a pass.
    if !(200..300).contains(&status) {
        return Err(Rejection::ErrorStatus {
            status,
            kind: failure_kind(&response.error_codes),
            codes: response.error_codes.clone(),
        });
    }

    if !response.success {
        return Err(Rejection::Failed {
            kind: failure_kind(&response.error_codes),
            codes: response.error_codes.clone(),
        });
    }

    let testing = response.is_from_testing_key();
    match response.action.as_deref() {
        Some(action) if action == expected.as_str() => {}
        // The dummy keys issue tokens without an action, so the check cannot
        // apply to them. Only an answer that says it comes from one is let
        // through this way: a real token without an action is no pass.
        None if testing => {}
        received => {
            return Err(Rejection::ActionMismatch {
                received: received.map(str::to_owned),
            });
        }
    }

    Ok(if testing {
        Acceptance::TestingKey
    } else {
        Acceptance::Verified
    })
}

fn failure_kind(codes: &[String]) -> FailureKind {
    if codes
        .iter()
        .any(|code| CONFIGURATION_CODES.contains(&code.as_str()))
    {
        FailureKind::Configuration
    } else {
        FailureKind::Ordinary
    }
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

    // The body is read whatever the status, for the error codes it may carry.
    // `decide` refuses an error status on its own.
    let status = response.status();
    let answer: SiteverifyResponse = match response.json().await {
        Ok(answer) => answer,
        // The 3 second budget also covers the body.
        Err(error) if error.is_timeout() => return Err(unreachable(&error, action)),
        // Refused below on the status alone; there are just no codes to log.
        Err(_) if !status.is_success() => SiteverifyResponse::default(),
        Err(_) => {
            tracing::warn!(
                action = action.as_str(),
                status = status.as_u16(),
                "Turnstile siteverify answer is not readable, refusing"
            );
            return Err(TurnstileError::Failed);
        }
    };

    match decide(status.as_u16(), &answer, action) {
        Ok(Acceptance::Verified) => Ok(()),
        Ok(Acceptance::TestingKey) => {
            tracing::warn!(
                action = action.as_str(),
                "Turnstile answered for one of Cloudflare's test keys: every request \
                 passes and the form is not protected"
            );
            Ok(())
        }
        Err(rejection) => Err(report(rejection, action)),
    }
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
        }
        | Rejection::ErrorStatus {
            kind: FailureKind::Configuration,
            codes,
            ..
        } => tracing::error!(
            action = action.as_str(),
            ?codes,
            "Turnstile refused the secret key: TURNSTILE_SECRET_KEY is missing or invalid, \
             or does not belong to the widget TURNSTILE_SITE_KEY names"
        ),
        Rejection::ErrorStatus {
            status,
            kind: FailureKind::Ordinary,
            codes,
        } => tracing::warn!(
            action = action.as_str(),
            status,
            ?codes,
            "Turnstile siteverify answered with an error status, refusing"
        ),
        Rejection::Failed {
            kind: FailureKind::Ordinary,
            codes,
        } => tracing::debug!(
            action = action.as_str(),
            ?codes,
            "Turnstile refused the token"
        ),
        // Warn rather than debug: a form always sends a token solved for its
        // own action, so a mismatch is a token solved elsewhere and replayed
        // here.
        Rejection::ActionMismatch { received } => tracing::warn!(
            expected = action.as_str(),
            ?received,
            "Turnstile token issued for another action, refusing"
        ),
        Rejection::EmptyToken => {
            tracing::debug!(
                action = action.as_str(),
                "request carried no Turnstile token"
            )
        }
    }

    match rejection {
        Rejection::EmptyToken => TurnstileError::EmptyToken,
        _ => TurnstileError::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{EmailBackend, SmtpTls, StorageConfig};

    /// The status of every answer below that is not about the status itself.
    const OK: u16 = 200;

    fn answer(success: bool, action: Option<&str>, codes: &[&str]) -> SiteverifyResponse {
        SiteverifyResponse {
            success,
            action: action.map(str::to_owned),
            error_codes: codes.iter().map(|code| (*code).to_owned()).collect(),
            metadata: None,
        }
    }

    /// A success flagged the way Cloudflare's dummy keys flag theirs.
    fn testing_key_answer(action: Option<&str>) -> SiteverifyResponse {
        SiteverifyResponse {
            metadata: Some(SiteverifyMetadata {
                result_with_testing_key: true,
            }),
            ..answer(true, action, &[])
        }
    }

    #[test]
    fn a_success_for_the_expected_action_is_accepted() {
        let response = answer(true, Some("login"), &[]);
        assert_eq!(
            decide(OK, &response, TurnstileAction::Login),
            Ok(Acceptance::Verified)
        );
    }

    #[test]
    fn a_success_for_another_action_is_refused() {
        let response = answer(true, Some("contact"), &[]);
        assert_eq!(
            decide(OK, &response, TurnstileAction::Login),
            Err(Rejection::ActionMismatch {
                received: Some("contact".to_owned())
            })
        );
    }

    /// No action and no testing-key flag: a real token issued for no form.
    #[test]
    fn a_success_without_an_action_is_refused() {
        let response = answer(true, None, &[]);
        assert_eq!(
            decide(OK, &response, TurnstileAction::Register),
            Err(Rejection::ActionMismatch { received: None })
        );
    }

    #[test]
    fn a_testing_key_answer_without_an_action_is_accepted() {
        let response = testing_key_answer(None);
        assert_eq!(
            decide(OK, &response, TurnstileAction::Login),
            Ok(Acceptance::TestingKey)
        );
    }

    /// The flag excuses a missing action, not a wrong one.
    #[test]
    fn a_testing_key_answer_for_another_action_is_refused() {
        let response = testing_key_answer(Some("contact"));
        assert_eq!(
            decide(OK, &response, TurnstileAction::Login),
            Err(Rejection::ActionMismatch {
                received: Some("contact".to_owned())
            })
        );
    }

    /// Still reported as a testing key, so the warning fires whatever the
    /// action.
    #[test]
    fn a_testing_key_answer_for_the_expected_action_is_still_a_testing_key() {
        let response = testing_key_answer(Some("login"));
        assert_eq!(
            decide(OK, &response, TurnstileAction::Login),
            Ok(Acceptance::TestingKey)
        );
    }

    #[test]
    fn an_error_status_is_refused_even_when_the_body_says_success() {
        let response = answer(true, Some("login"), &[]);
        for status in [400, 500, 503] {
            assert_eq!(
                decide(status, &response, TurnstileAction::Login),
                Err(Rejection::ErrorStatus {
                    status,
                    kind: FailureKind::Ordinary,
                    codes: vec![]
                }),
                "a {status} must be refused"
            );
        }
    }

    /// The codes of an error answer are kept, and a secret-key error among
    /// them is still the operator's configuration.
    #[test]
    fn an_error_status_keeps_the_codes_of_its_body() {
        let response = answer(false, None, &["invalid-input-secret"]);
        assert_eq!(
            decide(400, &response, TurnstileAction::Contact),
            Err(Rejection::ErrorStatus {
                status: 400,
                kind: FailureKind::Configuration,
                codes: vec!["invalid-input-secret".to_owned()]
            })
        );
    }

    #[test]
    fn a_failure_is_refused() {
        let response = answer(false, None, &[]);
        assert_eq!(
            decide(OK, &response, TurnstileAction::Contact),
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
                decide(OK, &response, TurnstileAction::ForgotPassword),
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
                decide(OK, &response, TurnstileAction::Login),
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
        assert!(!response.is_from_testing_key());
    }

    /// The answer siteverify gave the dummy secret and token on 2026-10-09,
    /// verbatim.
    #[test]
    fn the_dummy_key_answer_decodes_as_a_testing_key() {
        let response: SiteverifyResponse = serde_json::from_str(
            r#"{"challenge_ts":"2026-10-09T21:48:20.870Z","error-codes":[],"hostname":"example.com","metadata":{"result_with_testing_key":true},"success":true}"#,
        )
        .expect("the dummy answer must decode");
        assert!(response.is_from_testing_key());
        assert_eq!(
            decide(OK, &response, TurnstileAction::Contact),
            Ok(Acceptance::TestingKey)
        );
    }

    fn turnstile_on() -> Config {
        Config {
            port: 0,
            public_url: "http://localhost".to_owned(),
            cors_origin: "http://localhost:5173".to_owned(),
            setup_token: None,
            database_url: String::new(),
            database_max_connections: 1,
            redis_url: String::new(),
            jwt_secret: "x".repeat(32),
            encryption_key: "x".repeat(32),
            turnstile_site_key: Some("site-key-never-rendered".to_owned()),
            turnstile_secret_key: Some("secret-key-never-sent".to_owned()),
            cookie_secure: false,
            trusted_proxy_count: 0,
            block_disposable_emails: false,
            check_pwned_passwords: false,
            email_backend: EmailBackend::Console,
            email_from: String::new(),
            support_email: None,
            smtp_host: String::new(),
            smtp_port: 0,
            smtp_username: String::new(),
            smtp_password: String::new(),
            smtp_tls: SmtpTls::None,
            resend_api_key: String::new(),
            storage: StorageConfig {
                endpoint: None,
                region: "unused".to_owned(),
                access_key_id: "unused".to_owned(),
                secret_access_key: "unused".to_owned(),
                force_path_style: true,
                bucket_attachments: "unused".to_owned(),
                bucket_avatars: "unused".to_owned(),
                key_prefix: None,
                public_base_url: None,
            },
        }
    }

    /// Through the public entry point: with Turnstile on, an empty token is
    /// refused for its own reason, before any call. A call could not end in
    /// `EmptyToken`: Cloudflare would refuse the fake secret, or be
    /// unreachable, and both are `Failed`.
    #[tokio::test]
    async fn verify_refuses_an_empty_token_without_asking_cloudflare() {
        let config = turnstile_on();
        let client = IpAddr::from([192, 0, 2, 1]);

        for token in ["", "   "] {
            assert_eq!(
                verify(&config, token, TurnstileAction::Login, client).await,
                Err(TurnstileError::EmptyToken),
                "{token:?} must be refused locally"
            );
        }
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

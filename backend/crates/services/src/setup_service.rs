use crate::config::{Config, EmailBackend, SmtpTls};
use crate::{
    audit_service::{self, AuditContext, AuditError, AuditTarget},
    auth_service::{self, AuthError},
    email_service::EmailError,
    instance_service::{self, SETTINGS_ID},
    legal_service::{self, LegalError},
    password_service::{self, PasswordError},
    token_service,
};
use chrono::Utc;
use domain::{
    dto::{
        auth::SessionContext,
        setup::{EmailConfigResponse, SetupRequest, ValidateAdminRequest},
    },
    entities::{instance_settings, user_emails, users},
    legal::{AcceptanceMethod, LegalDocument},
    types::AuditAction,
};
use sea_orm::{
    ActiveModelTrait, DatabaseConnection, DbErr, EntityTrait, QuerySelect, Set, TransactionTrait,
};
use std::str::FromStr;
use subtle::ConstantTimeEq;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum SetupError {
    /// Wizard already ran. Handlers turn this into 404, not 403: the setup
    /// surface should not be advertised once it is closed.
    #[error("setup already completed")]
    AlreadyCompleted,

    /// Token missing or wrong. Same 404, for the same reason.
    #[error("invalid setup token")]
    InvalidToken,

    #[error("invalid timezone")]
    InvalidTimezone,

    #[error(transparent)]
    Email(#[from] EmailError),

    #[error(transparent)]
    Auth(#[from] AuthError),

    #[error(transparent)]
    Password(#[from] PasswordError),

    #[error(transparent)]
    Audit(#[from] AuditError),

    #[error(transparent)]
    Instance(#[from] instance_service::InstanceError),

    #[error(transparent)]
    Legal(#[from] LegalError),

    #[error(transparent)]
    Db(#[from] DbErr),
}
/// Generated at boot, only when the wizard still has to run.
///
/// Held in memory and never persisted: a restart issues a new one, which is
/// the intended behaviour. `configured` takes precedence when set, for
/// platforms where the operator cannot read container output.
pub async fn issue_boot_token(
    db: &DatabaseConnection,
    configured: Option<&str>,
) -> Result<Option<String>, SetupError> {
    if instance_service::is_setup_completed(db).await? {
        return Ok(None);
    }

    if let Some(token) = configured {
        return Ok(Some(token.to_string()));
    }

    let token = token_service::generate_opaque_token().secret;

    Ok(Some(token))
}

/// Compares in constant time: a plain == on a short secret leaks its length
/// and its prefix through timing.
pub fn verify_token(expected: Option<&str>, presented: &str) -> Result<(), SetupError> {
    // None means the setup is closed: refuse without comparing.
    let expected = expected.ok_or(SetupError::InvalidToken)?;

    if expected.as_bytes().ct_eq(presented.as_bytes()).unwrap_u8() == 1 {
        Ok(())
    } else {
        Err(SetupError::InvalidToken)
    }
}

/// Reports the effective email configuration, secrets removed.
///
/// SMTP fields stay None unless that backend is active: showing a stale host
/// on a Resend instance would confuse rather than help.
pub fn email_config(config: &Config) -> EmailConfigResponse {
    let (backend, smtp_host, smtp_port, smtp_tls, smtp_auth, resend_key_hint) =
        match config.email_backend {
            EmailBackend::Console => ("console", None, None, None, false, None),

            EmailBackend::Smtp => (
                "smtp",
                Some(config.smtp_host.clone()),
                Some(config.smtp_port),
                Some(
                    match config.smtp_tls {
                        SmtpTls::Implicit => "implicit",
                        SmtpTls::StartTls => "starttls",
                        SmtpTls::None => "none",
                    }
                    .to_owned(),
                ),
                // Whether credentials are configured, never what they are.
                !config.smtp_username.is_empty(),
                None,
            ),

            EmailBackend::Resend => (
                "resend",
                None,
                None,
                None,
                false,
                key_hint(&config.resend_api_key),
            ),
        };

    EmailConfigResponse {
        backend: backend.to_owned(),
        from: config.email_from.clone(),
        smtp_host,
        smtp_port,
        smtp_tls,
        smtp_auth,
        public_url: config.public_url.clone(),
        resend_key_hint,
    }
}

/// Runs the account rules against the administrator fields, creating nothing.
///
/// Exactly the checks `complete` performs, so a pass here means the same input
/// will not be turned away on those grounds a few seconds later. Not a
/// guarantee: the breach corpus is remote and the reserved list can change,
/// which is why `complete` re-runs all of it rather than trusting this call.
pub async fn validate_admin(
    db: &DatabaseConnection,
    config: &Config,
    payload: &ValidateAdminRequest,
) -> Result<(), SetupError> {
    let email = auth_service::normalize_email(&payload.admin_email);
    let username = payload.admin_username.trim();

    auth_service::validate_new_account(db, config, username, &email, &payload.admin_password)
        .await?;

    Ok(())
}

/// Creates the first administrator and records the instance settings.
pub async fn complete(
    db: &DatabaseConnection,
    config: &Config,
    ctx: &AuditContext,
    payload: SetupRequest,
) -> Result<(users::Model, user_emails::Model), SetupError> {
    if chrono_tz::Tz::from_str(&payload.default_timezone).is_err() {
        return Err(SetupError::InvalidTimezone);
    }

    let email = auth_service::normalize_email(&payload.admin_email);
    let username = payload.admin_username.trim().to_owned();

    // An instance administrator is not exempt from the rules that apply to
    // any other account.
    auth_service::validate_new_account(db, config, &username, &email, &payload.admin_password)
        .await?;

    let password_hash = password_service::hash_password(payload.admin_password).await?;

    let txn = db.begin().await?;

    // Re-checked inside the lock: two concurrent requests would otherwise
    // both pass the check outside and create two administrators.
    let settings = instance_settings::Entity::find_by_id(SETTINGS_ID)
        .lock_exclusive()
        .one(&txn)
        .await?
        .ok_or_else(|| DbErr::Custom("instance_settings row missing".to_owned()))?;

    if settings.setup_completed_at.is_some() {
        return Err(SetupError::AlreadyCompleted);
    }

    let user = users::ActiveModel {
        id: Set(Uuid::now_v7()),
        username: Set(username),
        password_hash: Set(password_hash),
        is_instance_admin: Set(true),
        locale: Set(payload.default_locale.clone()),
        ..Default::default()
    }
    .insert(&txn)
    .await?;

    let email_row = user_emails::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(user.id),
        email: Set(email),
        is_primary: Set(true),
        verified_at: Set(Some(Utc::now().into())),
        ..Default::default()
    }
    .insert(&txn)
    .await?;

    let legal_ctx = SessionContext {
        user_agent: ctx.user_agent.clone(),
        ip_address: ctx.ip_address,
    };

    for document in LegalDocument::ALL {
        legal_service::record(
            &txn,
            user.id,
            document,
            document.current_version(),
            AcceptanceMethod::Operator,
            &legal_ctx,
        )
        .await?;
    }

    instance_settings::ActiveModel {
        id: Set(SETTINGS_ID),
        default_locale: Set(payload.default_locale),
        default_timezone: Set(payload.default_timezone),
        allow_public_signup: Set(payload.allow_public_signup),
        setup_completed_at: Set(Some(Utc::now().into())),
        hide_landing_page: Set(payload.hide_landing_page),
        ..Default::default()
    }
    .update(&txn)
    .await?;

    let audit_ctx = AuditContext {
        // The user just created is the one who ran the wizard.
        actor_id: Some(user.id),
        ip_address: ctx.ip_address,
        user_agent: ctx.user_agent.clone(),
    };

    audit_service::record(
        &txn,
        AuditAction::InstanceSetupCompleted,
        &audit_ctx,
        AuditTarget {
            user_id: Some(user.id),
            ..Default::default()
        },
        None,
        serde_json::json!({}),
    )
    .await?;

    // TODO(3.6): create the personal team here, same transaction.

    txn.commit().await?;

    Ok((user, email_row))
}

fn key_hint(key: &str) -> Option<String> {
    (!key.is_empty()).then(|| format!("{}...", key.chars().take(8).collect::<String>()))
}

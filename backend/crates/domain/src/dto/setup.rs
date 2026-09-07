use crate::dto::auth::{USERNAME_RE, password_composition};
use serde::{Deserialize, Serialize};
use validator::Validate;

/// Answered without a token: the frontend needs it to decide whether to show
/// the wizard at all, before it has anything to authenticate with.
#[derive(Serialize, utoipa::ToSchema)]
pub struct SetupStatusResponse {
    pub completed: bool,
}

/// What the backend read from its environment, so the operator can check that
/// the .env is what the process actually understood. Never carries the SMTP
/// password nor the API key: a wizard is not a reason to expose secrets.
#[derive(Serialize, utoipa::ToSchema)]
pub struct EmailConfigResponse {
    pub backend: String,
    pub from: String,
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    pub smtp_tls: Option<String>,
    /// Whether credentials are configured, not what they are.
    pub smtp_auth: bool,
    pub resend_key_hint: Option<String>,
    pub public_url: String,
}

#[derive(Deserialize, Validate, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TestEmailRequest {
    #[validate(email, length(max = 254))]
    pub to: String,
}

/// The administrator half of `SetupRequest`, checked on its own.
///
/// Reserved usernames, disposable domains and breached passwords are all
/// server-side rules. Without this route the wizard could only report them once
/// every step had been filled, which means sending the operator back two
/// screens to fix a field they left long ago.
#[derive(Deserialize, Validate, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ValidateAdminRequest {
    #[validate(length(min = 3, max = 32), regex(path = *USERNAME_RE))]
    pub admin_username: String,

    #[validate(email, length(max = 254))]
    pub admin_email: String,

    #[validate(length(min = 12, max = 128), custom(function = password_composition))]
    pub admin_password: String,
}

#[derive(Deserialize, Validate, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SetupRequest {
    #[validate(length(min = 3, max = 32), regex(path = *USERNAME_RE))]
    pub admin_username: String,

    #[validate(email, length(max = 254))]
    pub admin_email: String,

    #[validate(length(min = 12, max = 128), custom(function = password_composition))]
    pub admin_password: String,

    #[validate(length(min = 2, max = 10))]
    pub default_locale: String,

    /// IANA identifier, never a UTC offset: offsets change twice a year while
    /// the identifier carries the whole rule.
    #[validate(length(max = 64))]
    pub default_timezone: String,

    pub allow_public_signup: bool,

    pub hide_landing_page: bool,
}

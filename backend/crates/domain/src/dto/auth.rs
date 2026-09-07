// domain/src/dto/auth.rs
use crate::{
    dto::legal::LegalAcceptanceInput,
    entities::{user_emails, users},
};
use regex::Regex;
use sea_orm::{entity::prelude::IpNetwork, prelude::DateTimeUtc};
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use uuid::Uuid;
use validator::{Validate, ValidationError};

// Alphanumeric, underscore and dash. Must start with a letter or digit.
pub static USERNAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9_-]*$").unwrap());

/// At least one digit and one non-alphanumeric character.
///
/// A function rather than a regex: the `regex` crate has no lookahead, so
/// `^(?=.*\d)(?=.*\W)` will not compile. "Special" is whatever
/// `char::is_alphanumeric` rejects, which is Unicode-aware — `é` counts as a
/// letter, not as a symbol.
///
/// Applied to every route that sets a password: register, reset, change and
/// the two setup requests. A rule enforced on one path only is not a rule.
pub(crate) fn password_composition(value: &str) -> Result<(), ValidationError> {
    let has_digit = value.chars().any(|c| c.is_ascii_digit());
    let has_special = value.chars().any(|c| !c.is_alphanumeric());

    if has_digit && has_special {
        Ok(())
    } else {
        Err(ValidationError::new("password_composition"))
    }
}

pub struct RegisteredUser {
    pub user: users::Model,
    pub email: user_emails::Model,
}

pub struct LoggedInUser {
    pub user: users::Model,
    pub email: user_emails::Model,
    pub access_token: String,
    pub refresh_token: String,
}

pub struct SessionContext {
    pub user_agent: Option<String>,
    pub ip_address: Option<IpNetwork>,
}

pub struct IssuedSession {
    pub secret: String,
    pub session_id: Uuid,
}

pub struct RotatedSession {
    pub user_id: Uuid,
    pub secret: String,
    pub session_id: Uuid,
}

#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct RegisterRequest {
    #[validate(email, length(max = 254))]
    pub email: String,

    #[validate(length(min = 3, max = 32), regex(path = *USERNAME_RE))]
    pub username: String,

    #[validate(length(min = 12, max = 128), custom(function = password_composition))]
    pub password: String,

    #[validate(nested)]
    pub legal: LegalAcceptanceInput,

    pub turnstile_token: String,
}

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub email_verified: bool,
    pub created_at: DateTimeUtc,
}

impl From<RegisteredUser> for UserResponse {
    fn from(registered: RegisteredUser) -> Self {
        Self {
            id: registered.user.id,
            username: registered.user.username,
            email: registered.email.email,
            email_verified: registered.email.verified_at.is_some(),
            created_at: registered.user.created_at.into(),
        }
    }
}

#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct LoginRequest {
    #[validate(length(max = 254))]
    pub username_or_email: String,

    // No composition rule here on purpose: an account created before the rule
    // existed must still be able to sign in. Tightening this would lock those
    // users out of the very flow that lets them change their password.
    #[validate(length(min = 12, max = 128))]
    pub password: String,

    pub turnstile_token: String,
}

impl UserResponse {
    pub fn new(user: users::Model, email: user_emails::Model) -> Self {
        Self {
            id: user.id,
            username: user.username,
            email: email.email,
            email_verified: email.verified_at.is_some(),
            created_at: user.created_at.into(),
        }
    }
}

#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct PasswordForgotRequest {
    #[validate(email, length(max = 254))]
    pub email: String,

    pub turnstile_token: String,
}

#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct PasswordResetRequest {
    #[validate(length(min = 1, max = 128))]
    pub token: String,

    #[validate(
        length(min = 12, max = 128),
        custom(function = password_composition)
    )]
    pub new_password: String,
}

#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct PasswordChangeRequest {
    #[validate(length(max = 128))]
    pub current_password: String,

    #[validate(
        length(min = 12, max = 128),
        custom(function = password_composition)
    )]
    pub new_password: String,
}

#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct EmailVerifyRequest {
    #[validate(length(min = 1, max = 128))]
    pub token: String,
}

#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct EmailResendRequest {
    #[validate(length(min = 1, max = 128))]
    pub token: String,
}

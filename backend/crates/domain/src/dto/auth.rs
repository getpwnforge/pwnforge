// domain/src/dto/auth.rs
use crate::entities::{user_emails, users};
use regex::Regex;
use sea_orm::{entity::prelude::IpNetwork, prelude::DateTimeUtc, sea_query::value::prelude::rust_decimal::str};
use serde::{Deserialize, Serialize};
use std::{sync::LazyLock};
use uuid::Uuid;
use validator::Validate;

// Alphanumeric, underscore and dash. Must start with a letter or digit.
pub static USERNAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9_-]*$").unwrap());

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

    #[validate(length(min = 12, max = 128))]
    pub password: String,
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

    #[validate(length(min = 12, max = 128))]
    pub password: String,
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
}

#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct PasswordResetRequest {
    #[validate(length(min = 1, max = 128))]
    pub token: String,

    #[validate(length(min = 12, max = 128))]
    pub new_password: String,
}

#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct PasswordChangeRequest {
    #[validate(length(max = 128))]
    pub current_password: String,

    #[validate(length(min = 12, max = 128))]
    pub new_password: String,
}

#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct EmailVerifyRequest {
    #[validate(length(min = 1, max = 128))]
    pub token: String,
}

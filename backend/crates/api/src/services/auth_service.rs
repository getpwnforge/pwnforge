// crates/api/src/services/auth_service.rs
use super::{blocked_email_service, jwt_service, password_service, session_service, email_service};
use crate::services::{auth_token_service, instance_service};
use crate::{config::Config};
use crate::services::blocked_email_service::BlockedEmailError;
use chrono::Utc;
use domain::{
    dto::auth::{LoggedInUser, LoginRequest, RegisterRequest, RegisteredUser, SessionContext},
    entities::{reserved_usernames, user_emails, users},
};
use jsonwebtoken::EncodingKey;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
    TransactionTrait,
    prelude::{DateTimeWithTimeZone},
    sea_query::Expr,
};
use sea_orm::{DbErr, SqlErr};
use thiserror::Error;
use uuid::Uuid;

/// Maps Postgres unique-violation errors to the matching domain error.
/// This is the real uniqueness guarantee: the pre-checks in register() only
/// exist to produce a clear message on the common path.
fn map_unique_violation(err: DbErr) -> AuthError {
    match err.sql_err() {
        Some(SqlErr::UniqueConstraintViolation(detail)) => {
            if detail.contains("users_username_key") {
                AuthError::UsernameTaken
            } else if detail.contains("user_emails_email_key") {
                AuthError::EmailTaken
            } else {
                AuthError::Db(err)
            }
        }
        _ => AuthError::Db(err),
    }
}

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("email domain is not allowed")]
    BlockedEmail(#[from] BlockedEmailError),

    #[error("public signup is disabled")]
    PublicSignupDisabled,

    #[error("username is already taken")]
    UsernameTaken,

    #[error("email is already registered")]
    EmailTaken,

    #[error("username is reserved")]
    UsernameReserved,

    #[error("email is invalid")]
    InvalidEmail,

    #[error("credentials are invalid")]
    InvalidCredentials,

    #[error("email is not verified")]
    EmailNotVerified,

    #[error("account is suspended")]
    AccountSuspended {
        until: Option<DateTimeWithTimeZone>,
        reason: Option<String>,
    },

    #[error(transparent)]
    Password(#[from] password_service::PasswordError),

    #[error("password appears in known breaches")]
    PasswordCompromised,

    #[error("new password must be different from the old one")]
    PasswordUnchanged,

    #[error("refresh token is invalid or expired")]
    InvalidRefreshToken,

    #[error(transparent)]
    Session(#[from] session_service::SessionError),

    #[error(transparent)]
    Email(#[from] email_service::EmailError),

    #[error(transparent)]
    AuthToken(#[from] auth_token_service::AuthTokenError),

    #[error(transparent)]
    Jwt(#[from] jwt_service::JwtError),

    #[error(transparent)]
    Instance(#[from] instance_service::InstanceError),

    #[error(transparent)]
    Db(#[from] sea_orm::DbErr),
}

pub async fn register(
    db: &DatabaseConnection,
    config: &Config,
    payload: RegisterRequest,
) -> Result<RegisteredUser, AuthError> {
    // Check if public signup is allowed.
    let settings = instance_service::settings(db).await?;

    if !settings.allow_public_signup {
        return Err(AuthError::PublicSignupDisabled);
    }

    // Normalize inputs. Only the domain part of the email is lowercased:
    // the local part is case-sensitive per RFC 5321.
    let email = normalize_email(&payload.email);
    let username = payload.username.trim().to_owned();

    validate_new_account(db, config, &username, &email, &payload.password).await?;

    // Hash the password. Done last: Argon2 costs ~100ms with OWASP params,
    // no point spending it on a registration that was going to fail.
    let password_hash = password_service::hash_password(payload.password).await?;

    // Open the transaction. Both inserts must succeed together: a user with
    // no primary email cannot log in.
    let transaction = db.begin().await?;

    let user = users::ActiveModel {
        id: Set(Uuid::now_v7()),
        username: Set(username),
        password_hash: Set(password_hash),
        locale: Set(settings.default_locale),
        ..Default::default()
    }
    .insert(&transaction)
    .await
    .map_err(map_unique_violation)?;

    // Insert the primary email.
    let email = user_emails::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(user.id),
        email: Set(email),
        is_primary: Set(true),

        ..Default::default()
    }
    .insert(&transaction)
    .await
    .map_err(map_unique_violation)?;

    // TODO(3.6): create the personal team here, same transaction.

    transaction.commit().await?;

    // Send the verification email. Best effort: a mail failure must not undo a completed registration.
    match auth_token_service::issue(
        db,
        user.id,
        email.id,
        domain::types::TokenKind::EmailVerification,
    )
    .await
    {
        Ok(token) => {
            if let Err(err) = email_service::send_email_verification(
                config,
                &email.email,
                &user.locale,
                &token,
            )
            .await
            {
                tracing::error!(error = ?err, user_id = %user.id, "failed to send verification email");
            }
        }
        Err(err) => {
            tracing::error!(error = ?err, user_id = %user.id, "failed to issue verification token");
        }
    }

    // Return both rows so the caller does not have to read them back.
    Ok(RegisteredUser { user, email })
}

pub async fn login(
    db: &DatabaseConnection,
    payload: LoginRequest,
    encoding_key: &EncodingKey,
    ctx: SessionContext,
) -> Result<LoggedInUser, AuthError> {
    let identifier = payload.username_or_email.trim();

    let (user, login_email) = if identifier.contains('@') {
        let normalized = normalize_email(identifier);

        let found = user_emails::Entity::find()
            .filter(user_emails::Column::Email.eq(normalized))
            .find_also_related(users::Entity)
            .one(db)
            .await?
            .and_then(|(email, user)| user.map(|u| (u, email)));

        match found {
            Some((user, email)) => (Some(user), Some(email)),
            None => (None, None),
        }
    } else {
        let user = users::Entity::find()
            .filter(users::Column::Username.eq(identifier))
            .one(db)
            .await?;
        (user, None)
    };

    // Use a dummy hash if the user does not exist, to avoid leaking existence information via timing.
    let hash = user
        .as_ref()
        .map(|u| u.password_hash.clone())
        .unwrap_or_else(|| password_service::DUMMY_HASH.clone());

    let valid_password = password_service::verify_password(payload.password, hash).await?;

    let user = match (user, valid_password) {
        (Some(user), true) => user,
        _ => return Err(AuthError::InvalidCredentials),
    };

    // Account state checks (email verified, account disabled, etc.)
    if let Some(suspension) = check_account_suspended(&user) {
        return Err(suspension);
    }

    let email = match login_email {
        Some(email) => email,
        None => primary_email(db, user.id)
            .await?
            .ok_or(AuthError::InvalidCredentials)?,
    };

    if email.verified_at.is_none() {
        return Err(AuthError::EmailNotVerified);
    }

    // Generate access and refresh tokens.
    let access_token = jwt_service::generate_access_token(user.id, encoding_key)?;
    let refresh_token = session_service::issue(db, user.id, ctx).await?;

    Ok(LoggedInUser {
        user,
        email,
        access_token,
        refresh_token: refresh_token.secret,
    })
}

pub async fn refresh(
    db: &DatabaseConnection,
    encoding_key: &EncodingKey,
    refresh_token: &str,
    ctx: SessionContext,
) -> Result<LoggedInUser, AuthError> {

    let new_refresh_token = session_service::rotate(db, refresh_token, ctx).await?;

    let user = users::Entity::find_by_id(new_refresh_token.user_id)
        .one(db)
        .await?
        .ok_or(AuthError::InvalidRefreshToken)?;

    if let Some(suspension) = check_account_suspended(&user) {
        return Err(suspension);
    }

    let email = primary_email(db, user.id)
        .await?
        .ok_or(AuthError::InvalidRefreshToken)?;

    let access_token = jwt_service::generate_access_token(user.id, encoding_key)?;

    Ok(LoggedInUser {
        user,
        email,
        access_token,
        refresh_token: new_refresh_token.secret,
    })
}

pub async fn logout(db: &DatabaseConnection, refresh_token: &str) -> Result<(), AuthError> {
    session_service::revoke(db, refresh_token, domain::types::RevokedReason::UserLogout).await?;

    Ok(())
}

pub async fn current_user(
    db: &DatabaseConnection,
    user_id: Uuid,
) -> Result<(users::Model, user_emails::Model), AuthError> {
    let user = users::Entity::find_by_id(user_id)
        .one(db)
        .await?
        .ok_or(AuthError::InvalidCredentials)?;

    let email = primary_email(db, user_id)
        .await?
        .ok_or(AuthError::InvalidCredentials)?;

    Ok((user, email))
}

pub async fn password_forgot(
    db: &DatabaseConnection,
    config: &Config,
    email: &str,
) -> Result<(), AuthError> {
    let email = normalize_email(email);

    let found = user_emails::Entity::find()
            .filter(user_emails::Column::Email.eq(email.clone()))
            .find_also_related(users::Entity)
            .one(db)
            .await?
            .and_then(|(email, user)| user.map(|u| (u, email)));

    match found {
        Some((user, email_row)) => {
            // Do not reveal whether the email exists. The caller should always get a success response.
            // Log the attempt for monitoring, but do not return an error to the user.
            if check_account_suspended(&user).is_some() {
                tracing::warn!("Password reset requested for suspended account: {} ({})", user.id, email);
                return Ok(());
            }

            if email_row.verified_at.is_none() {
                tracing::warn!("Password reset requested for unverified email: {} ({})", user.id, email);
                return Ok(());
            }

            let token = match auth_token_service::issue(
                db,
                user.id,
                email_row.id,
                domain::types::TokenKind::PasswordReset,
            )
            .await
            {
                Ok(token) => token,
                Err(err) => {
                    // Swallowed on purpose: propagating would make the response
                    // depend on whether the account exists.
                    tracing::error!(error = ?err, user_id = %user.id, "failed to issue reset token");
                    return Ok(());
                }
            };

            if let Err(err) =
                email_service::send_password_reset(config, &email_row.email, &user.locale, &token).await
            {
                tracing::error!(error = ?err, user_id = %user.id, "failed to send password reset email");
            }
        }
        None => {
            // Do not reveal whether the email exists. The caller should always get a success response.
            // Log the attempt for monitoring, but do not return an error to the user.
            tracing::warn!("Password reset requested for non-existent email: {}", email);
        }
    }

    Ok(())
}

/// Applies a new password from a reset link.
pub async fn password_reset(
    db: &DatabaseConnection,
    config: &Config,
    secret: &str,
    new_password: String,
    ctx: SessionContext,
) -> Result<(), AuthError> {
    // Check if the password is already compromised
    reject_if_compromised(config, &new_password).await?;

    let token = auth_token_service::consume(db, secret, domain::types::TokenKind::PasswordReset).await?;
    let user = users::Entity::find_by_id(token.user_id)
        .one(db)
        .await?
        .ok_or(AuthError::InvalidCredentials)?;

    let email_row = user_emails::Entity::find_by_id(
        token.email_id.ok_or(AuthError::InvalidCredentials)?,
    )
    .one(db)
    .await?
    .ok_or(AuthError::InvalidCredentials)?;

    let hashed_password = password_service::hash_password(new_password).await?;

    let txn = db.begin().await?;
    users::Entity::update_many()
        .col_expr(users::Column::PasswordHash, Expr::value(hashed_password))
        .filter(users::Column::Id.eq(user.id))
        .exec(&txn)
        .await?;

    session_service::revoke_all(&txn, user.id, domain::types::RevokedReason::PasswordReset, None).await?;

    txn.commit().await?;
    // TODO(2.13): audit entry, and notify the user by mail.
    // Best effort: a mail failure must not undo a completed reset.
    if let Err(err) = email_service::send_security_alert(
        config,
        &email_row.email,
        &user.locale,
        email_service::AlertKind::PasswordReset,
        &Utc::now().format("%Y-%m-%d %H:%M UTC").to_string(),
        ctx.ip_address.map(|ip| ip.ip().to_string()).as_deref(),
        ctx.user_agent.as_deref(),
    )
    .await
    {
        tracing::error!(error = ?err, user_id = %user.id, "failed to send password reset alert");
    }

    Ok(())
}

/// Changes the password of a signed-in user.
pub async fn password_change(
    db: &DatabaseConnection,
    config: &Config,
    user_id: Uuid,
    current_refresh_token: Option<&str>,
    old_password: String,
    new_password: String,
    ctx: SessionContext,
) -> Result<(), AuthError> {
    let user = users::Entity::find_by_id(user_id)
        .one(db)
        .await?
        .ok_or(AuthError::InvalidCredentials)?;

    let email = primary_email(db, user.id)
        .await?
        .ok_or(AuthError::InvalidCredentials)?;

    if old_password == new_password {
        return Err(AuthError::PasswordUnchanged);
    }

    // Check if the password is already compromised
    reject_if_compromised(config, &new_password).await?;

    if !password_service::verify_password(old_password, user.password_hash).await? {
        return Err(AuthError::InvalidCredentials);
    }


    let hashed_password = password_service::hash_password(new_password).await?;

    let txn = db.begin().await?;

    // Resolve the session to spare, if the caller sent one. The ownership
    // check matters: without it, presenting another account's cookie would
    // spare a session that is not ours.
    let except = match current_refresh_token {
        Some(secret) => session_service::find_session_id(db, secret, user_id).await?,
        None => None,
    };

    users::Entity::update_many()
        .col_expr(users::Column::PasswordHash, Expr::value(hashed_password))
        .filter(users::Column::Id.eq(user.id))
        .exec(&txn)
        .await?;

    session_service::revoke_all(&txn, user.id, domain::types::RevokedReason::PasswordChange, except).await?;

    txn.commit().await?;
    // TODO(2.13): audit entry and notification mail.
    // Best effort: a mail failure must not undo a completed reset.
    if let Err(err) = email_service::send_security_alert(
        config,
        &email.email,
        &user.locale,
        email_service::AlertKind::PasswordChanged,
        &Utc::now().format("%Y-%m-%d %H:%M UTC").to_string(),
        ctx.ip_address.map(|ip| ip.ip().to_string()).as_deref(),
        ctx.user_agent.as_deref(),
    )
    .await
    {
        tracing::error!(error = ?err, user_id = %user.id, "failed to send password reset alert");
    }

    Ok(())
}

/// Marks an address as verified.
pub async fn verify_email(db: &DatabaseConnection, secret: &str) -> Result<(), AuthError> {
    let token = auth_token_service::consume(db, secret, domain::types::TokenKind::EmailVerification).await?;

    let email_id = token.email_id.ok_or(AuthError::InvalidCredentials)?;

    // Idempotent by filtering on NULL: clicking the link twice is not an
    // error, and the second click must not overwrite the original timestamp.
    user_emails::Entity::update_many()
        .col_expr(user_emails::Column::VerifiedAt, Expr::current_timestamp())
        .filter(user_emails::Column::Id.eq(email_id))
        .filter(user_emails::Column::VerifiedAt.is_null())
        .exec(db)
        .await?;

    Ok(())
}

/// Re-sends the verification link for the caller's primary address.
///
/// Takes a user_id rather than an address: the route is authenticated, so
/// there is nothing to enumerate and no way to aim the mail at a third party.
pub async fn resend_verification(
    db: &DatabaseConnection,
    config: &Config,
    user_id: Uuid,
) -> Result<(), AuthError> {
    let email = primary_email(db, user_id)
        .await?
        .ok_or(AuthError::InvalidCredentials)?;

    if email.verified_at.is_some() {
        return Ok(());
    }

    let user = users::Entity::find_by_id(user_id)
        .one(db)
        .await?
        .ok_or(AuthError::InvalidCredentials)?;

    let secret = auth_token_service::issue(
        db,
        user_id,
        email.id,
        domain::types::TokenKind::EmailVerification,
    )
    .await?;

    email_service::send_email_verification(config, &email.email, &user.locale, &secret).await?;

    Ok(())
}


pub(crate) fn suspension_error(
    suspended_at: Option<DateTimeWithTimeZone>,
    suspended_until: Option<DateTimeWithTimeZone>,
    reason: Option<String>,
) -> Option<AuthError> {
    suspended_at?;

    match suspended_until {
        Some(until) if until <= Utc::now() => None,
        until => Some(AuthError::AccountSuspended { until, reason }),
    }
}

pub(crate) async fn validate_new_account(
    db: &DatabaseConnection,
    config: &Config,
    username: &str,
    email: &str,
    password: &str,
) -> Result<(), AuthError> {
    if is_reserved_username(db, username).await? {
        return Err(AuthError::UsernameReserved);
    }

    let domain = email
        .rsplit_once('@')
        .map(|(_, d)| d)
        .ok_or(AuthError::InvalidEmail)?;

    blocked_email_service::check_domain(db, domain, config.block_disposable_emails).await?;

    // Advisory only: the UNIQUE CITEXT constraints are the real guarantee and
    // also cover the window between these queries and the inserts.
    if username_exists(db, username).await? {
        return Err(AuthError::UsernameTaken);
    }
    if email_exists(db, email).await? {
        return Err(AuthError::EmailTaken);
    }

    reject_if_compromised(config, password).await?;

    Ok(())
}

pub async fn is_reserved_username(db: &DatabaseConnection, username: &str) -> Result<bool, DbErr> {
    reserved_usernames::Entity::find_by_id(username)
        .one(db)
        .await
        .map(|found| found.is_some())
}

/// Lowercases the domain only: the local part is case-sensitive per RFC 5321.
pub fn normalize_email(raw: &str) -> String {
    let trimmed = raw.trim();
    match trimmed.rsplit_once('@') {
        Some((local, domain)) => format!("{local}@{}", domain.to_lowercase()),
        None => trimmed.to_owned(),
    }
}

pub async fn username_exists(db: &DatabaseConnection, username: &str) -> Result<bool, DbErr> {
    users::Entity::find()
        .filter(users::Column::Username.eq(username))
        .one(db)
        .await
        .map(|found| found.is_some())
}

pub async fn email_exists(db: &DatabaseConnection, email: &str) -> Result<bool, DbErr> {
    user_emails::Entity::find()
        .filter(user_emails::Column::Email.eq(email))
        .one(db)
        .await
        .map(|found| found.is_some())
}

pub async fn primary_email(
    db: &DatabaseConnection,
    user_id: Uuid,
) -> Result<Option<user_emails::Model>, DbErr> {
    user_emails::Entity::find()
        .filter(user_emails::Column::UserId.eq(user_id))
        .filter(user_emails::Column::IsPrimary.eq(true))
        .one(db)
        .await
}

async fn reject_if_compromised(config: &Config, password: &str) -> Result<(), AuthError> {
    if !config.check_pwned_passwords {
        return Ok(());
    }

    match password_service::is_compromised(password).await {
        Ok(true) => Err(AuthError::PasswordCompromised),
        Ok(false) => Ok(()),
        Err(err) => {
            tracing::warn!(error = ?err, "pwned password check unavailable");
            Ok(())
        }
    }
}

fn check_account_suspended(user: &users::Model) -> Option<AuthError> {
    suspension_error(
        user.suspended_at,
        user.suspended_until,
        user.suspended_reason.clone(),
    )
}

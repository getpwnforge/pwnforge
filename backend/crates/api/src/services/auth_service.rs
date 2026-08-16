// crates/api/src/services/auth_service.rs
use super::{blocked_email_service, jwt_service, password_service, token_service};
use crate::config::Config;
use crate::services::blocked_email_service::BlockedEmailError;
use chrono::Utc;
use domain::{
    dto::auth::{LoggedInUser, LoginRequest, RegisterRequest, RegisteredUser},
    entities::{refresh_tokens, reserved_usernames, user_emails, users},
};
use jsonwebtoken::EncodingKey;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
    TransactionTrait,
    prelude::{DateTimeWithTimeZone, IpNetwork},
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

    #[error("refresh token is invalid or expired")]
    InvalidRefreshToken,

    #[error(transparent)]
    Jwt(#[from] jwt_service::JwtError),

    #[error(transparent)]
    Db(#[from] sea_orm::DbErr),
}

pub async fn register(
    db: &DatabaseConnection,
    config: &Config,
    payload: RegisterRequest,
) -> Result<RegisteredUser, AuthError> {
    // Normalize inputs. Only the domain part of the email is lowercased:
    // the local part is case-sensitive per RFC 5321.
    let email = normalize_email(&payload.email);
    let username = payload.username.trim().to_owned();

    // Reject reserved usernames. No I/O, so it comes first.
    if is_reserved_username(db, &username).await? {
        return Err(AuthError::UsernameReserved);
    }

    // Check the email domain against the block/allow lists.
    // blocked_email_service::check_domain(db, domain, config.block_disposable_emails)
    let domain = email
        .rsplit_once('@')
        .map(|(_, d)| d)
        .ok_or(AuthError::InvalidEmail)?;

    blocked_email_service::check_domain(db, domain, config.block_disposable_emails).await?;

    // Advisory uniqueness checks, for clear error messages only.
    // The UNIQUE CITEXT constraints are the real guarantee and also cover
    // the TOCTOU window between these queries and the inserts below.
    if username_exists(db, &username).await? {
        return Err(AuthError::UsernameTaken);
    }
    if email_exists(db, &email).await? {
        return Err(AuthError::EmailTaken);
    }

    // Hash the password. Done last: Argon2 costs ~100ms with OWASP params,
    // no point spending it on a registration that was going to fail.
    let password_hash = password_service::hash_password(payload.password).await?;

    // Open the transaction. Both inserts must succeed together: a user with
    // no primary email cannot log in.
    let transaction = db.begin().await?;

    // Insert the user (id = Uuid::now_v7()).
    let user = users::ActiveModel {
        id: Set(Uuid::now_v7()),
        username: Set(username),
        password_hash: Set(password_hash),
        ..Default::default()
    }
    .insert(&transaction)
    .await
    .map_err(map_unique_violation)?;

    // Insert the primary email. verified_at stays NULL until the 2.12 flow.
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

    // Return both rows so the caller does not have to read them back.
    Ok(RegisteredUser { user, email })
}

pub async fn login(
    db: &DatabaseConnection,
    payload: LoginRequest,
    encoding_key: &jwt_service::EncodingKey,
    user_agent: Option<String>,
    ip_address: Option<IpNetwork>,
) -> Result<LoggedInUser, AuthError> {
    let identifier = payload.username_or_email.trim();

    let (user, login_email) = if identifier.contains('@') {
        let normalized = normalize_email(identifier);

        // Two separate queries: find_also_related trips over the citext select_as
        // aliasing in sea-orm 2.0.
        let email = user_emails::Entity::find()
            .filter(user_emails::Column::Email.eq(normalized))
            .one(db)
            .await?;

        match email {
            Some(email) => {
                let user = users::Entity::find_by_id(email.user_id).one(db).await?;
                (user, Some(email))
            }
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
    let refresh_token = token_service::generate_opaque_token();

    // Store the refresh token in the database, associated with the user.
    refresh_tokens::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(user.id),
        token_hash: Set(refresh_token.hash),
        expires_at: Set((Utc::now()
            + chrono::Duration::seconds(jwt_service::REFRESH_TOKEN_TTL_SECS))
        .into()),
        user_agent: Set(user_agent),
        ip_address: Set(ip_address),
        last_used_at: Set(Some(Utc::now().into())),
        ..Default::default()
    }
    .insert(db)
    .await?;

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
    user_agent: Option<String>, // 3. carried over on rotation
    ip_address: Option<IpNetwork>,
) -> Result<LoggedInUser, AuthError> {
    let token_hash = token_service::hash_opaque_token(refresh_token);

    let token = refresh_tokens::Entity::find()
        .filter(refresh_tokens::Column::TokenHash.eq(token_hash))
        .one(db)
        .await?
        .ok_or(AuthError::InvalidRefreshToken)?;

    // 1. Reuse of a revoked token means the chain has been duplicated: either an
    //    attacker or the legitimate user is replaying a copy. Revoke every live
    //    token of this user so both are forced to re-authenticate.
    if token.revoked_at.is_some() {
        refresh_tokens::Entity::update_many()
            .col_expr(refresh_tokens::Column::RevokedAt, Expr::current_timestamp())
            .col_expr(
                refresh_tokens::Column::RevokedReason,
                Expr::cust("'resuse_detected'"),
            )
            .filter(refresh_tokens::Column::UserId.eq(token.user_id))
            .filter(refresh_tokens::Column::RevokedAt.is_null())
            .exec(db)
            .await?;

        tracing::warn!(
            user_id = %token.user_id,
            "refresh token reuse detected, all sessions revoked"
        );

        // TODO(2.14): record this in instance_audit_logs and notify the user.
        return Err(AuthError::InvalidRefreshToken);
    }

    if token.expires_at < Utc::now() {
        return Err(AuthError::InvalidRefreshToken);
    }

    let user = users::Entity::find_by_id(token.user_id)
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
    let new_refresh_token = token_service::generate_opaque_token();

    // 2. Both writes must land together: rotating without revoking would leave
    //    two live tokens on the same chain.
    let txn = db.begin().await?;

    refresh_tokens::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(user.id),
        token_hash: Set(new_refresh_token.hash),
        expires_at: Set((Utc::now()
            + chrono::Duration::seconds(jwt_service::REFRESH_TOKEN_TTL_SECS))
        .into()),
        user_agent: Set(user_agent),
        ip_address: Set(ip_address),
        last_used_at: Set(Some(Utc::now().into())),
        ..Default::default()
    }
    .insert(&txn)
    .await?;

    // 4. Record when the rotated token was last used before retiring it.
    refresh_tokens::ActiveModel {
        id: Set(token.id),
        revoked_at: Set(Some(Utc::now().into())),
        last_used_at: Set(Some(Utc::now().into())),
        revoked_reason: Set(Some("token_rotation".to_owned())),
        ..Default::default()
    }
    .update(&txn)
    .await?;

    txn.commit().await?;

    Ok(LoggedInUser {
        user,
        email,
        access_token,
        refresh_token: new_refresh_token.secret,
    })
}

pub async fn logout(db: &DatabaseConnection, refresh_token: &str) -> Result<(), AuthError> {
    let token_hash = token_service::hash_opaque_token(refresh_token);

    let token = refresh_tokens::Entity::find()
        .filter(refresh_tokens::Column::TokenHash.eq(token_hash))
        .one(db)
        .await?
        .ok_or(AuthError::InvalidRefreshToken)?;

    if token.revoked_at.is_some() {
        return Err(AuthError::InvalidRefreshToken);
    }

    refresh_tokens::ActiveModel {
        id: Set(token.id),
        revoked_at: Set(Some(Utc::now().into())),
        last_used_at: Set(Some(Utc::now().into())),
        revoked_reason: Set(Some("user_logout".to_owned())),
        ..Default::default()
    }
    .update(db)
    .await?;

    Ok(())
}

async fn is_reserved_username(db: &DatabaseConnection, username: &str) -> Result<bool, DbErr> {
    reserved_usernames::Entity::find_by_id(username)
        .one(db)
        .await
        .map(|found| found.is_some())
}

/// Lowercases the domain only: the local part is case-sensitive per RFC 5321.
fn normalize_email(raw: &str) -> String {
    let trimmed = raw.trim();
    match trimmed.rsplit_once('@') {
        Some((local, domain)) => format!("{local}@{}", domain.to_lowercase()),
        None => trimmed.to_owned(),
    }
}

async fn username_exists(db: &DatabaseConnection, username: &str) -> Result<bool, DbErr> {
    users::Entity::find()
        .filter(users::Column::Username.eq(username))
        .one(db)
        .await
        .map(|found| found.is_some())
}

async fn email_exists(db: &DatabaseConnection, email: &str) -> Result<bool, DbErr> {
    user_emails::Entity::find()
        .filter(user_emails::Column::Email.eq(email))
        .one(db)
        .await
        .map(|found| found.is_some())
}

async fn primary_email(
    db: &DatabaseConnection,
    user_id: Uuid,
) -> Result<Option<user_emails::Model>, DbErr> {
    user_emails::Entity::find()
        .filter(user_emails::Column::UserId.eq(user_id))
        .filter(user_emails::Column::IsPrimary.eq(true))
        .one(db)
        .await
}

fn check_account_suspended(user: &users::Model) -> Option<AuthError> {
    user.suspended_at?;

    match user.suspended_until {
        Some(until) if until <= Utc::now() => None, // suspension expired
        until => Some(AuthError::AccountSuspended {
            until,
            reason: user.suspended_reason.clone(),
        }),
    }
}

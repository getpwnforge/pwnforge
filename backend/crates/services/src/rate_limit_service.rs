// crates/services/src/rate_limit_service.rs
use crate::token_service::hash_opaque_token;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use std::net::IpAddr;
use uuid::Uuid;

// --- Register ---
const REGISTER_IP_LIMIT: isize = 5;
const REGISTER_WINDOW_SECS: i64 = 60 * 60;

// --- Login ---
const LOGIN_IDENTIFIER_LIMIT: isize = 10;
const LOGIN_IP_LIMIT: isize = 30;
const LOGIN_WINDOW_SECS: i64 = 15 * 60;

// --- Password reset request ---
// Lower and longer than login: every request that gets through sends a mail to
// an address the caller does not have to prove they own.
const FORGOT_EMAIL_LIMIT: isize = 3;
const FORGOT_IP_LIMIT: isize = 10;
const FORGOT_WINDOW_SECS: i64 = 60 * 60;

// --- Password reset submission ---
// Only bounds the Argon2 cost an unauthenticated caller can trigger: the token
// itself is 256 bits, so this is not what stops a brute force.
const RESET_IP_LIMIT: isize = 20;
const RESET_WINDOW_SECS: i64 = 60 * 60;

// --- Verification email resend ---
// Two windows on the same key: a short one so the button cannot be hammered,
// a long one so the daily volume stays bounded.
const RESEND_BURST_LIMIT: isize = 1;
const RESEND_BURST_WINDOW_SECS: i64 = 60;
const RESEND_HOURLY_LIMIT: isize = 5;
const RESEND_HOURLY_WINDOW_SECS: i64 = 60 * 60;
// And one per IP, checked first: the token windows key on whatever string
// the caller sends, so on their own every made-up token gets fresh ones.
const RESEND_IP_LIMIT: isize = 20;
const RESEND_IP_WINDOW_SECS: i64 = 60 * 60;

// --- Email verification ---
// Bounds the lookups an unauthenticated caller can trigger: the token is
// 256 bits, so this is not what stops a guess.
const VERIFY_IP_LIMIT: isize = 30;
const VERIFY_IP_WINDOW_SECS: i64 = 60 * 60;

// --- Password change ---
// Per account: every attempt runs Argon2 on the current password, so an
// unmetered route is a CPU sink for any signed-in account.
const PASSWORD_CHANGE_USER_LIMIT: isize = 10;
const PASSWORD_CHANGE_WINDOW_SECS: i64 = 60 * 60;

// --- Setup ---
// The setup wizard is a single endpoint that creates the first administrator and instance settings.
const SETUP_IP_LIMIT: isize = 5;
const SETUP_WINDOW_SECS: i64 = 60 * 60;
const SETUP_TEST_EMAIL_IP_LIMIT: isize = 5;
const SETUP_TEST_EMAIL_WINDOW_SECS: i64 = 60 * 60;
// Roomier than the completion limit: this call creates nothing, and an operator
// picking a password the breach corpus keeps rejecting will legitimately hit it
// several times in a row. Still bounded - every request reaches out to HIBP.
const SETUP_VALIDATE_IP_LIMIT: isize = 30;
const SETUP_VALIDATE_WINDOW_SECS: i64 = 60 * 60;

// --- Contact form ---
const CONTACT_IP_LIMIT: isize = 3;
const CONTACT_WINDOW_SECS: i64 = 60 * 60;

// --- Avatar upload ---
// Two windows on the same user, for two different abuses.
//
// The per-user hourly window bounds storage: every accepted upload writes two
// objects and best-effort deletes two more, so an unbounded one is a way to
// fill someone else's bucket and their bill.
//
// The burst window bounds CPU, which is the one that actually matters here.
// An avatar upload is the only authenticated route that hands attacker-chosen
// bytes to an image decoder, and a decode is orders of magnitude more
// expensive than the request that carried it. Ten a minute is well past what a
// person adjusting their profile picture does and well under what it takes to
// occupy the blocking pool.
//
// Keyed on the user, not the IP: the route is authenticated, so the account is
// the stable identifier and an attacker rotating IPs gains nothing. An IP
// window would also punish a whole NAT for one account.
const AVATAR_UPLOAD_BURST_LIMIT: isize = 10;
const AVATAR_UPLOAD_BURST_WINDOW_SECS: i64 = 60;
const AVATAR_UPLOAD_HOURLY_LIMIT: isize = 30;
const AVATAR_UPLOAD_HOURLY_WINDOW_SECS: i64 = 60 * 60;

// --- Targeted invitations (teams and workspaces) ---
// Two windows per flow: one on the author, so one account cannot spray
// invitations, one on the team or the workspace, so several of its members
// together cannot either. The email path is tighter than the username path:
// each invitation there mails an address nobody had to prove they own.
//
// One set of keys per flow (team by username, team by email, workspace by
// username, workspace by email): a burst on one never eats another's budget.
const INVITE_BY_USERNAME_AUTHOR_LIMIT: isize = 20;
const INVITE_BY_USERNAME_SCOPE_LIMIT: isize = 50;
const INVITE_BY_EMAIL_AUTHOR_LIMIT: isize = 10;
const INVITE_BY_EMAIL_SCOPE_LIMIT: isize = 30;
const INVITE_WINDOW_SECS: i64 = 60 * 60;

// --- Invitation mail to one address ---
// Across every team and every workspace, whoever sends it: the bound that
// stops several teams from flooding one inbox. Past it the invitation is
// still created, only its mail is not sent.
const INVITE_MAIL_ADDRESS_LIMIT: isize = 3;
const INVITE_MAIL_ADDRESS_WINDOW_SECS: i64 = 24 * 60 * 60;

pub enum RateLimitDecision {
    Allowed,
    Limited { retry_after_secs: u64 },
}

/// Registration attempts, keyed on IP only: there is no stable identifier
/// before the account exists. Deliberately looser than login, since a shared
/// NAT or a classroom can legitimately produce several signups in a row.
pub async fn check_register_attempt(
    redis: &mut ConnectionManager,
    ip: IpAddr,
) -> Result<RateLimitDecision, redis::RedisError> {
    check_window(
        redis,
        &register_ip_key(ip),
        REGISTER_IP_LIMIT,
        REGISTER_WINDOW_SECS,
    )
    .await
}

/// Login, per IP: every attempt counts, checked before anything else.
pub async fn check_login_ip(
    redis: &mut ConnectionManager,
    ip: IpAddr,
) -> Result<RateLimitDecision, redis::RedisError> {
    check_window(redis, &login_ip_key(ip), LOGIN_IP_LIMIT, LOGIN_WINDOW_SECS).await
}

/// Login, per identifier: whether the submitted username or email is over its
/// share of failed attempts, read without counting this one.
///
/// Only failures count (see `record_login_failure`), and only those that got
/// past Turnstile: anyone may send a request naming any account, so a window
/// every request filled was a way to lock a known account out with eleven
/// requests and no password. The identifier is lowercased so a case-varying
/// caller cannot spread failures across buckets, and keyed on the submitted
/// string, not the resolved account: someone who knows both the username and
/// the email of a target gets two quotas. Resolving the account first would
/// close that gap at the cost of a database round-trip on every attempt; the
/// IP window covers the remainder.
pub async fn login_identifier_blocked(
    redis: &mut ConnectionManager,
    identifier: &str,
) -> Result<RateLimitDecision, redis::RedisError> {
    peek_window(
        redis,
        &login_identifier_key(identifier),
        LOGIN_IDENTIFIER_LIMIT,
    )
    .await
}

/// Counts one failed password for the identifier.
pub async fn record_login_failure(
    redis: &mut ConnectionManager,
    identifier: &str,
) -> Result<(), redis::RedisError> {
    check_window(
        redis,
        &login_identifier_key(identifier),
        LOGIN_IDENTIFIER_LIMIT,
        LOGIN_WINDOW_SECS,
    )
    .await
    .map(|_| ())
}

/// Clears the per-identifier window after a successful login.
///
/// Only the identifier bucket is cleared, not the per-IP one: resetting the IP
/// bucket on any success would let an attacker who owns one valid account buy a
/// fresh IP-wide budget by logging into it, defeating the point of that window,
/// which is to catch a single IP spraying attempts across many identifiers.
pub async fn reset_login_attempts(
    redis: &mut ConnectionManager,
    identifier: &str,
) -> Result<(), redis::RedisError> {
    let _: () = redis.del(login_identifier_key(identifier)).await?;
    Ok(())
}

/// Password reset requests, per IP: checked first, on every request.
///
/// The caller must treat a Redis failure as a refusal rather than swallowing
/// it, here and for the address window: unlike login, there is no password
/// check underneath this endpoint, so failing open turns it into a free
/// mailer aimed at arbitrary addresses.
pub async fn check_password_forgot_ip(
    redis: &mut ConnectionManager,
    ip: IpAddr,
) -> Result<RateLimitDecision, redis::RedisError> {
    check_window(
        redis,
        &forgot_ip_key(ip),
        FORGOT_IP_LIMIT,
        FORGOT_WINDOW_SECS,
    )
    .await
}

/// Password reset requests, per address: checked after Turnstile, so a
/// request that fails it does not spend the share of the address it names.
/// Before, three requests without a challenge kept anyone from resetting the
/// password of an address they knew.
pub async fn check_password_forgot_email(
    redis: &mut ConnectionManager,
    email: &str,
) -> Result<RateLimitDecision, redis::RedisError> {
    check_window(
        redis,
        &forgot_email_key(email),
        FORGOT_EMAIL_LIMIT,
        FORGOT_WINDOW_SECS,
    )
    .await
}

pub async fn check_password_reset(
    redis: &mut ConnectionManager,
    ip: IpAddr,
) -> Result<RateLimitDecision, redis::RedisError> {
    check_window(redis, &reset_ip_key(ip), RESET_IP_LIMIT, RESET_WINDOW_SECS).await
}

/// Verification resend, per IP, checked before the token windows.
pub async fn check_email_resend_ip(
    redis: &mut ConnectionManager,
    ip: IpAddr,
) -> Result<RateLimitDecision, redis::RedisError> {
    check_window(
        redis,
        &email_resend_ip_key(ip),
        RESEND_IP_LIMIT,
        RESEND_IP_WINDOW_SECS,
    )
    .await
}

/// Email verification, per IP.
pub async fn check_email_verify_ip(
    redis: &mut ConnectionManager,
    ip: IpAddr,
) -> Result<RateLimitDecision, redis::RedisError> {
    check_window(
        redis,
        &email_verify_ip_key(ip),
        VERIFY_IP_LIMIT,
        VERIFY_IP_WINDOW_SECS,
    )
    .await
}

/// Password change, per account. The caller fails closed, as for the avatar
/// upload: any account can reach the route, and each attempt is an Argon2
/// verification.
pub async fn check_password_change(
    redis: &mut ConnectionManager,
    user_id: Uuid,
) -> Result<RateLimitDecision, redis::RedisError> {
    check_window(
        redis,
        &password_change_key(user_id),
        PASSWORD_CHANGE_USER_LIMIT,
        PASSWORD_CHANGE_WINDOW_SECS,
    )
    .await
}

pub async fn check_email_resend(
    redis: &mut ConnectionManager,
    token: &str,
) -> Result<RateLimitDecision, redis::RedisError> {
    let token_hash = hash_opaque_token(token);
    let by_user = check_window(
        redis,
        &email_resend_key(&token_hash),
        RESEND_HOURLY_LIMIT,
        RESEND_HOURLY_WINDOW_SECS,
    )
    .await?;

    let by_user_min = check_window(
        redis,
        &email_resend_burst_key(&token_hash),
        RESEND_BURST_LIMIT,
        RESEND_BURST_WINDOW_SECS,
    )
    .await?;

    Ok(most_restrictive(by_user, by_user_min))
}

pub async fn check_setup_attempt(
    redis: &mut ConnectionManager,
    ip: IpAddr,
) -> Result<RateLimitDecision, redis::RedisError> {
    check_window(redis, &setup_ip_key(ip), SETUP_IP_LIMIT, SETUP_WINDOW_SECS).await
}

pub async fn check_setup_test_email(
    redis: &mut ConnectionManager,
    ip: IpAddr,
) -> Result<RateLimitDecision, redis::RedisError> {
    check_window(
        redis,
        &setup_test_email_ip_key(ip),
        SETUP_TEST_EMAIL_IP_LIMIT,
        SETUP_TEST_EMAIL_WINDOW_SECS,
    )
    .await
}

pub async fn check_setup_validate(
    redis: &mut ConnectionManager,
    ip: IpAddr,
) -> Result<RateLimitDecision, redis::RedisError> {
    check_window(
        redis,
        &setup_validate_ip_key(ip),
        SETUP_VALIDATE_IP_LIMIT,
        SETUP_VALIDATE_WINDOW_SECS,
    )
    .await
}

pub async fn check_contact_attempt(
    redis: &mut ConnectionManager,
    ip: IpAddr,
) -> Result<RateLimitDecision, redis::RedisError> {
    check_window(
        redis,
        &contact_ip_key(ip),
        CONTACT_IP_LIMIT,
        CONTACT_WINDOW_SECS,
    )
    .await
}

/// Rate limit for avatar uploads, its own function rather than a generic
/// helper: every limit in this module names the route it protects, so the
/// windows that guard it are visible in one place and changing one cannot
/// silently change another.
///
/// Both windows are always incremented, as `check_login_attempt` does, so the
/// counts stay accurate regardless of which one is checked first.
pub async fn check_avatar_upload(
    redis: &mut ConnectionManager,
    user_id: Uuid,
) -> Result<RateLimitDecision, redis::RedisError> {
    let burst = check_window(
        redis,
        &avatar_upload_burst_key(user_id),
        AVATAR_UPLOAD_BURST_LIMIT,
        AVATAR_UPLOAD_BURST_WINDOW_SECS,
    )
    .await?;

    let hourly = check_window(
        redis,
        &avatar_upload_hourly_key(user_id),
        AVATAR_UPLOAD_HOURLY_LIMIT,
        AVATAR_UPLOAD_HOURLY_WINDOW_SECS,
    )
    .await?;

    Ok(most_restrictive(burst, hourly))
}

/// Team invitation by username. Both windows are always incremented, as
/// `check_login_attempt` does. The caller fails closed: a member of any team
/// could otherwise send unmetered invitations.
pub async fn check_team_invitation_by_username(
    redis: &mut ConnectionManager,
    author_id: Uuid,
    team_id: Uuid,
) -> Result<RateLimitDecision, redis::RedisError> {
    check_invitation_windows(
        redis,
        &invite_author_key("team", "username", author_id),
        &invite_scope_key("team", "username", team_id),
        INVITE_BY_USERNAME_AUTHOR_LIMIT,
        INVITE_BY_USERNAME_SCOPE_LIMIT,
    )
    .await
}

/// Team invitation by email address; see `check_team_invitation_by_username`.
pub async fn check_team_invitation_by_email(
    redis: &mut ConnectionManager,
    author_id: Uuid,
    team_id: Uuid,
) -> Result<RateLimitDecision, redis::RedisError> {
    check_invitation_windows(
        redis,
        &invite_author_key("team", "email", author_id),
        &invite_scope_key("team", "email", team_id),
        INVITE_BY_EMAIL_AUTHOR_LIMIT,
        INVITE_BY_EMAIL_SCOPE_LIMIT,
    )
    .await
}

/// Workspace invitation by username; see `check_team_invitation_by_username`.
pub async fn check_workspace_invitation_by_username(
    redis: &mut ConnectionManager,
    author_id: Uuid,
    workspace_id: Uuid,
) -> Result<RateLimitDecision, redis::RedisError> {
    check_invitation_windows(
        redis,
        &invite_author_key("workspace", "username", author_id),
        &invite_scope_key("workspace", "username", workspace_id),
        INVITE_BY_USERNAME_AUTHOR_LIMIT,
        INVITE_BY_USERNAME_SCOPE_LIMIT,
    )
    .await
}

/// Workspace invitation by email address; see
/// `check_team_invitation_by_username`.
pub async fn check_workspace_invitation_by_email(
    redis: &mut ConnectionManager,
    author_id: Uuid,
    workspace_id: Uuid,
) -> Result<RateLimitDecision, redis::RedisError> {
    check_invitation_windows(
        redis,
        &invite_author_key("workspace", "email", author_id),
        &invite_scope_key("workspace", "email", workspace_id),
        INVITE_BY_EMAIL_AUTHOR_LIMIT,
        INVITE_BY_EMAIL_SCOPE_LIMIT,
    )
    .await
}

/// Whether one more invitation mail may go to `email` today, counted across
/// every team and workspace. Unlike the others this never refuses a request:
/// the caller creates the invitation either way and only skips the mail, so
/// the answer is the same whether the address was flooded or not. A Redis
/// failure is the caller's to treat as a refusal, which here means no mail.
pub async fn check_invitation_mail_to_address(
    redis: &mut ConnectionManager,
    email: &str,
) -> Result<RateLimitDecision, redis::RedisError> {
    check_window(
        redis,
        &invite_mail_address_key(email),
        INVITE_MAIL_ADDRESS_LIMIT,
        INVITE_MAIL_ADDRESS_WINDOW_SECS,
    )
    .await
}

async fn check_invitation_windows(
    redis: &mut ConnectionManager,
    author_key: &str,
    scope_key: &str,
    author_limit: isize,
    scope_limit: isize,
) -> Result<RateLimitDecision, redis::RedisError> {
    let by_author = check_window(redis, author_key, author_limit, INVITE_WINDOW_SECS).await?;
    let by_scope = check_window(redis, scope_key, scope_limit, INVITE_WINDOW_SECS).await?;
    Ok(most_restrictive(by_author, by_scope))
}

/// Returns the blocking decision when either window tripped, keeping the
/// longest wait so a client that retries at the returned delay is not refused
/// again by the other window.
fn most_restrictive(a: RateLimitDecision, b: RateLimitDecision) -> RateLimitDecision {
    use RateLimitDecision::*;

    match (a, b) {
        (
            Limited {
                retry_after_secs: x,
            },
            Limited {
                retry_after_secs: y,
            },
        ) => Limited {
            retry_after_secs: x.max(y),
        },
        (Limited { retry_after_secs }, _) | (_, Limited { retry_after_secs }) => {
            Limited { retry_after_secs }
        }
        _ => Allowed,
    }
}

fn register_ip_key(ip: IpAddr) -> String {
    format!("ratelimit:register:ip:{ip}")
}

/// Hashed rather than stored raw: the identifier is often an email address, and
/// Redis key names should not become another place PII sits at rest.
fn login_identifier_key(identifier: &str) -> String {
    format!(
        "ratelimit:login:identifier:{}",
        hash_opaque_token(&identifier.trim().to_lowercase())
    )
}

fn login_ip_key(ip: IpAddr) -> String {
    format!("ratelimit:login:ip:{ip}")
}

fn forgot_email_key(email: &str) -> String {
    format!(
        "ratelimit:forgot:email:{}",
        hash_opaque_token(&email.trim().to_lowercase())
    )
}

fn forgot_ip_key(ip: IpAddr) -> String {
    format!("ratelimit:forgot:ip:{ip}")
}

fn reset_ip_key(ip: IpAddr) -> String {
    format!("ratelimit:reset:ip:{ip}")
}

fn email_resend_ip_key(ip: IpAddr) -> String {
    format!("ratelimit:resend:ip:{ip}")
}

fn email_verify_ip_key(ip: IpAddr) -> String {
    format!("ratelimit:verify:ip:{ip}")
}

fn password_change_key(user_id: Uuid) -> String {
    format!("ratelimit:password_change:user:{user_id}")
}

fn email_resend_key(token_hash: &str) -> String {
    format!("ratelimit:resend:token:{token_hash}")
}

fn email_resend_burst_key(token_hash: &str) -> String {
    format!("ratelimit:resend:burst:token:{token_hash}")
}

fn setup_ip_key(ip: IpAddr) -> String {
    format!("ratelimit:setup:ip:{ip}")
}

fn setup_test_email_ip_key(ip: IpAddr) -> String {
    format!("ratelimit:setup:test_email:ip:{ip}")
}

fn setup_validate_ip_key(ip: IpAddr) -> String {
    format!("ratelimit:setup:validate:ip:{ip}")
}

fn contact_ip_key(ip: IpAddr) -> String {
    format!("ratelimit:contact:ip:{ip}")
}

// Not hashed, unlike the login and password-reset keys: a user id is already
// an opaque uuid rather than PII, and keeping it readable makes a stuck limit
// something an operator can actually investigate.
fn avatar_upload_burst_key(user_id: Uuid) -> String {
    format!("ratelimit:avatar:burst:user:{user_id}")
}

fn avatar_upload_hourly_key(user_id: Uuid) -> String {
    format!("ratelimit:avatar:hourly:user:{user_id}")
}

// Readable, as the avatar keys are: both ids are opaque uuids.
fn invite_author_key(scope: &str, path: &str, author_id: Uuid) -> String {
    format!("ratelimit:invite:{scope}:{path}:author:{author_id}")
}

fn invite_scope_key(scope: &str, path: &str, scope_id: Uuid) -> String {
    format!("ratelimit:invite:{scope}:{path}:{scope}:{scope_id}")
}

/// Hashed, as the login and password-reset keys are: the address is PII.
/// Trimmed and lowercased first, as the citext columns compare it, so
/// changing its case does not buy a fresh budget.
fn invite_mail_address_key(email: &str) -> String {
    format!(
        "ratelimit:invite:mail:address:{}",
        hash_opaque_token(&email.trim().to_lowercase())
    )
}

/// Reads a fixed window without counting this request: limited once the
/// window already holds `limit`, so the request that would make it go over
/// is the one refused. Absent means nothing counted yet.
async fn peek_window(
    redis: &mut ConnectionManager,
    key: &str,
    limit: isize,
) -> Result<RateLimitDecision, redis::RedisError> {
    let count: Option<isize> = redis.get(key).await?;
    if count.unwrap_or(0) >= limit {
        let ttl: i64 = redis.ttl(key).await?;
        return Ok(RateLimitDecision::Limited {
            retry_after_secs: ttl.max(0) as u64,
        });
    }
    Ok(RateLimitDecision::Allowed)
}

/// Increments a single fixed window and reports whether it is over `limit`.
///
/// A plain INCR followed by an EXPIRE set only on the first request of the
/// window. INCR is atomic, so the count itself is always correct under
/// concurrent requests; the one known gap is a crash between the INCR and the
/// EXPIRE call, which would leave that key without a TTL. That is an
/// acceptable, self-healing risk here and not worth a Lua script for.
async fn check_window(
    redis: &mut ConnectionManager,
    key: &str,
    limit: isize,
    window_secs: i64,
) -> Result<RateLimitDecision, redis::RedisError> {
    let count: isize = redis.incr(key, 1).await?;
    if count == 1 {
        let _: () = redis.expire(key, window_secs).await?;
    }

    if count > limit {
        let ttl: i64 = redis.ttl(key).await?;
        return Ok(RateLimitDecision::Limited {
            retry_after_secs: ttl.max(0) as u64,
        });
    }

    Ok(RateLimitDecision::Allowed)
}

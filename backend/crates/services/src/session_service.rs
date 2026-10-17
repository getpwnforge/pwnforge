// crates/services/src/session_service.rs
use crate::{
    audit_service::{self, AuditContext, AuditError, AuditTarget},
    jwt_service, token_service,
};
use chrono::Utc;
use domain::{
    dto::auth::{IssuedSession, RotatedSession, SessionContext},
    entities::{refresh_tokens, users},
    types::RevokedReason,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr,
    EntityTrait, ExprTrait, QueryFilter, QuerySelect, Set, TransactionTrait,
    sea_query::{Expr, LockType},
};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("refresh token not found")]
    NotFound,

    #[error("refresh token has expired")]
    Expired,

    #[error("refresh token was already used")]
    Reused,

    /// Retired by a rotation less than `ROTATION_GRACE_SECS` ago: refused,
    /// without treating it as a replay (see `rotate`).
    #[error("refresh token was rotated moments ago")]
    RecentlyRotated,

    #[error(transparent)]
    Audit(#[from] AuditError),

    #[error(transparent)]
    Db(#[from] sea_orm::DbErr),
}

/// How long after a rotation the retired token is refused without being
/// treated as a replay.
///
/// Two tabs, or a tab and a slow retry, routinely send the same refresh
/// cookie a moment apart: the first rotates it, the second arrives with the
/// old one. Read as reuse, that logged the account out everywhere. Inside
/// this window the late request is refused, which gives a thief holding a
/// copy nothing either; after it, reuse detection applies as before.
pub const ROTATION_GRACE_SECS: i64 = 10;

/// Serializes, per account, the writes that open, rotate or end its
/// sessions: `issue`, `rotate`, `revoke_all`, and the forced password
/// reset. Each takes the account's row first, inside its transaction.
///
/// Without it a rotation could claim a token, then a `revoke_all` begin and
/// wait on that token, and the rotation commit a replacement that
/// `revoke_all`, reading as of its own start, never saw: a session that
/// survived a suspension or a password change. Taking the row first also
/// gives every one of them the same lock order, account then tokens.
///
/// `FOR NO KEY UPDATE`, not `FOR UPDATE`: the writers above still exclude
/// each other, but an insert into another table that references the account
/// only needs `FOR KEY SHARE` on it, which this mode leaves free, so a
/// session write never holds up an audit entry or a team membership.
pub(crate) async fn lock_user<C: ConnectionTrait>(conn: &C, user_id: Uuid) -> Result<(), DbErr> {
    users::Entity::find_by_id(user_id)
        .select_only()
        .column(users::Column::Id)
        .lock(LockType::NoKeyUpdate)
        .into_tuple::<Uuid>()
        .one(conn)
        .await?;
    Ok(())
}

/// Whether the token was retired by a rotation less than
/// `ROTATION_GRACE_SECS` ago. Compared in SQL against `now()`: `revoked_at`
/// is stamped by the database, so its clock alone decides, and a skew
/// between the application and the database cannot stretch or close the
/// window.
async fn rotated_recently<C: ConnectionTrait>(conn: &C, token_id: Uuid) -> Result<bool, DbErr> {
    let recent = refresh_tokens::Entity::find_by_id(token_id)
        .select_only()
        .column(refresh_tokens::Column::Id)
        .filter(refresh_tokens::Column::RevokedReason.eq(RevokedReason::Rotated.as_str()))
        .filter(
            Expr::col(refresh_tokens::Column::RevokedAt).gt(Expr::cust_with_values(
                "now() - make_interval(secs => $1)",
                [ROTATION_GRACE_SECS],
            )),
        )
        .into_tuple::<Uuid>()
        .one(conn)
        .await?;
    Ok(recent.is_some())
}

/// Called by auth_service::login and, later, the OAuth callback.
pub async fn issue(
    db: &DatabaseConnection,
    user_id: Uuid,
    ctx: &SessionContext,
) -> Result<IssuedSession, SessionError> {
    let refresh_token = token_service::generate_opaque_token();

    // One transaction for the session and the activity stamp, so a sign-in
    // never records activity for a session that does not exist.
    let txn = db.begin().await?;
    lock_user(&txn, user_id).await?;

    // Store the refresh token in the database, associated with the user.
    let issued_session = refresh_tokens::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(user_id),
        token_hash: Set(refresh_token.hash),
        expires_at: Set((Utc::now()
            + chrono::Duration::seconds(jwt_service::REFRESH_TOKEN_TTL_SECS))
        .into()),
        user_agent: Set(ctx.user_agent.clone()),
        ip_address: Set(ctx.ip_address),
        last_used_at: Set(Some(Utc::now().into())),
        ..Default::default()
    }
    .insert(&txn)
    .await?;

    touch_last_activity(&txn, user_id).await?;

    txn.commit().await?;

    Ok(IssuedSession {
        secret: refresh_token.secret,
        session_id: issued_session.id,
    })
}

/// Rotates a refresh token, retiring the presented one and issuing its
/// replacement.
///
/// The retirement is a conditional claim rather than a plain update: only the
/// caller whose `UPDATE ... WHERE revoked_at IS NULL` actually matches a row
/// goes on to mint a new token. Without that condition, two concurrent
/// refreshes carrying the same secret both read it as live, both inserted a
/// replacement and both revoked the original, so one stolen token quietly
/// became two working sessions, and the reuse detection below never fired because
/// nothing was revoked at the time either of them looked.
///
/// A caller that loses the claim to a rotation less than
/// `ROTATION_GRACE_SECS` old is a second tab: refused, nothing else. Any
/// other loss is treated exactly like a replay, because from the server's
/// side the two are the same event: one secret presented twice. The whole
/// chain is revoked and both parties must sign in again.
pub async fn rotate(
    db: &DatabaseConnection,
    presented: &str,
    ctx: &SessionContext,
) -> Result<RotatedSession, SessionError> {
    let token = find_by_secret(db, presented)
        .await?
        .ok_or(SessionError::NotFound)?;

    // Reuse of a revoked token means the chain has been duplicated: either an
    // attacker or the legitimate user is replaying a copy. Revoke every live
    // token of this user so both are forced to re-authenticate. Unless it was
    // rotated a moment ago, which is a second tab rather than a copy (see
    // `ROTATION_GRACE_SECS`).
    if token.revoked_at.is_some() {
        if token.revoked_reason.as_deref() == Some(RevokedReason::Rotated.as_str()) {
            if rotated_recently(db, token.id).await? {
                return Err(SessionError::RecentlyRotated);
            }
            return Err(handle_reuse(db, &token, ctx).await?);
        }

        return Err(SessionError::NotFound);
    }

    if token.expires_at < Utc::now() {
        return Err(SessionError::Expired);
    }

    let new_refresh_token = token_service::generate_opaque_token();

    let txn = db.begin().await?;
    lock_user(&txn, token.user_id).await?;

    // Claim the presented token. `revoked_at IS NULL` is the whole point: it
    // makes the retirement the race's arbiter, so exactly one concurrent
    // refresh can proceed.
    let claimed = refresh_tokens::Entity::update_many()
        .col_expr(refresh_tokens::Column::RevokedAt, Expr::current_timestamp())
        .col_expr(
            refresh_tokens::Column::RevokedReason,
            Expr::value(RevokedReason::Rotated.as_str().to_owned()),
        )
        .filter(refresh_tokens::Column::Id.eq(token.id))
        .filter(refresh_tokens::Column::RevokedAt.is_null())
        .exec(&txn)
        .await?;

    if claimed.rows_affected == 0 {
        // Someone else retired this token between the read above and here.
        // Roll back before touching the chain, so the revocation is not undone
        // along with this transaction. A concurrent rotation that won is the
        // grace case; anything else is a replay.
        txn.rollback().await?;
        if rotated_recently(db, token.id).await? {
            return Err(SessionError::RecentlyRotated);
        }
        return Err(handle_reuse(db, &token, ctx).await?);
    }

    let new_row = refresh_tokens::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(token.user_id),
        token_hash: Set(new_refresh_token.hash),
        expires_at: Set((Utc::now()
            + chrono::Duration::seconds(jwt_service::REFRESH_TOKEN_TTL_SECS))
        .into()),
        user_agent: Set(ctx.user_agent.clone()),
        ip_address: Set(ctx.ip_address),
        last_used_at: Set(Some(Utc::now().into())),
        ..Default::default()
    }
    .insert(&txn)
    .await?;

    // Only a rotation that won the claim gets here: an unknown, expired or
    // replayed token has returned above without touching the account.
    touch_last_activity(&txn, token.user_id).await?;

    txn.commit().await?;

    Ok(RotatedSession {
        user_id: token.user_id,
        secret: new_refresh_token.secret,
        session_id: new_row.id,
    })
}

/// Stamps the account's last activity, with the database clock.
///
/// Last activity is measured on tokens: a sign-in issues the first one and
/// every successful refresh the next, so this runs at exactly those two
/// points, inside their transactions.
///
/// It is the one write on `users` that leaves `updated_at` alone, on purpose.
/// `updated_at` says the account was changed. An active user refreshes their
/// access token every 15 minutes; stamping `updated_at` here would rewrite it
/// all day for everyone signed in and leave it meaning nothing.
async fn touch_last_activity<C: ConnectionTrait>(conn: &C, user_id: Uuid) -> Result<(), DbErr> {
    users::Entity::update_many()
        .col_expr(users::Column::LastActivityAt, Expr::current_timestamp())
        .filter(users::Column::Id.eq(user_id))
        .exec(conn)
        .await?;

    Ok(())
}

/// Kills every live session of the token's owner and records why.
///
/// Shared by the two ways a replay surfaces: presenting an already-retired
/// token, and losing the race to retire a live one. Both mean the same secret
/// reached the server twice, and neither caller can be told apart from an
/// attacker holding a copy, so the safe answer is the same.
///
/// The sweep waits on the account's row (see `lock_user`), so a rotation
/// still inside its transaction finishes first and its replacement is swept
/// too.
async fn handle_reuse(
    db: &DatabaseConnection,
    token: &refresh_tokens::Model,
    ctx: &SessionContext,
) -> Result<SessionError, SessionError> {
    let txn = db.begin().await?;
    let sessions_revoked =
        revoke_all(&txn, token.user_id, RevokedReason::ReuseDetected, None).await?;
    txn.commit().await?;

    tracing::warn!(
        user_id = %token.user_id,
        "refresh token reuse detected, all sessions revoked"
    );

    audit_service::record(
        db,
        domain::types::AuditAction::AuthRefreshReuseDetected,
        &AuditContext {
            actor_id: None,
            ip_address: ctx.ip_address,
            user_agent: ctx.user_agent.clone(),
        },
        AuditTarget {
            user_id: Some(token.user_id),
            ..Default::default()
        },
        None,
        serde_json::json!({
            "presented_token_id": token.id,
            "revoked_reason": token.revoked_reason,
            "sessions_revoked": sessions_revoked,
        }),
    )
    .await?;

    Ok(SessionError::Reused)
}

/// Called by auth_service::logout. Idempotent.
pub async fn revoke(
    db: &DatabaseConnection,
    presented: &str,
    reason: RevokedReason,
) -> Result<(), SessionError> {
    let token = find_by_secret(db, presented)
        .await?
        .ok_or(SessionError::NotFound)?;

    if token.revoked_at.is_some() {
        return Ok(());
    }

    refresh_tokens::ActiveModel {
        id: Set(token.id),
        revoked_at: Set(Some(Utc::now().into())),
        revoked_reason: Set(Some(reason.as_str().to_owned())),
        ..Default::default()
    }
    .update(db)
    .await?;

    Ok(())
}

/// Called by rotate (reuse), password_reset, password_change,
/// admin_service::suspend_user (2.13) and the CLI (2.24).
///
/// Takes a transaction, not a connection: it locks the account's row first
/// (see `lock_user`), which only holds until the transaction ends, so the
/// sweep sees every session a rotation in progress was about to add.
pub async fn revoke_all(
    txn: &DatabaseTransaction,
    user_id: Uuid,
    reason: RevokedReason,
    except: Option<Uuid>,
) -> Result<u64, SessionError> {
    lock_user(txn, user_id).await?;

    let mut query = refresh_tokens::Entity::update_many()
        .col_expr(refresh_tokens::Column::RevokedAt, Expr::current_timestamp())
        .col_expr(
            refresh_tokens::Column::RevokedReason,
            Expr::value(reason.as_str().to_owned()),
        )
        .filter(refresh_tokens::Column::UserId.eq(user_id))
        .filter(refresh_tokens::Column::RevokedAt.is_null());

    if let Some(except) = except {
        query = query.filter(refresh_tokens::Column::Id.ne(except));
    }

    let update_result = query.exec(txn).await?;

    Ok(update_result.rows_affected)
}

/// List all active (non-revoked, non-expired) refresh tokens for a user. Used by the CLI and the web UI.
pub async fn list_active(
    db: &DatabaseConnection,
    user_id: Uuid,
) -> Result<Vec<refresh_tokens::Model>, SessionError> {
    let tokens = refresh_tokens::Entity::find()
        .filter(refresh_tokens::Column::UserId.eq(user_id))
        .filter(refresh_tokens::Column::RevokedAt.is_null())
        .filter(refresh_tokens::Column::ExpiresAt.gt(Utc::now()))
        .all(db)
        .await?;

    Ok(tokens)
}

/// Returns the id of the live session behind this secret, but only if it
/// belongs to `user_id`. Any mismatch yields None rather than an error: the
/// caller only wants to know which session to spare.
pub async fn find_session_id(
    db: &DatabaseConnection,
    presented: &str,
    user_id: Uuid,
) -> Result<Option<Uuid>, SessionError> {
    let hash = token_service::hash_opaque_token(presented);

    let id = refresh_tokens::Entity::find()
        .filter(refresh_tokens::Column::TokenHash.eq(hash))
        .filter(refresh_tokens::Column::UserId.eq(user_id))
        .filter(refresh_tokens::Column::RevokedAt.is_null())
        .one(db)
        .await?
        .map(|row| row.id);

    Ok(id)
}

async fn find_by_secret(
    db: &DatabaseConnection,
    presented: &str,
) -> Result<Option<refresh_tokens::Model>, DbErr> {
    let token_hash = token_service::hash_opaque_token(presented);

    refresh_tokens::Entity::find()
        .filter(refresh_tokens::Column::TokenHash.eq(token_hash))
        .one(db)
        .await
}

// backend/crates/services/src/invitation_service.rs
//
// What team and workspace invitations share: which invitations an account is
// the invitee of, and how an invitation's mail goes out.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use redis::aio::ConnectionManager;
use sea_orm::sea_query::{Expr, Query, SelectStatement};
use sea_orm::{
    ColumnTrait, Condition, ConnectionTrait, DbErr, EntityTrait, ExprTrait, QueryFilter,
    QuerySelect,
};
use uuid::Uuid;

use domain::entities::{user_emails, users};

use crate::config::Config;
use crate::email_service::{self, InvitationKind};
use crate::instance_service::{self, InstanceError};
use crate::rate_limit_service::{self, RateLimitDecision};

/// The addresses `user_id` has verified, as a subquery.
///
/// An invitation by address names no account: it is for whoever has that
/// address verified when the invitation is read or answered. Matched at each
/// read rather than resolved once at creation, so an account that verifies
/// the address later sees the invitation, one that gives it up stops seeing
/// it, and the answer to the author never depended on either.
fn verified_addresses_of(user_id: Uuid) -> SelectStatement {
    Query::select()
        .column((user_emails::Entity, user_emails::Column::Email))
        .from(user_emails::Entity)
        .and_where(Expr::col((user_emails::Entity, user_emails::Column::UserId)).eq(user_id))
        .and_where(Expr::col((user_emails::Entity, user_emails::Column::VerifiedAt)).is_not_null())
        .to_owned()
}

/// The rows `user_id` is the invitee of: named by account, or by one of its
/// verified addresses. An unverified address matches nothing.
pub fn addressed_to<A, E>(account: A, email: E, user_id: Uuid) -> Condition
where
    A: ColumnTrait,
    E: ColumnTrait,
{
    Condition::any()
        .add(account.eq(user_id))
        .add(email.in_subquery(verified_addresses_of(user_id)))
}

/// The usernames of `ids`, for the author and invitee columns of a listing.
/// An id whose account is gone is simply absent.
pub async fn usernames<C: ConnectionTrait>(
    conn: &C,
    ids: impl IntoIterator<Item = Uuid>,
) -> Result<HashMap<Uuid, String>, DbErr> {
    let mut ids: Vec<Uuid> = ids.into_iter().collect();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows: Vec<(Uuid, String)> = users::Entity::find()
        .select_only()
        .column(users::Column::Id)
        .column(users::Column::Username)
        .filter(users::Column::Id.is_in(ids))
        .into_tuple()
        .all(conn)
        .await?;

    Ok(rows.into_iter().collect())
}

/// An invitation just created, and what became of its mail: `None` for a
/// generic team link, which names nobody to write to. The routes answer with
/// `invitation` alone.
pub struct CreatedInvitation<T> {
    pub invitation: T,
    pub mail: Option<MailOutcome>,
}

/// What became of an invitation's mail. Never part of the answer to the
/// author: it depends on the address (how much mail it already received,
/// whether an account behind it has one to write to), so the routes drop it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailOutcome {
    Sent,
    /// The account invited by username has no verified primary address.
    NoAddress,
    /// The address already received its daily share of invitation mail.
    AddressCapReached,
    /// The cap could not be read. It fails closed: no mail.
    LimiterUnavailable,
    /// The rendering or the provider failed; logged, the invitation stands.
    Failed,
}

/// Where a mail goes, and in which language.
pub struct Recipient {
    pub email: String,
    pub locale: String,
}

/// An invitation by username writes to the account's primary address, once
/// verified, in the account's language.
pub async fn account_recipient<C: ConnectionTrait>(
    conn: &C,
    user_id: Uuid,
) -> Result<Option<Recipient>, DbErr> {
    let Some(user) = users::Entity::find_by_id(user_id).one(conn).await? else {
        return Ok(None);
    };

    let primary = user_emails::Entity::find()
        .filter(user_emails::Column::UserId.eq(user_id))
        .filter(user_emails::Column::IsPrimary.eq(true))
        .filter(user_emails::Column::VerifiedAt.is_not_null())
        .one(conn)
        .await?;

    Ok(primary.map(|primary| Recipient {
        email: primary.email,
        locale: user.locale,
    }))
}

/// An invitation by address writes to that address in the instance's
/// language: whether an account holds it is not looked up, so its language
/// cannot be either.
pub async fn address_recipient<C: ConnectionTrait>(
    conn: &C,
    email: &str,
) -> Result<Recipient, InstanceError> {
    let settings = instance_service::settings(conn).await?;
    Ok(Recipient {
        email: email.to_owned(),
        locale: settings.default_locale,
    })
}

/// The invitation being mailed.
pub struct InvitationMail<'a> {
    pub kind: InvitationKind,
    pub name: &'a str,
    pub token: &'a str,
    pub expires_at: DateTime<Utc>,
}

/// Sends one invitation mail under the per-address cap, best effort.
///
/// Never fails the invitation it belongs to: past the cap, or with the
/// provider down, the invitation is still created and still visible in-app,
/// and the author gets the same answer. The cap fails closed: without the
/// limiter nothing bounds how much mail one address receives, so no mail
/// goes out. Neither the address nor the token is logged.
pub async fn send_mail(
    redis: &mut ConnectionManager,
    config: &Config,
    to: &Recipient,
    mail: InvitationMail<'_>,
) -> MailOutcome {
    match rate_limit_service::check_invitation_mail_to_address(redis, &to.email).await {
        Ok(RateLimitDecision::Allowed) => {}
        Ok(RateLimitDecision::Limited { .. }) => {
            tracing::info!(kind = ?mail.kind, "invitation mail not sent: the address reached its daily cap");
            return MailOutcome::AddressCapReached;
        }
        Err(error) => {
            tracing::error!(?error, kind = ?mail.kind, "invitation mail limiter unavailable, mail not sent");
            return MailOutcome::LimiterUnavailable;
        }
    }

    match email_service::send_invitation(
        config,
        &to.email,
        &to.locale,
        mail.kind,
        mail.name,
        mail.token,
        mail.expires_at,
    )
    .await
    {
        Ok(()) => MailOutcome::Sent,
        Err(error) => {
            tracing::error!(?error, kind = ?mail.kind, "failed to send invitation email");
            MailOutcome::Failed
        }
    }
}

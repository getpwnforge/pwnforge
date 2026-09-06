//! Business logic for legal document acceptance.
//!
//! Target path: backend/crates/services/src/legal_service.rs
//!
//! Two distinct mechanisms live here, and conflating them is the mistake to
//! avoid:
//!
//! - **Formation.** Registration requires an explicit checkbox. An account
//!   that never accepted anything is blocked until it does, because the very
//!   clause allowing tacit acceptance sits inside the document it never
//!   agreed to.
//! - **Revision.** Once formed, article 23 of the terms makes continued use
//!   count as acceptance after the notice period. No modal, but the tacit
//!   acceptance is recorded so it can be proven.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use domain::entities::{legal_acceptances, users};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr,
    EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};
use uuid::Uuid;

use domain::dto::auth::SessionContext;
use domain::dto::legal::{LegalAcceptanceInput, LegalState, LegalStatusDto};
use domain::legal::{
    AcceptanceMethod, CURRENT_PRIVACY_VERSION, CURRENT_TERMS_VERSION, LEGAL_EFFECTIVE_AT,
    LegalDocument,
};

/* -------------------------------------------------------------------------- */
/* Errors                                                                     */
/* -------------------------------------------------------------------------- */

#[derive(Debug, thiserror::Error)]
pub enum LegalError {
    /// The submitted version is superseded. Mapped to 409
    /// `legal_version_stale`, so the frontend reloads the documents and
    /// re-prompts instead of silently accepting.
    #[error("submitted legal version is superseded")]
    VersionStale,

    /// The account has never accepted the documents. Mapped to 403
    /// `legal_acceptance_required`. Only reachable for accounts created
    /// outside the registration form.
    #[error("legal acceptance required")]
    AcceptanceRequired,

    #[error(transparent)]
    Db(#[from] DbErr),
}

/* -------------------------------------------------------------------------- */
/* Pure decision logic                                                        */
/* -------------------------------------------------------------------------- */

/// Both documents accepted and still valid.
pub fn is_up_to_date(terms_version: Option<&str>, privacy_version: Option<&str>) -> bool {
    match (terms_version, privacy_version) {
        (Some(terms), Some(privacy)) => {
            LegalDocument::Terms.is_valid(terms) && LegalDocument::Privacy.is_valid(privacy)
        }
        _ => false,
    }
}

/// Resolves the state from two version strings. No database access.
///
/// Shared by `status`, which reads the append-only table, and by `enforce`,
/// which reads the cache columns on `users`. A single implementation is what
/// guarantees both paths agree: a divergence here would silently void the
/// notice period promised in article 23.
pub fn resolve_state(
    terms_version: Option<&str>,
    privacy_version: Option<&str>,
    now: DateTime<Utc>,
) -> LegalState {
    if is_up_to_date(terms_version, privacy_version) {
        return LegalState::Ok;
    }

    match parse_effective_at() {
        // Revision published, notice period still running.
        Some(effective_at) if now < effective_at => LegalState::Pending,
        // Elapsed, or no revision pending: continued use is acceptance.
        _ => LegalState::Ok,
    }
}

/// Rejects an acceptance that refers to a superseded version.
///
/// Pure string comparison, so handlers call it before entering a service.
/// That keeps a stale version out of `AuthError` during registration and lets
/// it surface as a clean 409.
pub fn validate_input(input: &LegalAcceptanceInput) -> Result<(), LegalError> {
    if LegalDocument::Terms.is_valid(&input.terms_version)
        && LegalDocument::Privacy.is_valid(&input.privacy_version)
    {
        Ok(())
    } else {
        Err(LegalError::VersionStale)
    }
}

/// Parses `LEGAL_EFFECTIVE_AT`, treating a malformed value as absent.
///
/// A typo must not make the notice period silently permanent, so an
/// unparseable value logs and falls through to `None`.
fn parse_effective_at() -> Option<DateTime<Utc>> {
    let raw = LEGAL_EFFECTIVE_AT?;
    match DateTime::parse_from_rfc3339(raw) {
        Ok(dt) => Some(dt.with_timezone(&Utc)),
        Err(err) => {
            tracing::error!(value = raw, error = ?err, "LEGAL_EFFECTIVE_AT is not valid RFC 3339");
            None
        }
    }
}

/* -------------------------------------------------------------------------- */
/* Writes                                                                     */
/* -------------------------------------------------------------------------- */

/// Appends one acceptance row and refreshes the matching cache column.
///
/// The insert and the cache update must stay in this single function. This is
/// the only place allowed to write those columns: if the row and the cache
/// diverge, the request gate believes a user is up to date while the evidence
/// says otherwise.
pub async fn record<C: ConnectionTrait>(
    conn: &C,
    user_id: Uuid,
    document: LegalDocument,
    version: &str,
    method: AcceptanceMethod,
    ctx: &SessionContext,
) -> Result<(), LegalError> {
    legal_acceptances::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(user_id),
        document: Set(document.as_str().to_string()),
        version: Set(version.to_string()),
        method: Set(method.as_str().to_string()),
        user_agent: Set(ctx.user_agent.clone()),
        ip_address: Set(ctx.ip_address),
        ..Default::default()
    }
    .insert(conn)
    .await?;

    users::Entity::update_many()
        .col_expr(
            match document {
                LegalDocument::Terms => users::Column::LegalTermsVersion,
                LegalDocument::Privacy => users::Column::LegalPrivacyVersion,
            },
            Expr::value(version.to_string()),
        )
        .filter(users::Column::Id.eq(user_id))
        .exec(conn)
        .await?;

    Ok(())
}

/// Records an explicit acceptance of both documents, inside a caller-owned
/// transaction.
///
/// `auth_service::register` passes its own transaction, so a failure rolls
/// the account creation back. An account that exists without a recorded
/// acceptance is precisely what this table prevents.
pub async fn record_both(
    txn: &DatabaseTransaction,
    user_id: Uuid,
    input: &LegalAcceptanceInput,
    ctx: &SessionContext,
) -> Result<(), LegalError> {
    validate_input(input)?;

    record(
        txn,
        user_id,
        LegalDocument::Terms,
        &input.terms_version,
        AcceptanceMethod::Explicit,
        ctx,
    )
    .await?;
    record(
        txn,
        user_id,
        LegalDocument::Privacy,
        &input.privacy_version,
        AcceptanceMethod::Explicit,
        ctx,
    )
    .await?;

    Ok(())
}

/// Opens a transaction, delegates to `record_both`, commits.
/// Used by `POST /legal/accept`, which has no ambient transaction.
pub async fn accept(
    conn: &DatabaseConnection,
    user_id: Uuid,
    input: &LegalAcceptanceInput,
    ctx: &SessionContext,
) -> Result<(), LegalError> {
    let txn = conn.begin().await?;
    record_both(&txn, user_id, input, ctx).await?;
    txn.commit().await?;

    Ok(())
}

/// Records a tacit acceptance when continued use amounts to acceptance under
/// article 23 of the terms.
///
/// Called from `auth_service::login` and `auth_service::refresh`, never on the
/// request hot path: it writes at most once per user per revision, and a
/// login is the natural moment at which "continued use" is observed.
///
/// Failure must not break the login. The caller logs and proceeds: refusing a
/// sign-in because an evidentiary row could not be written would be worse
/// than the missing row.
pub async fn apply_tacit_acceptance(
    conn: &DatabaseConnection,
    user_id: Uuid,
    terms_version: Option<&str>,
    privacy_version: Option<&str>,
    now: DateTime<Utc>,
    ctx: &SessionContext,
) -> Result<(), LegalError> {
    // Pending means the notice period is still running: continued use does
    // not amount to acceptance yet.
    if resolve_state(terms_version, privacy_version, now) != LegalState::Ok {
        return Ok(());
    }

    let terms_stale = !terms_version.is_some_and(|v| LegalDocument::Terms.is_valid(v));
    let privacy_stale = !privacy_version.is_some_and(|v| LegalDocument::Privacy.is_valid(v));

    // Ok also covers the nominal case where everything is current. Bail out
    // before opening a transaction, otherwise this commits an empty one on
    // every login and every refresh.
    if !terms_stale && !privacy_stale {
        return Ok(());
    }

    let txn = conn.begin().await?;

    if terms_stale {
        record(
            &txn,
            user_id,
            LegalDocument::Terms,
            CURRENT_TERMS_VERSION,
            AcceptanceMethod::Tacit,
            ctx,
        )
        .await?;
    }

    if privacy_stale {
        record(
            &txn,
            user_id,
            LegalDocument::Privacy,
            CURRENT_PRIVACY_VERSION,
            AcceptanceMethod::Tacit,
            ctx,
        )
        .await?;
    }

    txn.commit().await?;

    Ok(())
}

/* -------------------------------------------------------------------------- */
/* Reads                                                                      */
/* -------------------------------------------------------------------------- */

/// Latest accepted version per document, read from the append-only table.
///
/// This is the authoritative read. The columns on `users` are only a hot path
/// cache and must never answer an evidentiary question such as "which version
/// did this person accept, and by which method".
pub async fn latest_accepted<C: ConnectionTrait>(
    conn: &C,
    user_id: Uuid,
) -> Result<HashMap<LegalDocument, String>, LegalError> {
    let rows = legal_acceptances::Entity::find()
        .filter(legal_acceptances::Column::UserId.eq(user_id))
        .distinct_on([legal_acceptances::Column::Document])
        .order_by_asc(legal_acceptances::Column::Document)
        .order_by_desc(legal_acceptances::Column::AcceptedAt)
        .all(conn)
        .await?;

    Ok(rows
        .into_iter()
        .filter_map(|row| LegalDocument::parse(&row.document).map(|doc| (doc, row.version)))
        .collect())
}

/// Resolves the state to expose to the client.
///
/// `now` is a parameter rather than a call to `Utc::now()`, so the notice
/// period logic is testable without freezing the clock.
pub async fn status<C: ConnectionTrait>(
    conn: &C,
    user_id: Uuid,
    now: DateTime<Utc>,
) -> Result<LegalStatusDto, LegalError> {
    let accepted = latest_accepted(conn, user_id).await?;

    let accepted_terms = accepted.get(&LegalDocument::Terms).cloned();
    let accepted_privacy = accepted.get(&LegalDocument::Privacy).cloned();

    if accepted_terms.is_none() || accepted_privacy.is_none() {
        // Registration and setup both write two rows in their own
        // transaction.
        tracing::warn!(user_id = %user_id, "user has no recorded legal acceptance");
    }

    let state = resolve_state(accepted_terms.as_deref(), accepted_privacy.as_deref(), now);

    Ok(LegalStatusDto {
        state,
        current_terms_version: CURRENT_TERMS_VERSION,
        current_privacy_version: CURRENT_PRIVACY_VERSION,
        accepted_terms_version: accepted_terms,
        accepted_privacy_version: accepted_privacy,
        // Only surfaced while it still means something: in Ok it is
        // irrelevant, in Required the date has passed.
        effective_at: match state {
            LegalState::Pending => LEGAL_EFFECTIVE_AT.map(str::to_owned),
            _ => None,
        },
    })
}

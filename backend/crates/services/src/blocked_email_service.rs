// crates/api/src/services/blocked_email_service.rs
use domain::entities::{allowed_emails_domains, blocked_emails_domains};
use domain::types::BlockSource;
use sea_orm::{
    ActiveModelTrait,
    ActiveValue::{NotSet, Set},
    ColumnTrait, DatabaseConnection, DbErr, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum BlockedEmailError {
    #[error("email domain is not allowed")]
    DomainNotAllowed,

    #[error("domain not found in the blocked list")]
    NotFound,

    #[error("this domain comes from the disposable list, use refresh-disposable instead")]
    DisposableListEntry,

    #[error(transparent)]
    Db(#[from] sea_orm::DbErr),
}

/// Rejects registration if the email domain is blocked.
///
/// Manual blocks always apply. The public disposable list is consulted only
/// when enabled, and can be overridden per domain by the allowlist.
///
/// # Errors
///
/// Returns [`BlockedEmailError::DomainNotAllowed`] if the domain is blocked, or a
/// database error if a lookup fails. The caller must surface a generic message:
/// revealing the reason leaks the contents of the list.
pub async fn check_domain(
    db: &DatabaseConnection,
    domain: &str,
    block_disposable_emails: bool,
) -> Result<(), BlockedEmailError> {
    // Manual blocks always apply: disabling the disposable check is about the
    // public list, not about the operator's own decisions.
    if is_domain_blocked(db, domain, Some(BlockSource::Manual)).await? {
        return Err(BlockedEmailError::DomainNotAllowed);
    }

    if !block_disposable_emails {
        return Ok(());
    }

    // The allowlist wins: it exists to override false positives in the public
    // disposable list without editing that list by hand.
    if is_domain_allowed(db, domain).await? {
        return Ok(());
    }

    if is_domain_blocked(db, domain, None).await? {
        return Err(BlockedEmailError::DomainNotAllowed);
    }

    Ok(())
}

/// Adds a domain to the blocked list as a manual entry.
/// `added_by` is `None` when the action comes from the CLI.
pub async fn block_domain(
    db: &DatabaseConnection,
    domain: &str,
    reason: &str,
    added_by: Option<Uuid>,
) -> Result<(), BlockedEmailError> {
    let domain = domain.trim().to_lowercase();

    blocked_emails_domains::ActiveModel {
        domain: Set(domain),
        reason: Set(Some(reason.to_owned())),
        source: Set(BlockSource::Manual.as_str().to_owned()),
        added_by: Set(added_by),
        ..Default::default()
    }
    .insert(db)
    .await?;

    Ok(())
}

/// Removes a manually-blocked domain. Refuses disposable-list entries: those
/// only move via `sync_disposable_domains`, so the DB never drifts from the
/// embedded list one manual delete at a time.
pub async fn unblock_domain(
    db: &DatabaseConnection,
    domain: &str,
) -> Result<(), BlockedEmailError> {
    let domain = domain.trim().to_lowercase();

    let entry = blocked_emails_domains::Entity::find_by_id(&domain)
        .one(db)
        .await?
        .ok_or(BlockedEmailError::NotFound)?;

    if entry.source != BlockSource::Manual.as_str() {
        return Err(BlockedEmailError::DisposableListEntry);
    }

    blocked_emails_domains::Entity::delete_by_id(domain)
        .exec(db)
        .await?;

    Ok(())
}

/// Lists every blocked domain, most recently added first.
pub async fn list_blocked(
    db: &DatabaseConnection,
) -> Result<Vec<blocked_emails_domains::Model>, BlockedEmailError> {
    Ok(blocked_emails_domains::Entity::find()
        .order_by_desc(blocked_emails_domains::Column::AddedAt)
        .all(db)
        .await?)
}

/// Adds a domain to the allowlist, overriding a false positive in the
/// disposable list without editing that list by hand.
pub async fn allow_domain(
    db: &DatabaseConnection,
    domain: &str,
    reason: &str,
    added_by: Option<Uuid>,
) -> Result<(), BlockedEmailError> {
    let domain = domain.trim().to_lowercase();

    allowed_emails_domains::ActiveModel {
        domain: Set(domain),
        reason: Set(Some(reason.to_owned())),
        added_by: Set(added_by),
        added_at: NotSet,
    }
    .insert(db)
    .await?;

    Ok(())
}

/// Removes a domain from the allowlist. No source distinction needed here,
/// unlike `unblock_domain`: every allowlist entry is manual, there's no
/// public "allowed" list to protect from accidental one-off deletes.
pub async fn disallow_domain(
    db: &DatabaseConnection,
    domain: &str,
) -> Result<(), BlockedEmailError> {
    let domain = domain.trim().to_lowercase();

    allowed_emails_domains::Entity::find_by_id(&domain)
        .one(db)
        .await?
        .ok_or(BlockedEmailError::NotFound)?;

    allowed_emails_domains::Entity::delete_by_id(domain)
        .exec(db)
        .await?;
    Ok(())
}

/// Lists every allowlisted domain, most recently added first.
pub async fn list_allowed(
    db: &DatabaseConnection,
) -> Result<Vec<allowed_emails_domains::Model>, BlockedEmailError> {
    Ok(allowed_emails_domains::Entity::find()
        .order_by_desc(allowed_emails_domains::Column::AddedAt)
        .all(db)
        .await?)
}

/// Re-applies the disposable-list entries embedded in the current binary.
/// Additive only, by design: a domain that drops out of the public list is
/// never auto-unblocked here. `allow_domain` is the deliberate, audited path
/// for false positives
pub async fn sync_disposable_domains(
    db: &DatabaseConnection,
    raw: &str,
) -> Result<usize, BlockedEmailError> {
    let domains = parse_domains(raw);

    let models = domains
        .iter()
        .map(|domain| blocked_emails_domains::ActiveModel {
            domain: Set(domain.clone()),
            source: Set(BlockSource::DisposableList.as_str().to_owned()),
            reason: NotSet,
            added_by: NotSet,
            added_at: NotSet,
        });

    let result = blocked_emails_domains::Entity::insert_many(models)
        .on_conflict(
            sea_orm::sea_query::OnConflict::column(blocked_emails_domains::Column::Domain)
                .do_nothing()
                .to_owned(),
        )
        .exec_without_returning(db)
        .await?;

    Ok(result as usize)
}

/// Returns whether a domain has been explicitly allowed by an instance admin.
///
/// Allowlist entries override the disposable list, which is why this is checked
/// first. The comparison is case-insensitive: the column is `citext`.
async fn is_domain_allowed(db: &DatabaseConnection, domain: &str) -> Result<bool, DbErr> {
    let count = allowed_emails_domains::Entity::find_by_id(domain)
        .count(db)
        .await?;

    Ok(count > 0)
}

/// Returns whether a domain is blocked.
///
/// `source` narrows the lookup to a single origin, or checks every block when
/// `None`. The comparison is case-insensitive: the column is `citext`.
async fn is_domain_blocked(
    db: &DatabaseConnection,
    domain: &str,
    source: Option<BlockSource>,
) -> Result<bool, DbErr> {
    let mut query = blocked_emails_domains::Entity::find_by_id(domain);

    if let Some(source) = source {
        query = query.filter(blocked_emails_domains::Column::Source.eq(source.as_str()));
    }

    let count = query.count(db).await?;
    Ok(count > 0)
}

/// Deliberately duplicated from `seed::seeders::disposable_domains::parse_domains`:
/// four lines of stable text parsing, not worth a cross-crate dependency for.
fn parse_domains(raw: &str) -> Vec<String> {
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{DatabaseBackend, MockDatabase, Value};
    use std::collections::BTreeMap;

    /// Builds a connection that answers each count query in order.
    fn mock_counts(counts: &[i64]) -> DatabaseConnection {
        let rows: Vec<Vec<BTreeMap<String, Value>>> = counts
            .iter()
            .map(|c| {
                let mut row = BTreeMap::new();
                row.insert("num_items".to_owned(), Value::BigInt(Some(*c)));
                vec![row]
            })
            .collect();

        MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results(rows)
            .into_connection()
    }

    #[tokio::test]
    async fn manual_block_applies_even_when_disposable_check_is_off() {
        // First lookup: manual block found.
        let db = mock_counts(&[1]);
        let result = check_domain(&db, "spam.example", false).await;
        assert!(matches!(result, Err(BlockedEmailError::DomainNotAllowed)));
    }

    #[tokio::test]
    async fn allowlist_wins_over_disposable_list() {
        // No manual block, then an allowlist hit.
        let db = mock_counts(&[0, 1]);
        assert!(check_domain(&db, "proton.me", true).await.is_ok());
    }

    #[tokio::test]
    async fn disposable_domain_is_rejected() {
        // No manual block, not allowed, present in the list.
        let db = mock_counts(&[0, 0, 1]);
        let result = check_domain(&db, "mailinator.com", true).await;
        assert!(matches!(result, Err(BlockedEmailError::DomainNotAllowed)));
    }

    #[tokio::test]
    async fn unknown_domain_passes() {
        let db = mock_counts(&[0, 0, 0]);
        assert!(check_domain(&db, "enzo.dev", true).await.is_ok());
    }
}

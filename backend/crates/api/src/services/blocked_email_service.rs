// crates/api/src/services/blocked_email_service.rs
use domain::entities::{allowed_emails_domains, blocked_emails_domains};
use domain::types::BlockSource;
use sea_orm::{ColumnTrait, DatabaseConnection, DbErr, EntityTrait, PaginatorTrait, QueryFilter};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BlockedEmailError {
    #[error("email domain is not allowed")]
    DomainNotAllowed,

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

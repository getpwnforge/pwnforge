use domain::entities::blocked_emails_domains;
use domain::types::BlockSource;
use sea_orm::sea_query::OnConflict;
use sea_orm::{ActiveValue::NotSet, ActiveValue::Set, DatabaseConnection, DbErr, EntityTrait};

/// Rows per INSERT. A single statement for the whole list would build a query
/// of several hundred kilobytes.
const CHUNK_SIZE: usize = 1_000;

/// Inserts the public disposable email domain list, preserving existing rows.
///
/// Idempotent: entries already present are left untouched, including manual
/// admin blocks that happen to share a domain with the public list.
///
/// # Errors
///
/// Returns a database error if an insert fails.
pub async fn seed_disposable_domains(db: &DatabaseConnection, raw: &str) -> Result<usize, DbErr> {
    let domains = parse_domains(raw);

    for chunk in domains.chunks(CHUNK_SIZE) {
        let models: Vec<_> = chunk
            .iter()
            .map(|domain| blocked_emails_domains::ActiveModel {
                domain: Set(domain.clone()),
                source: Set(BlockSource::DisposableList.as_str().to_owned()),
                reason: NotSet,
                added_by: NotSet,
                added_at: NotSet, // defaults to now() in the database
            })
            .collect();

        let result = blocked_emails_domains::Entity::insert_many(models)
            .on_conflict(
                OnConflict::column(blocked_emails_domains::Column::Domain)
                    .do_nothing()
                    .to_owned(),
            )
            .exec(db)
            .await;

        match result {
            Ok(_) => {}
            // Every domain in this chunk was already present. Expected on any
            // run after the first: not an error for an idempotent seed.
            Err(DbErr::RecordNotInserted) => {}
            Err(e) => return Err(e),
        }
    }

    Ok(domains.len())
}

/// Extracts domains from the raw list file, skipping blanks and comments.
fn parse_domains(raw: &str) -> Vec<String> {
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_lowercase)
        .collect()
}

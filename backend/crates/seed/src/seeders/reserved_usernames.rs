use domain::entities::reserved_usernames;
use domain::types::ReservedSource;
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
pub async fn seed_reserved_usernames(db: &DatabaseConnection, raw: &str) -> Result<usize, DbErr> {
    let usernames = parse_usernames(raw);

    for chunk in usernames.chunks(CHUNK_SIZE) {
        let models: Vec<_> = chunk
            .iter()
            .map(|username| reserved_usernames::ActiveModel {
                username: Set(username.clone()),
                source: Set(ReservedSource::ReservedList.as_str().to_owned()),
                reason: NotSet,
                added_by: NotSet,
                added_at: NotSet, // defaults to now() in the database
            })
            .collect();

        let result = reserved_usernames::Entity::insert_many(models)
            .on_conflict(
                OnConflict::column(reserved_usernames::Column::Username)
                    .do_nothing()
                    .to_owned(),
            )
            .exec(db)
            .await;

        match result {
            Ok(_) => {}
            // Every username in this chunk was already present. Expected on any
            // run after the first: not an error for an idempotent seed.
            Err(DbErr::RecordNotInserted) => {}
            Err(e) => return Err(e),
        }
    }

    Ok(usernames.len())
}

/// Extracts usernames from the raw list file, skipping blanks and comments.
fn parse_usernames(raw: &str) -> Vec<String> {
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_lowercase)
        .collect()
}

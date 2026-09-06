//! Resolves a user-supplied identifier (a UUID or a username) to a real user
//! id. Shared by every command that targets a user, and by `--actor` in
//! `main.rs`, so the two ways to name a user stay consistent everywhere.

use domain::entities::{user_emails, users};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use services::auth_service;
use uuid::Uuid;

/// Tries the identifier as a UUID first (unambiguous, no query needed —
/// useful when piping a `target_user_id` from `audit recent` straight back
/// in), then falls back to a username lookup.
pub async fn resolve_user(db: &DatabaseConnection, identifier: &str) -> anyhow::Result<Uuid> {
    if let Ok(id) = Uuid::parse_str(identifier) {
        return Ok(id);
    }

    let user_id = if identifier.contains('@') {
        let normalized = auth_service::normalize_email(identifier);

        let user_email = user_emails::Entity::find()
            .filter(user_emails::Column::Email.eq(normalized))
            .one(db)
            .await?
            .ok_or_else(|| anyhow::anyhow!("no user matching '{identifier}'"))?;

        user_email.user_id
    } else {
        let user = users::Entity::find()
            .filter(users::Column::Username.eq(identifier))
            .one(db)
            .await?
            .ok_or_else(|| anyhow::anyhow!("no user matching '{identifier}'"))?;

        user.id
    };

    Ok(user_id)
}

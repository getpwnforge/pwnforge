//! Instance alerts: admin-authored banners shown above the app shell (info,
//! warning, danger, maintenance).
//!
//! Messages are stored as plain text, no translation used.

use chrono::{DateTime, FixedOffset, Utc};
use domain::entities::instance_alerts::{ActiveModel, Column, Entity, Model};
use domain::types::{AlertKind, AuditAction};
use sea_orm::{
    ActiveModelTrait,
    ActiveValue::{NotSet, Set},
    ColumnTrait, DatabaseConnection, DbErr, EntityTrait, ExprTrait, Order, QueryFilter, QueryOrder,
    TransactionTrait,
};
use thiserror::Error;
use uuid::Uuid;

use crate::audit_service::{self, AuditContext, AuditError, AuditTarget};

#[derive(Debug, Error)]
pub enum AlertError {
    #[error("alert not found")]
    NotFound,

    #[error(transparent)]
    Audit(#[from] AuditError),

    #[error(transparent)]
    Db(#[from] DbErr),
}

/// Higher wins when more than one alert qualifies as "active" at once.
fn priority(kind: &str) -> u8 {
    match AlertKind::parse(kind) {
        Some(AlertKind::Danger) => 3,
        Some(AlertKind::Warning) => 2,
        Some(AlertKind::Maintenance) => 1,
        Some(AlertKind::Info) => 0,
        None => {
            tracing::warn!(kind, "unknown alert kind in database, treating as info");
            0
        }
    }
}

/// Creates a new alert. `starts_at` has no default at the DB level on
/// purpose: the caller must always be explicit about when it starts,
/// immediately or scheduled.
pub async fn create_alert(
    db: &DatabaseConnection,
    ctx: &AuditContext,
    kind: AlertKind,
    message: String,
    link_url: Option<&str>,
    starts_at: DateTime<FixedOffset>,
    ends_at: Option<DateTime<FixedOffset>>,
) -> Result<Model, AlertError> {
    let txn = db.begin().await?;

    let alert = ActiveModel {
        id: NotSet,
        kind: Set(kind.as_str().to_owned()),
        message: Set(message),
        link_url: Set(link_url.map(str::to_owned)),
        starts_at: Set(starts_at),
        ends_at: Set(ends_at),
        is_active: Set(true),
        created_by: Set(ctx.actor_id),
        created_at: NotSet,
    }
    .insert(&txn)
    .await?;

    audit_service::record(
        &txn,
        AuditAction::AlertCreated,
        ctx,
        AuditTarget::default(),
        None,
        serde_json::json!({ "alert_id": alert.id, "kind": alert.kind }),
    )
    .await?;

    txn.commit().await?;
    Ok(alert)
}

/// Updates an existing alert. Every field is independently optional: `None`
/// leaves it untouched. `link_url`/`ends_at` use `Option<Option<T>>` so a
/// PATCH can distinguish "don't touch" from "clear this field" - plain
/// `Option<T>` can't express that distinction.
///
/// Setting `is_active = Some(false)` is how an admin kills a live alert
/// immediately, without waiting for `ends_at` or deleting the row (see
/// ARCHITECTURE.md §6.11: "Update (dont désactivation via is_active)").
#[allow(clippy::too_many_arguments)]
pub async fn update_alert(
    db: &DatabaseConnection,
    ctx: &AuditContext,
    id: Uuid,
    message: Option<String>,
    link_url: Option<Option<String>>,
    ends_at: Option<Option<DateTime<FixedOffset>>>,
    is_active: Option<bool>,
) -> Result<Model, AlertError> {
    let txn = db.begin().await?;

    let existing = Entity::find_by_id(id)
        .one(&txn)
        .await?
        .ok_or(AlertError::NotFound)?;

    let mut model: ActiveModel = existing.into();

    if let Some(message) = message {
        model.message = Set(message);
    }
    if let Some(link_url) = link_url {
        model.link_url = Set(link_url);
    }
    if let Some(ends_at) = ends_at {
        model.ends_at = Set(ends_at);
    }
    if let Some(is_active) = is_active {
        model.is_active = Set(is_active);
    }

    let updated = model.update(&txn).await?;

    audit_service::record(
        &txn,
        AuditAction::AlertUpdated,
        ctx,
        AuditTarget::default(),
        None,
        serde_json::json!({ "alert_id": updated.id }),
    )
    .await?;

    txn.commit().await?;
    Ok(updated)
}

/// Permanently deletes an alert. No soft-disable here (that's `is_active`)
/// - this is for cleaning up a mistake or genuinely obsolete history, not
///   for taking a currently-live alert down.
pub async fn delete_alert(
    db: &DatabaseConnection,
    ctx: &AuditContext,
    id: Uuid,
) -> Result<(), AlertError> {
    let txn = db.begin().await?;

    Entity::find_by_id(id)
        .one(&txn)
        .await?
        .ok_or(AlertError::NotFound)?;

    Entity::delete_by_id(id).exec(&txn).await?;

    audit_service::record(
        &txn,
        AuditAction::AlertDeleted,
        ctx,
        AuditTarget::default(),
        None,
        serde_json::json!({ "alert_id": id }),
    )
    .await?;

    txn.commit().await?;
    Ok(())
}

/// Lists every alert, active and past, most recent first. For the admin
/// console/CLI, not the public banner - see `active_alerts` for that.
pub async fn list_alerts(db: &DatabaseConnection) -> Result<Vec<Model>, AlertError> {
    Ok(Entity::find()
        .order_by(Column::CreatedAt, Order::Desc)
        .all(db)
        .await?)
}

/// Every alert to show right now: `is_active` and within its time window,
/// ordered by descending `kind` priority (danger > warning > maintenance >
/// info), then most recent first.
///
/// Returns the whole list rather than a single winner. The client dismisses
/// alerts one by one and needs the next one to be reachable: with a single
/// result, dismissing the top-priority alert buried every other one until it
/// expired, because the endpoint kept answering with the same row.
///
/// Backs the public, unauthenticated `GET /alerts/active`.
pub async fn active_alerts(db: &DatabaseConnection) -> Result<Vec<Model>, AlertError> {
    let now = Utc::now();

    let mut candidates = Entity::find()
        .filter(Column::IsActive.eq(true))
        .filter(Column::StartsAt.lte(now))
        .filter(Column::EndsAt.is_null().or(Column::EndsAt.gt(now)))
        .all(db)
        .await?;

    // Sorted in Rust rather than SQL: the ordering is by `priority(kind)`,
    // which is a Rust function, not a column. Expressing it as a CASE in the
    // query would duplicate the mapping in two places.
    candidates.sort_by(|a, b| {
        priority(&b.kind)
            .cmp(&priority(&a.kind))
            .then(b.created_at.cmp(&a.created_at))
    });

    Ok(candidates)
}

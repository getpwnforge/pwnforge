//! Instance-wide statistics, computed on demand from `users`. No dedicated
//! storage: read-only aggregates, consumed today by `pwnforge-admin stats`,
//! tomorrow by `GET /admin/metrics` (see ARCHITECTURE.md §6.11).

use chrono::{Duration, NaiveDate, Utc};
use domain::entities::users;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ColumnTrait, DatabaseConnection, DbErr, EntityTrait, ExprTrait, FromQueryResult,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect,
};
use serde::Serialize;

#[derive(Debug, Serialize, FromQueryResult)]
pub struct SignupsByDay {
    pub day: NaiveDate,
    pub count: i64,
}

pub async fn users_count(db: &DatabaseConnection) -> Result<u64, DbErr> {
    users::Entity::find().count(db).await
}

pub async fn signups_by_day(
    db: &DatabaseConnection,
    days: u32,
) -> Result<Vec<SignupsByDay>, DbErr> {
    let since = Utc::now() - Duration::days(days as i64);
    // Postgres-specific cast, written once: the three call sites below must
    // stay identical or the GROUP BY stops matching the projection.
    let day = Expr::cust("created_at::date");

    users::Entity::find()
        .filter(users::Column::CreatedAt.gte(since))
        .select_only()
        .column_as(day.clone(), "day")
        .column_as(Expr::col(users::Column::Id).count(), "count")
        .group_by(day.clone())
        .order_by_asc(day)
        .into_model::<SignupsByDay>()
        .all(db)
        .await
}

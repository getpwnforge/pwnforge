use crate::entities::instance_alerts;
use chrono::{DateTime, FixedOffset};
use serde::Serialize;

/// Public shape of an alert: no `created_by` (which admin authored it is
/// nobody else's business on an unauthenticated route), no `is_active` (a
/// row reaching this DTO is already known to be the active one).
#[derive(Serialize, utoipa::ToSchema)]
pub struct PublicAlertResponse {
    pub id: uuid::Uuid,
    pub kind: String,
    pub message: String,
    pub link_url: Option<String>,
    pub starts_at: DateTime<FixedOffset>,
    pub ends_at: Option<DateTime<FixedOffset>>,
}

impl From<instance_alerts::Model> for PublicAlertResponse {
    fn from(row: instance_alerts::Model) -> Self {
        PublicAlertResponse {
            id: row.id,
            kind: row.kind,
            message: row.message,
            link_url: row.link_url,
            starts_at: row.starts_at,
            ends_at: row.ends_at,
        }
    }
}

// crates/api/src/routes/alerts.rs
use crate::{error::AppError, state::AppState};
use axum::{Json, Router, extract::State, routing::get};
use domain::dto::{alerts::PublicAlertResponse, error_responses::SimpleErrorResponse};
use services::alert_service;

pub fn router() -> Router<AppState> {
    Router::new().route("/alerts/active", get(active))
}

/// Public, unauthenticated. Returns every currently active alert, highest
/// priority first.
///
/// An empty array replaces the previous 204: the client dismisses alerts one
/// at a time and needs to see what comes next, and "no alert" is a list of
/// length zero rather than a separate status code to special-case.
// Private: only reachable through router() above.
#[utoipa::path(
    get,
    path = "/api/v1/alerts/active",
    tag = "Alerts",
    responses(
        (status = 200, description = "Active alerts", body = Vec<PublicAlertResponse>),
        (status = 503, description = "Database unavailable", body = SimpleErrorResponse),
    )
)]
async fn active(State(state): State<AppState>) -> Result<Json<Vec<PublicAlertResponse>>, AppError> {
    let alerts = alert_service::active_alerts(&state.db).await?;

    Ok(Json(
        alerts.into_iter().map(PublicAlertResponse::from).collect(),
    ))
}

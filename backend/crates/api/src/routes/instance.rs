// crates/api/src/routes/instance.rs
use axum::{Json, extract::State};
use domain::dto::{error_responses::SimpleErrorResponse, instance::PublicInstanceConfig};
use services::instance_service;

use crate::{error::AppError, state::AppState};

pub fn router() -> axum::Router<AppState> {
    axum::Router::new().route("/config", axum::routing::get(public_config))
}

#[utoipa::path(
    get,
    path = "/api/v1/instance/config",
    tag = "Instance",
    responses(
        (status = 200, description = "Public instance configuration", body = PublicInstanceConfig),
        (status = 503, description = "Database unavailable", body = SimpleErrorResponse),
    )
)]
pub async fn public_config(
    State(state): State<AppState>,
) -> Result<Json<PublicInstanceConfig>, AppError> {
    let hide_landing_page = instance_service::cached_hide_landing_page(&state.db).await?;
    Ok(Json(PublicInstanceConfig { hide_landing_page }))
}

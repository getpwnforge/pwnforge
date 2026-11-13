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

    // Straight from the process configuration, not the database: it is fixed
    // for the life of the process, so the settings cache has nothing to add.
    Ok(Json(PublicInstanceConfig {
        hide_landing_page,
        turnstile_site_key: state.config.turnstile_site_key.clone(),
    }))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use crate::test_support::{app, app_with_config, config, db, request_as};

    /// Turnstile off: the key is published as null, so the forms mount no
    /// widget.
    #[tokio::test]
    async fn no_site_key_is_published_when_turnstile_is_off() {
        let db = db().await;
        let app = app(&db).await;

        let (status, body) = request_as(&app, "GET", "/api/v1/instance/config", None, None).await;

        assert_eq!(status, StatusCode::OK);
        assert!(body["turnstile_site_key"].is_null(), "got {body}");
    }

    /// Turnstile on: the site key the widget needs is published.
    #[tokio::test]
    async fn the_site_key_is_published_when_turnstile_is_on() {
        let db = db().await;
        let mut config = config();
        config.turnstile_site_key = Some("1x00000000000000000000AA".to_owned());
        config.turnstile_secret_key = Some("1x0000000000000000000000000000000AA".to_owned());
        let app = app_with_config(&db, config).await;

        let (status, body) = request_as(&app, "GET", "/api/v1/instance/config", None, None).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["turnstile_site_key"], "1x00000000000000000000AA");
    }
}

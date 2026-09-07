pub mod json;
use axum::http::{HeaderMap, header};
use domain::dto::auth::SessionContext;
use sea_orm::prelude::IpNetwork;
use std::net::IpAddr;

pub mod auth;
pub mod client_ip;
pub mod setup;
pub mod tracing;

pub(crate) fn session_context(headers: &HeaderMap, client_ip: Option<IpAddr>) -> SessionContext {
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        // Fully client-controlled: cap it before it reaches the database.
        .map(|s| s.chars().take(512).collect::<String>());

    SessionContext {
        user_agent,
        ip_address: client_ip.map(IpNetwork::from),
    }
}

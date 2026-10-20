use axum::{
    Router,
    extract::{ConnectInfo, State},
    http::StatusCode,
    routing::post,
};
use domain::dto::{
    contact::ContactRequest,
    error_responses::{RateLimitedErrorResponse, SimpleErrorResponse, UnprocessableErrorResponse},
};
use services::contact_service;
use std::net::SocketAddr;
use validator::Validate;

use crate::{
    error::AppError,
    middleware::{client_ip::ClientIp, json::JsonBody},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/contact", post(submit))
}

/// Submits the public contact form.
///
/// Generates a ticket reference used as the subject of both the notification
/// and the confirmation mail, so replies stay in a single thread.
#[utoipa::path(
    post,
    path = "/api/v1/contact",
    tag = "Contact",
    request_body = ContactRequest,
    responses(
        (status = 204, description = "Contact request submitted"),
        (status = 400, description = "malformed_json or invalid_body: the body is not JSON, or could not be read; or turnstile_failed", body = SimpleErrorResponse),
        (status = 415, description = "unsupported_media_type: Content-Type is not application/json", body = SimpleErrorResponse),
        (status = 422, description = "validation_failed or invalid_body", body = UnprocessableErrorResponse),
        (status = 429, description = "Rate limit exceeded", body = RateLimitedErrorResponse),
        (status = 502, description = "email_send_failed: the mail service did not take the message (unreachable, down or refusing it); nothing was sent", body = SimpleErrorResponse),
        (status = 503, description = "service_unavailable: the rate limiter is unreachable, and the route fails closed", body = SimpleErrorResponse),
    )
)]
async fn submit(
    State(state): State<AppState>,
    ClientIp(client_ip): ClientIp,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    JsonBody(payload): JsonBody<ContactRequest>,
) -> Result<StatusCode, AppError> {
    payload.validate()?;

    let rate_limit_ip = client_ip.unwrap_or_else(|| remote.ip());

    contact_service::submit_contact(
        &state.db,
        &mut state.redis.clone(),
        &state.config,
        rate_limit_ip,
        payload,
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::{Request, StatusCode};
    use serde_json::{Value, json};
    use services::config::Config;
    use std::net::{IpAddr, Ipv6Addr, SocketAddr};
    use tower::ServiceExt;
    use uuid::Uuid;

    use crate::test_support::{MailSink, app_with_config, db};

    /// A documentation address (RFC 3849) of its own per call, so no two
    /// tests share the contact form's rate-limit bucket.
    fn fresh_ip() -> IpAddr {
        let id = Uuid::now_v7().as_u128();
        IpAddr::V6(Ipv6Addr::new(
            0x2001,
            0x0db8,
            0xc07,
            (id >> 64) as u16,
            (id >> 48) as u16,
            (id >> 32) as u16,
            (id >> 16) as u16,
            id as u16,
        ))
    }

    async fn contact(config: Config, email: &str) -> (StatusCode, Value) {
        let app = app_with_config(&db().await, config).await;
        let body = json!({
            "name": "A visitor",
            "email": email,
            "category": "general",
            "message": "A message long enough to pass.",
            "turnstile_token": "",
        });
        let mut request = Request::builder()
            .method("POST")
            .uri("/api/v1/contact")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .expect("request must build");
        request
            .extensions_mut()
            .insert(ConnectInfo(SocketAddr::new(fresh_ip(), 0)));
        let response = app.oneshot(request).await.expect("router must answer");
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("the body must be readable");
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).expect("the body must be JSON")
        };
        (status, body)
    }

    /// Control: the notification reaches the operator's address.
    #[tokio::test]
    async fn a_contact_reaches_the_operator() {
        let sink = MailSink::start().await;

        let (status, body) = contact(sink.config(), "visitor@example.test").await;

        assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
        let mails = sink.wait_for(1).await;
        assert!(
            mails
                .iter()
                .any(|mail| mail.to == ["noreply@ctf.example.test"]),
            "{mails:?}"
        );
    }

    /// A mail server that does not answer was a 500: the dependency's
    /// failure, so 502, as for the setup's test mail.
    #[tokio::test]
    async fn an_unreachable_mail_server_is_502_email_send_failed() {
        let closed = std::net::TcpListener::bind("127.0.0.1:0").expect("a port must be free");
        let port = closed.local_addr().expect("an address").port();
        drop(closed);
        let mut config = MailSink::start().await.config();
        config.smtp_port = port;

        let (status, body) = contact(config, "visitor@example.test").await;

        assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
        assert_eq!(body, json!({ "error": "email_send_failed" }));
    }
}

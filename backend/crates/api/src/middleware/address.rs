// crates/api/src/middleware/address.rs
//! The check every route that takes an email address runs on it, right after
//! the request's own validation and before anything else: rate limits,
//! lookups, writes, mail.
//!
//! The `email` rule of the request bodies takes addresses the mail transport
//! cannot carry, `a..b@example.test` or `a@[127.0.0.1]`: stored or mailed,
//! they failed later, as a 500 or as a mail that never left. This parses the
//! address as the transport will (`email_service::is_valid_mailbox`) and
//! answers a refusal exactly as the `email` rule answers a malformed address:
//! 422 `validation_failed`, the field listed with the code `email`. A client
//! cannot tell the two apart, and only the form of the address is looked at,
//! never whether an account holds it.

use services::email_service;
use validator::{ValidationError, ValidationErrors};

use crate::error::AppError;

/// Refuses `address`, sent as `field`, unless a mail can be addressed to it.
pub fn require_mailbox(field: &'static str, address: &str) -> Result<(), AppError> {
    if email_service::is_valid_mailbox(address) {
        return Ok(());
    }

    let mut errors = ValidationErrors::new();
    errors.add(field, ValidationError::new("email"));
    Err(errors.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;

    /// The same answer as the `email` rule of a request body: the status, the
    /// code and the field.
    #[tokio::test]
    async fn an_address_no_mail_can_go_to_is_answered_as_a_malformed_one() {
        for address in ["a..b@example.test", "a@[127.0.0.1]"] {
            let response = require_mailbox("email", address)
                .expect_err("refused")
                .into_response();

            assert_eq!(
                response.status(),
                axum::http::StatusCode::UNPROCESSABLE_ENTITY
            );
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("the body must be readable");
            let body: serde_json::Value =
                serde_json::from_slice(&bytes).expect("the body must be JSON");
            assert_eq!(body["error"], "validation_failed", "{address}");
            assert_eq!(body["fields"], serde_json::json!({ "email": ["email"] }));
        }
    }

    #[test]
    fn an_address_a_mail_can_go_to_passes() {
        for address in [
            "visitor@example.test",
            "A.B-c+d@Example.TEST",
            "a@exämple.test",
        ] {
            assert!(require_mailbox("email", address).is_ok(), "{address}");
        }
    }
}

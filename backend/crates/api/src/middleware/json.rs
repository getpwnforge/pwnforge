// crates/api/src/middleware/json.rs
use axum::{
    Json,
    extract::{FromRequest, Request, rejection::JsonRejection},
};

use crate::error::AppError;

/// Drop-in replacement for `Json<T>` in handler arguments.
///
/// `axum::Json` answers a malformed body itself, before the handler runs, with
/// a `text/plain` message describing the failure. That bypasses `AppError`
/// entirely: the client gets a shape it cannot parse, and the message spells
/// out the field names the endpoint expects.
///
/// This wrapper does nothing but route the rejection through `AppError`, so
/// every response leaving the API carries the same JSON envelope and a
/// machine-readable code.
///
/// Only for extraction. `Json` stays as-is in return position.
pub struct JsonBody<T>(pub T);

impl<S, T> FromRequest<S> for JsonBody<T>
where
    Json<T>: FromRequest<S, Rejection = JsonRejection>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req, state).await?;
        Ok(Self(value))
    }
}

// crates/domain/src/dto/error_responses.rs
//
// Types used ONLY for OpenAPI documentation. They describe the shape of the
// error bodies produced by AppError::into_response(), but are never built nor
// serialized by the application itself: the real responses are assembled with
// serde_json::json! in error.rs.
//
// Keep in sync with error.rs: a new body shape there needs a new type here,
// otherwise the documented responses stop matching what clients receive.

/// `{ "error": "invalid_credentials" }` and every other single-code failure.
/// Covers the majority of the error surface.
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct SimpleErrorResponse {
    /// Stable machine-readable code, never a human sentence.
    pub error: String,
}

/// `validation_failed`, with the offending fields mapped to validator codes.
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ValidationErrorResponse {
    pub error: String,
    /// Field name to the list of validator codes it failed.
    #[schema(value_type = Object)]
    pub fields: std::collections::HashMap<String, Vec<String>>,
}

/// `rate_limited`, mirrored by the Retry-After header.
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct RateLimitedErrorResponse {
    pub error: String,
    pub retry_after_secs: u64,
}

/// `email_not_verified`, carrying the token needed to request a new
/// verification mail without a second round trip.
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct EmailNotVerifiedErrorResponse {
    pub error: String,
    pub resend_token: String,
}

/// `legal_version_stale`, carrying the current versions so the client can
/// re-prompt immediately.
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct LegalVersionStaleErrorResponse {
    pub error: String,
    pub current_terms_version: String,
    pub current_privacy_version: String,
}

// crates/api/src/openapi.rs
//
// Central aggregation point for the OpenAPI spec. Every annotated handler
// must be listed in `paths(...)` and every DTO it references in
// `components(schemas(...))`. An omission does not break the build: the route
// is simply absent from the generated spec, silently.

use utoipa::{
    Modify, OpenApi,
    openapi::security::{ApiKey, ApiKeyValue, SecurityScheme},
};

/// Declares the two session cookies. They are separate schemes on purpose:
/// the access token is sent on every request, while the refresh token is
/// scoped to /api/v1/auth and only reaches the refresh and logout routes.
///
// Both are consumed through macro-generated code and, for ApiDoc, only from
// the export test behind #[cfg(test)]. Neither is constructed on the binary's
// own code path, which is what the dead-code lint looks at.
#[allow(dead_code)]
struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "access_token",
                SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new("access_token"))),
            );
            components.add_security_scheme(
                "refresh_token",
                SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new("refresh_token"))),
            );
        }
    }
}

#[allow(dead_code)]
#[derive(OpenApi)]
#[openapi(
    paths(
        crate::routes::auth::register,
        crate::routes::auth::login,
        crate::routes::auth::refresh,
        crate::routes::auth::logout,
        crate::routes::auth::me,
        crate::routes::auth::password_forgot,
        crate::routes::auth::password_reset,
        crate::routes::auth::password_change,
        crate::routes::auth::email_verify,
        crate::routes::auth::email_resend,
        crate::routes::setup::status,
        crate::routes::setup::email_config,
        crate::routes::setup::test_email,
        crate::routes::setup::validate_admin,
        crate::routes::setup::complete,
        crate::routes::legal::status,
        crate::routes::legal::accept,
        crate::routes::legal::versions,
        crate::routes::alerts::active,
        crate::routes::contact::submit,
        crate::routes::instance::public_config,
        crate::routes::health::health,
    ),
    components(schemas(
        domain::dto::auth::RegisterRequest,
        domain::dto::auth::LoginRequest,
        domain::dto::auth::UserResponse,
        domain::dto::auth::PasswordForgotRequest,
        domain::dto::auth::PasswordResetRequest,
        domain::dto::auth::PasswordChangeRequest,
        domain::dto::auth::EmailVerifyRequest,
        domain::dto::auth::EmailResendRequest,
        domain::dto::setup::SetupStatusResponse,
        domain::dto::setup::EmailConfigResponse,
        domain::dto::setup::TestEmailRequest,
        domain::dto::setup::ValidateAdminRequest,
        domain::dto::setup::SetupRequest,
        domain::dto::legal::LegalAcceptanceInput,
        domain::dto::legal::LegalVersionsDto,
        domain::dto::legal::LegalStatusDto,
        domain::dto::legal::LegalState,
        domain::dto::alerts::PublicAlertResponse,
        domain::dto::contact::ContactRequest,
        domain::dto::instance::PublicInstanceConfig,
        domain::dto::health::HealthResponse,
        domain::types::ContactCategory,
        domain::dto::error_responses::SimpleErrorResponse,
        domain::dto::error_responses::ValidationErrorResponse,
        domain::dto::error_responses::RateLimitedErrorResponse,
        domain::dto::error_responses::EmailNotVerifiedErrorResponse,
        domain::dto::error_responses::LegalVersionStaleErrorResponse,
    )),
    modifiers(&SecurityAddon),
    tags(
        (name = "Auth", description = "Authentication and session management"),
        (name = "Setup", description = "First-run instance setup wizard"),
        (name = "Legal", description = "Legal document versions and acceptance"),
        (name = "Alerts", description = "Instance-wide alert banners"),
        (name = "Contact", description = "Public contact form"),
        (name = "Instance", description = "Public instance configuration"),
        (name = "Health", description = "Service health probe"),
    ),
    info(
        title = "PwnForge API",
        version = "0.1.0",
        description = "Reference documentation for the PwnForge API."
    )
)]
pub struct ApiDoc;

#[cfg(test)]
mod tests {
    use super::*;

    /// Not a real test: writes the spec to disk on demand.
    /// Run with: cargo test -p api openapi::tests::export -- --ignored
    #[test]
    #[ignore]
    fn export() {
        let spec = ApiDoc::openapi()
            .to_pretty_json()
            .expect("failed to serialize OpenAPI spec");
        std::fs::write("openapi.json", spec).expect("failed to write openapi.json");
    }
}

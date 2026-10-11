// crates/domain/src/dto/instance.rs
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct PublicInstanceConfig {
    pub hide_landing_page: bool,
    /// The Turnstile site key the protected forms render their widget with,
    /// or `null` when the instance has Turnstile off.
    // Public by design: it is in every page that shows the widget. Always
    // present, hence required.
    #[schema(required = true)]
    pub turnstile_site_key: Option<String>,
}

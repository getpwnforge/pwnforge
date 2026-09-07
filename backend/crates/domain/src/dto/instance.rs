// crates/domain/src/dto/instance.rs
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct PublicInstanceConfig {
    pub hide_landing_page: bool,
}

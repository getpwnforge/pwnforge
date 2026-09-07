// crates/domain/src/dto/health.rs

/// Reported as strings rather than booleans: a monitoring probe reads the
/// same shape whether it looks at the top-level status or a single component.
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct HealthResponse {
    /// "ok" when every component is reachable, "down" otherwise.
    pub status: &'static str,
    pub db: &'static str,
    pub redis: &'static str,
}

impl HealthResponse {
    pub fn new(db_ok: bool, redis_ok: bool) -> Self {
        fn state(ok: bool) -> &'static str {
            if ok { "ok" } else { "down" }
        }

        Self {
            status: state(db_ok && redis_ok),
            db: state(db_ok),
            redis: state(redis_ok),
        }
    }
}

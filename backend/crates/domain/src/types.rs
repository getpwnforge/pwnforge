// crates/domain/src/types.rs

#[derive(Debug, Clone, Copy)]
pub enum BlockSource {
    Manual,
    DisposableList,
}

impl BlockSource {
    pub fn as_str(self) -> &'static str {
        match self {
            BlockSource::Manual => "manual",
            BlockSource::DisposableList => "disposable-list",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ReservedSource {
    Manual,
    ReservedList,
}

impl ReservedSource {
    pub fn as_str(self) -> &'static str {
        match self {
            ReservedSource::Manual => "manual",
            ReservedSource::ReservedList => "reserved-list",
        }
    }
}

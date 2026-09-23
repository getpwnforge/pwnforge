// crates/domain/src/serde_helpers.rs
//
// Generic serde utilities shared across DTOs. Not domain-specific: nothing
// here references workspaces, teams, or any other entity.

use serde::{Deserialize, Deserializer};

/// Distinguishes "field absent" from "field present" on an `Option<Option<T>>`
/// used with `#[serde(default)]`.
///
/// Without this, a PATCH payload can't tell "don't touch this field" from
/// "clear this field": both an absent key and `"field": null` deserialize an
/// `Option<T>` to the same `None`. Combined with `#[serde(default)]`, this
/// deserializer makes only the absent-key case stay `None`; a present `null`
/// becomes `Some(None)` (clear it), and a present value becomes
/// `Some(Some(value))` (set it).
///
/// Usage: `#[serde(default, deserialize_with = "deserialize_some")] pub
/// field: Option<Option<T>>`.
pub fn deserialize_some<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Deserialize::deserialize(deserializer).map(Some)
}

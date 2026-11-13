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

/// `deserialize_some` for nullable text that is trimmed on the way in: a
/// string that is empty once trimmed reads as `null`, so it clears the field
/// rather than storing blank text.
///
/// Trimmed here rather than in the service so that `validate()` measures what
/// will be stored, and a length bound on the field means the same thing as
/// the database check behind it.
///
/// Usage: `#[serde(default, deserialize_with = "deserialize_some_trimmed")]
/// pub field: Option<Option<String>>`.
pub fn deserialize_some_trimmed<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    let text: Option<String> = Deserialize::deserialize(deserializer)?;
    Ok(Some(text.and_then(|text| {
        let trimmed = text.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    })))
}

//! Shared date/time parsing for CLI flags. Accepts either a full RFC 3339
//! timestamp or a bare date (midnight UTC) - typing a full timestamp by
//! hand for something like `--starts-at 2026-08-30` is needless friction
//! most of the time.

use chrono::{DateTime, NaiveDate, Utc};

pub fn parse_datetime(s: &str) -> Result<DateTime<Utc>, String> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }

    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Ok(date.and_hms_opt(0, 0, 0).unwrap().and_utc());
    }

    Err(format!(
        "'{s}' is not a valid date/time; use RFC 3339 (2026-08-30T14:00:00Z) or a bare date (2026-08-30)"
    ))
}

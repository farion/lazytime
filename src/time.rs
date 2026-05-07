use anyhow::{Context, Result};
use chrono::{DateTime, Local, NaiveDateTime, TimeZone, Utc};

/// Format timestamps as local time (YYYY-MM-ddTHH:mm:ss, no timezone suffix)
pub fn format_ts(dt: &DateTime<Utc>) -> String {
    dt.with_timezone(&Local)
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string()
}

/// Format timestamps for logs in local time (YYYY-MM-ddTHH:mm:ss)
pub fn format_ts_local(dt: &DateTime<Utc>) -> String {
    format_ts(dt)
}

/// Parse timestamps as local time (or RFC3339 with explicit offset) and return UTC instant.
pub fn parse_ts(s: &str) -> Result<DateTime<Utc>> {
    // RFC3339 keeps explicit timezone/offset semantics.
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }

    // Naive timestamps are interpreted as LOCAL time.
    for fmt in [
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M",
    ] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(s, fmt) {
            return Ok(Local
                .from_local_datetime(&naive)
                .earliest()
                .or_else(|| Local.from_local_datetime(&naive).latest())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|| Utc.from_utc_datetime(&naive)));
        }
    }

    let naive = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").with_context(|| {
        format!(
            "invalid timestamp format, expected YYYY-MM-ddTHH:mm:ss or RFC3339: {}",
            s
        )
    })?;
    Ok(Local
        .from_local_datetime(&naive)
        .earliest()
        .or_else(|| Local.from_local_datetime(&naive).latest())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|| Utc.from_utc_datetime(&naive)))
}

/// Parse user-facing timestamps as local time and convert to UTC for storage.
pub fn parse_local_ts(s: &str) -> Result<DateTime<Utc>> {
    parse_ts(s)
}

use chrono::{TimeZone, Utc};

/// Format a millisecond timestamp as UTC string, or return a placeholder on error.
pub fn format_timestamp(ts_millis: i64) -> String {
    match Utc.timestamp_millis_opt(ts_millis) {
        chrono::LocalResult::Single(datetime) => datetime.format("%Y-%m-%d %H:%M:%S").to_string(),
        _ => "invalid timestamp".to_string(),
    }
}

/// Format an optional millisecond timestamp, using '-' when missing.
pub fn format_timestamp_opt(ts: Option<i64>) -> String {
    ts.map(format_timestamp).unwrap_or_else(|| "-".to_string())
}

use std::fs;
use std::path::Path;
use std::time::SystemTime;

use anyhow::{Context, Result};
use chrono::DateTime;

pub(crate) fn parse_timestamp(raw: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|value| value.timestamp_millis())
}

fn system_time_to_timestamp_millis(raw: SystemTime) -> Option<i64> {
    raw.duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .and_then(|value| i64::try_from(value.as_millis()).ok())
}

pub(crate) fn file_modified_timestamp_millis(path: &Path) -> Result<i64> {
    Ok(fs::metadata(path)
        .with_context(|| format!("Failed to read metadata for {}", path.display()))?
        .modified()
        .ok()
        .and_then(system_time_to_timestamp_millis)
        .unwrap_or_default())
}

use anyhow::{Context, Result};
use serde_json::Value;

pub(crate) fn parse_json_line(line: &str) -> Result<Value> {
    serde_json::from_str(line).with_context(|| format!("Invalid JSONL line: {line}"))
}

pub(crate) fn json_type(value: &Value) -> Option<&str> {
    value.get("type").and_then(Value::as_str)
}

pub(crate) fn json_string(value: &Value, keys: &[&str]) -> Option<String> {
    let mut current = value;

    for key in keys {
        current = current.get(*key)?;
    }

    current.as_str().map(ToString::to_string)
}

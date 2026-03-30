use serde_json::Value;

/// Extract a string field from a JSON value, returning "" if missing or non-string.
pub(crate) fn str_field(val: &Value, key: &str) -> String {
    val.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// Extract an optional string field from a JSON value.
pub(crate) fn opt_str(val: &Value, key: &str) -> Option<String> {
    val.get(key).and_then(|v| v.as_str()).map(String::from)
}

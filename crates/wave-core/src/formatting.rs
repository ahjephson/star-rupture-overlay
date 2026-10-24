//! Time/number formatting retained for wire compatibility. No save-file access.
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};
pub fn now_ms() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_millis() as i64,
        Err(e) => -(e.duration().as_millis() as i64),
    }
}
pub fn iso(ms: i64) -> String {
    DateTime::<Utc>::from_timestamp_millis(ms)
        .map(|d| d.to_rfc3339_opts(SecondsFormat::Millis, true))
        .unwrap_or_default()
}
// Only validated numeric PIDs are passed here. Preserve the original JS numeric formatting.
pub fn js_json(v: &Value) -> String {
    match v {
        Value::Number(n) => ryu_js::Buffer::new().format(n.as_f64().unwrap()).into(),
        _ => v.to_string(),
    }
}

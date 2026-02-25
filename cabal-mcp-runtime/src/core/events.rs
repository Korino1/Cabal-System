use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    pub ts_unix: u64,
    pub kind: String,
    pub digest: u64,
    #[serde(default)]
    pub summary: String,
}

pub fn summarize_payload(payload: &Value) -> String {
    let mut s = payload.to_string();
    if s.len() > 300 {
        s.truncate(300);
    }
    s
}

pub fn truncate_text(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let mut out = s.to_string();
        out.truncate(max);
        out
    }
}

pub fn event_hash_material(kind: &str, payload: &Value, ts_unix: u64) -> Result<String> {
    Ok(serde_json::to_string(&json!({
        "kind": kind,
        "payload": payload,
        "ts_unix": ts_unix
    }))?)
}

pub fn build_event_record(
    kind: &str,
    payload: &Value,
    ts_unix: u64,
    digest: u64,
) -> Result<EventRecord> {
    let body = json!({
        "kind": kind,
        "payload": payload,
        "ts_unix": ts_unix
    });
    let kind = body["kind"]
        .as_str()
        .ok_or_else(|| anyhow!("event kind missing"))?
        .to_string();
    Ok(EventRecord {
        ts_unix,
        kind,
        digest,
        summary: summarize_payload(payload),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn summarize_payload_truncates_to_limit() {
        let payload = json!({
            "text": "x".repeat(400)
        });
        let out = summarize_payload(&payload);
        assert!(out.len() <= 300);
    }

    #[test]
    fn truncate_text_respects_max() {
        assert_eq!(truncate_text("abcd", 2), "ab");
        assert_eq!(truncate_text("ab", 4), "ab");
    }

    #[test]
    fn event_hash_material_contains_canonical_fields() {
        let raw = event_hash_material("k", &json!({"x": 1}), 7).expect("hash material");
        assert!(raw.contains("\"kind\":\"k\""));
        assert!(raw.contains("\"ts_unix\":7"));
    }

    #[test]
    fn build_event_record_sets_summary_and_kind() {
        let payload = json!({"message": "hello"});
        let rec = build_event_record("evt", &payload, 11, 99).expect("event record");
        assert_eq!(rec.ts_unix, 11);
        assert_eq!(rec.kind, "evt");
        assert_eq!(rec.digest, 99);
    }
}

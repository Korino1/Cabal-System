use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyTraceRecord {
    pub ts_unix: u64,
    pub category: String,
    pub operation: String,
    pub target: String,
    pub allow: bool,
    pub executed: bool,
    pub reason: String,
    pub digest: u64,
}

pub fn evaluate_proxy_request(
    proxy_allow: &BTreeMap<String, Vec<String>>,
    proxy_allowed_operations: &BTreeMap<String, Vec<String>>,
    proxy_denied_operations: &BTreeMap<String, Vec<String>>,
    deny_by_default: bool,
    category: &str,
    operation: &str,
    target: &str,
) -> Result<Value> {
    if category.trim().is_empty() || operation.trim().is_empty() || target.trim().is_empty() {
        bail!("category, operation and target are required");
    }

    let category_norm = category.trim().to_ascii_lowercase();
    let operation_norm = operation.trim().to_ascii_lowercase();

    if let Some(denylist) = proxy_denied_operations.get(&category_norm)
        && operation_is_listed(denylist, &operation_norm)
    {
        return Ok(json!({
            "allow": false,
            "mode": if deny_by_default { "deny_by_default" } else { "allow_by_default" },
            "reason": "operation is in denylist",
            "category": category_norm,
            "operation": operation_norm,
            "target": target
        }));
    }

    if let Some(allow_ops) = proxy_allowed_operations.get(&category_norm)
        && !allow_ops.is_empty()
        && !operation_is_listed(allow_ops, &operation_norm)
    {
        return Ok(json!({
            "allow": false,
            "mode": if deny_by_default { "deny_by_default" } else { "allow_by_default" },
            "reason": "operation is not in allowlist",
            "category": category_norm,
            "operation": operation_norm,
            "target": target
        }));
    }

    let allow = proxy_allow
        .get(&category_norm)
        .map(|prefixes| prefixes.iter().any(|prefix| target.starts_with(prefix)))
        .unwrap_or(false);

    if allow {
        Ok(json!({
            "allow": true,
            "mode": if deny_by_default { "deny_by_default" } else { "allow_by_default" },
            "reason": "target matched allowlist prefix",
            "category": category_norm,
            "operation": operation_norm,
            "target": target
        }))
    } else if deny_by_default {
        Ok(json!({
            "allow": false,
            "mode": "deny_by_default",
            "reason": "target is not in allowlist",
            "category": category_norm,
            "operation": operation_norm,
            "target": target
        }))
    } else {
        Ok(json!({
            "allow": true,
            "mode": "allow_by_default",
            "reason": "allow-by-default mode",
            "category": category_norm,
            "operation": operation_norm,
            "target": target
        }))
    }
}

fn operation_is_listed(operations: &[String], operation: &str) -> bool {
    operations
        .iter()
        .any(|item| item == "*" || item == operation)
}

pub fn proxy_trace_hash_input(
    ts_unix: u64,
    category: &str,
    operation: &str,
    target: &str,
    allow: bool,
    executed: bool,
    reason: &str,
) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}",
        ts_unix, category, operation, target, allow, executed, reason
    )
}

pub fn build_proxy_trace_record(
    ts_unix: u64,
    category: &str,
    operation: &str,
    target: &str,
    allow: bool,
    executed: bool,
    reason: &str,
    digest: u64,
) -> ProxyTraceRecord {
    ProxyTraceRecord {
        ts_unix,
        category: category.to_string(),
        operation: operation.to_string(),
        target: target.to_string(),
        allow,
        executed,
        reason: reason.to_string(),
        digest,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deny_by_default_blocks_missing_allowlist() {
        let allow = BTreeMap::from([("fs".to_string(), vec![".memory/".to_string()])]);
        let allow_ops = BTreeMap::from([
            ("fs".to_string(), vec!["read_text".to_string()]),
            ("shell".to_string(), vec!["run".to_string()]),
        ]);
        let deny_ops = BTreeMap::new();
        let out = evaluate_proxy_request(
            &allow,
            &allow_ops,
            &deny_ops,
            true,
            "shell",
            "run",
            "cargo test",
        )
        .expect("out");
        assert_eq!(out["allow"].as_bool(), Some(false));
    }

    #[test]
    fn allowlist_prefix_allows_request() {
        let allow = BTreeMap::from([("fs".to_string(), vec![".memory/".to_string()])]);
        let allow_ops = BTreeMap::from([("fs".to_string(), vec!["read_text".to_string()])]);
        let deny_ops = BTreeMap::new();
        let out = evaluate_proxy_request(
            &allow,
            &allow_ops,
            &deny_ops,
            true,
            "fs",
            "read_text",
            ".memory/x.md",
        )
        .expect("out");
        assert_eq!(out["allow"].as_bool(), Some(true));
    }

    #[test]
    fn operation_denylist_blocks_request() {
        let allow = BTreeMap::from([("shell".to_string(), vec!["cargo".to_string()])]);
        let allow_ops = BTreeMap::from([("shell".to_string(), vec!["run".to_string()])]);
        let deny_ops = BTreeMap::from([("shell".to_string(), vec!["run".to_string()])]);
        let out = evaluate_proxy_request(
            &allow,
            &allow_ops,
            &deny_ops,
            false,
            "shell",
            "run",
            "cargo test",
        )
        .expect("out");
        assert_eq!(out["allow"].as_bool(), Some(false));
        assert_eq!(out["reason"].as_str(), Some("operation is in denylist"));
    }

    #[test]
    fn operation_allowlist_blocks_unknown_operation() {
        let allow = BTreeMap::from([("fs".to_string(), vec![".memory/".to_string()])]);
        let allow_ops = BTreeMap::from([("fs".to_string(), vec!["read_text".to_string()])]);
        let deny_ops = BTreeMap::new();
        let out = evaluate_proxy_request(
            &allow,
            &allow_ops,
            &deny_ops,
            false,
            "fs",
            "write_text",
            ".memory/x.md",
        )
        .expect("out");
        assert_eq!(out["allow"].as_bool(), Some(false));
        assert_eq!(
            out["reason"].as_str(),
            Some("operation is not in allowlist")
        );
    }

    #[test]
    fn trace_hash_input_has_stable_format() {
        let raw = proxy_trace_hash_input(1, "fs", "read_text", "a.txt", true, true, "ok");
        assert_eq!(raw, "1|fs|read_text|a.txt|true|true|ok");
    }

    #[test]
    fn build_trace_record_copies_fields() {
        let rec = build_proxy_trace_record(1, "fs", "read_text", "a.txt", true, false, "x", 99);
        assert_eq!(rec.ts_unix, 1);
        assert_eq!(rec.category, "fs");
        assert_eq!(rec.digest, 99);
    }
}

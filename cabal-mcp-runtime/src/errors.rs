use anyhow::{Error, Result, anyhow, bail};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct ClassifiedError {
    pub rpc_code: i64,
    pub cabal_code: &'static str,
    pub retryable: bool,
    pub message: String,
}

pub fn classify_error(method: &str, tool_name: Option<&str>, err: &Error) -> ClassifiedError {
    let msg = err.to_string();
    let lower = msg.to_ascii_lowercase();
    let contains_any = |patterns: &[&str]| patterns.iter().any(|p| lower.contains(p));

    if lower.contains("unsupported method") {
        return ClassifiedError {
            rpc_code: -32601,
            cabal_code: "UNSUPPORTED_METHOD",
            retryable: false,
            message: msg,
        };
    }
    if lower.contains("unknown tool") {
        return ClassifiedError {
            rpc_code: -32601,
            cabal_code: "UNKNOWN_TOOL",
            retryable: false,
            message: msg,
        };
    }
    if contains_any(&[
        "failed to parse newline-delimited jsonrpc message",
        "invalid jsonrpc body",
    ]) {
        return ClassifiedError {
            rpc_code: -32700,
            cabal_code: "PARSE_ERROR",
            retryable: false,
            message: msg,
        };
    }
    if contains_any(&[
        "missing content-length header",
        "invalid content-length header",
        "invalid content-length value",
        "content-length must be > 0",
        "failed to read jsonrpc body",
    ]) {
        return ClassifiedError {
            rpc_code: -32060,
            cabal_code: "TRANSPORT_ERROR",
            retryable: false,
            message: msg,
        };
    }
    if lower.contains("policy revision mismatch") {
        return ClassifiedError {
            rpc_code: -32010,
            cabal_code: "REVISION_MISMATCH",
            retryable: true,
            message: msg,
        };
    }
    if contains_any(&[
        "signature verification failed",
        "invalid signature",
        "signature is required",
        "signature must be hex",
        "invalid hmac key",
        "signing key is revoked",
        "signing key is not active yet",
        "signing key expired",
        "required for signed policy mode",
        "unsupported signing algorithm",
        "invalid signature file format",
    ]) {
        return ClassifiedError {
            rpc_code: -32011,
            cabal_code: "SIGNATURE_INVALID",
            retryable: false,
            message: msg,
        };
    }
    if lower.contains("nonce replay") {
        return ClassifiedError {
            rpc_code: -32012,
            cabal_code: "NONCE_REPLAY",
            retryable: false,
            message: msg,
        };
    }
    if lower.contains("entry gate failed")
        || lower.contains("exit gate failed")
        || lower.contains("unsupported gate kind")
    {
        return ClassifiedError {
            rpc_code: -32020,
            cabal_code: "GATE_FAIL",
            retryable: false,
            message: msg,
        };
    }
    if contains_any(&["forbidden token", "policy deny"]) {
        return ClassifiedError {
            rpc_code: -32030,
            cabal_code: "POLICY_DENY",
            retryable: false,
            message: msg,
        };
    }
    if method == "tools/call"
        && matches!(
            tool_name,
            Some("cabal.proxy_request" | "cabal.proxy_execute")
        )
        && (lower.contains("unsupported proxy category")
            || lower.contains("unsupported fs operation")
            || lower.contains("unsupported shell operation")
            || lower.contains("unsupported network operation")
            || lower.contains("allowlist")
            || lower.contains("shell command blocked by policy")
            || lower.contains("network target blocked by policy")
            || lower.contains("invalid network target url"))
    {
        return ClassifiedError {
            rpc_code: -32031,
            cabal_code: "PROXY_DENY",
            retryable: false,
            message: msg,
        };
    }
    if contains_any(&[
        "failed to run shell command",
        "shell command timed out",
        "http_get failed",
        "failed to read response body",
    ]) {
        return ClassifiedError {
            rpc_code: -32040,
            cabal_code: "EXECUTOR_FAILURE",
            retryable: true,
            message: msg,
        };
    }
    if contains_any(&[
        "failed to read runtime state",
        "failed to write runtime state",
        "failed to open audit log",
        "failed to write audit record",
        "failed to write audit newline",
        "failed to create export dir",
        "failed to create export file",
        "failed to write audit export record",
        "failed to write audit export newline",
    ]) {
        return ClassifiedError {
            rpc_code: -32050,
            cabal_code: "STORAGE_FAILURE",
            retryable: true,
            message: msg,
        };
    }
    if contains_any(&[
        "failed to read file:",
        "failed to write file:",
        "failed to list dir:",
        "failed to read error codes doc:",
        "invalid network target url",
    ]) {
        return ClassifiedError {
            rpc_code: -32051,
            cabal_code: "IO_FAILURE",
            retryable: false,
            message: msg,
        };
    }
    if lower.contains("failed to parse runtime state") {
        return ClassifiedError {
            rpc_code: -32052,
            cabal_code: "STATE_CORRUPT",
            retryable: false,
            message: msg,
        };
    }
    if contains_any(&[
        "is required",
        "unsupported mode",
        "unsupported consult priority",
        "invalid phase transition",
        "unknown phase",
        "unknown key_id",
        "must be <=",
        "absolute paths are forbidden",
        "path traversal",
        "key_id and key_env are required",
        "rules must not be empty",
        "consult_type and executor are required",
        "timeout_sec must be > 0",
        "max_retries must be <=",
        "roles must not be empty",
        "shell target command is too long",
        "unsupported escalation target",
        "confidence_floor must be in [0,1]",
        "exploration_rate must be in [0,1]",
        "exploration_min_samples must be > 0",
        "consult_type and executor are required for feedback",
        "latency_ms must be > 0",
        "agent_ack_path and subagent_ack_path are required",
        "allowed_profiles must not be empty",
        "unsupported ide profile",
        "archive_path is required",
        "archive_dir is required",
        "audit log is empty",
        "keep_last must be > 0",
        "verify_last must be > 0",
        "max_bytes must be > 0",
        "max_age_sec must be > 0",
        "payload.text is too large",
        "limit must be > 0",
        "query is too long",
        "calls exceeds max_calls policy",
        "programmatic_call recursion is forbidden",
        "calls[].name is required",
        "calls[].arguments must be an object",
        "max_calls must be in [1, 256]",
        "max_chars must be in [256, 200000]",
        "preview_items must be in [1, 128]",
        "lazy_threshold_pct must be in [1, 95]",
        "programmatic_max_calls must be in [1, 256]",
    ]) {
        return ClassifiedError {
            rpc_code: -32602,
            cabal_code: "INVALID_REQUEST",
            retryable: false,
            message: msg,
        };
    }

    ClassifiedError {
        rpc_code: -32000,
        cabal_code: "INTERNAL_ERROR",
        retryable: true,
        message: msg,
    }
}

pub fn error_codes_catalog() -> Value {
    json!({
        "codes": [
            {"cabal_code": "PARSE_ERROR", "rpc_code": -32700, "retryable": false, "description": "Malformed JSON-RPC payload."},
            {"cabal_code": "TRANSPORT_ERROR", "rpc_code": -32060, "retryable": false, "description": "Invalid MCP transport frame/headers."},
            {"cabal_code": "UNSUPPORTED_METHOD", "rpc_code": -32601, "retryable": false, "description": "Unsupported JSON-RPC method."},
            {"cabal_code": "UNKNOWN_TOOL", "rpc_code": -32601, "retryable": false, "description": "Unknown MCP tool name."},
            {"cabal_code": "INVALID_REQUEST", "rpc_code": -32602, "retryable": false, "description": "Invalid params or unsupported argument values."},
            {"cabal_code": "REVISION_MISMATCH", "rpc_code": -32010, "retryable": true, "description": "Policy expected_revision does not match current revision."},
            {"cabal_code": "SIGNATURE_INVALID", "rpc_code": -32011, "retryable": false, "description": "Policy signature invalid or missing in signed mode."},
            {"cabal_code": "NONCE_REPLAY", "rpc_code": -32012, "retryable": false, "description": "Policy nonce replay detected."},
            {"cabal_code": "GATE_FAIL", "rpc_code": -32020, "retryable": false, "description": "Phase gate entry/exit validation failed."},
            {"cabal_code": "POLICY_DENY", "rpc_code": -32030, "retryable": false, "description": "Action denied by policy."},
            {"cabal_code": "PROXY_DENY", "rpc_code": -32031, "retryable": false, "description": "Proxy request denied by policy/allowlist."},
            {"cabal_code": "EXECUTOR_FAILURE", "rpc_code": -32040, "retryable": true, "description": "Underlying executor/tool failed."},
            {"cabal_code": "STORAGE_FAILURE", "rpc_code": -32050, "retryable": true, "description": "Runtime storage/audit persistence failure."},
            {"cabal_code": "IO_FAILURE", "rpc_code": -32051, "retryable": false, "description": "Filesystem operation failed for requested target."},
            {"cabal_code": "STATE_CORRUPT", "rpc_code": -32052, "retryable": false, "description": "Runtime state file is malformed or corrupted."},
            {"cabal_code": "INTERNAL_ERROR", "rpc_code": -32000, "retryable": true, "description": "Unhandled internal runtime error."}
        ]
    })
}

pub fn validate_error_codes_doc_parity(doc: &str) -> Result<Value> {
    let runtime = runtime_error_code_index()?;
    let doc_map = parse_error_codes_markdown(doc)?;

    let mut missing_in_doc = Vec::new();
    let mut missing_in_runtime = Vec::new();
    let mut mismatches = Vec::new();

    for (code, (rpc, retry)) in &runtime {
        match doc_map.get(code) {
            None => missing_in_doc.push(code.clone()),
            Some((doc_rpc, doc_retry)) => {
                if doc_rpc != rpc || doc_retry != retry {
                    mismatches.push(json!({
                        "cabal_code": code,
                        "runtime": {"rpc_code": rpc, "retryable": retry},
                        "doc": {"rpc_code": doc_rpc, "retryable": doc_retry}
                    }));
                }
            }
        }
    }
    for code in doc_map.keys() {
        if !runtime.contains_key(code) {
            missing_in_runtime.push(code.clone());
        }
    }

    let pass = missing_in_doc.is_empty() && missing_in_runtime.is_empty() && mismatches.is_empty();
    Ok(json!({
        "pass": pass,
        "runtime_total": runtime.len(),
        "doc_total": doc_map.len(),
        "missing_in_doc": missing_in_doc,
        "missing_in_runtime": missing_in_runtime,
        "mismatches": mismatches
    }))
}

fn runtime_error_code_index() -> Result<BTreeMap<String, (i64, bool)>> {
    let catalog = error_codes_catalog();
    let rows = catalog
        .get("codes")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("error_codes_catalog: codes array missing"))?;
    let mut out = BTreeMap::new();
    for row in rows {
        let code = row
            .get("cabal_code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("error_codes_catalog: cabal_code missing"))?;
        let rpc_code = row
            .get("rpc_code")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| anyhow!("error_codes_catalog: rpc_code missing"))?;
        let retryable = row
            .get("retryable")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| anyhow!("error_codes_catalog: retryable missing"))?;
        out.insert(code.to_string(), (rpc_code, retryable));
    }
    Ok(out)
}

fn parse_error_codes_markdown(doc: &str) -> Result<BTreeMap<String, (i64, bool)>> {
    let mut out = BTreeMap::new();
    for raw in doc.lines() {
        let line = raw.trim();
        if !line.starts_with('|') {
            continue;
        }
        let cols: Vec<&str> = line.split('|').map(|x| x.trim()).collect();
        if cols.len() < 6 {
            continue;
        }
        let code = normalize_md_cell(cols[1]);
        let rpc_raw = normalize_md_cell(cols[2]);
        let retry_raw = normalize_md_cell(cols[3]).to_ascii_lowercase();

        if code.is_empty()
            || code.eq_ignore_ascii_case("cabal_code")
            || code.starts_with("---")
            || code.starts_with(":---")
        {
            continue;
        }
        if rpc_raw.starts_with("---") || rpc_raw.starts_with(":---") {
            continue;
        }
        if retry_raw.starts_with("---") || retry_raw.starts_with(":---") {
            continue;
        }

        let rpc_code: i64 = rpc_raw
            .parse()
            .map_err(|_| anyhow!("invalid rpc_code in CABAL_ERROR_CODES.md row: {}", line))?;
        let retryable = match retry_raw.as_str() {
            "true" => true,
            "false" => false,
            _ => {
                bail!(
                    "invalid retryable value in CABAL_ERROR_CODES.md row: {}",
                    line
                )
            }
        };
        if out.insert(code.clone(), (rpc_code, retryable)).is_some() {
            bail!("duplicate cabal_code in CABAL_ERROR_CODES.md: {code}");
        }
    }
    Ok(out)
}

fn normalize_md_cell(s: &str) -> String {
    s.trim().trim_matches('`').trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;

    #[test]
    fn classify_revision_mismatch() {
        let err = anyhow!("policy revision mismatch: expected=1 actual=2");
        let out = classify_error("tools/call", Some("cabal.apply_policy_bundle"), &err);
        assert_eq!(out.cabal_code, "REVISION_MISMATCH");
        assert_eq!(out.rpc_code, -32010);
    }

    #[test]
    fn classify_nonce_replay() {
        let err = anyhow!("nonce replay detected");
        let out = classify_error("tools/call", Some("cabal.apply_policy_bundle"), &err);
        assert_eq!(out.cabal_code, "NONCE_REPLAY");
        assert_eq!(out.rpc_code, -32012);
    }

    #[test]
    fn classify_gate_fail() {
        let err = anyhow!("exit gate failed for phase C-0");
        let out = classify_error("tools/call", Some("cabal.transition_phase_strict"), &err);
        assert_eq!(out.cabal_code, "GATE_FAIL");
        assert_eq!(out.rpc_code, -32020);
    }

    #[test]
    fn classify_signature_invalid_for_expired_key() {
        let err = anyhow!("signing key expired: k-expired");
        let out = classify_error("tools/call", Some("cabal.apply_policy_bundle"), &err);
        assert_eq!(out.cabal_code, "SIGNATURE_INVALID");
        assert_eq!(out.rpc_code, -32011);
    }

    #[test]
    fn classify_storage_failure() {
        let err = anyhow!("failed to write audit record");
        let out = classify_error("tools/call", Some("cabal.record_event"), &err);
        assert_eq!(out.cabal_code, "STORAGE_FAILURE");
        assert_eq!(out.rpc_code, -32050);
    }

    #[test]
    fn classify_io_failure() {
        let err = anyhow!("failed to read file: .memory/x.txt");
        let out = classify_error("tools/call", Some("cabal.proxy_execute"), &err);
        assert_eq!(out.cabal_code, "IO_FAILURE");
        assert_eq!(out.rpc_code, -32051);
    }

    #[test]
    fn classify_policy_deny_for_ide_profile_block() {
        let err = anyhow!("policy deny: ide profile is not allowed: vscode");
        let out = classify_error("initialize", None, &err);
        assert_eq!(out.cabal_code, "POLICY_DENY");
        assert_eq!(out.rpc_code, -32030);
    }

    #[test]
    fn classify_proxy_deny_for_blocked_shell_command() {
        let err = anyhow!("shell command blocked by policy: contains forbidden fragment `rm -rf`");
        let out = classify_error("tools/call", Some("cabal.proxy_execute"), &err);
        assert_eq!(out.cabal_code, "PROXY_DENY");
        assert_eq!(out.rpc_code, -32031);
    }

    #[test]
    fn classify_proxy_deny_for_blocked_network_target() {
        let err = anyhow!("network target blocked by policy: forbidden host `localhost`");
        let out = classify_error("tools/call", Some("cabal.proxy_execute"), &err);
        assert_eq!(out.cabal_code, "PROXY_DENY");
        assert_eq!(out.rpc_code, -32031);
    }

    #[test]
    fn classify_proxy_deny_for_invalid_network_target_url() {
        let err = anyhow!("invalid network target url");
        let out = classify_error("tools/call", Some("cabal.proxy_execute"), &err);
        assert_eq!(out.cabal_code, "PROXY_DENY");
        assert_eq!(out.rpc_code, -32031);
    }

    #[test]
    fn classify_signature_invalid_for_broken_sidecar() {
        let err = anyhow!("invalid signature file format");
        let out = classify_error("tools/call", Some("cabal.verify_audit_archive"), &err);
        assert_eq!(out.cabal_code, "SIGNATURE_INVALID");
        assert_eq!(out.rpc_code, -32011);
    }

    #[test]
    fn classify_invalid_request_for_bad_priority() {
        let err = anyhow!("unsupported consult priority: urgent");
        let out = classify_error("tools/call", Some("cabal.route_consult"), &err);
        assert_eq!(out.cabal_code, "INVALID_REQUEST");
        assert_eq!(out.rpc_code, -32602);
    }

    #[test]
    fn classify_invalid_request_for_empty_audit_rotate() {
        let err = anyhow!("audit log is empty");
        let out = classify_error("tools/call", Some("cabal.rotate_audit_log"), &err);
        assert_eq!(out.cabal_code, "INVALID_REQUEST");
        assert_eq!(out.rpc_code, -32602);
    }

    #[test]
    fn classify_invalid_request_for_prune_keep_last() {
        let err = anyhow!("keep_last must be > 0");
        let out = classify_error("tools/call", Some("cabal.prune_audit_archives"), &err);
        assert_eq!(out.cabal_code, "INVALID_REQUEST");
        assert_eq!(out.rpc_code, -32602);
    }

    #[test]
    fn classify_invalid_request_for_audit_rotation_policy_limits() {
        let err = anyhow!("max_bytes must be > 0");
        let out = classify_error("tools/call", Some("cabal.set_audit_rotation_policy"), &err);
        assert_eq!(out.cabal_code, "INVALID_REQUEST");
        assert_eq!(out.rpc_code, -32602);
    }

    #[test]
    fn classify_executor_failure_for_shell_timeout() {
        let err = anyhow!("shell command timed out");
        let out = classify_error("tools/call", Some("cabal.proxy_execute"), &err);
        assert_eq!(out.cabal_code, "EXECUTOR_FAILURE");
        assert_eq!(out.rpc_code, -32040);
    }

    #[test]
    fn classify_invalid_request_for_oversized_write_text() {
        let err = anyhow!("payload.text is too large");
        let out = classify_error("tools/call", Some("cabal.proxy_execute"), &err);
        assert_eq!(out.cabal_code, "INVALID_REQUEST");
        assert_eq!(out.rpc_code, -32602);
    }

    #[test]
    fn classify_invalid_request_for_zero_limit() {
        let err = anyhow!("limit must be > 0");
        let out = classify_error("tools/call", Some("cabal.get_proxy_log"), &err);
        assert_eq!(out.cabal_code, "INVALID_REQUEST");
        assert_eq!(out.rpc_code, -32602);
    }

    #[test]
    fn classify_invalid_request_for_shell_command_too_long() {
        let err = anyhow!("shell target command is too long");
        let out = classify_error("tools/call", Some("cabal.proxy_execute"), &err);
        assert_eq!(out.cabal_code, "INVALID_REQUEST");
        assert_eq!(out.rpc_code, -32602);
    }

    #[test]
    fn classify_invalid_request_for_adaptive_exploration_policy_limits() {
        let err = anyhow!("exploration_min_samples must be > 0");
        let out = classify_error(
            "tools/call",
            Some("cabal.set_adaptive_exploration_policy"),
            &err,
        );
        assert_eq!(out.cabal_code, "INVALID_REQUEST");
        assert_eq!(out.rpc_code, -32602);
    }

    #[test]
    fn classify_invalid_request_for_ack_cross_rules_paths() {
        let err = anyhow!("agent_ack_path and subagent_ack_path are required");
        let out = classify_error("tools/call", Some("cabal.ack_cross_rules"), &err);
        assert_eq!(out.cabal_code, "INVALID_REQUEST");
        assert_eq!(out.rpc_code, -32602);
    }

    #[test]
    fn classify_parse_error() {
        let err = anyhow!("failed to parse newline-delimited jsonrpc message");
        let out = classify_error("protocol.read", None, &err);
        assert_eq!(out.cabal_code, "PARSE_ERROR");
        assert_eq!(out.rpc_code, -32700);
    }

    #[test]
    fn classify_transport_error() {
        let err = anyhow!("invalid content-length value");
        let out = classify_error("protocol.read", None, &err);
        assert_eq!(out.cabal_code, "TRANSPORT_ERROR");
        assert_eq!(out.rpc_code, -32060);
    }

    #[test]
    fn classify_state_corrupt() {
        let err = anyhow!("failed to parse runtime state");
        let out = classify_error("startup", None, &err);
        assert_eq!(out.cabal_code, "STATE_CORRUPT");
        assert_eq!(out.rpc_code, -32052);
    }

    #[test]
    fn parity_with_doc_file_is_clean() {
        let doc = include_str!("../../spec/docs/CABAL_ERROR_CODES.md");
        let report = validate_error_codes_doc_parity(doc).expect("parity report");
        assert_eq!(report["pass"], Value::Bool(true));
        assert_eq!(
            report["missing_in_doc"]
                .as_array()
                .expect("missing_in_doc array")
                .len(),
            0
        );
        assert_eq!(
            report["missing_in_runtime"]
                .as_array()
                .expect("missing_in_runtime array")
                .len(),
            0
        );
    }

    #[test]
    fn parity_detects_mismatch() {
        let bad_doc = r#"
| cabal_code | rpc_code | retryable | Класс |
| --- | ---: | :---: | --- |
| `PARSE_ERROR` | -32000 | false | malformed json payload |
| `TRANSPORT_ERROR` | -32060 | false | invalid frame/header/body length |
"#;
        let report = validate_error_codes_doc_parity(bad_doc).expect("parity report");
        assert_eq!(report["pass"], Value::Bool(false));
        assert!(
            report["mismatches"]
                .as_array()
                .expect("mismatches")
                .iter()
                .any(|x| x["cabal_code"] == Value::String("PARSE_ERROR".to_string()))
        );
    }
}

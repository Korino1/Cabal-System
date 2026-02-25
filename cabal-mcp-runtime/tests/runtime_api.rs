use cabal_mcp_runtime::cpu::{CpuProfile, ExecutionPath};
use cabal_mcp_runtime::runtime::CabalRuntime;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_root(prefix: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    p.push(format!("{}_{}", prefix, nanos));
    p
}

#[test]
fn integration_guard_action_rejects_forbidden() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_guard");
    fs::create_dir_all(&root).expect("mkdir");
    let runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    let result = runtime
        .guard_action("agent", "please bypass control plane")
        .expect("guard_action");
    assert_eq!(result["allow"].as_bool(), Some(false));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_transition_phase_rejects_skip_and_accepts_next() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_phase");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    let err = runtime
        .transition_phase("GA-3")
        .expect_err("skip should fail");
    assert!(err.to_string().contains("invalid phase transition"));

    let ok = runtime.transition_phase("GA-1").expect("next should pass");
    assert_eq!(ok["changed"].as_bool(), Some(true));
    assert_eq!(ok["phase"].as_str(), Some("GA-1"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_apply_policy_revision_mismatch() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_policy");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    let err = runtime
        .apply_policy(
            &cpu,
            999,
            "v2".to_string(),
            vec!["rule".to_string()],
            None,
            None,
            None,
            vec![],
        )
        .expect_err("revision mismatch should fail");
    assert!(err.to_string().contains("policy revision mismatch"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_proxy_execute_fs_read_allow_path() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_proxy_fs");
    fs::create_dir_all(root.join(".memory")).expect("mkdir");
    fs::write(root.join(".memory").join("x.txt"), "ok").expect("write");

    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");
    let out = runtime
        .proxy_execute(
            &cpu,
            "fs",
            "read_text",
            ".memory/x.txt",
            serde_json::json!({}),
        )
        .expect("proxy_execute");
    assert_eq!(out["allow"].as_bool(), Some(true));
    assert_eq!(out["executed"].as_bool(), Some(true));
    assert_eq!(out["result"]["text"].as_str(), Some("ok"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_proxy_execute_network_denied_by_default() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_proxy_net");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    let out = runtime
        .proxy_execute(
            &cpu,
            "network",
            "http_get",
            "https://example.com",
            serde_json::json!({}),
        )
        .expect("proxy_execute");
    assert_eq!(out["allow"].as_bool(), Some(false));
    assert_eq!(out["executed"].as_bool(), Some(false));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_proxy_execute_network_blocks_local_targets_even_allow_by_default() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_proxy_net_guard");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");
    runtime
        .set_proxy_policy(Some(false), None, None)
        .expect("allow-by-default");

    let err = runtime
        .proxy_execute(
            &cpu,
            "network",
            "http_get",
            "http://127.0.0.1:8080",
            serde_json::json!({}),
        )
        .expect_err("local network target must be blocked");
    assert!(err.to_string().contains("network target blocked by policy"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_proxy_execute_fs_blocks_traversal_in_allow_by_default_mode() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_proxy_traversal");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");
    runtime
        .set_proxy_policy(Some(false), None, None)
        .expect("allow-by-default");

    let err = runtime
        .proxy_execute(
            &cpu,
            "fs",
            "read_text",
            "../secret.txt",
            serde_json::json!({}),
        )
        .expect_err("traversal must fail");
    assert!(err.to_string().contains("path traversal"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_proxy_execute_fs_blocks_absolute_path_in_allow_by_default_mode() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_proxy_abs");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");
    runtime
        .set_proxy_policy(Some(false), None, None)
        .expect("allow-by-default");

    #[cfg(target_os = "windows")]
    let target = "C:\\Windows\\win.ini";
    #[cfg(not(target_os = "windows"))]
    let target = "/etc/passwd";

    let err = runtime
        .proxy_execute(&cpu, "fs", "read_text", target, serde_json::json!({}))
        .expect_err("absolute path must fail");
    assert!(err.to_string().contains("absolute paths are forbidden"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_proxy_execute_shell_blocks_dangerous_command_even_allow_by_default() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_proxy_shell_danger");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");
    runtime
        .set_proxy_policy(Some(false), None, None)
        .expect("allow-by-default");

    let err = runtime
        .proxy_execute(
            &cpu,
            "shell",
            "run",
            "git reset --hard HEAD",
            serde_json::json!({}),
        )
        .expect_err("dangerous shell command must be blocked");
    assert!(err.to_string().contains("shell command blocked by policy"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_proxy_operation_policy_denies_operation_before_execution() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_proxy_operation_policy");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");
    runtime
        .set_proxy_policy(Some(false), None, None)
        .expect("allow-by-default");
    runtime
        .set_proxy_operation_policy("fs".to_string(), Some(vec!["read_text".to_string()]), None)
        .expect("set operation policy");

    let out = runtime
        .proxy_execute(
            &cpu,
            "fs",
            "write_text",
            ".memory/should_not_write.txt",
            serde_json::json!({"text":"x"}),
        )
        .expect("proxy_execute");
    assert_eq!(out["allow"].as_bool(), Some(false));
    assert_eq!(out["executed"].as_bool(), Some(false));
    assert_eq!(
        out["reason"].as_str(),
        Some("operation is not in allowlist")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_transition_phase_strict_requires_gates() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_gate");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    let err = runtime
        .transition_phase_strict("GA-1")
        .expect_err("strict transition must fail without exit evidence");
    assert!(err.to_string().contains("exit gate failed"));

    runtime
        .register_evidence(
            "concept_master".to_string(),
            "spec/docs/CONCEPT_MASTER.md".to_string(),
        )
        .expect("e1");
    runtime
        .register_evidence(
            "concept_math_proof".to_string(),
            "spec/docs/CONCEPT_MATH_PROOF.md".to_string(),
        )
        .expect("e2");
    runtime
        .register_evidence(
            "c0_digest".to_string(),
            ".memory/PHASES/C-0/DIGEST.md".to_string(),
        )
        .expect("e3");
    runtime
        .register_evidence(
            "cross_rules_agent_ack".to_string(),
            "spec/docs/CONCEPT_MASTER.md".to_string(),
        )
        .expect("e4");
    runtime
        .register_evidence(
            "cross_rules_subagent_ack".to_string(),
            "spec/docs/CONCEPT_MASTER.md".to_string(),
        )
        .expect("e5");

    let out = runtime
        .transition_phase_strict("GA-1")
        .expect("strict transition should pass after evidence");
    assert_eq!(out["changed"].as_bool(), Some(true));
    assert_eq!(out["phase"].as_str(), Some("GA-1"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_gate_entry_requires_cross_rules_ack_evidence() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_gate_entry_rules");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    let report = runtime.gate_check("entry", "GA-1").expect("gate check");
    assert_eq!(report["pass"].as_bool(), Some(false));
    let checks = report["checks"].as_array().expect("checks");
    assert!(
        checks.iter().any(|item| {
            item["id"].as_str() == Some("cross_rules_agent_ack")
                && item["pass"].as_bool() == Some(false)
        }),
        "cross_rules_agent_ack check missing in report: {report}"
    );
    assert!(
        checks.iter().any(|item| {
            item["id"].as_str() == Some("cross_rules_subagent_ack")
                && item["pass"].as_bool() == Some(false)
        }),
        "cross_rules_subagent_ack check missing in report: {report}"
    );

    runtime
        .register_evidence(
            "cross_rules_agent_ack".to_string(),
            "spec/docs/CONCEPT_MASTER.md".to_string(),
        )
        .expect("ack agent");
    runtime
        .register_evidence(
            "cross_rules_subagent_ack".to_string(),
            "spec/docs/CONCEPT_MASTER.md".to_string(),
        )
        .expect("ack subagent");

    let report = runtime.gate_check("entry", "GA-1").expect("gate check");
    let checks = report["checks"].as_array().expect("checks");
    assert!(
        checks.iter().any(|item| {
            item["id"].as_str() == Some("cross_rules_agent_ack")
                && item["pass"].as_bool() == Some(true)
        }),
        "cross_rules_agent_ack should pass after evidence: {report}"
    );
    assert!(
        checks.iter().any(|item| {
            item["id"].as_str() == Some("cross_rules_subagent_ack")
                && item["pass"].as_bool() == Some(true)
        }),
        "cross_rules_subagent_ack should pass after evidence: {report}"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_gate_policy_strict_artifacts_toggles_required_files_check() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_gate_strict_policy");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    runtime
        .set_gate_policy(Some(false))
        .expect("disable strict mode");
    let entry_loose = runtime.gate_check("entry", "GA-1").expect("gate check");
    let checks = entry_loose["checks"].as_array().expect("checks");
    assert!(
        checks.iter().any(|item| {
            item["id"].as_str() == Some("entry_required_files_present")
                && item["pass"].as_bool() == Some(true)
        }),
        "entry_required_files_present should pass when strict mode is disabled: {entry_loose}"
    );

    runtime
        .set_gate_policy(Some(true))
        .expect("enable strict mode");
    let entry_strict = runtime.gate_check("entry", "GA-1").expect("gate check");
    let checks = entry_strict["checks"].as_array().expect("checks");
    assert!(
        checks.iter().any(|item| {
            item["id"].as_str() == Some("entry_required_files_present")
                && item["pass"].as_bool() == Some(false)
        }),
        "entry_required_files_present should fail when strict mode is enabled: {entry_strict}"
    );

    let exit_strict = runtime.gate_check("exit", "C-0").expect("gate check");
    let checks = exit_strict["checks"].as_array().expect("checks");
    assert!(
        checks.iter().any(|item| {
            item["id"].as_str() == Some("exit_required_files_present")
                && item["pass"].as_bool() == Some(false)
        }),
        "exit_required_files_present should fail when strict mode is enabled: {exit_strict}"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_proxy_request_denies_shell_by_default() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_proxy");
    fs::create_dir_all(&root).expect("mkdir");
    let runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    let out = runtime
        .proxy_request("shell", "run", "cargo test")
        .expect("proxy_request");
    assert_eq!(out["allow"].as_bool(), Some(false));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_audit_log_appends_records() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_audit");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    runtime
        .set_consult_mode("YOLO")
        .expect("set_consult_mode should pass");
    runtime
        .register_evidence("audit_probe".to_string(), ".memory/probe.md".to_string())
        .expect("register evidence");
    runtime.persist().expect("persist");

    let audit_path = root.join(".cabal_runtime").join("audit.jsonl");
    let text = fs::read_to_string(&audit_path).expect("read audit");
    let lines: Vec<&str> = text.lines().filter(|x| !x.trim().is_empty()).collect();
    assert!(lines.len() >= 2);

    for line in lines {
        let parsed: serde_json::Value = serde_json::from_str(line).expect("json line");
        assert!(parsed.get("kind").is_some());
        assert!(parsed.get("ts_unix").is_some());
    }

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_query_audit_log_and_replay() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_audit_query");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    runtime
        .set_consult_mode("YOLO")
        .expect("set_consult_mode should pass");
    runtime
        .register_evidence("ev-query".to_string(), ".memory/query.md".to_string())
        .expect("register evidence");
    runtime
        .record_event(
            &cpu,
            "probe".to_string(),
            serde_json::json!({"request_id": "r-1"}),
        )
        .expect("record event");

    let filtered = runtime
        .query_audit_log(
            Some("evidence.registered".to_string()),
            None,
            None,
            None,
            None,
            None,
            Some(20),
        )
        .expect("query audit");
    assert!(filtered["matched"].as_u64().unwrap_or(0) >= 1);
    assert_eq!(filtered["max_limit"].as_u64(), Some(2000));
    let items = filtered["items"].as_array().expect("items");
    for item in items {
        assert_eq!(item["kind"].as_str(), Some("evidence.registered"));
    }

    let replay = runtime
        .replay_audit_state(None, None)
        .expect("replay should pass");
    assert_eq!(replay["snapshot"]["consult_mode"].as_str(), Some("yolo"));
    assert!(replay["snapshot"]["evidence_total"].as_u64().unwrap_or(0) >= 1);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_export_audit_log_writes_file() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_audit_export");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    runtime
        .set_consult_mode("YOLO")
        .expect("set_consult_mode should pass");
    runtime.persist().expect("persist");

    let out_path = ".memory/audit_export.jsonl".to_string();
    let export = runtime
        .export_audit_log(
            out_path.clone(),
            Some("consult_mode.changed".to_string()),
            None,
            None,
            None,
            None,
            None,
            Some(10),
        )
        .expect("export should pass");
    assert!(export["exported"].as_u64().unwrap_or(0) >= 1);
    assert_eq!(export["max_limit"].as_u64(), Some(2000));
    assert_eq!(export["applied_limit"].as_u64(), Some(10));

    let file_path = root.join(".memory").join("audit_export.jsonl");
    let text = fs::read_to_string(&file_path).expect("read export");
    assert!(!text.trim().is_empty());
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let rec: serde_json::Value = serde_json::from_str(line).expect("json line");
        assert_eq!(rec["kind"].as_str(), Some("consult_mode.changed"));
    }

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_audit_query_export_replay_large_log_are_capped() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_audit_large_log_caps");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    for idx in 0..2600 {
        runtime
            .record_event(
                &cpu,
                "probe.large".to_string(),
                serde_json::json!({"idx": idx, "request_id": format!("rq-{idx}")}),
            )
            .expect("record event");
    }

    let query = runtime
        .query_audit_log(
            Some("event.recorded".to_string()),
            None,
            None,
            None,
            None,
            None,
            Some(50000),
        )
        .expect("query");
    assert_eq!(query["max_limit"].as_u64(), Some(2000));
    assert_eq!(query["matched"].as_u64(), Some(2600));
    assert_eq!(query["items"].as_array().map(|x| x.len()), Some(2000));

    let export = runtime
        .export_audit_log(
            ".memory/large_export.jsonl".to_string(),
            Some("event.recorded".to_string()),
            None,
            None,
            None,
            None,
            None,
            Some(50000),
        )
        .expect("export");
    assert_eq!(export["max_limit"].as_u64(), Some(2000));
    assert_eq!(export["applied_limit"].as_u64(), Some(2000));
    assert_eq!(export["exported"].as_u64(), Some(2000));

    let replay = runtime
        .replay_audit_state(None, None)
        .expect("replay should pass");
    assert!(replay["total_events"].as_u64().unwrap_or(0) >= 2600);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_rotate_and_verify_audit_archive() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_audit_rotate");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    runtime
        .set_consult_mode("YOLO")
        .expect("set_consult_mode should pass");
    runtime.persist().expect("persist");

    let rotated = runtime
        .rotate_audit_log(None, Some(true))
        .expect("rotate audit");
    let archive = &rotated["archive"];
    assert_eq!(archive["rotated"].as_bool(), Some(true));
    assert_eq!(archive["compressed"].as_bool(), Some(true));
    assert!(archive["archived_lines"].as_u64().unwrap_or(0) >= 1);

    let archive_path = archive["archive_path"]
        .as_str()
        .expect("archive_path")
        .to_string();
    let signature_path = archive["signature_path"]
        .as_str()
        .expect("signature_path")
        .to_string();

    let verify = runtime
        .verify_audit_archive(archive_path, Some(signature_path))
        .expect("verify archive");
    assert_eq!(verify["pass"].as_bool(), Some(true));
    assert!(verify["line_count"].as_u64().unwrap_or(0) >= 1);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_verify_audit_archive_detects_tamper() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_audit_rotate_tamper");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    runtime
        .set_consult_mode("YOLO")
        .expect("set_consult_mode should pass");
    runtime.persist().expect("persist");

    let rotated = runtime
        .rotate_audit_log(None, Some(false))
        .expect("rotate audit");
    let archive = &rotated["archive"];
    let archive_path = archive["archive_path"]
        .as_str()
        .expect("archive_path")
        .to_string();
    let signature_path = archive["signature_path"]
        .as_str()
        .expect("signature_path")
        .to_string();

    fs::write(root.join(&signature_path), "deadbeef  broken\n").expect("tamper signature");
    let verify = runtime
        .verify_audit_archive(archive_path, Some(signature_path))
        .expect("verify archive");
    assert_eq!(verify["pass"].as_bool(), Some(false));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_prune_audit_archives_keeps_last_n() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_audit_prune");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    for idx in 0..3 {
        if idx % 2 == 0 {
            runtime
                .set_consult_mode("YOLO")
                .expect("set_consult_mode should pass");
        } else {
            runtime
                .set_consult_mode("USER_TRACKING")
                .expect("set_consult_mode should pass");
        }
        runtime.persist().expect("persist");
        runtime
            .rotate_audit_log(Some(".cabal_runtime/archive".to_string()), Some(false))
            .expect("rotate");
    }

    let out = runtime
        .prune_audit_archives(Some(".cabal_runtime/archive".to_string()), Some(1))
        .expect("prune");
    assert!(out["removed"].as_u64().unwrap_or(0) >= 2);
    assert_eq!(out["kept"].as_u64(), Some(1));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_audit_health_check_pass_and_fail_paths() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_audit_health");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    runtime
        .set_consult_mode("YOLO")
        .expect("set_consult_mode should pass");
    let rotated = runtime
        .rotate_audit_log(Some(".cabal_runtime/archive".to_string()), Some(false))
        .expect("rotate");

    let health_ok = runtime
        .audit_health_check(Some(".cabal_runtime/archive".to_string()), Some(5))
        .expect("health check pass");
    assert_eq!(health_ok["status"].as_str(), Some("pass"));
    assert!(health_ok["archives"]["total"].as_u64().unwrap_or(0) >= 1);
    assert!(health_ok["archives"]["checked"].as_u64().unwrap_or(0) >= 1);
    assert_eq!(health_ok["archives"]["failed"].as_u64(), Some(0));
    assert!(health_ok["archives"]["passed"].as_u64().unwrap_or(0) >= 1);

    let signature_path = rotated["archive"]["signature_path"]
        .as_str()
        .expect("signature path");
    fs::write(root.join(signature_path), "deadbeef  broken\n").expect("tamper signature");

    let health_fail = runtime
        .audit_health_check(Some(".cabal_runtime/archive".to_string()), Some(5))
        .expect("health check fail");
    assert_eq!(health_fail["status"].as_str(), Some("fail"));
    assert!(health_fail["archives"]["failed"].as_u64().unwrap_or(0) >= 1);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_set_audit_rotation_policy_and_get() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_audit_rotation_policy");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    let out = runtime
        .set_audit_rotation_policy(
            Some(true),
            Some(2048),
            Some(300),
            Some(false),
            Some(5),
            Some(".cabal_runtime/archive_custom".to_string()),
        )
        .expect("set policy");
    assert_eq!(out["enabled"].as_bool(), Some(true));
    assert_eq!(out["max_bytes"].as_u64(), Some(2048));
    assert_eq!(out["max_age_sec"].as_u64(), Some(300));
    assert_eq!(out["compress"].as_bool(), Some(false));
    assert_eq!(out["keep_last"].as_u64(), Some(5));
    assert_eq!(
        out["archive_dir"].as_str(),
        Some(".cabal_runtime/archive_custom")
    );

    let got = runtime.get_audit_rotation_policy();
    assert_eq!(got["enabled"].as_bool(), Some(true));
    assert_eq!(got["max_bytes"].as_u64(), Some(2048));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_set_cpu_policy_and_get_state_snapshot() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_cpu_policy");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    let out = runtime
        .set_cpu_policy(&cpu, Some(false), None, None, None, None, None)
        .expect("set cpu policy");
    assert_eq!(out["require_zen4_fast_path"].as_bool(), Some(false));

    let got = runtime.get_cpu_policy();
    assert_eq!(got["require_zen4_fast_path"].as_bool(), Some(false));

    let state = runtime.get_state_value();
    assert_eq!(
        state["cpu_policy"]["require_zen4_fast_path"].as_bool(),
        Some(false)
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_startup_fails_on_incompatible_cpu_policy_state() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_cpu_policy_startup");
    fs::create_dir_all(&root).expect("mkdir");
    let _runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    let required_key = if !cpu.has_avx512f {
        Some("require_avx512f")
    } else if !cpu.has_avx512vl {
        Some("require_avx512vl")
    } else if !cpu.has_fma {
        Some("require_fma")
    } else if !cpu.has_bmi2 {
        Some("require_bmi2")
    } else if !cpu.has_sha {
        Some("require_sha")
    } else if !matches!(cpu.path, ExecutionPath::Zen4Avx512) {
        Some("require_zen4_fast_path")
    } else {
        None
    };
    let Some(required_key) = required_key else {
        let _ = fs::remove_dir_all(root);
        return;
    };

    let state_path = root.join(".cabal_runtime").join("state.json");
    let text = fs::read_to_string(&state_path).expect("read state");
    let mut state: serde_json::Value = serde_json::from_str(&text).expect("parse state");
    state[required_key] = serde_json::Value::Bool(true);
    fs::write(
        &state_path,
        serde_json::to_string_pretty(&state).expect("serialize"),
    )
    .expect("write state");

    let output = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .expect("run runtime");
    assert!(
        !output.status.success(),
        "runtime startup must fail on incompatible cpu policy"
    );
    let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
    assert!(
        stderr.contains("policy deny"),
        "unexpected stderr for startup cpu policy failure: {stderr}"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_auto_rotate_audit_by_size() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_audit_auto_rotate_size");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    runtime
        .set_audit_rotation_policy(
            Some(true),
            Some(1),
            Some(86_400),
            Some(false),
            Some(10),
            Some(".cabal_runtime/archive".to_string()),
        )
        .expect("set policy");
    runtime
        .set_consult_mode("YOLO")
        .expect("set_consult_mode should pass");

    let archive_dir = root.join(".cabal_runtime").join("archive");
    let sidecars = fs::read_dir(&archive_dir)
        .expect("read archive_dir")
        .filter_map(|x| x.ok())
        .filter(|x| {
            x.file_name()
                .to_string_lossy()
                .to_ascii_lowercase()
                .ends_with(".sha256")
        })
        .count();
    assert!(sidecars >= 1, "no audit archives found");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_auto_rotate_audit_by_age() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_audit_auto_rotate_age");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    runtime
        .set_audit_rotation_policy(
            Some(true),
            Some(1_000_000_000),
            Some(1),
            Some(false),
            Some(10),
            Some(".cabal_runtime/archive".to_string()),
        )
        .expect("set policy");
    runtime.state.audit_last_rotation_unix = 1;
    runtime
        .set_consult_mode("YOLO")
        .expect("set_consult_mode should pass");

    let archive_dir = root.join(".cabal_runtime").join("archive");
    let sidecars = fs::read_dir(&archive_dir)
        .expect("read archive_dir")
        .filter_map(|x| x.ok())
        .filter(|x| {
            x.file_name()
                .to_string_lossy()
                .to_ascii_lowercase()
                .ends_with(".sha256")
        })
        .count();
    assert!(sidecars >= 1, "no audit archives found");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_route_consult_guard_requires_cross_rules_ack_evidence() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_consult_guard");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    runtime.set_consult_mode("YOLO").expect("set mode");
    runtime
        .set_consult_guard_policy(
            Some(true),
            Some(vec![
                "cross_rules_agent_ack".to_string(),
                "cross_rules_subagent_ack".to_string(),
            ]),
        )
        .expect("set consult guard");

    let err = runtime
        .route_consult(
            "optimize unsafe kernel",
            Some("performance"),
            Some("high"),
            None,
            Some("rq-consult-guard-1"),
        )
        .expect_err("route should fail without evidence");
    assert!(err.to_string().contains("policy deny"));

    runtime
        .register_evidence(
            "cross_rules_agent_ack".to_string(),
            "spec/docs/CONCEPT_MASTER.md".to_string(),
        )
        .expect("register agent ack");
    runtime
        .register_evidence(
            "cross_rules_subagent_ack".to_string(),
            "spec/docs/CONCEPT_MASTER.md".to_string(),
        )
        .expect("register subagent ack");

    let out = runtime
        .route_consult(
            "optimize unsafe kernel",
            Some("performance"),
            Some("high"),
            None,
            Some("rq-consult-guard-2"),
        )
        .expect("route");
    assert_eq!(out["route"].as_str(), Some("orchestrator"));

    let blocked = runtime
        .query_audit_log(
            Some("consult.blocked_missing_evidence".to_string()),
            None,
            None,
            Some("rq-consult-guard-1".to_string()),
            None,
            None,
            Some(10),
        )
        .expect("query blocked audit");
    assert!(blocked["matched"].as_u64().unwrap_or(0) >= 1);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_ack_cross_rules_sets_status_and_unblocks_consult() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_cross_rules_ack");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    runtime.set_consult_mode("YOLO").expect("set mode");
    runtime
        .set_consult_guard_policy(
            Some(true),
            Some(vec![
                "cross_rules_agent_ack".to_string(),
                "cross_rules_subagent_ack".to_string(),
            ]),
        )
        .expect("set guard");

    let initial = runtime.get_cross_rules_status();
    assert_eq!(initial["entry_gate_all_present"].as_bool(), Some(false));
    assert_eq!(initial["consult_guard"]["enabled"].as_bool(), Some(true));

    let out = runtime
        .ack_cross_rules(
            "spec/docs/CONCEPT_MASTER.md".to_string(),
            "spec/docs/CONCEPT_MASTER.md".to_string(),
            Some(true),
        )
        .expect("ack cross rules");
    assert_eq!(out["entry_gate_all_present"].as_bool(), Some(true));
    assert_eq!(out["consult_guard"]["all_present"].as_bool(), Some(true));

    let routed = runtime
        .route_consult(
            "optimize unsafe kernel",
            Some("performance"),
            Some("high"),
            None,
            Some("rq-cross-rules-ack-1"),
        )
        .expect("route");
    assert_eq!(routed["route"].as_str(), Some("orchestrator"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_route_consult_yolo_dispatches_to_orchestrator() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_consult");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    runtime.set_consult_mode("YOLO").expect("set mode");
    let out = runtime
        .route_consult(
            "optimize unsafe kernel",
            Some("performance"),
            Some("critical"),
            None,
            Some("rq-consult-1"),
        )
        .expect("route");
    let state = runtime.get_state_value();
    assert_eq!(out["route"].as_str(), Some("orchestrator"));
    assert_eq!(out["actor"].as_str(), Some("orchestrator"));
    assert_eq!(
        out["policy_revision"].as_u64(),
        state["policy_revision"].as_u64()
    );
    assert_eq!(out["ide_profile"].as_str(), Some("generic"));
    assert_eq!(out["dispatch"]["executor"].as_str(), Some("perf_engineer"));
    assert_eq!(out["timeout_sec"].as_u64(), Some(300));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_route_consult_uses_policy_driven_matrix() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_consult_policy");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    runtime.set_consult_mode("YOLO").expect("set mode");
    runtime
        .set_consult_routing_rule("math".to_string(), "symbolic_solver".to_string())
        .expect("set routing rule");
    runtime
        .set_consult_priority_timeout("high".to_string(), 222)
        .expect("set timeout");
    runtime
        .set_consult_retry_limit("high".to_string(), 4)
        .expect("set retries");
    runtime
        .set_consult_escalation_target("high".to_string(), "architect".to_string())
        .expect("set escalation");
    runtime
        .set_consult_allowed_roles("math".to_string(), vec!["mathematician".to_string()])
        .expect("set allowed roles");

    let out = runtime
        .route_consult(
            "prove invariant",
            Some("math"),
            Some("high"),
            Some("developer"),
            Some("rq-consult-2"),
        )
        .expect("route");
    let state = runtime.get_state_value();
    assert_eq!(out["route"].as_str(), Some("orchestrator"));
    assert_eq!(out["actor"].as_str(), Some("orchestrator"));
    assert_eq!(
        out["policy_revision"].as_u64(),
        state["policy_revision"].as_u64()
    );
    assert_eq!(out["ide_profile"].as_str(), Some("generic"));
    assert_eq!(out["dispatch"]["executor"].as_str(), Some("mathematician"));
    assert_eq!(out["timeout_sec"].as_u64(), Some(222));
    assert_eq!(out["retry_policy"]["max_retries"].as_u64(), Some(4));
    assert_eq!(out["escalation"]["required"].as_bool(), Some(true));
    assert_eq!(out["escalation"]["target"].as_str(), Some("architect"));
    assert_eq!(
        out["escalation"]["reason"].as_str(),
        Some("preferred_role_not_allowed")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_route_consult_includes_active_ide_profile_context() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_consult_ide_context");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    runtime
        .register_ide_client_session(Some("Visual Studio Code"), Some("1.0"))
        .expect("register ide");
    runtime.set_consult_mode("YOLO").expect("set mode");

    let out = runtime
        .route_consult(
            "review patch",
            Some("code"),
            Some("normal"),
            None,
            Some("rq-consult-ide-1"),
        )
        .expect("route");
    assert_eq!(out["ide_profile"].as_str(), Some("vscode"));
    assert_eq!(out["ide_client_name"].as_str(), Some("Visual Studio Code"));

    let audit = runtime
        .query_audit_log(
            Some("consult.routed".to_string()),
            None,
            None,
            Some("rq-consult-ide-1".to_string()),
            None,
            None,
            Some(10),
        )
        .expect("query");
    let items = audit["items"].as_array().expect("items");
    let payload = &items[0]["payload"];
    assert_eq!(payload["ide_profile"].as_str(), Some("vscode"));
    assert_eq!(
        payload["ide_client_name"].as_str(),
        Some("Visual Studio Code")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_route_consult_adaptive_switches_executor() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_consult_adaptive");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    runtime.set_consult_mode("YOLO").expect("set mode");
    runtime
        .set_adaptive_router(Some(true), Some(0.2))
        .expect("adaptive mode");
    runtime
        .set_consult_routing_rule("performance".to_string(), "developer".to_string())
        .expect("set routing");
    runtime
        .set_consult_allowed_roles(
            "performance".to_string(),
            vec!["developer".to_string(), "perf_engineer".to_string()],
        )
        .expect("set allowlist");

    for _ in 0..8 {
        runtime
            .record_consult_feedback(
                Some("rq-dev".to_string()),
                "performance".to_string(),
                "developer".to_string(),
                false,
                Some(2600),
            )
            .expect("dev feedback");
        runtime
            .record_consult_feedback(
                Some("rq-perf".to_string()),
                "performance".to_string(),
                "perf_engineer".to_string(),
                true,
                Some(130),
            )
            .expect("perf feedback");
    }

    let out = runtime
        .route_consult(
            "optimize simd path",
            Some("performance"),
            Some("high"),
            None,
            Some("rq-consult-adaptive"),
        )
        .expect("route");

    assert_eq!(out["route"].as_str(), Some("orchestrator"));
    assert_eq!(out["dispatch"]["executor"].as_str(), Some("perf_engineer"));
    assert_eq!(
        out["routing_decision"]["strategy"].as_str(),
        Some("adaptive")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_route_consult_adaptive_exploration_uses_undertrained_executor() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_consult_adaptive_exploration");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    runtime.set_consult_mode("YOLO").expect("set mode");
    runtime
        .set_adaptive_router(Some(true), Some(0.95))
        .expect("adaptive mode");
    runtime
        .set_adaptive_exploration_policy(Some(1.0), Some(5))
        .expect("exploration mode");
    runtime
        .set_consult_routing_rule("performance".to_string(), "developer".to_string())
        .expect("set routing");
    runtime
        .set_consult_allowed_roles(
            "performance".to_string(),
            vec!["developer".to_string(), "perf_engineer".to_string()],
        )
        .expect("set allowlist");

    for _ in 0..8 {
        runtime
            .record_consult_feedback(
                Some("rq-dev-mature".to_string()),
                "performance".to_string(),
                "developer".to_string(),
                true,
                Some(120),
            )
            .expect("dev feedback");
    }

    let out = runtime
        .route_consult(
            "optimize simd path",
            Some("performance"),
            Some("high"),
            None,
            Some("rq-consult-adaptive-explore"),
        )
        .expect("route");

    assert_eq!(out["route"].as_str(), Some("orchestrator"));
    assert_eq!(out["dispatch"]["executor"].as_str(), Some("perf_engineer"));
    assert_eq!(
        out["routing_decision"]["strategy"].as_str(),
        Some("adaptive_explore")
    );
    assert_eq!(
        out["routing_decision"]["exploration_rate"].as_f64(),
        Some(1.0)
    );
    assert_eq!(
        out["routing_decision"]["exploration_min_samples"].as_u64(),
        Some(5)
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_register_ide_client_tracks_vscode_profile() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_ide_profile");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    let out = runtime
        .register_ide_client_session(Some("Visual Studio Code"), Some("1.96.0"))
        .expect("register ide client");
    assert_eq!(out["active_profile"].as_str(), Some("vscode"));
    assert_eq!(
        out["active_client"]["name"].as_str(),
        Some("Visual Studio Code")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_register_ide_client_denied_when_enforced() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_ide_profile_deny");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    runtime
        .set_ide_profile_policy(
            Some(true),
            None,
            Some(vec!["generic".to_string(), "jetbrains".to_string()]),
        )
        .expect("set ide policy");
    let err = runtime
        .register_ide_client_session(Some("Visual Studio Code"), Some("1.96.0"))
        .expect_err("vscode must be denied");
    assert!(err.to_string().contains("policy deny"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_register_ide_client_missing_name_denied_when_required() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_ide_profile_require_client_info");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    runtime
        .set_ide_profile_policy(
            Some(true),
            Some(true),
            Some(vec!["generic".to_string(), "jetbrains".to_string()]),
        )
        .expect("set ide policy");
    let err = runtime
        .register_ide_client_session(None, Some("1.96.0"))
        .expect_err("missing client info must be denied");
    assert!(err.to_string().contains("client_info.name is required"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_register_ide_client_with_name_allowed_when_required() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_ide_profile_require_client_info_allow");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    runtime
        .set_ide_profile_policy(
            Some(true),
            Some(true),
            Some(vec!["generic".to_string(), "jetbrains".to_string()]),
        )
        .expect("set ide policy");
    let out = runtime
        .register_ide_client_session(Some("JetBrains IntelliJ IDEA"), Some("2025.1"))
        .expect("client info with allowed profile should pass");
    assert_eq!(out["active_profile"].as_str(), Some("jetbrains"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn integration_validate_error_codes_parity() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_error_parity");
    fs::create_dir_all(root.join("spec").join("docs")).expect("mkdir");
    fs::write(
        root.join("spec").join("docs").join("CABAL_ERROR_CODES.md"),
        include_str!("../../spec/docs/CABAL_ERROR_CODES.md"),
    )
    .expect("write doc");
    let runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    let out = runtime
        .validate_error_codes_parity(None)
        .expect("parity check");
    assert_eq!(out["report"]["pass"], serde_json::Value::Bool(true));

    let _ = fs::remove_dir_all(root);
}

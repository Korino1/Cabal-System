use serde_json::{Value, json};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{ChildStdout, Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_root(prefix: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    p.push(format!("{}_{}", prefix, nanos));
    p
}

fn send_ndjson(stdin: &mut std::process::ChildStdin, value: &Value) {
    let line = serde_json::to_string(value).expect("serialize");
    stdin
        .write_all(format!("{line}\n").as_bytes())
        .expect("write request");
    stdin.flush().expect("flush");
}

fn send_raw(stdin: &mut std::process::ChildStdin, bytes: &[u8]) {
    stdin.write_all(bytes).expect("write raw request");
    stdin.flush().expect("flush raw");
}

fn send_framed_jsonrpc(stdin: &mut std::process::ChildStdin, value: &Value) {
    let body = serde_json::to_vec(value).expect("serialize framed body");
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    stdin.write_all(header.as_bytes()).expect("write header");
    stdin.write_all(&body).expect("write body");
    stdin.flush().expect("flush framed");
}

fn read_content_length_response(stdout: &mut BufReader<ChildStdout>) -> Value {
    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        let n = stdout.read_line(&mut line).expect("read header line");
        assert!(n > 0, "unexpected EOF while reading headers");
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        if trimmed.to_ascii_lowercase().starts_with("content-length:") {
            let (_, rhs) = trimmed.split_once(':').expect("header split");
            content_length = Some(rhs.trim().parse::<usize>().expect("parse length"));
        }
    }
    let len = content_length.expect("Content-Length");
    let mut body = vec![0u8; len];
    stdout.read_exact(&mut body).expect("read body");
    serde_json::from_slice(&body).expect("parse json")
}

fn decode_tool_result(response: &Value) -> Value {
    let text = response["result"]["content"][0]["text"]
        .as_str()
        .expect("tool content text");
    serde_json::from_str(text).expect("decode tool result json")
}

#[test]
fn mcp_stdio_route_consult_adaptive_e2e() {
    let root = temp_root("cabal_mcp_stdio");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"test","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));
    assert_eq!(
        init["result"]["serverInfo"]["name"].as_str(),
        Some("cabal-mcp-runtime")
    );

    let calls = vec![
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"cabal.set_consult_mode","arguments":{"mode":"YOLO"}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"cabal.set_adaptive_router","arguments":{"enabled":true,"confidence_floor":0.2}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"cabal.set_consult_routing_rule","arguments":{"consult_type":"performance","executor":"developer"}}}),
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"cabal.set_consult_allowed_roles","arguments":{"consult_type":"performance","roles":["developer","perf_engineer"]}}}),
    ];
    for call in calls {
        send_ndjson(&mut child_stdin, &call);
        let resp = read_content_length_response(&mut child_stdout);
        assert!(resp.get("result").is_some(), "tools/call failed: {resp}");
        let _ = decode_tool_result(&resp);
    }

    for id in 6..14 {
        send_ndjson(
            &mut child_stdin,
            &json!({
                "jsonrpc":"2.0","id":id,
                "method":"tools/call",
                "params":{
                    "name":"cabal.record_consult_feedback",
                    "arguments":{
                        "request_id":"rq-dev",
                        "consult_type":"performance",
                        "executor":"developer",
                        "success":false,
                        "latency_ms":2500
                    }
                }
            }),
        );
        let resp = read_content_length_response(&mut child_stdout);
        assert!(resp.get("result").is_some(), "feedback failed: {resp}");
        let _ = decode_tool_result(&resp);
    }
    for id in 14..22 {
        send_ndjson(
            &mut child_stdin,
            &json!({
                "jsonrpc":"2.0","id":id,
                "method":"tools/call",
                "params":{
                    "name":"cabal.record_consult_feedback",
                    "arguments":{
                        "request_id":"rq-perf",
                        "consult_type":"performance",
                        "executor":"perf_engineer",
                        "success":true,
                        "latency_ms":120
                    }
                }
            }),
        );
        let resp = read_content_length_response(&mut child_stdout);
        assert!(resp.get("result").is_some(), "feedback failed: {resp}");
        let _ = decode_tool_result(&resp);
    }

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":22,
            "method":"tools/call",
            "params":{
                "name":"cabal.route_consult",
                "arguments":{
                    "question":"optimize kernel",
                    "consult_type":"performance",
                    "priority":"high",
                    "request_id":"rq-final"
                }
            }
        }),
    );
    let routed_raw = read_content_length_response(&mut child_stdout);
    assert!(
        routed_raw.get("error").is_none(),
        "route_consult returned error: {routed_raw}"
    );
    let routed = decode_tool_result(&routed_raw);
    assert_eq!(
        routed["route"].as_str(),
        Some("orchestrator"),
        "unexpected route payload: {routed_raw}"
    );
    assert_eq!(
        routed["dispatch"]["executor"].as_str(),
        Some("perf_engineer"),
        "unexpected dispatch payload: {routed_raw}"
    );
    assert_eq!(
        routed["routing_decision"]["strategy"].as_str(),
        Some("adaptive"),
        "unexpected routing_decision payload: {routed_raw}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_startup_flag_enables_strict_artifacts_gate_policy() {
    let root = temp_root("cabal_mcp_stdio_startup_strict");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .arg("--strict-artifacts")
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"startup-flag-test","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"cabal.get_gate_policy","arguments":{}}
        }),
    );
    let gate_policy_resp = read_content_length_response(&mut child_stdout);
    assert!(
        gate_policy_resp.get("result").is_some(),
        "get_gate_policy failed: {gate_policy_resp}"
    );
    let gate_policy = decode_tool_result(&gate_policy_resp);
    assert_eq!(
        gate_policy["strict_artifacts"].as_bool(),
        Some(true),
        "startup strict-artifacts flag was not applied: {gate_policy_resp}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_task_planner_and_patch_gate_e2e() {
    let root = temp_root("cabal_mcp_stdio_task_patch");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"task-patch-test","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{
                "name":"cabal.plan_task_execution",
                "arguments":{
                    "question":"Deploy critical release gate update to production",
                    "priority":"critical"
                }
            }
        }),
    );
    let plan_resp = read_content_length_response(&mut child_stdout);
    assert!(
        plan_resp.get("result").is_some(),
        "plan failed: {plan_resp}"
    );
    let plan = decode_tool_result(&plan_resp);
    assert_eq!(
        plan["classification"]["risk"].as_str(),
        Some("critical"),
        "unexpected risk classification: {plan_resp}"
    );
    assert_eq!(
        plan["priority"].as_str(),
        Some("critical"),
        "unexpected priority payload: {plan_resp}"
    );

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{
                "name":"cabal.evaluate_patch_gate",
                "arguments":{
                    "files":[".env.production","src/main.rs"],
                    "task_risk":"medium",
                    "tests_passed":true
                }
            }
        }),
    );
    let patch_resp = read_content_length_response(&mut child_stdout);
    assert!(
        patch_resp.get("result").is_some(),
        "patch gate failed: {patch_resp}"
    );
    let patch_gate = decode_tool_result(&patch_resp);
    assert_eq!(patch_gate["allow"].as_bool(), Some(false));
    assert_eq!(patch_gate["mode"].as_str(), Some("deny"));

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_proxy_execute_traversal_is_blocked() {
    let root = temp_root("cabal_mcp_stdio_proxy");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"test","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"cabal.set_proxy_policy","arguments":{"deny_by_default":false}}
        }),
    );
    let policy_resp = read_content_length_response(&mut child_stdout);
    assert!(
        policy_resp.get("result").is_some(),
        "set_proxy_policy failed: {policy_resp}"
    );

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{
                "name":"cabal.proxy_execute",
                "arguments":{
                    "category":"fs",
                    "operation":"read_text",
                    "target":"../secret.txt",
                    "payload":{}
                }
            }
        }),
    );
    let resp = read_content_length_response(&mut child_stdout);
    assert!(
        resp.get("error").is_some(),
        "proxy traversal must fail: {resp}"
    );
    assert_eq!(
        resp["error"]["data"]["cabal_code"].as_str(),
        Some("INVALID_REQUEST"),
        "unexpected cabal_code payload: {resp}"
    );

    #[cfg(target_os = "windows")]
    let absolute_target = "C:\\Windows\\win.ini";
    #[cfg(not(target_os = "windows"))]
    let absolute_target = "/etc/passwd";

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":4,"method":"tools/call",
            "params":{
                "name":"cabal.proxy_execute",
                "arguments":{
                    "category":"fs",
                    "operation":"read_text",
                    "target":absolute_target,
                    "payload":{}
                }
            }
        }),
    );
    let abs_resp = read_content_length_response(&mut child_stdout);
    assert!(
        abs_resp.get("error").is_some(),
        "proxy absolute path must fail: {abs_resp}"
    );
    assert_eq!(
        abs_resp["error"]["data"]["cabal_code"].as_str(),
        Some("INVALID_REQUEST"),
        "unexpected abs-path cabal_code payload: {abs_resp}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_proxy_execute_shell_dangerous_command_is_proxy_deny() {
    let root = temp_root("cabal_mcp_stdio_proxy_shell_danger");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"proxy-shell-danger","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"cabal.set_proxy_policy","arguments":{"deny_by_default":false}}
        }),
    );
    let policy_resp = read_content_length_response(&mut child_stdout);
    assert!(
        policy_resp.get("result").is_some(),
        "set_proxy_policy failed: {policy_resp}"
    );

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{
                "name":"cabal.proxy_execute",
                "arguments":{
                    "category":"shell",
                    "operation":"run",
                    "target":"git reset --hard HEAD",
                    "payload":{}
                }
            }
        }),
    );
    let resp = read_content_length_response(&mut child_stdout);
    assert!(
        resp.get("error").is_some(),
        "dangerous shell command must fail: {resp}"
    );
    assert_eq!(
        resp["error"]["data"]["cabal_code"].as_str(),
        Some("PROXY_DENY"),
        "unexpected cabal_code payload: {resp}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_proxy_execute_shell_overlong_command_is_invalid_request() {
    let root = temp_root("cabal_mcp_stdio_proxy_shell_overlong");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"proxy-shell-overlong","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"cabal.set_proxy_policy","arguments":{"deny_by_default":false}}
        }),
    );
    let policy_resp = read_content_length_response(&mut child_stdout);
    assert!(
        policy_resp.get("result").is_some(),
        "set_proxy_policy failed: {policy_resp}"
    );

    let long_command = "a".repeat(1100);
    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{
                "name":"cabal.proxy_execute",
                "arguments":{
                    "category":"shell",
                    "operation":"run",
                    "target":long_command,
                    "payload":{}
                }
            }
        }),
    );
    let resp = read_content_length_response(&mut child_stdout);
    assert!(
        resp.get("error").is_some(),
        "overlong shell command must fail: {resp}"
    );
    assert_eq!(
        resp["error"]["data"]["cabal_code"].as_str(),
        Some("INVALID_REQUEST"),
        "unexpected cabal_code payload: {resp}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_proxy_execute_shell_timeout_is_executor_failure() {
    let root = temp_root("cabal_mcp_stdio_proxy_shell_timeout");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .env("CABAL_PROXY_SHELL_TIMEOUT_MS", "20")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"proxy-shell-timeout","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"cabal.set_proxy_policy","arguments":{"deny_by_default":false}}
        }),
    );
    let policy_resp = read_content_length_response(&mut child_stdout);
    assert!(
        policy_resp.get("result").is_some(),
        "set_proxy_policy failed: {policy_resp}"
    );

    #[cfg(target_os = "windows")]
    let long_cmd = "ping 127.0.0.1 -n 6 > nul";
    #[cfg(not(target_os = "windows"))]
    let long_cmd = "sleep 2";

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{
                "name":"cabal.proxy_execute",
                "arguments":{
                    "category":"shell",
                    "operation":"run",
                    "target":long_cmd,
                    "payload":{}
                }
            }
        }),
    );
    let resp = read_content_length_response(&mut child_stdout);
    assert!(
        resp.get("error").is_some(),
        "timed out shell command must fail: {resp}"
    );
    assert_eq!(
        resp["error"]["data"]["cabal_code"].as_str(),
        Some("EXECUTOR_FAILURE"),
        "unexpected cabal_code payload: {resp}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_proxy_execute_fs_write_oversized_payload_is_invalid_request() {
    let root = temp_root("cabal_mcp_stdio_proxy_fs_write_oversized");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"proxy-fs-write-oversized","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"cabal.set_proxy_policy","arguments":{"deny_by_default":false}}
        }),
    );
    let policy_resp = read_content_length_response(&mut child_stdout);
    assert!(
        policy_resp.get("result").is_some(),
        "set_proxy_policy failed: {policy_resp}"
    );

    let oversized = "x".repeat(1_048_577);
    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{
                "name":"cabal.proxy_execute",
                "arguments":{
                    "category":"fs",
                    "operation":"write_text",
                    "target":".memory/oversized.txt",
                    "payload":{"text":oversized}
                }
            }
        }),
    );
    let resp = read_content_length_response(&mut child_stdout);
    assert!(
        resp.get("error").is_some(),
        "oversized fs write must fail: {resp}"
    );
    assert_eq!(
        resp["error"]["data"]["cabal_code"].as_str(),
        Some("INVALID_REQUEST"),
        "unexpected cabal_code payload: {resp}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_get_proxy_log_zero_limit_is_invalid_request() {
    let root = temp_root("cabal_mcp_stdio_proxy_log_zero_limit");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"name":"cabal.get_proxy_log","arguments":{"limit":0}}
        }),
    );
    let resp = read_content_length_response(&mut child_stdout);
    assert!(resp.get("error").is_some(), "zero limit must fail: {resp}");
    assert_eq!(
        resp["error"]["data"]["cabal_code"].as_str(),
        Some("INVALID_REQUEST"),
        "unexpected cabal_code payload: {resp}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_proxy_execute_network_local_target_is_proxy_deny() {
    let root = temp_root("cabal_mcp_stdio_proxy_net_guard");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"proxy-net-guard","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"cabal.set_proxy_policy","arguments":{"deny_by_default":false}}
        }),
    );
    let policy_resp = read_content_length_response(&mut child_stdout);
    assert!(
        policy_resp.get("result").is_some(),
        "set_proxy_policy failed: {policy_resp}"
    );

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{
                "name":"cabal.proxy_execute",
                "arguments":{
                    "category":"network",
                    "operation":"http_get",
                    "target":"http://127.0.0.1:8080",
                    "payload":{}
                }
            }
        }),
    );
    let resp = read_content_length_response(&mut child_stdout);
    assert!(
        resp.get("error").is_some(),
        "local network target must fail: {resp}"
    );
    assert_eq!(
        resp["error"]["data"]["cabal_code"].as_str(),
        Some("PROXY_DENY"),
        "unexpected cabal_code payload: {resp}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_proxy_execute_network_invalid_url_is_proxy_deny() {
    let root = temp_root("cabal_mcp_stdio_proxy_net_invalid_url");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"proxy-net-invalid-url","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"cabal.set_proxy_policy","arguments":{"deny_by_default":false}}
        }),
    );
    let policy_resp = read_content_length_response(&mut child_stdout);
    assert!(
        policy_resp.get("result").is_some(),
        "set_proxy_policy failed: {policy_resp}"
    );

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{
                "name":"cabal.proxy_execute",
                "arguments":{
                    "category":"network",
                    "operation":"http_get",
                    "target":"not-a-url",
                    "payload":{}
                }
            }
        }),
    );
    let resp = read_content_length_response(&mut child_stdout);
    assert!(
        resp.get("error").is_some(),
        "invalid network target must fail: {resp}"
    );
    assert_eq!(
        resp["error"]["data"]["cabal_code"].as_str(),
        Some("PROXY_DENY"),
        "unexpected cabal_code payload: {resp}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_proxy_operation_policy_blocks_non_allowlisted_operation() {
    let root = temp_root("cabal_mcp_stdio_proxy_ops_policy");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"proxy-ops-policy","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"cabal.set_proxy_policy","arguments":{"deny_by_default":false}}
        }),
    );
    let policy_resp = read_content_length_response(&mut child_stdout);
    assert!(
        policy_resp.get("result").is_some(),
        "set_proxy_policy failed: {policy_resp}"
    );

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{
                "name":"cabal.set_proxy_operation_policy",
                "arguments":{
                    "category":"fs",
                    "allowed_operations":["read_text"]
                }
            }
        }),
    );
    let ops_resp = read_content_length_response(&mut child_stdout);
    assert!(
        ops_resp.get("result").is_some(),
        "set_proxy_operation_policy failed: {ops_resp}"
    );
    let ops_out = decode_tool_result(&ops_resp);
    assert_eq!(
        ops_out["allowed_operations"]["fs"][0].as_str(),
        Some("read_text")
    );

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":4,"method":"tools/call",
            "params":{
                "name":"cabal.proxy_execute",
                "arguments":{
                    "category":"fs",
                    "operation":"write_text",
                    "target":".memory/x.txt",
                    "payload":{"text":"blocked"}
                }
            }
        }),
    );
    let exec_resp = read_content_length_response(&mut child_stdout);
    assert!(
        exec_resp.get("error").is_none(),
        "operation-policy deny should return result payload: {exec_resp}"
    );
    let exec_out = decode_tool_result(&exec_resp);
    assert_eq!(exec_out["allow"].as_bool(), Some(false));
    assert_eq!(exec_out["executed"].as_bool(), Some(false));
    assert_eq!(
        exec_out["reason"].as_str(),
        Some("operation is not in allowlist")
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_transition_phase_strict_gate_fail_has_cabal_code() {
    let root = temp_root("cabal_mcp_stdio_gate");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"test","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{
                "name":"cabal.transition_phase_strict",
                "arguments":{"target_phase":"GA-1"}
            }
        }),
    );
    let resp = read_content_length_response(&mut child_stdout);
    assert!(
        resp.get("error").is_some(),
        "strict transition without evidence must fail: {resp}"
    );
    assert_eq!(
        resp["error"]["data"]["cabal_code"].as_str(),
        Some("GATE_FAIL"),
        "unexpected gate cabal_code payload: {resp}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_transition_phase_strict_requires_cross_rules_ack_evidence() {
    let root = temp_root("cabal_mcp_stdio_gate_cross_rules");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"gate-cross-rules","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    let register_calls = vec![
        json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"cabal.register_evidence","arguments":{"id":"concept_master","path":"spec/docs/CONCEPT_MASTER.md"}}
        }),
        json!({
            "jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{"name":"cabal.register_evidence","arguments":{"id":"concept_math_proof","path":"spec/docs/CONCEPT_MATH_PROOF.md"}}
        }),
        json!({
            "jsonrpc":"2.0","id":4,"method":"tools/call",
            "params":{"name":"cabal.register_evidence","arguments":{"id":"c0_digest","path":".memory/PHASES/C-0/DIGEST.md"}}
        }),
    ];
    for call in register_calls {
        send_ndjson(&mut child_stdin, &call);
        let resp = read_content_length_response(&mut child_stdout);
        assert!(
            resp.get("result").is_some(),
            "register evidence failed: {resp}"
        );
    }

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":5,"method":"tools/call",
            "params":{"name":"cabal.transition_phase_strict","arguments":{"target_phase":"GA-1"}}
        }),
    );
    let deny = read_content_length_response(&mut child_stdout);
    assert!(
        deny.get("error").is_some(),
        "strict transition must fail without cross-rules ack evidence: {deny}"
    );
    assert_eq!(
        deny["error"]["data"]["cabal_code"].as_str(),
        Some("GATE_FAIL"),
        "unexpected gate cabal_code payload: {deny}"
    );

    let ack_calls = vec![
        json!({
            "jsonrpc":"2.0","id":6,"method":"tools/call",
            "params":{"name":"cabal.register_evidence","arguments":{"id":"cross_rules_agent_ack","path":"spec/docs/CONCEPT_MASTER.md"}}
        }),
        json!({
            "jsonrpc":"2.0","id":7,"method":"tools/call",
            "params":{"name":"cabal.register_evidence","arguments":{"id":"cross_rules_subagent_ack","path":"spec/docs/CONCEPT_MASTER.md"}}
        }),
    ];
    for call in ack_calls {
        send_ndjson(&mut child_stdin, &call);
        let resp = read_content_length_response(&mut child_stdout);
        assert!(
            resp.get("result").is_some(),
            "register cross-rules ack failed: {resp}"
        );
    }

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":8,"method":"tools/call",
            "params":{"name":"cabal.transition_phase_strict","arguments":{"target_phase":"GA-1"}}
        }),
    );
    let ok = read_content_length_response(&mut child_stdout);
    assert!(
        ok.get("result").is_some(),
        "strict transition should pass after cross-rules ack evidence: {ok}"
    );
    let out = decode_tool_result(&ok);
    assert_eq!(out["changed"].as_bool(), Some(true));
    assert_eq!(out["phase"].as_str(), Some("GA-1"));

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_gate_policy_strict_artifacts_affects_gate_report() {
    let root = temp_root("cabal_mcp_stdio_gate_policy");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"gate-policy","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"cabal.set_gate_policy","arguments":{"strict_artifacts":true}}
        }),
    );
    let set_resp = read_content_length_response(&mut child_stdout);
    assert!(
        set_resp.get("result").is_some(),
        "set_gate_policy failed: {set_resp}"
    );
    let set_out = decode_tool_result(&set_resp);
    assert_eq!(set_out["strict_artifacts"].as_bool(), Some(true));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{"name":"cabal.gate_check","arguments":{"kind":"entry","phase":"GA-1"}}
        }),
    );
    let gate_resp = read_content_length_response(&mut child_stdout);
    assert!(
        gate_resp.get("result").is_some(),
        "gate_check failed: {gate_resp}"
    );
    let gate_out = decode_tool_result(&gate_resp);
    let checks = gate_out["checks"].as_array().expect("checks");
    assert!(
        checks.iter().any(|item| {
            item["id"].as_str() == Some("entry_required_files_present")
                && item["pass"].as_bool() == Some(false)
        }),
        "strict gate must fail required-files check in empty repo: {gate_out}"
    );

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":4,"method":"tools/call",
            "params":{"name":"cabal.get_gate_policy","arguments":{}}
        }),
    );
    let get_resp = read_content_length_response(&mut child_stdout);
    assert!(
        get_resp.get("result").is_some(),
        "get_gate_policy failed: {get_resp}"
    );
    let get_out = decode_tool_result(&get_resp);
    assert_eq!(get_out["strict_artifacts"].as_bool(), Some(true));

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_cpu_policy_get_set_roundtrip() {
    let root = temp_root("cabal_mcp_stdio_cpu_policy");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"cpu-policy","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"cabal.set_cpu_policy","arguments":{"require_zen4_fast_path":false}}
        }),
    );
    let set_resp = read_content_length_response(&mut child_stdout);
    assert!(
        set_resp.get("result").is_some(),
        "set_cpu_policy failed: {set_resp}"
    );
    let set_out = decode_tool_result(&set_resp);
    assert_eq!(set_out["require_zen4_fast_path"].as_bool(), Some(false));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{"name":"cabal.get_cpu_policy","arguments":{}}
        }),
    );
    let get_resp = read_content_length_response(&mut child_stdout);
    assert!(
        get_resp.get("result").is_some(),
        "get_cpu_policy failed: {get_resp}"
    );
    let get_out = decode_tool_result(&get_resp);
    assert_eq!(get_out["require_zen4_fast_path"].as_bool(), Some(false));

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_cpu_policy_rejects_unavailable_requirement() {
    let root = temp_root("cabal_mcp_stdio_cpu_policy_requirements");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"cpu-policy-req","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"cabal.get_capabilities","arguments":{}}
        }),
    );
    let caps_raw = read_content_length_response(&mut child_stdout);
    assert!(
        caps_raw.get("result").is_some(),
        "get_capabilities failed: {caps_raw}"
    );
    let caps = decode_tool_result(&caps_raw);
    let cpu = &caps["cpu"];

    let required_key = if cpu["has_avx512f"].as_bool() == Some(false) {
        Some("require_avx512f")
    } else if cpu["has_avx512vl"].as_bool() == Some(false) {
        Some("require_avx512vl")
    } else if cpu["has_fma"].as_bool() == Some(false) {
        Some("require_fma")
    } else if cpu["has_bmi2"].as_bool() == Some(false) {
        Some("require_bmi2")
    } else if cpu["has_sha"].as_bool() == Some(false) {
        Some("require_sha")
    } else if cpu["path"].as_str() != Some("zen4_avx512") {
        Some("require_zen4_fast_path")
    } else {
        None
    };

    if let Some(key) = required_key {
        let mut args = serde_json::Map::new();
        args.insert(key.to_string(), Value::Bool(true));
        send_ndjson(
            &mut child_stdin,
            &json!({
                "jsonrpc":"2.0","id":3,"method":"tools/call",
                "params":{"name":"cabal.set_cpu_policy","arguments":Value::Object(args)}
            }),
        );
        let resp = read_content_length_response(&mut child_stdout);
        assert!(
            resp.get("error").is_some(),
            "unavailable cpu requirement should fail: {resp}"
        );
        assert_eq!(
            resp["error"]["data"]["cabal_code"].as_str(),
            Some("POLICY_DENY"),
            "unexpected cabal_code payload: {resp}"
        );
    } else {
        send_ndjson(
            &mut child_stdin,
            &json!({
                "jsonrpc":"2.0","id":3,"method":"tools/call",
                "params":{"name":"cabal.set_cpu_policy","arguments":{"require_sha":false}}
            }),
        );
        let resp = read_content_length_response(&mut child_stdout);
        assert!(
            resp.get("result").is_some(),
            "cpu policy safe update should pass: {resp}"
        );
    }

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_proxy_execute_unsupported_category_is_proxy_deny() {
    let root = temp_root("cabal_mcp_stdio_proxy_deny");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"test","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{
                "name":"cabal.set_proxy_policy",
                "arguments":{
                    "category":"invalid_category",
                    "allow_prefixes":["noop"]
                }
            }
        }),
    );
    let set_policy_resp = read_content_length_response(&mut child_stdout);
    assert!(
        set_policy_resp.get("result").is_some(),
        "set_proxy_policy failed: {set_policy_resp}"
    );

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{
                "name":"cabal.proxy_execute",
                "arguments":{
                    "category":"invalid_category",
                    "operation":"run",
                    "target":"noop-command",
                    "payload":{}
                }
            }
        }),
    );
    let resp = read_content_length_response(&mut child_stdout);
    assert!(
        resp.get("error").is_some(),
        "unsupported proxy category must fail: {resp}"
    );
    assert_eq!(
        resp["error"]["data"]["cabal_code"].as_str(),
        Some("PROXY_DENY"),
        "unexpected proxy cabal_code payload: {resp}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_unknown_tool_returns_unknown_tool_code() {
    let root = temp_root("cabal_mcp_stdio_unknown_tool");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"test","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"cabal.not_existing_tool","arguments":{}}
        }),
    );
    let resp = read_content_length_response(&mut child_stdout);
    assert!(
        resp.get("error").is_some(),
        "unknown tool must fail: {resp}"
    );
    assert_eq!(
        resp["error"]["data"]["cabal_code"].as_str(),
        Some("UNKNOWN_TOOL"),
        "unexpected unknown-tool cabal_code payload: {resp}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_unsupported_method_returns_unsupported_method_code() {
    let root = temp_root("cabal_mcp_stdio_bad_method");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"test","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,
            "method":"cabal.unsupported_method",
            "params":{}
        }),
    );
    let resp = read_content_length_response(&mut child_stdout);
    assert!(
        resp.get("error").is_some(),
        "unsupported method must fail: {resp}"
    );
    assert_eq!(
        resp["error"]["data"]["cabal_code"].as_str(),
        Some("UNSUPPORTED_METHOD"),
        "unexpected unsupported-method cabal_code payload: {resp}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_malformed_ndjson_returns_parse_error_code() {
    let root = temp_root("cabal_mcp_stdio_parse_error");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_raw(&mut child_stdin, b"{invalid-json}\n");
    let resp = read_content_length_response(&mut child_stdout);
    assert!(
        resp.get("error").is_some(),
        "malformed json must fail: {resp}"
    );
    assert_eq!(
        resp["error"]["data"]["cabal_code"].as_str(),
        Some("PARSE_ERROR"),
        "unexpected parse cabal_code payload: {resp}"
    );
    assert_eq!(resp["id"], Value::Null);

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_missing_content_length_returns_transport_error_code() {
    let root = temp_root("cabal_mcp_stdio_transport_error");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_raw(&mut child_stdin, b"Foo: bar\r\n\r\n");
    let resp = read_content_length_response(&mut child_stdout);
    assert!(
        resp.get("error").is_some(),
        "transport frame must fail: {resp}"
    );
    assert_eq!(
        resp["error"]["data"]["cabal_code"].as_str(),
        Some("TRANSPORT_ERROR"),
        "unexpected transport cabal_code payload: {resp}"
    );
    assert_eq!(resp["id"], Value::Null);

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_content_length_framed_requests_work() {
    let root = temp_root("cabal_mcp_stdio_framed");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_framed_jsonrpc(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"framed","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));
    assert_eq!(
        init["result"]["serverInfo"]["name"].as_str(),
        Some("cabal-mcp-runtime")
    );

    send_framed_jsonrpc(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,
            "method":"tools/list",
            "params":{}
        }),
    );
    let list = read_content_length_response(&mut child_stdout);
    assert_eq!(list["id"].as_i64(), Some(2));
    let tools = list["result"]["tools"].as_array().expect("tools array");
    assert!(
        tools
            .iter()
            .any(|x| x["name"].as_str() == Some("cabal.get_state"))
    );

    send_framed_jsonrpc(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":3,
            "method":"tools/call",
            "params":{"name":"cabal.set_consult_mode","arguments":{"mode":"YOLO"}}
        }),
    );
    let call_resp = read_content_length_response(&mut child_stdout);
    assert!(
        call_resp.get("result").is_some(),
        "framed tools/call failed: {call_resp}"
    );
    let tool_result = decode_tool_result(&call_resp);
    assert_eq!(tool_result["consult_mode"].as_str(), Some("yolo"));

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_mixed_ndjson_and_framed_requests_work() {
    let root = temp_root("cabal_mcp_stdio_mixed");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_framed_jsonrpc(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"mixed","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"cabal.set_consult_mode","arguments":{"mode":"YOLO"}}
        }),
    );
    let ndjson_call = read_content_length_response(&mut child_stdout);
    assert!(
        ndjson_call.get("result").is_some(),
        "ndjson tools/call failed: {ndjson_call}"
    );
    let ndjson_result = decode_tool_result(&ndjson_call);
    assert_eq!(ndjson_result["consult_mode"].as_str(), Some("yolo"));

    send_framed_jsonrpc(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":3,
            "method":"tools/list",
            "params":{}
        }),
    );
    let list = read_content_length_response(&mut child_stdout);
    assert_eq!(list["id"].as_i64(), Some(3));
    assert!(list["result"]["tools"].as_array().is_some());

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_batch_connect_handshake_works() {
    let root = temp_root("cabal_mcp_stdio_batch_connect");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_framed_jsonrpc(
        &mut child_stdin,
        &json!([
            {
                "jsonrpc":"2.0","id":1,
                "method":"initialize",
                "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"Roo Code","version":"3.50.5"}}
            },
            {
                "jsonrpc":"2.0",
                "method":"notifications/initialized",
                "params":{}
            },
            {
                "jsonrpc":"2.0","id":2,
                "method":"tools/list",
                "params":{}
            }
        ]),
    );
    let batch_resp = read_content_length_response(&mut child_stdout);
    let responses = batch_resp.as_array().expect("batch response array");
    assert_eq!(
        responses.len(),
        2,
        "unexpected batch response payload: {batch_resp}"
    );

    let init = responses
        .iter()
        .find(|x| x["id"].as_i64() == Some(1))
        .expect("initialize response");
    assert_eq!(
        init["result"]["serverInfo"]["name"].as_str(),
        Some("cabal-mcp-runtime")
    );
    assert_eq!(
        init["result"]["protocolVersion"].as_str(),
        Some("2025-01-01")
    );

    let tools = responses
        .iter()
        .find(|x| x["id"].as_i64() == Some(2))
        .expect("tools/list response");
    assert!(tools["result"]["tools"].as_array().is_some());

    send_raw(
        &mut child_stdin,
        b"[{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"ping\",\"params\":{}}]\n",
    );
    let ping_batch = read_content_length_response(&mut child_stdout);
    let ping_arr = ping_batch.as_array().expect("ndjson batch response array");
    assert_eq!(ping_arr.len(), 1);
    assert_eq!(ping_arr[0]["id"].as_i64(), Some(3));
    assert_eq!(ping_arr[0]["result"], json!({}));

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_proxy_policy_deny_returns_result_not_error() {
    let root = temp_root("cabal_mcp_stdio_proxy_deny_result");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"deny-result","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{
                "name":"cabal.proxy_execute",
                "arguments":{
                    "category":"network",
                    "operation":"http_get",
                    "target":"https://example.com",
                    "payload":{}
                }
            }
        }),
    );
    let resp = read_content_length_response(&mut child_stdout);
    assert!(
        resp.get("error").is_none(),
        "policy deny should be a result payload: {resp}"
    );
    let out = decode_tool_result(&resp);
    assert_eq!(out["allow"].as_bool(), Some(false));
    assert_eq!(out["executed"].as_bool(), Some(false));

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_apply_policy_revision_mismatch_returns_revision_mismatch_code() {
    let root = temp_root("cabal_mcp_stdio_policy_rev");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"policy-rev","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{
                "name":"cabal.apply_policy_bundle",
                "arguments":{
                    "expected_revision":999,
                    "version":"v2",
                    "rules":["r1"],
                    "forbidden_tokens":[]
                }
            }
        }),
    );
    let resp = read_content_length_response(&mut child_stdout);
    assert!(
        resp.get("error").is_some(),
        "revision mismatch must fail: {resp}"
    );
    assert_eq!(
        resp["error"]["data"]["cabal_code"].as_str(),
        Some("REVISION_MISMATCH"),
        "unexpected revision cabal_code payload: {resp}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_signed_policy_without_signature_returns_signature_invalid_code() {
    let root = temp_root("cabal_mcp_stdio_policy_sig");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"policy-sig","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"cabal.set_policy_security","arguments":{"require_signed_policy":true}}
        }),
    );
    let sec = read_content_length_response(&mut child_stdout);
    assert!(
        sec.get("result").is_some(),
        "set_policy_security failed: {sec}"
    );

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{
                "name":"cabal.apply_policy_bundle",
                "arguments":{
                    "expected_revision":1,
                    "version":"v2",
                    "rules":["r1"],
                    "nonce":"n-1",
                    "forbidden_tokens":[]
                }
            }
        }),
    );
    let resp = read_content_length_response(&mut child_stdout);
    assert!(
        resp.get("error").is_some(),
        "missing signature must fail: {resp}"
    );
    assert_eq!(
        resp["error"]["data"]["cabal_code"].as_str(),
        Some("SIGNATURE_INVALID"),
        "unexpected signature cabal_code payload: {resp}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_apply_policy_with_expired_signing_key_returns_signature_invalid_code() {
    let root = temp_root("cabal_mcp_stdio_policy_expired_key");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"policy-expired-key","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{
                "name":"cabal.upsert_policy_signing_key",
                "arguments":{
                    "key_id":"k-expired",
                    "key_env":"CABAL_TEST_EXPIRED_KEY",
                    "not_after_unix":1,
                    "set_active":true
                }
            }
        }),
    );
    let upsert = read_content_length_response(&mut child_stdout);
    assert!(
        upsert.get("result").is_some(),
        "upsert_policy_signing_key failed: {upsert}"
    );

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{
                "name":"cabal.apply_policy_bundle",
                "arguments":{
                    "expected_revision":1,
                    "version":"v2",
                    "rules":["r1"],
                    "forbidden_tokens":[],
                    "key_id":"k-expired",
                    "nonce":"n-expired",
                    "signature":"00"
                }
            }
        }),
    );
    let resp = read_content_length_response(&mut child_stdout);
    assert!(
        resp.get("error").is_some(),
        "apply_policy_bundle with expired key must fail: {resp}"
    );
    assert_eq!(
        resp["error"]["data"]["cabal_code"].as_str(),
        Some("SIGNATURE_INVALID"),
        "unexpected cabal_code payload for expired key: {resp}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_apply_policy_with_revoked_signing_key_returns_signature_invalid_code() {
    let root = temp_root("cabal_mcp_stdio_policy_revoked_key");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"policy-revoked-key","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{
                "name":"cabal.upsert_policy_signing_key",
                "arguments":{
                    "key_id":"k-revoked",
                    "key_env":"CABAL_TEST_REVOKED_KEY"
                }
            }
        }),
    );
    let upsert = read_content_length_response(&mut child_stdout);
    assert!(
        upsert.get("result").is_some(),
        "upsert_policy_signing_key failed: {upsert}"
    );

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{
                "name":"cabal.revoke_policy_signing_key",
                "arguments":{"key_id":"k-revoked"}
            }
        }),
    );
    let revoke = read_content_length_response(&mut child_stdout);
    assert!(
        revoke.get("result").is_some(),
        "revoke_policy_signing_key failed: {revoke}"
    );

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":4,"method":"tools/call",
            "params":{
                "name":"cabal.apply_policy_bundle",
                "arguments":{
                    "expected_revision":1,
                    "version":"v2",
                    "rules":["r1"],
                    "forbidden_tokens":[],
                    "key_id":"k-revoked",
                    "nonce":"n-revoked",
                    "signature":"00"
                }
            }
        }),
    );
    let resp = read_content_length_response(&mut child_stdout);
    assert!(
        resp.get("error").is_some(),
        "apply_policy_bundle with revoked key must fail: {resp}"
    );
    assert_eq!(
        resp["error"]["data"]["cabal_code"].as_str(),
        Some("SIGNATURE_INVALID"),
        "unexpected cabal_code payload for revoked key: {resp}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_route_consult_invalid_priority_returns_invalid_request_code() {
    let root = temp_root("cabal_mcp_stdio_consult_priority");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"consult-priority","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{
                "name":"cabal.route_consult",
                "arguments":{
                    "question":"q",
                    "consult_type":"code",
                    "priority":"urgent"
                }
            }
        }),
    );
    let resp = read_content_length_response(&mut child_stdout);
    assert!(
        resp.get("error").is_some(),
        "invalid priority must fail: {resp}"
    );
    assert_eq!(
        resp["error"]["data"]["cabal_code"].as_str(),
        Some("INVALID_REQUEST"),
        "unexpected invalid-request cabal_code payload: {resp}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_gate_check_unsupported_kind_returns_gate_fail_code() {
    let root = temp_root("cabal_mcp_stdio_gate_kind");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"gate-kind","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{
                "name":"cabal.gate_check",
                "arguments":{"kind":"unsupported_kind","phase":"C-0"}
            }
        }),
    );
    let resp = read_content_length_response(&mut child_stdout);
    assert!(
        resp.get("error").is_some(),
        "unsupported gate kind must fail: {resp}"
    );
    assert_eq!(
        resp["error"]["data"]["cabal_code"].as_str(),
        Some("GATE_FAIL"),
        "unexpected gate-fail cabal_code payload: {resp}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_proxy_execute_missing_file_returns_io_failure_code() {
    let root = temp_root("cabal_mcp_stdio_io_failure");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"io-failure","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{
                "name":"cabal.proxy_execute",
                "arguments":{
                    "category":"fs",
                    "operation":"read_text",
                    "target":".memory/not_existing.txt",
                    "payload":{}
                }
            }
        }),
    );
    let resp = read_content_length_response(&mut child_stdout);
    assert!(
        resp.get("error").is_some(),
        "missing file should fail: {resp}"
    );
    assert_eq!(
        resp["error"]["data"]["cabal_code"].as_str(),
        Some("IO_FAILURE"),
        "unexpected IO_FAILURE payload: {resp}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_export_audit_log_to_directory_returns_storage_failure_code() {
    let root = temp_root("cabal_mcp_stdio_storage_failure");
    fs::create_dir_all(root.join(".memory")).expect("mkdir .memory");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"storage-failure","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{
                "name":"cabal.export_audit_log",
                "arguments":{"out_path":".memory"}
            }
        }),
    );
    let resp = read_content_length_response(&mut child_stdout);
    assert!(
        resp.get("error").is_some(),
        "export to directory should fail: {resp}"
    );
    assert_eq!(
        resp["error"]["data"]["cabal_code"].as_str(),
        Some("STORAGE_FAILURE"),
        "unexpected STORAGE_FAILURE payload: {resp}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_route_consult_audit_contract_fields_present() {
    let root = temp_root("cabal_mcp_stdio_consult_audit");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"Visual Studio Code","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"cabal.set_consult_mode","arguments":{"mode":"YOLO"}}
        }),
    );
    let mode_resp = read_content_length_response(&mut child_stdout);
    assert!(
        mode_resp.get("result").is_some(),
        "set mode failed: {mode_resp}"
    );

    let request_id = "rq-stdio-consult-1";
    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{
                "name":"cabal.route_consult",
                "arguments":{
                    "question":"need routing contract",
                    "consult_type":"code",
                    "priority":"high",
                    "request_id":request_id
                }
            }
        }),
    );
    let routed_raw = read_content_length_response(&mut child_stdout);
    assert!(
        routed_raw.get("error").is_none(),
        "route_consult should pass: {routed_raw}"
    );
    let routed = decode_tool_result(&routed_raw);
    assert_eq!(routed["actor"].as_str(), Some("orchestrator"));
    assert_eq!(routed["ide_profile"].as_str(), Some("vscode"));
    assert_eq!(
        routed["ide_client_name"].as_str(),
        Some("Visual Studio Code")
    );
    assert!(routed["policy_revision"].as_u64().unwrap_or(0) >= 1);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":4,"method":"tools/call",
            "params":{
                "name":"cabal.query_audit_log",
                "arguments":{
                    "kind":"consult.routed",
                    "request_id":request_id,
                    "limit":20
                }
            }
        }),
    );
    let query_raw = read_content_length_response(&mut child_stdout);
    assert!(
        query_raw.get("error").is_none(),
        "query_audit_log should pass: {query_raw}"
    );
    let query = decode_tool_result(&query_raw);
    assert!(
        query["matched"].as_u64().unwrap_or(0) >= 1,
        "no matching audit rows: {query}"
    );
    let items = query["items"].as_array().expect("items");
    let found = items.iter().any(|item| {
        item["payload"]["request_id"].as_str() == Some(request_id)
            && item["payload"]["actor"].as_str() == Some("orchestrator")
            && item["payload"]["ide_profile"].as_str() == Some("vscode")
            && item["payload"]["ide_client_name"].as_str() == Some("Visual Studio Code")
            && item["payload"]["policy_revision"].as_u64().unwrap_or(0) >= 1
    });
    assert!(
        found,
        "consult.routed audit contract fields missing: {query}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_route_consult_audit_contract_fields_present_for_jetbrains_profile() {
    let root = temp_root("cabal_mcp_stdio_consult_audit_jetbrains");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"IntelliJ IDEA","version":"2025.1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"cabal.set_consult_mode","arguments":{"mode":"YOLO"}}
        }),
    );
    let mode_resp = read_content_length_response(&mut child_stdout);
    assert!(
        mode_resp.get("result").is_some(),
        "set mode failed: {mode_resp}"
    );

    let request_id = "rq-stdio-consult-jetbrains-1";
    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{
                "name":"cabal.route_consult",
                "arguments":{
                    "question":"need routing contract for jetbrains profile",
                    "consult_type":"code",
                    "priority":"high",
                    "request_id":request_id
                }
            }
        }),
    );
    let routed_raw = read_content_length_response(&mut child_stdout);
    assert!(
        routed_raw.get("error").is_none(),
        "route_consult should pass: {routed_raw}"
    );
    let routed = decode_tool_result(&routed_raw);
    assert_eq!(routed["actor"].as_str(), Some("orchestrator"));
    assert_eq!(routed["ide_profile"].as_str(), Some("jetbrains"));
    assert_eq!(routed["ide_client_name"].as_str(), Some("IntelliJ IDEA"));
    assert!(routed["policy_revision"].as_u64().unwrap_or(0) >= 1);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":4,"method":"tools/call",
            "params":{
                "name":"cabal.query_audit_log",
                "arguments":{
                    "kind":"consult.routed",
                    "request_id":request_id,
                    "limit":20
                }
            }
        }),
    );
    let query_raw = read_content_length_response(&mut child_stdout);
    assert!(
        query_raw.get("error").is_none(),
        "query_audit_log should pass: {query_raw}"
    );
    let query = decode_tool_result(&query_raw);
    assert!(
        query["matched"].as_u64().unwrap_or(0) >= 1,
        "no matching audit rows: {query}"
    );
    let items = query["items"].as_array().expect("items");
    let found = items.iter().any(|item| {
        item["payload"]["request_id"].as_str() == Some(request_id)
            && item["payload"]["actor"].as_str() == Some("orchestrator")
            && item["payload"]["ide_profile"].as_str() == Some("jetbrains")
            && item["payload"]["ide_client_name"].as_str() == Some("IntelliJ IDEA")
            && item["payload"]["policy_revision"].as_u64().unwrap_or(0) >= 1
    });
    assert!(
        found,
        "consult.routed audit contract fields missing for jetbrains profile: {query}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_validate_error_codes_parity_passes() {
    let root = temp_root("cabal_mcp_stdio_error_parity");
    fs::create_dir_all(root.join("spec").join("docs")).expect("mkdir spec/docs");
    fs::write(
        root.join("spec").join("docs").join("CABAL_ERROR_CODES.md"),
        include_str!("../../spec/docs/CABAL_ERROR_CODES.md"),
    )
    .expect("write codes doc");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"error-parity","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"cabal.validate_error_codes_parity","arguments":{}}
        }),
    );
    let parity_raw = read_content_length_response(&mut child_stdout);
    assert!(
        parity_raw.get("error").is_none(),
        "validate_error_codes_parity failed: {parity_raw}"
    );
    let parity = decode_tool_result(&parity_raw);
    assert_eq!(parity["report"]["pass"].as_bool(), Some(true));

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_initialize_tracks_vscode_ide_profile() {
    let root = temp_root("cabal_mcp_stdio_ide_profile");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"Visual Studio Code","version":"1.96.0"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));
    assert_eq!(
        init["result"]["cabal"]["ide_profile"].as_str(),
        Some("vscode")
    );

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"cabal.get_ide_profile_policy","arguments":{}}
        }),
    );
    let policy_raw = read_content_length_response(&mut child_stdout);
    assert!(
        policy_raw.get("error").is_none(),
        "get_ide_profile_policy should pass: {policy_raw}"
    );
    let policy = decode_tool_result(&policy_raw);
    assert_eq!(policy["active_profile"].as_str(), Some("vscode"));
    assert_eq!(
        policy["active_client"]["name"].as_str(),
        Some("Visual Studio Code")
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_initialize_profile_enforcement_blocks_disallowed_client() {
    let root = temp_root("cabal_mcp_stdio_ide_profile_deny");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{
                "name":"cabal.set_ide_profile_policy",
                "arguments":{
                    "enforce_ide_profile":true,
                    "allowed_profiles":["generic","jetbrains"]
                }
            }
        }),
    );
    let set_resp = read_content_length_response(&mut child_stdout);
    assert!(
        set_resp.get("result").is_some(),
        "set ide policy failed: {set_resp}"
    );

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"Visual Studio Code","version":"1.96.0"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert!(
        init.get("error").is_some(),
        "initialize should fail under profile enforcement: {init}"
    );
    assert_eq!(
        init["error"]["data"]["cabal_code"].as_str(),
        Some("POLICY_DENY"),
        "unexpected cabal_code for blocked IDE profile: {init}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_initialize_profile_enforcement_allows_jetbrains_client() {
    let root = temp_root("cabal_mcp_stdio_ide_profile_allow");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{
                "name":"cabal.set_ide_profile_policy",
                "arguments":{
                    "enforce_ide_profile":true,
                    "allowed_profiles":["generic","jetbrains"]
                }
            }
        }),
    );
    let set_resp = read_content_length_response(&mut child_stdout);
    assert!(
        set_resp.get("result").is_some(),
        "set ide policy failed: {set_resp}"
    );

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"JetBrains IntelliJ IDEA","version":"2025.1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert!(
        init.get("error").is_none(),
        "initialize should pass: {init}"
    );
    assert_eq!(
        init["result"]["cabal"]["ide_profile"].as_str(),
        Some("jetbrains")
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_ide_enforcement_then_consult_route_uses_allowed_profile_context() {
    let root = temp_root("cabal_mcp_stdio_ide_consult_chain");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{
                "name":"cabal.set_ide_profile_policy",
                "arguments":{
                    "enforce_ide_profile":true,
                    "require_client_info":true,
                    "allowed_profiles":["generic","jetbrains"]
                }
            }
        }),
    );
    let set_resp = read_content_length_response(&mut child_stdout);
    assert!(
        set_resp.get("result").is_some(),
        "set ide policy failed: {set_resp}"
    );

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"Visual Studio Code","version":"1.96.0"}}
        }),
    );
    let blocked = read_content_length_response(&mut child_stdout);
    assert!(
        blocked.get("error").is_some(),
        "initialize must fail for disallowed ide: {blocked}"
    );
    assert_eq!(
        blocked["error"]["data"]["cabal_code"].as_str(),
        Some("POLICY_DENY"),
        "unexpected cabal_code for blocked ide: {blocked}"
    );

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":3,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"IntelliJ IDEA","version":"2025.1"}}
        }),
    );
    let allowed = read_content_length_response(&mut child_stdout);
    assert!(
        allowed.get("error").is_none(),
        "initialize should pass for allowed ide: {allowed}"
    );
    assert_eq!(
        allowed["result"]["cabal"]["ide_profile"].as_str(),
        Some("jetbrains")
    );

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":4,"method":"tools/call",
            "params":{"name":"cabal.set_consult_mode","arguments":{"mode":"YOLO"}}
        }),
    );
    let mode_resp = read_content_length_response(&mut child_stdout);
    assert!(
        mode_resp.get("result").is_some(),
        "set consult mode failed: {mode_resp}"
    );

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":5,"method":"tools/call",
            "params":{
                "name":"cabal.route_consult",
                "arguments":{
                    "question":"verify ide-profile consult chain",
                    "consult_type":"code",
                    "priority":"normal",
                    "request_id":"rq-ide-chain-1"
                }
            }
        }),
    );
    let route_resp = read_content_length_response(&mut child_stdout);
    assert!(
        route_resp.get("error").is_none(),
        "route_consult should pass: {route_resp}"
    );
    let routed = decode_tool_result(&route_resp);
    assert_eq!(routed["route"].as_str(), Some("orchestrator"));
    assert_eq!(routed["ide_profile"].as_str(), Some("jetbrains"));
    assert_eq!(routed["ide_client_name"].as_str(), Some("IntelliJ IDEA"));

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_initialize_missing_client_info_denied_when_required() {
    let root = temp_root("cabal_mcp_stdio_ide_profile_require_client_info");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{
                "name":"cabal.set_ide_profile_policy",
                "arguments":{
                    "enforce_ide_profile":true,
                    "require_client_info":true,
                    "allowed_profiles":["generic","jetbrains"]
                }
            }
        }),
    );
    let set_resp = read_content_length_response(&mut child_stdout);
    assert!(
        set_resp.get("result").is_some(),
        "set ide policy failed: {set_resp}"
    );

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert!(
        init.get("error").is_some(),
        "initialize should fail without clientInfo.name: {init}"
    );
    assert_eq!(
        init["error"]["data"]["cabal_code"].as_str(),
        Some("POLICY_DENY"),
        "unexpected cabal_code for missing client info: {init}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_initialize_with_client_info_allowed_when_required() {
    let root = temp_root("cabal_mcp_stdio_ide_profile_require_client_info_allow");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{
                "name":"cabal.set_ide_profile_policy",
                "arguments":{
                    "enforce_ide_profile":true,
                    "require_client_info":true,
                    "allowed_profiles":["generic","jetbrains"]
                }
            }
        }),
    );
    let set_resp = read_content_length_response(&mut child_stdout);
    assert!(
        set_resp.get("result").is_some(),
        "set ide policy failed: {set_resp}"
    );

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"JetBrains IntelliJ IDEA","version":"2025.1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert!(
        init.get("error").is_none(),
        "initialize should pass with clientInfo.name: {init}"
    );
    assert_eq!(
        init["result"]["cabal"]["ide_profile"].as_str(),
        Some("jetbrains")
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_rotate_and_verify_audit_archive() {
    let root = temp_root("cabal_mcp_stdio_audit_rotate");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"audit-rotate","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"cabal.set_consult_mode","arguments":{"mode":"YOLO"}}
        }),
    );
    let mode_resp = read_content_length_response(&mut child_stdout);
    assert!(
        mode_resp.get("result").is_some(),
        "set mode failed: {mode_resp}"
    );

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{"name":"cabal.rotate_audit_log","arguments":{"compress":true}}
        }),
    );
    let rotate_raw = read_content_length_response(&mut child_stdout);
    assert!(
        rotate_raw.get("error").is_none(),
        "rotate_audit_log failed: {rotate_raw}"
    );
    let rotate = decode_tool_result(&rotate_raw);
    assert_eq!(rotate["archive"]["rotated"].as_bool(), Some(true));
    let archive_path = rotate["archive"]["archive_path"]
        .as_str()
        .expect("archive_path")
        .to_string();
    let signature_path = rotate["archive"]["signature_path"]
        .as_str()
        .expect("signature_path")
        .to_string();

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":4,"method":"tools/call",
            "params":{
                "name":"cabal.verify_audit_archive",
                "arguments":{
                    "archive_path": archive_path,
                    "signature_path": signature_path
                }
            }
        }),
    );
    let verify_raw = read_content_length_response(&mut child_stdout);
    assert!(
        verify_raw.get("error").is_none(),
        "verify_audit_archive failed: {verify_raw}"
    );
    let verify = decode_tool_result(&verify_raw);
    assert_eq!(verify["pass"].as_bool(), Some(true));

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_rotate_empty_audit_returns_invalid_request() {
    let root = temp_root("cabal_mcp_stdio_audit_rotate_empty");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"name":"cabal.rotate_audit_log","arguments":{"compress":false}}
        }),
    );
    let rotate = read_content_length_response(&mut child_stdout);
    assert!(
        rotate.get("error").is_some(),
        "rotate should fail on empty audit: {rotate}"
    );
    assert_eq!(
        rotate["error"]["data"]["cabal_code"].as_str(),
        Some("INVALID_REQUEST"),
        "unexpected cabal_code for empty rotate: {rotate}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_prune_audit_archives_keeps_last_n() {
    let root = temp_root("cabal_mcp_stdio_audit_prune");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"audit-prune","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    let mut id = 2i64;
    for idx in 0..3 {
        let mode = if idx % 2 == 0 {
            "YOLO"
        } else {
            "USER_TRACKING"
        };
        send_ndjson(
            &mut child_stdin,
            &json!({
                "jsonrpc":"2.0","id":id,"method":"tools/call",
                "params":{"name":"cabal.set_consult_mode","arguments":{"mode":mode}}
            }),
        );
        let mode_resp = read_content_length_response(&mut child_stdout);
        assert!(
            mode_resp.get("result").is_some(),
            "set mode failed: {mode_resp}"
        );
        id += 1;

        send_ndjson(
            &mut child_stdin,
            &json!({
                "jsonrpc":"2.0","id":id,"method":"tools/call",
                "params":{"name":"cabal.rotate_audit_log","arguments":{"compress":false}}
            }),
        );
        let rotate_resp = read_content_length_response(&mut child_stdout);
        assert!(
            rotate_resp.get("error").is_none(),
            "rotate_audit_log failed: {rotate_resp}"
        );
        id += 1;
    }

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":id,"method":"tools/call",
            "params":{"name":"cabal.prune_audit_archives","arguments":{"keep_last":1}}
        }),
    );
    let prune_raw = read_content_length_response(&mut child_stdout);
    assert!(
        prune_raw.get("error").is_none(),
        "prune_audit_archives failed: {prune_raw}"
    );
    let prune = decode_tool_result(&prune_raw);
    assert_eq!(prune["kept"].as_u64(), Some(1));
    assert!(prune["removed"].as_u64().unwrap_or(0) >= 2);

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_set_and_get_audit_rotation_policy() {
    let root = temp_root("cabal_mcp_stdio_audit_rotation_policy");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"audit-rotation-policy","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{
                "name":"cabal.set_audit_rotation_policy",
                "arguments":{
                    "enabled":true,
                    "max_bytes":4096,
                    "max_age_sec":300,
                    "compress":false,
                    "keep_last":4,
                    "archive_dir":".cabal_runtime/archive_custom"
                }
            }
        }),
    );
    let set_raw = read_content_length_response(&mut child_stdout);
    assert!(
        set_raw.get("error").is_none(),
        "set policy failed: {set_raw}"
    );

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{"name":"cabal.get_audit_rotation_policy","arguments":{}}
        }),
    );
    let get_raw = read_content_length_response(&mut child_stdout);
    assert!(
        get_raw.get("error").is_none(),
        "get policy failed: {get_raw}"
    );
    let get = decode_tool_result(&get_raw);
    assert_eq!(get["enabled"].as_bool(), Some(true));
    assert_eq!(get["max_bytes"].as_u64(), Some(4096));
    assert_eq!(get["max_age_sec"].as_u64(), Some(300));
    assert_eq!(get["compress"].as_bool(), Some(false));
    assert_eq!(get["keep_last"].as_u64(), Some(4));
    assert_eq!(
        get["archive_dir"].as_str(),
        Some(".cabal_runtime/archive_custom")
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_auto_rotate_by_size_records_audit_rotated() {
    let root = temp_root("cabal_mcp_stdio_audit_auto_rotate");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"audit-auto-rotate","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{
                "name":"cabal.set_audit_rotation_policy",
                "arguments":{
                    "enabled":true,
                    "max_bytes":500,
                    "max_age_sec":86400,
                    "compress":false,
                    "keep_last":5
                }
            }
        }),
    );
    let set_raw = read_content_length_response(&mut child_stdout);
    assert!(
        set_raw.get("error").is_none(),
        "set policy failed: {set_raw}"
    );

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{"name":"cabal.set_consult_mode","arguments":{"mode":"YOLO"}}
        }),
    );
    let mode_raw = read_content_length_response(&mut child_stdout);
    assert!(
        mode_raw.get("error").is_none(),
        "set mode failed: {mode_raw}"
    );

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":4,"method":"tools/call",
            "params":{
                "name":"cabal.query_audit_log",
                "arguments":{"kind":"audit.rotated","limit":20}
            }
        }),
    );
    let query_raw = read_content_length_response(&mut child_stdout);
    assert!(
        query_raw.get("error").is_none(),
        "query_audit_log failed: {query_raw}"
    );
    let query = decode_tool_result(&query_raw);
    assert!(
        query["matched"].as_u64().unwrap_or(0) >= 1,
        "expected at least one audit.rotated event: {query}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_prune_audit_archives_zero_keep_last_is_invalid_request() {
    let root = temp_root("cabal_mcp_stdio_audit_prune_invalid");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"name":"cabal.prune_audit_archives","arguments":{"keep_last":0}}
        }),
    );
    let prune = read_content_length_response(&mut child_stdout);
    assert!(
        prune.get("error").is_some(),
        "prune with keep_last=0 must fail: {prune}"
    );
    assert_eq!(
        prune["error"]["data"]["cabal_code"].as_str(),
        Some("INVALID_REQUEST"),
        "unexpected cabal_code: {prune}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_audit_health_check_pass_and_fail_paths() {
    let root = temp_root("cabal_mcp_stdio_audit_health");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"audit-health","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"cabal.set_consult_mode","arguments":{"mode":"YOLO"}}
        }),
    );
    let mode_raw = read_content_length_response(&mut child_stdout);
    assert!(
        mode_raw.get("error").is_none(),
        "set mode failed: {mode_raw}"
    );

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{"name":"cabal.rotate_audit_log","arguments":{"compress":false}}
        }),
    );
    let rotate_raw = read_content_length_response(&mut child_stdout);
    assert!(
        rotate_raw.get("error").is_none(),
        "rotate failed: {rotate_raw}"
    );
    let rotate = decode_tool_result(&rotate_raw);
    let signature_path = rotate["archive"]["signature_path"]
        .as_str()
        .expect("signature_path")
        .to_string();

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":4,"method":"tools/call",
            "params":{"name":"cabal.audit_health_check","arguments":{"verify_last":5}}
        }),
    );
    let health_ok_raw = read_content_length_response(&mut child_stdout);
    assert!(
        health_ok_raw.get("error").is_none(),
        "health check failed: {health_ok_raw}"
    );
    let health_ok = decode_tool_result(&health_ok_raw);
    assert_eq!(health_ok["status"].as_str(), Some("pass"));
    assert!(health_ok["archives"]["checked"].as_u64().unwrap_or(0) >= 1);

    fs::write(root.join(signature_path), "deadbeef  broken\n").expect("tamper signature");

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":5,"method":"tools/call",
            "params":{"name":"cabal.audit_health_check","arguments":{"verify_last":5}}
        }),
    );
    let health_fail_raw = read_content_length_response(&mut child_stdout);
    assert!(
        health_fail_raw.get("error").is_none(),
        "health check failed after tamper: {health_fail_raw}"
    );
    let health_fail = decode_tool_result(&health_fail_raw);
    assert_eq!(health_fail["status"].as_str(), Some("fail"));
    assert!(health_fail["archives"]["failed"].as_u64().unwrap_or(0) >= 1);

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_audit_health_check_zero_verify_last_is_invalid_request() {
    let root = temp_root("cabal_mcp_stdio_audit_health_invalid");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"name":"cabal.audit_health_check","arguments":{"verify_last":0}}
        }),
    );
    let health = read_content_length_response(&mut child_stdout);
    assert!(
        health.get("error").is_some(),
        "health check with verify_last=0 must fail: {health}"
    );
    assert_eq!(
        health["error"]["data"]["cabal_code"].as_str(),
        Some("INVALID_REQUEST"),
        "unexpected cabal_code: {health}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_query_audit_log_zero_limit_is_invalid_request() {
    let root = temp_root("cabal_mcp_stdio_audit_query_zero_limit");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"name":"cabal.query_audit_log","arguments":{"limit":0}}
        }),
    );
    let resp = read_content_length_response(&mut child_stdout);
    assert!(
        resp.get("error").is_some(),
        "query_audit_log with limit=0 must fail: {resp}"
    );
    assert_eq!(
        resp["error"]["data"]["cabal_code"].as_str(),
        Some("INVALID_REQUEST"),
        "unexpected cabal_code payload: {resp}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_query_audit_log_exposes_max_limit() {
    let root = temp_root("cabal_mcp_stdio_audit_query_max_limit");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"audit-query-max-limit","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"cabal.set_consult_mode","arguments":{"mode":"YOLO"}}
        }),
    );
    let mode_raw = read_content_length_response(&mut child_stdout);
    assert!(
        mode_raw.get("error").is_none(),
        "set mode failed: {mode_raw}"
    );

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{"name":"cabal.query_audit_log","arguments":{"kind":"consult_mode.changed","limit":99999}}
        }),
    );
    let query_raw = read_content_length_response(&mut child_stdout);
    assert!(
        query_raw.get("error").is_none(),
        "query_audit_log failed: {query_raw}"
    );
    let query = decode_tool_result(&query_raw);
    assert_eq!(query["max_limit"].as_u64(), Some(2000));
    assert!(query["matched"].as_u64().unwrap_or(0) >= 1);

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_route_consult_guard_requires_cross_rules_ack_evidence() {
    let root = temp_root("cabal_mcp_stdio_consult_guard");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"consult-guard","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    let setup = vec![
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"cabal.set_consult_mode","arguments":{"mode":"YOLO"}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"cabal.set_consult_guard_policy","arguments":{"require_cross_rules_ack":true,"required_evidence_ids":["cross_rules_agent_ack","cross_rules_subagent_ack"]}}}),
    ];
    for call in setup {
        send_ndjson(&mut child_stdin, &call);
        let resp = read_content_length_response(&mut child_stdout);
        assert!(resp.get("error").is_none(), "setup call failed: {resp}");
    }

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":4,"method":"tools/call",
            "params":{
                "name":"cabal.route_consult",
                "arguments":{
                    "question":"optimize unsafe kernel",
                    "consult_type":"performance",
                    "priority":"high",
                    "request_id":"rq-guard-stdio-1"
                }
            }
        }),
    );
    let blocked = read_content_length_response(&mut child_stdout);
    assert!(
        blocked.get("error").is_some(),
        "route should fail: {blocked}"
    );
    assert_eq!(
        blocked["error"]["data"]["cabal_code"].as_str(),
        Some("POLICY_DENY"),
        "unexpected cabal_code for consult guard deny: {blocked}"
    );

    let evidence_calls = vec![
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"cabal.register_evidence","arguments":{"id":"cross_rules_agent_ack","path":"spec/docs/CONCEPT_MASTER.md"}}}),
        json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"cabal.register_evidence","arguments":{"id":"cross_rules_subagent_ack","path":"spec/docs/CONCEPT_MASTER.md"}}}),
    ];
    for call in evidence_calls {
        send_ndjson(&mut child_stdin, &call);
        let resp = read_content_length_response(&mut child_stdout);
        assert!(
            resp.get("error").is_none(),
            "register_evidence failed: {resp}"
        );
    }

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":7,"method":"tools/call",
            "params":{
                "name":"cabal.route_consult",
                "arguments":{
                    "question":"optimize unsafe kernel",
                    "consult_type":"performance",
                    "priority":"high",
                    "request_id":"rq-guard-stdio-2"
                }
            }
        }),
    );
    let routed_raw = read_content_length_response(&mut child_stdout);
    assert!(
        routed_raw.get("error").is_none(),
        "route_consult should pass after evidence: {routed_raw}"
    );
    let routed = decode_tool_result(&routed_raw);
    assert_eq!(routed["route"].as_str(), Some("orchestrator"));

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_ack_cross_rules_updates_status_and_unblocks_consult() {
    let root = temp_root("cabal_mcp_stdio_ack_cross_rules");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"ack-cross-rules","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    let setup = vec![
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"cabal.set_consult_mode","arguments":{"mode":"YOLO"}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"cabal.set_consult_guard_policy","arguments":{"require_cross_rules_ack":true,"required_evidence_ids":["cross_rules_agent_ack","cross_rules_subagent_ack"]}}}),
    ];
    for call in setup {
        send_ndjson(&mut child_stdin, &call);
        let resp = read_content_length_response(&mut child_stdout);
        assert!(resp.get("error").is_none(), "setup call failed: {resp}");
    }

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":4,"method":"tools/call",
            "params":{"name":"cabal.get_cross_rules_status","arguments":{}}
        }),
    );
    let status_before_raw = read_content_length_response(&mut child_stdout);
    assert!(
        status_before_raw.get("error").is_none(),
        "get_cross_rules_status failed: {status_before_raw}"
    );
    let status_before = decode_tool_result(&status_before_raw);
    assert_eq!(
        status_before["entry_gate_all_present"].as_bool(),
        Some(false)
    );
    assert_eq!(
        status_before["consult_guard"]["all_present"].as_bool(),
        Some(false)
    );

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":5,"method":"tools/call",
            "params":{
                "name":"cabal.ack_cross_rules",
                "arguments":{
                    "agent_ack_path":"spec/docs/CONCEPT_MASTER.md",
                    "subagent_ack_path":"spec/docs/CONCEPT_MASTER.md",
                    "enable_consult_guard":true
                }
            }
        }),
    );
    let ack_raw = read_content_length_response(&mut child_stdout);
    assert!(
        ack_raw.get("error").is_none(),
        "ack_cross_rules failed: {ack_raw}"
    );
    let ack = decode_tool_result(&ack_raw);
    assert_eq!(ack["entry_gate_all_present"].as_bool(), Some(true));
    assert_eq!(ack["consult_guard"]["all_present"].as_bool(), Some(true));
    assert_eq!(ack["consult_guard"]["enabled"].as_bool(), Some(true));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":6,"method":"tools/call",
            "params":{
                "name":"cabal.route_consult",
                "arguments":{
                    "question":"optimize unsafe kernel",
                    "consult_type":"performance",
                    "priority":"high",
                    "request_id":"rq-ack-cross-rules-1"
                }
            }
        }),
    );
    let routed_raw = read_content_length_response(&mut child_stdout);
    assert!(
        routed_raw.get("error").is_none(),
        "route_consult should pass after ack_cross_rules: {routed_raw}"
    );
    let routed = decode_tool_result(&routed_raw);
    assert_eq!(routed["route"].as_str(), Some("orchestrator"));

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_ack_cross_rules_empty_path_is_invalid_request() {
    let root = temp_root("cabal_mcp_stdio_ack_cross_rules_invalid");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"ack-cross-rules-invalid","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{
                "name":"cabal.ack_cross_rules",
                "arguments":{
                    "agent_ack_path":" ",
                    "subagent_ack_path":"spec/docs/CONCEPT_MASTER.md",
                    "enable_consult_guard":true
                }
            }
        }),
    );
    let resp = read_content_length_response(&mut child_stdout);
    assert!(
        resp.get("error").is_some(),
        "ack_cross_rules with empty path must fail: {resp}"
    );
    assert_eq!(
        resp["error"]["data"]["cabal_code"].as_str(),
        Some("INVALID_REQUEST"),
        "unexpected cabal_code payload: {resp}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_route_consult_role_mismatch_fallback_and_escalation() {
    let root = temp_root("cabal_mcp_stdio_consult_role_fallback");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"consult-role-fallback","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    let calls = vec![
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"cabal.set_consult_mode","arguments":{"mode":"YOLO"}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"cabal.set_consult_routing_rule","arguments":{"consult_type":"math","executor":"symbolic_solver"}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"cabal.set_consult_allowed_roles","arguments":{"consult_type":"math","roles":["mathematician"]}}}),
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"cabal.set_consult_escalation_target","arguments":{"priority":"high","target":"architect"}}}),
        json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"cabal.set_consult_retry_limit","arguments":{"priority":"high","max_retries":4}}}),
    ];
    for call in calls {
        send_ndjson(&mut child_stdin, &call);
        let resp = read_content_length_response(&mut child_stdout);
        assert!(resp.get("error").is_none(), "setup call failed: {resp}");
    }

    let request_id = "rq-role-fallback-1";
    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":7,"method":"tools/call",
            "params":{
                "name":"cabal.route_consult",
                "arguments":{
                    "question":"prove invariant",
                    "consult_type":"math",
                    "priority":"high",
                    "preferred_role":"developer",
                    "request_id":request_id
                }
            }
        }),
    );
    let routed_raw = read_content_length_response(&mut child_stdout);
    assert!(
        routed_raw.get("error").is_none(),
        "route_consult failed: {routed_raw}"
    );
    let routed = decode_tool_result(&routed_raw);
    assert_eq!(routed["route"].as_str(), Some("orchestrator"));
    assert_eq!(
        routed["dispatch"]["executor"].as_str(),
        Some("mathematician")
    );
    assert_eq!(routed["timeout_sec"].as_u64(), Some(900));
    assert_eq!(routed["retry_policy"]["max_retries"].as_u64(), Some(4));
    assert_eq!(routed["escalation"]["required"].as_bool(), Some(true));
    assert_eq!(routed["escalation"]["target"].as_str(), Some("architect"));
    assert_eq!(
        routed["escalation"]["reason"].as_str(),
        Some("preferred_role_not_allowed")
    );

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":8,"method":"tools/call",
            "params":{
                "name":"cabal.query_audit_log",
                "arguments":{
                    "kind":"consult.routed",
                    "request_id":request_id,
                    "limit":20
                }
            }
        }),
    );
    let query_raw = read_content_length_response(&mut child_stdout);
    assert!(
        query_raw.get("error").is_none(),
        "query_audit_log failed: {query_raw}"
    );
    let query = decode_tool_result(&query_raw);
    assert!(query["matched"].as_u64().unwrap_or(0) >= 1);
    let found_reason = query["items"]
        .as_array()
        .expect("items")
        .iter()
        .any(|item| {
            item["payload"]["request_id"].as_str() == Some(request_id)
                && item["payload"]["escalation"]["reason"].as_str()
                    == Some("preferred_role_not_allowed")
        });
    assert!(
        found_reason,
        "consult.routed payload missing expected escalation reason: {query}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_route_consult_critical_priority_sla_and_escalation() {
    let root = temp_root("cabal_mcp_stdio_consult_critical");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"consult-critical","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"cabal.set_consult_mode","arguments":{"mode":"YOLO"}}
        }),
    );
    let mode_resp = read_content_length_response(&mut child_stdout);
    assert!(
        mode_resp.get("error").is_none(),
        "set mode failed: {mode_resp}"
    );

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{
                "name":"cabal.route_consult",
                "arguments":{
                    "question":"urgent production rollback",
                    "consult_type":"code",
                    "priority":"critical",
                    "request_id":"rq-critical-1"
                }
            }
        }),
    );
    let routed_raw = read_content_length_response(&mut child_stdout);
    assert!(
        routed_raw.get("error").is_none(),
        "route_consult failed: {routed_raw}"
    );
    let routed = decode_tool_result(&routed_raw);
    assert_eq!(routed["route"].as_str(), Some("orchestrator"));
    assert_eq!(routed["timeout_sec"].as_u64(), Some(300));
    assert_eq!(routed["retry_policy"]["max_retries"].as_u64(), Some(0));
    assert_eq!(routed["escalation"]["required"].as_bool(), Some(true));
    assert_eq!(routed["escalation"]["target"].as_str(), Some("user"));
    assert_eq!(
        routed["escalation"]["reason"].as_str(),
        Some("critical_priority")
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_route_consult_adaptive_confidence_floor_fallback() {
    let root = temp_root("cabal_mcp_stdio_consult_confidence_floor");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"consult-confidence-floor","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    let calls = vec![
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"cabal.set_consult_mode","arguments":{"mode":"YOLO"}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"cabal.set_adaptive_router","arguments":{"enabled":true,"confidence_floor":0.95}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"cabal.set_consult_routing_rule","arguments":{"consult_type":"performance","executor":"developer"}}}),
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"cabal.set_consult_allowed_roles","arguments":{"consult_type":"performance","roles":["developer","perf_engineer"]}}}),
        json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"cabal.record_consult_feedback","arguments":{"request_id":"rq-perf-low","consult_type":"performance","executor":"perf_engineer","success":true,"latency_ms":100}}}),
    ];
    for call in calls {
        send_ndjson(&mut child_stdin, &call);
        let resp = read_content_length_response(&mut child_stdout);
        assert!(resp.get("error").is_none(), "setup call failed: {resp}");
    }

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":7,"method":"tools/call",
            "params":{
                "name":"cabal.route_consult",
                "arguments":{
                    "question":"optimize simd kernel",
                    "consult_type":"performance",
                    "priority":"high",
                    "request_id":"rq-floor-1"
                }
            }
        }),
    );
    let routed_raw = read_content_length_response(&mut child_stdout);
    assert!(
        routed_raw.get("error").is_none(),
        "route_consult failed: {routed_raw}"
    );
    let routed = decode_tool_result(&routed_raw);
    assert_eq!(routed["dispatch"]["executor"].as_str(), Some("developer"));
    assert_eq!(
        routed["routing_decision"]["strategy"].as_str(),
        Some("policy_confidence_floor")
    );
    assert!(
        routed["routing_decision"]["confidence"]
            .as_f64()
            .expect("confidence")
            < 0.95
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_set_adaptive_exploration_policy_zero_min_samples_is_invalid_request() {
    let root = temp_root("cabal_mcp_stdio_adaptive_exploration_invalid");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{
                "name":"cabal.set_adaptive_exploration_policy",
                "arguments":{"exploration_min_samples":0}
            }
        }),
    );
    let resp = read_content_length_response(&mut child_stdout);
    assert!(
        resp.get("error").is_some(),
        "invalid min samples must fail: {resp}"
    );
    assert_eq!(
        resp["error"]["data"]["cabal_code"].as_str(),
        Some("INVALID_REQUEST"),
        "unexpected cabal_code payload: {resp}"
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_route_consult_adaptive_exploration_selects_undertrained_executor() {
    let root = temp_root("cabal_mcp_stdio_consult_adaptive_explore");
    fs::create_dir_all(&root).expect("mkdir");

    let mut child = Command::new(env!("CARGO_BIN_EXE_cabal-mcp-runtime"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn runtime");

    let mut child_stdin = child.stdin.take().expect("stdin");
    let child_stdout = child.stdout.take().expect("stdout");
    let mut child_stdout = BufReader::new(child_stdout);

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":1,
            "method":"initialize",
            "params":{"protocolVersion":"2025-01-01","capabilities":{},"clientInfo":{"name":"consult-adaptive-explore","version":"1"}}
        }),
    );
    let init = read_content_length_response(&mut child_stdout);
    assert_eq!(init["id"].as_i64(), Some(1));

    let calls = vec![
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"cabal.set_consult_mode","arguments":{"mode":"YOLO"}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"cabal.set_adaptive_router","arguments":{"enabled":true,"confidence_floor":0.95}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"cabal.set_adaptive_exploration_policy","arguments":{"exploration_rate":1.0,"exploration_min_samples":5}}}),
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"cabal.set_consult_routing_rule","arguments":{"consult_type":"performance","executor":"developer"}}}),
        json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"cabal.set_consult_allowed_roles","arguments":{"consult_type":"performance","roles":["developer","perf_engineer"]}}}),
    ];
    for call in calls {
        send_ndjson(&mut child_stdin, &call);
        let resp = read_content_length_response(&mut child_stdout);
        assert!(resp.get("error").is_none(), "setup call failed: {resp}");
    }

    for id in 7..15 {
        send_ndjson(
            &mut child_stdin,
            &json!({
                "jsonrpc":"2.0","id":id,"method":"tools/call",
                "params":{
                    "name":"cabal.record_consult_feedback",
                    "arguments":{
                        "request_id":"rq-dev-mature",
                        "consult_type":"performance",
                        "executor":"developer",
                        "success":true,
                        "latency_ms":120
                    }
                }
            }),
        );
        let resp = read_content_length_response(&mut child_stdout);
        assert!(resp.get("error").is_none(), "feedback call failed: {resp}");
    }

    send_ndjson(
        &mut child_stdin,
        &json!({
            "jsonrpc":"2.0","id":20,"method":"tools/call",
            "params":{
                "name":"cabal.route_consult",
                "arguments":{
                    "question":"optimize simd kernel",
                    "consult_type":"performance",
                    "priority":"high",
                    "request_id":"rq-explore-stdio-1"
                }
            }
        }),
    );
    let routed_raw = read_content_length_response(&mut child_stdout);
    assert!(
        routed_raw.get("error").is_none(),
        "route_consult failed: {routed_raw}"
    );
    let routed = decode_tool_result(&routed_raw);
    assert_eq!(
        routed["dispatch"]["executor"].as_str(),
        Some("perf_engineer")
    );
    assert_eq!(
        routed["routing_decision"]["strategy"].as_str(),
        Some("adaptive_explore")
    );
    assert_eq!(
        routed["routing_decision"]["exploration_rate"].as_f64(),
        Some(1.0)
    );
    assert_eq!(
        routed["routing_decision"]["exploration_min_samples"].as_u64(),
        Some(5)
    );

    drop(child_stdin);
    let _ = child.wait();
    let _ = fs::remove_dir_all(root);
}

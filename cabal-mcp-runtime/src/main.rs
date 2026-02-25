#![recursion_limit = "256"]

use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};
use std::io::{BufReader, stdin, stdout};
use std::path::PathBuf;

use cabal_mcp_runtime::cpu::CpuProfile;
use cabal_mcp_runtime::errors::{classify_error, error_codes_catalog};
use cabal_mcp_runtime::protocol::{read_jsonrpc_message, write_jsonrpc_message};
use cabal_mcp_runtime::runtime::CabalRuntime;

fn main() -> Result<()> {
    let cpu = CpuProfile::detect().context("cpu feature gate failed")?;
    eprintln!(
        "[cabal-mcp-runtime] started; path={:?}; vendor={}",
        cpu.path, cpu.vendor
    );

    let cwd = std::env::current_dir().context("failed to get cwd")?;
    let state_root = resolve_state_root(&cwd);
    let mut runtime = CabalRuntime::load_or_create(&state_root, &cpu)?;
    runtime.validate_cpu_policy(&cpu)?;

    let mut reader = BufReader::new(stdin().lock());
    let mut writer = stdout().lock();

    loop {
        let msg = match read_jsonrpc_message(&mut reader) {
            Ok(Some(v)) => v,
            Ok(None) => break,
            Err(err) => {
                let classified = classify_error("protocol.read", None, &err);
                let response = json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": {
                        "code": classified.rpc_code,
                        "message": classified.message,
                        "data": {
                            "cabal_code": classified.cabal_code,
                            "retryable": classified.retryable,
                            "method": "protocol.read",
                            "tool": Value::Null
                        }
                    }
                });
                write_jsonrpc_message(&mut writer, &response)?;
                continue;
            }
        };
        let Some(id) = msg.get("id").cloned() else {
            continue;
        };
        let method = msg
            .get("method")
            .and_then(|m| m.as_str())
            .unwrap_or_default();
        let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));

        let response = match handle_request(&mut runtime, &cpu, method, params.clone()) {
            Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
            Err(err) => {
                let tool_name = if method == "tools/call" {
                    params.get("name").and_then(|v| v.as_str())
                } else {
                    None
                };
                let classified = classify_error(method, tool_name, &err);
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": classified.rpc_code,
                        "message": classified.message,
                        "data": {
                            "cabal_code": classified.cabal_code,
                            "retryable": classified.retryable,
                            "method": method,
                            "tool": tool_name
                        }
                    }
                })
            }
        };
        runtime.persist()?;
        write_jsonrpc_message(&mut writer, &response)?;
    }
    Ok(())
}

fn resolve_state_root(cwd: &PathBuf) -> PathBuf {
    cwd.clone()
}

fn handle_request(
    runtime: &mut CabalRuntime,
    cpu: &CpuProfile,
    method: &str,
    params: Value,
) -> Result<Value> {
    match method {
        "initialize" => {
            let client_name = params
                .get("clientInfo")
                .and_then(|x| x.get("name"))
                .and_then(|x| x.as_str());
            let client_version = params
                .get("clientInfo")
                .and_then(|x| x.get("version"))
                .and_then(|x| x.as_str());
            let ide = runtime.register_ide_client_session(client_name, client_version)?;
            Ok(json!({
                "protocolVersion": "2025-01-01",
                "serverInfo": {
                    "name": "cabal-mcp-runtime",
                    "version": "0.1.0"
                },
                "capabilities": {
                    "tools": {}
                },
                "cabal": {
                    "ide_profile": ide["active_profile"],
                    "enforce_ide_profile": ide["enforce_ide_profile"]
                }
            }))
        }
        "tools/list" => Ok(json!({
            "tools": tools_catalog()
        })),
        "tools/call" => {
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("tools/call missing name"))?;
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            call_tool(runtime, cpu, name, arguments)
        }
        "notifications/initialized" => Ok(json!({})),
        _ => Err(anyhow!("unsupported method: {method}")),
    }
}

fn call_tool(
    runtime: &mut CabalRuntime,
    cpu: &CpuProfile,
    name: &str,
    arguments: Value,
) -> Result<Value> {
    let result = match name {
        "cabal.get_capabilities" => json!({
            "cpu": cpu,
            "constraints": {
                "min_cpu": "avx2",
                "fast_path": "zen4 + avx512f + avx512vl + fma + bmi2 + sha"
            }
        }),
        "cabal.get_error_codes" => error_codes_catalog(),
        "cabal.validate_error_codes_parity" => {
            let doc_path = arguments
                .get("doc_path")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            runtime.validate_error_codes_parity(doc_path)?
        }
        "cabal.get_state" => runtime.get_state_value(),
        "cabal.get_cpu_policy" => runtime.get_cpu_policy(),
        "cabal.set_cpu_policy" => {
            let require_zen4_fast_path = arguments
                .get("require_zen4_fast_path")
                .and_then(|x| x.as_bool());
            let require_avx512f = arguments.get("require_avx512f").and_then(|x| x.as_bool());
            let require_avx512vl = arguments.get("require_avx512vl").and_then(|x| x.as_bool());
            let require_fma = arguments.get("require_fma").and_then(|x| x.as_bool());
            let require_bmi2 = arguments.get("require_bmi2").and_then(|x| x.as_bool());
            let require_sha = arguments.get("require_sha").and_then(|x| x.as_bool());
            runtime.set_cpu_policy(
                cpu,
                require_zen4_fast_path,
                require_avx512f,
                require_avx512vl,
                require_fma,
                require_bmi2,
                require_sha,
            )?
        }
        "cabal.get_gate_policy" => runtime.get_gate_policy(),
        "cabal.set_gate_policy" => {
            let strict_artifacts = arguments.get("strict_artifacts").and_then(|x| x.as_bool());
            runtime.set_gate_policy(strict_artifacts)?
        }
        "cabal.get_ide_profile_policy" => runtime.get_ide_profile_policy(),
        "cabal.set_ide_profile_policy" => {
            let enforce_ide_profile = arguments
                .get("enforce_ide_profile")
                .and_then(|x| x.as_bool());
            let require_client_info = arguments
                .get("require_client_info")
                .and_then(|x| x.as_bool());
            let allowed_profiles = arguments
                .get("allowed_profiles")
                .map(|v| read_string_array(Some(v)))
                .transpose()?;
            runtime.set_ide_profile_policy(
                enforce_ide_profile,
                require_client_info,
                allowed_profiles,
            )?
        }
        "cabal.get_audit_rotation_policy" => runtime.get_audit_rotation_policy(),
        "cabal.set_audit_rotation_policy" => {
            let enabled = arguments.get("enabled").and_then(|x| x.as_bool());
            let max_bytes = arguments.get("max_bytes").and_then(|x| x.as_u64());
            let max_age_sec = arguments.get("max_age_sec").and_then(|x| x.as_u64());
            let compress = arguments.get("compress").and_then(|x| x.as_bool());
            let keep_last = arguments.get("keep_last").and_then(|x| x.as_u64());
            let archive_dir = arguments
                .get("archive_dir")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            runtime.set_audit_rotation_policy(
                enabled,
                max_bytes,
                max_age_sec,
                compress,
                keep_last,
                archive_dir,
            )?
        }
        "cabal.get_consult_routing" => runtime.get_consult_routing(),
        "cabal.get_cross_rules_status" => runtime.get_cross_rules_status(),
        "cabal.get_consult_guard_policy" => runtime.get_consult_guard_policy(),
        "cabal.get_adaptive_router" => runtime.get_adaptive_router(),
        "cabal.set_consult_mode" => {
            let mode = arguments
                .get("mode")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("mode is required"))?;
            runtime.set_consult_mode(mode)?
        }
        "cabal.set_consult_guard_policy" => {
            let require_cross_rules_ack = arguments
                .get("require_cross_rules_ack")
                .and_then(|x| x.as_bool());
            let required_evidence_ids = arguments
                .get("required_evidence_ids")
                .map(|v| read_string_array(Some(v)))
                .transpose()?;
            runtime.set_consult_guard_policy(require_cross_rules_ack, required_evidence_ids)?
        }
        "cabal.ack_cross_rules" => {
            let agent_ack_path = arguments
                .get("agent_ack_path")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("agent_ack_path is required"))?
                .to_string();
            let subagent_ack_path = arguments
                .get("subagent_ack_path")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("subagent_ack_path is required"))?
                .to_string();
            let enable_consult_guard = arguments
                .get("enable_consult_guard")
                .and_then(|x| x.as_bool());
            runtime.ack_cross_rules(agent_ack_path, subagent_ack_path, enable_consult_guard)?
        }
        "cabal.set_adaptive_router" => {
            let enabled = arguments.get("enabled").and_then(|x| x.as_bool());
            let confidence_floor = arguments.get("confidence_floor").and_then(|x| x.as_f64());
            runtime.set_adaptive_router(enabled, confidence_floor)?
        }
        "cabal.set_adaptive_exploration_policy" => {
            let exploration_rate = arguments.get("exploration_rate").and_then(|x| x.as_f64());
            let exploration_min_samples = arguments
                .get("exploration_min_samples")
                .and_then(|x| x.as_u64());
            runtime.set_adaptive_exploration_policy(exploration_rate, exploration_min_samples)?
        }
        "cabal.set_consult_routing_rule" => {
            let consult_type = arguments
                .get("consult_type")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("consult_type is required"))?
                .to_string();
            let executor = arguments
                .get("executor")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("executor is required"))?
                .to_string();
            runtime.set_consult_routing_rule(consult_type, executor)?
        }
        "cabal.set_consult_priority_timeout" => {
            let priority = arguments
                .get("priority")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("priority is required"))?
                .to_string();
            let timeout_sec = arguments
                .get("timeout_sec")
                .and_then(|x| x.as_u64())
                .ok_or_else(|| anyhow!("timeout_sec is required"))?;
            runtime.set_consult_priority_timeout(priority, timeout_sec)?
        }
        "cabal.set_consult_retry_limit" => {
            let priority = arguments
                .get("priority")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("priority is required"))?
                .to_string();
            let max_retries = arguments
                .get("max_retries")
                .and_then(|x| x.as_u64())
                .ok_or_else(|| anyhow!("max_retries is required"))?;
            runtime.set_consult_retry_limit(priority, max_retries)?
        }
        "cabal.set_consult_escalation_target" => {
            let priority = arguments
                .get("priority")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("priority is required"))?
                .to_string();
            let target = arguments
                .get("target")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("target is required"))?
                .to_string();
            runtime.set_consult_escalation_target(priority, target)?
        }
        "cabal.set_consult_allowed_roles" => {
            let consult_type = arguments
                .get("consult_type")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("consult_type is required"))?
                .to_string();
            let roles = read_string_array(arguments.get("roles"))?;
            runtime.set_consult_allowed_roles(consult_type, roles)?
        }
        "cabal.record_consult_feedback" => {
            let request_id = arguments
                .get("request_id")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            let consult_type = arguments
                .get("consult_type")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("consult_type is required"))?
                .to_string();
            let executor = arguments
                .get("executor")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("executor is required"))?
                .to_string();
            let success = arguments
                .get("success")
                .and_then(|x| x.as_bool())
                .ok_or_else(|| anyhow!("success is required"))?;
            let latency_ms = arguments.get("latency_ms").and_then(|x| x.as_u64());
            runtime.record_consult_feedback(
                request_id,
                consult_type,
                executor,
                success,
                latency_ms,
            )?
        }
        "cabal.apply_policy_bundle" => {
            let expected_revision = arguments
                .get("expected_revision")
                .and_then(|x| x.as_u64())
                .ok_or_else(|| anyhow!("expected_revision is required"))?;
            let version = arguments
                .get("version")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("version is required"))?
                .to_string();
            let rules = read_string_array(arguments.get("rules"))?;
            let signature = arguments
                .get("signature")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            let key_id = arguments
                .get("key_id")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            let nonce = arguments
                .get("nonce")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            let forbidden_tokens = read_string_array(arguments.get("forbidden_tokens"))?;
            runtime.apply_policy(
                cpu,
                expected_revision,
                version,
                rules,
                signature,
                key_id,
                nonce,
                forbidden_tokens,
            )?
        }
        "cabal.set_policy_security" => {
            let require_signed_policy = arguments
                .get("require_signed_policy")
                .and_then(|x| x.as_bool());
            runtime.set_policy_security(require_signed_policy)?
        }
        "cabal.list_policy_signing_keys" => runtime.list_policy_signing_keys(),
        "cabal.upsert_policy_signing_key" => {
            let key_id = arguments
                .get("key_id")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("key_id is required"))?
                .to_string();
            let key_env = arguments
                .get("key_env")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("key_env is required"))?
                .to_string();
            let not_before_unix = arguments.get("not_before_unix").and_then(|x| x.as_u64());
            let not_after_unix = arguments.get("not_after_unix").and_then(|x| x.as_u64());
            let set_active = arguments.get("set_active").and_then(|x| x.as_bool());
            runtime.upsert_policy_signing_key(
                key_id,
                key_env,
                not_before_unix,
                not_after_unix,
                set_active,
            )?
        }
        "cabal.set_active_policy_signing_key" => {
            let key_id = arguments
                .get("key_id")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("key_id is required"))?
                .to_string();
            runtime.set_active_policy_signing_key(key_id)?
        }
        "cabal.revoke_policy_signing_key" => {
            let key_id = arguments
                .get("key_id")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("key_id is required"))?
                .to_string();
            runtime.revoke_policy_signing_key(key_id)?
        }
        "cabal.guard_action" => {
            let agent = arguments
                .get("agent")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("agent is required"))?;
            let action = arguments
                .get("action")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("action is required"))?;
            runtime.guard_action(agent, action)?
        }
        "cabal.get_proxy_operation_policy" => runtime.get_proxy_operation_policy(),
        "cabal.set_proxy_operation_policy" => {
            let category = arguments
                .get("category")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("category is required"))?
                .to_string();
            let allowed_operations = arguments
                .get("allowed_operations")
                .map(|v| read_string_array(Some(v)))
                .transpose()?;
            let denied_operations = arguments
                .get("denied_operations")
                .map(|v| read_string_array(Some(v)))
                .transpose()?;
            runtime.set_proxy_operation_policy(category, allowed_operations, denied_operations)?
        }
        "cabal.set_proxy_policy" => {
            let deny_by_default = arguments.get("deny_by_default").and_then(|x| x.as_bool());
            let category = arguments
                .get("category")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            let allow_prefixes = arguments
                .get("allow_prefixes")
                .map(|v| read_string_array(Some(v)))
                .transpose()?;
            runtime.set_proxy_policy(deny_by_default, category, allow_prefixes)?
        }
        "cabal.get_proxy_log" => {
            let limit = arguments
                .get("limit")
                .and_then(|x| x.as_u64())
                .map(|x| x as usize);
            runtime.get_proxy_log(limit)?
        }
        "cabal.get_audit_log" => {
            let limit = arguments
                .get("limit")
                .and_then(|x| x.as_u64())
                .map(|x| x as usize);
            runtime.get_audit_log(limit)?
        }
        "cabal.query_audit_log" => {
            let kind = arguments
                .get("kind")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            let phase = arguments
                .get("phase")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            let policy_revision = arguments.get("policy_revision").and_then(|x| x.as_u64());
            let request_id = arguments
                .get("request_id")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            let from_ts_unix = arguments.get("from_ts_unix").and_then(|x| x.as_u64());
            let to_ts_unix = arguments.get("to_ts_unix").and_then(|x| x.as_u64());
            let limit = arguments
                .get("limit")
                .and_then(|x| x.as_u64())
                .map(|x| x as usize);
            runtime.query_audit_log(
                kind,
                phase,
                policy_revision,
                request_id,
                from_ts_unix,
                to_ts_unix,
                limit,
            )?
        }
        "cabal.export_audit_log" => {
            let out_path = arguments
                .get("out_path")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("out_path is required"))?
                .to_string();
            let kind = arguments
                .get("kind")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            let phase = arguments
                .get("phase")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            let policy_revision = arguments.get("policy_revision").and_then(|x| x.as_u64());
            let request_id = arguments
                .get("request_id")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            let from_ts_unix = arguments.get("from_ts_unix").and_then(|x| x.as_u64());
            let to_ts_unix = arguments.get("to_ts_unix").and_then(|x| x.as_u64());
            let limit = arguments
                .get("limit")
                .and_then(|x| x.as_u64())
                .map(|x| x as usize);
            runtime.export_audit_log(
                out_path,
                kind,
                phase,
                policy_revision,
                request_id,
                from_ts_unix,
                to_ts_unix,
                limit,
            )?
        }
        "cabal.replay_audit_state" => {
            let upto_event_id = arguments
                .get("upto_event_id")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            let upto_ts_unix = arguments.get("upto_ts_unix").and_then(|x| x.as_u64());
            runtime.replay_audit_state(upto_event_id, upto_ts_unix)?
        }
        "cabal.rotate_audit_log" => {
            let archive_dir = arguments
                .get("archive_dir")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            let compress = arguments.get("compress").and_then(|x| x.as_bool());
            runtime.rotate_audit_log(archive_dir, compress)?
        }
        "cabal.verify_audit_archive" => {
            let archive_path = arguments
                .get("archive_path")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("archive_path is required"))?
                .to_string();
            let signature_path = arguments
                .get("signature_path")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            runtime.verify_audit_archive(archive_path, signature_path)?
        }
        "cabal.prune_audit_archives" => {
            let archive_dir = arguments
                .get("archive_dir")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            let keep_last = arguments.get("keep_last").and_then(|x| x.as_u64());
            runtime.prune_audit_archives(archive_dir, keep_last)?
        }
        "cabal.audit_health_check" => {
            let archive_dir = arguments
                .get("archive_dir")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            let verify_last = arguments.get("verify_last").and_then(|x| x.as_u64());
            runtime.audit_health_check(archive_dir, verify_last)?
        }
        "cabal.proxy_request" => {
            let category = arguments
                .get("category")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("category is required"))?;
            let operation = arguments
                .get("operation")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("operation is required"))?;
            let target = arguments
                .get("target")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("target is required"))?;
            runtime.proxy_request(category, operation, target)?
        }
        "cabal.proxy_execute" => {
            let category = arguments
                .get("category")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("category is required"))?;
            let operation = arguments
                .get("operation")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("operation is required"))?;
            let target = arguments
                .get("target")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("target is required"))?;
            let payload = arguments
                .get("payload")
                .cloned()
                .unwrap_or_else(|| json!({}));
            runtime.proxy_execute(cpu, category, operation, target, payload)?
        }
        "cabal.transition_phase" => {
            let target = arguments
                .get("target_phase")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("target_phase is required"))?;
            runtime.transition_phase(target)?
        }
        "cabal.transition_phase_strict" => {
            let target = arguments
                .get("target_phase")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("target_phase is required"))?;
            runtime.transition_phase_strict(target)?
        }
        "cabal.gate_check" => {
            let kind = arguments
                .get("kind")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("kind is required"))?;
            let phase = arguments
                .get("phase")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("phase is required"))?;
            runtime.gate_check(kind, phase)?
        }
        "cabal.route_consult" => {
            let question = arguments
                .get("question")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("question is required"))?;
            let consult_type = arguments.get("consult_type").and_then(|x| x.as_str());
            let priority = arguments.get("priority").and_then(|x| x.as_str());
            let preferred_role = arguments.get("preferred_role").and_then(|x| x.as_str());
            let request_id = arguments.get("request_id").and_then(|x| x.as_str());
            runtime.route_consult(question, consult_type, priority, preferred_role, request_id)?
        }
        "cabal.register_evidence" => {
            let id = arguments
                .get("id")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("id is required"))?
                .to_string();
            let path = arguments
                .get("path")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("path is required"))?
                .to_string();
            runtime.register_evidence(id, path)?
        }
        "cabal.record_event" => {
            let kind = arguments
                .get("kind")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("kind is required"))?
                .to_string();
            let payload = arguments
                .get("payload")
                .cloned()
                .unwrap_or_else(|| json!({}));
            runtime.record_event(cpu, kind, payload)?
        }
        _ => return Err(anyhow!("unknown tool: {name}")),
    };

    Ok(json!({
        "content": [{"type": "text", "text": serde_json::to_string_pretty(&result)?}],
        "isError": false
    }))
}

fn read_string_array(value: Option<&Value>) -> Result<Vec<String>> {
    let Some(arr) = value.and_then(|v| v.as_array()) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        if let Some(s) = item.as_str() {
            out.push(s.to_string());
        }
    }
    Ok(out)
}

fn tools_catalog() -> Value {
    json!([
        {
            "name": "cabal.get_capabilities",
            "description": "Возвращает CPU-профиль и активный SIMD execution path.",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "cabal.get_error_codes",
            "description": "Возвращает машинную таксономию кодов ошибок Cabal Runtime.",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "cabal.validate_error_codes_parity",
            "description": "Проверяет паритет runtime error codes с CABAL_ERROR_CODES.md.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "doc_path": {"type": "string"}
                }
            }
        },
        {
            "name": "cabal.get_state",
            "description": "Текущее состояние Cabal runtime: фаза, режим CONSULT, policy hash.",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "cabal.get_cpu_policy",
            "description": "Возвращает CPU policy runtime (например, требование Zen4 fast-path).",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "cabal.set_cpu_policy",
            "description": "Настраивает CPU policy runtime.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "require_zen4_fast_path": {"type": "boolean"},
                    "require_avx512f": {"type": "boolean"},
                    "require_avx512vl": {"type": "boolean"},
                    "require_fma": {"type": "boolean"},
                    "require_bmi2": {"type": "boolean"},
                    "require_sha": {"type": "boolean"}
                }
            }
        },
        {
            "name": "cabal.get_gate_policy",
            "description": "Возвращает policy strict gate checks для phase artifacts.",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "cabal.set_gate_policy",
            "description": "Настраивает strict gate artifacts mode для entry/exit phase checks.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "strict_artifacts": {"type": "boolean"}
                }
            }
        },
        {
            "name": "cabal.get_ide_profile_policy",
            "description": "Возвращает active IDE client profile и policy enforce/allowlist.",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "cabal.set_ide_profile_policy",
            "description": "Настраивает enforcement IDE profile allowlist и require_client_info для initialize.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "enforce_ide_profile": {"type": "boolean"},
                    "require_client_info": {"type": "boolean"},
                    "allowed_profiles": {
                        "type": "array",
                        "items": {"type": "string"},
                        "minItems": 1
                    }
                }
            }
        },
        {
            "name": "cabal.get_audit_rotation_policy",
            "description": "Возвращает policy авто-ротации аудита (size/time/compress/retention).",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "cabal.set_audit_rotation_policy",
            "description": "Настраивает auto-rotation policy для audit.jsonl.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "enabled": {"type": "boolean"},
                    "max_bytes": {"type": "integer", "minimum": 1},
                    "max_age_sec": {"type": "integer", "minimum": 1},
                    "compress": {"type": "boolean"},
                    "keep_last": {"type": "integer", "minimum": 1},
                    "archive_dir": {"type": "string"}
                }
            }
        },
        {
            "name": "cabal.get_consult_routing",
            "description": "Возвращает policy-driven routing map и SLA timeout policy для CONSULT.",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "cabal.get_cross_rules_status",
            "description": "Возвращает статус обязательных cross-rules evidence для entry-gate и CONSULT guard.",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "cabal.get_consult_guard_policy",
            "description": "Возвращает guard policy для CONSULT (require_cross_rules_ack + required_evidence_ids).",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "cabal.get_adaptive_router",
            "description": "Возвращает состояние адаптивного (эмерджентного) роутера и телеметрию исполнителей.",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "cabal.set_consult_mode",
            "description": "Устанавливает режим CONSULT: USER_TRACKING или YOLO.",
            "inputSchema": {
                "type": "object",
                "required": ["mode"],
                "properties": { "mode": {"type": "string"} }
            }
        },
        {
            "name": "cabal.set_consult_guard_policy",
            "description": "Настраивает guard policy для CONSULT (требовать cross-rules evidence перед route_consult).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "require_cross_rules_ack": {"type": "boolean"},
                    "required_evidence_ids": {"type": "array", "items": {"type": "string"}, "minItems": 1}
                }
            }
        },
        {
            "name": "cabal.ack_cross_rules",
            "description": "Атомарно регистрирует cross-rules ack evidence и (опционально) включает CONSULT guard.",
            "inputSchema": {
                "type": "object",
                "required": ["agent_ack_path", "subagent_ack_path"],
                "properties": {
                    "agent_ack_path": {"type": "string"},
                    "subagent_ack_path": {"type": "string"},
                    "enable_consult_guard": {"type": "boolean"}
                }
            }
        },
        {
            "name": "cabal.set_adaptive_router",
            "description": "Включает/настраивает адаптивный выбор исполнителя по телеметрии.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "enabled": {"type": "boolean"},
                    "confidence_floor": {"type": "number", "minimum": 0, "maximum": 1}
                }
            }
        },
        {
            "name": "cabal.set_adaptive_exploration_policy",
            "description": "Настраивает exploration режим для adaptive router (rate/min_samples).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "exploration_rate": {"type": "number", "minimum": 0, "maximum": 1},
                    "exploration_min_samples": {"type": "integer", "minimum": 1}
                }
            }
        },
        {
            "name": "cabal.set_consult_routing_rule",
            "description": "Устанавливает правило маршрутизации CONSULT type -> executor.",
            "inputSchema": {
                "type": "object",
                "required": ["consult_type", "executor"],
                "properties": {
                    "consult_type": {"type": "string"},
                    "executor": {"type": "string"}
                }
            }
        },
        {
            "name": "cabal.set_consult_priority_timeout",
            "description": "Устанавливает SLA timeout (sec) для приоритета CONSULT.",
            "inputSchema": {
                "type": "object",
                "required": ["priority", "timeout_sec"],
                "properties": {
                    "priority": {"type": "string", "enum": ["low", "normal", "high", "critical"]},
                    "timeout_sec": {"type": "integer", "minimum": 1}
                }
            }
        },
        {
            "name": "cabal.set_consult_retry_limit",
            "description": "Устанавливает max_retries для приоритета CONSULT.",
            "inputSchema": {
                "type": "object",
                "required": ["priority", "max_retries"],
                "properties": {
                    "priority": {"type": "string", "enum": ["low", "normal", "high", "critical"]},
                    "max_retries": {"type": "integer", "minimum": 0, "maximum": 10}
                }
            }
        },
        {
            "name": "cabal.set_consult_escalation_target",
            "description": "Устанавливает target эскалации для приоритета CONSULT.",
            "inputSchema": {
                "type": "object",
                "required": ["priority", "target"],
                "properties": {
                    "priority": {"type": "string", "enum": ["low", "normal", "high", "critical"]},
                    "target": {"type": "string", "enum": ["none", "user", "orchestrator", "architect", "security_reviewer"]}
                }
            }
        },
        {
            "name": "cabal.set_consult_allowed_roles",
            "description": "Устанавливает allowlist ролей для consult_type.",
            "inputSchema": {
                "type": "object",
                "required": ["consult_type", "roles"],
                "properties": {
                    "consult_type": {"type": "string"},
                    "roles": {"type": "array", "items": {"type": "string"}, "minItems": 1}
                }
            }
        },
        {
            "name": "cabal.record_consult_feedback",
            "description": "Записывает outcome/latency телеметрию для адаптивного роутера CONSULT.",
            "inputSchema": {
                "type": "object",
                "required": ["consult_type", "executor", "success"],
                "properties": {
                    "request_id": {"type": "string"},
                    "consult_type": {"type": "string"},
                    "executor": {"type": "string"},
                    "success": {"type": "boolean"},
                    "latency_ms": {"type": "integer", "minimum": 1}
                }
            }
        },
        {
            "name": "cabal.apply_policy_bundle",
            "description": "Обновляет policy bundle в runtime-реестре (revision-locked) с пересчётом SIMD hash.",
            "inputSchema": {
                "type": "object",
                "required": ["expected_revision", "version", "rules"],
                "properties": {
                    "expected_revision": {"type": "integer", "minimum": 0},
                    "version": {"type": "string"},
                    "rules": {"type": "array", "items": {"type": "string"}},
                    "signature": {"type": "string"},
                    "key_id": {"type": "string"},
                    "nonce": {"type": "string"},
                    "forbidden_tokens": {"type": "array", "items": {"type": "string"}}
                }
            }
        },
        {
            "name": "cabal.set_policy_security",
            "description": "Управляет режимом обязательной подписи policy bundle.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "require_signed_policy": {"type": "boolean"}
                }
            }
        },
        {
            "name": "cabal.list_policy_signing_keys",
            "description": "Возвращает registry ключей подписи policy (key-id, env, revoke/expiry).",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "cabal.upsert_policy_signing_key",
            "description": "Добавляет/обновляет ключ подписи policy с key-id и сроком действия.",
            "inputSchema": {
                "type": "object",
                "required": ["key_id", "key_env"],
                "properties": {
                    "key_id": {"type": "string"},
                    "key_env": {"type": "string"},
                    "not_before_unix": {"type": "integer", "minimum": 0},
                    "not_after_unix": {"type": "integer", "minimum": 0},
                    "set_active": {"type": "boolean"}
                }
            }
        },
        {
            "name": "cabal.set_active_policy_signing_key",
            "description": "Переключает активный key-id для подписи policy.",
            "inputSchema": {
                "type": "object",
                "required": ["key_id"],
                "properties": {"key_id": {"type": "string"}}
            }
        },
        {
            "name": "cabal.revoke_policy_signing_key",
            "description": "Отзывает key-id из policy signing registry.",
            "inputSchema": {
                "type": "object",
                "required": ["key_id"],
                "properties": {"key_id": {"type": "string"}}
            }
        },
        {
            "name": "cabal.guard_action",
            "description": "Проверяет действие агента на соответствие policy bundle.",
            "inputSchema": {
                "type": "object",
                "required": ["agent", "action"],
                "properties": {
                    "agent": {"type": "string"},
                    "action": {"type": "string"}
                }
            }
        },
        {
            "name": "cabal.get_proxy_operation_policy",
            "description": "Возвращает policy allow/deny операций для категорий Tool Proxy.",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "cabal.set_proxy_operation_policy",
            "description": "Настраивает allow/deny список операций для категории Tool Proxy.",
            "inputSchema": {
                "type": "object",
                "required": ["category"],
                "properties": {
                    "category": {"type": "string"},
                    "allowed_operations": {"type": "array", "items": {"type": "string"}},
                    "denied_operations": {"type": "array", "items": {"type": "string"}}
                }
            }
        },
        {
            "name": "cabal.set_proxy_policy",
            "description": "Обновляет политику Tool Proxy (deny_by_default и allowlist).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "deny_by_default": {"type": "boolean"},
                    "category": {"type": "string"},
                    "allow_prefixes": {"type": "array", "items": {"type": "string"}}
                }
            }
        },
        {
            "name": "cabal.get_proxy_log",
            "description": "Возвращает последние записи трассировки Tool Proxy.",
            "inputSchema": {
                "type": "object",
                "properties": { "limit": {"type": "integer", "minimum": 1} }
            }
        },
        {
            "name": "cabal.get_audit_log",
            "description": "Возвращает хвост append-only аудита runtime (audit.jsonl).",
            "inputSchema": {
                "type": "object",
                "properties": { "limit": {"type": "integer", "minimum": 1} }
            }
        },
        {
            "name": "cabal.query_audit_log",
            "description": "Фильтрация audit.jsonl по kind/phase/revision/request_id/time.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "kind": {"type": "string"},
                    "phase": {"type": "string"},
                    "policy_revision": {"type": "integer", "minimum": 0},
                    "request_id": {"type": "string"},
                    "from_ts_unix": {"type": "integer", "minimum": 0},
                    "to_ts_unix": {"type": "integer", "minimum": 0},
                    "limit": {"type": "integer", "minimum": 1}
                }
            }
        },
        {
            "name": "cabal.export_audit_log",
            "description": "Экспортирует отфильтрованный аудит в repo-relative jsonl файл.",
            "inputSchema": {
                "type": "object",
                "required": ["out_path"],
                "properties": {
                    "out_path": {"type": "string"},
                    "kind": {"type": "string"},
                    "phase": {"type": "string"},
                    "policy_revision": {"type": "integer", "minimum": 0},
                    "request_id": {"type": "string"},
                    "from_ts_unix": {"type": "integer", "minimum": 0},
                    "to_ts_unix": {"type": "integer", "minimum": 0},
                    "limit": {"type": "integer", "minimum": 1}
                }
            }
        },
        {
            "name": "cabal.replay_audit_state",
            "description": "Восстанавливает snapshot состояния из audit trail до event_id/time.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "upto_event_id": {"type": "string"},
                    "upto_ts_unix": {"type": "integer", "minimum": 0}
                }
            }
        },
        {
            "name": "cabal.rotate_audit_log",
            "description": "Ротирует активный audit.jsonl в архив с sha256 sidecar (опционально gzip).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "archive_dir": {"type": "string"},
                    "compress": {"type": "boolean"}
                }
            }
        },
        {
            "name": "cabal.verify_audit_archive",
            "description": "Проверяет sha256 подпись audit-архива (.jsonl/.jsonl.gz) по sidecar.",
            "inputSchema": {
                "type": "object",
                "required": ["archive_path"],
                "properties": {
                    "archive_path": {"type": "string"},
                    "signature_path": {"type": "string"}
                }
            }
        },
        {
            "name": "cabal.prune_audit_archives",
            "description": "Удаляет старые audit-архивы, оставляя только keep_last последних ротаций.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "archive_dir": {"type": "string"},
                    "keep_last": {"type": "integer", "minimum": 1}
                }
            }
        },
        {
            "name": "cabal.audit_health_check",
            "description": "Агрегированная проверка audit log + архивов (verify последних N архивов).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "archive_dir": {"type": "string"},
                    "verify_last": {"type": "integer", "minimum": 1}
                }
            }
        },
        {
            "name": "cabal.proxy_request",
            "description": "Tool Proxy проверка (deny-by-default) для fs/shell/network запросов.",
            "inputSchema": {
                "type": "object",
                "required": ["category", "operation", "target"],
                "properties": {
                    "category": {"type": "string"},
                    "operation": {"type": "string"},
                    "target": {"type": "string"}
                }
            }
        },
        {
            "name": "cabal.proxy_execute",
            "description": "Tool Proxy исполнение fs/shell/network с enforce и trace.",
            "inputSchema": {
                "type": "object",
                "required": ["category", "operation", "target"],
                "properties": {
                    "category": {"type": "string"},
                    "operation": {"type": "string"},
                    "target": {"type": "string"},
                    "payload": {}
                }
            }
        },
        {
            "name": "cabal.transition_phase",
            "description": "Переводит фазу в следующий разрешённый этап протокола.",
            "inputSchema": {
                "type": "object",
                "required": ["target_phase"],
                "properties": { "target_phase": {"type": "string"} }
            }
        },
        {
            "name": "cabal.transition_phase_strict",
            "description": "Переводит фазу только после exit+entry gate checks.",
            "inputSchema": {
                "type": "object",
                "required": ["target_phase"],
                "properties": { "target_phase": {"type": "string"} }
            }
        },
        {
            "name": "cabal.gate_check",
            "description": "Возвращает machine-readable gate report для entry/exit.",
            "inputSchema": {
                "type": "object",
                "required": ["kind", "phase"],
                "properties": {
                    "kind": {"type": "string", "enum": ["entry", "exit"]},
                    "phase": {"type": "string"}
                }
            }
        },
        {
            "name": "cabal.route_consult",
            "description": "Маршрутизирует CONSULT согласно активному режиму.",
            "inputSchema": {
                "type": "object",
                "required": ["question"],
                "properties": {
                    "question": {"type": "string"},
                    "consult_type": {"type": "string"},
                    "priority": {"type": "string", "enum": ["low", "normal", "high", "critical"]},
                    "preferred_role": {"type": "string"},
                    "request_id": {"type": "string"}
                }
            }
        },
        {
            "name": "cabal.register_evidence",
            "description": "Регистрирует evidence-артефакт в runtime state.",
            "inputSchema": {
                "type": "object",
                "required": ["id", "path"],
                "properties": {
                    "id": {"type": "string"},
                    "path": {"type": "string"}
                }
            }
        },
        {
            "name": "cabal.record_event",
            "description": "Пишет событие в audit log с SIMD digest.",
            "inputSchema": {
                "type": "object",
                "required": ["kind"],
                "properties": {
                    "kind": {"type": "string"},
                    "payload": {}
                }
            }
        }
    ])
}

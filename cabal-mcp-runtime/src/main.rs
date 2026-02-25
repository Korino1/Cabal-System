#![recursion_limit = "256"]

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::borrow::Cow;
use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::{BufReader, Write, stdin, stdout};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use cabal_mcp_runtime::cpu::CpuProfile;
use cabal_mcp_runtime::errors::{classify_error, error_codes_catalog};
use cabal_mcp_runtime::protocol::{
    MessageFormat, read_jsonrpc_message, write_jsonrpc_message, write_jsonrpc_message_ndjson,
};
use cabal_mcp_runtime::runtime::CabalRuntime;

const DEFAULT_PROTOCOL_VERSION: &str = "2025-01-01";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct StartupOptions {
    show_help: bool,
    strict_artifacts: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResponseWireMode {
    Framed,
    Ndjson,
    Mirror,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompatAliasProfile {
    None,
    Core,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolNameFormat {
    Canonical,
    RooCompact,
}

fn main() -> Result<()> {
    trace_line(&format!(
        "process.start argv={:?}",
        std::env::args().collect::<Vec<_>>()
    ));
    let startup = parse_startup_options(std::env::args().skip(1))?;
    if startup.show_help {
        print!("{}", startup_help_text());
        return Ok(());
    }

    let cpu = CpuProfile::detect().context("cpu feature gate failed")?;
    if startup_logging_enabled() {
        eprintln!(
            "[cabal-mcp-runtime] started; path={:?}; vendor={}",
            cpu.path, cpu.vendor
        );
    }

    let cwd = std::env::current_dir().context("failed to get cwd")?;
    trace_line(&format!("process.cwd {}", cwd.display()));
    let state_root = resolve_state_root(&cwd);
    let mut runtime = CabalRuntime::load_or_create(&state_root, &cpu)?;
    if let Some(strict) = startup.strict_artifacts {
        runtime.set_gate_policy(Some(strict))?;
        runtime.persist()?;
        if startup_logging_enabled() {
            eprintln!(
                "[cabal-mcp-runtime] startup flag applied: strict_artifacts={}",
                strict
            );
        }
    }
    runtime.validate_cpu_policy(&cpu)?;

    let mut reader = BufReader::new(stdin().lock());
    let mut writer = stdout().lock();
    let response_mode = response_wire_mode_from_env();
    trace_line(&format!("response.mode {:?}", response_mode));

    loop {
        let incoming = match read_jsonrpc_message(&mut reader) {
            Ok(Some(v)) => v,
            Ok(None) => break,
            Err(err) => {
                trace_line(&format!("protocol.read.error {}", err));
                let response = build_error_response("protocol.read", None, Value::Null, &err);
                write_response_for_mode(&mut writer, &response, None, response_mode)?;
                trace_line("protocol.read.error.response_written");
                continue;
            }
        };
        trace_line(&format!(
            "protocol.msg kind={} format={:?}",
            if incoming.value.is_array() {
                "batch"
            } else if incoming.value.is_object() {
                "object"
            } else {
                "other"
            },
            incoming.format
        ));
        let maybe_response = dispatch_jsonrpc_message(&mut runtime, &cpu, incoming.value)?;
        runtime.persist()?;
        if let Some(response) = maybe_response {
            write_response_for_mode(&mut writer, &response, Some(incoming.format), response_mode)?;
            trace_line("protocol.response.written");
        }
    }
    trace_line("process.exit.ok");
    Ok(())
}

fn response_wire_mode_from_env() -> ResponseWireMode {
    match std::env::var("CABAL_MCP_RESPONSE_MODE")
        .ok()
        .map(|v| v.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("ndjson") => ResponseWireMode::Ndjson,
        Some("mirror") => ResponseWireMode::Mirror,
        _ => ResponseWireMode::Framed,
    }
}

fn compat_alias_profile_from_env() -> CompatAliasProfile {
    match std::env::var("CABAL_MCP_COMPAT_ALIAS_PROFILE")
        .ok()
        .map(|v| v.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("none" | "off" | "0") => CompatAliasProfile::None,
        Some("full" | "all") => CompatAliasProfile::Full,
        _ => CompatAliasProfile::Core,
    }
}

fn tool_name_format_from_env() -> ToolNameFormat {
    match std::env::var("CABAL_MCP_TOOL_NAME_FORMAT")
        .ok()
        .map(|v| v.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("roo" | "compact" | "cabalcompact" | "nodot") => ToolNameFormat::RooCompact,
        _ => ToolNameFormat::Canonical,
    }
}

fn write_response_for_mode<W: Write>(
    writer: &mut W,
    response: &Value,
    request_format: Option<MessageFormat>,
    mode: ResponseWireMode,
) -> Result<()> {
    let emit_ndjson = match mode {
        ResponseWireMode::Framed => false,
        ResponseWireMode::Ndjson => true,
        ResponseWireMode::Mirror => matches!(request_format, Some(MessageFormat::Ndjson)),
    };
    if emit_ndjson {
        write_jsonrpc_message_ndjson(writer, response)
    } else {
        write_jsonrpc_message(writer, response)
    }
}

fn dispatch_jsonrpc_message(
    runtime: &mut CabalRuntime,
    cpu: &CpuProfile,
    msg: Value,
) -> Result<Option<Value>> {
    match msg {
        Value::Object(_) => dispatch_single_message(runtime, cpu, msg),
        Value::Array(items) => dispatch_batch_messages(runtime, cpu, items),
        _ => {
            let err = anyhow!("invalid request payload: expected object or batch array");
            Ok(Some(build_error_response(
                "protocol.dispatch",
                None,
                Value::Null,
                &err,
            )))
        }
    }
}

fn dispatch_batch_messages(
    runtime: &mut CabalRuntime,
    cpu: &CpuProfile,
    items: Vec<Value>,
) -> Result<Option<Value>> {
    trace_line(&format!("protocol.batch size={}", items.len()));
    if items.is_empty() {
        let err = anyhow!("invalid request payload: empty batch is not allowed");
        return Ok(Some(build_error_response(
            "protocol.dispatch",
            None,
            Value::Null,
            &err,
        )));
    }

    let mut responses = Vec::new();
    for item in items {
        if let Some(response) = dispatch_single_message(runtime, cpu, item)? {
            responses.push(response);
        }
    }

    if responses.is_empty() {
        Ok(None)
    } else {
        Ok(Some(Value::Array(responses)))
    }
}

fn dispatch_single_message(
    runtime: &mut CabalRuntime,
    cpu: &CpuProfile,
    msg: Value,
) -> Result<Option<Value>> {
    let Some(obj) = msg.as_object() else {
        let err = anyhow!("invalid request payload: expected object");
        return Ok(Some(build_error_response(
            "protocol.dispatch",
            None,
            Value::Null,
            &err,
        )));
    };

    let method = obj
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or_default();
    let params = obj.get("params").cloned().unwrap_or_else(|| json!({}));
    let id = obj.get("id").cloned();
    trace_line(&format!(
        "request method={} id_present={} id_is_null={}",
        method,
        id.is_some(),
        matches!(id, Some(Value::Null))
    ));

    if id.is_none() {
        // Compatibility path: some clients may accidentally send initialize as a
        // notification (without id) but still wait for initialize result.
        if method == "initialize" {
            let result = handle_request(runtime, cpu, method, params)?;
            return Ok(Some(
                json!({"jsonrpc": "2.0", "id": Value::Null, "result": result}),
            ));
        }
        let _ = handle_notification(runtime, cpu, method, params);
        return Ok(None);
    }
    let id = id.expect("id checked");

    let response = match handle_request(runtime, cpu, method, params.clone()) {
        Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
        Err(err) => {
            let tool_name = if method == "tools/call" {
                params.get("name").and_then(|v| v.as_str())
            } else {
                None
            };
            build_error_response(method, tool_name, id, &err)
        }
    };
    trace_line(&format!("request.done method={}", method));
    Ok(Some(response))
}

fn handle_notification(
    runtime: &mut CabalRuntime,
    cpu: &CpuProfile,
    method: &str,
    params: Value,
) -> Result<()> {
    match method {
        "notifications/initialized" | "$/cancelRequest" | "$/progress" => Ok(()),
        "logging/setLevel" => Ok(()),
        // Some hosts send ping as notification even though it is usually a request.
        "ping" => Ok(()),
        // Process known request-like methods if they accidentally arrive as notifications.
        _ if method == "initialize"
            || method == "tools/list"
            || method == "tools/call"
            || method == "resources/list"
            || method == "resources/templates/list"
            || method == "prompts/list" =>
        {
            let _ = handle_request(runtime, cpu, method, params)?;
            Ok(())
        }
        _ => Ok(()),
    }
}

fn build_error_response(
    method: &str,
    tool_name: Option<&str>,
    id: Value,
    err: &anyhow::Error,
) -> Value {
    let classified = classify_error(method, tool_name, err);
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

fn trace_line(msg: &str) {
    let Ok(path) = std::env::var("CABAL_MCP_TRACE_FILE") else {
        return;
    };
    if path.trim().is_empty() {
        return;
    }
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "[{ts:.3}] {msg}");
    }
}

fn startup_logging_enabled() -> bool {
    matches!(
        std::env::var("CABAL_MCP_STARTUP_LOG")
            .ok()
            .map(|v| v.trim().to_ascii_lowercase())
            .as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

fn resolve_state_root(cwd: &PathBuf) -> PathBuf {
    cwd.clone()
}

fn startup_help_text() -> &'static str {
    concat!(
        "Cabal MCP Runtime\n\n",
        "Usage:\n",
        "  cabal-mcp-runtime [OPTIONS]\n\n",
        "Options:\n",
        "  -h, --help               Show this help and exit\n",
        "  --strict-artifacts       Enable strict file-based gate artifacts before MCP loop\n",
        "  --no-strict-artifacts    Disable strict file-based gate artifacts before MCP loop\n",
        "  --strict-artifacts=<v>   Explicit value: true|false|1|0|yes|no|on|off\n",
    )
}

fn parse_startup_options<I, S>(args: I) -> Result<StartupOptions>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut out = StartupOptions::default();
    let raw: Vec<String> = args.into_iter().map(Into::into).collect();
    let mut idx = 0usize;
    while idx < raw.len() {
        let arg = raw[idx].as_str();
        match arg {
            "-h" | "--help" => {
                out.show_help = true;
            }
            "--strict-artifacts" => {
                if idx + 1 < raw.len() {
                    let next = raw[idx + 1].as_str();
                    if let Some(v) = parse_bool_flag_value(next) {
                        out.strict_artifacts = Some(v);
                        idx += 1;
                    } else {
                        out.strict_artifacts = Some(true);
                    }
                } else {
                    out.strict_artifacts = Some(true);
                }
            }
            "--no-strict-artifacts" => {
                out.strict_artifacts = Some(false);
            }
            _ => {
                if let Some(value) = arg.strip_prefix("--strict-artifacts=") {
                    let parsed = parse_bool_flag_value(value).ok_or_else(|| {
                        anyhow!(
                            "invalid value for --strict-artifacts: {value} (expected true|false|1|0|yes|no|on|off)"
                        )
                    })?;
                    out.strict_artifacts = Some(parsed);
                } else {
                    trace_line(&format!("startup.unknown_flag.ignored {arg}"));
                }
            }
        }
        idx += 1;
    }
    Ok(out)
}

fn parse_bool_flag_value(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn handle_request(
    runtime: &mut CabalRuntime,
    cpu: &CpuProfile,
    method: &str,
    params: Value,
) -> Result<Value> {
    match method {
        "initialize" => {
            let negotiated_protocol = params
                .get("protocolVersion")
                .and_then(|x| x.as_str())
                .filter(|x| !x.trim().is_empty())
                .unwrap_or(DEFAULT_PROTOCOL_VERSION);
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
                "protocolVersion": negotiated_protocol,
                "serverInfo": {
                    "name": "cabal-mcp-runtime",
                    "version": "0.1.0"
                },
                "capabilities": {
                    "tools": {
                        "listChanged": false
                    }
                },
                "cabal": {
                    "ide_profile": ide["active_profile"],
                    "enforce_ide_profile": ide["enforce_ide_profile"]
                }
            }))
        }
        "ping" => Ok(json!({})),
        "logging/setLevel" => Ok(json!({})),
        "resources/list" => Ok(json!({"resources": []})),
        "resources/templates/list" => Ok(json!({"resourceTemplates": []})),
        "prompts/list" => Ok(json!({"prompts": []})),
        "tools/list" => Ok(json!({
            "tools": tools_catalog(runtime)
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
    let normalized_name = normalize_tool_name_for_dispatch(name);
    if normalized_name.as_ref() != name {
        trace_line(&format!(
            "tools.call.normalized original={} normalized={}",
            name,
            normalized_name.as_ref()
        ));
    }
    if !is_known_canonical_tool_name(normalized_name.as_ref()) {
        return Err(anyhow!(
            "unknown tool: {name} (normalized: {})",
            normalized_name.as_ref()
        ));
    }
    runtime.ensure_tool_allowed_for_active_role(normalized_name.as_ref())?;

    let result = match normalized_name.as_ref() {
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
        "cabal.tool_search" => handle_tool_search(runtime, &arguments)?,
        "cabal.get_tool_schema" => handle_get_tool_schema(runtime, &arguments)?,
        "cabal.programmatic_call" => handle_programmatic_call(runtime, cpu, &arguments)?,
        "cabal.result_compact" => {
            let payload = arguments
                .get("payload")
                .cloned()
                .ok_or_else(|| anyhow!("payload is required"))?;
            let max_chars = arguments.get("max_chars").and_then(|x| x.as_u64());
            runtime.compact_result_value(&payload, max_chars)?
        }
        "cabal.get_result_compact_policy" => runtime.get_result_compact_policy(),
        "cabal.set_result_compact_policy" => {
            let enabled = arguments.get("enabled").and_then(|x| x.as_bool());
            let max_chars = arguments.get("max_chars").and_then(|x| x.as_u64());
            let preview_items = arguments.get("preview_items").and_then(|x| x.as_u64());
            runtime.set_result_compact_policy(enabled, max_chars, preview_items)?
        }
        "cabal.get_context_window_policy" => runtime.get_context_window_policy(),
        "cabal.set_context_window_policy" => {
            let lazy_tool_search = arguments.get("lazy_tool_search").and_then(|x| x.as_bool());
            let lazy_threshold_pct = arguments.get("lazy_threshold_pct").and_then(|x| x.as_u64());
            let programmatic_max_calls = arguments
                .get("programmatic_max_calls")
                .and_then(|x| x.as_u64());
            runtime.set_context_window_policy(
                lazy_tool_search,
                lazy_threshold_pct,
                programmatic_max_calls,
            )?
        }
        "cabal.get_role_profile" => runtime.get_role_profile(),
        "cabal.list_role_profiles" => runtime.list_role_profiles(),
        "cabal.request_role_switch" => {
            let target_role = arguments
                .get("target_role")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("target_role is required"))?
                .to_string();
            let requested_by = arguments
                .get("requested_by")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            let reason = arguments
                .get("reason")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            runtime.request_role_switch(target_role, requested_by, reason)?
        }
        "cabal.approve_role_switch" => {
            let approved = arguments
                .get("approved")
                .and_then(|x| x.as_bool())
                .ok_or_else(|| anyhow!("approved is required"))?;
            let approved_by = arguments
                .get("approved_by")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            let note = arguments
                .get("note")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            runtime.approve_role_switch(approved, approved_by, note)?
        }
        "cabal.set_role_profile" => {
            let target_role = arguments
                .get("target_role")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("target_role is required"))?
                .to_string();
            let actor = arguments
                .get("actor")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            let reason = arguments
                .get("reason")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            runtime.set_role_profile(target_role, actor, reason)?
        }
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
        "cabal.classify_task" => {
            let question = arguments
                .get("question")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("question is required"))?;
            let task_type = arguments
                .get("task_type")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            runtime.classify_task(question, task_type)?
        }
        "cabal.get_budget_policy" => runtime.get_budget_policy(),
        "cabal.set_budget_policy" => {
            let risk = arguments
                .get("risk")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("risk is required"))?
                .to_string();
            let max_steps = arguments.get("max_steps").and_then(|x| x.as_u64());
            let max_tool_calls = arguments.get("max_tool_calls").and_then(|x| x.as_u64());
            let max_runtime_sec = arguments.get("max_runtime_sec").and_then(|x| x.as_u64());
            runtime.set_budget_policy(risk, max_steps, max_tool_calls, max_runtime_sec)?
        }
        "cabal.plan_task_execution" => {
            let question = arguments
                .get("question")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow!("question is required"))?;
            let task_type = arguments
                .get("task_type")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            let priority = arguments.get("priority").and_then(|x| x.as_str());
            runtime.plan_task_execution(question, task_type, priority)?
        }
        "cabal.get_patch_gate_policy" => runtime.get_patch_gate_policy(),
        "cabal.set_patch_gate_policy" => {
            let require_review_on_unsafe = arguments
                .get("require_review_on_unsafe")
                .and_then(|x| x.as_bool());
            let require_review_on_build_scripts = arguments
                .get("require_review_on_build_scripts")
                .and_then(|x| x.as_bool());
            let deny_on_secrets = arguments.get("deny_on_secrets").and_then(|x| x.as_bool());
            let max_auto_apply_files = arguments
                .get("max_auto_apply_files")
                .and_then(|x| x.as_u64());
            runtime.set_patch_gate_policy(
                require_review_on_unsafe,
                require_review_on_build_scripts,
                deny_on_secrets,
                max_auto_apply_files,
            )?
        }
        "cabal.evaluate_patch_gate" => {
            let files = read_string_array(arguments.get("files"))?;
            if files.is_empty() {
                return Err(anyhow!("files is required"));
            }
            let task_risk = arguments.get("task_risk").and_then(|x| x.as_str());
            let touches_unsafe = arguments.get("touches_unsafe").and_then(|x| x.as_bool());
            let touches_build_scripts = arguments
                .get("touches_build_scripts")
                .and_then(|x| x.as_bool());
            let touches_secrets = arguments.get("touches_secrets").and_then(|x| x.as_bool());
            let tests_passed = arguments.get("tests_passed").and_then(|x| x.as_bool());
            runtime.evaluate_patch_gate(
                files,
                task_risk,
                touches_unsafe,
                touches_build_scripts,
                touches_secrets,
                tests_passed,
            )?
        }
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
        _ => unreachable!("tool name pre-validated"),
    };

    Ok(json!({
        "content": [{"type": "text", "text": serde_json::to_string_pretty(&result)?}],
        "isError": false
    }))
}

fn handle_tool_search(runtime: &CabalRuntime, arguments: &Value) -> Result<Value> {
    let query = arguments
        .get("query")
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if query.chars().count() > 256 {
        bail!("query is too long");
    }
    let limit = arguments
        .get("limit")
        .and_then(|x| x.as_u64())
        .unwrap_or(12);
    if limit == 0 || limit > 100 {
        bail!("limit must be in [1, 100]");
    }
    let include_schema = arguments
        .get("include_schema")
        .and_then(|x| x.as_bool())
        .unwrap_or(false);
    let include_unavailable = arguments
        .get("include_unavailable")
        .and_then(|x| x.as_bool())
        .unwrap_or(false);

    let allowed: HashSet<String> = runtime
        .allowed_tools_for_active_role()
        .into_iter()
        .collect();
    let catalog = if include_unavailable {
        tools_catalog_with_alias_profile_and_format_filtered(
            CompatAliasProfile::None,
            ToolNameFormat::Canonical,
            None,
        )
    } else {
        tools_catalog_with_alias_profile_and_format_filtered(
            CompatAliasProfile::None,
            ToolNameFormat::Canonical,
            Some(&allowed),
        )
    };
    let Some(arr) = catalog.as_array() else {
        return Ok(json!({"query": query, "total_candidates": 0, "tools": []}));
    };

    let query_tokens: Vec<&str> = query.split_whitespace().filter(|x| !x.is_empty()).collect();
    let mut ranked: Vec<(i64, Value)> = Vec::new();
    for tool in arr {
        let Some(name) = tool.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        let desc = tool
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let mut input_keys = Vec::new();
        if let Some(props) = tool
            .get("inputSchema")
            .and_then(|v| v.get("properties"))
            .and_then(|v| v.as_object())
        {
            input_keys.extend(props.keys().map(|x| x.as_str()));
        }

        let score = score_tool_match(&query_tokens, name, desc, &input_keys);
        if !query_tokens.is_empty() && score == 0 {
            continue;
        }

        let mut card = json!({
            "name": name,
            "summary": first_sentence_or_clip(desc, 180),
            "available_for_active_role": allowed.contains(name),
            "score": score,
        });
        if include_schema {
            card["inputSchema"] = tool
                .get("inputSchema")
                .cloned()
                .unwrap_or_else(|| json!({}));
        } else {
            card["input_keys"] = json!(input_keys);
        }
        ranked.push((score, card));
    }

    ranked.sort_by(|a, b| {
        let an = a.1.get("name").and_then(|v| v.as_str()).unwrap_or_default();
        let bn = b.1.get("name").and_then(|v| v.as_str()).unwrap_or_default();
        b.0.cmp(&a.0).then_with(|| an.cmp(bn))
    });
    let tools: Vec<Value> = ranked
        .into_iter()
        .take(limit as usize)
        .map(|(_, v)| v)
        .collect();
    Ok(json!({
        "query": query,
        "active_role_profile": runtime.state.active_role_profile,
        "lazy_tool_search": runtime.state.context_window_policy.lazy_tool_search,
        "lazy_threshold_pct": runtime.state.context_window_policy.lazy_threshold_pct,
        "returned": tools.len(),
        "tools": tools
    }))
}

fn handle_get_tool_schema(runtime: &CabalRuntime, arguments: &Value) -> Result<Value> {
    let requested = arguments
        .get("name")
        .and_then(|x| x.as_str())
        .ok_or_else(|| anyhow!("name is required"))?;
    let normalized = normalize_tool_name_for_dispatch(requested).into_owned();
    if !is_known_canonical_tool_name(&normalized) {
        bail!("unknown tool: {requested} (normalized: {normalized})");
    }

    let catalog = tools_catalog_with_alias_profile_and_format_filtered(
        CompatAliasProfile::None,
        ToolNameFormat::Canonical,
        None,
    );
    let Some(arr) = catalog.as_array() else {
        bail!("tool catalog is unavailable");
    };
    let tool = arr
        .iter()
        .find(|x| x.get("name").and_then(|v| v.as_str()) == Some(normalized.as_str()))
        .cloned()
        .ok_or_else(|| anyhow!("unknown tool: {normalized}"))?;
    let allowed: HashSet<String> = runtime
        .allowed_tools_for_active_role()
        .into_iter()
        .collect();
    Ok(json!({
        "requested_name": requested,
        "normalized_name": normalized,
        "available_for_active_role": allowed.contains(&normalized),
        "schema": tool
    }))
}

fn handle_programmatic_call(
    runtime: &mut CabalRuntime,
    cpu: &CpuProfile,
    arguments: &Value,
) -> Result<Value> {
    let calls = arguments
        .get("calls")
        .and_then(|x| x.as_array())
        .ok_or_else(|| anyhow!("calls is required"))?;
    if calls.is_empty() {
        bail!("calls is required");
    }

    let stop_on_error = arguments
        .get("stop_on_error")
        .and_then(|x| x.as_bool())
        .unwrap_or(true);
    let compact_each_result = arguments
        .get("compact_each_result")
        .and_then(|x| x.as_bool())
        .unwrap_or(true);
    let max_chars = arguments.get("max_chars").and_then(|x| x.as_u64());
    let limit = arguments
        .get("max_calls")
        .and_then(|x| x.as_u64())
        .unwrap_or(runtime.state.context_window_policy.programmatic_max_calls);
    if !(1..=256).contains(&limit) {
        bail!("max_calls must be in [1, 256]");
    }
    if calls.len() > limit as usize {
        bail!("calls exceeds max_calls policy");
    }

    let mut steps = Vec::new();
    let mut halted = false;

    for (idx, item) in calls.iter().enumerate() {
        let call_name = item
            .get("name")
            .and_then(|x| x.as_str())
            .ok_or_else(|| anyhow!("calls[].name is required"))?;
        let call_args = item.get("arguments").cloned().unwrap_or_else(|| json!({}));
        if !call_args.is_object() {
            bail!("calls[].arguments must be an object");
        }
        let normalized = normalize_tool_name_for_dispatch(call_name).into_owned();
        if normalized == "cabal.programmatic_call" {
            bail!("programmatic_call recursion is forbidden");
        }

        match call_tool(runtime, cpu, call_name, call_args.clone()) {
            Ok(wrapper) => {
                let payload = extract_tool_payload(&wrapper)?;
                let result = if compact_each_result {
                    runtime.compact_result_value(&payload, max_chars)?
                } else {
                    json!({
                        "truncated": false,
                        "original_chars": serde_json::to_string_pretty(&payload)?.chars().count(),
                        "max_chars": max_chars,
                        "text": serde_json::to_string_pretty(&payload)?
                    })
                };
                steps.push(json!({
                    "index": idx + 1,
                    "name": call_name,
                    "normalized_name": normalized,
                    "ok": true,
                    "result": result
                }));
            }
            Err(err) => {
                let classified = classify_error("tools/call", Some(normalized.as_str()), &err);
                let step = json!({
                    "index": idx + 1,
                    "name": call_name,
                    "normalized_name": normalized,
                    "ok": false,
                    "error": {
                        "cabal_code": classified.cabal_code,
                        "rpc_code": classified.rpc_code,
                        "retryable": classified.retryable,
                        "message": classified.message
                    }
                });
                steps.push(step);
                if stop_on_error {
                    halted = true;
                    break;
                }
            }
        }
    }

    let failed = steps
        .iter()
        .filter(|x| x.get("ok").and_then(|v| v.as_bool()) == Some(false))
        .count();
    let aggregate = runtime.compact_result_value(&json!(steps), max_chars)?;
    Ok(json!({
        "active_role_profile": runtime.state.active_role_profile,
        "stop_on_error": stop_on_error,
        "compact_each_result": compact_each_result,
        "max_calls": limit,
        "executed_steps": steps.len(),
        "failed_steps": failed,
        "halted": halted,
        "steps": steps,
        "aggregate": aggregate
    }))
}

fn extract_tool_payload(wrapper: &Value) -> Result<Value> {
    let text = wrapper
        .get("content")
        .and_then(|x| x.as_array())
        .and_then(|arr| arr.first())
        .and_then(|x| x.get("text"))
        .and_then(|x| x.as_str())
        .ok_or_else(|| anyhow!("tool response payload is malformed"))?;
    match serde_json::from_str::<Value>(text) {
        Ok(v) => Ok(v),
        Err(_) => Ok(Value::String(text.to_string())),
    }
}

fn score_tool_match(tokens: &[&str], name: &str, description: &str, input_keys: &[&str]) -> i64 {
    if tokens.is_empty() {
        return 1;
    }
    let name_l = name.to_ascii_lowercase();
    let desc_l = description.to_ascii_lowercase();
    let keys_l: Vec<String> = input_keys.iter().map(|x| x.to_ascii_lowercase()).collect();
    let mut score = 0i64;
    for token in tokens {
        let t = token.to_ascii_lowercase();
        if name_l.contains(t.as_str()) {
            score += 6;
        }
        if desc_l.contains(t.as_str()) {
            score += 2;
        }
        if keys_l.iter().any(|k| k.contains(t.as_str())) {
            score += 3;
        }
    }
    score
}

fn first_sentence_or_clip(input: &str, max_chars: usize) -> String {
    let input = input.trim();
    if input.is_empty() {
        return String::new();
    }
    if let Some((idx, _)) = input.char_indices().find(|(_, c)| *c == '.') {
        let first = &input[..=idx];
        if first.chars().count() <= max_chars {
            return first.to_string();
        }
    }
    let mut out = String::new();
    for ch in input.chars().take(max_chars) {
        out.push(ch);
    }
    if input.chars().count() > max_chars {
        out.push_str("...");
    }
    out
}

fn normalize_tool_name_for_dispatch(name: &str) -> Cow<'_, str> {
    if name.starts_with("cabal.") {
        return Cow::Borrowed(name);
    }
    if let Some(rest) = name.strip_prefix("cabal")
        && !rest.is_empty()
        && !rest.starts_with('.')
    {
        return Cow::Owned(format!("cabal.{rest}"));
    }
    Cow::Borrowed(name)
}

fn known_canonical_tool_names() -> &'static HashSet<String> {
    static NAMES: OnceLock<HashSet<String>> = OnceLock::new();
    NAMES.get_or_init(|| {
        let tools = tools_catalog_with_alias_profile_and_format(
            CompatAliasProfile::None,
            ToolNameFormat::Canonical,
        );
        let mut names = HashSet::new();
        if let Some(arr) = tools.as_array() {
            for tool in arr {
                if let Some(name) = tool.get("name").and_then(|v| v.as_str()) {
                    names.insert(name.to_string());
                }
            }
        }
        names
    })
}

fn is_known_canonical_tool_name(name: &str) -> bool {
    known_canonical_tool_names().contains(name)
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

fn tools_catalog(runtime: &CabalRuntime) -> Value {
    let allowed_tools: HashSet<String> = runtime.allowed_tools_for_active_role().into_iter().collect();
    let visible_tools = select_visible_tools_for_list(runtime, &allowed_tools);
    tools_catalog_with_alias_profile_and_format_filtered(
        compat_alias_profile_from_env(),
        tool_name_format_from_env(),
        Some(&visible_tools),
    )
}

fn select_visible_tools_for_list(
    runtime: &CabalRuntime,
    allowed_tools: &HashSet<String>,
) -> HashSet<String> {
    select_visible_tools(allowed_tools, runtime.state.context_window_policy.lazy_tool_search)
}

fn select_visible_tools(allowed_tools: &HashSet<String>, lazy_tool_search: bool) -> HashSet<String> {
    if !lazy_tool_search {
        return allowed_tools.clone();
    }
    let bootstrap = bootstrap_visible_tool_names();
    allowed_tools
        .iter()
        .filter(|name| bootstrap.contains(name.as_str()))
        .cloned()
        .collect()
}

fn bootstrap_visible_tool_names() -> &'static HashSet<&'static str> {
    static BOOTSTRAP: OnceLock<HashSet<&'static str>> = OnceLock::new();
    BOOTSTRAP.get_or_init(|| {
        HashSet::from([
            "cabal.get_state",
            "cabal.get_role_profile",
            "cabal.list_role_profiles",
            "cabal.request_role_switch",
            "cabal.approve_role_switch",
            "cabal.set_role_profile",
            "cabal.tool_search",
            "cabal.get_tool_schema",
            "cabal.programmatic_call",
            "cabal.result_compact",
            "cabal.get_result_compact_policy",
            "cabal.set_result_compact_policy",
            "cabal.get_context_window_policy",
            "cabal.set_context_window_policy",
        ])
    })
}

fn tools_catalog_with_alias_profile_and_format(
    profile: CompatAliasProfile,
    format: ToolNameFormat,
) -> Value {
    tools_catalog_with_alias_profile_and_format_filtered(profile, format, None)
}

fn tools_catalog_with_alias_profile_and_format_filtered(
    profile: CompatAliasProfile,
    format: ToolNameFormat,
    allowed_canonical_tools: Option<&HashSet<String>>,
) -> Value {
    let mut tools = json!([
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
            "name": "cabal.tool_search",
            "description": "Ленивый поиск инструментов по имени/описанию с выдачей кратких карточек.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 100},
                    "include_schema": {"type": "boolean"},
                    "include_unavailable": {"type": "boolean"}
                }
            }
        },
        {
            "name": "cabal.get_tool_schema",
            "description": "Возвращает полное описание и inputSchema выбранного инструмента.",
            "inputSchema": {
                "type": "object",
                "required": ["name"],
                "properties": {
                    "name": {"type": "string"}
                }
            }
        },
        {
            "name": "cabal.programmatic_call",
            "description": "Программный вызов цепочки MCP-инструментов с агрегированным и компактным результатом.",
            "inputSchema": {
                "type": "object",
                "required": ["calls"],
                "properties": {
                    "calls": {
                        "type": "array",
                        "minItems": 1,
                        "items": {
                            "type": "object",
                            "required": ["name"],
                            "properties": {
                                "name": {"type": "string"},
                                "arguments": {"type": "object"}
                            }
                        }
                    },
                    "stop_on_error": {"type": "boolean"},
                    "compact_each_result": {"type": "boolean"},
                    "max_calls": {"type": "integer", "minimum": 1, "maximum": 256},
                    "max_chars": {"type": "integer", "minimum": 256, "maximum": 200000}
                }
            }
        },
        {
            "name": "cabal.result_compact",
            "description": "Сжимает произвольный JSON-результат в компактную форму для контекста.",
            "inputSchema": {
                "type": "object",
                "required": ["payload"],
                "properties": {
                    "payload": {},
                    "max_chars": {"type": "integer", "minimum": 256, "maximum": 200000}
                }
            }
        },
        {
            "name": "cabal.get_result_compact_policy",
            "description": "Возвращает policy компактирования результатов.",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "cabal.set_result_compact_policy",
            "description": "Настраивает policy компактирования результатов.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "enabled": {"type": "boolean"},
                    "max_chars": {"type": "integer", "minimum": 256, "maximum": 200000},
                    "preview_items": {"type": "integer", "minimum": 1, "maximum": 128}
                }
            }
        },
        {
            "name": "cabal.get_context_window_policy",
            "description": "Возвращает policy экономии контекста и limits programmatic call.",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "cabal.set_context_window_policy",
            "description": "Настраивает policy экономии контекста и limits programmatic call.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "lazy_tool_search": {"type": "boolean"},
                    "lazy_threshold_pct": {"type": "integer", "minimum": 1, "maximum": 95},
                    "programmatic_max_calls": {"type": "integer", "minimum": 1, "maximum": 256}
                }
            }
        },
        {
            "name": "cabal.get_role_profile",
            "description": "Возвращает активный role-profile и доступные инструменты для текущей роли.",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "cabal.list_role_profiles",
            "description": "Возвращает карту role-profile -> доступные инструменты.",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "cabal.request_role_switch",
            "description": "Создаёт pending-запрос на переключение role-profile.",
            "inputSchema": {
                "type": "object",
                "required": ["target_role"],
                "properties": {
                    "target_role": {"type": "string"},
                    "requested_by": {"type": "string"},
                    "reason": {"type": "string"}
                }
            }
        },
        {
            "name": "cabal.approve_role_switch",
            "description": "Подтверждает/отклоняет pending-запрос на переключение role-profile.",
            "inputSchema": {
                "type": "object",
                "required": ["approved"],
                "properties": {
                    "approved": {"type": "boolean"},
                    "approved_by": {"type": "string"},
                    "note": {"type": "string"}
                }
            }
        },
        {
            "name": "cabal.set_role_profile",
            "description": "Немедленно переключает активный role-profile (guarded).",
            "inputSchema": {
                "type": "object",
                "required": ["target_role"],
                "properties": {
                    "target_role": {"type": "string"},
                    "actor": {"type": "string"},
                    "reason": {"type": "string"}
                }
            }
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
            "name": "cabal.classify_task",
            "description": "Детерминированно классифицирует задачу (type/risk/confidence) и возвращает базовый budget-profile.",
            "inputSchema": {
                "type": "object",
                "required": ["question"],
                "properties": {
                    "question": {"type": "string"},
                    "task_type": {"type": "string"}
                }
            }
        },
        {
            "name": "cabal.get_budget_policy",
            "description": "Возвращает policy бюджетов выполнения по risk-level (steps/tool_calls/runtime_sec).",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "cabal.set_budget_policy",
            "description": "Обновляет budget-policy для risk-level.",
            "inputSchema": {
                "type": "object",
                "required": ["risk"],
                "properties": {
                    "risk": {"type": "string", "enum": ["low", "medium", "high", "critical"]},
                    "max_steps": {"type": "integer", "minimum": 1},
                    "max_tool_calls": {"type": "integer", "minimum": 1},
                    "max_runtime_sec": {"type": "integer", "minimum": 1}
                }
            }
        },
        {
            "name": "cabal.plan_task_execution",
            "description": "Строит execution-plan: классификация задачи + budget с учётом priority.",
            "inputSchema": {
                "type": "object",
                "required": ["question"],
                "properties": {
                    "question": {"type": "string"},
                    "task_type": {"type": "string"},
                    "priority": {"type": "string", "enum": ["low", "normal", "high", "critical"]}
                }
            }
        },
        {
            "name": "cabal.get_patch_gate_policy",
            "description": "Возвращает policy patch-gate (unsafe/build/secrets/auto-apply limit).",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "cabal.set_patch_gate_policy",
            "description": "Обновляет policy patch-gate.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "require_review_on_unsafe": {"type": "boolean"},
                    "require_review_on_build_scripts": {"type": "boolean"},
                    "deny_on_secrets": {"type": "boolean"},
                    "max_auto_apply_files": {"type": "integer", "minimum": 1}
                }
            }
        },
        {
            "name": "cabal.evaluate_patch_gate",
            "description": "Оценивает патч и возвращает режим применения: auto_apply|suggest_only|require_confirmation|deny.",
            "inputSchema": {
                "type": "object",
                "required": ["files"],
                "properties": {
                    "files": {"type": "array", "items": {"type": "string"}, "minItems": 1},
                    "task_risk": {"type": "string", "enum": ["low", "medium", "high", "critical"]},
                    "touches_unsafe": {"type": "boolean"},
                    "touches_build_scripts": {"type": "boolean"},
                    "touches_secrets": {"type": "boolean"},
                    "tests_passed": {"type": "boolean"}
                }
            }
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
    ]);
    if let Some(allowed) = allowed_canonical_tools {
        if let Some(arr) = tools.as_array_mut() {
            arr.retain(|tool| {
                let Some(name) = tool.get("name").and_then(|v| v.as_str()) else {
                    return false;
                };
                allowed.contains(name)
            });
        }
    }
    if matches!(format, ToolNameFormat::Canonical) {
        append_compact_cabal_aliases(&mut tools, profile);
    } else {
        rewrite_tools_for_roo_compact_names(&mut tools);
    }
    tools
}

fn rewrite_tools_for_roo_compact_names(tools: &mut Value) {
    let Some(arr) = tools.as_array_mut() else {
        return;
    };
    for tool in arr.iter_mut() {
        let Some(obj) = tool.as_object_mut() else {
            continue;
        };
        let Some(name) = obj.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(rest) = name.strip_prefix("cabal.") else {
            continue;
        };
        obj.insert("name".to_string(), Value::String(format!("cabal{rest}")));
    }
}

fn append_compact_cabal_aliases(tools: &mut Value, profile: CompatAliasProfile) {
    if matches!(profile, CompatAliasProfile::None) {
        return;
    }
    let Some(arr) = tools.as_array_mut() else {
        return;
    };

    let existing_names: std::collections::HashSet<String> = arr
        .iter()
        .filter_map(|t| {
            t.get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect();

    let base = arr.clone();
    let mut aliases = Vec::new();
    for tool in base {
        let Some(name) = tool.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(rest) = name.strip_prefix("cabal.") else {
            continue;
        };
        if !should_expose_alias(name, profile) {
            continue;
        }
        let alias_name = format!("cabal{rest}");
        if existing_names.contains(&alias_name) {
            continue;
        }
        let mut alias_tool = tool.clone();
        if let Some(obj) = alias_tool.as_object_mut() {
            obj.insert("name".to_string(), Value::String(alias_name));
            if let Some(desc) = obj.get("description").and_then(|v| v.as_str()) {
                obj.insert(
                    "description".to_string(),
                    Value::String(format!("{desc} (compat alias without dot).")),
                );
            }
        }
        aliases.push(alias_tool);
    }
    arr.extend(aliases);
}

fn should_expose_alias(name: &str, profile: CompatAliasProfile) -> bool {
    match profile {
        CompatAliasProfile::None => false,
        CompatAliasProfile::Full => true,
        CompatAliasProfile::Core => {
            name.starts_with("cabal.get_")
                || matches!(
                    name,
                    "cabal.classify_task"
                        | "cabal.plan_task_execution"
                        | "cabal.evaluate_patch_gate"
                        | "cabal.proxy_execute"
                        | "cabal.transition_phase"
                        | "cabal.transition_phase_strict"
                        | "cabal.gate_check"
                        | "cabal.route_consult"
                        | "cabal.register_evidence"
                        | "cabal.record_event"
                        | "cabal.ack_cross_rules"
                )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_startup_options_defaults() {
        let parsed = parse_startup_options(Vec::<String>::new()).expect("parse");
        assert_eq!(
            parsed,
            StartupOptions {
                show_help: false,
                strict_artifacts: None
            }
        );
    }

    #[test]
    fn parse_startup_options_help_and_strict_toggle() {
        let parsed = parse_startup_options(vec!["--help", "--strict-artifacts"]).expect("parse");
        assert_eq!(parsed.show_help, true);
        assert_eq!(parsed.strict_artifacts, Some(true));
    }

    #[test]
    fn parse_startup_options_explicit_bool_values() {
        let parsed = parse_startup_options(vec!["--strict-artifacts=false"]).expect("parse");
        assert_eq!(parsed.strict_artifacts, Some(false));

        let parsed = parse_startup_options(vec!["--strict-artifacts", "yes"]).expect("parse");
        assert_eq!(parsed.strict_artifacts, Some(true));

        let parsed = parse_startup_options(vec!["--no-strict-artifacts"]).expect("parse");
        assert_eq!(parsed.strict_artifacts, Some(false));
    }

    #[test]
    fn parse_startup_options_ignores_unknown_flag() {
        let parsed = parse_startup_options(vec!["--unknown"]).expect("must parse");
        assert_eq!(parsed.show_help, false);
        assert_eq!(parsed.strict_artifacts, None);
    }

    #[test]
    fn normalize_tool_name_keeps_canonical_name() {
        let normalized = normalize_tool_name_for_dispatch("cabal.get_state");
        assert_eq!(normalized.as_ref(), "cabal.get_state");
    }

    #[test]
    fn normalize_tool_name_recovers_missing_dot_after_prefix() {
        let normalized = normalize_tool_name_for_dispatch("cabalget_capabilities");
        assert_eq!(normalized.as_ref(), "cabal.get_capabilities");

        let normalized = normalize_tool_name_for_dispatch("cabalroute_consult");
        assert_eq!(normalized.as_ref(), "cabal.route_consult");
    }

    #[test]
    fn normalize_tool_name_leaves_non_cabal_unchanged() {
        let normalized = normalize_tool_name_for_dispatch("tools.list");
        assert_eq!(normalized.as_ref(), "tools.list");
    }

    #[test]
    fn tools_catalog_exposes_only_canonical_names() {
        let tools = tools_catalog_with_alias_profile_and_format(
            CompatAliasProfile::None,
            ToolNameFormat::Canonical,
        );
        let arr = tools.as_array().expect("tools array");
        let has_canonical = arr
            .iter()
            .any(|t| t.get("name").and_then(|v| v.as_str()) == Some("cabal.get_state"));
        let has_alias = arr
            .iter()
            .any(|t| t.get("name").and_then(|v| v.as_str()) == Some("cabalget_state"));
        assert!(has_canonical, "expected canonical cabal.get_state tool");
        assert!(
            !has_alias,
            "compact alias must not be exposed in tools/list"
        );
    }

    #[test]
    fn tools_catalog_core_profile_includes_get_state_alias() {
        let tools = tools_catalog_with_alias_profile_and_format(
            CompatAliasProfile::Core,
            ToolNameFormat::Canonical,
        );
        let arr = tools.as_array().expect("tools array");
        let has_alias = arr
            .iter()
            .any(|t| t.get("name").and_then(|v| v.as_str()) == Some("cabalget_state"));
        let has_non_core_alias = arr
            .iter()
            .any(|t| t.get("name").and_then(|v| v.as_str()) == Some("cabalset_policy_security"));
        assert!(has_alias, "core profile must expose cabalget_state");
        assert!(
            !has_non_core_alias,
            "core profile must not expose all aliases"
        );
    }

    #[test]
    fn tools_catalog_roo_compact_has_no_dot_names() {
        let tools = tools_catalog_with_alias_profile_and_format(
            CompatAliasProfile::None,
            ToolNameFormat::RooCompact,
        );
        let arr = tools.as_array().expect("tools array");
        let has_compact = arr
            .iter()
            .any(|t| t.get("name").and_then(|v| v.as_str()) == Some("cabalget_state"));
        let has_canonical = arr
            .iter()
            .any(|t| t.get("name").and_then(|v| v.as_str()) == Some("cabal.get_state"));
        assert!(has_compact, "expected compact cabalget_state");
        assert!(
            !has_canonical,
            "canonical name must be absent in Roo format"
        );
    }

    #[test]
    fn tools_catalog_filtered_by_role_allowlist() {
        let allowed = HashSet::from([
            "cabal.get_state".to_string(),
            "cabal.get_role_profile".to_string(),
            "cabal.request_role_switch".to_string(),
        ]);
        let tools = tools_catalog_with_alias_profile_and_format_filtered(
            CompatAliasProfile::None,
            ToolNameFormat::Canonical,
            Some(&allowed),
        );
        let arr = tools.as_array().expect("tools array");
        assert_eq!(arr.len(), 3);
        assert!(
            arr.iter()
                .any(|t| t.get("name").and_then(|v| v.as_str()) == Some("cabal.get_state"))
        );
        assert!(
            arr.iter()
                .any(|t| t.get("name").and_then(|v| v.as_str()) == Some("cabal.get_role_profile"))
        );
        assert!(
            arr.iter().any(
                |t| t.get("name").and_then(|v| v.as_str()) == Some("cabal.request_role_switch")
            )
        );
    }

    #[test]
    fn known_tool_name_set_contains_role_switch_tools() {
        assert!(is_known_canonical_tool_name("cabal.get_role_profile"));
        assert!(is_known_canonical_tool_name("cabal.request_role_switch"));
        assert!(is_known_canonical_tool_name("cabal.approve_role_switch"));
        assert!(is_known_canonical_tool_name("cabal.tool_search"));
        assert!(is_known_canonical_tool_name("cabal.programmatic_call"));
        assert!(is_known_canonical_tool_name("cabal.result_compact"));
        assert!(!is_known_canonical_tool_name("cabal.unknown_tool"));
    }

    #[test]
    fn first_sentence_or_clip_prefers_sentence_boundary() {
        let out = first_sentence_or_clip("Первая фраза. Вторая фраза.", 80);
        assert_eq!(out, "Первая фраза.");
    }

    #[test]
    fn score_tool_match_prefers_name_and_input_keys() {
        let score = score_tool_match(
            &["compact", "result"],
            "cabal.result_compact",
            "Сжимает результат",
            &["payload", "max_chars"],
        );
        assert!(score >= 6);
    }

    #[test]
    fn select_visible_tools_lazy_mode_returns_bootstrap_only() {
        let allowed = HashSet::from([
            "cabal.get_state".to_string(),
            "cabal.tool_search".to_string(),
            "cabal.get_tool_schema".to_string(),
            "cabal.programmatic_call".to_string(),
            "cabal.set_consult_mode".to_string(),
        ]);
        let visible = select_visible_tools(&allowed, true);
        assert!(visible.contains("cabal.get_state"));
        assert!(visible.contains("cabal.tool_search"));
        assert!(!visible.contains("cabal.set_consult_mode"));
    }

    #[test]
    fn select_visible_tools_non_lazy_mode_returns_full_allowed_set() {
        let allowed = HashSet::from([
            "cabal.get_state".to_string(),
            "cabal.set_consult_mode".to_string(),
        ]);
        let visible = select_visible_tools(&allowed, false);
        assert_eq!(visible, allowed);
    }
}

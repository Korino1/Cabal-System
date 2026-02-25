use crate::core::audit::{
    AuditQuery as CoreAuditQuery, append_audit_record as core_append_audit_record,
    prune_audit_archives as core_prune_audit_archives, query_audit_items as core_query_audit_items,
    read_audit_items as core_read_audit_items, replay_audit_items as core_replay_audit_items,
    rotate_audit_log as core_rotate_audit_log, verify_audit_archive as core_verify_audit_archive,
};
use crate::core::events::{
    EventRecord, build_event_record as core_build_event_record,
    event_hash_material as core_event_hash_material,
};
use crate::core::fsm::{
    transition_phase as core_transition_phase,
    validate_strict_phase_transition as core_validate_strict_phase_transition,
};
use crate::core::gate::GateReport;
use crate::core::gate_engine::{GateEvalContext, build_gate_report as core_build_gate_report};
use crate::core::ide::{
    default_allowed_ide_profiles as core_default_allowed_ide_profiles,
    detect_ide_profile_from_client_name as core_detect_ide_profile_from_client_name,
    is_ide_profile_allowed as core_is_ide_profile_allowed,
    normalize_allowed_ide_profiles as core_normalize_allowed_ide_profiles,
};
use crate::core::policy::{
    PolicySigningKey, default_policy_signing_algorithm as core_default_policy_signing_algorithm,
    default_policy_signing_keys as core_default_policy_signing_keys,
    register_policy_nonce as core_register_policy_nonce,
    verify_policy_signature as core_verify_policy_signature,
};
use crate::core::proxy::{
    ProxyTraceRecord, build_proxy_trace_record as core_build_proxy_trace_record,
    evaluate_proxy_request as core_evaluate_proxy_request,
    proxy_trace_hash_input as core_proxy_trace_hash_input,
};
use crate::core::proxy_exec::{
    exec_fs as core_exec_fs, exec_network as core_exec_network, exec_shell as core_exec_shell,
    resolve_safe_repo_path as core_resolve_safe_repo_path,
};
use crate::core::router::{
    ConsultExecutorTelemetry,
    consult_allowed_roles_for_type as core_consult_allowed_roles_for_type, consult_feedback_key,
    default_consult_allowed_roles as core_default_consult_allowed_roles,
    default_consult_escalation_targets as core_default_consult_escalation_targets,
    default_consult_priority_timeouts as core_default_consult_priority_timeouts,
    default_consult_retry_limits as core_default_consult_retry_limits,
    default_consult_routing_map as core_default_consult_routing_map,
    is_consult_role_allowed as core_is_consult_role_allowed,
    normalize_consult_priority as core_normalize_consult_priority,
    normalize_escalation_target as core_normalize_escalation_target,
    resolve_adaptive_consult_executor as core_resolve_adaptive_consult_executor,
    resolve_adaptive_exploration_executor as core_resolve_adaptive_exploration_executor,
    resolve_consult_allowed_role_fallback as core_resolve_consult_allowed_role_fallback,
    resolve_consult_escalation as core_resolve_consult_escalation,
    resolve_consult_executor as core_resolve_consult_executor,
    resolve_consult_retries as core_resolve_consult_retries,
    resolve_consult_timeout as core_resolve_consult_timeout,
    should_use_adaptive_exploration as core_should_use_adaptive_exploration,
};
use crate::cpu::{CpuProfile, ExecutionPath};
use crate::errors::validate_error_codes_doc_parity;
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const PROXY_LOG_MAX_ENTRIES: usize = 5000;
const PROXY_LOG_RESULT_MAX_LIMIT: usize = 1000;
const AUDIT_QUERY_MAX_LIMIT: usize = 2000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsultMode {
    UserTracking,
    Yolo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyBundle {
    pub version: String,
    #[serde(default)]
    pub revision: u64,
    pub rules: Vec<String>,
    #[serde(default)]
    pub signature: Option<String>,
    pub forbidden_tokens: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskBudgetProfile {
    pub max_steps: u64,
    pub max_tool_calls: u64,
    pub max_runtime_sec: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchGatePolicy {
    #[serde(default = "default_patch_gate_require_review_on_unsafe")]
    pub require_review_on_unsafe: bool,
    #[serde(default = "default_patch_gate_require_review_on_build_scripts")]
    pub require_review_on_build_scripts: bool,
    #[serde(default = "default_patch_gate_deny_on_secrets")]
    pub deny_on_secrets: bool,
    #[serde(default = "default_patch_gate_max_auto_apply_files")]
    pub max_auto_apply_files: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskClassification {
    pub task_type: String,
    pub risk: String,
    pub confidence: f64,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleSwitchRequest {
    pub target_role: String,
    pub requested_by: String,
    pub reason: String,
    pub requested_at_unix: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultCompactPolicy {
    #[serde(default = "default_result_compact_enabled")]
    pub enabled: bool,
    #[serde(default = "default_result_compact_max_chars")]
    pub max_chars: u64,
    #[serde(default = "default_result_compact_preview_items")]
    pub preview_items: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextWindowPolicy {
    #[serde(default = "default_context_lazy_tool_search")]
    pub lazy_tool_search: bool,
    #[serde(default = "default_context_lazy_threshold_pct")]
    pub lazy_threshold_pct: u64,
    #[serde(default = "default_context_programmatic_max_calls")]
    pub programmatic_max_calls: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeState {
    pub project_id: String,
    pub phase: String,
    pub consult_mode: ConsultMode,
    #[serde(default = "default_consult_routing_map")]
    pub consult_routing_map: BTreeMap<String, String>,
    #[serde(default = "default_consult_priority_timeouts")]
    pub consult_priority_timeouts: BTreeMap<String, u64>,
    #[serde(default = "default_consult_retry_limits")]
    pub consult_retry_limits: BTreeMap<String, u64>,
    #[serde(default = "default_consult_escalation_targets")]
    pub consult_escalation_targets: BTreeMap<String, String>,
    #[serde(default = "default_consult_allowed_roles")]
    pub consult_allowed_roles: BTreeMap<String, Vec<String>>,
    #[serde(default = "default_consult_require_cross_rules_ack")]
    pub consult_require_cross_rules_ack: bool,
    #[serde(default = "default_consult_required_evidence_ids")]
    pub consult_required_evidence_ids: Vec<String>,
    #[serde(default = "default_adaptive_router_enabled")]
    pub adaptive_router_enabled: bool,
    #[serde(default = "default_adaptive_confidence_floor")]
    pub adaptive_confidence_floor: f64,
    #[serde(default = "default_adaptive_exploration_rate")]
    pub adaptive_exploration_rate: f64,
    #[serde(default = "default_adaptive_exploration_min_samples")]
    pub adaptive_exploration_min_samples: u64,
    #[serde(default = "default_consult_executor_telemetry")]
    pub consult_executor_telemetry: BTreeMap<String, ConsultExecutorTelemetry>,
    #[serde(default = "default_task_budget_profiles")]
    pub task_budget_profiles: BTreeMap<String, TaskBudgetProfile>,
    #[serde(default = "default_patch_gate_policy")]
    pub patch_gate_policy: PatchGatePolicy,
    #[serde(default = "default_active_role_profile")]
    pub active_role_profile: String,
    #[serde(default = "default_role_tool_access_profiles")]
    pub role_tool_access_profiles: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub pending_role_switch: Option<RoleSwitchRequest>,
    #[serde(default = "default_result_compact_policy")]
    pub result_compact_policy: ResultCompactPolicy,
    #[serde(default = "default_context_window_policy")]
    pub context_window_policy: ContextWindowPolicy,
    pub policy: PolicyBundle,
    pub policy_hash: u64,
    #[serde(default)]
    pub policy_revision: u64,
    #[serde(default = "default_require_zen4_fast_path")]
    pub require_zen4_fast_path: bool,
    #[serde(default = "default_require_avx512f")]
    pub require_avx512f: bool,
    #[serde(default = "default_require_avx512vl")]
    pub require_avx512vl: bool,
    #[serde(default = "default_require_fma")]
    pub require_fma: bool,
    #[serde(default = "default_require_bmi2")]
    pub require_bmi2: bool,
    #[serde(default = "default_require_sha")]
    pub require_sha: bool,
    #[serde(default = "default_require_signed_policy")]
    pub require_signed_policy: bool,
    #[serde(default = "default_policy_signing_keys")]
    pub policy_signing_keys: Vec<PolicySigningKey>,
    #[serde(default = "default_active_policy_key_id")]
    pub active_policy_key_id: String,
    #[serde(default)]
    pub used_policy_nonces: Vec<String>,
    #[serde(default = "default_proxy_deny_by_default")]
    pub proxy_deny_by_default: bool,
    #[serde(default = "default_proxy_allow")]
    pub proxy_allow: BTreeMap<String, Vec<String>>,
    #[serde(default = "default_proxy_allowed_operations")]
    pub proxy_allowed_operations: BTreeMap<String, Vec<String>>,
    #[serde(default = "default_proxy_denied_operations")]
    pub proxy_denied_operations: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub proxy_log: Vec<ProxyTraceRecord>,
    #[serde(default = "default_enforce_ide_profile")]
    pub enforce_ide_profile: bool,
    #[serde(default = "default_require_ide_client_info")]
    pub require_ide_client_info: bool,
    #[serde(default = "default_strict_gate_artifacts")]
    pub strict_gate_artifacts: bool,
    #[serde(default = "default_allowed_ide_profiles")]
    pub allowed_ide_profiles: Vec<String>,
    #[serde(default = "default_active_ide_profile")]
    pub active_ide_profile: String,
    #[serde(default)]
    pub active_ide_client_name: String,
    #[serde(default)]
    pub active_ide_client_version: String,
    #[serde(default = "default_audit_auto_rotate_enabled")]
    pub audit_auto_rotate_enabled: bool,
    #[serde(default = "default_audit_auto_rotate_max_bytes")]
    pub audit_auto_rotate_max_bytes: u64,
    #[serde(default = "default_audit_auto_rotate_max_age_sec")]
    pub audit_auto_rotate_max_age_sec: u64,
    #[serde(default = "default_audit_auto_rotate_compress")]
    pub audit_auto_rotate_compress: bool,
    #[serde(default = "default_audit_auto_rotate_keep_last")]
    pub audit_auto_rotate_keep_last: u64,
    #[serde(default = "default_audit_archive_dir")]
    pub audit_archive_dir: String,
    #[serde(default = "default_audit_last_rotation_unix")]
    pub audit_last_rotation_unix: u64,
    pub evidence: BTreeMap<String, String>,
    pub events: Vec<EventRecord>,
}

pub struct CabalRuntime {
    state_path: PathBuf,
    audit_path: PathBuf,
    pub state: RuntimeState,
}

impl CabalRuntime {
    pub fn load_or_create(root: &Path, cpu: &CpuProfile) -> Result<Self> {
        let dir = root.join(".cabal_runtime");
        if !dir.exists() {
            fs::create_dir_all(&dir).context("failed to create .cabal_runtime dir")?;
        }
        let state_path = dir.join("state.json");
        let audit_path = dir.join("audit.jsonl");
        if !audit_path.exists() {
            fs::File::create(&audit_path).context("failed to create audit.jsonl")?;
        }

        if state_path.exists() {
            let text = fs::read_to_string(&state_path).context("failed to read runtime state")?;
            let state: RuntimeState =
                serde_json::from_str(&text).context("failed to parse runtime state")?;
            return Ok(Self {
                state_path,
                audit_path,
                state,
            });
        }

        let policy = PolicyBundle {
            version: "0.1.0".to_string(),
            revision: 1,
            rules: vec![
                "all actions must go through cabal mcp runtime".to_string(),
                "phase transitions require gate validation".to_string(),
            ],
            signature: None,
            forbidden_tokens: vec!["bypass".to_string(), "direct-run".to_string()],
        };
        let policy_hash = cpu.hash_bytes(serde_json::to_string(&policy)?.as_bytes());
        let state = RuntimeState {
            project_id: "cabal-project".to_string(),
            phase: "C-0".to_string(),
            consult_mode: ConsultMode::UserTracking,
            consult_routing_map: default_consult_routing_map(),
            consult_priority_timeouts: default_consult_priority_timeouts(),
            consult_retry_limits: default_consult_retry_limits(),
            consult_escalation_targets: default_consult_escalation_targets(),
            consult_allowed_roles: default_consult_allowed_roles(),
            consult_require_cross_rules_ack: default_consult_require_cross_rules_ack(),
            consult_required_evidence_ids: default_consult_required_evidence_ids(),
            adaptive_router_enabled: default_adaptive_router_enabled(),
            adaptive_confidence_floor: default_adaptive_confidence_floor(),
            adaptive_exploration_rate: default_adaptive_exploration_rate(),
            adaptive_exploration_min_samples: default_adaptive_exploration_min_samples(),
            consult_executor_telemetry: default_consult_executor_telemetry(),
            task_budget_profiles: default_task_budget_profiles(),
            patch_gate_policy: default_patch_gate_policy(),
            active_role_profile: default_active_role_profile(),
            role_tool_access_profiles: default_role_tool_access_profiles(),
            pending_role_switch: None,
            result_compact_policy: default_result_compact_policy(),
            context_window_policy: default_context_window_policy(),
            policy,
            policy_hash,
            policy_revision: 1,
            require_zen4_fast_path: default_require_zen4_fast_path(),
            require_avx512f: default_require_avx512f(),
            require_avx512vl: default_require_avx512vl(),
            require_fma: default_require_fma(),
            require_bmi2: default_require_bmi2(),
            require_sha: default_require_sha(),
            require_signed_policy: true,
            policy_signing_keys: default_policy_signing_keys(),
            active_policy_key_id: default_active_policy_key_id(),
            used_policy_nonces: Vec::new(),
            proxy_deny_by_default: true,
            proxy_allow: default_proxy_allow(),
            proxy_allowed_operations: default_proxy_allowed_operations(),
            proxy_denied_operations: default_proxy_denied_operations(),
            proxy_log: Vec::new(),
            enforce_ide_profile: default_enforce_ide_profile(),
            require_ide_client_info: default_require_ide_client_info(),
            strict_gate_artifacts: default_strict_gate_artifacts(),
            allowed_ide_profiles: default_allowed_ide_profiles(),
            active_ide_profile: default_active_ide_profile(),
            active_ide_client_name: String::new(),
            active_ide_client_version: String::new(),
            audit_auto_rotate_enabled: default_audit_auto_rotate_enabled(),
            audit_auto_rotate_max_bytes: default_audit_auto_rotate_max_bytes(),
            audit_auto_rotate_max_age_sec: default_audit_auto_rotate_max_age_sec(),
            audit_auto_rotate_compress: default_audit_auto_rotate_compress(),
            audit_auto_rotate_keep_last: default_audit_auto_rotate_keep_last(),
            audit_archive_dir: default_audit_archive_dir(),
            audit_last_rotation_unix: now_unix()?,
            evidence: BTreeMap::new(),
            events: Vec::new(),
        };

        let runtime = Self {
            state_path,
            audit_path,
            state,
        };
        runtime.persist()?;
        Ok(runtime)
    }

    pub fn persist(&self) -> Result<()> {
        let text = serde_json::to_string_pretty(&self.state)?;
        fs::write(&self.state_path, text).context("failed to write runtime state")?;
        Ok(())
    }

    pub fn get_state_value(&self) -> Value {
        json!({
            "project_id": self.state.project_id,
            "phase": self.state.phase,
            "consult_mode": self.state.consult_mode,
            "consult_routing_rules_total": self.state.consult_routing_map.len(),
            "consult_priority_timeouts": self.state.consult_priority_timeouts,
            "consult_retry_limits": self.state.consult_retry_limits,
            "consult_escalation_targets": self.state.consult_escalation_targets,
            "consult_allowed_roles_total": self.state.consult_allowed_roles.len(),
            "consult_guard_policy": {
                "require_cross_rules_ack": self.state.consult_require_cross_rules_ack,
                "required_evidence_ids": self.state.consult_required_evidence_ids
            },
            "adaptive_router_enabled": self.state.adaptive_router_enabled,
            "adaptive_confidence_floor": self.state.adaptive_confidence_floor,
            "adaptive_exploration_rate": self.state.adaptive_exploration_rate,
            "adaptive_exploration_min_samples": self.state.adaptive_exploration_min_samples,
            "consult_feedback_profiles_total": self.state.consult_executor_telemetry.len(),
            "task_budget_profiles": self.state.task_budget_profiles,
            "patch_gate_policy": self.state.patch_gate_policy,
            "active_role_profile": self.state.active_role_profile,
            "known_role_profiles_total": self.state.role_tool_access_profiles.len(),
            "pending_role_switch": self.state.pending_role_switch,
            "result_compact_policy": self.state.result_compact_policy,
            "context_window_policy": self.state.context_window_policy,
            "policy_version": self.state.policy.version,
            "policy_revision": self.state.policy_revision,
            "cpu_policy": {
                "require_zen4_fast_path": self.state.require_zen4_fast_path,
                "require_avx512f": self.state.require_avx512f,
                "require_avx512vl": self.state.require_avx512vl,
                "require_fma": self.state.require_fma,
                "require_bmi2": self.state.require_bmi2,
                "require_sha": self.state.require_sha
            },
            "require_signed_policy": self.state.require_signed_policy,
            "active_policy_key_id": self.state.active_policy_key_id,
            "policy_signing_keys_total": self.state.policy_signing_keys.len(),
            "policy_hash": self.state.policy_hash,
            "proxy_deny_by_default": self.state.proxy_deny_by_default,
            "proxy_allow": self.state.proxy_allow,
            "proxy_allowed_operations": self.state.proxy_allowed_operations,
            "proxy_denied_operations": self.state.proxy_denied_operations,
            "proxy_log_total": self.state.proxy_log.len(),
            "proxy_log_max_entries": PROXY_LOG_MAX_ENTRIES,
            "ide_profile_policy": {
                "enforce": self.state.enforce_ide_profile,
                "require_client_info": self.state.require_ide_client_info,
                "allowed_profiles": self.state.allowed_ide_profiles
            },
            "gate_policy": {
                "strict_artifacts": self.state.strict_gate_artifacts
            },
            "active_ide_session": {
                "profile": self.state.active_ide_profile,
                "client_name": self.state.active_ide_client_name,
                "client_version": self.state.active_ide_client_version
            },
            "audit_rotation_policy": {
                "enabled": self.state.audit_auto_rotate_enabled,
                "max_bytes": self.state.audit_auto_rotate_max_bytes,
                "max_age_sec": self.state.audit_auto_rotate_max_age_sec,
                "compress": self.state.audit_auto_rotate_compress,
                "keep_last": self.state.audit_auto_rotate_keep_last,
                "archive_dir": self.state.audit_archive_dir,
                "last_rotation_unix": self.state.audit_last_rotation_unix
            },
            "events_total": self.state.events.len(),
            "evidence_total": self.state.evidence.len()
        })
    }

    pub fn get_role_profile(&self) -> Value {
        let allowed = self.allowed_tools_for_active_role();
        json!({
            "active_role_profile": self.state.active_role_profile,
            "allowed_tools_total": allowed.len(),
            "allowed_tools": allowed,
            "pending_role_switch": self.state.pending_role_switch,
        })
    }

    pub fn list_role_profiles(&self) -> Value {
        let mut profiles = BTreeMap::new();
        for (role, tools) in &self.state.role_tool_access_profiles {
            profiles.insert(
                role.clone(),
                json!({
                    "tools_total": tools.len(),
                    "tools": tools,
                }),
            );
        }
        json!({
            "active_role_profile": self.state.active_role_profile,
            "profiles": profiles
        })
    }

    pub fn request_role_switch(
        &mut self,
        target_role: String,
        requested_by: Option<String>,
        reason: Option<String>,
    ) -> Result<Value> {
        let target = normalize_role_name(&target_role)?;
        self.ensure_role_profile_exists(&target)?;
        let actor = requested_by
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
            .unwrap_or_else(|| self.state.active_role_profile.clone());
        let why = reason
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
            .unwrap_or_else(|| "role switch requested".to_string());
        let requested_at = now_unix()?;
        self.state.pending_role_switch = Some(RoleSwitchRequest {
            target_role: target.clone(),
            requested_by: actor.clone(),
            reason: why.clone(),
            requested_at_unix: requested_at,
        });
        self.append_audit(
            "role.switch.requested",
            json!({
                "from_role": self.state.active_role_profile,
                "to_role": target,
                "requested_by": actor,
                "reason": why,
                "requested_at_unix": requested_at
            }),
        )?;
        Ok(self.get_role_profile())
    }

    pub fn approve_role_switch(
        &mut self,
        approved: bool,
        approved_by: Option<String>,
        note: Option<String>,
    ) -> Result<Value> {
        let approver = approved_by
            .map(|x| x.trim().to_ascii_lowercase())
            .filter(|x| !x.is_empty())
            .unwrap_or_else(|| self.state.active_role_profile.clone());
        if self.state.active_role_profile != "orchestrator" && approver != "user" {
            bail!("policy deny: only orchestrator or user can approve role switch");
        }
        let pending = self
            .state
            .pending_role_switch
            .clone()
            .ok_or_else(|| anyhow!("no pending role switch request"))?;
        if !approved {
            self.state.pending_role_switch = None;
            self.append_audit(
                "role.switch.rejected",
                json!({
                    "from_role": self.state.active_role_profile,
                    "to_role": pending.target_role,
                    "approved_by": approver,
                    "note": note.unwrap_or_else(|| "rejected".to_string())
                }),
            )?;
            return Ok(self.get_role_profile());
        }
        self.ensure_role_switch_guards()?;
        self.apply_role_profile(
            pending.target_role.clone(),
            approver,
            note.unwrap_or_else(|| "approved role switch".to_string()),
        )?;
        self.state.pending_role_switch = None;
        Ok(self.get_role_profile())
    }

    pub fn set_role_profile(
        &mut self,
        target_role: String,
        actor: Option<String>,
        reason: Option<String>,
    ) -> Result<Value> {
        let target = normalize_role_name(&target_role)?;
        self.ensure_role_profile_exists(&target)?;
        let actor_normalized = actor
            .map(|x| x.trim().to_ascii_lowercase())
            .filter(|x| !x.is_empty())
            .unwrap_or_else(|| self.state.active_role_profile.clone());
        if self.state.active_role_profile != "orchestrator" && actor_normalized != "user" {
            bail!("policy deny: role switch requires orchestrator or user actor");
        }
        self.ensure_role_switch_guards()?;
        self.apply_role_profile(
            target.clone(),
            actor_normalized.clone(),
            reason
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .unwrap_or_else(|| "direct role switch".to_string()),
        )?;
        if self
            .state
            .pending_role_switch
            .as_ref()
            .map(|x| x.target_role.as_str())
            == Some(target.as_str())
        {
            self.state.pending_role_switch = None;
        }
        Ok(self.get_role_profile())
    }

    pub fn get_result_compact_policy(&self) -> Value {
        json!(self.state.result_compact_policy)
    }

    pub fn set_result_compact_policy(
        &mut self,
        enabled: Option<bool>,
        max_chars: Option<u64>,
        preview_items: Option<u64>,
    ) -> Result<Value> {
        if let Some(v) = enabled {
            self.state.result_compact_policy.enabled = v;
        }
        if let Some(v) = max_chars {
            if !(256..=200_000).contains(&v) {
                bail!("max_chars must be in [256, 200000]");
            }
            self.state.result_compact_policy.max_chars = v;
        }
        if let Some(v) = preview_items {
            if !(1..=128).contains(&v) {
                bail!("preview_items must be in [1, 128]");
            }
            self.state.result_compact_policy.preview_items = v;
        }
        self.append_audit(
            "policy.result_compact.updated",
            json!(self.state.result_compact_policy),
        )?;
        Ok(self.get_result_compact_policy())
    }

    pub fn get_context_window_policy(&self) -> Value {
        json!(self.state.context_window_policy)
    }

    pub fn set_context_window_policy(
        &mut self,
        lazy_tool_search: Option<bool>,
        lazy_threshold_pct: Option<u64>,
        programmatic_max_calls: Option<u64>,
    ) -> Result<Value> {
        if let Some(v) = lazy_tool_search {
            self.state.context_window_policy.lazy_tool_search = v;
        }
        if let Some(v) = lazy_threshold_pct {
            if !(1..=95).contains(&v) {
                bail!("lazy_threshold_pct must be in [1, 95]");
            }
            self.state.context_window_policy.lazy_threshold_pct = v;
        }
        if let Some(v) = programmatic_max_calls {
            if !(1..=256).contains(&v) {
                bail!("programmatic_max_calls must be in [1, 256]");
            }
            self.state.context_window_policy.programmatic_max_calls = v;
        }
        self.append_audit(
            "policy.context_window.updated",
            json!(self.state.context_window_policy),
        )?;
        Ok(self.get_context_window_policy())
    }

    pub fn compact_result_value(
        &self,
        value: &Value,
        max_chars_override: Option<u64>,
    ) -> Result<Value> {
        let policy = &self.state.result_compact_policy;
        let max_chars_u64 = max_chars_override.unwrap_or(policy.max_chars);
        if !(256..=200_000).contains(&max_chars_u64) {
            bail!("max_chars must be in [256, 200000]");
        }
        let max_chars = max_chars_u64 as usize;
        let original_text = serde_json::to_string_pretty(value)?;
        let original_chars = original_text.chars().count();
        if !policy.enabled || original_chars <= max_chars {
            return Ok(json!({
                "truncated": false,
                "original_chars": original_chars,
                "max_chars": max_chars_u64,
                "text": original_text
            }));
        }

        let preview_items = policy.preview_items as usize;
        let summary = summarize_value(value, preview_items);
        let summary_text = serde_json::to_string_pretty(&summary)?;
        let text = if summary_text.chars().count() <= max_chars {
            summary_text
        } else {
            truncate_chars_with_suffix(&summary_text, max_chars, "\n...<truncated>")
        };
        Ok(json!({
            "truncated": true,
            "original_chars": original_chars,
            "max_chars": max_chars_u64,
            "text": text,
            "summary": summary
        }))
    }

    pub fn allowed_tools_for_active_role(&self) -> Vec<String> {
        resolve_allowed_tools_for_role(
            &self.state.active_role_profile,
            &self.state.role_tool_access_profiles,
        )
        .into_iter()
        .collect()
    }

    pub fn ensure_tool_allowed_for_active_role(&self, tool_name: &str) -> Result<()> {
        let allowed = resolve_allowed_tools_for_role(
            &self.state.active_role_profile,
            &self.state.role_tool_access_profiles,
        );
        if allowed.contains(tool_name) {
            return Ok(());
        }
        bail!(
            "policy deny: tool is not allowed for active role profile (role={}, tool={})",
            self.state.active_role_profile,
            tool_name
        );
    }

    fn ensure_role_profile_exists(&self, role: &str) -> Result<()> {
        if self.state.role_tool_access_profiles.contains_key(role) {
            return Ok(());
        }
        bail!("unknown role profile: {role}");
    }

    fn ensure_role_switch_guards(&self) -> Result<()> {
        let exit_report = self.build_gate_report("exit", &self.state.phase)?;
        let entry_report = self.build_gate_report("entry", &self.state.phase)?;
        if !exit_report.pass || !entry_report.pass {
            bail!("policy deny: role switch blocked by gate check");
        }
        let mut required = cross_rules_required_evidence_ids();
        if self.state.consult_require_cross_rules_ack {
            for id in &self.state.consult_required_evidence_ids {
                if !required.iter().any(|x| x == id) {
                    required.push(id.clone());
                }
            }
        }
        let missing = missing_evidence_ids(&self.state.evidence, &required);
        if !missing.is_empty() {
            bail!(
                "policy deny: role switch requires cross-rules evidence (missing={})",
                missing.join(", ")
            );
        }
        Ok(())
    }

    fn apply_role_profile(
        &mut self,
        target_role: String,
        actor: String,
        reason: String,
    ) -> Result<()> {
        let from = self.state.active_role_profile.clone();
        if from == target_role {
            self.append_audit(
                "role.switch.applied",
                json!({
                    "from_role": from,
                    "to_role": target_role,
                    "actor": actor,
                    "reason": reason,
                    "changed": false
                }),
            )?;
            return Ok(());
        }
        self.state.active_role_profile = target_role.clone();
        self.append_audit(
            "role.switch.applied",
            json!({
                "from_role": from,
                "to_role": target_role,
                "actor": actor,
                "reason": reason,
                "changed": true
            }),
        )?;
        Ok(())
    }

    pub fn validate_cpu_policy(&self, cpu: &CpuProfile) -> Result<()> {
        if self.state.require_zen4_fast_path && !matches!(cpu.path, ExecutionPath::Zen4Avx512) {
            bail!("policy deny: zen4 fast path is required by cpu policy");
        }
        if self.state.require_avx512f && !cpu.has_avx512f {
            bail!("policy deny: cpu feature avx512f is required by cpu policy");
        }
        if self.state.require_avx512vl && !cpu.has_avx512vl {
            bail!("policy deny: cpu feature avx512vl is required by cpu policy");
        }
        if self.state.require_fma && !cpu.has_fma {
            bail!("policy deny: cpu feature fma is required by cpu policy");
        }
        if self.state.require_bmi2 && !cpu.has_bmi2 {
            bail!("policy deny: cpu feature bmi2 is required by cpu policy");
        }
        if self.state.require_sha && !cpu.has_sha {
            bail!("policy deny: cpu feature sha is required by cpu policy");
        }
        Ok(())
    }

    pub fn get_cpu_policy(&self) -> Value {
        json!({
            "require_zen4_fast_path": self.state.require_zen4_fast_path,
            "require_avx512f": self.state.require_avx512f,
            "require_avx512vl": self.state.require_avx512vl,
            "require_fma": self.state.require_fma,
            "require_bmi2": self.state.require_bmi2,
            "require_sha": self.state.require_sha
        })
    }

    pub fn set_cpu_policy(
        &mut self,
        cpu: &CpuProfile,
        require_zen4_fast_path: Option<bool>,
        require_avx512f: Option<bool>,
        require_avx512vl: Option<bool>,
        require_fma: Option<bool>,
        require_bmi2: Option<bool>,
        require_sha: Option<bool>,
    ) -> Result<Value> {
        if let Some(v) = require_avx512f {
            if v && !cpu.has_avx512f {
                bail!("policy deny: cpu feature avx512f is required by cpu policy");
            }
            self.state.require_avx512f = v;
        }
        if let Some(v) = require_avx512vl {
            if v && !cpu.has_avx512vl {
                bail!("policy deny: cpu feature avx512vl is required by cpu policy");
            }
            self.state.require_avx512vl = v;
        }
        if let Some(v) = require_fma {
            if v && !cpu.has_fma {
                bail!("policy deny: cpu feature fma is required by cpu policy");
            }
            self.state.require_fma = v;
        }
        if let Some(v) = require_bmi2 {
            if v && !cpu.has_bmi2 {
                bail!("policy deny: cpu feature bmi2 is required by cpu policy");
            }
            self.state.require_bmi2 = v;
        }
        if let Some(v) = require_sha {
            if v && !cpu.has_sha {
                bail!("policy deny: cpu feature sha is required by cpu policy");
            }
            self.state.require_sha = v;
        }
        if let Some(v) = require_zen4_fast_path {
            if v && !matches!(cpu.path, ExecutionPath::Zen4Avx512) {
                bail!("policy deny: zen4 fast path is required by cpu policy");
            }
            self.state.require_zen4_fast_path = v;
        }
        self.append_audit(
            "cpu.policy_set",
            json!({
                "require_zen4_fast_path": self.state.require_zen4_fast_path,
                "require_avx512f": self.state.require_avx512f,
                "require_avx512vl": self.state.require_avx512vl,
                "require_fma": self.state.require_fma,
                "require_bmi2": self.state.require_bmi2,
                "require_sha": self.state.require_sha,
                "cpu_path": cpu.path
            }),
        )?;
        Ok(self.get_cpu_policy())
    }

    pub fn register_ide_client_session(
        &mut self,
        client_name: Option<&str>,
        client_version: Option<&str>,
    ) -> Result<Value> {
        let normalized_name = client_name.unwrap_or("").trim();
        if self.state.enforce_ide_profile
            && self.state.require_ide_client_info
            && normalized_name.is_empty()
        {
            bail!("policy deny: client_info.name is required under ide profile enforcement");
        }

        let profile = core_detect_ide_profile_from_client_name(Some(normalized_name));
        if self.state.enforce_ide_profile
            && !core_is_ide_profile_allowed(&profile, &self.state.allowed_ide_profiles)
        {
            bail!("policy deny: ide profile is not allowed: {profile}");
        }

        self.state.active_ide_profile = profile.clone();
        self.state.active_ide_client_name = normalized_name.to_string();
        self.state.active_ide_client_version = client_version.unwrap_or("").trim().to_string();

        self.append_audit(
            "ide.client_initialized",
            json!({
                "profile": self.state.active_ide_profile,
                "client_name": self.state.active_ide_client_name,
                "client_version": self.state.active_ide_client_version,
                "enforce_ide_profile": self.state.enforce_ide_profile,
                "require_client_info": self.state.require_ide_client_info,
                "allowed_profiles": self.state.allowed_ide_profiles
            }),
        )?;

        Ok(self.get_ide_profile_policy())
    }

    pub fn get_ide_profile_policy(&self) -> Value {
        json!({
            "enforce_ide_profile": self.state.enforce_ide_profile,
            "require_client_info": self.state.require_ide_client_info,
            "allowed_profiles": self.state.allowed_ide_profiles,
            "active_profile": self.state.active_ide_profile,
            "active_client": {
                "name": self.state.active_ide_client_name,
                "version": self.state.active_ide_client_version
            }
        })
    }

    pub fn set_ide_profile_policy(
        &mut self,
        enforce_ide_profile: Option<bool>,
        require_client_info: Option<bool>,
        allowed_profiles: Option<Vec<String>>,
    ) -> Result<Value> {
        if let Some(v) = enforce_ide_profile {
            self.state.enforce_ide_profile = v;
        }
        if let Some(v) = require_client_info {
            self.state.require_ide_client_info = v;
        }

        if let Some(raw_profiles) = allowed_profiles {
            self.state.allowed_ide_profiles = core_normalize_allowed_ide_profiles(&raw_profiles)?;
        }

        if self.state.enforce_ide_profile
            && !core_is_ide_profile_allowed(
                &self.state.active_ide_profile,
                &self.state.allowed_ide_profiles,
            )
        {
            bail!(
                "policy deny: active ide profile is not allowed under current policy: {}",
                self.state.active_ide_profile
            );
        }

        self.append_audit(
            "ide.profile_policy_set",
            json!({
                "enforce_ide_profile": self.state.enforce_ide_profile,
                "require_client_info": self.state.require_ide_client_info,
                "allowed_profiles": self.state.allowed_ide_profiles,
                "active_profile": self.state.active_ide_profile
            }),
        )?;

        Ok(self.get_ide_profile_policy())
    }

    pub fn get_gate_policy(&self) -> Value {
        json!({
            "strict_artifacts": self.state.strict_gate_artifacts
        })
    }

    pub fn set_gate_policy(&mut self, strict_artifacts: Option<bool>) -> Result<Value> {
        if let Some(v) = strict_artifacts {
            self.state.strict_gate_artifacts = v;
            self.append_audit(
                "gate.policy_set",
                json!({
                    "strict_artifacts": self.state.strict_gate_artifacts
                }),
            )?;
        }
        Ok(self.get_gate_policy())
    }

    pub fn get_audit_rotation_policy(&self) -> Value {
        json!({
            "enabled": self.state.audit_auto_rotate_enabled,
            "max_bytes": self.state.audit_auto_rotate_max_bytes,
            "max_age_sec": self.state.audit_auto_rotate_max_age_sec,
            "compress": self.state.audit_auto_rotate_compress,
            "keep_last": self.state.audit_auto_rotate_keep_last,
            "archive_dir": self.state.audit_archive_dir,
            "last_rotation_unix": self.state.audit_last_rotation_unix
        })
    }

    pub fn set_audit_rotation_policy(
        &mut self,
        enabled: Option<bool>,
        max_bytes: Option<u64>,
        max_age_sec: Option<u64>,
        compress: Option<bool>,
        keep_last: Option<u64>,
        archive_dir: Option<String>,
    ) -> Result<Value> {
        if let Some(v) = enabled {
            self.state.audit_auto_rotate_enabled = v;
        }
        if let Some(v) = max_bytes {
            if v == 0 {
                bail!("max_bytes must be > 0");
            }
            self.state.audit_auto_rotate_max_bytes = v;
        }
        if let Some(v) = max_age_sec {
            if v == 0 {
                bail!("max_age_sec must be > 0");
            }
            self.state.audit_auto_rotate_max_age_sec = v;
        }
        if let Some(v) = compress {
            self.state.audit_auto_rotate_compress = v;
        }
        if let Some(v) = keep_last {
            if v == 0 {
                bail!("keep_last must be > 0");
            }
            self.state.audit_auto_rotate_keep_last = v;
        }
        if let Some(v) = archive_dir {
            let normalized = v.trim().replace('\\', "/");
            if normalized.is_empty() {
                bail!("archive_dir is required");
            }
            let repo_root = self.repo_root();
            let _ = core_resolve_safe_repo_path(&repo_root, &normalized)?;
            self.state.audit_archive_dir = normalized;
        }
        if self.state.audit_last_rotation_unix == 0 {
            self.state.audit_last_rotation_unix = now_unix()?;
        }

        self.append_audit(
            "audit.rotation_policy_changed",
            json!({
                "enabled": self.state.audit_auto_rotate_enabled,
                "max_bytes": self.state.audit_auto_rotate_max_bytes,
                "max_age_sec": self.state.audit_auto_rotate_max_age_sec,
                "compress": self.state.audit_auto_rotate_compress,
                "keep_last": self.state.audit_auto_rotate_keep_last,
                "archive_dir": self.state.audit_archive_dir
            }),
        )?;
        Ok(self.get_audit_rotation_policy())
    }

    pub fn set_consult_mode(&mut self, mode: &str) -> Result<Value> {
        self.state.consult_mode = match mode {
            "USER_TRACKING" | "user_tracking" => ConsultMode::UserTracking,
            "YOLO" | "yolo" => ConsultMode::Yolo,
            _ => bail!("unsupported mode: {mode}"),
        };
        self.append_audit(
            "consult_mode.changed",
            json!({"mode": self.state.consult_mode}),
        )?;
        Ok(json!({"consult_mode": self.state.consult_mode}))
    }

    pub fn get_consult_routing(&self) -> Value {
        json!({
            "consult_mode": self.state.consult_mode,
            "routing_map": self.state.consult_routing_map,
            "priority_timeouts": self.state.consult_priority_timeouts,
            "retry_limits": self.state.consult_retry_limits,
            "escalation_targets": self.state.consult_escalation_targets,
            "allowed_roles": self.state.consult_allowed_roles,
            "guard_policy": {
                "require_cross_rules_ack": self.state.consult_require_cross_rules_ack,
                "required_evidence_ids": self.state.consult_required_evidence_ids
            },
            "adaptive_router": {
                "enabled": self.state.adaptive_router_enabled,
                "confidence_floor": self.state.adaptive_confidence_floor,
                "exploration_rate": self.state.adaptive_exploration_rate,
                "exploration_min_samples": self.state.adaptive_exploration_min_samples,
                "feedback_profiles_total": self.state.consult_executor_telemetry.len()
            }
        })
    }

    pub fn classify_task(
        &self,
        question: &str,
        explicit_task_type: Option<String>,
    ) -> Result<Value> {
        if question.trim().is_empty() {
            bail!("question must not be empty");
        }
        let classification = classify_task_from_text(question, explicit_task_type.as_deref())?;
        let budget =
            resolve_budget_profile(&self.state.task_budget_profiles, &classification.risk)?;
        Ok(json!({
            "classification": classification,
            "budget_profile": budget
        }))
    }

    pub fn get_budget_policy(&self) -> Value {
        json!({
            "profiles": self.state.task_budget_profiles
        })
    }

    pub fn set_budget_policy(
        &mut self,
        risk: String,
        max_steps: Option<u64>,
        max_tool_calls: Option<u64>,
        max_runtime_sec: Option<u64>,
    ) -> Result<Value> {
        let risk = normalize_task_risk(Some(risk.as_str()))?;
        let mut profile = resolve_budget_profile(&self.state.task_budget_profiles, &risk)?;
        if let Some(v) = max_steps {
            if v == 0 {
                bail!("max_steps must be > 0");
            }
            profile.max_steps = v;
        }
        if let Some(v) = max_tool_calls {
            if v == 0 {
                bail!("max_tool_calls must be > 0");
            }
            profile.max_tool_calls = v;
        }
        if let Some(v) = max_runtime_sec {
            if v == 0 {
                bail!("max_runtime_sec must be > 0");
            }
            profile.max_runtime_sec = v;
        }
        self.state
            .task_budget_profiles
            .insert(risk.clone(), profile.clone());
        self.append_audit(
            "task.budget_policy_set",
            json!({
                "risk": risk,
                "profile": profile
            }),
        )?;
        Ok(self.get_budget_policy())
    }

    pub fn plan_task_execution(
        &mut self,
        question: &str,
        explicit_task_type: Option<String>,
        priority: Option<&str>,
    ) -> Result<Value> {
        if question.trim().is_empty() {
            bail!("question must not be empty");
        }
        let classification = classify_task_from_text(question, explicit_task_type.as_deref())?;
        let priority = core_normalize_consult_priority(priority.unwrap_or("normal"))?;
        let base_budget =
            resolve_budget_profile(&self.state.task_budget_profiles, &classification.risk)?;
        let planned_budget = scale_budget_for_priority(&base_budget, &priority)?;
        self.append_audit(
            "task.planned",
            json!({
                "task_type": classification.task_type,
                "risk": classification.risk,
                "priority": priority,
                "confidence": classification.confidence,
                "keywords": classification.keywords,
                "budget": planned_budget
            }),
        )?;
        Ok(json!({
            "classification": classification,
            "priority": priority,
            "budget": planned_budget
        }))
    }

    pub fn get_patch_gate_policy(&self) -> Value {
        json!(self.state.patch_gate_policy)
    }

    pub fn set_patch_gate_policy(
        &mut self,
        require_review_on_unsafe: Option<bool>,
        require_review_on_build_scripts: Option<bool>,
        deny_on_secrets: Option<bool>,
        max_auto_apply_files: Option<u64>,
    ) -> Result<Value> {
        if let Some(v) = require_review_on_unsafe {
            self.state.patch_gate_policy.require_review_on_unsafe = v;
        }
        if let Some(v) = require_review_on_build_scripts {
            self.state.patch_gate_policy.require_review_on_build_scripts = v;
        }
        if let Some(v) = deny_on_secrets {
            self.state.patch_gate_policy.deny_on_secrets = v;
        }
        if let Some(v) = max_auto_apply_files {
            if v == 0 {
                bail!("max_auto_apply_files must be > 0");
            }
            self.state.patch_gate_policy.max_auto_apply_files = v;
        }
        self.append_audit(
            "patch_gate.policy_set",
            json!({
                "policy": self.state.patch_gate_policy
            }),
        )?;
        Ok(self.get_patch_gate_policy())
    }

    pub fn evaluate_patch_gate(
        &mut self,
        files: Vec<String>,
        task_risk: Option<&str>,
        touches_unsafe: Option<bool>,
        touches_build_scripts: Option<bool>,
        touches_secrets: Option<bool>,
        tests_passed: Option<bool>,
    ) -> Result<Value> {
        if files.is_empty() {
            bail!("files must not be empty");
        }
        let mut normalized_files: Vec<String> = files
            .into_iter()
            .map(|x| x.trim().replace('\\', "/"))
            .filter(|x| !x.is_empty())
            .collect();
        if normalized_files.is_empty() {
            bail!("files must not be empty");
        }
        normalized_files.sort();
        normalized_files.dedup();

        let inferred_unsafe = detect_unsafe_files(&normalized_files);
        let inferred_build_scripts = detect_build_script_files(&normalized_files);
        let inferred_secrets = detect_secret_files(&normalized_files);
        let touches_unsafe = touches_unsafe.unwrap_or(inferred_unsafe);
        let touches_build_scripts = touches_build_scripts.unwrap_or(inferred_build_scripts);
        let touches_secrets = touches_secrets.unwrap_or(inferred_secrets);
        let task_risk = normalize_task_risk(task_risk)?;

        let mut mode = "auto_apply";
        let mut reasons: Vec<String> = Vec::new();
        if self.state.patch_gate_policy.deny_on_secrets && touches_secrets {
            mode = "deny";
            reasons.push("secrets_change_blocked".to_string());
        } else {
            if touches_unsafe && self.state.patch_gate_policy.require_review_on_unsafe {
                mode = "require_confirmation";
                reasons.push("unsafe_changes_require_review".to_string());
            }
            if touches_build_scripts && self.state.patch_gate_policy.require_review_on_build_scripts
            {
                mode = "require_confirmation";
                reasons.push("build_or_pipeline_changes_require_review".to_string());
            }
            if normalized_files.len() as u64 > self.state.patch_gate_policy.max_auto_apply_files {
                if mode == "auto_apply" {
                    mode = "suggest_only";
                }
                reasons.push("change_set_too_large".to_string());
            }
            match task_risk.as_str() {
                "critical" => {
                    if mode == "auto_apply" {
                        mode = "require_confirmation";
                    }
                    reasons.push("critical_task_risk".to_string());
                }
                "high" => {
                    if mode == "auto_apply" {
                        mode = "suggest_only";
                    }
                    reasons.push("high_task_risk".to_string());
                }
                _ => {}
            }
            if matches!(tests_passed, Some(false)) {
                if mode == "auto_apply" {
                    mode = "suggest_only";
                }
                reasons.push("tests_not_passed".to_string());
            }
        }

        let allow = mode != "deny";
        let requires_confirmation = mode == "require_confirmation";
        let out = json!({
            "allow": allow,
            "mode": mode,
            "requires_confirmation": requires_confirmation,
            "task_risk": task_risk,
            "tests_passed": tests_passed,
            "changed_files_total": normalized_files.len(),
            "flags": {
                "touches_unsafe": touches_unsafe,
                "touches_build_scripts": touches_build_scripts,
                "touches_secrets": touches_secrets
            },
            "reasons": reasons,
            "policy": self.state.patch_gate_policy
        });
        self.append_audit("patch_gate.evaluated", out.clone())?;
        Ok(out)
    }

    pub fn get_consult_guard_policy(&self) -> Value {
        json!({
            "require_cross_rules_ack": self.state.consult_require_cross_rules_ack,
            "required_evidence_ids": self.state.consult_required_evidence_ids
        })
    }

    pub fn set_consult_guard_policy(
        &mut self,
        require_cross_rules_ack: Option<bool>,
        required_evidence_ids: Option<Vec<String>>,
    ) -> Result<Value> {
        if let Some(v) = require_cross_rules_ack {
            self.state.consult_require_cross_rules_ack = v;
        }
        if let Some(ids) = required_evidence_ids {
            self.state.consult_required_evidence_ids = normalize_evidence_ids(ids)?;
        }
        self.append_audit(
            "consult.guard_policy_set",
            json!({
                "require_cross_rules_ack": self.state.consult_require_cross_rules_ack,
                "required_evidence_ids": self.state.consult_required_evidence_ids
            }),
        )?;
        Ok(self.get_consult_guard_policy())
    }

    pub fn get_cross_rules_status(&self) -> Value {
        let entry_required = cross_rules_required_evidence_ids();
        let consult_required = self.state.consult_required_evidence_ids.clone();
        let entry_missing = missing_evidence_ids(&self.state.evidence, &entry_required);
        let consult_missing = missing_evidence_ids(&self.state.evidence, &consult_required);
        let entry_all_present = entry_missing.is_empty();
        let consult_all_present = consult_missing.is_empty();

        json!({
            "entry_gate_required_evidence_ids": entry_required,
            "entry_gate_missing_evidence_ids": entry_missing,
            "entry_gate_all_present": entry_all_present,
            "consult_guard": {
                "enabled": self.state.consult_require_cross_rules_ack,
                "required_evidence_ids": consult_required,
                "missing_evidence_ids": consult_missing,
                "all_present": consult_all_present
            }
        })
    }

    pub fn ack_cross_rules(
        &mut self,
        agent_ack_path: String,
        subagent_ack_path: String,
        enable_consult_guard: Option<bool>,
    ) -> Result<Value> {
        let agent_ack_path = agent_ack_path.trim().to_string();
        let subagent_ack_path = subagent_ack_path.trim().to_string();
        if agent_ack_path.is_empty() || subagent_ack_path.is_empty() {
            bail!("agent_ack_path and subagent_ack_path are required");
        }

        self.register_evidence("cross_rules_agent_ack".to_string(), agent_ack_path)?;
        self.register_evidence("cross_rules_subagent_ack".to_string(), subagent_ack_path)?;

        let enable_guard = enable_consult_guard.unwrap_or(true);
        if enable_guard {
            self.state.consult_require_cross_rules_ack = true;
            let mut merged = self.state.consult_required_evidence_ids.clone();
            for required in cross_rules_required_evidence_ids() {
                if !merged.contains(&required) {
                    merged.push(required);
                }
            }
            self.state.consult_required_evidence_ids = merged;
        }

        self.append_audit(
            "cross_rules.acknowledged",
            json!({
                "consult_guard_enabled": self.state.consult_require_cross_rules_ack,
                "consult_required_evidence_ids": self.state.consult_required_evidence_ids
            }),
        )?;

        Ok(self.get_cross_rules_status())
    }

    pub fn set_consult_routing_rule(
        &mut self,
        consult_type: String,
        executor: String,
    ) -> Result<Value> {
        let consult_type = consult_type.trim().to_ascii_lowercase();
        let executor = executor.trim().to_ascii_lowercase();
        if consult_type.is_empty() || executor.is_empty() {
            bail!("consult_type and executor are required");
        }
        self.state
            .consult_routing_map
            .insert(consult_type.clone(), executor.clone());
        self.append_audit(
            "consult.routing_rule_set",
            json!({"consult_type": consult_type, "executor": executor}),
        )?;
        Ok(self.get_consult_routing())
    }

    pub fn set_consult_priority_timeout(
        &mut self,
        priority: String,
        timeout_sec: u64,
    ) -> Result<Value> {
        let priority = core_normalize_consult_priority(&priority)?;
        if timeout_sec == 0 {
            bail!("timeout_sec must be > 0");
        }
        self.state
            .consult_priority_timeouts
            .insert(priority.clone(), timeout_sec);
        self.append_audit(
            "consult.priority_timeout_set",
            json!({"priority": priority, "timeout_sec": timeout_sec}),
        )?;
        Ok(self.get_consult_routing())
    }

    pub fn set_consult_retry_limit(&mut self, priority: String, max_retries: u64) -> Result<Value> {
        let priority = core_normalize_consult_priority(&priority)?;
        if max_retries > 10 {
            bail!("max_retries must be <= 10");
        }
        self.state
            .consult_retry_limits
            .insert(priority.clone(), max_retries);
        self.append_audit(
            "consult.retry_limit_set",
            json!({"priority": priority, "max_retries": max_retries}),
        )?;
        Ok(self.get_consult_routing())
    }

    pub fn set_consult_escalation_target(
        &mut self,
        priority: String,
        target: String,
    ) -> Result<Value> {
        let priority = core_normalize_consult_priority(&priority)?;
        let target = core_normalize_escalation_target(&target)?;
        self.state
            .consult_escalation_targets
            .insert(priority.clone(), target.clone());
        self.append_audit(
            "consult.escalation_target_set",
            json!({"priority": priority, "target": target}),
        )?;
        Ok(self.get_consult_routing())
    }

    pub fn set_consult_allowed_roles(
        &mut self,
        consult_type: String,
        roles: Vec<String>,
    ) -> Result<Value> {
        let consult_type = consult_type.trim().to_ascii_lowercase();
        if consult_type.is_empty() {
            bail!("consult_type is required");
        }
        if roles.is_empty() {
            bail!("roles must not be empty");
        }
        let mut normalized = Vec::new();
        for role in roles {
            let role = role.trim().to_ascii_lowercase();
            if !role.is_empty() {
                normalized.push(role);
            }
        }
        if normalized.is_empty() {
            bail!("roles must not be empty");
        }
        normalized.sort();
        normalized.dedup();
        self.state
            .consult_allowed_roles
            .insert(consult_type.clone(), normalized.clone());
        self.append_audit(
            "consult.allowed_roles_set",
            json!({"consult_type": consult_type, "roles": normalized}),
        )?;
        Ok(self.get_consult_routing())
    }

    pub fn get_adaptive_router(&self) -> Value {
        json!({
            "enabled": self.state.adaptive_router_enabled,
            "confidence_floor": self.state.adaptive_confidence_floor,
            "exploration_rate": self.state.adaptive_exploration_rate,
            "exploration_min_samples": self.state.adaptive_exploration_min_samples,
            "feedback_profiles_total": self.state.consult_executor_telemetry.len(),
            "feedback": self.state.consult_executor_telemetry
        })
    }

    pub fn set_adaptive_router(
        &mut self,
        enabled: Option<bool>,
        confidence_floor: Option<f64>,
    ) -> Result<Value> {
        if let Some(value) = enabled {
            self.state.adaptive_router_enabled = value;
        }
        if let Some(floor) = confidence_floor {
            if !floor.is_finite() || !(0.0..=1.0).contains(&floor) {
                bail!("confidence_floor must be in [0,1]");
            }
            self.state.adaptive_confidence_floor = floor;
        }
        self.append_audit(
            "consult.adaptive_router_set",
            json!({
                "enabled": self.state.adaptive_router_enabled,
                "confidence_floor": self.state.adaptive_confidence_floor
            }),
        )?;
        Ok(self.get_adaptive_router())
    }

    pub fn set_adaptive_exploration_policy(
        &mut self,
        exploration_rate: Option<f64>,
        exploration_min_samples: Option<u64>,
    ) -> Result<Value> {
        if let Some(rate) = exploration_rate {
            if !rate.is_finite() || !(0.0..=1.0).contains(&rate) {
                bail!("exploration_rate must be in [0,1]");
            }
            self.state.adaptive_exploration_rate = rate;
        }
        if let Some(samples) = exploration_min_samples {
            if samples == 0 {
                bail!("exploration_min_samples must be > 0");
            }
            self.state.adaptive_exploration_min_samples = samples;
        }
        self.append_audit(
            "consult.adaptive_exploration_policy_set",
            json!({
                "exploration_rate": self.state.adaptive_exploration_rate,
                "exploration_min_samples": self.state.adaptive_exploration_min_samples
            }),
        )?;
        Ok(self.get_adaptive_router())
    }

    pub fn record_consult_feedback(
        &mut self,
        request_id: Option<String>,
        consult_type: String,
        executor: String,
        success: bool,
        latency_ms: Option<u64>,
    ) -> Result<Value> {
        let consult_type = consult_type.trim().to_ascii_lowercase();
        let executor = executor.trim().to_ascii_lowercase();
        if consult_type.is_empty() || executor.is_empty() {
            bail!("consult_type and executor are required for feedback");
        }
        if matches!(latency_ms, Some(0)) {
            bail!("latency_ms must be > 0");
        }
        let key = consult_feedback_key(&consult_type, &executor);
        let ts_unix = now_unix()?;
        let metrics = self
            .state
            .consult_executor_telemetry
            .entry(key.clone())
            .or_default();
        metrics.total_feedback = metrics.total_feedback.saturating_add(1);
        if success {
            metrics.successes = metrics.successes.saturating_add(1);
        } else {
            metrics.failures = metrics.failures.saturating_add(1);
        }
        if let Some(ms) = latency_ms {
            let total = metrics
                .avg_latency_ms
                .saturating_mul(metrics.latency_samples)
                .saturating_add(ms);
            metrics.latency_samples = metrics.latency_samples.saturating_add(1);
            metrics.avg_latency_ms = total / metrics.latency_samples.max(1);
        }
        metrics.last_ts_unix = ts_unix;
        let telemetry = metrics.clone();
        self.append_audit(
            "consult.feedback_recorded",
            json!({
                "request_id": request_id,
                "consult_type": consult_type,
                "executor": executor,
                "success": success,
                "latency_ms": latency_ms,
                "feedback_key": key,
                "telemetry": telemetry
            }),
        )?;
        Ok(json!({
            "feedback_key": key,
            "telemetry": telemetry
        }))
    }

    pub fn apply_policy(
        &mut self,
        cpu: &CpuProfile,
        expected_revision: u64,
        version: String,
        rules: Vec<String>,
        signature: Option<String>,
        key_id: Option<String>,
        nonce: Option<String>,
        forbidden_tokens: Vec<String>,
    ) -> Result<Value> {
        if rules.is_empty() {
            bail!("rules must not be empty");
        }
        if expected_revision != self.state.policy_revision {
            bail!(
                "policy revision mismatch: expected={} actual={}",
                expected_revision,
                self.state.policy_revision
            );
        }
        let next_revision = self.state.policy_revision + 1;
        let mut resolved_key_id: Option<String> = None;
        if self.state.require_signed_policy {
            let nonce = nonce
                .as_deref()
                .ok_or_else(|| anyhow!("nonce is required when signed policy is enabled"))?;
            let signature = signature
                .as_deref()
                .ok_or_else(|| anyhow!("signature is required when signed policy is enabled"))?;
            let now = now_unix()?;
            let selected_key_id = core_verify_policy_signature(
                &self.state.policy_signing_keys,
                &self.state.active_policy_key_id,
                &self.state.used_policy_nonces,
                &version,
                next_revision,
                &rules,
                &forbidden_tokens,
                key_id.as_deref(),
                nonce,
                signature,
                now,
            )?;
            core_register_policy_nonce(
                &mut self.state.used_policy_nonces,
                &selected_key_id,
                nonce,
            )?;
            resolved_key_id = Some(selected_key_id);
        }
        self.state.policy = PolicyBundle {
            version,
            revision: next_revision,
            rules,
            signature,
            forbidden_tokens,
        };
        self.state.policy_revision = next_revision;
        self.state.policy_hash =
            cpu.hash_bytes(serde_json::to_string(&self.state.policy)?.as_bytes());
        self.append_audit(
            "policy.applied",
            json!({
                "revision": self.state.policy_revision,
                "version": self.state.policy.version,
                "policy_hash": self.state.policy_hash,
                "key_id": resolved_key_id
            }),
        )?;
        Ok(json!({
            "policy_version": self.state.policy.version,
            "policy_revision": self.state.policy_revision,
            "policy_hash": self.state.policy_hash,
            "key_id": resolved_key_id
        }))
    }

    pub fn set_policy_security(&mut self, require_signed_policy: Option<bool>) -> Result<Value> {
        if let Some(v) = require_signed_policy {
            self.state.require_signed_policy = v;
        }
        self.append_audit(
            "policy.security_changed",
            json!({"require_signed_policy": self.state.require_signed_policy}),
        )?;
        Ok(json!({
            "require_signed_policy": self.state.require_signed_policy
        }))
    }

    pub fn list_policy_signing_keys(&self) -> Value {
        json!({
            "active_policy_key_id": self.state.active_policy_key_id,
            "keys": self.state.policy_signing_keys
        })
    }

    pub fn upsert_policy_signing_key(
        &mut self,
        key_id: String,
        key_env: String,
        not_before_unix: Option<u64>,
        not_after_unix: Option<u64>,
        set_active: Option<bool>,
    ) -> Result<Value> {
        if key_id.trim().is_empty() || key_env.trim().is_empty() {
            bail!("key_id and key_env are required");
        }
        if let (Some(nb), Some(na)) = (not_before_unix, not_after_unix) {
            if nb > na {
                bail!("not_before_unix must be <= not_after_unix");
            }
        }
        let mut found = false;
        for item in &mut self.state.policy_signing_keys {
            if item.key_id == key_id {
                item.key_env = key_env.clone();
                item.algorithm = default_policy_signing_algorithm();
                item.not_before_unix = not_before_unix;
                item.not_after_unix = not_after_unix;
                item.revoked_at_unix = None;
                found = true;
                break;
            }
        }
        if !found {
            self.state.policy_signing_keys.push(PolicySigningKey {
                key_id: key_id.clone(),
                algorithm: default_policy_signing_algorithm(),
                key_env: key_env.clone(),
                not_before_unix,
                not_after_unix,
                revoked_at_unix: None,
            });
        }
        if set_active.unwrap_or(false) {
            self.state.active_policy_key_id = key_id.clone();
        }
        self.append_audit(
            "policy.signing_key_upserted",
            json!({
                "key_id": key_id,
                "key_env": key_env,
                "not_before_unix": not_before_unix,
                "not_after_unix": not_after_unix,
                "set_active": set_active.unwrap_or(false)
            }),
        )?;
        Ok(self.list_policy_signing_keys())
    }

    pub fn set_active_policy_signing_key(&mut self, key_id: String) -> Result<Value> {
        let key = self
            .state
            .policy_signing_keys
            .iter()
            .find(|x| x.key_id == key_id)
            .ok_or_else(|| anyhow!("unknown key_id: {key_id}"))?;
        if key.revoked_at_unix.is_some() {
            bail!("cannot activate revoked key: {key_id}");
        }
        self.state.active_policy_key_id = key_id.clone();
        self.append_audit("policy.active_key_changed", json!({"key_id": key_id}))?;
        Ok(self.list_policy_signing_keys())
    }

    pub fn revoke_policy_signing_key(&mut self, key_id: String) -> Result<Value> {
        let now = now_unix()?;
        let mut found = false;
        for item in &mut self.state.policy_signing_keys {
            if item.key_id == key_id {
                item.revoked_at_unix = Some(now);
                found = true;
                break;
            }
        }
        if !found {
            bail!("unknown key_id: {key_id}");
        }
        if self.state.active_policy_key_id == key_id {
            let replacement = self
                .state
                .policy_signing_keys
                .iter()
                .find(|x| x.key_id != key_id && x.revoked_at_unix.is_none())
                .map(|x| x.key_id.clone())
                .ok_or_else(|| anyhow!("cannot revoke the only active non-revoked key"))?;
            self.state.active_policy_key_id = replacement;
        }
        self.append_audit(
            "policy.signing_key_revoked",
            json!({"key_id": key_id, "revoked_at_unix": now}),
        )?;
        Ok(self.list_policy_signing_keys())
    }

    pub fn guard_action(&self, agent: &str, action: &str) -> Result<Value> {
        if agent.trim().is_empty() || action.trim().is_empty() {
            bail!("agent and action are required");
        }
        let lower = action.to_ascii_lowercase();
        let violation = self
            .state
            .policy
            .forbidden_tokens
            .iter()
            .find(|x| lower.contains(&x.to_ascii_lowercase()))
            .cloned();

        if let Some(token) = violation {
            Ok(json!({
                "allow": false,
                "reason": format!("action contains forbidden token: {token}")
            }))
        } else {
            Ok(json!({
                "allow": true,
                "reason": "policy passed"
            }))
        }
    }

    pub fn proxy_request(&self, category: &str, operation: &str, target: &str) -> Result<Value> {
        core_evaluate_proxy_request(
            &self.state.proxy_allow,
            &self.state.proxy_allowed_operations,
            &self.state.proxy_denied_operations,
            self.state.proxy_deny_by_default,
            category,
            operation,
            target,
        )
    }

    pub fn set_proxy_policy(
        &mut self,
        deny_by_default: Option<bool>,
        category: Option<String>,
        allow_prefixes: Option<Vec<String>>,
    ) -> Result<Value> {
        if let Some(v) = deny_by_default {
            self.state.proxy_deny_by_default = v;
        }
        if let (Some(cat), Some(prefixes)) = (category, allow_prefixes) {
            let cat = cat.trim().to_ascii_lowercase();
            if cat.is_empty() {
                bail!("category is required");
            }
            let normalized = prefixes
                .into_iter()
                .map(|x| x.trim().replace('\\', "/"))
                .filter(|x| !x.is_empty())
                .collect::<Vec<_>>();
            self.state.proxy_allow.insert(cat, normalized);
        }
        self.append_audit(
            "proxy.policy_changed",
            json!({
                "proxy_deny_by_default": self.state.proxy_deny_by_default,
                "proxy_allow": self.state.proxy_allow,
                "proxy_allowed_operations": self.state.proxy_allowed_operations,
                "proxy_denied_operations": self.state.proxy_denied_operations
            }),
        )?;
        Ok(json!({
            "proxy_deny_by_default": self.state.proxy_deny_by_default,
            "proxy_allow": self.state.proxy_allow,
            "proxy_allowed_operations": self.state.proxy_allowed_operations,
            "proxy_denied_operations": self.state.proxy_denied_operations
        }))
    }

    pub fn get_proxy_operation_policy(&self) -> Value {
        json!({
            "allowed_operations": self.state.proxy_allowed_operations,
            "denied_operations": self.state.proxy_denied_operations
        })
    }

    pub fn set_proxy_operation_policy(
        &mut self,
        category: String,
        allowed_operations: Option<Vec<String>>,
        denied_operations: Option<Vec<String>>,
    ) -> Result<Value> {
        if allowed_operations.is_none() && denied_operations.is_none() {
            bail!("allowed_operations or denied_operations is required");
        }
        let category = category.trim().to_ascii_lowercase();
        if category.is_empty() {
            bail!("category is required");
        }

        if let Some(ops) = allowed_operations {
            let normalized = normalize_proxy_operations(ops)?;
            self.state
                .proxy_allowed_operations
                .insert(category.clone(), normalized);
        }

        if let Some(ops) = denied_operations {
            let normalized = normalize_proxy_operations(ops)?;
            self.state
                .proxy_denied_operations
                .insert(category.clone(), normalized);
        }

        self.append_audit(
            "proxy.operation_policy_changed",
            json!({
                "category": category,
                "allowed_operations": self.state.proxy_allowed_operations.get(&category),
                "denied_operations": self.state.proxy_denied_operations.get(&category)
            }),
        )?;

        Ok(self.get_proxy_operation_policy())
    }

    pub fn get_proxy_log(&self, limit: Option<usize>) -> Result<Value> {
        let n =
            normalize_limit(limit, 50, PROXY_LOG_RESULT_MAX_LIMIT)?.min(self.state.proxy_log.len());
        let start = self.state.proxy_log.len().saturating_sub(n);
        let slice = &self.state.proxy_log[start..];
        Ok(json!({
            "total": self.state.proxy_log.len(),
            "max_limit": PROXY_LOG_RESULT_MAX_LIMIT,
            "items": slice
        }))
    }

    pub fn get_audit_log(&self, limit: Option<usize>) -> Result<Value> {
        self.query_audit_log(None, None, None, None, None, None, limit)
    }

    pub fn validate_error_codes_parity(&self, doc_path: Option<String>) -> Result<Value> {
        let rel_path = doc_path.unwrap_or_else(|| "spec/docs/CABAL_ERROR_CODES.md".to_string());
        let repo_root = self.repo_root();
        let path = core_resolve_safe_repo_path(&repo_root, &rel_path)?;
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read error codes doc: {}", path.display()))?;
        let report = validate_error_codes_doc_parity(&text)?;
        Ok(json!({
            "path": rel_path,
            "report": report
        }))
    }

    pub fn query_audit_log(
        &self,
        kind: Option<String>,
        phase: Option<String>,
        policy_revision: Option<u64>,
        request_id: Option<String>,
        from_ts_unix: Option<u64>,
        to_ts_unix: Option<u64>,
        limit: Option<usize>,
    ) -> Result<Value> {
        let bounded_limit = Some(normalize_limit(limit, 100, AUDIT_QUERY_MAX_LIMIT)?);
        let items = self.read_audit_items();
        let mut out = core_query_audit_items(
            &items,
            &CoreAuditQuery {
                kind,
                phase,
                policy_revision,
                request_id,
                from_ts_unix,
                to_ts_unix,
                limit: bounded_limit,
            },
        )?;
        if let Some(obj) = out.as_object_mut() {
            obj.insert("max_limit".to_string(), json!(AUDIT_QUERY_MAX_LIMIT));
        }
        Ok(out)
    }

    pub fn export_audit_log(
        &self,
        out_path: String,
        kind: Option<String>,
        phase: Option<String>,
        policy_revision: Option<u64>,
        request_id: Option<String>,
        from_ts_unix: Option<u64>,
        to_ts_unix: Option<u64>,
        limit: Option<usize>,
    ) -> Result<Value> {
        let applied_limit = normalize_limit(limit, 100, AUDIT_QUERY_MAX_LIMIT)?;
        let query = self.query_audit_log(
            kind,
            phase,
            policy_revision,
            request_id,
            from_ts_unix,
            to_ts_unix,
            Some(applied_limit),
        )?;
        let items = query
            .get("items")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow!("query_audit_log response missing items"))?;

        let repo_root = self.repo_root();
        let target = core_resolve_safe_repo_path(&repo_root, &out_path)?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create export dir: {}", parent.display()))?;
        }

        let mut f = fs::File::create(&target)
            .with_context(|| format!("failed to create export file: {}", target.display()))?;
        for item in items {
            f.write_all(serde_json::to_string(item)?.as_bytes())
                .context("failed to write audit export record")?;
            f.write_all(b"\n")
                .context("failed to write audit export newline")?;
        }

        Ok(json!({
            "exported": items.len(),
            "path": out_path,
            "requested_limit": limit,
            "applied_limit": applied_limit,
            "max_limit": AUDIT_QUERY_MAX_LIMIT
        }))
    }

    pub fn replay_audit_state(
        &self,
        upto_event_id: Option<String>,
        upto_ts_unix: Option<u64>,
    ) -> Result<Value> {
        let items = self.read_audit_items();
        Ok(core_replay_audit_items(
            &items,
            upto_event_id,
            upto_ts_unix,
            &self.state.phase,
            self.state.policy_revision,
            consult_mode_as_str(&self.state.consult_mode),
        ))
    }

    pub fn rotate_audit_log(
        &mut self,
        archive_dir: Option<String>,
        compress: Option<bool>,
    ) -> Result<Value> {
        let repo_root = self.repo_root();
        let archive_dir = archive_dir.unwrap_or_else(|| self.state.audit_archive_dir.clone());
        let archive_path = core_resolve_safe_repo_path(&repo_root, &archive_dir)?;
        let mut rotated = core_rotate_audit_log(
            &self.audit_path,
            &archive_path,
            compress.unwrap_or(self.state.audit_auto_rotate_compress),
        )?;
        if let Some(obj) = rotated.as_object_mut() {
            if let Some(archive_abs) = obj.get("archive_path").and_then(|x| x.as_str()) {
                let rel = repo_relative_path_string(&repo_root, Path::new(archive_abs));
                obj.insert("archive_path".to_string(), Value::String(rel));
            }
            if let Some(signature_abs) = obj.get("signature_path").and_then(|x| x.as_str()) {
                let rel = repo_relative_path_string(&repo_root, Path::new(signature_abs));
                obj.insert("signature_path".to_string(), Value::String(rel));
            }
        }
        self.state.audit_last_rotation_unix = now_unix()?;
        let pruned = core_prune_audit_archives(
            &archive_path,
            self.state.audit_auto_rotate_keep_last as usize,
        )?;
        self.append_audit(
            "audit.rotated",
            json!({
                "trigger": "manual",
                "archive": rotated,
                "prune": pruned
            }),
        )?;
        Ok(json!({
            "archive": rotated,
            "prune": pruned
        }))
    }

    pub fn verify_audit_archive(
        &self,
        archive_path: String,
        signature_path: Option<String>,
    ) -> Result<Value> {
        let repo_root = self.repo_root();
        let archive_path = core_resolve_safe_repo_path(&repo_root, &archive_path)?;
        let signature_path = signature_path
            .map(|x| core_resolve_safe_repo_path(&repo_root, &x))
            .transpose()?;
        core_verify_audit_archive(&archive_path, signature_path.as_deref())
    }

    pub fn prune_audit_archives(
        &mut self,
        archive_dir: Option<String>,
        keep_last: Option<u64>,
    ) -> Result<Value> {
        let repo_root = self.repo_root();
        let archive_dir = archive_dir.unwrap_or_else(|| self.state.audit_archive_dir.clone());
        let archive_path = core_resolve_safe_repo_path(&repo_root, &archive_dir)?;
        let keep_last = keep_last.unwrap_or(self.state.audit_auto_rotate_keep_last) as usize;
        let result = core_prune_audit_archives(&archive_path, keep_last)?;
        self.append_audit(
            "audit.archives_pruned",
            json!({
                "archive_dir": archive_dir,
                "keep_last": keep_last,
                "result": result
            }),
        )?;
        Ok(result)
    }

    pub fn audit_health_check(
        &self,
        archive_dir: Option<String>,
        verify_last: Option<u64>,
    ) -> Result<Value> {
        let verify_last = verify_last.unwrap_or(5) as usize;
        if verify_last == 0 {
            bail!("verify_last must be > 0");
        }

        let repo_root = self.repo_root();
        let archive_dir = archive_dir.unwrap_or_else(|| self.state.audit_archive_dir.clone());
        let archive_path = core_resolve_safe_repo_path(&repo_root, &archive_dir)?;

        let active_exists = self.audit_path.exists();
        let active_bytes = fs::metadata(&self.audit_path).map(|m| m.len()).unwrap_or(0);
        let active_items = self.read_audit_items();
        let active_last_event_id = active_items
            .last()
            .and_then(|x| x.get("event_id"))
            .and_then(|x| x.as_str())
            .map(|x| x.to_string());
        let active_last_kind = active_items
            .last()
            .and_then(|x| x.get("kind"))
            .and_then(|x| x.as_str())
            .map(|x| x.to_string());
        let active_last_ts_unix = active_items
            .last()
            .and_then(|x| x.get("ts_unix"))
            .and_then(|x| x.as_u64());

        let mut archives: Vec<(PathBuf, SystemTime, String)> = Vec::new();
        if archive_path.exists() {
            for entry in fs::read_dir(&archive_path)? {
                let entry = entry?;
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let Some(name) = path
                    .file_name()
                    .and_then(|x| x.to_str())
                    .map(|x| x.to_string())
                else {
                    continue;
                };
                if !(name.ends_with(".jsonl") || name.ends_with(".jsonl.gz")) {
                    continue;
                }
                let modified = fs::metadata(&path)
                    .and_then(|m| m.modified())
                    .unwrap_or(UNIX_EPOCH);
                archives.push((path, modified, name));
            }
        }
        archives.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.2.cmp(&a.2)));

        let mut passed = 0usize;
        let mut failed = 0usize;
        let mut missing_signature = 0usize;
        let mut checked_items = Vec::new();
        for (idx, (archive_file, _, archive_name)) in archives.iter().enumerate() {
            if idx >= verify_last {
                break;
            }
            let sidecar_file = archive_file.with_file_name(format!("{archive_name}.sha256"));
            if !sidecar_file.exists() {
                missing_signature += 1;
                checked_items.push(json!({
                    "archive_path": repo_relative_path_string(&repo_root, archive_file),
                    "signature_path": repo_relative_path_string(&repo_root, &sidecar_file),
                    "has_signature": false,
                    "pass": false,
                    "error": "signature sidecar not found"
                }));
                continue;
            }

            match core_verify_audit_archive(archive_file, Some(&sidecar_file)) {
                Ok(mut verify) => {
                    if let Some(obj) = verify.as_object_mut() {
                        if let Some(archive_abs) = obj.get("archive_path").and_then(|x| x.as_str())
                        {
                            obj.insert(
                                "archive_path".to_string(),
                                Value::String(repo_relative_path_string(
                                    &repo_root,
                                    Path::new(archive_abs),
                                )),
                            );
                        }
                        if let Some(signature_abs) =
                            obj.get("signature_path").and_then(|x| x.as_str())
                        {
                            obj.insert(
                                "signature_path".to_string(),
                                Value::String(repo_relative_path_string(
                                    &repo_root,
                                    Path::new(signature_abs),
                                )),
                            );
                        }
                    }
                    let pass = verify
                        .get("pass")
                        .and_then(|x| x.as_bool())
                        .unwrap_or(false);
                    if pass {
                        passed += 1;
                    } else {
                        failed += 1;
                    }
                    checked_items.push(json!({
                        "archive_path": repo_relative_path_string(&repo_root, archive_file),
                        "signature_path": repo_relative_path_string(&repo_root, &sidecar_file),
                        "has_signature": true,
                        "pass": pass,
                        "verify": verify
                    }));
                }
                Err(err) => {
                    failed += 1;
                    checked_items.push(json!({
                        "archive_path": repo_relative_path_string(&repo_root, archive_file),
                        "signature_path": repo_relative_path_string(&repo_root, &sidecar_file),
                        "has_signature": true,
                        "pass": false,
                        "error": err.to_string()
                    }));
                }
            }
        }

        let status = if failed > 0 {
            "fail"
        } else if missing_signature > 0 {
            "warn"
        } else {
            "pass"
        };

        Ok(json!({
            "status": status,
            "active_log": {
                "path": repo_relative_path_string(&repo_root, &self.audit_path),
                "exists": active_exists,
                "bytes": active_bytes,
                "line_count": active_items.len(),
                "last_event_id": active_last_event_id,
                "last_kind": active_last_kind,
                "last_ts_unix": active_last_ts_unix
            },
            "rotation_policy": self.get_audit_rotation_policy(),
            "archives": {
                "archive_dir": archive_dir,
                "total": archives.len(),
                "verify_limit": verify_last,
                "checked": checked_items.len(),
                "passed": passed,
                "failed": failed,
                "missing_signature": missing_signature,
                "items": checked_items
            }
        }))
    }

    pub fn proxy_execute(
        &mut self,
        cpu: &CpuProfile,
        category: &str,
        operation: &str,
        target: &str,
        payload: Value,
    ) -> Result<Value> {
        let decision = self.proxy_request(category, operation, target)?;
        let allow = decision
            .get("allow")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !allow {
            self.append_proxy_trace(
                cpu,
                category,
                operation,
                target,
                false,
                false,
                decision
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("policy deny"),
            )?;
            return Ok(json!({
                "allow": false,
                "executed": false,
                "reason": decision["reason"]
            }));
        }

        let repo_root = self.repo_root();
        let out = match category {
            "fs" => core_exec_fs(&repo_root, operation, target, payload)?,
            "shell" => core_exec_shell(operation, target)?,
            "network" => core_exec_network(operation, target)?,
            _ => bail!("unsupported proxy category: {category}"),
        };

        self.append_proxy_trace(cpu, category, operation, target, true, true, "executed")?;
        Ok(json!({
            "allow": true,
            "executed": true,
            "result": out
        }))
    }

    pub fn transition_phase(&mut self, target: &str) -> Result<Value> {
        let decision = core_transition_phase(&self.state.phase, target)?;
        if !decision.changed {
            return Ok(json!({"phase": self.state.phase, "changed": false}));
        }
        self.state.phase = decision.to_phase.clone();
        self.append_audit(
            "phase.transition",
            json!({"from": decision.from_phase, "to": self.state.phase}),
        )?;
        Ok(json!({"phase": self.state.phase, "changed": true}))
    }

    pub fn gate_check(&self, kind: &str, phase: &str) -> Result<Value> {
        let report = self.build_gate_report(kind, phase)?;
        Ok(serde_json::to_value(report)?)
    }

    pub fn transition_phase_strict(&mut self, target: &str) -> Result<Value> {
        let exit_report = self.build_gate_report("exit", &self.state.phase)?;
        let entry_report = self.build_gate_report("entry", target)?;
        core_validate_strict_phase_transition(
            &self.state.phase,
            target,
            &exit_report,
            &entry_report,
        )?;
        self.transition_phase(target)?;
        Ok(json!({
            "phase": self.state.phase,
            "changed": true,
            "exit_report": exit_report,
            "entry_report": entry_report
        }))
    }

    pub fn route_consult(
        &mut self,
        question: &str,
        consult_type: Option<&str>,
        priority: Option<&str>,
        preferred_role: Option<&str>,
        request_id: Option<&str>,
    ) -> Result<Value> {
        if question.trim().is_empty() {
            bail!("question must not be empty");
        }
        let consult_type = consult_type.unwrap_or("general").to_ascii_lowercase();
        let priority = core_normalize_consult_priority(priority.unwrap_or("normal"))?;
        let task_profile = classify_task_from_text(question, Some(consult_type.as_str()))?;
        let task_budget_base =
            resolve_budget_profile(&self.state.task_budget_profiles, &task_profile.risk)?;
        let task_budget = scale_budget_for_priority(&task_budget_base, &priority)?;
        self.ensure_consult_cross_rules_ack(&consult_type, &priority, request_id)?;
        let timeout_sec =
            core_resolve_consult_timeout(priority.as_str(), &self.state.consult_priority_timeouts);
        let escalation_target = core_resolve_consult_escalation(
            priority.as_str(),
            &self.state.consult_escalation_targets,
        );
        let max_retries =
            core_resolve_consult_retries(priority.as_str(), &self.state.consult_retry_limits);

        let preferred = preferred_role.map(|x| x.to_ascii_lowercase());
        let allowed_roles =
            core_consult_allowed_roles_for_type(&consult_type, &self.state.consult_allowed_roles);
        if allowed_roles.is_empty() {
            bail!("no allowed executor configured for consult_type={consult_type}");
        }
        let mut executor = preferred.clone().unwrap_or_else(|| {
            core_resolve_consult_executor(&consult_type, &self.state.consult_routing_map)
        });
        let mut role_mismatch = false;
        if !core_is_consult_role_allowed(
            &consult_type,
            &executor,
            &self.state.consult_allowed_roles,
        ) {
            role_mismatch = true;
            executor = core_resolve_consult_allowed_role_fallback(
                &consult_type,
                &allowed_roles,
                &self.state.consult_routing_map,
            )
            .ok_or_else(|| {
                anyhow!("no allowed executor configured for consult_type={consult_type}")
            })?;
        }
        let mut routing_strategy = "policy".to_string();
        let mut routing_score: Option<f64> = None;
        let mut routing_confidence: Option<f64> = None;
        if matches!(self.state.consult_mode, ConsultMode::Yolo)
            && self.state.adaptive_router_enabled
            && (preferred.is_none() || role_mismatch)
        {
            let explore_seed = request_id.unwrap_or(question);
            let mut explored = false;
            if core_should_use_adaptive_exploration(
                explore_seed,
                self.state.adaptive_exploration_rate,
            ) {
                if let Some((explore_executor, score, confidence)) =
                    core_resolve_adaptive_exploration_executor(
                        &consult_type,
                        priority.as_str(),
                        &allowed_roles,
                        &self.state.consult_executor_telemetry,
                        self.state.adaptive_exploration_min_samples,
                        explore_seed,
                    )
                {
                    executor = explore_executor;
                    routing_score = Some(score);
                    routing_confidence = Some(confidence);
                    routing_strategy = "adaptive_explore".to_string();
                    explored = true;
                }
            }
            if !explored
                && let Some((adaptive_executor, score, confidence)) =
                    core_resolve_adaptive_consult_executor(
                        &consult_type,
                        priority.as_str(),
                        &allowed_roles,
                        &self.state.consult_routing_map,
                        &self.state.consult_executor_telemetry,
                    )
            {
                routing_score = Some(score);
                routing_confidence = Some(confidence);
                if confidence >= self.state.adaptive_confidence_floor {
                    executor = adaptive_executor;
                    routing_strategy = "adaptive".to_string();
                } else {
                    routing_strategy = "policy_confidence_floor".to_string();
                }
            }
        }
        if !core_is_consult_role_allowed(
            &consult_type,
            &executor,
            &self.state.consult_allowed_roles,
        ) {
            bail!("resolved executor is not allowed for consult_type={consult_type}");
        }
        let escalation_required =
            priority == "critical" || role_mismatch || task_profile.risk == "critical";
        let escalation_reason = if role_mismatch {
            "preferred_role_not_allowed"
        } else if priority == "critical" {
            "critical_priority"
        } else if task_profile.risk == "critical" {
            "critical_task_risk"
        } else {
            "none"
        };

        let out = match self.state.consult_mode {
            ConsultMode::UserTracking => json!({
                "route": "user",
                "reason": "consult mode USER_TRACKING",
                "actor": "user",
                "policy_revision": self.state.policy_revision,
                "ide_profile": self.state.active_ide_profile,
                "ide_client_name": self.state.active_ide_client_name,
                "consult_type": consult_type,
                "priority": priority,
                "task_profile": {
                    "task_type": task_profile.task_type,
                    "risk": task_profile.risk,
                    "confidence": task_profile.confidence,
                    "keywords": task_profile.keywords,
                    "budget": task_budget
                },
                "timeout_sec": timeout_sec,
                "routing_decision": {
                    "strategy": routing_strategy,
                    "adaptive_enabled": self.state.adaptive_router_enabled,
                    "confidence_floor": self.state.adaptive_confidence_floor,
                    "exploration_rate": self.state.adaptive_exploration_rate,
                    "exploration_min_samples": self.state.adaptive_exploration_min_samples,
                    "score": routing_score,
                    "confidence": routing_confidence
                },
                "retry_policy": {
                    "max_retries": max_retries
                },
                "escalation": {
                    "required": escalation_required,
                    "target": if escalation_required { escalation_target } else { "none".to_string() },
                    "reason": escalation_reason
                }
            }),
            ConsultMode::Yolo => {
                json!({
                    "route": "orchestrator",
                    "reason": "consult mode YOLO",
                    "actor": "orchestrator",
                    "policy_revision": self.state.policy_revision,
                    "ide_profile": self.state.active_ide_profile,
                    "ide_client_name": self.state.active_ide_client_name,
                    "consult_type": consult_type,
                    "priority": priority,
                    "task_profile": {
                        "task_type": task_profile.task_type,
                        "risk": task_profile.risk,
                        "confidence": task_profile.confidence,
                        "keywords": task_profile.keywords,
                        "budget": task_budget
                    },
                    "timeout_sec": timeout_sec,
                    "routing_decision": {
                        "strategy": routing_strategy,
                        "adaptive_enabled": self.state.adaptive_router_enabled,
                        "confidence_floor": self.state.adaptive_confidence_floor,
                        "exploration_rate": self.state.adaptive_exploration_rate,
                        "exploration_min_samples": self.state.adaptive_exploration_min_samples,
                        "score": routing_score,
                        "confidence": routing_confidence
                    },
                    "retry_policy": {
                        "max_retries": max_retries
                    },
                    "escalation": {
                        "required": escalation_required,
                        "target": if escalation_required { escalation_target } else { "none".to_string() },
                        "reason": escalation_reason
                    },
                    "dispatch": {
                        "target": "orchestrator",
                        "executor": executor
                    }
                })
            }
        };
        self.append_audit(
            "consult.routed",
            json!({
                "request_id": request_id,
                "consult_mode": consult_mode_as_str(&self.state.consult_mode),
                "consult_type": out["consult_type"],
                "priority": out["priority"],
                "route": out["route"],
                "actor": out["actor"],
                "policy_revision": out["policy_revision"],
                "task_profile": out["task_profile"],
                "ide_profile": out["ide_profile"],
                "ide_client_name": out["ide_client_name"],
                "dispatch": out.get("dispatch"),
                "routing_decision": out["routing_decision"],
                "escalation": out["escalation"],
                "retry_policy": out["retry_policy"]
            }),
        )?;
        Ok(out)
    }

    fn ensure_consult_cross_rules_ack(
        &mut self,
        consult_type: &str,
        priority: &str,
        request_id: Option<&str>,
    ) -> Result<()> {
        if !self.state.consult_require_cross_rules_ack
            || !matches!(self.state.consult_mode, ConsultMode::Yolo)
        {
            return Ok(());
        }
        let missing: Vec<String> = self
            .state
            .consult_required_evidence_ids
            .iter()
            .filter(|id| !self.state.evidence.contains_key(id.as_str()))
            .cloned()
            .collect();
        if missing.is_empty() {
            return Ok(());
        }
        self.append_audit(
            "consult.blocked_missing_evidence",
            json!({
                "request_id": request_id,
                "consult_type": consult_type,
                "priority": priority,
                "consult_mode": consult_mode_as_str(&self.state.consult_mode),
                "missing_evidence_ids": missing
            }),
        )?;
        bail!("policy deny: consult requires cross-rules ack evidence");
    }

    pub fn route_consult_legacy(&self, question: &str) -> Result<Value> {
        if question.trim().is_empty() {
            bail!("question must not be empty");
        }
        match self.state.consult_mode {
            ConsultMode::UserTracking => Ok(json!({
                "route": "user",
                "reason": "consult mode USER_TRACKING"
            })),
            ConsultMode::Yolo => Ok(json!({
                "route": "orchestrator",
                "reason": "consult mode YOLO"
            })),
        }
    }

    pub fn register_evidence(&mut self, id: String, path: String) -> Result<Value> {
        if id.trim().is_empty() || path.trim().is_empty() {
            bail!("id and path are required");
        }
        self.state.evidence.insert(id.clone(), path.clone());
        self.append_audit("evidence.registered", json!({"id": id, "path": path}))?;
        Ok(json!({"evidence_id": id, "path": path}))
    }

    pub fn record_event(
        &mut self,
        cpu: &CpuProfile,
        kind: String,
        payload: Value,
    ) -> Result<Value> {
        if kind.trim().is_empty() {
            bail!("kind is required");
        }
        let ts_unix = now_unix()?;
        let raw = core_event_hash_material(&kind, &payload, ts_unix)?;
        let digest = cpu.hash_bytes(raw.as_bytes());
        let record = core_build_event_record(&kind, &payload, ts_unix, digest)?;
        let event_kind = record.kind.clone();
        self.state.events.push(record);
        self.append_audit(
            "event.recorded",
            json!({
                "digest": digest,
                "kind": event_kind
            }),
        )?;
        Ok(json!({"digest": digest, "ts_unix": ts_unix}))
    }

    fn repo_root(&self) -> PathBuf {
        self.state_path
            .parent()
            .and_then(|p| p.parent())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    }

    fn append_proxy_trace(
        &mut self,
        cpu: &CpuProfile,
        category: &str,
        operation: &str,
        target: &str,
        allow: bool,
        executed: bool,
        reason: &str,
    ) -> Result<()> {
        let ts_unix = now_unix()?;
        let raw = core_proxy_trace_hash_input(
            ts_unix, category, operation, target, allow, executed, reason,
        );
        let digest = cpu.hash_bytes(raw.as_bytes());
        self.state.proxy_log.push(core_build_proxy_trace_record(
            ts_unix, category, operation, target, allow, executed, reason, digest,
        ));
        if self.state.proxy_log.len() > PROXY_LOG_MAX_ENTRIES {
            let extra = self.state.proxy_log.len() - PROXY_LOG_MAX_ENTRIES;
            self.state.proxy_log.drain(0..extra);
        }
        self.append_audit(
            "proxy.trace",
            json!({
                "category": category,
                "operation": operation,
                "target": target,
                "allow": allow,
                "executed": executed,
                "reason": reason,
                "digest": digest
            }),
        )?;
        Ok(())
    }

    fn append_audit(&mut self, kind: &str, payload: Value) -> Result<()> {
        core_append_audit_record(
            &self.audit_path,
            &self.state.phase,
            self.state.policy_revision,
            kind,
            payload,
        )
        .with_context(|| {
            format!(
                "failed to append audit record: {}",
                self.audit_path.display()
            )
        })?;
        self.maybe_auto_rotate_audit(kind)
    }

    fn maybe_auto_rotate_audit(&mut self, source_kind: &str) -> Result<()> {
        if !self.state.audit_auto_rotate_enabled {
            return Ok(());
        }
        if source_kind == "audit.rotated" || source_kind == "audit.archives_pruned" {
            return Ok(());
        }
        if self.audit_path.as_os_str().is_empty() {
            return Ok(());
        }
        let meta = match fs::metadata(&self.audit_path) {
            Ok(v) => v,
            Err(_) => return Ok(()),
        };
        let now = now_unix()?;
        if self.state.audit_last_rotation_unix == 0 {
            self.state.audit_last_rotation_unix = now;
        }

        let by_size = meta.len() >= self.state.audit_auto_rotate_max_bytes;
        let by_age = now.saturating_sub(self.state.audit_last_rotation_unix)
            >= self.state.audit_auto_rotate_max_age_sec;
        if !by_size && !by_age {
            return Ok(());
        }

        let repo_root = self.repo_root();
        let archive_path = core_resolve_safe_repo_path(&repo_root, &self.state.audit_archive_dir)?;
        let mut rotated = core_rotate_audit_log(
            &self.audit_path,
            &archive_path,
            self.state.audit_auto_rotate_compress,
        )?;
        if let Some(obj) = rotated.as_object_mut() {
            if let Some(archive_abs) = obj.get("archive_path").and_then(|x| x.as_str()) {
                let rel = repo_relative_path_string(&repo_root, Path::new(archive_abs));
                obj.insert("archive_path".to_string(), Value::String(rel));
            }
            if let Some(signature_abs) = obj.get("signature_path").and_then(|x| x.as_str()) {
                let rel = repo_relative_path_string(&repo_root, Path::new(signature_abs));
                obj.insert("signature_path".to_string(), Value::String(rel));
            }
        }
        let pruned = core_prune_audit_archives(
            &archive_path,
            self.state.audit_auto_rotate_keep_last as usize,
        )?;
        self.state.audit_last_rotation_unix = now;
        core_append_audit_record(
            &self.audit_path,
            &self.state.phase,
            self.state.policy_revision,
            "audit.rotated",
            json!({
                "trigger": if by_size { "size" } else { "age" },
                "source_kind": source_kind,
                "archive": rotated,
                "prune": pruned
            }),
        )
        .with_context(|| {
            format!(
                "failed to append audit record: {}",
                self.audit_path.display()
            )
        })
    }

    fn read_audit_items(&self) -> Vec<Value> {
        core_read_audit_items(&self.audit_path)
    }

    fn build_gate_report(&self, kind: &str, phase: &str) -> Result<GateReport> {
        let repo_root = self.repo_root();
        let ctx = GateEvalContext {
            current_phase: &self.state.phase,
            consult_mode_is_set: matches!(
                self.state.consult_mode,
                ConsultMode::UserTracking | ConsultMode::Yolo
            ),
            consult_mode_is_yolo: matches!(self.state.consult_mode, ConsultMode::Yolo),
            strict_artifacts: self.state.strict_gate_artifacts,
            evidence: &self.state.evidence,
        };
        core_build_gate_report(&repo_root, kind, phase, &ctx)
    }
}

fn classify_task_from_text(
    question: &str,
    explicit_task_type: Option<&str>,
) -> Result<TaskClassification> {
    let mut keywords = Vec::new();
    let text = question.trim().to_ascii_lowercase();
    let explicit = explicit_task_type
        .map(|x| normalize_task_type(x))
        .transpose()?;
    let task_type = if let Some(v) = explicit {
        v
    } else if contains_any(
        &text,
        &[
            "debug",
            "bug",
            "fix",
            "trace",
            "crash",
            "исправ",
            "ошиб",
            "дебаг",
            "паден",
        ],
    ) {
        keywords.push("debug".to_string());
        "debug".to_string()
    } else if contains_any(
        &text,
        &[
            "refactor",
            "cleanup",
            "rename",
            "restructure",
            "рефактор",
            "чистк",
        ],
    ) {
        keywords.push("refactor".to_string());
        "refactor".to_string()
    } else if contains_any(
        &text,
        &[
            "optimiz",
            "performance",
            "latency",
            "throughput",
            "simd",
            "avx",
            "fma",
            "ускор",
            "оптим",
        ],
    ) {
        keywords.push("optimization".to_string());
        "optimization".to_string()
    } else if contains_any(
        &text,
        &["test", "qa", "verify", "validation", "провер", "тест"],
    ) {
        keywords.push("testing".to_string());
        "testing".to_string()
    } else if contains_any(
        &text,
        &["docs", "readme", "document", "spec", "докум", "описан"],
    ) {
        keywords.push("docs".to_string());
        "docs".to_string()
    } else if contains_any(
        &text,
        &[
            "security",
            "secret",
            "credential",
            "token",
            "vulnerab",
            "уязв",
            "секрет",
        ],
    ) {
        keywords.push("security".to_string());
        "security".to_string()
    } else {
        "codegen".to_string()
    };

    let mut risk = "medium".to_string();
    if task_type == "docs" {
        risk = "low".to_string();
    }
    if contains_any(
        &text,
        &[
            "unsafe",
            "nightly",
            "kernel",
            "branch protection",
            "release gate",
            "migration",
            "mcp runtime",
            "prod",
            "прод",
            "релиз",
            "миграц",
            "ключ",
            "secret",
            "credential",
            "token",
        ],
    ) {
        risk = "high".to_string();
        keywords.push("high_risk_signal".to_string());
    }
    if contains_any(
        &text,
        &[
            "deploy",
            "production",
            "rollback",
            "critical",
            "incident",
            "авар",
            "инцидент",
        ],
    ) {
        risk = "critical".to_string();
        keywords.push("critical_signal".to_string());
    }

    keywords.sort();
    keywords.dedup();
    let confidence = if explicit_task_type.is_some() {
        0.95
    } else {
        let score = 0.5 + (keywords.len() as f64 * 0.12);
        score.clamp(0.5, 0.98)
    };
    Ok(TaskClassification {
        task_type,
        risk,
        confidence,
        keywords,
    })
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack.contains(n))
}

fn normalize_task_type(value: &str) -> Result<String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        bail!("task_type must not be empty");
    }
    let mapped = match normalized.as_str() {
        "general" => "codegen",
        "code" => "codegen",
        "performance" => "optimization",
        "math" => "analysis",
        "integration" => "analysis",
        "architecture" => "analysis",
        "consult" => "analysis",
        "analysis" => "analysis",
        "debug" => "debug",
        "refactor" => "refactor",
        "testing" => "testing",
        "docs" => "docs",
        "security" => "security",
        "security_review" => "security",
        "codegen" => "codegen",
        "optimization" => "optimization",
        _ => "analysis",
    };
    Ok(mapped.to_string())
}

fn normalize_task_risk(value: Option<&str>) -> Result<String> {
    let normalized = value.unwrap_or("medium").trim().to_ascii_lowercase();
    match normalized.as_str() {
        "low" | "medium" | "high" | "critical" => Ok(normalized),
        _ => bail!("unsupported risk level: {normalized}"),
    }
}

fn resolve_budget_profile(
    profiles: &BTreeMap<String, TaskBudgetProfile>,
    risk: &str,
) -> Result<TaskBudgetProfile> {
    if let Some(profile) = profiles.get(risk) {
        validate_budget_profile(profile)?;
        return Ok(profile.clone());
    }
    let defaults = default_task_budget_profiles();
    let profile = defaults
        .get(risk)
        .cloned()
        .ok_or_else(|| anyhow!("missing budget profile for risk={risk}"))?;
    validate_budget_profile(&profile)?;
    Ok(profile)
}

fn validate_budget_profile(profile: &TaskBudgetProfile) -> Result<()> {
    if profile.max_steps == 0 {
        bail!("budget max_steps must be > 0");
    }
    if profile.max_tool_calls == 0 {
        bail!("budget max_tool_calls must be > 0");
    }
    if profile.max_runtime_sec == 0 {
        bail!("budget max_runtime_sec must be > 0");
    }
    Ok(())
}

fn scale_budget_for_priority(
    profile: &TaskBudgetProfile,
    priority: &str,
) -> Result<TaskBudgetProfile> {
    validate_budget_profile(profile)?;
    let multiplier = match priority {
        "low" => 0.8,
        "normal" => 1.0,
        "high" => 1.2,
        "critical" => 1.4,
        _ => bail!("unsupported priority: {priority}"),
    };
    Ok(TaskBudgetProfile {
        max_steps: ((profile.max_steps as f64) * multiplier).ceil() as u64,
        max_tool_calls: ((profile.max_tool_calls as f64) * multiplier).ceil() as u64,
        max_runtime_sec: ((profile.max_runtime_sec as f64) * multiplier).ceil() as u64,
    })
}

fn detect_unsafe_files(files: &[String]) -> bool {
    files.iter().any(|path| {
        let p = path.to_ascii_lowercase();
        p.ends_with(".rs") && (p.contains("unsafe") || p.contains("simd") || p.contains("intrin"))
    })
}

fn detect_build_script_files(files: &[String]) -> bool {
    files.iter().any(|path| {
        let p = path.to_ascii_lowercase();
        p.ends_with("cargo.toml")
            || p.ends_with("build.rs")
            || p.starts_with(".github/workflows/")
            || p.contains("/workflows/")
            || p.ends_with("dockerfile")
            || p.contains("/deploy/")
    })
}

fn detect_secret_files(files: &[String]) -> bool {
    files.iter().any(|path| {
        let p = path.to_ascii_lowercase();
        p.contains(".env")
            || p.contains("secret")
            || p.contains("credential")
            || p.contains("token")
            || p.contains("id_rsa")
            || p.contains("private_key")
    })
}

fn default_proxy_deny_by_default() -> bool {
    true
}

fn default_require_signed_policy() -> bool {
    true
}

fn default_require_zen4_fast_path() -> bool {
    false
}

fn default_require_avx512f() -> bool {
    false
}

fn default_require_avx512vl() -> bool {
    false
}

fn default_require_fma() -> bool {
    false
}

fn default_require_bmi2() -> bool {
    false
}

fn default_require_sha() -> bool {
    false
}

fn default_policy_signing_algorithm() -> String {
    core_default_policy_signing_algorithm()
}

fn default_active_policy_key_id() -> String {
    "default".to_string()
}

fn default_policy_signing_keys() -> Vec<PolicySigningKey> {
    core_default_policy_signing_keys(&default_active_policy_key_id())
}

fn default_adaptive_router_enabled() -> bool {
    false
}

fn default_adaptive_confidence_floor() -> f64 {
    0.25
}

fn default_adaptive_exploration_rate() -> f64 {
    0.10
}

fn default_adaptive_exploration_min_samples() -> u64 {
    5
}

fn default_consult_executor_telemetry() -> BTreeMap<String, ConsultExecutorTelemetry> {
    BTreeMap::new()
}

fn default_task_budget_profiles() -> BTreeMap<String, TaskBudgetProfile> {
    BTreeMap::from([
        (
            "low".to_string(),
            TaskBudgetProfile {
                max_steps: 4,
                max_tool_calls: 16,
                max_runtime_sec: 300,
            },
        ),
        (
            "medium".to_string(),
            TaskBudgetProfile {
                max_steps: 8,
                max_tool_calls: 32,
                max_runtime_sec: 900,
            },
        ),
        (
            "high".to_string(),
            TaskBudgetProfile {
                max_steps: 12,
                max_tool_calls: 64,
                max_runtime_sec: 1800,
            },
        ),
        (
            "critical".to_string(),
            TaskBudgetProfile {
                max_steps: 16,
                max_tool_calls: 96,
                max_runtime_sec: 3600,
            },
        ),
    ])
}

fn default_patch_gate_policy() -> PatchGatePolicy {
    PatchGatePolicy {
        require_review_on_unsafe: default_patch_gate_require_review_on_unsafe(),
        require_review_on_build_scripts: default_patch_gate_require_review_on_build_scripts(),
        deny_on_secrets: default_patch_gate_deny_on_secrets(),
        max_auto_apply_files: default_patch_gate_max_auto_apply_files(),
    }
}

fn default_patch_gate_require_review_on_unsafe() -> bool {
    true
}

fn default_patch_gate_require_review_on_build_scripts() -> bool {
    true
}

fn default_patch_gate_deny_on_secrets() -> bool {
    true
}

fn default_patch_gate_max_auto_apply_files() -> u64 {
    7
}

fn default_result_compact_enabled() -> bool {
    true
}

fn default_result_compact_max_chars() -> u64 {
    4000
}

fn default_result_compact_preview_items() -> u64 {
    8
}

fn default_result_compact_policy() -> ResultCompactPolicy {
    ResultCompactPolicy {
        enabled: default_result_compact_enabled(),
        max_chars: default_result_compact_max_chars(),
        preview_items: default_result_compact_preview_items(),
    }
}

fn default_context_lazy_tool_search() -> bool {
    true
}

fn default_context_lazy_threshold_pct() -> u64 {
    10
}

fn default_context_programmatic_max_calls() -> u64 {
    16
}

fn default_context_window_policy() -> ContextWindowPolicy {
    ContextWindowPolicy {
        lazy_tool_search: default_context_lazy_tool_search(),
        lazy_threshold_pct: default_context_lazy_threshold_pct(),
        programmatic_max_calls: default_context_programmatic_max_calls(),
    }
}

fn default_consult_routing_map() -> BTreeMap<String, String> {
    core_default_consult_routing_map()
}

fn default_consult_priority_timeouts() -> BTreeMap<String, u64> {
    core_default_consult_priority_timeouts()
}

fn default_consult_retry_limits() -> BTreeMap<String, u64> {
    core_default_consult_retry_limits()
}

fn default_consult_escalation_targets() -> BTreeMap<String, String> {
    core_default_consult_escalation_targets()
}

fn default_consult_allowed_roles() -> BTreeMap<String, Vec<String>> {
    core_default_consult_allowed_roles()
}

fn default_active_role_profile() -> String {
    "orchestrator".to_string()
}

fn role_tools_with_base(extra: &[&str]) -> Vec<String> {
    let mut set = BTreeSet::new();
    for tool in [
        "cabal.get_state",
        "cabal.tool_search",
        "cabal.get_tool_schema",
        "cabal.programmatic_call",
        "cabal.result_compact",
        "cabal.get_result_compact_policy",
        "cabal.get_context_window_policy",
        "cabal.get_gate_policy",
        "cabal.get_cross_rules_status",
        "cabal.ack_cross_rules",
        "cabal.gate_check",
        "cabal.transition_phase_strict",
        "cabal.route_consult",
        "cabal.register_evidence",
        "cabal.record_event",
        "cabal.get_role_profile",
        "cabal.list_role_profiles",
        "cabal.request_role_switch",
    ] {
        set.insert(tool.to_string());
    }
    for tool in extra {
        set.insert((*tool).to_string());
    }
    set.into_iter().collect()
}

fn default_role_tool_access_profiles() -> BTreeMap<String, Vec<String>> {
    let mut profiles = BTreeMap::new();

    profiles.insert(
        "orchestrator".to_string(),
        role_tools_with_base(&[
            "cabal.get_capabilities",
            "cabal.get_error_codes",
            "cabal.validate_error_codes_parity",
            "cabal.get_cpu_policy",
            "cabal.set_cpu_policy",
            "cabal.set_gate_policy",
            "cabal.get_ide_profile_policy",
            "cabal.set_ide_profile_policy",
            "cabal.get_audit_rotation_policy",
            "cabal.set_audit_rotation_policy",
            "cabal.set_result_compact_policy",
            "cabal.set_context_window_policy",
            "cabal.get_consult_routing",
            "cabal.get_consult_guard_policy",
            "cabal.get_adaptive_router",
            "cabal.classify_task",
            "cabal.get_budget_policy",
            "cabal.set_budget_policy",
            "cabal.plan_task_execution",
            "cabal.get_patch_gate_policy",
            "cabal.set_patch_gate_policy",
            "cabal.evaluate_patch_gate",
            "cabal.set_consult_mode",
            "cabal.set_consult_guard_policy",
            "cabal.set_consult_routing_rule",
            "cabal.set_consult_priority_timeout",
            "cabal.set_consult_retry_limit",
            "cabal.set_consult_escalation_target",
            "cabal.set_consult_allowed_roles",
            "cabal.set_adaptive_router",
            "cabal.set_adaptive_exploration_policy",
            "cabal.record_consult_feedback",
            "cabal.apply_policy_bundle",
            "cabal.set_policy_security",
            "cabal.list_policy_signing_keys",
            "cabal.upsert_policy_signing_key",
            "cabal.set_active_policy_signing_key",
            "cabal.revoke_policy_signing_key",
            "cabal.guard_action",
            "cabal.get_proxy_operation_policy",
            "cabal.set_proxy_operation_policy",
            "cabal.set_proxy_policy",
            "cabal.get_proxy_log",
            "cabal.get_audit_log",
            "cabal.query_audit_log",
            "cabal.export_audit_log",
            "cabal.replay_audit_state",
            "cabal.rotate_audit_log",
            "cabal.verify_audit_archive",
            "cabal.prune_audit_archives",
            "cabal.audit_health_check",
            "cabal.proxy_request",
            "cabal.proxy_execute",
            "cabal.transition_phase",
            "cabal.approve_role_switch",
            "cabal.set_role_profile",
        ]),
    );
    profiles.insert(
        "global_architect".to_string(),
        role_tools_with_base(&[
            "cabal.classify_task",
            "cabal.get_budget_policy",
            "cabal.plan_task_execution",
            "cabal.evaluate_patch_gate",
        ]),
    );
    profiles.insert(
        "architect".to_string(),
        role_tools_with_base(&[
            "cabal.classify_task",
            "cabal.get_budget_policy",
            "cabal.plan_task_execution",
            "cabal.evaluate_patch_gate",
        ]),
    );
    profiles.insert(
        "conceptualizer".to_string(),
        role_tools_with_base(&[
            "cabal.classify_task",
            "cabal.get_budget_policy",
            "cabal.get_patch_gate_policy",
        ]),
    );
    profiles.insert(
        "mathematician".to_string(),
        role_tools_with_base(&[
            "cabal.classify_task",
            "cabal.get_budget_policy",
            "cabal.get_patch_gate_policy",
        ]),
    );
    profiles.insert(
        "integrator_runtime".to_string(),
        role_tools_with_base(&[
            "cabal.classify_task",
            "cabal.plan_task_execution",
            "cabal.get_proxy_operation_policy",
            "cabal.proxy_request",
            "cabal.proxy_execute",
        ]),
    );
    profiles.insert(
        "rust_engineer".to_string(),
        role_tools_with_base(&[
            "cabal.get_cpu_policy",
            "cabal.classify_task",
            "cabal.plan_task_execution",
            "cabal.evaluate_patch_gate",
            "cabal.proxy_execute",
        ]),
    );
    profiles.insert(
        "simd_specialist".to_string(),
        role_tools_with_base(&[
            "cabal.get_cpu_policy",
            "cabal.classify_task",
            "cabal.plan_task_execution",
            "cabal.evaluate_patch_gate",
            "cabal.proxy_execute",
        ]),
    );
    profiles.insert(
        "debuger".to_string(),
        role_tools_with_base(&[
            "cabal.classify_task",
            "cabal.plan_task_execution",
            "cabal.get_proxy_log",
            "cabal.query_audit_log",
        ]),
    );
    profiles.insert(
        "fixer".to_string(),
        role_tools_with_base(&[
            "cabal.classify_task",
            "cabal.plan_task_execution",
            "cabal.evaluate_patch_gate",
            "cabal.proxy_execute",
        ]),
    );
    profiles.insert(
        "qa_agent".to_string(),
        role_tools_with_base(&[
            "cabal.query_audit_log",
            "cabal.export_audit_log",
            "cabal.replay_audit_state",
            "cabal.audit_health_check",
            "cabal.evaluate_patch_gate",
        ]),
    );
    profiles.insert(
        "tester".to_string(),
        role_tools_with_base(&[
            "cabal.query_audit_log",
            "cabal.export_audit_log",
            "cabal.replay_audit_state",
            "cabal.audit_health_check",
            "cabal.evaluate_patch_gate",
        ]),
    );

    profiles
}

fn normalize_role_name(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("role is required");
    }

    let mut out = String::with_capacity(trimmed.len());
    let mut prev_underscore = false;
    for ch in trimmed.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_underscore = false;
            continue;
        }
        if matches!(ch, '_' | '-' | ' ' | '/' | '.') {
            if !out.is_empty() && !prev_underscore {
                out.push('_');
                prev_underscore = true;
            }
            continue;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() {
        bail!("role is required");
    }
    Ok(out)
}

fn resolve_allowed_tools_for_role(
    role: &str,
    profiles: &BTreeMap<String, Vec<String>>,
) -> BTreeSet<String> {
    let normalized = normalize_role_name(role).unwrap_or_else(|_| default_active_role_profile());
    let fallback = default_active_role_profile();
    let tools = profiles
        .get(normalized.as_str())
        .or_else(|| profiles.get(fallback.as_str()));
    let mut allowed = BTreeSet::new();
    if let Some(items) = tools {
        for tool in items {
            let normalized_tool = tool.trim();
            if !normalized_tool.is_empty() {
                allowed.insert(normalized_tool.to_string());
            }
        }
    }
    allowed
}

fn default_proxy_allow() -> BTreeMap<String, Vec<String>> {
    BTreeMap::from([
        (
            "fs".to_string(),
            vec![
                ".memory/".to_string(),
                "spec/docs/".to_string(),
                "cabal-mcp-runtime/".to_string(),
            ],
        ),
        ("network".to_string(), vec![]),
        ("shell".to_string(), vec![]),
    ])
}

fn default_proxy_allowed_operations() -> BTreeMap<String, Vec<String>> {
    BTreeMap::from([
        (
            "fs".to_string(),
            vec![
                "read_text".to_string(),
                "write_text".to_string(),
                "list_dir".to_string(),
            ],
        ),
        ("shell".to_string(), vec!["run".to_string()]),
        ("network".to_string(), vec!["http_get".to_string()]),
    ])
}

fn default_proxy_denied_operations() -> BTreeMap<String, Vec<String>> {
    BTreeMap::new()
}

fn normalize_proxy_operations(raw_ops: Vec<String>) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for raw in raw_ops {
        let normalized = raw.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            continue;
        }
        if !out.contains(&normalized) {
            out.push(normalized);
        }
    }
    if out.is_empty() {
        bail!("operations must not be empty");
    }
    Ok(out)
}

fn cross_rules_required_evidence_ids() -> Vec<String> {
    vec![
        "cross_rules_agent_ack".to_string(),
        "cross_rules_subagent_ack".to_string(),
    ]
}

fn missing_evidence_ids(
    evidence: &BTreeMap<String, String>,
    required_ids: &[String],
) -> Vec<String> {
    required_ids
        .iter()
        .filter(|id| !evidence.contains_key(id.as_str()))
        .cloned()
        .collect()
}

fn normalize_evidence_ids(raw_ids: Vec<String>) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for raw in raw_ids {
        let normalized = raw.trim().to_string();
        if normalized.is_empty() {
            continue;
        }
        if !out.contains(&normalized) {
            out.push(normalized);
        }
    }
    if out.is_empty() {
        bail!("required_evidence_ids must not be empty");
    }
    Ok(out)
}

fn default_consult_require_cross_rules_ack() -> bool {
    false
}

fn default_consult_required_evidence_ids() -> Vec<String> {
    cross_rules_required_evidence_ids()
}

fn default_enforce_ide_profile() -> bool {
    false
}

fn default_require_ide_client_info() -> bool {
    false
}

fn default_strict_gate_artifacts() -> bool {
    false
}

fn default_allowed_ide_profiles() -> Vec<String> {
    core_default_allowed_ide_profiles()
}

fn default_active_ide_profile() -> String {
    "generic".to_string()
}

fn default_audit_auto_rotate_enabled() -> bool {
    false
}

fn default_audit_auto_rotate_max_bytes() -> u64 {
    8 * 1024 * 1024
}

fn default_audit_auto_rotate_max_age_sec() -> u64 {
    3600
}

fn default_audit_auto_rotate_compress() -> bool {
    true
}

fn default_audit_auto_rotate_keep_last() -> u64 {
    20
}

fn default_audit_archive_dir() -> String {
    ".cabal_runtime/archive".to_string()
}

fn default_audit_last_rotation_unix() -> u64 {
    0
}

fn consult_mode_as_str(mode: &ConsultMode) -> &'static str {
    match mode {
        ConsultMode::UserTracking => "user_tracking",
        ConsultMode::Yolo => "yolo",
    }
}

fn repo_relative_path_string(repo_root: &Path, path: &Path) -> String {
    match path.strip_prefix(repo_root) {
        Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}

fn normalize_limit(limit: Option<usize>, default_limit: usize, max_limit: usize) -> Result<usize> {
    if max_limit == 0 {
        bail!("max_limit must be > 0");
    }
    match limit {
        Some(0) => bail!("limit must be > 0"),
        Some(v) => Ok(v.min(max_limit)),
        None => Ok(default_limit.min(max_limit)),
    }
}

fn now_unix() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("clock before unix epoch")?
        .as_secs())
}

fn truncate_chars_with_suffix(input: &str, max_chars: usize, suffix: &str) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }
    let suffix_chars = suffix.chars().count();
    if max_chars <= suffix_chars {
        return suffix.chars().take(max_chars).collect();
    }
    let keep = max_chars - suffix_chars;
    let mut out: String = input.chars().take(keep).collect();
    out.push_str(suffix);
    out
}

fn summarize_value(value: &Value, preview_items: usize) -> Value {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {
            json!({"type": "scalar", "value": value})
        }
        Value::Array(items) => {
            let preview: Vec<Value> = items
                .iter()
                .take(preview_items)
                .map(|v| summarize_value(v, preview_items.saturating_div(2).max(1)))
                .collect();
            json!({
                "type": "array",
                "len": items.len(),
                "preview_items": preview
            })
        }
        Value::Object(map) => {
            let mut keys: Vec<&str> = map.keys().map(|k| k.as_str()).collect();
            keys.sort_unstable();
            let preview_keys: Vec<&str> = keys.iter().take(preview_items).copied().collect();
            let mut preview = serde_json::Map::new();
            for key in preview_keys {
                if let Some(v) = map.get(key) {
                    preview.insert(
                        key.to_string(),
                        summarize_value(v, preview_items.saturating_div(2).max(1)),
                    );
                }
            }
            Value::Object(serde_json::Map::from_iter([
                ("type".to_string(), Value::String("object".to_string())),
                ("keys_total".to_string(), json!(keys.len())),
                (
                    "keys_preview".to_string(),
                    json!(keys.iter().take(preview_items).collect::<Vec<_>>()),
                ),
                ("preview".to_string(), Value::Object(preview)),
            ]))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    use std::sync::{Mutex, OnceLock};

    fn test_runtime() -> CabalRuntime {
        let state = RuntimeState {
            project_id: "test".to_string(),
            phase: "C-0".to_string(),
            consult_mode: ConsultMode::UserTracking,
            consult_routing_map: default_consult_routing_map(),
            consult_priority_timeouts: default_consult_priority_timeouts(),
            consult_retry_limits: default_consult_retry_limits(),
            consult_escalation_targets: default_consult_escalation_targets(),
            consult_allowed_roles: default_consult_allowed_roles(),
            consult_require_cross_rules_ack: default_consult_require_cross_rules_ack(),
            consult_required_evidence_ids: default_consult_required_evidence_ids(),
            adaptive_router_enabled: default_adaptive_router_enabled(),
            adaptive_confidence_floor: default_adaptive_confidence_floor(),
            adaptive_exploration_rate: default_adaptive_exploration_rate(),
            adaptive_exploration_min_samples: default_adaptive_exploration_min_samples(),
            consult_executor_telemetry: default_consult_executor_telemetry(),
            task_budget_profiles: default_task_budget_profiles(),
            patch_gate_policy: default_patch_gate_policy(),
            active_role_profile: default_active_role_profile(),
            role_tool_access_profiles: default_role_tool_access_profiles(),
            pending_role_switch: None,
            result_compact_policy: default_result_compact_policy(),
            context_window_policy: default_context_window_policy(),
            policy: PolicyBundle {
                version: "test".to_string(),
                revision: 1,
                rules: vec!["x".to_string()],
                signature: None,
                forbidden_tokens: vec!["bypass".to_string()],
            },
            policy_hash: 123,
            policy_revision: 1,
            require_zen4_fast_path: default_require_zen4_fast_path(),
            require_avx512f: default_require_avx512f(),
            require_avx512vl: default_require_avx512vl(),
            require_fma: default_require_fma(),
            require_bmi2: default_require_bmi2(),
            require_sha: default_require_sha(),
            require_signed_policy: false,
            policy_signing_keys: default_policy_signing_keys(),
            active_policy_key_id: default_active_policy_key_id(),
            used_policy_nonces: Vec::new(),
            proxy_deny_by_default: true,
            proxy_allow: default_proxy_allow(),
            proxy_allowed_operations: default_proxy_allowed_operations(),
            proxy_denied_operations: default_proxy_denied_operations(),
            proxy_log: Vec::new(),
            enforce_ide_profile: default_enforce_ide_profile(),
            require_ide_client_info: default_require_ide_client_info(),
            strict_gate_artifacts: default_strict_gate_artifacts(),
            allowed_ide_profiles: default_allowed_ide_profiles(),
            active_ide_profile: default_active_ide_profile(),
            active_ide_client_name: String::new(),
            active_ide_client_version: String::new(),
            audit_auto_rotate_enabled: default_audit_auto_rotate_enabled(),
            audit_auto_rotate_max_bytes: default_audit_auto_rotate_max_bytes(),
            audit_auto_rotate_max_age_sec: default_audit_auto_rotate_max_age_sec(),
            audit_auto_rotate_compress: default_audit_auto_rotate_compress(),
            audit_auto_rotate_keep_last: default_audit_auto_rotate_keep_last(),
            audit_archive_dir: default_audit_archive_dir(),
            audit_last_rotation_unix: 1,
            evidence: BTreeMap::new(),
            events: Vec::new(),
        };
        CabalRuntime {
            state_path: PathBuf::new(),
            audit_path: PathBuf::new(),
            state,
        }
    }

    #[test]
    fn guard_action_blocks_forbidden_token() {
        let rt = test_runtime();
        let out = rt
            .guard_action("agent", "try bypass policy")
            .expect("guard_action should return");
        assert_eq!(out["allow"], Value::Bool(false));
    }

    #[test]
    fn guard_action_allows_safe_action() {
        let rt = test_runtime();
        let out = rt
            .guard_action("agent", "read policy bundle")
            .expect("guard_action should return");
        assert_eq!(out["allow"], Value::Bool(true));
    }

    #[test]
    fn transition_phase_accepts_next_only() {
        let mut rt = test_runtime();
        let ok = rt.transition_phase("GA-1").expect("must pass");
        assert_eq!(ok["changed"], Value::Bool(true));
        assert_eq!(rt.state.phase, "GA-1");
    }

    #[test]
    fn transition_phase_rejects_skip() {
        let mut rt = test_runtime();
        let err = rt
            .transition_phase("GA-3")
            .expect_err("skip transition must fail");
        assert!(err.to_string().contains("invalid phase transition"));
    }

    #[test]
    fn proxy_request_is_deny_by_default() {
        let rt = test_runtime();
        let out = rt
            .proxy_request("shell", "exec", "cargo check")
            .expect("proxy must return");
        assert_eq!(out["allow"], Value::Bool(false));
    }

    #[test]
    fn apply_policy_rejects_revision_mismatch() {
        let cpu = CpuProfile::detect().expect("cpu");
        let mut rt = test_runtime();
        let err = rt
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
            .expect_err("revision mismatch must fail");
        assert!(err.to_string().contains("policy revision mismatch"));
    }

    #[test]
    fn gate_check_exit_requires_evidence() {
        let rt = test_runtime();
        let report = rt.gate_check("exit", "C-0").expect("gate_check");
        assert_eq!(report["pass"], Value::Bool(false));
    }

    #[test]
    fn register_ide_client_detects_profile() {
        let mut rt = test_runtime();
        let out = rt
            .register_ide_client_session(Some("Visual Studio Code"), Some("1.2.3"))
            .expect("register");
        assert_eq!(out["active_profile"].as_str(), Some("vscode"));
        assert_eq!(
            out["active_client"]["name"].as_str(),
            Some("Visual Studio Code")
        );
    }

    #[test]
    fn set_ide_profile_policy_normalizes_profiles() {
        let mut rt = test_runtime();
        let out = rt
            .set_ide_profile_policy(
                None,
                None,
                Some(vec![
                    "Visual Studio Code".to_string(),
                    "idea".to_string(),
                    "vscode".to_string(),
                ]),
            )
            .expect("set policy");
        let arr = out["allowed_profiles"]
            .as_array()
            .expect("allowed_profiles");
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0].as_str(), Some("jetbrains"));
        assert_eq!(arr[1].as_str(), Some("vscode"));
    }

    #[test]
    fn register_ide_client_denied_when_profile_not_allowed() {
        let mut rt = test_runtime();
        rt.set_ide_profile_policy(
            Some(true),
            None,
            Some(vec!["generic".to_string(), "jetbrains".to_string()]),
        )
        .expect("set policy");
        let err = rt
            .register_ide_client_session(Some("Visual Studio Code"), Some("1"))
            .expect_err("must fail");
        assert!(err.to_string().contains("policy deny"));
    }

    #[test]
    fn register_ide_client_denied_when_client_info_required() {
        let mut rt = test_runtime();
        rt.set_ide_profile_policy(Some(true), Some(true), Some(vec!["generic".to_string()]))
            .expect("set policy");
        let err = rt
            .register_ide_client_session(None, Some("1"))
            .expect_err("must fail");
        assert!(err.to_string().contains("client_info.name is required"));
    }

    #[test]
    fn set_gate_policy_updates_values() {
        let mut rt = test_runtime();
        let out = rt.set_gate_policy(Some(true)).expect("set gate policy");
        assert_eq!(out["strict_artifacts"].as_bool(), Some(true));
        assert_eq!(rt.state.strict_gate_artifacts, true);

        let out = rt.set_gate_policy(Some(false)).expect("set gate policy");
        assert_eq!(out["strict_artifacts"].as_bool(), Some(false));
        assert_eq!(rt.state.strict_gate_artifacts, false);
    }

    #[test]
    fn set_consult_guard_policy_updates_values() {
        let mut rt = test_runtime();
        let out = rt
            .set_consult_guard_policy(
                Some(true),
                Some(vec![
                    "cross_rules_agent_ack".to_string(),
                    "cross_rules_subagent_ack".to_string(),
                ]),
            )
            .expect("set consult guard");
        assert_eq!(out["require_cross_rules_ack"].as_bool(), Some(true));
        let ids = out["required_evidence_ids"]
            .as_array()
            .expect("required_evidence_ids");
        assert_eq!(ids.len(), 2);
        assert_eq!(ids[0].as_str(), Some("cross_rules_agent_ack"));
        assert_eq!(ids[1].as_str(), Some("cross_rules_subagent_ack"));

        let err = rt
            .set_consult_guard_policy(None, Some(vec![" ".to_string()]))
            .expect_err("must fail");
        assert!(
            err.to_string()
                .contains("required_evidence_ids must not be empty")
        );
    }

    #[test]
    fn get_cross_rules_status_reports_missing_and_present() {
        let mut rt = test_runtime();
        let status = rt.get_cross_rules_status();
        assert_eq!(status["entry_gate_all_present"].as_bool(), Some(false));
        assert_eq!(
            status["consult_guard"]["all_present"].as_bool(),
            Some(false)
        );

        rt.register_evidence(
            "cross_rules_agent_ack".to_string(),
            "spec/docs/CONCEPT_MASTER.md".to_string(),
        )
        .expect("register agent ack");
        rt.register_evidence(
            "cross_rules_subagent_ack".to_string(),
            "spec/docs/CONCEPT_MASTER.md".to_string(),
        )
        .expect("register subagent ack");
        let status = rt.get_cross_rules_status();
        assert_eq!(status["entry_gate_all_present"].as_bool(), Some(true));
        assert_eq!(status["consult_guard"]["all_present"].as_bool(), Some(true));
    }

    #[test]
    fn ack_cross_rules_registers_evidence_and_enables_guard() {
        let mut rt = test_runtime();
        let out = rt
            .ack_cross_rules(
                "spec/docs/CONCEPT_MASTER.md".to_string(),
                "spec/docs/CONCEPT_MASTER.md".to_string(),
                Some(true),
            )
            .expect("ack cross rules");
        assert_eq!(out["entry_gate_all_present"].as_bool(), Some(true));
        assert_eq!(out["consult_guard"]["enabled"].as_bool(), Some(true));
        assert!(rt.state.evidence.contains_key("cross_rules_agent_ack"));
        assert!(rt.state.evidence.contains_key("cross_rules_subagent_ack"));

        let err = rt
            .ack_cross_rules(" ".to_string(), "x".to_string(), None)
            .expect_err("must fail");
        assert!(
            err.to_string()
                .contains("agent_ack_path and subagent_ack_path are required")
        );
    }

    #[test]
    fn set_proxy_operation_policy_updates_lists() {
        let mut rt = test_runtime();
        let out = rt
            .set_proxy_operation_policy(
                "shell".to_string(),
                Some(vec!["run".to_string()]),
                Some(vec!["run".to_string()]),
            )
            .expect("set policy");
        let allowed = out["allowed_operations"]["shell"]
            .as_array()
            .expect("allowed ops");
        assert_eq!(allowed.len(), 1);
        assert_eq!(allowed[0].as_str(), Some("run"));
        let denied = out["denied_operations"]["shell"]
            .as_array()
            .expect("denied ops");
        assert_eq!(denied.len(), 1);
        assert_eq!(denied[0].as_str(), Some("run"));
    }

    #[test]
    fn set_cpu_policy_validates_zen4_requirement() {
        let mut rt = test_runtime();
        let cpu = CpuProfile::detect().expect("cpu");

        let out = rt
            .set_cpu_policy(&cpu, Some(false), None, None, None, None, None)
            .expect("set cpu policy false");
        assert_eq!(out["require_zen4_fast_path"].as_bool(), Some(false));

        if matches!(cpu.path, ExecutionPath::Zen4Avx512) {
            let out = rt
                .set_cpu_policy(&cpu, Some(true), None, None, None, None, None)
                .expect("set cpu policy true");
            assert_eq!(out["require_zen4_fast_path"].as_bool(), Some(true));
        } else {
            let err = rt
                .set_cpu_policy(&cpu, Some(true), None, None, None, None, None)
                .expect_err("non-zen4 should fail");
            assert!(err.to_string().contains("zen4 fast path is required"));
        }
    }

    #[test]
    fn set_cpu_policy_validates_feature_requirements() {
        let cpu = CpuProfile::detect().expect("cpu");

        let mut rt = test_runtime();
        if cpu.has_avx512f {
            rt.set_cpu_policy(&cpu, None, Some(true), None, None, None, None)
                .expect("avx512f should pass");
        } else {
            let err = rt
                .set_cpu_policy(&cpu, None, Some(true), None, None, None, None)
                .expect_err("avx512f should fail");
            assert!(err.to_string().contains("avx512f"));
        }

        let mut rt = test_runtime();
        if cpu.has_fma {
            rt.set_cpu_policy(&cpu, None, None, None, Some(true), None, None)
                .expect("fma should pass");
        } else {
            let err = rt
                .set_cpu_policy(&cpu, None, None, None, Some(true), None, None)
                .expect_err("fma should fail");
            assert!(err.to_string().contains("fma"));
        }
    }

    #[test]
    fn set_audit_rotation_policy_updates_values() {
        let mut rt = test_runtime();
        let out = rt
            .set_audit_rotation_policy(
                Some(true),
                Some(1024),
                Some(120),
                Some(false),
                Some(7),
                Some(".cabal_runtime/custom_archive".to_string()),
            )
            .expect("set policy");
        assert_eq!(out["enabled"].as_bool(), Some(true));
        assert_eq!(out["max_bytes"].as_u64(), Some(1024));
        assert_eq!(out["max_age_sec"].as_u64(), Some(120));
        assert_eq!(out["compress"].as_bool(), Some(false));
        assert_eq!(out["keep_last"].as_u64(), Some(7));
    }

    #[test]
    fn set_audit_rotation_policy_rejects_zero_limits() {
        let mut rt = test_runtime();
        let err = rt
            .set_audit_rotation_policy(None, Some(0), None, None, None, None)
            .expect_err("must fail");
        assert!(err.to_string().contains("max_bytes must be > 0"));

        let err = rt
            .set_audit_rotation_policy(None, None, Some(0), None, None, None)
            .expect_err("must fail");
        assert!(err.to_string().contains("max_age_sec must be > 0"));
    }

    #[test]
    fn audit_health_check_rejects_zero_verify_last() {
        let rt = test_runtime();
        let err = rt.audit_health_check(None, Some(0)).expect_err("must fail");
        assert!(err.to_string().contains("verify_last must be > 0"));
    }

    #[test]
    fn normalize_limit_rejects_zero() {
        let err = normalize_limit(Some(0), 10, 100).expect_err("must fail");
        assert!(err.to_string().contains("limit must be > 0"));
    }

    #[test]
    fn normalize_limit_caps_to_max() {
        let out = normalize_limit(Some(5000), 10, 100).expect("normalize");
        assert_eq!(out, 100);
    }

    #[test]
    fn set_adaptive_exploration_policy_validates_inputs() {
        let mut rt = test_runtime();

        let out = rt
            .set_adaptive_exploration_policy(Some(0.5), Some(7))
            .expect("set exploration policy");
        assert_eq!(out["exploration_rate"].as_f64(), Some(0.5));
        assert_eq!(out["exploration_min_samples"].as_u64(), Some(7));

        let err = rt
            .set_adaptive_exploration_policy(Some(1.1), None)
            .expect_err("rate > 1 must fail");
        assert!(
            err.to_string()
                .contains("exploration_rate must be in [0,1]")
        );

        let err = rt
            .set_adaptive_exploration_policy(None, Some(0))
            .expect_err("min samples == 0 must fail");
        assert!(
            err.to_string()
                .contains("exploration_min_samples must be > 0")
        );
    }

    #[test]
    fn classify_task_detects_docs_low_risk() {
        let rt = test_runtime();
        let out = rt
            .classify_task("Обновить README и документацию интеграции", None)
            .expect("classify");
        assert_eq!(out["classification"]["task_type"].as_str(), Some("docs"));
        assert_eq!(out["classification"]["risk"].as_str(), Some("low"));
        assert_eq!(
            out["budget_profile"]["max_steps"].as_u64(),
            Some(default_task_budget_profiles()["low"].max_steps)
        );
    }

    #[test]
    fn plan_task_execution_scales_budget_by_priority() {
        let mut rt = test_runtime();
        let out = rt
            .plan_task_execution(
                "Need production deploy and release gate verification",
                None,
                Some("critical"),
            )
            .expect("plan");
        assert_eq!(out["classification"]["risk"].as_str(), Some("critical"));
        assert_eq!(out["priority"].as_str(), Some("critical"));
        let base = default_task_budget_profiles()
            .get("critical")
            .expect("critical profile")
            .max_steps;
        assert!(out["budget"]["max_steps"].as_u64().expect("max_steps") >= base);
    }

    #[test]
    fn set_budget_policy_updates_profile() {
        let mut rt = test_runtime();
        let out = rt
            .set_budget_policy("high".to_string(), Some(21), Some(101), Some(7200))
            .expect("set budget");
        assert_eq!(out["profiles"]["high"]["max_steps"].as_u64(), Some(21));
        assert_eq!(
            out["profiles"]["high"]["max_tool_calls"].as_u64(),
            Some(101)
        );
        assert_eq!(
            out["profiles"]["high"]["max_runtime_sec"].as_u64(),
            Some(7200)
        );
    }

    #[test]
    fn evaluate_patch_gate_blocks_secret_changes() {
        let mut rt = test_runtime();
        let out = rt
            .evaluate_patch_gate(
                vec![".env.production".to_string(), "src/main.rs".to_string()],
                Some("medium"),
                None,
                None,
                None,
                Some(true),
            )
            .expect("evaluate");
        assert_eq!(out["allow"], Value::Bool(false));
        assert_eq!(out["mode"].as_str(), Some("deny"));
        assert_eq!(out["flags"]["touches_secrets"].as_bool(), Some(true));
    }

    #[test]
    fn evaluate_patch_gate_requires_confirmation_for_unsafe() {
        let mut rt = test_runtime();
        let out = rt
            .evaluate_patch_gate(
                vec!["cabal-mcp-runtime/src/unsafe_simd.rs".to_string()],
                Some("high"),
                None,
                None,
                Some(false),
                Some(true),
            )
            .expect("evaluate");
        assert_eq!(out["allow"], Value::Bool(true));
        assert_eq!(out["mode"].as_str(), Some("require_confirmation"));
        assert_eq!(out["requires_confirmation"].as_bool(), Some(true));
        assert_eq!(out["flags"]["touches_unsafe"].as_bool(), Some(true));
    }

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn apply_policy_signed_and_replay_blocked() {
        let _guard = env_lock().lock().expect("lock");
        let cpu = CpuProfile::detect().expect("cpu");
        let mut rt = test_runtime();
        rt.state.require_signed_policy = true;

        let key = "test_signing_key";
        // SAFETY: тест сериализован через глобальный mutex, чтобы исключить гонки env var.
        unsafe { std::env::set_var("CABAL_POLICY_HMAC_KEY", key) };

        let next_revision = rt.state.policy_revision + 1;
        let nonce = "nonce-1";
        let message = crate::core::policy::build_policy_signing_message(
            "v2",
            next_revision,
            &["rule1".to_string()],
            &[],
            "default",
            nonce,
        )
        .expect("message");
        let mut mac: Hmac<Sha256> = Hmac::new_from_slice(key.as_bytes()).expect("hmac");
        mac.update(message.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());

        let out = rt
            .apply_policy(
                &cpu,
                1,
                "v2".to_string(),
                vec!["rule1".to_string()],
                Some(signature.clone()),
                Some("default".to_string()),
                Some(nonce.to_string()),
                vec![],
            )
            .expect("signed apply should pass");
        assert_eq!(out["policy_revision"], Value::from(2u64));

        let err = rt
            .apply_policy(
                &cpu,
                2,
                "v3".to_string(),
                vec!["rule2".to_string()],
                Some(signature),
                Some("default".to_string()),
                Some(nonce.to_string()),
                vec![],
            )
            .expect_err("replay nonce should fail");
        assert!(err.to_string().contains("nonce replay"));
    }

    #[test]
    fn apply_policy_rejects_expired_signing_key() {
        let _guard = env_lock().lock().expect("lock");
        let cpu = CpuProfile::detect().expect("cpu");
        let mut rt = test_runtime();
        rt.state.require_signed_policy = true;

        let key = "expired_key";
        // SAFETY: serialized under test mutex.
        unsafe { std::env::set_var("CABAL_POLICY_HMAC_KEY", key) };

        let now = now_unix().expect("clock");
        rt.upsert_policy_signing_key(
            "k-expired".to_string(),
            "CABAL_POLICY_HMAC_KEY".to_string(),
            None,
            Some(now.saturating_sub(1)),
            Some(true),
        )
        .expect("upsert key");

        let next_revision = rt.state.policy_revision + 1;
        let nonce = "nonce-expired";
        let msg = crate::core::policy::build_policy_signing_message(
            "v2",
            next_revision,
            &["rule1".to_string()],
            &[],
            "k-expired",
            nonce,
        )
        .expect("message");
        let mut mac: Hmac<Sha256> = Hmac::new_from_slice(key.as_bytes()).expect("hmac");
        mac.update(msg.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());

        let err = rt
            .apply_policy(
                &cpu,
                1,
                "v2".to_string(),
                vec!["rule1".to_string()],
                Some(signature),
                Some("k-expired".to_string()),
                Some(nonce.to_string()),
                vec![],
            )
            .expect_err("expired signing key should fail");
        assert!(err.to_string().contains("expired"));
    }

    #[test]
    fn route_consult_user_tracking_returns_user() {
        let mut rt = test_runtime();
        rt.state.consult_mode = ConsultMode::UserTracking;
        let out = rt
            .route_consult(
                "need approval",
                Some("code"),
                Some("normal"),
                None,
                Some("rq-1"),
            )
            .expect("route");
        assert_eq!(out["route"].as_str(), Some("user"));
        assert_eq!(out["actor"].as_str(), Some("user"));
        assert_eq!(
            out["policy_revision"].as_u64(),
            Some(rt.state.policy_revision)
        );
        assert_eq!(out["priority"].as_str(), Some("normal"));
    }

    #[test]
    fn route_consult_yolo_selects_executor() {
        let mut rt = test_runtime();
        rt.state.consult_mode = ConsultMode::Yolo;
        let out = rt
            .route_consult(
                "check proof",
                Some("math"),
                Some("high"),
                None,
                Some("rq-2"),
            )
            .expect("route");
        assert_eq!(out["route"].as_str(), Some("orchestrator"));
        assert_eq!(out["actor"].as_str(), Some("orchestrator"));
        assert_eq!(
            out["policy_revision"].as_u64(),
            Some(rt.state.policy_revision)
        );
        assert_eq!(out["dispatch"]["executor"].as_str(), Some("mathematician"));
        assert_eq!(out["timeout_sec"].as_u64(), Some(900));
    }

    #[test]
    fn route_consult_uses_custom_routing_rule() {
        let mut rt = test_runtime();
        rt.state.consult_mode = ConsultMode::Yolo;
        rt.set_consult_routing_rule("math".to_string(), "symbolic_solver".to_string())
            .expect("set rule");
        rt.set_consult_allowed_roles("math".to_string(), vec!["symbolic_solver".to_string()])
            .expect("set allowed roles");
        let out = rt
            .route_consult(
                "check proof",
                Some("math"),
                Some("high"),
                None,
                Some("rq-3"),
            )
            .expect("route");
        assert_eq!(
            out["dispatch"]["executor"].as_str(),
            Some("symbolic_solver")
        );
    }

    #[test]
    fn route_consult_uses_custom_timeout_rule() {
        let mut rt = test_runtime();
        rt.state.consult_mode = ConsultMode::Yolo;
        rt.set_consult_priority_timeout("high".to_string(), 111)
            .expect("set timeout");
        let out = rt
            .route_consult(
                "check proof",
                Some("math"),
                Some("high"),
                None,
                Some("rq-4"),
            )
            .expect("route");
        assert_eq!(out["timeout_sec"].as_u64(), Some(111));
    }

    #[test]
    fn route_consult_role_mismatch_escalates_and_fallbacks() {
        let mut rt = test_runtime();
        rt.state.consult_mode = ConsultMode::Yolo;
        rt.set_consult_escalation_target("high".to_string(), "architect".to_string())
            .expect("set escalation");
        rt.set_consult_retry_limit("high".to_string(), 4)
            .expect("set retries");
        let out = rt
            .route_consult(
                "prove invariant",
                Some("math"),
                Some("high"),
                Some("developer"),
                Some("rq-5"),
            )
            .expect("route");
        assert_eq!(out["dispatch"]["executor"].as_str(), Some("mathematician"));
        assert_eq!(out["escalation"]["required"].as_bool(), Some(true));
        assert_eq!(out["escalation"]["target"].as_str(), Some("architect"));
        assert_eq!(
            out["escalation"]["reason"].as_str(),
            Some("preferred_role_not_allowed")
        );
        assert_eq!(out["retry_policy"]["max_retries"].as_u64(), Some(4));
    }

    #[test]
    fn route_consult_fails_when_allowed_roles_are_empty() {
        let mut rt = test_runtime();
        rt.state.consult_mode = ConsultMode::Yolo;
        rt.state
            .consult_allowed_roles
            .insert("math".to_string(), vec![]);
        let err = rt
            .route_consult(
                "prove invariant",
                Some("math"),
                Some("high"),
                Some("developer"),
                Some("rq-6"),
            )
            .expect_err("empty allowed_roles must fail");
        assert!(
            err.to_string()
                .contains("no allowed executor configured for consult_type=math")
        );
    }

    #[test]
    fn route_consult_adaptive_prefers_better_telemetry() {
        let mut rt = test_runtime();
        rt.state.consult_mode = ConsultMode::Yolo;
        rt.set_adaptive_router(Some(true), Some(0.2))
            .expect("adaptive on");
        rt.set_consult_routing_rule("performance".to_string(), "developer".to_string())
            .expect("set routing");
        rt.set_consult_allowed_roles(
            "performance".to_string(),
            vec!["developer".to_string(), "perf_engineer".to_string()],
        )
        .expect("set allowlist");
        for _ in 0..8 {
            rt.record_consult_feedback(
                Some("rq-dev".to_string()),
                "performance".to_string(),
                "developer".to_string(),
                false,
                Some(2400),
            )
            .expect("dev feedback");
        }
        for _ in 0..8 {
            rt.record_consult_feedback(
                Some("rq-perf".to_string()),
                "performance".to_string(),
                "perf_engineer".to_string(),
                true,
                Some(120),
            )
            .expect("perf feedback");
        }

        let out = rt
            .route_consult(
                "optimize kernel",
                Some("performance"),
                Some("high"),
                None,
                Some("rq-7"),
            )
            .expect("route");
        assert_eq!(out["dispatch"]["executor"].as_str(), Some("perf_engineer"));
        assert_eq!(
            out["routing_decision"]["strategy"].as_str(),
            Some("adaptive")
        );
        assert!(
            out["routing_decision"]["confidence"]
                .as_f64()
                .expect("confidence")
                >= 0.2
        );
    }

    #[test]
    fn route_consult_adaptive_respects_confidence_floor() {
        let mut rt = test_runtime();
        rt.state.consult_mode = ConsultMode::Yolo;
        rt.set_adaptive_router(Some(true), Some(0.9))
            .expect("adaptive on");
        rt.set_consult_routing_rule("performance".to_string(), "developer".to_string())
            .expect("set routing");
        rt.set_consult_allowed_roles(
            "performance".to_string(),
            vec!["developer".to_string(), "perf_engineer".to_string()],
        )
        .expect("set allowlist");
        rt.record_consult_feedback(
            Some("rq-perf-low".to_string()),
            "performance".to_string(),
            "perf_engineer".to_string(),
            true,
            Some(100),
        )
        .expect("feedback");

        let out = rt
            .route_consult(
                "optimize kernel",
                Some("performance"),
                Some("high"),
                None,
                Some("rq-8"),
            )
            .expect("route");
        assert_eq!(out["dispatch"]["executor"].as_str(), Some("developer"));
        assert_eq!(
            out["routing_decision"]["strategy"].as_str(),
            Some("policy_confidence_floor")
        );
    }

    #[test]
    fn route_consult_adaptive_exploration_selects_undertrained_executor() {
        let mut rt = test_runtime();
        rt.state.consult_mode = ConsultMode::Yolo;
        rt.set_adaptive_router(Some(true), Some(0.95))
            .expect("adaptive on");
        rt.set_adaptive_exploration_policy(Some(1.0), Some(5))
            .expect("exploration on");
        rt.set_consult_routing_rule("performance".to_string(), "developer".to_string())
            .expect("set routing");
        rt.set_consult_allowed_roles(
            "performance".to_string(),
            vec!["developer".to_string(), "perf_engineer".to_string()],
        )
        .expect("set allowlist");

        for _ in 0..8 {
            rt.record_consult_feedback(
                Some("rq-dev-mature".to_string()),
                "performance".to_string(),
                "developer".to_string(),
                true,
                Some(120),
            )
            .expect("developer feedback");
        }

        let out = rt
            .route_consult(
                "optimize kernel",
                Some("performance"),
                Some("high"),
                None,
                Some("rq-explore-undertrained-1"),
            )
            .expect("route");
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
    }

    #[test]
    fn route_consult_guard_blocks_without_required_cross_rules_ack() {
        let mut rt = test_runtime();
        rt.state.consult_mode = ConsultMode::Yolo;
        rt.set_consult_guard_policy(
            Some(true),
            Some(vec![
                "cross_rules_agent_ack".to_string(),
                "cross_rules_subagent_ack".to_string(),
            ]),
        )
        .expect("set consult guard policy");

        let err = rt
            .route_consult(
                "optimize kernel",
                Some("performance"),
                Some("high"),
                None,
                Some("rq-guard-1"),
            )
            .expect_err("missing evidence must fail");
        assert!(err.to_string().contains("policy deny"));

        rt.register_evidence(
            "cross_rules_agent_ack".to_string(),
            "spec/docs/CONCEPT_MASTER.md".to_string(),
        )
        .expect("register agent ack");
        rt.register_evidence(
            "cross_rules_subagent_ack".to_string(),
            "spec/docs/CONCEPT_MASTER.md".to_string(),
        )
        .expect("register subagent ack");

        let out = rt
            .route_consult(
                "optimize kernel",
                Some("performance"),
                Some("high"),
                None,
                Some("rq-guard-2"),
            )
            .expect("route");
        assert_eq!(out["route"].as_str(), Some("orchestrator"));
    }

    #[test]
    fn proxy_log_is_bounded_to_max_entries() {
        let cpu = CpuProfile::detect().expect("cpu");
        let mut rt = test_runtime();
        let total = PROXY_LOG_MAX_ENTRIES + 64;
        for idx in 0..total {
            rt.append_proxy_trace(
                &cpu,
                "fs",
                "read_text",
                &format!(".memory/{idx}.txt"),
                true,
                true,
                "ok",
            )
            .expect("append trace");
        }
        assert_eq!(rt.state.proxy_log.len(), PROXY_LOG_MAX_ENTRIES);
        let first_target = rt.state.proxy_log[0].target.clone();
        assert_eq!(
            first_target,
            format!(".memory/{}.txt", total - PROXY_LOG_MAX_ENTRIES)
        );
        let last_target = rt.state.proxy_log[PROXY_LOG_MAX_ENTRIES - 1].target.clone();
        assert_eq!(last_target, format!(".memory/{}.txt", total - 1));
    }

    #[test]
    fn get_proxy_log_rejects_zero_limit() {
        let rt = test_runtime();
        let err = rt.get_proxy_log(Some(0)).expect_err("must fail");
        assert!(err.to_string().contains("limit must be > 0"));
    }

    #[test]
    fn role_profile_defaults_to_orchestrator() {
        let rt = test_runtime();
        assert_eq!(rt.state.active_role_profile, "orchestrator");
        assert!(
            rt.allowed_tools_for_active_role()
                .iter()
                .any(|x| x == "cabal.set_role_profile")
        );
    }

    #[test]
    fn role_policy_denies_tool_outside_profile() {
        let mut rt = test_runtime();
        rt.state.active_role_profile = "conceptualizer".to_string();
        let err = rt
            .ensure_tool_allowed_for_active_role("cabal.proxy_execute")
            .expect_err("must deny");
        assert!(err.to_string().contains("policy deny"));
    }

    #[test]
    fn role_switch_request_and_reject_flow_updates_pending_state() {
        let mut rt = test_runtime();
        let out = rt
            .request_role_switch(
                "conceptualizer".to_string(),
                Some("architect".to_string()),
                Some("handoff".to_string()),
            )
            .expect("request role switch");
        assert_eq!(
            out["pending_role_switch"]["target_role"].as_str(),
            Some("conceptualizer")
        );

        let out = rt
            .approve_role_switch(false, Some("orchestrator".to_string()), None)
            .expect("reject role switch");
        assert_eq!(out["pending_role_switch"], Value::Null);
        assert_eq!(out["active_role_profile"].as_str(), Some("orchestrator"));
    }

    #[test]
    fn role_profile_list_contains_expected_profiles() {
        let rt = test_runtime();
        let out = rt.list_role_profiles();
        let profiles = out["profiles"].as_object().expect("profiles");
        assert!(profiles.contains_key("orchestrator"));
        assert!(profiles.contains_key("rust_engineer"));
    }

    #[test]
    fn result_compact_policy_roundtrip_and_compaction() {
        let mut rt = test_runtime();
        rt.set_result_compact_policy(Some(true), Some(256), Some(4))
            .expect("set policy");
        let policy = rt.get_result_compact_policy();
        assert_eq!(policy["enabled"].as_bool(), Some(true));
        assert_eq!(policy["max_chars"].as_u64(), Some(256));
        assert_eq!(policy["preview_items"].as_u64(), Some(4));

        let large = json!({
            "items": (0..200).map(|x| format!("item-{x}")).collect::<Vec<_>>()
        });
        let compacted = rt
            .compact_result_value(&large, None)
            .expect("compact result");
        assert_eq!(compacted["truncated"].as_bool(), Some(true));
        assert!(
            compacted["text"]
                .as_str()
                .unwrap_or_default()
                .chars()
                .count()
                <= 256
        );
    }

    #[test]
    fn context_window_policy_roundtrip() {
        let mut rt = test_runtime();
        rt.set_context_window_policy(Some(false), Some(15), Some(24))
            .expect("set context policy");
        let policy = rt.get_context_window_policy();
        assert_eq!(policy["lazy_tool_search"].as_bool(), Some(false));
        assert_eq!(policy["lazy_threshold_pct"].as_u64(), Some(15));
        assert_eq!(policy["programmatic_max_calls"].as_u64(), Some(24));
    }
}

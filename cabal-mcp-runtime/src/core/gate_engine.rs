use crate::core::gate::{GateReport, gate_item};
use crate::core::phase::{
    phase_order_index as core_phase_order_index,
    required_exit_evidence as core_required_exit_evidence,
};
use anyhow::{Result, bail};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub struct GateEvalContext<'a> {
    pub current_phase: &'a str,
    pub consult_mode_is_set: bool,
    pub consult_mode_is_yolo: bool,
    pub strict_artifacts: bool,
    pub evidence: &'a BTreeMap<String, String>,
}

pub fn build_gate_report(
    repo_root: &Path,
    kind: &str,
    phase: &str,
    ctx: &GateEvalContext<'_>,
) -> Result<GateReport> {
    let mut checks = Vec::new();
    let phase_dir = repo_root.join(".memory").join("PHASES").join(phase);
    let phase_index = phase_dir.join("INDEX.md");
    let phase_digest = phase_dir.join("DIGEST.md");
    let phase_tasks = phase_dir.join("TASKS.md");
    let phase_worklog = phase_dir.join("WORKLOG.md");
    let global_index = repo_root.join(".memory").join("GLOBAL_INDEX.md");
    let concept_master = repo_root
        .join("spec")
        .join("docs")
        .join("CONCEPT_MASTER.md");
    let concept_math_proof = repo_root
        .join("spec")
        .join("docs")
        .join("CONCEPT_MATH_PROOF.md");
    let strict = ctx.strict_artifacts;

    match kind {
        "entry" => {
            checks.push(gate_item(
                "entry_required_files_present",
                !strict
                    || (global_index.exists() && phase_index.exists() && concept_master.exists()),
                "strict gate requires GLOBAL_INDEX, PHASE INDEX and CONCEPT_MASTER to exist"
                    .to_string(),
            ));
            checks.push(gate_item(
                "phase_known",
                core_phase_order_index(phase).is_some(),
                format!("phase={phase}"),
            ));
            if let Some(active) = parse_active_phase_from_global_index(&global_index) {
                let pass = active == phase || active == ctx.current_phase;
                checks.push(gate_item(
                    "global_active_phase",
                    pass,
                    format!(
                        "active phase in GLOBAL_INDEX must match target ({phase}) or current ({})",
                        ctx.current_phase
                    ),
                ));
            } else {
                checks.push(gate_item(
                    "global_active_phase",
                    !strict,
                    if strict {
                        "GLOBAL_INDEX active phase is required in strict gate mode".to_string()
                    } else {
                        "GLOBAL_INDEX not found, skipped in local test sandbox".to_string()
                    },
                ));
            }
            checks.push(gate_item(
                "entry_criteria_filled",
                markdown_criteria_filled(&phase_index, "Entry Criteria:"),
                format!("entry criteria must be filled in {}", phase_index.display()),
            ));
            checks.push(gate_item(
                "inputs_available",
                markdown_paths_exist_in_section(repo_root, &phase_index, "## Inputs"),
                format!("inputs from {} must exist", phase_index.display()),
            ));
            checks.push(gate_item(
                "cross_rules_declared",
                concept_master_cross_rules_present(&concept_master),
                "CONCEPT_MASTER must contain cross-rules section for all agents/subagents"
                    .to_string(),
            ));
            checks.push(gate_item(
                "cross_rules_agent_ack",
                ctx.evidence.contains_key("cross_rules_agent_ack"),
                "before phase entry, evidence cross_rules_agent_ack is required".to_string(),
            ));
            checks.push(gate_item(
                "cross_rules_subagent_ack",
                ctx.evidence.contains_key("cross_rules_subagent_ack"),
                "before phase entry, evidence cross_rules_subagent_ack is required".to_string(),
            ));
            checks.push(gate_item(
                "consult_mode_set",
                ctx.consult_mode_is_set,
                "consult mode must be set".to_string(),
            ));
            if phase == "GA-1" {
                checks.push(gate_item(
                    "ga1_consult_mode_in_concept_master",
                    concept_master_has_consult_mode(&concept_master),
                    "GA-1 requires consult mode fixed in CONCEPT_MASTER section 6.7".to_string(),
                ));
            }
            if phase == "GA-1" && ctx.consult_mode_is_yolo {
                checks.push(gate_item(
                    "ga1_yolo_worklog_exists",
                    !strict || phase_worklog.exists(),
                    "strict gate requires PHASES/GA-1/WORKLOG.md for YOLO activation checks"
                        .to_string(),
                ));
                checks.push(gate_item(
                    "yolo_activation_evidence",
                    ctx.evidence.contains_key("ga1_yolo_activated"),
                    "GA-1 + YOLO requires evidence ga1_yolo_activated".to_string(),
                ));
                checks.push(gate_item(
                    "yolo_additional_rules",
                    concept_master_has_yolo_additional_rules(&concept_master),
                    "GA-1 + YOLO requires additional cross-rules in CONCEPT_MASTER".to_string(),
                ));
                checks.push(gate_item(
                    "yolo_activation_worklog",
                    worklog_contains_yolo_activation(&phase_worklog),
                    "GA-1 + YOLO requires activation record in PHASES/GA-1/WORKLOG.md".to_string(),
                ));
            }
        }
        "exit" => {
            checks.push(gate_item(
                "exit_required_files_present",
                !strict || (global_index.exists() && phase_index.exists() && phase_digest.exists()),
                "strict gate requires GLOBAL_INDEX, PHASE INDEX and PHASE DIGEST to exist"
                    .to_string(),
            ));
            checks.push(gate_item(
                "phase_known",
                core_phase_order_index(phase).is_some(),
                format!("phase={phase}"),
            ));
            checks.push(gate_item(
                "exit_criteria_filled",
                markdown_criteria_filled(&phase_index, "Exit Criteria:"),
                format!("exit criteria must be filled in {}", phase_index.display()),
            ));
            checks.push(gate_item(
                "digest_filled",
                file_is_nonempty_and_has_no_todo(&phase_digest),
                format!("digest must be filled: {}", phase_digest.display()),
            ));
            checks.push(gate_item(
                "evidence_paths_exist",
                markdown_paths_exist_in_section(repo_root, &phase_index, "## Evidence"),
                format!("evidence paths from {} must exist", phase_index.display()),
            ));
            if let Some(status) = parse_phase_status_from_global_index(&global_index, phase) {
                checks.push(gate_item(
                    "global_phase_status_updated",
                    status == "done",
                    format!("phase status in GLOBAL_INDEX must be done (actual={status})"),
                ));
            } else {
                checks.push(gate_item(
                    "global_phase_status_updated",
                    !strict,
                    if strict {
                        "GLOBAL_INDEX phase status is required in strict gate mode".to_string()
                    } else {
                        "GLOBAL_INDEX phase status not found, skipped in local test sandbox"
                            .to_string()
                    },
                ));
            }
            let required = core_required_exit_evidence(phase)?;
            for key in required {
                checks.push(gate_item(
                    &format!("evidence:{key}"),
                    ctx.evidence.contains_key(*key),
                    format!("required evidence key={key}"),
                ));
            }
            if phase == "C-0" {
                checks.push(gate_item(
                    "c0_required_files_present",
                    !strict
                        || (phase_tasks.exists()
                            && phase_worklog.exists()
                            && concept_master.exists()
                            && concept_math_proof.exists()),
                    "strict gate requires C-0 TASKS/WORKLOG and canonical docs to exist"
                        .to_string(),
                ));
                checks.push(gate_item(
                    "c0_tasks_1_6_closed",
                    c0_tasks_closed(&phase_tasks),
                    "C-0.1..C-0.6 must be closed in PHASES/C-0/TASKS.md".to_string(),
                ));
                checks.push(gate_item(
                    "c0_concept_sync_note",
                    c0_worklog_contains_sync_note(&phase_worklog),
                    "C-0 WORKLOG must contain canon sync note (CONCEPT_MASTER + CONCEPT_MATH_PROOF)"
                        .to_string(),
                ));
                checks.push(gate_item(
                    "c0_docs_no_todo",
                    c0_docs_have_no_todo(&concept_master, &concept_math_proof),
                    "CONCEPT_MASTER and CONCEPT_MATH_PROOF must not contain TODO in core"
                        .to_string(),
                ));
                checks.push(gate_item(
                    "c0_sync_rule_declared",
                    c0_sync_rule_present(&concept_master),
                    "CONCEPT_MASTER must contain C-0 sync rule (section 6.6)".to_string(),
                ));
                checks.push(gate_item(
                    "c0_latest_cycle_closed",
                    c0_latest_cycle_closed(&phase_tasks),
                    "if user changed variant/params, C-0.2..C-0.6 must be re-opened and re-closed"
                        .to_string(),
                ));
            }
        }
        _ => bail!("unsupported gate kind: {kind}"),
    }

    let pass = checks.iter().all(|x| x.pass);
    Ok(GateReport {
        kind: kind.to_string(),
        phase: phase.to_string(),
        pass,
        checks,
    })
}

fn parse_active_phase_from_global_index(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("- ID:") {
            let v = rest.trim();
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn parse_phase_status_from_global_index(path: &Path, phase: &str) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            continue;
        }
        let cols: Vec<&str> = trimmed.split('|').map(|x| x.trim()).collect();
        if cols.len() >= 5 && cols[1] == phase {
            return Some(cols[3].to_string());
        }
    }
    None
}

fn markdown_criteria_filled(path: &Path, marker: &str) -> bool {
    let text = match fs::read_to_string(path) {
        Ok(v) => v,
        Err(_) => return true,
    };
    let lines = text.lines().collect::<Vec<_>>();
    let mut start_idx = None;
    for (i, line) in lines.iter().enumerate() {
        if line.contains(marker) {
            start_idx = Some(i + 1);
            break;
        }
    }
    let Some(mut idx) = start_idx else {
        return false;
    };
    let mut has_value = false;
    while idx < lines.len() {
        let t = lines[idx].trim();
        if t.starts_with("## ") {
            break;
        }
        if t.contains("Entry Criteria:") || t.contains("Exit Criteria:") {
            break;
        }
        if t.starts_with('-') {
            let val = t
                .trim_start_matches('-')
                .trim()
                .trim_matches('`')
                .to_ascii_lowercase();
            if !val.is_empty() && !val.contains("todo") && val != "нет" && val != "n/a" {
                has_value = true;
            }
        }
        idx += 1;
    }
    has_value
}

fn extract_section_lines(path: &Path, section_header: &str) -> Vec<String> {
    let text = match fs::read_to_string(path) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let mut in_section = false;
    let mut out = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("## ") {
            if in_section {
                break;
            }
            if t.eq_ignore_ascii_case(section_header) {
                in_section = true;
                continue;
            }
        }
        if in_section {
            out.push(line.to_string());
        }
    }
    out
}

fn extract_markdown_paths(lines: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for line in lines {
        let t = line.trim();
        if !t.starts_with('-') {
            continue;
        }
        if let Some(start) = t.find('`')
            && let Some(rel_end) = t[start + 1..].find('`')
        {
            let p = t[start + 1..start + 1 + rel_end].trim();
            if !p.is_empty() {
                out.push(p.to_string());
            }
            continue;
        }
        let raw = t
            .trim_start_matches('-')
            .trim()
            .trim_end_matches('.')
            .trim_end_matches(';')
            .trim();
        if raw.contains('/') || raw.contains('\\') {
            out.push(raw.to_string());
        }
    }
    out
}

fn markdown_paths_exist_in_section(
    repo_root: &Path,
    index_path: &Path,
    section_header: &str,
) -> bool {
    let lines = extract_section_lines(index_path, section_header);
    if lines.is_empty() {
        return true;
    }
    let paths = extract_markdown_paths(&lines);
    for rel in paths {
        if rel.contains("LOGIC_PROTOCOL.md (") {
            if !repo_root.join(".memory").join("LOGIC_PROTOCOL.md").exists() {
                return false;
            }
            continue;
        }
        if !repo_root.join(rel).exists() {
            return false;
        }
    }
    true
}

fn file_is_nonempty_and_has_no_todo(path: &Path) -> bool {
    let text = match fs::read_to_string(path) {
        Ok(v) => v,
        Err(_) => return true,
    };
    if text.trim().is_empty() {
        return false;
    }
    !text.to_ascii_lowercase().contains("todo")
}

fn concept_master_cross_rules_present(path: &Path) -> bool {
    let text = match fs::read_to_string(path) {
        Ok(v) => v,
        Err(_) => return true,
    };
    let lower = text.to_ascii_lowercase();
    lower.contains("6.9")
        && lower.contains("сквоз")
        && lower.contains("агент")
        && lower.contains("субагент")
}

fn concept_master_has_consult_mode(path: &Path) -> bool {
    let text = match fs::read_to_string(path) {
        Ok(v) => v,
        Err(_) => return true,
    };
    for line in text.lines() {
        let t = line.trim().to_ascii_lowercase();
        if t.starts_with("- mode:") {
            return t.contains("user_tracking") || t.contains("yolo");
        }
    }
    false
}

fn concept_master_has_yolo_additional_rules(path: &Path) -> bool {
    let text = match fs::read_to_string(path) {
        Ok(v) => v,
        Err(_) => return true,
    };
    for line in text.lines() {
        let t = line.trim().to_ascii_lowercase();
        if t.starts_with("- yolo_additional_rules:") {
            return !t.contains("none") && !t.contains("n/a");
        }
    }
    false
}

fn worklog_contains_yolo_activation(path: &Path) -> bool {
    let text = match fs::read_to_string(path) {
        Ok(v) => v,
        Err(_) => return true,
    };
    let lower = text.to_ascii_lowercase();
    lower.contains("yolo") && (lower.contains("активац") || lower.contains("activated"))
}

fn c0_tasks_closed(path: &Path) -> bool {
    let text = match fs::read_to_string(path) {
        Ok(v) => v,
        Err(_) => return true,
    };
    for id in ["C-0.1", "C-0.2", "C-0.3", "C-0.4", "C-0.5", "C-0.6"] {
        let marker = format!("- [x] T {id}");
        if !text.contains(&marker) {
            return false;
        }
    }
    true
}

fn c0_latest_cycle_closed(path: &Path) -> bool {
    let text = match fs::read_to_string(path) {
        Ok(v) => v,
        Err(_) => return true,
    };
    for id in ["C-0.2", "C-0.3", "C-0.4", "C-0.5", "C-0.6"] {
        let marker = format!("- [ ] T {id}");
        if text.contains(&marker) {
            return false;
        }
    }
    true
}

fn c0_worklog_contains_sync_note(path: &Path) -> bool {
    let text = match fs::read_to_string(path) {
        Ok(v) => v,
        Err(_) => return true,
    };
    let lower = text.to_ascii_lowercase();
    lower.contains("синхрон")
        && lower.contains("concept_master")
        && lower.contains("concept_math_proof")
}

fn c0_docs_have_no_todo(concept_master: &Path, concept_math_proof: &Path) -> bool {
    let cm = match fs::read_to_string(concept_master) {
        Ok(v) => v,
        Err(_) => return true,
    };
    let mp = match fs::read_to_string(concept_math_proof) {
        Ok(v) => v,
        Err(_) => return true,
    };
    !cm.to_ascii_lowercase().contains("todo") && !mp.to_ascii_lowercase().contains("todo")
}

fn c0_sync_rule_present(path: &Path) -> bool {
    let text = match fs::read_to_string(path) {
        Ok(v) => v,
        Err(_) => return true,
    };
    let lower = text.to_ascii_lowercase();
    lower.contains("6.6")
        && lower.contains("обновление канона")
        && lower.contains("concept_master")
        && lower.contains("concept_math_proof")
}

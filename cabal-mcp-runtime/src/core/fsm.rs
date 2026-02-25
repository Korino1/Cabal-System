use crate::core::gate::GateReport;
use crate::core::phase::is_valid_transition;
use anyhow::{Result, bail};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseTransitionDecision {
    pub changed: bool,
    pub from_phase: String,
    pub to_phase: String,
}

pub fn transition_phase(
    current_phase: &str,
    target_phase: &str,
) -> Result<PhaseTransitionDecision> {
    if current_phase == target_phase {
        return Ok(PhaseTransitionDecision {
            changed: false,
            from_phase: current_phase.to_string(),
            to_phase: current_phase.to_string(),
        });
    }
    if !is_valid_transition(current_phase, target_phase) {
        bail!(
            "invalid phase transition: current={} target={}",
            current_phase,
            target_phase
        );
    }
    Ok(PhaseTransitionDecision {
        changed: true,
        from_phase: current_phase.to_string(),
        to_phase: target_phase.to_string(),
    })
}

pub fn validate_strict_phase_transition(
    current_phase: &str,
    target_phase: &str,
    exit_report: &GateReport,
    entry_report: &GateReport,
) -> Result<()> {
    if !exit_report.pass {
        bail!(
            "exit gate failed for phase {}: {}",
            current_phase,
            serde_json::to_string(exit_report)?
        );
    }
    if !entry_report.pass {
        bail!(
            "entry gate failed for phase {}: {}",
            target_phase,
            serde_json::to_string(entry_report)?
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pass_report(kind: &str, phase: &str, pass: bool) -> GateReport {
        GateReport {
            kind: kind.to_string(),
            phase: phase.to_string(),
            pass,
            checks: Vec::new(),
        }
    }

    #[test]
    fn transition_rejects_invalid_skip() {
        let err = transition_phase("C-0", "GA-2").expect_err("skip must fail");
        assert!(err.to_string().contains("invalid phase transition"));
    }

    #[test]
    fn strict_validation_fails_on_exit_report() {
        let err = validate_strict_phase_transition(
            "GA-1",
            "GA-2",
            &pass_report("exit", "GA-1", false),
            &pass_report("entry", "GA-2", true),
        )
        .expect_err("exit fail");
        assert!(err.to_string().contains("exit gate failed"));
    }

    #[test]
    fn strict_validation_passes_when_reports_are_green() {
        validate_strict_phase_transition(
            "GA-1",
            "GA-2",
            &pass_report("exit", "GA-1", true),
            &pass_report("entry", "GA-2", true),
        )
        .expect("strict validation should pass");
    }
}

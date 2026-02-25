use anyhow::{Result, bail};

const ORDER: &[&str] = &[
    "C-0",
    "GA-1",
    "GA-2",
    "GA-3",
    "GA-4",
    "GA-5",
    "ARCH",
    "INTEGRATOR",
    "ORCHESTRATOR",
];

pub fn is_valid_transition(current: &str, target: &str) -> bool {
    let cur_idx = ORDER.iter().position(|x| *x == current);
    let tgt_idx = ORDER.iter().position(|x| *x == target);
    match (cur_idx, tgt_idx) {
        (Some(c), Some(t)) => t == c + 1,
        _ => false,
    }
}

pub fn phase_order_index(phase: &str) -> Option<usize> {
    ORDER.iter().position(|x| *x == phase)
}

pub fn required_exit_evidence(phase: &str) -> Result<&'static [&'static str]> {
    match phase {
        "C-0" => Ok(&["concept_master", "concept_math_proof", "c0_digest"]),
        "GA-1" => Ok(&["ga1_schema", "ga1_digest"]),
        "GA-2" => Ok(&["ga2_methods", "ga2_digest"]),
        "GA-3" => Ok(&["ga3_block_schemas", "ga3_digest"]),
        "GA-4" => Ok(&["ga4_functions", "ga4_digest"]),
        "GA-5" => Ok(&["ga5_descriptions", "ga5_digest"]),
        "ARCH" => Ok(&["arch_digest"]),
        "INTEGRATOR" => Ok(&["integrator_digest"]),
        "ORCHESTRATOR" => Ok(&["orchestrator_digest"]),
        _ => bail!("unknown phase: {phase}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_accepts_next_only() {
        assert!(is_valid_transition("C-0", "GA-1"));
        assert!(!is_valid_transition("C-0", "GA-2"));
    }

    #[test]
    fn required_evidence_known_phase() {
        let ev = required_exit_evidence("GA-3").expect("phase");
        assert!(ev.iter().any(|x| *x == "ga3_digest"));
    }

    #[test]
    fn required_evidence_unknown_phase_fails() {
        assert!(required_exit_evidence("X").is_err());
    }
}

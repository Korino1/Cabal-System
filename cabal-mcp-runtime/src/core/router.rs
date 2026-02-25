use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConsultExecutorTelemetry {
    #[serde(default)]
    pub successes: u64,
    #[serde(default)]
    pub failures: u64,
    #[serde(default)]
    pub total_feedback: u64,
    #[serde(default)]
    pub avg_latency_ms: u64,
    #[serde(default)]
    pub latency_samples: u64,
    #[serde(default)]
    pub last_ts_unix: u64,
}

pub fn consult_feedback_key(consult_type: &str, executor: &str) -> String {
    format!(
        "{}::{}",
        consult_type.trim().to_ascii_lowercase(),
        executor.trim().to_ascii_lowercase()
    )
}

pub fn default_consult_routing_map() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("general".to_string(), "generalist".to_string()),
        ("math".to_string(), "mathematician".to_string()),
        ("proof".to_string(), "mathematician".to_string()),
        ("security".to_string(), "security_reviewer".to_string()),
        ("performance".to_string(), "perf_engineer".to_string()),
        ("optimization".to_string(), "perf_engineer".to_string()),
        ("architecture".to_string(), "architect".to_string()),
        ("design".to_string(), "architect".to_string()),
        ("code".to_string(), "developer".to_string()),
        ("debug".to_string(), "developer".to_string()),
        ("refactor".to_string(), "developer".to_string()),
        ("test".to_string(), "developer".to_string()),
    ])
}

pub fn default_consult_priority_timeouts() -> BTreeMap<String, u64> {
    BTreeMap::from([
        ("low".to_string(), 7200),
        ("normal".to_string(), 3600),
        ("high".to_string(), 900),
        ("critical".to_string(), 300),
    ])
}

pub fn default_consult_retry_limits() -> BTreeMap<String, u64> {
    BTreeMap::from([
        ("low".to_string(), 2),
        ("normal".to_string(), 2),
        ("high".to_string(), 1),
        ("critical".to_string(), 0),
    ])
}

pub fn default_consult_escalation_targets() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("low".to_string(), "none".to_string()),
        ("normal".to_string(), "none".to_string()),
        ("high".to_string(), "orchestrator".to_string()),
        ("critical".to_string(), "user".to_string()),
    ])
}

pub fn default_consult_allowed_roles() -> BTreeMap<String, Vec<String>> {
    BTreeMap::from([
        (
            "general".to_string(),
            vec![
                "generalist".to_string(),
                "developer".to_string(),
                "architect".to_string(),
                "security_reviewer".to_string(),
                "mathematician".to_string(),
                "perf_engineer".to_string(),
            ],
        ),
        ("math".to_string(), vec!["mathematician".to_string()]),
        ("proof".to_string(), vec!["mathematician".to_string()]),
        (
            "security".to_string(),
            vec!["security_reviewer".to_string()],
        ),
        (
            "performance".to_string(),
            vec!["perf_engineer".to_string(), "developer".to_string()],
        ),
        ("architecture".to_string(), vec!["architect".to_string()]),
        ("design".to_string(), vec!["architect".to_string()]),
        ("code".to_string(), vec!["developer".to_string()]),
        ("debug".to_string(), vec!["developer".to_string()]),
        ("refactor".to_string(), vec!["developer".to_string()]),
        ("test".to_string(), vec!["developer".to_string()]),
    ])
}

pub fn normalize_consult_priority(value: &str) -> Result<String> {
    let out = value.to_ascii_lowercase();
    match out.as_str() {
        "low" | "normal" | "high" | "critical" => Ok(out),
        _ => bail!("unsupported consult priority: {value}"),
    }
}

pub fn consult_timeout_sec(priority: &str) -> u64 {
    match priority {
        "critical" => 300,
        "high" => 900,
        "normal" => 3600,
        "low" => 7200,
        _ => 3600,
    }
}

pub fn consult_retry_limit(priority: &str) -> u64 {
    match priority {
        "critical" => 0,
        "high" => 1,
        "normal" => 2,
        "low" => 2,
        _ => 2,
    }
}

pub fn consult_escalation_target(priority: &str) -> &'static str {
    match priority {
        "critical" => "user",
        "high" => "orchestrator",
        _ => "none",
    }
}

pub fn normalize_escalation_target(value: &str) -> Result<String> {
    let out = value.trim().to_ascii_lowercase();
    match out.as_str() {
        "none" | "user" | "orchestrator" | "architect" | "security_reviewer" => Ok(out),
        _ => bail!("unsupported escalation target: {value}"),
    }
}

pub fn select_executor_for_consult(consult_type: &str) -> &'static str {
    match consult_type {
        "math" | "proof" => "mathematician",
        "security" => "security_reviewer",
        "performance" | "optimization" => "perf_engineer",
        "architecture" | "design" => "architect",
        "code" | "debug" | "refactor" | "test" => "developer",
        _ => "generalist",
    }
}

pub fn resolve_consult_timeout(priority: &str, overrides: &BTreeMap<String, u64>) -> u64 {
    overrides
        .get(priority)
        .copied()
        .unwrap_or_else(|| consult_timeout_sec(priority))
}

pub fn resolve_consult_retries(priority: &str, overrides: &BTreeMap<String, u64>) -> u64 {
    overrides
        .get(priority)
        .copied()
        .unwrap_or_else(|| consult_retry_limit(priority))
}

pub fn resolve_consult_escalation(priority: &str, overrides: &BTreeMap<String, String>) -> String {
    overrides
        .get(priority)
        .cloned()
        .unwrap_or_else(|| consult_escalation_target(priority).to_string())
}

pub fn resolve_consult_executor(
    consult_type: &str,
    routing_map: &BTreeMap<String, String>,
) -> String {
    routing_map
        .get(consult_type)
        .cloned()
        .or_else(|| routing_map.get("general").cloned())
        .unwrap_or_else(|| select_executor_for_consult(consult_type).to_string())
}

pub fn consult_allowed_roles_for_type(
    consult_type: &str,
    allowed_roles_map: &BTreeMap<String, Vec<String>>,
) -> Vec<String> {
    allowed_roles_map
        .get(consult_type)
        .or_else(|| allowed_roles_map.get("general"))
        .cloned()
        .unwrap_or_default()
}

pub fn resolve_consult_allowed_role_fallback(
    consult_type: &str,
    allowed: &[String],
    routing_map: &BTreeMap<String, String>,
) -> Option<String> {
    if allowed.is_empty() {
        return None;
    }

    let mut candidates: Vec<String> = Vec::new();
    if let Some(executor) = routing_map.get(consult_type) {
        candidates.push(executor.to_ascii_lowercase());
    }
    if let Some(executor) = routing_map.get("general") {
        candidates.push(executor.to_ascii_lowercase());
    }
    candidates.push(select_executor_for_consult(consult_type).to_ascii_lowercase());

    for candidate in candidates {
        if allowed.iter().any(|role| role == &candidate) {
            return Some(candidate);
        }
    }
    allowed.first().cloned()
}

pub fn is_consult_role_allowed(
    consult_type: &str,
    role: &str,
    allowed_roles_map: &BTreeMap<String, Vec<String>>,
) -> bool {
    let role = role.to_ascii_lowercase();
    let allowed = consult_allowed_roles_for_type(consult_type, allowed_roles_map);
    if !allowed.is_empty() {
        return allowed.iter().any(|x| x == &role);
    }
    true
}

pub fn resolve_adaptive_consult_executor(
    consult_type: &str,
    priority: &str,
    allowed_roles: &[String],
    routing_map: &BTreeMap<String, String>,
    telemetry: &BTreeMap<String, ConsultExecutorTelemetry>,
) -> Option<(String, f64, f64)> {
    let policy_executor = resolve_consult_executor(consult_type, routing_map);
    let fallback_executor = select_executor_for_consult(consult_type).to_ascii_lowercase();
    select_adaptive_executor(
        consult_type,
        priority,
        allowed_roles,
        &policy_executor,
        &fallback_executor,
        telemetry,
    )
}

pub fn should_use_adaptive_exploration(seed: &str, rate: f64) -> bool {
    if !rate.is_finite() || rate <= 0.0 {
        return false;
    }
    if rate >= 1.0 {
        return true;
    }
    stable_fraction(seed) < rate
}

pub fn resolve_adaptive_exploration_executor(
    consult_type: &str,
    priority: &str,
    allowed_roles: &[String],
    telemetry: &BTreeMap<String, ConsultExecutorTelemetry>,
    min_samples: u64,
    seed: &str,
) -> Option<(String, f64, f64)> {
    if allowed_roles.is_empty() || min_samples == 0 {
        return None;
    }
    let mut candidates: Vec<(String, u64, f64, f64)> = Vec::new();
    for role in allowed_roles {
        let role = role.to_ascii_lowercase();
        let key = consult_feedback_key(consult_type, &role);
        let metrics = telemetry.get(&key);
        let outcomes = metrics
            .map(|m| m.successes.saturating_add(m.failures))
            .unwrap_or(0);
        if outcomes >= min_samples {
            continue;
        }
        let (score, confidence) = score_consult_executor(priority, metrics);
        candidates.push((role, outcomes, score, confidence));
    }
    if candidates.is_empty() {
        return None;
    }
    candidates.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    let idx = (stable_hash_u64(seed) % candidates.len() as u64) as usize;
    let selected = candidates[idx].clone();
    Some((selected.0, selected.2, selected.3))
}

pub fn select_adaptive_executor(
    consult_type: &str,
    priority: &str,
    allowed_roles: &[String],
    policy_executor: &str,
    fallback_executor: &str,
    telemetry: &BTreeMap<String, ConsultExecutorTelemetry>,
) -> Option<(String, f64, f64)> {
    if allowed_roles.is_empty() {
        return None;
    }
    let policy_executor = policy_executor.to_ascii_lowercase();
    let fallback_executor = fallback_executor.to_ascii_lowercase();
    let mut best: Option<(String, f64, f64)> = None;

    for role in allowed_roles {
        let role = role.to_ascii_lowercase();
        let key = consult_feedback_key(consult_type, &role);
        let (mut score, confidence) = score_consult_executor(priority, telemetry.get(&key));
        if role == policy_executor {
            score += 0.05;
        }
        if role == fallback_executor {
            score += 0.03;
        }
        score = score.clamp(0.0, 1.0);
        match &best {
            None => best = Some((role, score, confidence)),
            Some((_, best_score, best_confidence)) => {
                if score > *best_score
                    || ((score - *best_score).abs() < f64::EPSILON && confidence > *best_confidence)
                {
                    best = Some((role, score, confidence));
                }
            }
        }
    }

    best
}

pub fn score_consult_executor(
    priority: &str,
    metrics: Option<&ConsultExecutorTelemetry>,
) -> (f64, f64) {
    let Some(metrics) = metrics else {
        return (0.5, 0.0);
    };

    let outcomes = metrics.successes.saturating_add(metrics.failures);
    let success_rate = (metrics.successes as f64 + 1.0) / (outcomes as f64 + 2.0);
    let latency_quality = if metrics.latency_samples > 0 {
        1.0 - (metrics.avg_latency_ms.min(10_000) as f64 / 10_000.0)
    } else {
        0.5
    };
    let (success_weight, latency_weight) = match priority {
        "critical" => (0.85, 0.15),
        "high" => (0.75, 0.25),
        "normal" => (0.65, 0.35),
        "low" => (0.55, 0.45),
        _ => (0.65, 0.35),
    };
    let score = success_rate * success_weight + latency_quality * latency_weight;
    let confidence = (outcomes as f64).min(20.0) / 20.0;
    (score.clamp(0.0, 1.0), confidence.clamp(0.0, 1.0))
}

fn stable_hash_u64(seed: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in seed.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn stable_fraction(seed: &str) -> f64 {
    (stable_hash_u64(seed) as f64) / (u64::MAX as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_prefers_success_and_low_latency() {
        let metrics = ConsultExecutorTelemetry {
            successes: 10,
            failures: 1,
            total_feedback: 11,
            avg_latency_ms: 80,
            latency_samples: 11,
            last_ts_unix: 1,
        };
        let (score, confidence) = score_consult_executor("high", Some(&metrics));
        assert!(score > 0.7);
        assert!(confidence > 0.5);
    }

    #[test]
    fn adaptive_selection_prefers_higher_score() {
        let mut telemetry = BTreeMap::new();
        telemetry.insert(
            consult_feedback_key("performance", "developer"),
            ConsultExecutorTelemetry {
                successes: 1,
                failures: 9,
                total_feedback: 10,
                avg_latency_ms: 2500,
                latency_samples: 10,
                last_ts_unix: 1,
            },
        );
        telemetry.insert(
            consult_feedback_key("performance", "perf_engineer"),
            ConsultExecutorTelemetry {
                successes: 9,
                failures: 1,
                total_feedback: 10,
                avg_latency_ms: 120,
                latency_samples: 10,
                last_ts_unix: 1,
            },
        );

        let selected = select_adaptive_executor(
            "performance",
            "high",
            &["developer".to_string(), "perf_engineer".to_string()],
            "developer",
            "perf_engineer",
            &telemetry,
        )
        .expect("selection");
        assert_eq!(selected.0, "perf_engineer");
    }

    #[test]
    fn normalize_priority_accepts_known_values() {
        assert_eq!(
            normalize_consult_priority("HIGH").expect("normalize"),
            "high"
        );
        assert!(normalize_consult_priority("urgent").is_err());
    }

    #[test]
    fn defaults_include_required_roles() {
        let map = default_consult_allowed_roles();
        let math = map.get("math").expect("math role");
        assert!(math.iter().any(|x| x == "mathematician"));
        let perf = map.get("performance").expect("performance roles");
        assert!(perf.iter().any(|x| x == "perf_engineer"));
    }

    #[test]
    fn resolve_timeout_prefers_override() {
        let overrides = BTreeMap::from([("high".to_string(), 777)]);
        assert_eq!(resolve_consult_timeout("high", &overrides), 777);
        assert_eq!(resolve_consult_timeout("critical", &overrides), 300);
    }

    #[test]
    fn fallback_uses_allowed_policy_candidate() {
        let routing = BTreeMap::from([
            ("general".to_string(), "developer".to_string()),
            ("math".to_string(), "mathematician".to_string()),
        ]);
        let allowed = vec!["developer".to_string(), "architect".to_string()];
        let selected = resolve_consult_allowed_role_fallback("math", &allowed, &routing)
            .expect("fallback role");
        assert_eq!(selected, "developer");
    }

    #[test]
    fn role_allowed_is_true_when_allowlist_is_empty() {
        let allow = BTreeMap::new();
        assert!(is_consult_role_allowed("general", "anything", &allow));
    }

    #[test]
    fn adaptive_exploration_rate_bounds() {
        assert!(!should_use_adaptive_exploration("seed-a", 0.0));
        assert!(should_use_adaptive_exploration("seed-a", 1.0));
        assert!(!should_use_adaptive_exploration("seed-a", -0.1));
    }

    #[test]
    fn adaptive_exploration_selects_undertrained_executor() {
        let mut telemetry = BTreeMap::new();
        telemetry.insert(
            consult_feedback_key("performance", "developer"),
            ConsultExecutorTelemetry {
                successes: 10,
                failures: 2,
                total_feedback: 12,
                avg_latency_ms: 200,
                latency_samples: 12,
                last_ts_unix: 1,
            },
        );
        telemetry.insert(
            consult_feedback_key("performance", "perf_engineer"),
            ConsultExecutorTelemetry {
                successes: 1,
                failures: 0,
                total_feedback: 1,
                avg_latency_ms: 150,
                latency_samples: 1,
                last_ts_unix: 1,
            },
        );

        let selected = resolve_adaptive_exploration_executor(
            "performance",
            "high",
            &["developer".to_string(), "perf_engineer".to_string()],
            &telemetry,
            5,
            "rq-explore-1",
        )
        .expect("exploration selection");
        assert_eq!(selected.0, "perf_engineer");
    }
}

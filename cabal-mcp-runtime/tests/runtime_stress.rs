use cabal_mcp_runtime::cpu::CpuProfile;
use cabal_mcp_runtime::runtime::CabalRuntime;
use std::fs;
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const STRESS_SLA_QUERY_P99_MS: u64 = 10_000;
const STRESS_SLA_EXPORT_P99_MS: u64 = 10_000;
const STRESS_SLA_REPLAY_P99_MS: u64 = 10_000;

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
#[ignore = "stress profile: run explicitly"]
fn stress_audit_query_export_replay_profile() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let root = temp_root("cabal_stress_audit_profile");
    fs::create_dir_all(&root).expect("mkdir");
    let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

    let total_events = 10_000usize;
    let begin_ingest = Instant::now();
    for idx in 0..total_events {
        runtime
            .record_event(
                &cpu,
                "probe.stress".to_string(),
                serde_json::json!({"idx": idx, "request_id": format!("rq-stress-{idx}")}),
            )
            .expect("record event");
    }
    let ingest_elapsed = begin_ingest.elapsed();

    let begin_query = Instant::now();
    let query = runtime
        .query_audit_log(
            Some("event.recorded".to_string()),
            None,
            None,
            None,
            None,
            None,
            Some(200_000),
        )
        .expect("query");
    let query_elapsed = begin_query.elapsed();

    let begin_export = Instant::now();
    let export = runtime
        .export_audit_log(
            ".memory/stress_export.jsonl".to_string(),
            Some("event.recorded".to_string()),
            None,
            None,
            None,
            None,
            None,
            Some(200_000),
        )
        .expect("export");
    let export_elapsed = begin_export.elapsed();

    let begin_replay = Instant::now();
    let replay = runtime
        .replay_audit_state(None, None)
        .expect("replay should pass");
    let replay_elapsed = begin_replay.elapsed();

    assert_eq!(query["max_limit"].as_u64(), Some(2000));
    assert_eq!(query["items"].as_array().map(|x| x.len()), Some(2000));
    assert_eq!(export["max_limit"].as_u64(), Some(2000));
    assert_eq!(export["applied_limit"].as_u64(), Some(2000));
    assert_eq!(export["exported"].as_u64(), Some(2000));
    assert!(replay["total_events"].as_u64().unwrap_or(0) >= total_events as u64);

    let ingest_ms = ingest_elapsed.as_millis() as u64;
    let query_ms = query_elapsed.as_millis() as u64;
    let export_ms = export_elapsed.as_millis() as u64;
    let replay_ms = replay_elapsed.as_millis() as u64;

    // Generous profile thresholds to catch pathological regressions.
    assert!(
        query_ms < STRESS_SLA_QUERY_P99_MS,
        "query too slow: {query_ms}ms"
    );
    assert!(
        export_ms < STRESS_SLA_EXPORT_P99_MS,
        "export too slow: {export_ms}ms"
    );
    assert!(
        replay_ms < STRESS_SLA_REPLAY_P99_MS,
        "replay too slow: {replay_ms}ms"
    );

    eprintln!(
        "stress_audit_query_export_replay_profile: ingest={}ms query={}ms export={}ms replay={}ms",
        ingest_ms, query_ms, export_ms, replay_ms
    );

    let _ = fs::remove_dir_all(root);
}

fn percentile_ms(values: &[u64], p: f64) -> u64 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let p = p.clamp(0.0, 1.0);
    let idx = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[idx]
}

#[test]
#[ignore = "stress profile: run explicitly"]
fn stress_audit_query_export_replay_multirun_p95_p99() {
    let cpu = CpuProfile::detect().expect("cpu detect");
    let runs = 5usize;
    let total_events = 5_000usize;

    let mut query_ms = Vec::with_capacity(runs);
    let mut export_ms = Vec::with_capacity(runs);
    let mut replay_ms = Vec::with_capacity(runs);

    for run in 0..runs {
        let root = temp_root(&format!("cabal_stress_audit_multirun_{run}"));
        fs::create_dir_all(&root).expect("mkdir");
        let mut runtime = CabalRuntime::load_or_create(&root, &cpu).expect("runtime");

        for idx in 0..total_events {
            runtime
                .record_event(
                    &cpu,
                    "probe.stress".to_string(),
                    serde_json::json!({"idx": idx}),
                )
                .expect("record event");
        }

        let t0 = Instant::now();
        let query = runtime
            .query_audit_log(
                Some("event.recorded".to_string()),
                None,
                None,
                None,
                None,
                None,
                Some(200_000),
            )
            .expect("query");
        query_ms.push(t0.elapsed().as_millis() as u64);

        let t1 = Instant::now();
        let export = runtime
            .export_audit_log(
                ".memory/stress_export.jsonl".to_string(),
                Some("event.recorded".to_string()),
                None,
                None,
                None,
                None,
                None,
                Some(200_000),
            )
            .expect("export");
        export_ms.push(t1.elapsed().as_millis() as u64);

        let t2 = Instant::now();
        let replay = runtime
            .replay_audit_state(None, None)
            .expect("replay should pass");
        replay_ms.push(t2.elapsed().as_millis() as u64);

        assert_eq!(query["max_limit"].as_u64(), Some(2000));
        assert_eq!(export["max_limit"].as_u64(), Some(2000));
        assert_eq!(export["applied_limit"].as_u64(), Some(2000));
        assert_eq!(export["exported"].as_u64(), Some(2000));
        assert!(replay["total_events"].as_u64().unwrap_or(0) >= total_events as u64);

        let _ = fs::remove_dir_all(root);
    }

    let query_p95 = percentile_ms(&query_ms, 0.95);
    let query_p99 = percentile_ms(&query_ms, 0.99);
    let export_p95 = percentile_ms(&export_ms, 0.95);
    let export_p99 = percentile_ms(&export_ms, 0.99);
    let replay_p95 = percentile_ms(&replay_ms, 0.95);
    let replay_p99 = percentile_ms(&replay_ms, 0.99);

    // Conservative SLA guardrails to catch regressions.
    assert!(
        query_p95 < STRESS_SLA_QUERY_P99_MS,
        "query p95 too slow: {query_p95}ms"
    );
    assert!(
        query_p99 < STRESS_SLA_QUERY_P99_MS,
        "query p99 too slow: {query_p99}ms"
    );
    assert!(
        export_p95 < STRESS_SLA_EXPORT_P99_MS,
        "export p95 too slow: {export_p95}ms"
    );
    assert!(
        export_p99 < STRESS_SLA_EXPORT_P99_MS,
        "export p99 too slow: {export_p99}ms"
    );
    assert!(
        replay_p95 < STRESS_SLA_REPLAY_P99_MS,
        "replay p95 too slow: {replay_p95}ms"
    );
    assert!(
        replay_p99 < STRESS_SLA_REPLAY_P99_MS,
        "replay p99 too slow: {replay_p99}ms"
    );

    eprintln!(
        "stress_audit_query_export_replay_multirun_p95_p99: query(p95/p99)={}/{}ms export={}/{}ms replay={}/{}ms",
        query_p95, query_p99, export_p95, export_p99, replay_p95, replay_p99
    );
}

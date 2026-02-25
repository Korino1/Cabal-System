use anyhow::{Result, anyhow, bail};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Default)]
pub struct AuditQuery {
    pub kind: Option<String>,
    pub phase: Option<String>,
    pub policy_revision: Option<u64>,
    pub request_id: Option<String>,
    pub from_ts_unix: Option<u64>,
    pub to_ts_unix: Option<u64>,
    pub limit: Option<usize>,
}

pub fn query_audit_items(items: &[Value], q: &AuditQuery) -> Result<Value> {
    if let (Some(from_ts), Some(to_ts)) = (q.from_ts_unix, q.to_ts_unix)
        && from_ts > to_ts
    {
        bail!("from_ts_unix must be <= to_ts_unix");
    }

    let mut matched = Vec::new();
    for item in items.iter().cloned() {
        if let Some(ref kind) = q.kind
            && item.get("kind").and_then(|v| v.as_str()) != Some(kind.as_str())
        {
            continue;
        }
        if let Some(ref phase) = q.phase
            && item.get("phase").and_then(|v| v.as_str()) != Some(phase.as_str())
        {
            continue;
        }
        if let Some(rev) = q.policy_revision
            && item.get("policy_revision").and_then(|v| v.as_u64()) != Some(rev)
        {
            continue;
        }
        if let Some(ref req) = q.request_id
            && !audit_item_has_request_id(&item, req)
        {
            continue;
        }
        if let Some(from_ts) = q.from_ts_unix
            && item.get("ts_unix").and_then(|v| v.as_u64()).unwrap_or(0) < from_ts
        {
            continue;
        }
        if let Some(to_ts) = q.to_ts_unix
            && item
                .get("ts_unix")
                .and_then(|v| v.as_u64())
                .unwrap_or(u64::MAX)
                > to_ts
        {
            continue;
        }
        matched.push(item);
    }

    let n = q.limit.unwrap_or(100).min(matched.len());
    let start = matched.len().saturating_sub(n);
    let tail = matched[start..].to_vec();
    Ok(json!({
        "total": items.len(),
        "matched": matched.len(),
        "filters": {
            "kind": q.kind,
            "phase": q.phase,
            "policy_revision": q.policy_revision,
            "request_id": q.request_id,
            "from_ts_unix": q.from_ts_unix,
            "to_ts_unix": q.to_ts_unix
        },
        "items": tail
    }))
}

pub fn replay_audit_items(
    items: &[Value],
    upto_event_id: Option<String>,
    upto_ts_unix: Option<u64>,
    fallback_phase: &str,
    fallback_policy_revision: u64,
    fallback_consult_mode: &str,
) -> Value {
    let mut applied = 0usize;
    let mut replay_phase: Option<String> = None;
    let mut replay_policy_revision: Option<u64> = None;
    let mut replay_consult_mode: Option<String> = None;
    let mut evidence_ids = BTreeSet::new();
    let mut event_count = 0usize;
    let mut proxy_trace_count = 0usize;
    let mut policy_update_count = 0usize;
    let mut last_event_id: Option<String> = None;
    let mut stopped_by: Option<String> = None;

    for item in items {
        if let Some(max_ts) = upto_ts_unix {
            let ts = item.get("ts_unix").and_then(|v| v.as_u64()).unwrap_or(0);
            if ts > max_ts {
                stopped_by = Some("upto_ts_unix".to_string());
                break;
            }
        }

        replay_phase = item
            .get("phase")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string())
            .or(replay_phase);
        replay_policy_revision = item
            .get("policy_revision")
            .and_then(|v| v.as_u64())
            .or(replay_policy_revision);

        if let Some(kind) = item.get("kind").and_then(|v| v.as_str()) {
            match kind {
                "consult_mode.changed" => {
                    replay_consult_mode = item
                        .get("payload")
                        .and_then(|v| v.get("mode"))
                        .and_then(|v| v.as_str())
                        .map(|v| v.to_string())
                        .or(replay_consult_mode);
                }
                "evidence.registered" => {
                    if let Some(id) = item
                        .get("payload")
                        .and_then(|v| v.get("id"))
                        .and_then(|v| v.as_str())
                    {
                        evidence_ids.insert(id.to_string());
                    }
                }
                "event.recorded" => event_count += 1,
                "proxy.trace" => proxy_trace_count += 1,
                "policy.applied" => policy_update_count += 1,
                _ => {}
            }
        }

        applied += 1;
        if let Some(ev_id) = item.get("event_id").and_then(|v| v.as_str()) {
            last_event_id = Some(ev_id.to_string());
            if upto_event_id.as_deref() == Some(ev_id) {
                stopped_by = Some("upto_event_id".to_string());
                break;
            }
        }
    }

    json!({
        "total_events": items.len(),
        "applied_events": applied,
        "stopped_by": stopped_by,
        "upto_event_id": upto_event_id,
        "upto_ts_unix": upto_ts_unix,
        "snapshot": {
            "phase": replay_phase.unwrap_or_else(|| fallback_phase.to_string()),
            "policy_revision": replay_policy_revision.unwrap_or(fallback_policy_revision),
            "consult_mode": replay_consult_mode.unwrap_or_else(|| fallback_consult_mode.to_string()),
            "evidence_total": evidence_ids.len(),
            "events_total": event_count,
            "proxy_traces_total": proxy_trace_count,
            "policy_updates_total": policy_update_count,
            "last_event_id": last_event_id
        }
    })
}

pub fn append_audit_record(
    audit_path: &Path,
    phase: &str,
    policy_revision: u64,
    kind: &str,
    payload: Value,
) -> Result<()> {
    if audit_path.as_os_str().is_empty() {
        return Ok(());
    }
    let event_id = format!("ev-{}", now_unix_nanos()?);
    let base = json!({
        "event_id": event_id,
        "ts_unix": now_unix()?,
        "kind": kind,
        "phase": phase,
        "policy_revision": policy_revision,
        "payload": payload
    });
    let mut hasher = Sha256::new();
    hasher.update(serde_json::to_string(&base)?.as_bytes());
    let digest_sha256 = hex::encode(hasher.finalize());
    let rec = json!({
        "event_id": base["event_id"],
        "ts_unix": base["ts_unix"],
        "kind": base["kind"],
        "phase": base["phase"],
        "policy_revision": base["policy_revision"],
        "payload": base["payload"],
        "digest_sha256": digest_sha256
    });
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(audit_path)?;
    f.write_all(serde_json::to_string(&rec)?.as_bytes())?;
    f.write_all(b"\n")?;
    Ok(())
}

pub fn read_audit_items(audit_path: &Path) -> Vec<Value> {
    let text = match fs::read_to_string(audit_path) {
        Ok(v) => v,
        Err(_) => String::new(),
    };
    let mut items = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            items.push(v);
        }
    }
    items
}

pub fn rotate_audit_log(audit_path: &Path, archive_dir: &Path, compress: bool) -> Result<Value> {
    if audit_path.as_os_str().is_empty() {
        bail!("audit path is required");
    }
    if archive_dir.as_os_str().is_empty() {
        bail!("archive_dir is required");
    }

    let text = fs::read_to_string(audit_path)?;
    let mut lines = Vec::new();
    let mut first_ts: Option<u64> = None;
    let mut last_ts: Option<u64> = None;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        lines.push(line.to_string());
        if let Ok(v) = serde_json::from_str::<Value>(line)
            && let Some(ts) = v.get("ts_unix").and_then(|x| x.as_u64())
        {
            if first_ts.is_none() {
                first_ts = Some(ts);
            }
            last_ts = Some(ts);
        }
    }
    if lines.is_empty() {
        bail!("audit log is empty");
    }

    fs::create_dir_all(archive_dir)?;
    let first_ts = first_ts.unwrap_or(0);
    let last_ts = last_ts.unwrap_or(first_ts);
    let basename = format!("audit_{}_{}_{}", first_ts, last_ts, now_unix_nanos()?);
    let filename = if compress {
        format!("{basename}.jsonl.gz")
    } else {
        format!("{basename}.jsonl")
    };
    let archive_path = archive_dir.join(filename);
    let signature_path = archive_path.with_file_name(format!(
        "{}.sha256",
        archive_path
            .file_name()
            .and_then(|x| x.to_str())
            .ok_or_else(|| anyhow!("archive filename is invalid utf-8"))?
    ));

    let payload = format!("{}\n", lines.join("\n"));
    let payload_bytes = payload.as_bytes();
    if compress {
        let file = fs::File::create(&archive_path)?;
        let mut enc = GzEncoder::new(file, Compression::default());
        enc.write_all(payload_bytes)?;
        let _ = enc.finish()?;
    } else {
        fs::write(&archive_path, payload_bytes)?;
    }

    let digest_sha256 = sha256_hex(payload_bytes);
    let archive_file_name = archive_path
        .file_name()
        .and_then(|x| x.to_str())
        .ok_or_else(|| anyhow!("archive filename is invalid utf-8"))?;
    fs::write(
        &signature_path,
        format!("{digest_sha256}  {archive_file_name}\n"),
    )?;

    fs::write(audit_path, "")?;

    Ok(json!({
        "rotated": true,
        "compressed": compress,
        "archived_lines": lines.len(),
        "archive_path": archive_path,
        "signature_path": signature_path,
        "digest_sha256": digest_sha256,
        "first_ts_unix": first_ts,
        "last_ts_unix": last_ts
    }))
}

pub fn verify_audit_archive(archive_path: &Path, signature_path: Option<&Path>) -> Result<Value> {
    if archive_path.as_os_str().is_empty() {
        bail!("archive_path is required");
    }
    let signature_path = match signature_path {
        Some(v) => v.to_path_buf(),
        None => archive_path.with_file_name(format!(
            "{}.sha256",
            archive_path
                .file_name()
                .and_then(|x| x.to_str())
                .ok_or_else(|| anyhow!("archive filename is invalid utf-8"))?
        )),
    };

    let payload = read_archive_payload(archive_path)?;
    let digest_sha256 = sha256_hex(&payload);
    let signature = fs::read_to_string(&signature_path)?;
    let expected = signature
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow!("invalid signature file format"))?
        .to_ascii_lowercase();
    let pass = expected == digest_sha256.to_ascii_lowercase();
    let line_count = payload
        .split(|x| *x == b'\n')
        .filter(|x| !x.is_empty())
        .count();

    Ok(json!({
        "pass": pass,
        "archive_path": archive_path,
        "signature_path": signature_path,
        "expected_digest_sha256": expected,
        "actual_digest_sha256": digest_sha256,
        "line_count": line_count
    }))
}

pub fn prune_audit_archives(archive_dir: &Path, keep_last: usize) -> Result<Value> {
    if archive_dir.as_os_str().is_empty() {
        bail!("archive_dir is required");
    }
    if keep_last == 0 {
        bail!("keep_last must be > 0");
    }
    if !archive_dir.exists() {
        return Ok(json!({
            "total_archives": 0,
            "kept": 0,
            "removed": 0,
            "removed_items": []
        }));
    }

    let mut archives = Vec::new();
    for entry in fs::read_dir(archive_dir)? {
        let entry = entry?;
        let sidecar_path = entry.path();
        if !sidecar_path.is_file() {
            continue;
        }
        let Some(name) = sidecar_path
            .file_name()
            .and_then(|x| x.to_str())
            .map(|x| x.to_string())
        else {
            continue;
        };
        if !name.ends_with(".sha256") {
            continue;
        }
        let archive_name = &name[..name.len() - ".sha256".len()];
        if !(archive_name.ends_with(".jsonl") || archive_name.ends_with(".jsonl.gz")) {
            continue;
        }
        let archive_path = archive_dir.join(archive_name);
        if !archive_path.exists() || !archive_path.is_file() {
            continue;
        }
        let modified = fs::metadata(&archive_path)?
            .modified()
            .unwrap_or(UNIX_EPOCH);
        archives.push((
            archive_path,
            sidecar_path,
            modified,
            archive_name.to_string(),
        ));
    }

    archives.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| b.3.cmp(&a.3)));
    let total = archives.len();
    let mut removed_items = Vec::new();
    for (idx, (archive_path, sidecar_path, _, _)) in archives.into_iter().enumerate() {
        if idx < keep_last {
            continue;
        }
        fs::remove_file(&archive_path)?;
        fs::remove_file(&sidecar_path)?;
        removed_items.push(json!({
            "archive_path": archive_path,
            "signature_path": sidecar_path
        }));
    }

    Ok(json!({
        "total_archives": total,
        "kept": keep_last.min(total),
        "removed": removed_items.len(),
        "removed_items": removed_items
    }))
}

fn read_archive_payload(path: &Path) -> Result<Vec<u8>> {
    let is_gzip = path
        .extension()
        .and_then(|x| x.to_str())
        .map(|x| x.eq_ignore_ascii_case("gz"))
        .unwrap_or(false);
    if is_gzip {
        let file = fs::File::open(path)?;
        let mut dec = GzDecoder::new(file);
        let mut out = Vec::new();
        dec.read_to_end(&mut out)?;
        return Ok(out);
    }
    Ok(fs::read(path)?)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn audit_item_has_request_id(item: &Value, request_id: &str) -> bool {
    if item.get("request_id").and_then(|v| v.as_str()) == Some(request_id) {
        return true;
    }
    if item
        .get("payload")
        .and_then(|v| v.get("request_id"))
        .and_then(|v| v.as_str())
        == Some(request_id)
    {
        return true;
    }
    false
}

fn now_unix() -> Result<u64> {
    let dur = SystemTime::now().duration_since(UNIX_EPOCH)?;
    Ok(dur.as_secs())
}

fn now_unix_nanos() -> Result<u128> {
    let dur = SystemTime::now().duration_since(UNIX_EPOCH)?;
    Ok(dur.as_nanos())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn query_filters_by_kind_and_request() {
        let items = vec![
            json!({"kind":"a","request_id":"r1","ts_unix":1}),
            json!({"kind":"b","request_id":"r2","ts_unix":2}),
            json!({"kind":"a","payload":{"request_id":"r2"},"ts_unix":3}),
        ];
        let out = query_audit_items(
            &items,
            &AuditQuery {
                kind: Some("a".to_string()),
                request_id: Some("r2".to_string()),
                ..AuditQuery::default()
            },
        )
        .expect("query");
        assert_eq!(out["matched"].as_u64(), Some(1));
    }

    #[test]
    fn replay_builds_snapshot() {
        let items = vec![
            json!({"kind":"consult_mode.changed","payload":{"mode":"yolo"},"phase":"C-0","policy_revision":1,"ts_unix":1,"event_id":"e1"}),
            json!({"kind":"evidence.registered","payload":{"id":"x"},"phase":"GA-1","policy_revision":2,"ts_unix":2,"event_id":"e2"}),
        ];
        let out = replay_audit_items(&items, None, None, "C-0", 1, "user_tracking");
        assert_eq!(out["snapshot"]["consult_mode"].as_str(), Some("yolo"));
        assert_eq!(out["snapshot"]["evidence_total"].as_u64(), Some(1));
        assert_eq!(out["snapshot"]["policy_revision"].as_u64(), Some(2));
    }

    #[test]
    fn append_and_read_roundtrip() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "cabal_audit_core_{}.jsonl",
            now_unix_nanos().expect("ts")
        ));
        append_audit_record(&path, "C-0", 1, "probe", json!({"x": 1})).expect("append");
        let items = read_audit_items(&path);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["kind"].as_str(), Some("probe"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn rotate_and_verify_roundtrip() {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "cabal_audit_rotate_{}",
            now_unix_nanos().expect("ts")
        ));
        fs::create_dir_all(&dir).expect("mkdir");
        let audit_path = dir.join("audit.jsonl");
        let archive_dir = dir.join("archive");

        append_audit_record(&audit_path, "C-0", 1, "probe.a", json!({"x": 1})).expect("append1");
        append_audit_record(&audit_path, "C-0", 1, "probe.b", json!({"x": 2})).expect("append2");

        let rotated = rotate_audit_log(&audit_path, &archive_dir, true).expect("rotate");
        assert_eq!(rotated["rotated"].as_bool(), Some(true));
        assert_eq!(rotated["compressed"].as_bool(), Some(true));
        assert_eq!(rotated["archived_lines"].as_u64(), Some(2));

        let archive_path = Path::new(rotated["archive_path"].as_str().expect("archive_path"));
        let signature_path = Path::new(rotated["signature_path"].as_str().expect("signature_path"));
        let verify = verify_audit_archive(archive_path, Some(signature_path)).expect("verify");
        assert_eq!(verify["pass"].as_bool(), Some(true));
        assert_eq!(verify["line_count"].as_u64(), Some(2));

        let active = fs::read_to_string(&audit_path).expect("active read");
        assert!(active.trim().is_empty());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn verify_detects_signature_mismatch() {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "cabal_audit_verify_fail_{}",
            now_unix_nanos().expect("ts")
        ));
        fs::create_dir_all(&dir).expect("mkdir");
        let audit_path = dir.join("audit.jsonl");
        let archive_dir = dir.join("archive");

        append_audit_record(&audit_path, "C-0", 1, "probe.a", json!({"x": 1})).expect("append");
        let rotated = rotate_audit_log(&audit_path, &archive_dir, false).expect("rotate");
        let archive_path = Path::new(rotated["archive_path"].as_str().expect("archive_path"));
        let signature_path =
            Path::new(rotated["signature_path"].as_str().expect("signature_path")).to_path_buf();

        fs::write(&signature_path, "deadbeef  broken\n").expect("tamper sig");
        let verify = verify_audit_archive(archive_path, Some(&signature_path)).expect("verify");
        assert_eq!(verify["pass"].as_bool(), Some(false));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn prune_archives_keeps_latest_n() {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "cabal_audit_prune_keep_latest_{}",
            now_unix_nanos().expect("ts")
        ));
        fs::create_dir_all(&dir).expect("mkdir");
        let audit_path = dir.join("audit.jsonl");
        let archive_dir = dir.join("archive");

        for idx in 0..3 {
            append_audit_record(&audit_path, "C-0", 1, "probe", json!({"idx": idx}))
                .expect("append");
            rotate_audit_log(&audit_path, &archive_dir, false).expect("rotate");
            std::thread::sleep(Duration::from_millis(1));
        }

        let out = prune_audit_archives(&archive_dir, 1).expect("prune");
        assert_eq!(out["total_archives"].as_u64(), Some(3));
        assert_eq!(out["kept"].as_u64(), Some(1));
        assert_eq!(out["removed"].as_u64(), Some(2));

        let files = fs::read_dir(&archive_dir).expect("read_dir").count();
        assert_eq!(files, 2);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn prune_archives_rejects_zero_keep_last() {
        let err = prune_audit_archives(Path::new("."), 0).expect_err("must fail");
        assert!(err.to_string().contains("keep_last must be > 0"));
    }
}

use crate::core::events::truncate_text;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::fs;
use std::io::Read;
use std::net::IpAddr;
use std::path::Component;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use url::Url;
use wait_timeout::ChildExt;

const SHELL_FORBIDDEN_FRAGMENTS: &[&str] = &[
    "rm -rf",
    "rmdir /s /q",
    "del /f",
    "remove-item -recurse -force",
    "git reset --hard",
    "git clean -fd",
    "format ",
    "mkfs",
    "dd if=",
    "shutdown",
    "reboot",
    "poweroff",
    "halt",
    "reg delete",
    "sc delete",
];

const NETWORK_FORBIDDEN_HOSTS: &[&str] = &[
    "localhost",
    "metadata.google.internal",
    "metadata.aws.internal",
    "metadata.azure.internal",
];
const NETWORK_CONNECT_TIMEOUT_SEC: u64 = 5;
const NETWORK_IO_TIMEOUT_SEC: u64 = 15;
const NETWORK_MAX_BODY_BYTES: usize = 8192;
const SHELL_MAX_COMMAND_LEN: usize = 1024;
const SHELL_STDIO_MAX_BYTES: usize = 4000;
const SHELL_EXEC_TIMEOUT_SEC: u64 = 15;
const FS_READ_MAX_BYTES: usize = 131072;
const FS_WRITE_MAX_BYTES: usize = 1048576;
const FS_LIST_DIR_MAX_ENTRIES: usize = 1000;

pub fn resolve_safe_repo_path(repo_root: &Path, target: &str) -> Result<PathBuf> {
    let target_path = Path::new(target);
    if target_path.is_absolute() {
        bail!("absolute paths are forbidden");
    }
    for comp in target_path.components() {
        match comp {
            Component::Normal(_) => {}
            _ => bail!("path traversal or invalid path component is forbidden"),
        }
    }
    Ok(repo_root.join(target_path))
}

pub fn exec_fs(repo_root: &Path, operation: &str, target: &str, payload: Value) -> Result<Value> {
    let path = resolve_safe_repo_path(repo_root, target)?;
    match operation {
        "read_text" => {
            let file = fs::File::open(&path)
                .with_context(|| format!("failed to read file: {}", path.display()))?;
            let (text, truncated, read_bytes) = read_limited_utf8_body(file, FS_READ_MAX_BYTES)?;
            Ok(json!({
                "text": text,
                "truncated": truncated,
                "read_bytes": read_bytes
            }))
        }
        "write_text" => {
            let text = payload
                .get("text")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("payload.text is required for write_text"))?;
            if text.len() > FS_WRITE_MAX_BYTES {
                bail!("payload.text is too large");
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&path, text)
                .with_context(|| format!("failed to write file: {}", path.display()))?;
            Ok(json!({"written_bytes": text.len()}))
        }
        "list_dir" => {
            let mut entries = Vec::new();
            let mut total_entries = 0usize;
            for entry in fs::read_dir(&path)
                .with_context(|| format!("failed to list dir: {}", path.display()))?
            {
                let entry = entry?;
                total_entries = total_entries.saturating_add(1);
                if entries.len() < FS_LIST_DIR_MAX_ENTRIES {
                    entries.push(entry.file_name().to_string_lossy().to_string());
                }
            }
            entries.sort();
            Ok(json!({
                "entries": entries,
                "truncated": total_entries > FS_LIST_DIR_MAX_ENTRIES,
                "total_entries": total_entries
            }))
        }
        _ => bail!("unsupported fs operation: {operation}"),
    }
}

pub fn exec_shell(operation: &str, target: &str) -> Result<Value> {
    exec_shell_with_timeout(operation, target, shell_exec_timeout())
}

fn exec_shell_with_timeout(operation: &str, target: &str, timeout: Duration) -> Result<Value> {
    if operation != "run" {
        bail!("unsupported shell operation: {operation}");
    }
    ensure_shell_command_allowed(target)?;

    #[cfg(target_os = "windows")]
    let mut child = Command::new("cmd")
        .args(["/C", target])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to run shell command: {target}"))?;
    #[cfg(not(target_os = "windows"))]
    let mut child = Command::new("sh")
        .args(["-lc", target])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to run shell command: {target}"))?;

    let status = match child
        .wait_timeout(timeout)
        .context("failed to wait shell command with timeout")?
    {
        Some(status) => status,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            bail!("shell command timed out");
        }
    };

    let mut stdout_raw = String::new();
    let mut stderr_raw = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.read_to_string(&mut stdout_raw);
    }
    if let Some(mut stderr) = child.stderr.take() {
        let _ = stderr.read_to_string(&mut stderr_raw);
    }

    let (stdout, stdout_truncated, stdout_bytes) =
        bounded_text_output(&stdout_raw, SHELL_STDIO_MAX_BYTES);
    let (stderr, stderr_truncated, stderr_bytes) =
        bounded_text_output(&stderr_raw, SHELL_STDIO_MAX_BYTES);
    Ok(json!({
        "status": status.code(),
        "stdout": stdout,
        "stderr": stderr,
        "stdout_truncated": stdout_truncated,
        "stderr_truncated": stderr_truncated,
        "stdout_bytes": stdout_bytes,
        "stderr_bytes": stderr_bytes
    }))
}

pub fn ensure_shell_command_allowed(target: &str) -> Result<()> {
    let normalized = target.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        bail!("shell target command is required");
    }
    if normalized.len() > SHELL_MAX_COMMAND_LEN {
        bail!("shell target command is too long");
    }
    for fragment in SHELL_FORBIDDEN_FRAGMENTS {
        if normalized.contains(fragment) {
            bail!("shell command blocked by policy: contains forbidden fragment `{fragment}`");
        }
    }
    Ok(())
}

pub fn exec_network(operation: &str, target: &str) -> Result<Value> {
    if operation != "http_get" {
        bail!("unsupported network operation: {operation}");
    }
    ensure_network_target_allowed(target)?;
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(NETWORK_CONNECT_TIMEOUT_SEC))
        .timeout_read(Duration::from_secs(NETWORK_IO_TIMEOUT_SEC))
        .timeout_write(Duration::from_secs(NETWORK_IO_TIMEOUT_SEC))
        .build();
    let response = agent
        .get(target)
        .call()
        .with_context(|| format!("http_get failed: {target}"))?;
    let status = response.status();
    let (body, truncated, body_bytes) =
        read_limited_utf8_body(response.into_reader(), NETWORK_MAX_BODY_BYTES)?;
    Ok(json!({
        "status": status,
        "body": body,
        "truncated": truncated,
        "body_bytes": body_bytes
    }))
}

pub fn ensure_network_target_allowed(target: &str) -> Result<()> {
    let target = target.trim();
    if target.is_empty() {
        bail!("network target url is required");
    }

    let parsed = Url::parse(target).context("invalid network target url")?;
    let scheme = parsed.scheme().to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        bail!("network target blocked by policy: unsupported scheme `{scheme}`");
    }

    let Some(host) = parsed.host_str() else {
        bail!("invalid network target url");
    };
    let host_norm = host.to_ascii_lowercase();

    if NETWORK_FORBIDDEN_HOSTS.iter().any(|x| *x == host_norm) || host_norm.ends_with(".localhost")
    {
        bail!("network target blocked by policy: forbidden host `{host_norm}`");
    }

    if let Ok(ip) = host_norm.parse::<IpAddr>() {
        if is_forbidden_ip(ip) {
            bail!("network target blocked by policy: forbidden host ip `{ip}`");
        }
    }

    Ok(())
}

fn is_forbidden_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified()
        }
        IpAddr::V6(v6) => {
            let seg0 = v6.segments()[0];
            let is_unique_local = (seg0 & 0xfe00) == 0xfc00;
            let is_link_local = (seg0 & 0xffc0) == 0xfe80;
            v6.is_loopback() || v6.is_unspecified() || is_unique_local || is_link_local
        }
    }
}

fn read_limited_utf8_body(
    mut reader: impl Read,
    max_bytes: usize,
) -> Result<(String, bool, usize)> {
    let mut buf = Vec::with_capacity(max_bytes.saturating_add(1));
    let mut limited = reader.by_ref().take((max_bytes.saturating_add(1)) as u64);
    limited
        .read_to_end(&mut buf)
        .context("failed to read response body")?;
    let truncated = buf.len() > max_bytes;
    if truncated {
        buf.truncate(max_bytes);
    }
    let body = String::from_utf8_lossy(&buf).to_string();
    Ok((body, truncated, buf.len()))
}

fn bounded_text_output(raw: &str, max_bytes: usize) -> (String, bool, usize) {
    let truncated = raw.len() > max_bytes;
    let text = truncate_text(raw, max_bytes);
    let bytes = text.len();
    (text, truncated, bytes)
}

fn shell_exec_timeout() -> Duration {
    let default = Duration::from_secs(SHELL_EXEC_TIMEOUT_SEC);
    let Some(value) = std::env::var_os("CABAL_PROXY_SHELL_TIMEOUT_MS") else {
        return default;
    };
    let Some(raw) = value.to_str() else {
        return default;
    };
    let Ok(ms) = raw.trim().parse::<u64>() else {
        return default;
    };
    if ms == 0 {
        return default;
    }
    Duration::from_millis(ms.min(120_000))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn resolve_safe_repo_path_rejects_parent_component() {
        let root = Path::new(".");
        let err = resolve_safe_repo_path(root, "../secret").expect_err("must reject traversal");
        assert!(err.to_string().contains("path traversal"));
    }

    #[test]
    fn exec_fs_write_and_read_roundtrip() {
        let mut dir = std::env::temp_dir();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        dir.push(format!("cabal_proxy_exec_test_{nonce}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        exec_fs(&dir, "write_text", "a/b/c.txt", json!({"text": "hello"})).expect("write");
        let out = exec_fs(&dir, "read_text", "a/b/c.txt", json!({})).expect("read");
        assert_eq!(out["text"], Value::String("hello".to_string()));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ensure_shell_command_allowed_blocks_dangerous_pattern() {
        let err =
            ensure_shell_command_allowed("git reset --hard HEAD").expect_err("must block command");
        assert!(err.to_string().contains("shell command blocked by policy"));
    }

    #[test]
    fn ensure_shell_command_allowed_accepts_safe_command() {
        ensure_shell_command_allowed("cargo test -q").expect("must allow command");
    }

    #[test]
    fn ensure_shell_command_allowed_blocks_overlong_command() {
        let long_cmd = "a".repeat(SHELL_MAX_COMMAND_LEN + 1);
        let err = ensure_shell_command_allowed(&long_cmd).expect_err("must block long command");
        assert!(err.to_string().contains("shell target command is too long"));
    }

    #[test]
    fn exec_shell_times_out() {
        #[cfg(target_os = "windows")]
        let long_cmd = "ping 127.0.0.1 -n 6 > nul";
        #[cfg(not(target_os = "windows"))]
        let long_cmd = "sleep 2";

        let err = exec_shell_with_timeout("run", long_cmd, Duration::from_millis(20))
            .expect_err("must timeout");
        assert!(err.to_string().contains("shell command timed out"));
    }

    #[test]
    fn shell_exec_timeout_uses_env_override_when_valid() {
        // SAFETY: unit test scope only; value is restored in-process.
        unsafe { std::env::set_var("CABAL_PROXY_SHELL_TIMEOUT_MS", "1234") };
        assert_eq!(shell_exec_timeout(), Duration::from_millis(1234));
        // SAFETY: cleanup test env override.
        unsafe { std::env::remove_var("CABAL_PROXY_SHELL_TIMEOUT_MS") };
    }

    #[test]
    fn ensure_network_target_allowed_blocks_localhost() {
        let err =
            ensure_network_target_allowed("http://localhost:8080").expect_err("must block host");
        assert!(err.to_string().contains("network target blocked by policy"));
    }

    #[test]
    fn ensure_network_target_allowed_blocks_private_ip() {
        let err =
            ensure_network_target_allowed("http://127.0.0.1:8080").expect_err("must block ip");
        assert!(err.to_string().contains("network target blocked by policy"));
    }

    #[test]
    fn ensure_network_target_allowed_accepts_public_https() {
        ensure_network_target_allowed("https://example.com").expect("must allow target");
    }

    #[test]
    fn read_limited_utf8_body_truncates_large_payload() {
        let data = "a".repeat(16);
        let (body, truncated, body_bytes) =
            read_limited_utf8_body(Cursor::new(data.into_bytes()), 8).expect("read");
        assert_eq!(body, "aaaaaaaa");
        assert!(truncated);
        assert_eq!(body_bytes, 8);
    }

    #[test]
    fn read_limited_utf8_body_keeps_small_payload() {
        let (body, truncated, body_bytes) =
            read_limited_utf8_body(Cursor::new(b"hello".to_vec()), 8).expect("read");
        assert_eq!(body, "hello");
        assert!(!truncated);
        assert_eq!(body_bytes, 5);
    }

    #[test]
    fn exec_fs_read_text_is_bounded() {
        let mut dir = std::env::temp_dir();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        dir.push(format!("cabal_proxy_exec_fs_bounded_{nonce}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        let data = "b".repeat(FS_READ_MAX_BYTES + 100);
        fs::write(dir.join("big.txt"), data).expect("write big file");

        let out = exec_fs(&dir, "read_text", "big.txt", json!({})).expect("read text");
        assert_eq!(out["truncated"].as_bool(), Some(true));
        assert_eq!(out["read_bytes"].as_u64(), Some(FS_READ_MAX_BYTES as u64));
        assert_eq!(
            out["text"].as_str().map(|s| s.len()),
            Some(FS_READ_MAX_BYTES)
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn exec_fs_write_text_rejects_oversized_payload() {
        let mut dir = std::env::temp_dir();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        dir.push(format!("cabal_proxy_exec_fs_write_limit_{nonce}"));
        fs::create_dir_all(&dir).expect("create temp dir");

        let oversized = "x".repeat(FS_WRITE_MAX_BYTES + 1);
        let err = exec_fs(&dir, "write_text", "big.txt", json!({"text": oversized}))
            .expect_err("must reject oversized payload");
        assert!(err.to_string().contains("payload.text is too large"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn exec_fs_list_dir_is_bounded() {
        let mut dir = std::env::temp_dir();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        dir.push(format!("cabal_proxy_exec_fs_list_limit_{nonce}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        fs::create_dir_all(dir.join("list")).expect("create list subdir");

        for idx in 0..(FS_LIST_DIR_MAX_ENTRIES + 10) {
            fs::write(dir.join("list").join(format!("f_{idx:04}.txt")), "x").expect("write file");
        }

        let out = exec_fs(&dir, "list_dir", "list", json!({})).expect("list dir");
        assert_eq!(out["truncated"].as_bool(), Some(true));
        assert_eq!(
            out["total_entries"].as_u64(),
            Some((FS_LIST_DIR_MAX_ENTRIES + 10) as u64)
        );
        assert_eq!(
            out["entries"].as_array().map(|x| x.len()),
            Some(FS_LIST_DIR_MAX_ENTRIES)
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn bounded_text_output_marks_truncation() {
        let (out, truncated, bytes) = bounded_text_output("abcdef", 4);
        assert_eq!(out, "abcd");
        assert!(truncated);
        assert_eq!(bytes, 4);
    }
}

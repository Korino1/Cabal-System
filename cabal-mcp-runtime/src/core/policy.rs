use anyhow::{Context, Result, anyhow, bail};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::Sha256;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicySigningKey {
    pub key_id: String,
    #[serde(default = "default_policy_signing_algorithm")]
    pub algorithm: String,
    pub key_env: String,
    #[serde(default)]
    pub not_before_unix: Option<u64>,
    #[serde(default)]
    pub not_after_unix: Option<u64>,
    #[serde(default)]
    pub revoked_at_unix: Option<u64>,
}

pub fn default_policy_signing_algorithm() -> String {
    "hmac_sha256".to_string()
}

pub fn default_policy_signing_keys(active_policy_key_id: &str) -> Vec<PolicySigningKey> {
    vec![PolicySigningKey {
        key_id: active_policy_key_id.to_string(),
        algorithm: default_policy_signing_algorithm(),
        key_env: "CABAL_POLICY_HMAC_KEY".to_string(),
        not_before_unix: None,
        not_after_unix: None,
        revoked_at_unix: None,
    }]
}

pub fn build_policy_signing_message(
    version: &str,
    revision: u64,
    rules: &[String],
    forbidden_tokens: &[String],
    key_id: &str,
    nonce: &str,
) -> Result<String> {
    Ok(serde_json::to_string(&json!({
        "version": version,
        "revision": revision,
        "rules": rules,
        "forbidden_tokens": forbidden_tokens,
        "key_id": key_id,
        "nonce": nonce
    }))?)
}

pub fn resolve_policy_key_id(
    key_id: Option<&str>,
    active_policy_key_id: &str,
    policy_signing_keys: &[PolicySigningKey],
) -> Result<String> {
    if let Some(k) = key_id {
        return Ok(k.to_string());
    }
    if !active_policy_key_id.trim().is_empty() {
        return Ok(active_policy_key_id.to_string());
    }
    if policy_signing_keys.len() == 1 {
        return Ok(policy_signing_keys[0].key_id.clone());
    }
    bail!("key_id is required when multiple signing keys exist");
}

pub fn verify_policy_signature(
    policy_signing_keys: &[PolicySigningKey],
    active_policy_key_id: &str,
    used_policy_nonces: &[String],
    version: &str,
    revision: u64,
    rules: &[String],
    forbidden_tokens: &[String],
    key_id: Option<&str>,
    nonce: &str,
    signature_hex: &str,
    now_unix: u64,
) -> Result<String> {
    let key_id = resolve_policy_key_id(key_id, active_policy_key_id, policy_signing_keys)?;
    let nonce_key = format!("{key_id}:{nonce}");
    if used_policy_nonces
        .iter()
        .any(|x| x == &nonce_key || x == nonce)
    {
        bail!("nonce replay detected");
    }
    let key_cfg = policy_signing_keys
        .iter()
        .find(|x| x.key_id == key_id)
        .ok_or_else(|| anyhow!("unknown policy signing key_id: {key_id}"))?;
    if key_cfg.algorithm != "hmac_sha256" {
        bail!("unsupported signing algorithm: {}", key_cfg.algorithm);
    }
    if key_cfg.revoked_at_unix.is_some() {
        bail!("signing key is revoked: {key_id}");
    }
    if let Some(not_before) = key_cfg.not_before_unix
        && now_unix < not_before
    {
        bail!("signing key is not active yet: {key_id}");
    }
    if let Some(not_after) = key_cfg.not_after_unix
        && now_unix > not_after
    {
        bail!("signing key expired: {key_id}");
    }
    let key = std::env::var(&key_cfg.key_env)
        .with_context(|| format!("{} is required for signed policy mode", key_cfg.key_env))?;

    let message =
        build_policy_signing_message(version, revision, rules, forbidden_tokens, &key_id, nonce)?;
    let mut mac: Hmac<Sha256> = Hmac::new_from_slice(key.as_bytes()).context("invalid hmac key")?;
    mac.update(message.as_bytes());
    let expected = mac.finalize().into_bytes();
    let got = hex::decode(signature_hex).context("signature must be hex")?;
    if got.len() != expected.len() {
        bail!("invalid signature length");
    }
    // Constant-time compare by crypto crate.
    let mut mac_verify: Hmac<Sha256> =
        Hmac::new_from_slice(key.as_bytes()).context("invalid hmac key")?;
    mac_verify.update(message.as_bytes());
    mac_verify
        .verify_slice(&got)
        .map_err(|_| anyhow!("signature verification failed"))?;
    Ok(key_id)
}

pub fn register_policy_nonce(
    used_policy_nonces: &mut Vec<String>,
    key_id: &str,
    nonce: &str,
) -> Result<()> {
    let nonce_key = format!("{key_id}:{nonce}");
    if used_policy_nonces
        .iter()
        .any(|x| x == &nonce_key || x == nonce)
    {
        bail!("nonce replay detected");
    }
    used_policy_nonces.push(nonce_key);
    const MAX_NONCES: usize = 2048;
    if used_policy_nonces.len() > MAX_NONCES {
        let drop_n = used_policy_nonces.len() - MAX_NONCES;
        used_policy_nonces.drain(0..drop_n);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_policy_key_prefers_explicit_value() {
        let keys = vec![PolicySigningKey {
            key_id: "default".to_string(),
            algorithm: default_policy_signing_algorithm(),
            key_env: "CABAL_POLICY_HMAC_KEY".to_string(),
            not_before_unix: None,
            not_after_unix: None,
            revoked_at_unix: None,
        }];
        let got =
            resolve_policy_key_id(Some("custom"), "default", &keys).expect("resolve explicit");
        assert_eq!(got, "custom");
    }

    #[test]
    fn register_policy_nonce_rejects_replay() {
        let mut nonces = Vec::new();
        register_policy_nonce(&mut nonces, "default", "n-1").expect("first nonce");
        let err = register_policy_nonce(&mut nonces, "default", "n-1")
            .expect_err("replay must be rejected");
        assert!(err.to_string().contains("nonce replay"));
    }
}

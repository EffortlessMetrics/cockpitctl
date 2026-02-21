//! Policy-signing key-loading adapter extracted from `cockpitctl-io`.

use anyhow::{Context, Result};
use cockpitctl_types::PolicySigningConfig;
use std::fs;

/// Load policy signing key bytes from config.
///
/// Resolution order:
/// 1. `key_path` (file bytes)
/// 2. `key_env` (environment variable UTF-8 bytes)
///
/// If neither is configured, returns `Ok(None)`.
pub fn load_policy_signing_key(cfg: &PolicySigningConfig) -> Result<Option<Vec<u8>>> {
    if let Some(path) = cfg.key_path.as_deref() {
        if path.trim().is_empty() {
            anyhow::bail!("policy signing key_path is empty");
        }
        let bytes = fs::read(path).with_context(|| format!("read policy signing key {}", path))?;
        return Ok(Some(normalize_signing_key_bytes(bytes)?));
    }

    if let Some(env_name) = cfg.key_env.as_deref() {
        if env_name.trim().is_empty() {
            anyhow::bail!("policy signing key_env is empty");
        }
        let value = std::env::var(env_name)
            .with_context(|| format!("read policy signing key from env {}", env_name))?;
        return Ok(Some(normalize_signing_key_bytes(value.into_bytes())?));
    }

    Ok(None)
}

fn normalize_signing_key_bytes(mut bytes: Vec<u8>) -> Result<Vec<u8>> {
    while matches!(bytes.last(), Some(b'\n' | b'\r')) {
        bytes.pop();
    }
    if bytes.is_empty() {
        anyhow::bail!("policy signing key is empty");
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Mutex;
    use tempfile::tempdir;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn load_policy_signing_key_reads_path_bytes() {
        let temp = tempdir().expect("tempdir");
        let key_path = temp.path().join("policy.key");
        fs::write(&key_path, b"file-secret\n").expect("write key");

        let cfg = PolicySigningConfig {
            enabled: true,
            algorithm: cockpitctl_types::PolicySignatureAlgorithm::HmacSha256,
            key_path: Some(key_path.to_string_lossy().to_string()),
            key_env: None,
            key_id: None,
        };
        let key = load_policy_signing_key(&cfg)
            .expect("load key")
            .expect("key present");
        assert_eq!(key, b"file-secret");
    }

    #[test]
    fn load_policy_signing_key_reads_env_when_path_unset() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        unsafe {
            std::env::set_var("COCKPITCTL_POLICY_KEY_TEST", "env-secret\r\n");
        }
        let cfg = PolicySigningConfig {
            enabled: true,
            algorithm: cockpitctl_types::PolicySignatureAlgorithm::HmacSha256,
            key_path: None,
            key_env: Some("COCKPITCTL_POLICY_KEY_TEST".to_string()),
            key_id: None,
        };
        let key = load_policy_signing_key(&cfg)
            .expect("load key")
            .expect("key present");
        unsafe {
            std::env::remove_var("COCKPITCTL_POLICY_KEY_TEST");
        }
        assert_eq!(key, b"env-secret");
    }

    #[test]
    fn load_policy_signing_key_returns_none_when_unconfigured() {
        let cfg = PolicySigningConfig::default();
        let key = load_policy_signing_key(&cfg).expect("load key");
        assert!(key.is_none());
    }

    #[test]
    fn load_policy_signing_key_rejects_empty_key_material() {
        let temp = tempdir().expect("tempdir");
        let key_path = temp.path().join("policy.key");
        fs::write(&key_path, b"\n").expect("write key");

        let cfg = PolicySigningConfig {
            enabled: true,
            algorithm: cockpitctl_types::PolicySignatureAlgorithm::HmacSha256,
            key_path: Some(key_path.to_string_lossy().to_string()),
            key_env: None,
            key_id: None,
        };

        let err = load_policy_signing_key(&cfg).expect_err("expected empty key error");
        assert!(format!("{:#}", err).contains("policy signing key is empty"));
    }
}

//! Policy-signing key-loading adapter extracted from `cockpitctl-io`.
//!
//! Resolves a signing key from either a file path or an environment
//! variable, normalising the raw bytes for use by the signing domain.

#![warn(missing_docs)]

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
        #[expect(
            unsafe_code,
            reason = "Environment variable access is serialized or isolated at this boundary."
        )]
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
        #[expect(
            unsafe_code,
            reason = "Environment variable access is serialized or isolated at this boundary."
        )]
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

    #[test]
    fn load_key_path_takes_precedence_over_env() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let temp = tempdir().expect("tempdir");
        let key_path = temp.path().join("policy.key");
        fs::write(&key_path, b"from-file").expect("write key");

        #[expect(
            unsafe_code,
            reason = "Environment variable access is serialized or isolated at this boundary."
        )]
        unsafe {
            std::env::set_var("COCKPITCTL_POLICY_KEY_PREC_TEST", "from-env");
        }
        let cfg = PolicySigningConfig {
            enabled: true,
            algorithm: cockpitctl_types::PolicySignatureAlgorithm::HmacSha256,
            key_path: Some(key_path.to_string_lossy().to_string()),
            key_env: Some("COCKPITCTL_POLICY_KEY_PREC_TEST".to_string()),
            key_id: None,
        };
        let key = load_policy_signing_key(&cfg)
            .expect("load key")
            .expect("key present");
        #[expect(
            unsafe_code,
            reason = "Environment variable access is serialized or isolated at this boundary."
        )]
        unsafe {
            std::env::remove_var("COCKPITCTL_POLICY_KEY_PREC_TEST");
        }
        assert_eq!(key, b"from-file");
    }

    #[test]
    fn load_key_rejects_empty_key_path_string() {
        let cfg = PolicySigningConfig {
            enabled: true,
            algorithm: cockpitctl_types::PolicySignatureAlgorithm::HmacSha256,
            key_path: Some("   ".to_string()),
            key_env: None,
            key_id: None,
        };
        let err = load_policy_signing_key(&cfg).expect_err("expected error");
        assert!(
            err.to_string().contains("key_path is empty"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn load_key_rejects_empty_key_env_string() {
        let cfg = PolicySigningConfig {
            enabled: true,
            algorithm: cockpitctl_types::PolicySignatureAlgorithm::HmacSha256,
            key_path: None,
            key_env: Some("  ".to_string()),
            key_id: None,
        };
        let err = load_policy_signing_key(&cfg).expect_err("expected error");
        assert!(
            err.to_string().contains("key_env is empty"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn load_key_errors_on_missing_file() {
        let cfg = PolicySigningConfig {
            enabled: true,
            algorithm: cockpitctl_types::PolicySignatureAlgorithm::HmacSha256,
            key_path: Some("/nonexistent/path/policy.key".to_string()),
            key_env: None,
            key_id: None,
        };
        let err = load_policy_signing_key(&cfg).expect_err("expected error");
        assert!(
            format!("{:#}", err).contains("read policy signing key"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn load_key_errors_on_missing_env_var() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let cfg = PolicySigningConfig {
            enabled: true,
            algorithm: cockpitctl_types::PolicySignatureAlgorithm::HmacSha256,
            key_path: None,
            key_env: Some("COCKPITCTL_POLICY_KEY_NONEXISTENT_999".to_string()),
            key_id: None,
        };
        let err = load_policy_signing_key(&cfg).expect_err("expected error");
        assert!(
            format!("{:#}", err).contains("read policy signing key from env"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn normalize_strips_trailing_cr_lf_combinations() {
        assert_eq!(
            normalize_signing_key_bytes(b"key\r\n\r\n".to_vec()).unwrap(),
            b"key"
        );
        assert_eq!(
            normalize_signing_key_bytes(b"key\n\n".to_vec()).unwrap(),
            b"key"
        );
        assert_eq!(
            normalize_signing_key_bytes(b"key\r".to_vec()).unwrap(),
            b"key"
        );
        assert_eq!(
            normalize_signing_key_bytes(b"key\r\n".to_vec()).unwrap(),
            b"key"
        );
    }

    #[test]
    fn normalize_preserves_content_without_trailing_newlines() {
        assert_eq!(
            normalize_signing_key_bytes(b"raw-key".to_vec()).unwrap(),
            b"raw-key"
        );
    }

    #[test]
    fn normalize_preserves_embedded_newlines() {
        assert_eq!(
            normalize_signing_key_bytes(b"line1\nline2\n".to_vec()).unwrap(),
            b"line1\nline2"
        );
    }

    #[test]
    fn normalize_rejects_only_newlines() {
        let err = normalize_signing_key_bytes(b"\r\n".to_vec()).expect_err("expected error");
        assert!(err.to_string().contains("empty"));
        let err = normalize_signing_key_bytes(b"\n\n\n".to_vec()).expect_err("expected error");
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn normalize_rejects_empty_vec() {
        let err = normalize_signing_key_bytes(Vec::new()).expect_err("expected error");
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn load_key_from_file_strips_trailing_newlines() {
        let temp = tempdir().expect("tempdir");
        let key_path = temp.path().join("policy.key");
        fs::write(&key_path, b"my-secret\r\n\n").expect("write key");

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
        assert_eq!(key, b"my-secret");
    }

    #[test]
    fn load_key_from_file_only_crlf_is_empty() {
        let temp = tempdir().expect("tempdir");
        let key_path = temp.path().join("policy.key");
        fs::write(&key_path, b"\r\n").expect("write key");

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

    #[test]
    fn load_key_from_env_only_newlines_is_empty() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        #[expect(
            unsafe_code,
            reason = "Environment variable access is serialized or isolated at this boundary."
        )]
        unsafe {
            std::env::set_var("COCKPITCTL_POLICY_KEY_EMPTY_TEST", "\n\n");
        }
        let cfg = PolicySigningConfig {
            enabled: true,
            algorithm: cockpitctl_types::PolicySignatureAlgorithm::HmacSha256,
            key_path: None,
            key_env: Some("COCKPITCTL_POLICY_KEY_EMPTY_TEST".to_string()),
            key_id: None,
        };
        let err = load_policy_signing_key(&cfg).expect_err("expected empty key error");
        #[expect(
            unsafe_code,
            reason = "Environment variable access is serialized or isolated at this boundary."
        )]
        unsafe {
            std::env::remove_var("COCKPITCTL_POLICY_KEY_EMPTY_TEST");
        }
        assert!(format!("{:#}", err).contains("policy signing key is empty"));
    }

    #[test]
    fn load_key_preserves_binary_content_from_file() {
        let temp = tempdir().expect("tempdir");
        let key_path = temp.path().join("policy.key");
        let binary_key: Vec<u8> = vec![0x00, 0x01, 0xFF, 0xFE, 0xAB];
        fs::write(&key_path, &binary_key).expect("write key");

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
        assert_eq!(key, binary_key);
    }

    #[test]
    fn default_config_returns_none() {
        let cfg = PolicySigningConfig {
            enabled: false,
            algorithm: cockpitctl_types::PolicySignatureAlgorithm::HmacSha256,
            key_path: None,
            key_env: None,
            key_id: None,
        };
        assert!(load_policy_signing_key(&cfg).unwrap().is_none());
    }
}

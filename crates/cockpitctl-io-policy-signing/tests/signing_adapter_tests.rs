//! Integration tests for the policy-signing key-loading adapter.
//!
//! Exercises the public API (`load_policy_signing_key`) from the outside,
//! verifying key loading from files, missing/empty paths, and env fallback.

use cockpitctl_io_policy_signing::load_policy_signing_key;
use cockpitctl_types::{PolicySignatureAlgorithm, PolicySigningConfig};
use std::fs;
use std::sync::Mutex;
use tempfile::tempdir;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn cfg_with_path(path: &str) -> PolicySigningConfig {
    PolicySigningConfig {
        enabled: true,
        algorithm: PolicySignatureAlgorithm::HmacSha256,
        key_path: Some(path.to_string()),
        key_env: None,
        key_id: None,
    }
}

fn cfg_with_env(env_name: &str) -> PolicySigningConfig {
    PolicySigningConfig {
        enabled: true,
        algorithm: PolicySignatureAlgorithm::HmacSha256,
        key_path: None,
        key_env: Some(env_name.to_string()),
        key_id: None,
    }
}

// ── Loading key from valid path → key bytes ────────────────────────────

#[test]
fn valid_file_returns_key_bytes() {
    let tmp = tempdir().unwrap();
    let key_path = tmp.path().join("policy.key");
    fs::write(&key_path, b"my-secret-key\n").unwrap();

    let key = load_policy_signing_key(&cfg_with_path(&key_path.to_string_lossy()))
        .unwrap()
        .expect("key present");
    assert_eq!(key, b"my-secret-key");
}

#[test]
fn binary_content_preserved() {
    let tmp = tempdir().unwrap();
    let key_path = tmp.path().join("policy.key");
    let binary: Vec<u8> = vec![0x00, 0x01, 0xFF, 0xFE, 0xAB];
    fs::write(&key_path, &binary).unwrap();

    let key = load_policy_signing_key(&cfg_with_path(&key_path.to_string_lossy()))
        .unwrap()
        .expect("key present");
    assert_eq!(key, binary);
}

#[test]
fn trailing_newlines_stripped() {
    let tmp = tempdir().unwrap();
    let key_path = tmp.path().join("policy.key");
    fs::write(&key_path, b"secret\r\n\r\n").unwrap();

    let key = load_policy_signing_key(&cfg_with_path(&key_path.to_string_lossy()))
        .unwrap()
        .expect("key present");
    assert_eq!(key, b"secret");
}

// ── Loading key from missing path → error ──────────────────────────────

#[test]
fn missing_file_returns_error() {
    let err = load_policy_signing_key(&cfg_with_path("/nonexistent/path/policy.key")).unwrap_err();
    assert!(
        format!("{err:#}").contains("read policy signing key"),
        "unexpected: {err:#}"
    );
}

#[test]
fn empty_path_string_returns_error() {
    let err = load_policy_signing_key(&cfg_with_path("   ")).unwrap_err();
    assert!(
        err.to_string().contains("key_path is empty"),
        "unexpected: {err}"
    );
}

// ── Loading key from empty file → error ────────────────────────────────

#[test]
fn empty_file_returns_error() {
    let tmp = tempdir().unwrap();
    let key_path = tmp.path().join("policy.key");
    fs::write(&key_path, b"\n").unwrap();

    let err = load_policy_signing_key(&cfg_with_path(&key_path.to_string_lossy())).unwrap_err();
    assert!(format!("{err:#}").contains("policy signing key is empty"));
}

#[test]
fn file_only_crlf_returns_error() {
    let tmp = tempdir().unwrap();
    let key_path = tmp.path().join("policy.key");
    fs::write(&key_path, b"\r\n").unwrap();

    let err = load_policy_signing_key(&cfg_with_path(&key_path.to_string_lossy())).unwrap_err();
    assert!(format!("{err:#}").contains("policy signing key is empty"));
}

// ── Unconfigured → None ────────────────────────────────────────────────

#[test]
fn unconfigured_returns_none() {
    let cfg = PolicySigningConfig::default();
    assert!(load_policy_signing_key(&cfg).unwrap().is_none());
}

// ── Env fallback ───────────────────────────────────────────────────────

#[test]
fn env_fallback_works_when_path_unset() {
    let _guard = ENV_LOCK.lock().unwrap();
    unsafe {
        std::env::set_var("COCKPITCTL_SIGNING_INTEG_TEST", "env-secret\r\n");
    }
    let key = load_policy_signing_key(&cfg_with_env("COCKPITCTL_SIGNING_INTEG_TEST"))
        .unwrap()
        .expect("key present");
    unsafe {
        std::env::remove_var("COCKPITCTL_SIGNING_INTEG_TEST");
    }
    assert_eq!(key, b"env-secret");
}

#[test]
fn missing_env_var_returns_error() {
    let _guard = ENV_LOCK.lock().unwrap();
    let err =
        load_policy_signing_key(&cfg_with_env("COCKPITCTL_SIGNING_NONEXISTENT_999")).unwrap_err();
    assert!(
        format!("{err:#}").contains("read policy signing key from env"),
        "unexpected: {err:#}"
    );
}

#[test]
fn empty_env_name_returns_error() {
    let err = load_policy_signing_key(&cfg_with_env("  ")).unwrap_err();
    assert!(
        err.to_string().contains("key_env is empty"),
        "unexpected: {err}"
    );
}

// ── Precedence: path > env ─────────────────────────────────────────────

#[test]
fn path_takes_precedence_over_env() {
    let _guard = ENV_LOCK.lock().unwrap();
    let tmp = tempdir().unwrap();
    let key_path = tmp.path().join("policy.key");
    fs::write(&key_path, b"from-file").unwrap();

    unsafe {
        std::env::set_var("COCKPITCTL_SIGNING_PREC_TEST", "from-env");
    }
    let cfg = PolicySigningConfig {
        enabled: true,
        algorithm: PolicySignatureAlgorithm::HmacSha256,
        key_path: Some(key_path.to_string_lossy().to_string()),
        key_env: Some("COCKPITCTL_SIGNING_PREC_TEST".to_string()),
        key_id: None,
    };
    let key = load_policy_signing_key(&cfg).unwrap().expect("key present");
    unsafe {
        std::env::remove_var("COCKPITCTL_SIGNING_PREC_TEST");
    }
    assert_eq!(key, b"from-file");
}

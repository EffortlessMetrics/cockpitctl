//! Integration tests for cockpitctl-io-policy-signing.
//!
//! Focuses on real filesystem key loading, environment variable fallback,
//! key format validation, and edge cases around normalization.

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

// ── Policy signing key loading from file ───────────────────────────────

#[test]
fn load_key_from_file_returns_trimmed_bytes() {
    let tmp = tempdir().unwrap();
    let key_path = tmp.path().join("key.pem");
    fs::write(&key_path, b"super-secret-key\n").unwrap();

    let key = load_policy_signing_key(&cfg_with_path(&key_path.to_string_lossy()))
        .unwrap()
        .expect("key present");
    assert_eq!(key, b"super-secret-key");
}

#[test]
fn load_key_from_file_preserves_internal_newlines() {
    let tmp = tempdir().unwrap();
    let key_path = tmp.path().join("multi.key");
    fs::write(&key_path, b"line1\nline2\nline3\n").unwrap();

    let key = load_policy_signing_key(&cfg_with_path(&key_path.to_string_lossy()))
        .unwrap()
        .expect("key present");
    // Only trailing newlines stripped; internal ones preserved
    assert_eq!(key, b"line1\nline2\nline3");
}

#[test]
fn load_key_from_file_handles_windows_line_endings() {
    let tmp = tempdir().unwrap();
    let key_path = tmp.path().join("win.key");
    fs::write(&key_path, b"key-value\r\n\r\n").unwrap();

    let key = load_policy_signing_key(&cfg_with_path(&key_path.to_string_lossy()))
        .unwrap()
        .expect("key present");
    assert_eq!(key, b"key-value");
}

// ── Missing key file → error handling ──────────────────────────────────

#[test]
fn missing_key_file_returns_contextual_error() {
    let tmp = tempdir().unwrap();
    let missing = tmp.path().join("missing.key");
    let err = load_policy_signing_key(&cfg_with_path(&missing.to_string_lossy())).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("read policy signing key"),
        "expected context: {msg}"
    );
}

#[test]
fn directory_instead_of_file_returns_error() {
    let tmp = tempdir().unwrap();
    // Point at a directory, not a file
    let err = load_policy_signing_key(&cfg_with_path(&tmp.path().to_string_lossy())).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("read policy signing key"),
        "expected read error: {msg}"
    );
}

// ── Invalid key format → error ─────────────────────────────────────────

#[test]
fn empty_file_after_normalization_returns_error() {
    let tmp = tempdir().unwrap();
    let key_path = tmp.path().join("empty.key");
    fs::write(&key_path, b"\n\r\n\n").unwrap();

    let err = load_policy_signing_key(&cfg_with_path(&key_path.to_string_lossy())).unwrap_err();
    assert!(format!("{err:#}").contains("policy signing key is empty"));
}

#[test]
fn zero_byte_file_returns_error() {
    let tmp = tempdir().unwrap();
    let key_path = tmp.path().join("zero.key");
    fs::write(&key_path, b"").unwrap();

    let err = load_policy_signing_key(&cfg_with_path(&key_path.to_string_lossy())).unwrap_err();
    assert!(format!("{err:#}").contains("policy signing key is empty"));
}

// ── Signature verification (key round-trip) ────────────────────────────

#[test]
fn key_loaded_from_file_is_deterministic() {
    let tmp = tempdir().unwrap();
    let key_path = tmp.path().join("stable.key");
    fs::write(&key_path, b"deterministic-key\n").unwrap();

    let cfg = cfg_with_path(&key_path.to_string_lossy());
    let key1 = load_policy_signing_key(&cfg).unwrap().unwrap();
    let key2 = load_policy_signing_key(&cfg).unwrap().unwrap();
    assert_eq!(key1, key2, "same file should produce identical key bytes");
}

#[test]
fn binary_key_content_preserved_exactly() {
    let tmp = tempdir().unwrap();
    let key_path = tmp.path().join("binary.key");
    let binary: Vec<u8> = (0..=255).collect();
    fs::write(&key_path, &binary).unwrap();

    let key = load_policy_signing_key(&cfg_with_path(&key_path.to_string_lossy()))
        .unwrap()
        .expect("key present");
    // Only trailing \n and \r are stripped; 0x0A=\n is last byte of 0..=255
    // so it will be stripped. 0..=254 should remain, plus check length.
    assert!(!key.is_empty());
    // The last non-newline byte should be 0xFD (253) since 0xFE=254, 0xFF=255
    // Wait: 0x0A=10=\n, 0x0D=13=\r — those get stripped only from trailing.
    // 255=0xFF is last byte — that is NOT \n or \r, so nothing is stripped.
    assert_eq!(key.len(), 256);
    assert_eq!(key, binary);
}

// ── Key format validation ──────────────────────────────────────────────

#[test]
fn env_key_with_only_whitespace_newlines_is_empty_error() {
    let _guard = ENV_LOCK.lock().unwrap();
    unsafe {
        std::env::set_var("COCKPITCTL_INTEG_EMPTY_KEY", "\r\n\n\r");
    }
    let err = load_policy_signing_key(&cfg_with_env("COCKPITCTL_INTEG_EMPTY_KEY")).unwrap_err();
    unsafe {
        std::env::remove_var("COCKPITCTL_INTEG_EMPTY_KEY");
    }
    assert!(format!("{err:#}").contains("policy signing key is empty"));
}

#[test]
fn unconfigured_signing_returns_none() {
    let cfg = PolicySigningConfig {
        enabled: false,
        algorithm: PolicySignatureAlgorithm::HmacSha256,
        key_path: None,
        key_env: None,
        key_id: None,
    };
    assert!(load_policy_signing_key(&cfg).unwrap().is_none());
}

// ── Precedence: path > env ─────────────────────────────────────────────

#[test]
fn file_path_takes_precedence_over_env_var() {
    let _guard = ENV_LOCK.lock().unwrap();
    let tmp = tempdir().unwrap();
    let key_path = tmp.path().join("file.key");
    fs::write(&key_path, b"file-key").unwrap();

    unsafe {
        std::env::set_var("COCKPITCTL_INTEG_PREC_KEY", "env-key");
    }
    let cfg = PolicySigningConfig {
        enabled: true,
        algorithm: PolicySignatureAlgorithm::HmacSha256,
        key_path: Some(key_path.to_string_lossy().to_string()),
        key_env: Some("COCKPITCTL_INTEG_PREC_KEY".to_string()),
        key_id: None,
    };
    let key = load_policy_signing_key(&cfg).unwrap().expect("key present");
    unsafe {
        std::env::remove_var("COCKPITCTL_INTEG_PREC_KEY");
    }
    assert_eq!(key, b"file-key");
}

// ── Key from nested path ───────────────────────────────────────────────

#[test]
fn key_loaded_from_deeply_nested_path() {
    let tmp = tempdir().unwrap();
    let nested = tmp.path().join("a").join("b").join("c");
    fs::create_dir_all(&nested).unwrap();
    let key_path = nested.join("deep.key");
    fs::write(&key_path, b"deep-secret").unwrap();

    let key = load_policy_signing_key(&cfg_with_path(&key_path.to_string_lossy()))
        .unwrap()
        .expect("key present");
    assert_eq!(key, b"deep-secret");
}

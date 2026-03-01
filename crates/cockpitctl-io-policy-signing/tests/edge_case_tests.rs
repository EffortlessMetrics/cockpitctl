//! Edge-case tests for the policy-signing key-loading adapter.
//!
//! Covers null bytes in key material, large key files, UTF-8 key content,
//! and key file permission/format edge cases.

use cockpitctl_io_policy_signing::load_policy_signing_key;
use cockpitctl_types::{PolicySignatureAlgorithm, PolicySigningConfig};
use std::fs;
use tempfile::tempdir;

fn cfg_with_path(path: &str) -> PolicySigningConfig {
    PolicySigningConfig {
        enabled: true,
        algorithm: PolicySignatureAlgorithm::HmacSha256,
        key_path: Some(path.to_string()),
        key_env: None,
        key_id: None,
    }
}

// ── Key with embedded null bytes preserved ──────────────────────────────

#[test]
fn key_with_embedded_null_bytes_preserved() {
    let tmp = tempdir().unwrap();
    let key_path = tmp.path().join("null.key");
    let key_bytes: Vec<u8> = vec![0x41, 0x00, 0x42, 0x00, 0x43]; // A\0B\0C
    fs::write(&key_path, &key_bytes).unwrap();

    let key = load_policy_signing_key(&cfg_with_path(&key_path.to_string_lossy()))
        .unwrap()
        .expect("key present");
    assert_eq!(key, key_bytes, "null bytes should be preserved in key");
}

// ── Large key file (e.g. RSA PEM) loads successfully ───────────────────

#[test]
fn large_key_file_loads_successfully() {
    let tmp = tempdir().unwrap();
    let key_path = tmp.path().join("large.key");
    // Simulate a large key (8KB of random-ish bytes)
    let large_key: Vec<u8> = (0..8192).map(|i| (i % 251) as u8).collect();
    fs::write(&key_path, &large_key).unwrap();

    let key = load_policy_signing_key(&cfg_with_path(&key_path.to_string_lossy()))
        .unwrap()
        .expect("key present");
    assert_eq!(key.len(), 8192);
    assert_eq!(key, large_key);
}

// ── UTF-8 key content preserved correctly ──────────────────────────────

#[test]
fn utf8_key_content_preserved() {
    let tmp = tempdir().unwrap();
    let key_path = tmp.path().join("utf8.key");
    let utf8_key = "🔑secret-clé-密钥";
    fs::write(&key_path, utf8_key.as_bytes()).unwrap();

    let key = load_policy_signing_key(&cfg_with_path(&key_path.to_string_lossy()))
        .unwrap()
        .expect("key present");
    assert_eq!(key, utf8_key.as_bytes());
}

// ── Key with only \r characters (no \n) stripped correctly ──────────────

#[test]
fn key_with_only_carriage_returns_stripped() {
    let tmp = tempdir().unwrap();
    let key_path = tmp.path().join("cr.key");
    fs::write(&key_path, b"secret\r\r\r").unwrap();

    let key = load_policy_signing_key(&cfg_with_path(&key_path.to_string_lossy()))
        .unwrap()
        .expect("key present");
    assert_eq!(key, b"secret");
}

// ── Single byte key is valid ───────────────────────────────────────────

#[test]
fn single_byte_key_is_valid() {
    let tmp = tempdir().unwrap();
    let key_path = tmp.path().join("tiny.key");
    fs::write(&key_path, b"X").unwrap();

    let key = load_policy_signing_key(&cfg_with_path(&key_path.to_string_lossy()))
        .unwrap()
        .expect("key present");
    assert_eq!(key, b"X");
}

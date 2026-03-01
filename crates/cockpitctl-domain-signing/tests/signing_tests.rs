//! Integration / snapshot tests for `cockpitctl-domain-signing`.

use cockpitctl_domain_signing::{
    canonical_policy_snapshot_bytes, policy_snapshot_sha256_hex, sign_policy_snapshot,
};
use cockpitctl_types::{
    MissingPolicy, PolicySensorSnapshot, PolicySignatureAlgorithm, PolicySnapshot,
};

// ── Helpers ────────────────────────────────────────────────────────────────

fn sample_policy() -> PolicySnapshot {
    PolicySnapshot {
        warn_is_fail: false,
        max_highlights: 5,
        max_per_sensor_findings: 50,
        max_annotations: 10,
        section_order: vec!["quality".into(), "security".into()],
        sensors: vec![PolicySensorSnapshot {
            id: "clippy".into(),
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("quality".into()),
            require_label: None,
            repro: None,
        }],
    }
}

fn empty_policy() -> PolicySnapshot {
    PolicySnapshot {
        warn_is_fail: false,
        max_highlights: 0,
        max_per_sensor_findings: 0,
        max_annotations: 0,
        section_order: vec![],
        sensors: vec![],
    }
}

fn multi_sensor_policy() -> PolicySnapshot {
    PolicySnapshot {
        warn_is_fail: true,
        max_highlights: 20,
        max_per_sensor_findings: 100,
        max_annotations: 50,
        section_order: vec!["quality".into(), "security".into(), "coverage".into()],
        sensors: vec![
            PolicySensorSnapshot {
                id: "clippy".into(),
                blocking: true,
                missing: MissingPolicy::Fail,
                section: Some("quality".into()),
                require_label: None,
                repro: None,
            },
            PolicySensorSnapshot {
                id: "audit".into(),
                blocking: true,
                missing: MissingPolicy::Warn,
                section: Some("security".into()),
                require_label: None,
                repro: Some("cargo audit".into()),
            },
            PolicySensorSnapshot {
                id: "coverage".into(),
                blocking: false,
                missing: MissingPolicy::Skip,
                section: Some("coverage".into()),
                require_label: None,
                repro: None,
            },
        ],
    }
}

// ── SHA256 consistency: same policy → same hash ────────────────────────────

#[test]
fn same_policy_produces_same_sha256() {
    let h1 = policy_snapshot_sha256_hex(&sample_policy()).unwrap();
    let h2 = policy_snapshot_sha256_hex(&sample_policy()).unwrap();
    assert_eq!(h1, h2);
    insta::assert_snapshot!("sample_policy_sha256", h1);
}

// ── Different policies → different hashes ──────────────────────────────────

#[test]
fn different_policies_produce_different_hashes() {
    let h_sample = policy_snapshot_sha256_hex(&sample_policy()).unwrap();
    let h_empty = policy_snapshot_sha256_hex(&empty_policy()).unwrap();
    let h_multi = policy_snapshot_sha256_hex(&multi_sensor_policy()).unwrap();

    assert_ne!(h_sample, h_empty);
    assert_ne!(h_sample, h_multi);
    assert_ne!(h_empty, h_multi);
}

// ── Empty policy hashing ───────────────────────────────────────────────────

#[test]
fn empty_policy_sha256_snapshot() {
    let hex = policy_snapshot_sha256_hex(&empty_policy()).unwrap();
    assert_eq!(hex.len(), 64);
    insta::assert_snapshot!("empty_policy_sha256", hex);
}

// ── Signing determinism ────────────────────────────────────────────────────

#[test]
fn signing_is_deterministic() {
    let ev1 = sign_policy_snapshot(
        &sample_policy(),
        PolicySignatureAlgorithm::HmacSha256,
        b"test-key",
        Some("k1".into()),
    )
    .unwrap();
    let ev2 = sign_policy_snapshot(
        &sample_policy(),
        PolicySignatureAlgorithm::HmacSha256,
        b"test-key",
        Some("k1".into()),
    )
    .unwrap();
    assert_eq!(ev1, ev2);
    insta::assert_snapshot!("sample_policy_signature", ev1.signature);
}

// ── Signing with empty policy ──────────────────────────────────────────────

#[test]
fn sign_empty_policy_snapshot() {
    let ev = sign_policy_snapshot(
        &empty_policy(),
        PolicySignatureAlgorithm::HmacSha256,
        b"empty-key",
        None,
    )
    .unwrap();
    assert_eq!(ev.signature.len(), 64);
    assert_eq!(ev.policy_sha256.len(), 64);
    insta::assert_snapshot!("empty_policy_signature", ev.signature);
}

// ── Multi-sensor policy signing ────────────────────────────────────────────

#[test]
fn multi_sensor_policy_signing_snapshot() {
    let ev = sign_policy_snapshot(
        &multi_sensor_policy(),
        PolicySignatureAlgorithm::HmacSha256,
        b"multi-key",
        Some("v2".into()),
    )
    .unwrap();
    insta::assert_snapshot!("multi_sensor_signature", ev.signature);
    insta::assert_snapshot!("multi_sensor_sha256", ev.policy_sha256);
}

// ── Canonical bytes are compact JSON ───────────────────────────────────────

#[test]
fn canonical_bytes_are_compact() {
    let bytes = canonical_policy_snapshot_bytes(&sample_policy()).unwrap();
    let s = std::str::from_utf8(&bytes).unwrap();
    assert!(
        !s.contains('\n'),
        "canonical JSON must be compact (no newlines)"
    );
    assert!(
        !s.contains("  "),
        "canonical JSON must be compact (no indentation)"
    );
    // Round-trip
    let _: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
}

// ── policy_sha256 in evidence matches standalone digest ────────────────────

#[test]
fn evidence_sha256_matches_standalone() {
    let policy = multi_sensor_policy();
    let ev = sign_policy_snapshot(
        &policy,
        PolicySignatureAlgorithm::HmacSha256,
        b"cross-check",
        None,
    )
    .unwrap();
    let standalone = policy_snapshot_sha256_hex(&policy).unwrap();
    assert_eq!(ev.policy_sha256, standalone);
}

// ── Different keys → different signatures, same policy_sha256 ──────────────

#[test]
fn different_keys_same_policy_sha256() {
    let policy = sample_policy();
    let ev_a = sign_policy_snapshot(
        &policy,
        PolicySignatureAlgorithm::HmacSha256,
        b"key-a",
        None,
    )
    .unwrap();
    let ev_b = sign_policy_snapshot(
        &policy,
        PolicySignatureAlgorithm::HmacSha256,
        b"key-b",
        None,
    )
    .unwrap();
    assert_eq!(ev_a.policy_sha256, ev_b.policy_sha256);
    assert_ne!(ev_a.signature, ev_b.signature);
}

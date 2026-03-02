//! Integration tests for cockpitctl-domain-signing.
//!
//! Exercises the public signing API through realistic policy snapshots,
//! verifying determinism, roundtrip consistency, and edge-case behaviour.

use cockpitctl_domain_signing::{
    canonical_policy_snapshot_bytes, policy_snapshot_sha256_hex, sign_policy_snapshot,
    sign_policy_snapshot_hmac_sha256,
};
use cockpitctl_types::{
    MissingPolicy, PolicySensorSnapshot, PolicySignatureAlgorithm, PolicySnapshot,
};

// ── helpers ──────────────────────────────────────────────────────────

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

fn realistic_policy() -> PolicySnapshot {
    PolicySnapshot {
        warn_is_fail: true,
        max_highlights: 10,
        max_per_sensor_findings: 50,
        max_annotations: 20,
        section_order: vec!["quality".into(), "security".into(), "style".into()],
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
                id: "deny".into(),
                blocking: true,
                missing: MissingPolicy::Warn,
                section: Some("security".into()),
                require_label: Some("security-review".into()),
                repro: Some("cargo deny check".into()),
            },
            PolicySensorSnapshot {
                id: "fmt".into(),
                blocking: false,
                missing: MissingPolicy::Skip,
                section: Some("style".into()),
                require_label: None,
                repro: None,
            },
        ],
    }
}

// ── signing with known key → deterministic output ────────────────────

#[test]
fn sign_known_key_is_deterministic() {
    let policy = realistic_policy();
    let key = b"test-signing-key-v1";
    let e1 = sign_policy_snapshot_hmac_sha256(&policy, key, Some("k1".into())).unwrap();
    let e2 = sign_policy_snapshot_hmac_sha256(&policy, key, Some("k1".into())).unwrap();
    assert_eq!(e1.signature, e2.signature);
    assert_eq!(e1.policy_sha256, e2.policy_sha256);
}

#[test]
fn sign_via_dispatcher_matches_direct_call() {
    let policy = realistic_policy();
    let key = b"dispatcher-key";
    let direct = sign_policy_snapshot_hmac_sha256(&policy, key, Some("kid".into())).unwrap();
    let dispatched = sign_policy_snapshot(
        &policy,
        PolicySignatureAlgorithm::HmacSha256,
        key,
        Some("kid".into()),
    )
    .unwrap();
    assert_eq!(direct.signature, dispatched.signature);
    assert_eq!(direct.policy_sha256, dispatched.policy_sha256);
}

// ── signature verification roundtrip ─────────────────────────────────

#[test]
fn sha256_digest_matches_evidence_field() {
    let policy = realistic_policy();
    let digest = policy_snapshot_sha256_hex(&policy).unwrap();
    let evidence = sign_policy_snapshot_hmac_sha256(&policy, b"roundtrip-key", None).unwrap();
    assert_eq!(digest, evidence.policy_sha256);
}

#[test]
fn canonical_bytes_roundtrip_to_valid_json() {
    let policy = realistic_policy();
    let bytes = canonical_policy_snapshot_bytes(&policy).unwrap();
    let parsed: PolicySnapshot = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(parsed, policy);
}

// ── different key lengths → all work ─────────────────────────────────

#[test]
fn short_key_produces_valid_signature() {
    let evidence = sign_policy_snapshot_hmac_sha256(&empty_policy(), b"k", None).unwrap();
    assert_eq!(evidence.signature.len(), 64);
}

#[test]
fn long_key_exceeding_block_size_produces_valid_signature() {
    // SHA-256 block size is 64 bytes; keys > 64 are hashed first.
    let long_key = vec![0xABu8; 128];
    let evidence = sign_policy_snapshot_hmac_sha256(&empty_policy(), &long_key, None).unwrap();
    assert_eq!(evidence.signature.len(), 64);
}

#[test]
fn key_exactly_block_size_produces_valid_signature() {
    let key = vec![0x42u8; 64];
    let evidence = sign_policy_snapshot_hmac_sha256(&realistic_policy(), &key, None).unwrap();
    assert_eq!(evidence.signature.len(), 64);
}

// ── empty payload → defined behaviour ────────────────────────────────

#[test]
fn empty_policy_produces_stable_digest() {
    let d1 = policy_snapshot_sha256_hex(&empty_policy()).unwrap();
    let d2 = policy_snapshot_sha256_hex(&empty_policy()).unwrap();
    assert_eq!(d1, d2);
    assert_eq!(d1.len(), 64);
}

// ── invalid / edge-case keys → detection ─────────────────────────────

#[test]
fn empty_key_is_rejected() {
    let result = sign_policy_snapshot_hmac_sha256(&empty_policy(), b"", None);
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("empty"),
        "error should mention empty key: {msg}"
    );
}

#[test]
fn empty_key_rejected_via_dispatcher() {
    let result = sign_policy_snapshot(
        &empty_policy(),
        PolicySignatureAlgorithm::HmacSha256,
        b"",
        None,
    );
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("empty"));
}

// ── signing determinism across many iterations ───────────────────────

#[test]
fn signing_determinism_100_iterations() {
    let policy = realistic_policy();
    let key = b"iteration-key";
    let reference = sign_policy_snapshot_hmac_sha256(&policy, key, Some("iter".into())).unwrap();
    for i in 0..100 {
        let ev = sign_policy_snapshot_hmac_sha256(&policy, key, Some("iter".into())).unwrap();
        assert_eq!(
            reference.signature, ev.signature,
            "signature diverged at iteration {i}"
        );
        assert_eq!(reference.policy_sha256, ev.policy_sha256);
    }
}

// ── signature hex encoding is always lowercase ───────────────────────

#[test]
fn signature_hex_is_lowercase() {
    let ev = sign_policy_snapshot_hmac_sha256(&realistic_policy(), b"hex-key", None).unwrap();
    assert!(
        ev.signature
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        "signature should be lowercase hex: {}",
        ev.signature
    );
    assert!(
        ev.policy_sha256
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        "policy_sha256 should be lowercase hex: {}",
        ev.policy_sha256
    );
}

// ── signing policy evaluation (evidence fields) ──────────────────────

#[test]
fn evidence_contains_correct_schema_id() {
    let evidence =
        sign_policy_snapshot_hmac_sha256(&realistic_policy(), b"schema-key", None).unwrap();
    assert_eq!(evidence.schema, "cockpit.policy_signature.v1");
}

#[test]
fn evidence_algorithm_is_hmac_sha256() {
    let evidence =
        sign_policy_snapshot_hmac_sha256(&realistic_policy(), b"algo-key", None).unwrap();
    assert_eq!(evidence.algorithm, PolicySignatureAlgorithm::HmacSha256);
}

#[test]
fn key_id_propagated_when_provided() {
    let evidence =
        sign_policy_snapshot_hmac_sha256(&empty_policy(), b"key", Some("my-key-id".into()))
            .unwrap();
    assert_eq!(evidence.key_id.as_deref(), Some("my-key-id"));
}

#[test]
fn key_id_none_when_omitted() {
    let evidence = sign_policy_snapshot_hmac_sha256(&empty_policy(), b"key", None).unwrap();
    assert!(evidence.key_id.is_none());
}

// ── different policies produce different signatures ──────────────────

#[test]
fn different_policies_yield_different_signatures() {
    let key = b"diff-key";
    let e1 = sign_policy_snapshot_hmac_sha256(&empty_policy(), key, None).unwrap();
    let e2 = sign_policy_snapshot_hmac_sha256(&realistic_policy(), key, None).unwrap();
    assert_ne!(e1.signature, e2.signature);
    assert_ne!(e1.policy_sha256, e2.policy_sha256);
}

#[test]
fn different_keys_yield_different_signatures() {
    let policy = realistic_policy();
    let e1 = sign_policy_snapshot_hmac_sha256(&policy, b"key-alpha", None).unwrap();
    let e2 = sign_policy_snapshot_hmac_sha256(&policy, b"key-beta", None).unwrap();
    // Same policy → same digest, but different signatures.
    assert_eq!(e1.policy_sha256, e2.policy_sha256);
    assert_ne!(e1.signature, e2.signature);
}

// ── tamper detection: each field mutation invalidates signature ───────

#[test]
fn tampered_max_highlights_changes_signature() {
    let key = b"tamper-field-key";
    let original = sign_policy_snapshot_hmac_sha256(&realistic_policy(), key, None).unwrap();

    let mut policy = realistic_policy();
    policy.max_highlights += 1;
    let tampered = sign_policy_snapshot_hmac_sha256(&policy, key, None).unwrap();

    assert_ne!(original.signature, tampered.signature);
    assert_ne!(original.policy_sha256, tampered.policy_sha256);
}

#[test]
fn tampered_max_per_sensor_findings_changes_signature() {
    let key = b"tamper-findings";
    let original = sign_policy_snapshot_hmac_sha256(&realistic_policy(), key, None).unwrap();

    let mut policy = realistic_policy();
    policy.max_per_sensor_findings += 1;
    let tampered = sign_policy_snapshot_hmac_sha256(&policy, key, None).unwrap();

    assert_ne!(original.signature, tampered.signature);
}

#[test]
fn tampered_max_annotations_changes_signature() {
    let key = b"tamper-annotations";
    let original = sign_policy_snapshot_hmac_sha256(&realistic_policy(), key, None).unwrap();

    let mut policy = realistic_policy();
    policy.max_annotations += 1;
    let tampered = sign_policy_snapshot_hmac_sha256(&policy, key, None).unwrap();

    assert_ne!(original.signature, tampered.signature);
}

#[test]
fn tampered_section_order_changes_signature() {
    let key = b"tamper-sections";
    let original = sign_policy_snapshot_hmac_sha256(&realistic_policy(), key, None).unwrap();

    let mut policy = realistic_policy();
    policy.section_order.reverse();
    let tampered = sign_policy_snapshot_hmac_sha256(&policy, key, None).unwrap();

    assert_ne!(original.signature, tampered.signature);
}

#[test]
fn tampered_blocking_flag_changes_signature() {
    let key = b"tamper-blocking";
    let original = sign_policy_snapshot_hmac_sha256(&realistic_policy(), key, None).unwrap();

    let mut policy = realistic_policy();
    policy.sensors[0].blocking = !policy.sensors[0].blocking;
    let tampered = sign_policy_snapshot_hmac_sha256(&policy, key, None).unwrap();

    assert_ne!(original.signature, tampered.signature);
}

// ── unicode content in policy fields ─────────────────────────────────

#[test]
fn unicode_sensor_id_signs_correctly() {
    let policy = PolicySnapshot {
        warn_is_fail: false,
        max_highlights: 5,
        max_per_sensor_findings: 10,
        max_annotations: 5,
        section_order: vec!["品質".into()],
        sensors: vec![PolicySensorSnapshot {
            id: "lint-日本語".into(),
            blocking: true,
            missing: MissingPolicy::Fail,
            section: Some("品質".into()),
            require_label: None,
            repro: None,
        }],
    };
    let ev = sign_policy_snapshot_hmac_sha256(&policy, b"unicode-key", None).unwrap();
    assert_eq!(ev.signature.len(), 64);
    // Deterministic.
    let ev2 = sign_policy_snapshot_hmac_sha256(&policy, b"unicode-key", None).unwrap();
    assert_eq!(ev.signature, ev2.signature);
}

// ── policy with all optional fields populated ────────────────────────

#[test]
fn all_optional_fields_populated_affects_signature() {
    let key = b"optionals-key";

    let mut policy_without = realistic_policy();
    policy_without.sensors[0].require_label = None;
    policy_without.sensors[0].repro = None;
    let ev_without = sign_policy_snapshot_hmac_sha256(&policy_without, key, None).unwrap();

    let mut policy_with = realistic_policy();
    policy_with.sensors[0].require_label = Some("needs-review".into());
    policy_with.sensors[0].repro = Some("cargo clippy".into());
    let ev_with = sign_policy_snapshot_hmac_sha256(&policy_with, key, None).unwrap();

    assert_ne!(ev_without.signature, ev_with.signature);
}

// ── policy deserialization roundtrip preserves signing ────────────────

#[test]
fn policy_deserialize_roundtrip_preserves_signature() {
    let policy = realistic_policy();
    let key = b"roundtrip-key";
    let original = sign_policy_snapshot_hmac_sha256(&policy, key, None).unwrap();

    // Serialize → deserialize → re-sign.
    let bytes = canonical_policy_snapshot_bytes(&policy).unwrap();
    let deserialized: PolicySnapshot = serde_json::from_slice(&bytes).unwrap();
    let re_signed = sign_policy_snapshot_hmac_sha256(&deserialized, key, None).unwrap();

    assert_eq!(original.signature, re_signed.signature);
    assert_eq!(original.policy_sha256, re_signed.policy_sha256);
}

// ── evidence JSON serialization roundtrip ────────────────────────────

#[test]
fn evidence_json_serialization_roundtrip() {
    let ev =
        sign_policy_snapshot_hmac_sha256(&realistic_policy(), b"json-key", Some("kid-1".into()))
            .unwrap();
    let json = serde_json::to_string(&ev).unwrap();
    let deserialized: cockpitctl_types::PolicySignatureEvidence =
        serde_json::from_str(&json).unwrap();
    assert_eq!(ev, deserialized);
}

// ── key with null bytes ──────────────────────────────────────────────

#[test]
fn key_with_null_bytes_produces_valid_signature() {
    let key = b"\x00\x01\x00\x02\x00\x03";
    let ev = sign_policy_snapshot_hmac_sha256(&empty_policy(), key, None).unwrap();
    assert_eq!(ev.signature.len(), 64);
    // Different from an all-ones key of the same length.
    let ev2 = sign_policy_snapshot_hmac_sha256(&empty_policy(), b"\x01\x01\x01\x02\x01\x03", None)
        .unwrap();
    assert_ne!(ev.signature, ev2.signature);
}

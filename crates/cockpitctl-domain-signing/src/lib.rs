//! Policy-signing domain boundary microcrate.
//!
//! Provides canonical serialisation of policy snapshots and HMAC-SHA256
//! signing for tamper-evident policy evidence in the cockpit report.

#![warn(missing_docs)]

use anyhow::{Context, Result};
use cockpitctl_types::{
    POLICY_SIGNATURE_SCHEMA_ID, PolicySignatureAlgorithm, PolicySignatureEvidence, PolicySnapshot,
};
use sha2::{Digest, Sha256};

/// Canonical policy snapshot bytes used for hashing/signing.
///
/// Canonicalization is the compact serde JSON encoding of `PolicySnapshot`.
/// Determinism relies on stable field ordering and pre-sorted vectors.
///
/// # Examples
///
/// ```
/// use cockpitctl_domain_signing::canonical_policy_snapshot_bytes;
/// use cockpitctl_types::PolicySnapshot;
///
/// let policy = PolicySnapshot {
///     warn_is_fail: false,
///     max_highlights: 5,
///     max_per_sensor_findings: 20,
///     max_annotations: 10,
///     section_order: vec![],
///     sensors: vec![],
/// };
/// let bytes = canonical_policy_snapshot_bytes(&policy).unwrap();
/// let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
/// assert_eq!(json["max_highlights"], 5);
/// ```
pub fn canonical_policy_snapshot_bytes(policy: &PolicySnapshot) -> Result<Vec<u8>> {
    serde_json::to_vec(policy).context("serialize policy snapshot for signing")
}

/// Compute SHA-256 digest (hex) of the canonical policy snapshot bytes.
///
/// # Examples
///
/// ```
/// use cockpitctl_domain_signing::policy_snapshot_sha256_hex;
/// use cockpitctl_types::PolicySnapshot;
///
/// let policy = PolicySnapshot {
///     warn_is_fail: false,
///     max_highlights: 5,
///     max_per_sensor_findings: 20,
///     max_annotations: 10,
///     section_order: vec![],
///     sensors: vec![],
/// };
/// let hex = policy_snapshot_sha256_hex(&policy).unwrap();
/// assert_eq!(hex.len(), 64); // SHA-256 hex string
/// ```
pub fn policy_snapshot_sha256_hex(policy: &PolicySnapshot) -> Result<String> {
    let payload = canonical_policy_snapshot_bytes(policy)?;
    Ok(hex::encode(Sha256::digest(payload)))
}

/// Sign the policy snapshot with the configured algorithm.
///
/// # Examples
///
/// ```
/// use cockpitctl_domain_signing::sign_policy_snapshot;
/// use cockpitctl_types::{PolicySignatureAlgorithm, PolicySnapshot};
///
/// let policy = PolicySnapshot {
///     warn_is_fail: false,
///     max_highlights: 5,
///     max_per_sensor_findings: 20,
///     max_annotations: 10,
///     section_order: vec![],
///     sensors: vec![],
/// };
/// let evidence = sign_policy_snapshot(
///     &policy,
///     PolicySignatureAlgorithm::HmacSha256,
///     b"my-secret-key",
///     Some("key-v1".into()),
/// ).unwrap();
/// assert_eq!(evidence.signature.len(), 64);
/// ```
pub fn sign_policy_snapshot(
    policy: &PolicySnapshot,
    algorithm: PolicySignatureAlgorithm,
    key: &[u8],
    key_id: Option<String>,
) -> Result<PolicySignatureEvidence> {
    match algorithm {
        PolicySignatureAlgorithm::HmacSha256 => {
            sign_policy_snapshot_hmac_sha256(policy, key, key_id)
        }
    }
}

/// Sign the policy snapshot using HMAC-SHA256.
pub fn sign_policy_snapshot_hmac_sha256(
    policy: &PolicySnapshot,
    key: &[u8],
    key_id: Option<String>,
) -> Result<PolicySignatureEvidence> {
    if key.is_empty() {
        anyhow::bail!("policy signing key is empty");
    }

    let payload = canonical_policy_snapshot_bytes(policy)?;
    let policy_sha256 = hex::encode(Sha256::digest(&payload));
    let signature = hex::encode(hmac_sha256(key, &payload));

    Ok(PolicySignatureEvidence {
        schema: POLICY_SIGNATURE_SCHEMA_ID.to_string(),
        algorithm: PolicySignatureAlgorithm::HmacSha256,
        policy_sha256,
        signature,
        key_id,
    })
}

/// HMAC-SHA256 (RFC 2104) over raw key + message.
fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64; // SHA-256 block size.
    let mut key_block = [0u8; BLOCK];

    if key.len() > BLOCK {
        let hashed = Sha256::digest(key);
        key_block[..32].copy_from_slice(&hashed);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }

    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= key_block[i];
        opad[i] ^= key_block[i];
    }

    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(message);
    let inner_digest = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_digest);
    let digest = outer.finalize();

    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use cockpitctl_types::{MissingPolicy, PolicySensorSnapshot};

    fn sample_policy() -> PolicySnapshot {
        PolicySnapshot {
            warn_is_fail: false,
            max_highlights: 5,
            max_per_sensor_findings: 50,
            max_annotations: 10,
            section_order: vec!["quality".to_string(), "security".to_string()],
            sensors: vec![PolicySensorSnapshot {
                id: "clippy".to_string(),
                blocking: true,
                missing: MissingPolicy::Fail,
                section: Some("quality".to_string()),
                require_label: None,
                repro: None,
            }],
        }
    }

    #[test]
    fn canonical_bytes_deterministic() {
        let policy = sample_policy();
        let a = canonical_policy_snapshot_bytes(&policy).unwrap();
        let b = canonical_policy_snapshot_bytes(&policy).unwrap();
        assert_eq!(a, b);

        // Must be valid JSON.
        let parsed: serde_json::Value = serde_json::from_slice(&a).unwrap();
        assert_eq!(parsed["warn_is_fail"], false);
        assert_eq!(parsed["max_highlights"], 5);
    }

    #[test]
    fn sha256_hex_returns_expected_digest() {
        let policy = sample_policy();
        let hex_digest = policy_snapshot_sha256_hex(&policy).unwrap();

        // SHA-256 hex is always 64 characters.
        assert_eq!(hex_digest.len(), 64);
        assert!(hex_digest.chars().all(|c| c.is_ascii_hexdigit()));

        // Re-compute manually for cross-check.
        let payload = canonical_policy_snapshot_bytes(&policy).unwrap();
        let expected = hex::encode(Sha256::digest(&payload));
        assert_eq!(hex_digest, expected);
    }

    #[test]
    fn hmac_sha256_sign_produces_valid_evidence() {
        let policy = sample_policy();
        let key = b"test-secret-key";
        let key_id = Some("my-key-v1".to_string());

        let evidence = sign_policy_snapshot_hmac_sha256(&policy, key, key_id.clone()).unwrap();

        assert_eq!(evidence.schema, POLICY_SIGNATURE_SCHEMA_ID);
        assert_eq!(evidence.algorithm, PolicySignatureAlgorithm::HmacSha256);
        assert_eq!(evidence.key_id, key_id);
        assert!(!evidence.signature.is_empty());
        assert!(!evidence.policy_sha256.is_empty());
        // Signature is hex-encoded SHA-256 HMAC → 64 hex chars.
        assert_eq!(evidence.signature.len(), 64);
        assert_eq!(evidence.policy_sha256.len(), 64);
    }

    #[test]
    fn hmac_sha256_rejects_empty_key() {
        let policy = sample_policy();
        let err = sign_policy_snapshot_hmac_sha256(&policy, b"", None).unwrap_err();
        assert!(
            err.to_string().contains("empty"),
            "expected 'empty' in error: {err}"
        );
    }

    #[test]
    fn sign_policy_snapshot_dispatches_hmac_sha256() {
        let policy = sample_policy();
        let key = b"dispatch-key";

        let via_dispatch = sign_policy_snapshot(
            &policy,
            PolicySignatureAlgorithm::HmacSha256,
            key,
            Some("k1".to_string()),
        )
        .unwrap();

        let via_direct =
            sign_policy_snapshot_hmac_sha256(&policy, key, Some("k1".to_string())).unwrap();

        assert_eq!(via_dispatch, via_direct);
    }

    #[test]
    fn hmac_sha256_deterministic() {
        let policy = sample_policy();
        let key = b"determinism-key";

        let sig1 = sign_policy_snapshot_hmac_sha256(&policy, key, None).unwrap();
        let sig2 = sign_policy_snapshot_hmac_sha256(&policy, key, None).unwrap();

        assert_eq!(sig1.signature, sig2.signature);
        assert_eq!(sig1.policy_sha256, sig2.policy_sha256);
    }

    #[test]
    fn different_keys_produce_different_signatures() {
        let policy = sample_policy();
        let sig_a = sign_policy_snapshot_hmac_sha256(&policy, b"key-a", None).unwrap();
        let sig_b = sign_policy_snapshot_hmac_sha256(&policy, b"key-b", None).unwrap();

        // Same policy → same policy_sha256.
        assert_eq!(sig_a.policy_sha256, sig_b.policy_sha256);
        // Different keys → different signatures.
        assert_ne!(sig_a.signature, sig_b.signature);
    }

    // -- Edge case: empty policy (no sensors, no sections) --

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

    #[test]
    fn sign_empty_policy_succeeds() {
        let policy = empty_policy();
        let evidence = sign_policy_snapshot_hmac_sha256(&policy, b"key", None).unwrap();
        assert_eq!(evidence.signature.len(), 64);
        assert_eq!(evidence.policy_sha256.len(), 64);
    }

    #[test]
    fn empty_policy_canonical_bytes_are_valid_compact_json() {
        let policy = empty_policy();
        let bytes = canonical_policy_snapshot_bytes(&policy).unwrap();
        let json_str = std::str::from_utf8(&bytes).unwrap();
        // Compact JSON has no newlines or indentation.
        assert!(!json_str.contains('\n'));
        // Round-trip parse.
        let _: serde_json::Value = serde_json::from_str(json_str).unwrap();
    }

    #[test]
    fn empty_policy_sha256_is_deterministic() {
        let h1 = policy_snapshot_sha256_hex(&empty_policy()).unwrap();
        let h2 = policy_snapshot_sha256_hex(&empty_policy()).unwrap();
        assert_eq!(h1, h2);
    }

    // -- Edge case: minimal policy (single sensor, defaults) --

    fn minimal_policy() -> PolicySnapshot {
        PolicySnapshot {
            warn_is_fail: true,
            max_highlights: 1,
            max_per_sensor_findings: 1,
            max_annotations: 0,
            section_order: vec![],
            sensors: vec![PolicySensorSnapshot {
                id: "s".to_string(),
                blocking: false,
                missing: MissingPolicy::Skip,
                section: None,
                require_label: None,
                repro: None,
            }],
        }
    }

    #[test]
    fn minimal_policy_signs_correctly() {
        let evidence = sign_policy_snapshot_hmac_sha256(&minimal_policy(), b"k", None).unwrap();
        assert_eq!(evidence.algorithm, PolicySignatureAlgorithm::HmacSha256);
        assert_eq!(evidence.signature.len(), 64);
    }

    // -- Edge case: large policy (many sensors) --

    #[test]
    fn large_policy_signs_correctly() {
        let sensors: Vec<PolicySensorSnapshot> = (0..200)
            .map(|i| PolicySensorSnapshot {
                id: format!("sensor-{i:04}"),
                blocking: i % 2 == 0,
                missing: MissingPolicy::Warn,
                section: Some("sec".to_string()),
                require_label: None,
                repro: Some(format!("repro-{i}")),
            })
            .collect();
        let policy = PolicySnapshot {
            warn_is_fail: true,
            max_highlights: 100,
            max_per_sensor_findings: 500,
            max_annotations: 50,
            section_order: vec!["sec".to_string()],
            sensors,
        };
        let evidence = sign_policy_snapshot_hmac_sha256(&policy, b"big-key", None).unwrap();
        assert_eq!(evidence.signature.len(), 64);
        // Deterministic: re-sign produces the same result.
        let evidence2 = sign_policy_snapshot_hmac_sha256(&policy, b"big-key", None).unwrap();
        assert_eq!(evidence.signature, evidence2.signature);
    }

    // -- Tampered data detection --

    #[test]
    fn tampered_policy_produces_different_signature() {
        let key = b"tamper-key";
        let mut policy = sample_policy();
        let original = sign_policy_snapshot_hmac_sha256(&policy, key, None).unwrap();

        // Flip a boolean field.
        policy.warn_is_fail = !policy.warn_is_fail;
        let tampered = sign_policy_snapshot_hmac_sha256(&policy, key, None).unwrap();

        assert_ne!(original.signature, tampered.signature);
        assert_ne!(original.policy_sha256, tampered.policy_sha256);
    }

    #[test]
    fn tampered_sensor_id_changes_signature() {
        let key = b"sensor-tamper";
        let policy = sample_policy();
        let original = sign_policy_snapshot_hmac_sha256(&policy, key, None).unwrap();

        let mut tampered_policy = sample_policy();
        tampered_policy.sensors[0].id = "rustfmt".to_string();
        let tampered = sign_policy_snapshot_hmac_sha256(&tampered_policy, key, None).unwrap();

        assert_ne!(original.signature, tampered.signature);
    }

    // -- HMAC-SHA256 correctness: RFC 4231 test vector 2 --

    #[test]
    fn hmac_sha256_rfc4231_test_case_2() {
        // Key = "Jefe", Data = "what do ya want for nothing?"
        let key = b"Jefe";
        let data = b"what do ya want for nothing?";
        let expected = "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843";
        let result = hex::encode(hmac_sha256(key, data));
        assert_eq!(result, expected);
    }

    // -- Key format handling: key > 64 bytes triggers pre-hashing --

    #[test]
    fn long_key_produces_valid_signature() {
        let policy = sample_policy();
        let long_key = vec![0xABu8; 128]; // longer than SHA-256 block size (64)
        let evidence = sign_policy_snapshot_hmac_sha256(&policy, &long_key, None).unwrap();
        assert_eq!(evidence.signature.len(), 64);
    }

    #[test]
    fn long_key_is_deterministic() {
        let policy = sample_policy();
        let long_key = vec![0xCDu8; 100];
        let sig1 = sign_policy_snapshot_hmac_sha256(&policy, &long_key, None).unwrap();
        let sig2 = sign_policy_snapshot_hmac_sha256(&policy, &long_key, None).unwrap();
        assert_eq!(sig1.signature, sig2.signature);
    }

    #[test]
    fn exact_block_size_key_works() {
        let policy = sample_policy();
        let key = vec![0x42u8; 64]; // exactly SHA-256 block size
        let evidence = sign_policy_snapshot_hmac_sha256(&policy, &key, None).unwrap();
        assert_eq!(evidence.signature.len(), 64);
    }

    #[test]
    fn short_vs_long_key_differ() {
        let policy = sample_policy();
        let short = vec![0xAAu8; 32];
        let long = vec![0xAAu8; 128];
        let sig_short = sign_policy_snapshot_hmac_sha256(&policy, &short, None).unwrap();
        let sig_long = sign_policy_snapshot_hmac_sha256(&policy, &long, None).unwrap();
        assert_ne!(sig_short.signature, sig_long.signature);
    }

    // -- Single-byte key (minimum valid key) --

    #[test]
    fn single_byte_key_works() {
        let policy = sample_policy();
        let evidence = sign_policy_snapshot_hmac_sha256(&policy, &[0x01], None).unwrap();
        assert_eq!(evidence.signature.len(), 64);
    }

    // -- key_id propagation --

    #[test]
    fn key_id_none_propagated() {
        let evidence = sign_policy_snapshot_hmac_sha256(&sample_policy(), b"k", None).unwrap();
        assert_eq!(evidence.key_id, None);
    }

    #[test]
    fn key_id_some_propagated() {
        let kid = Some("prod/hmac/v2".to_string());
        let evidence =
            sign_policy_snapshot_hmac_sha256(&sample_policy(), b"k", kid.clone()).unwrap();
        assert_eq!(evidence.key_id, kid);
    }

    #[test]
    fn key_id_does_not_affect_signature() {
        let policy = sample_policy();
        let key = b"same-key";
        let sig_none = sign_policy_snapshot_hmac_sha256(&policy, key, None).unwrap();
        let sig_some =
            sign_policy_snapshot_hmac_sha256(&policy, key, Some("my-id".to_string())).unwrap();
        assert_eq!(sig_none.signature, sig_some.signature);
        assert_eq!(sig_none.policy_sha256, sig_some.policy_sha256);
    }

    // -- Schema ID is always correct --

    #[test]
    fn schema_id_matches_constant() {
        let evidence = sign_policy_snapshot_hmac_sha256(&sample_policy(), b"k", None).unwrap();
        assert_eq!(evidence.schema, "cockpit.policy_signature.v1");
    }

    // -- Different policies produce different hashes --

    #[test]
    fn different_policies_different_sha256() {
        let h1 = policy_snapshot_sha256_hex(&sample_policy()).unwrap();
        let h2 = policy_snapshot_sha256_hex(&empty_policy()).unwrap();
        assert_ne!(h1, h2);
    }

    #[test]
    fn different_policies_different_signatures_same_key() {
        let key = b"shared";
        let sig1 = sign_policy_snapshot_hmac_sha256(&sample_policy(), key, None).unwrap();
        let sig2 = sign_policy_snapshot_hmac_sha256(&empty_policy(), key, None).unwrap();
        assert_ne!(sig1.signature, sig2.signature);
    }

    // -- policy_sha256 in evidence matches standalone sha256_hex --

    #[test]
    fn evidence_sha256_matches_standalone_digest() {
        let policy = sample_policy();
        let evidence = sign_policy_snapshot_hmac_sha256(&policy, b"key", None).unwrap();
        let standalone = policy_snapshot_sha256_hex(&policy).unwrap();
        assert_eq!(evidence.policy_sha256, standalone);
    }

    // -- Canonical bytes are compact (no whitespace formatting) --

    #[test]
    fn canonical_bytes_are_compact_json() {
        let policy = sample_policy();
        let bytes = canonical_policy_snapshot_bytes(&policy).unwrap();
        let s = std::str::from_utf8(&bytes).unwrap();
        // serde_json::to_vec produces compact JSON — no indentation.
        assert!(!s.contains("  "), "canonical JSON should be compact");
    }

    // -- Raw HMAC-SHA256: empty message --

    #[test]
    fn hmac_sha256_empty_message() {
        let result = hmac_sha256(b"key", b"");
        // Must produce a valid 32-byte MAC, not panic.
        assert_eq!(result.len(), 32);
        // Deterministic.
        assert_eq!(result, hmac_sha256(b"key", b""));
    }

    // -- Raw HMAC-SHA256: RFC 4231 test case 1 --

    #[test]
    fn hmac_sha256_rfc4231_test_case_1() {
        // Key = 20 bytes of 0x0b, Data = "Hi There"
        let key = vec![0x0bu8; 20];
        let data = b"Hi There";
        let expected = "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7";
        let result = hex::encode(hmac_sha256(&key, data));
        assert_eq!(result, expected);
    }

    // -- Raw HMAC-SHA256: RFC 4231 test case 3 --

    #[test]
    fn hmac_sha256_rfc4231_test_case_3() {
        // Key = 0xaa repeated 20 times, Data = 0xdd repeated 50 times
        let key = vec![0xAAu8; 20];
        let data = vec![0xDDu8; 50];
        let expected = "773ea91e36800e46854db8ebd09181a72959098b3ef8c122d9635514ced565fe";
        let result = hex::encode(hmac_sha256(&key, &data));
        assert_eq!(result, expected);
    }

    // -- HMAC key boundary: 63 bytes (just below block size) --

    #[test]
    fn hmac_sha256_key_boundary_63_bytes() {
        let key = vec![0xBBu8; 63];
        let result = hmac_sha256(&key, b"test data");
        assert_eq!(result.len(), 32);
        // Deterministic.
        assert_eq!(result, hmac_sha256(&key, b"test data"));
    }

    // -- HMAC key boundary: 65 bytes (just above block size) --

    #[test]
    fn hmac_sha256_key_boundary_65_bytes() {
        let key = vec![0xCCu8; 65];
        let result = hmac_sha256(&key, b"test data");
        assert_eq!(result.len(), 32);
        // Differs from 63-byte key.
        let key_63 = vec![0xCCu8; 63];
        assert_ne!(result, hmac_sha256(&key_63, b"test data"));
    }

    // -- HMAC with key containing null bytes --

    #[test]
    fn hmac_sha256_key_with_null_bytes() {
        let key = b"\x00\x00\x01\x00\x00";
        let result = hmac_sha256(key, b"message");
        assert_eq!(result.len(), 32);
        // Null bytes in key should still produce a valid distinct MAC.
        let key_no_null = b"\x01\x01\x01\x01\x01";
        assert_ne!(result, hmac_sha256(key_no_null, b"message"));
    }

    // -- HMAC with very large key (1024 bytes) --

    #[test]
    fn hmac_sha256_very_large_key() {
        let key = vec![0xEEu8; 1024];
        let result = hmac_sha256(&key, b"payload");
        assert_eq!(result.len(), 32);
        // Deterministic.
        assert_eq!(result, hmac_sha256(&key, b"payload"));
    }
}

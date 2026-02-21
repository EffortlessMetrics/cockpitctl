//! Policy-signing domain boundary microcrate.

use anyhow::{Context, Result};
use cockpitctl_types::{
    POLICY_SIGNATURE_SCHEMA_ID, PolicySignatureAlgorithm, PolicySignatureEvidence, PolicySnapshot,
};
use sha2::{Digest, Sha256};

/// Canonical policy snapshot bytes used for hashing/signing.
///
/// Canonicalization is the compact serde JSON encoding of `PolicySnapshot`.
/// Determinism relies on stable field ordering and pre-sorted vectors.
pub fn canonical_policy_snapshot_bytes(policy: &PolicySnapshot) -> Result<Vec<u8>> {
    serde_json::to_vec(policy).context("serialize policy snapshot for signing")
}

/// Compute SHA-256 digest (hex) of the canonical policy snapshot bytes.
pub fn policy_snapshot_sha256_hex(policy: &PolicySnapshot) -> Result<String> {
    let payload = canonical_policy_snapshot_bytes(policy)?;
    Ok(hex::encode(Sha256::digest(payload)))
}

/// Sign the policy snapshot with the configured algorithm.
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
}

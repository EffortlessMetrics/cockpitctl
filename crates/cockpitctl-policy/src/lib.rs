//! Policy snapshot canonicalization and signing.

use anyhow::Context;
use cockpitctl_types::{
    POLICY_SIGNATURE_SCHEMA_ID, PolicySignatureAlgorithm, PolicySignatureEvidence, PolicySnapshot,
};
use sha2::{Digest, Sha256};

/// Canonical policy snapshot bytes used for hashing/signing.
///
/// Canonicalization is the compact serde JSON encoding of `PolicySnapshot`.
/// Determinism relies on stable field ordering and pre-sorted vectors.
pub fn canonical_policy_snapshot_bytes(policy: &PolicySnapshot) -> Result<Vec<u8>, anyhow::Error> {
    serde_json::to_vec(policy).context("serialize policy snapshot for signing")
}

/// Compute SHA-256 digest (hex) of the canonical policy snapshot bytes.
pub fn policy_snapshot_sha256_hex(policy: &PolicySnapshot) -> Result<String, anyhow::Error> {
    let payload = canonical_policy_snapshot_bytes(policy)?;
    Ok(hex::encode(Sha256::digest(payload)))
}

/// Sign the policy snapshot with the configured algorithm.
pub fn sign_policy_snapshot(
    policy: &PolicySnapshot,
    algorithm: PolicySignatureAlgorithm,
    key: &[u8],
    key_id: Option<String>,
) -> Result<PolicySignatureEvidence, anyhow::Error> {
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
) -> Result<PolicySignatureEvidence, anyhow::Error> {
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

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
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

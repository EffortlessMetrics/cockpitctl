use cockpitctl_domain::snapshot_policy;
use cockpitctl_policy::{
    policy_snapshot_sha256_hex, sign_policy_snapshot, sign_policy_snapshot_hmac_sha256,
};
use cockpitctl_types::{CockpitConfig, PolicySignatureAlgorithm};

fn sample_policy_snapshot() -> cockpitctl_types::PolicySnapshot {
    let mut cfg = CockpitConfig::default();
    cfg.policy.warn_is_fail = true;
    cfg.policy.max_highlights = 11;
    cfg.sensors.insert(
        "builddiag".to_string(),
        cockpitctl_types::SensorPolicy {
            blocking: true,
            missing: cockpitctl_types::MissingPolicy::Fail,
            section: Some("Repo contract".to_string()),
            require_label: None,
            repro: Some("builddiag check".to_string()),
        },
    );
    snapshot_policy(&cfg)
}

#[test]
fn policy_signature_is_deterministic_for_same_key_and_payload() {
    let policy = sample_policy_snapshot();
    let key = b"test-signing-key";

    let a = sign_policy_snapshot(
        &policy,
        PolicySignatureAlgorithm::HmacSha256,
        key,
        Some("ci-key".to_string()),
    )
    .expect("sign policy a");
    let b = sign_policy_snapshot_hmac_sha256(&policy, key, Some("ci-key".to_string()))
        .expect("sign policy b");

    assert_eq!(a.policy_sha256, b.policy_sha256);
    assert_eq!(a.signature, b.signature);
}

#[test]
fn policy_signature_changes_when_key_changes() {
    let policy = sample_policy_snapshot();
    let a = sign_policy_snapshot_hmac_sha256(&policy, b"key-a", None).expect("sign policy a");
    let b = sign_policy_snapshot_hmac_sha256(&policy, b"key-b", None).expect("sign policy b");
    assert_ne!(a.signature, b.signature);
}

#[test]
fn policy_signature_rejects_empty_key() {
    let policy = sample_policy_snapshot();
    let err = sign_policy_snapshot_hmac_sha256(&policy, b"", None).expect_err("empty key");
    assert!(format!("{:#}", err).contains("policy signing key is empty"));
}

#[test]
fn policy_sha256_matches_signing_payload_digest() {
    let policy = sample_policy_snapshot();
    let digest = policy_snapshot_sha256_hex(&policy).expect("digest");
    let sig =
        sign_policy_snapshot_hmac_sha256(&policy, b"shared-secret", None).expect("sign policy");
    assert_eq!(digest, sig.policy_sha256);
}

use cockpitctl_domain_signing::{
    canonical_policy_snapshot_bytes, policy_snapshot_sha256_hex, sign_policy_snapshot,
    sign_policy_snapshot_hmac_sha256,
};
use cockpitctl_types::{
    MissingPolicy, PolicySensorSnapshot, PolicySignatureAlgorithm, PolicySnapshot,
};

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
        max_highlights: 10,
        max_per_sensor_findings: 100,
        max_annotations: 20,
        section_order: vec!["build".to_string(), "lint".to_string(), "test".to_string()],
        sensors: vec![
            PolicySensorSnapshot {
                id: "builddiag".to_string(),
                blocking: true,
                missing: MissingPolicy::Fail,
                section: Some("build".to_string()),
                require_label: None,
                repro: Some("cargo build".to_string()),
            },
            PolicySensorSnapshot {
                id: "clippy".to_string(),
                blocking: true,
                missing: MissingPolicy::Warn,
                section: Some("lint".to_string()),
                require_label: None,
                repro: Some("cargo clippy".to_string()),
            },
            PolicySensorSnapshot {
                id: "nextest".to_string(),
                blocking: false,
                missing: MissingPolicy::Skip,
                section: Some("test".to_string()),
                require_label: None,
                repro: None,
            },
        ],
    }
}

// ---------------------------------------------------------------------------
// Canonical bytes snapshots
// ---------------------------------------------------------------------------

#[test]
fn snapshot_canonical_bytes_sample_policy() {
    let bytes = canonical_policy_snapshot_bytes(&sample_policy()).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    insta::assert_json_snapshot!("canonical_bytes_sample_policy", json);
}

#[test]
fn snapshot_canonical_bytes_empty_policy() {
    let bytes = canonical_policy_snapshot_bytes(&empty_policy()).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    insta::assert_json_snapshot!("canonical_bytes_empty_policy", json);
}

// ---------------------------------------------------------------------------
// SHA-256 digest snapshots (determinism regression)
// ---------------------------------------------------------------------------

#[test]
fn snapshot_sha256_sample_policy() {
    let hex = policy_snapshot_sha256_hex(&sample_policy()).unwrap();
    insta::assert_snapshot!("sha256_sample_policy", hex);
}

#[test]
fn snapshot_sha256_empty_policy() {
    let hex = policy_snapshot_sha256_hex(&empty_policy()).unwrap();
    insta::assert_snapshot!("sha256_empty_policy", hex);
}

// ---------------------------------------------------------------------------
// Signed policy evidence snapshots
// ---------------------------------------------------------------------------

#[test]
fn snapshot_signed_policy_hmac_sha256() {
    let evidence =
        sign_policy_snapshot_hmac_sha256(&sample_policy(), b"test-key", Some("key-v1".into()))
            .unwrap();
    insta::assert_json_snapshot!("signed_policy_hmac_sha256", evidence);
}

#[test]
fn snapshot_signed_policy_no_key_id() {
    let evidence = sign_policy_snapshot_hmac_sha256(&sample_policy(), b"test-key", None).unwrap();
    insta::assert_json_snapshot!("signed_policy_no_key_id", evidence);
}

#[test]
fn snapshot_signed_policy_different_key() {
    let evidence =
        sign_policy_snapshot_hmac_sha256(&sample_policy(), b"alternate-key", Some("key-v2".into()))
            .unwrap();
    insta::assert_json_snapshot!("signed_policy_different_key", evidence);
}

#[test]
fn snapshot_signed_policy_via_dispatch() {
    let evidence = sign_policy_snapshot(
        &sample_policy(),
        PolicySignatureAlgorithm::HmacSha256,
        b"dispatch-key",
        Some("dispatch-v1".into()),
    )
    .unwrap();
    insta::assert_json_snapshot!("signed_policy_via_dispatch", evidence);
}

#[test]
fn snapshot_signed_multi_sensor_policy() {
    let evidence =
        sign_policy_snapshot_hmac_sha256(&multi_sensor_policy(), b"multi-key", Some("mk-1".into()))
            .unwrap();
    insta::assert_json_snapshot!("signed_multi_sensor_policy", evidence);
}

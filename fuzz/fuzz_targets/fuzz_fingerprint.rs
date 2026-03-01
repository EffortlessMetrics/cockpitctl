//! Fuzz target for fingerprint derivation and finding sort logic.
//!
//! Exercises `derive_fingerprint`, `finding_sort_key`, `sort_findings`,
//! `cap_findings`, and `compute_counts` with arbitrary `Finding` JSON.
//!
//! Run with: cargo +nightly fuzz run fuzz_fingerprint

#![no_main]

use libfuzzer_sys::fuzz_target;
use cockpitctl_domain::{
    cap_findings, compute_counts, derive_fingerprint, finding_sort_key, sort_findings,
};
use cockpitctl_types::Finding;

fuzz_target!(|data: &[u8]| {
    // Try to parse as a single Finding.
    if let Ok(finding) = serde_json::from_slice::<Finding>(data) {
        // Fingerprint derivation must never panic.
        let fp = derive_fingerprint("fuzz-sensor", &finding);
        assert!(!fp.is_empty());

        // Sort key derivation must never panic.
        let _ = finding_sort_key("fuzz-sensor", &finding);

        // Counts must never panic.
        let _ = compute_counts(std::slice::from_ref(&finding));
    }

    // Try to parse as a Vec<Finding> for batch operations.
    if let Ok(mut findings) = serde_json::from_slice::<Vec<Finding>>(data) {
        // Sort must never panic.
        sort_findings("fuzz-sensor", &mut findings);

        // Cap must never panic at various limits.
        let _ = cap_findings(findings.clone(), 0);
        let _ = cap_findings(findings.clone(), 1);
        let _ = cap_findings(findings.clone(), 100);
        let _ = cap_findings(findings, usize::MAX);
    }
});

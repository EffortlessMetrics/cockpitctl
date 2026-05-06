//! Security-focused conformance tests for cockpitctl-conform.
//!
//! These tests verify that the conformance checker correctly handles
//! adversarial content in receipts: script injection, markdown injection,
//! path traversal in findings, resource exhaustion via huge arrays,
//! and undefined JSON behaviors like duplicate keys.

use cockpitctl_conform::{ConformChecks, conform_single};

/// All conformance checks enabled.
fn all_checks() -> ConformChecks {
    ConformChecks {
        path_hygiene: true,
        ordering: true,
        reason_lint: true,
        survivability: true,
        tool_error_identity: true,
        sensor_id_format: true,
        artifact_pointers: true,
    }
}

/// Build a minimal valid receipt JSON with custom findings.
fn receipt_with_findings(findings: &[serde_json::Value]) -> String {
    serde_json::json!({
        "schema": "sensor.report.v1",
        "tool": { "name": "security-test", "version": "0.1.0" },
        "run": { "started_at": "2025-01-01T00:00:00Z" },
        "verdict": {
            "status": "pass",
            "counts": { "info": 0, "warn": 0, "error": 0 }
        },
        "findings": findings
    })
    .to_string()
}

/// Build a minimal valid receipt JSON.
#[expect(
    dead_code,
    reason = "Security fixture helper is retained for targeted receipt cases."
)]
fn minimal_receipt() -> String {
    receipt_with_findings(&[])
}

// ═══════════════════════════════════════════════════════════════════════════
// SCRIPT INJECTION IN FINDING MESSAGES
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn script_injection_in_finding_message_does_not_crash() {
    let findings = vec![serde_json::json!({
        "severity": "error",
        "code": "xss-test",
        "message": "<script>alert('xss')</script>",
        "location": { "path": "src/main.rs", "line": 1 }
    })];
    let content = receipt_with_findings(&findings);
    let result = conform_single(&content, "test-sensor", &all_checks()).unwrap();
    // Should parse and validate without crashing — the message is just a string
    // Conformance doesn't strip HTML, but it must not fail on it
    assert!(
        result.violations.is_empty()
            || result
                .violations
                .iter()
                .all(|v| v.check != "script_injection"),
        "script tags in messages should not cause a special violation type"
    );
}

#[test]
fn script_injection_in_finding_code_handled() {
    let findings = vec![serde_json::json!({
        "severity": "warn",
        "code": "<img src=x onerror=alert(1)>",
        "message": "test",
        "location": { "path": "src/lib.rs", "line": 5 }
    })];
    let content = receipt_with_findings(&findings);
    // Should not panic on HTML-like code fields
    let result = conform_single(&content, "test-sensor", &all_checks());
    assert!(
        result.is_ok(),
        "HTML in code field should not cause a panic"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// MARKDOWN INJECTION IN MESSAGES
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn markdown_injection_in_finding_message_handled() {
    let findings = vec![serde_json::json!({
        "severity": "info",
        "code": "md-inject",
        "message": "[Click me](javascript:alert(document.cookie)) and ![img](https://evil.com/track.gif)",
        "location": { "path": "src/main.rs", "line": 1 }
    })];
    let content = receipt_with_findings(&findings);
    let result = conform_single(&content, "test-sensor", &all_checks()).unwrap();
    // Conformance should process this without crashing
    // The message is opaque text — conformance doesn't interpret markdown
    let _ = result;
}

#[test]
fn markdown_table_breaking_injection_handled() {
    let findings = vec![serde_json::json!({
        "severity": "error",
        "code": "table-break",
        "message": "| injected | column |\n|---|---|\n| data | here |",
        "location": { "path": "src/main.rs", "line": 1 }
    })];
    let content = receipt_with_findings(&findings);
    let result = conform_single(&content, "test-sensor", &all_checks());
    assert!(
        result.is_ok(),
        "markdown table injection should not crash conformance"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// PATH TRAVERSAL IN FINDINGS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn path_traversal_in_finding_location_flagged() {
    let findings = vec![serde_json::json!({
        "severity": "error",
        "code": "traversal-test",
        "message": "path traversal attempt",
        "location": { "path": "../../../etc/passwd", "line": 1 }
    })];
    let content = receipt_with_findings(&findings);
    let result = conform_single(&content, "test-sensor", &all_checks()).unwrap();

    // Path hygiene check should flag the traversal attempt
    let has_path_violation = result.violations.iter().any(|v| v.check == "path_hygiene");
    assert!(
        has_path_violation,
        "path traversal in finding location should be flagged by path_hygiene. violations: {:?}",
        result
            .violations
            .iter()
            .map(|v| format!("{}:{}", v.check, v.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn absolute_path_in_finding_location_flagged() {
    let findings = vec![serde_json::json!({
        "severity": "error",
        "code": "abs-path-test",
        "message": "absolute path attempt",
        "location": { "path": "/etc/shadow", "line": 1 }
    })];
    let content = receipt_with_findings(&findings);
    let result = conform_single(&content, "test-sensor", &all_checks()).unwrap();

    let has_path_violation = result.violations.iter().any(|v| v.check == "path_hygiene");
    assert!(
        has_path_violation,
        "absolute path in finding should be flagged. violations: {:?}",
        result
            .violations
            .iter()
            .map(|v| format!("{}:{}", v.check, v.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn windows_drive_path_in_finding_flagged() {
    let findings = vec![serde_json::json!({
        "severity": "warn",
        "code": "win-path",
        "message": "windows path attempt",
        "location": { "path": "C:\\Windows\\System32\\config\\sam", "line": 1 }
    })];
    let content = receipt_with_findings(&findings);
    let result = conform_single(&content, "test-sensor", &all_checks()).unwrap();

    let has_path_violation = result.violations.iter().any(|v| v.check == "path_hygiene");
    assert!(
        has_path_violation,
        "Windows drive path should be flagged. violations: {:?}",
        result
            .violations
            .iter()
            .map(|v| format!("{}:{}", v.check, v.message))
            .collect::<Vec<_>>()
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// VERY LARGE ARRAYS (RESOURCE EXHAUSTION)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn receipt_with_100k_findings_does_not_oom() {
    // Build a receipt with 100,000 findings — should be processable without OOM
    let mut findings = Vec::with_capacity(100_000);
    for i in 0..100_000 {
        findings.push(serde_json::json!({
            "severity": "info",
            "code": format!("code-{}", i),
            "message": format!("finding {}", i),
            "location": { "path": "src/main.rs", "line": i + 1 }
        }));
    }
    let content = receipt_with_findings(&findings);

    // This should complete without OOM — conformance processes them all
    let result = conform_single(&content, "test-sensor", &all_checks());
    assert!(result.is_ok(), "100K findings should not cause OOM");
}

// ═══════════════════════════════════════════════════════════════════════════
// DUPLICATE JSON KEYS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn receipt_with_duplicate_keys_has_defined_behavior() {
    // JSON with duplicate keys — serde_json uses last-value-wins semantics
    let content = r#"{
        "schema": "sensor.report.v1",
        "tool": { "name": "dup-test", "version": "1.0.0" },
        "run": { "started_at": "2025-01-01T00:00:00Z" },
        "verdict": {
            "status": "pass",
            "status": "fail",
            "counts": { "info": 0, "warn": 0, "error": 0 }
        },
        "findings": []
    }"#;

    // Should not panic — the behavior (last-value-wins) is defined by serde_json
    let result = conform_single(content, "test-sensor", &all_checks());
    assert!(
        result.is_ok(),
        "duplicate keys should not crash conformance"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// DEEPLY NESTED JSON
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn deeply_nested_findings_data_does_not_crash() {
    // Build a receipt where the opaque `data` field is deeply nested
    let depth = 200;
    let mut nested = String::new();
    for _ in 0..depth {
        nested.push_str("{\"a\":");
    }
    nested.push_str("null");
    for _ in 0..depth {
        nested.push('}');
    }

    let content = format!(
        r#"{{
            "schema": "sensor.report.v1",
            "tool": {{ "name": "deep-test", "version": "1.0.0" }},
            "run": {{ "started_at": "2025-01-01T00:00:00Z" }},
            "verdict": {{
                "status": "pass",
                "counts": {{ "info": 0, "warn": 0, "error": 0 }}
            }},
            "findings": [],
            "data": {}
        }}"#,
        nested
    );

    // serde_json may reject deeply nested JSON with a recursion limit error.
    // Both Ok (with violations) and Err (parse failure) are acceptable — the
    // important thing is no stack overflow or panic.
    let result = conform_single(&content, "test-sensor", &all_checks());
    if let Ok(r) = result {
        assert!(!r.is_pass(), "deeply nested data should not pass cleanly");
    }
    // Err (parse failure) is also acceptable — no stack overflow or panic
}

// ═══════════════════════════════════════════════════════════════════════════
// INVALID / EMPTY INPUT
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn empty_content_handled_gracefully() {
    let result = conform_single("", "test-sensor", &all_checks());
    // Should return an error or violations, not crash
    if let Ok(r) = result {
        assert!(!r.is_pass(), "empty content should not pass conformance");
    }
}

#[test]
fn null_json_handled_gracefully() {
    let result = conform_single("null", "test-sensor", &all_checks());
    if let Ok(r) = result {
        assert!(!r.is_pass(), "null JSON should not pass conformance");
    }
}

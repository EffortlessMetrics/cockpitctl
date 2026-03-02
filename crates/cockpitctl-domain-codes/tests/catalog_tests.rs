use cockpitctl_domain_codes::{all_codes, explain_code};

#[test]
fn explain_code_finds_known_code() {
    let explanation = explain_code("cockpit.missing_receipt").expect("known code");
    assert_eq!(explanation.title, "Missing Receipt");
}

#[test]
fn all_codes_are_unique() {
    let codes = all_codes();
    let mut seen = std::collections::BTreeSet::new();
    for code in &codes {
        assert!(seen.insert(code.code), "duplicate code: {}", code.code);
    }
}

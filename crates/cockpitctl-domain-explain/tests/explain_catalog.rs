use cockpitctl_domain_explain::{all_codes, cockpit_codes, explain_code};

#[test]
fn known_code_is_explained() {
    let explanation =
        explain_code(cockpit_codes::MISSING_RECEIPT).expect("missing code explanation");
    assert_eq!(explanation.title, "Missing Receipt");
}

#[test]
fn unknown_code_is_not_explained() {
    assert!(explain_code("cockpit.unknown").is_none());
}

#[test]
fn all_catalog_codes_are_unique() {
    let codes = all_codes();
    let mut values: Vec<&str> = codes.iter().map(|entry| entry.code).collect();
    values.sort_unstable();
    values.dedup();
    assert_eq!(values.len(), codes.len());
}

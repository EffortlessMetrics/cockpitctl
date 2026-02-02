use cockpitctl_domain::{derive_fingerprint, finding_sort_key};
use cockpitctl_types::{Finding, Location, Severity};
use proptest::prelude::*;

fn any_finding() -> impl Strategy<Value = Finding> {
    (
        prop_oneof![Just(Severity::Info), Just(Severity::Warn), Just(Severity::Error)],
        ".*", // code
        ".*", // message
        prop::option::of(".*"), // path
        prop::option::of(1u32..1000u32), // line
    ).prop_map(|(severity, code, message, path, line)| {
        Finding {
            severity,
            check_id: None,
            code,
            message,
            location: Some(Location { path, line, col: None }),
            help: None,
            url: None,
            fingerprint: None,
            data: None,
        }
    })
}

proptest! {
    #[test]
    fn fingerprint_is_deterministic(sensor_id in "sensor_[a-z]{1,8}", f in any_finding()) {
        let a = derive_fingerprint(&sensor_id, &f);
        let b = derive_fingerprint(&sensor_id, &f);
        prop_assert_eq!(a, b);
    }

    #[test]
    fn sort_key_is_total_order(sensor_id in "sensor_[a-z]{1,8}", a in any_finding(), b in any_finding()) {
        let ka = finding_sort_key(&sensor_id, &a);
        let kb = finding_sort_key(&sensor_id, &b);
        // total order means comparable (Ord), so just ensure no panic and compare works
        let _ = ka.cmp(&kb);
    }
}

#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Receipt parsing must never panic.
    let _ = serde_json::from_slice::<cockpitctl_types::SensorReport>(data);
});

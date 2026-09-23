//! Row 14.15: arbitrary document tree → typed builder → XHTML/OPF → re-parse and Tier 1, no repair.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let _ = oc_testkit::fuzz_props::xhtml_opf_roundtrip(data);
});

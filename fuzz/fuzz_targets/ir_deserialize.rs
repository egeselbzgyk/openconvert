//! Row 14.13: arbitrary bytes → the IR's semantic layer → canonical JSON, a fixed point.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    oc_testkit::fuzz_props::ir_deserialize(data);
});

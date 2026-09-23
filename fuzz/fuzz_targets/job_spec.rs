//! Row 14.14: arbitrary bytes → job-spec validation; every accepted spec names absolute,
//! non-climbing paths (RT B15).
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    oc_testkit::fuzz_props::job_spec(data);
});

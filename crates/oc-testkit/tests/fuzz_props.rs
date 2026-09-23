//! PHASE 14 rows 14.13–14.15, in the ordinary suite: the three fuzz properties over their committed
//! seed corpora (`fuzz/corpus/<target>/`) and a few hundred generated inputs each. The nightly
//! `fuzz` job runs the same properties under libFuzzer for fifteen minutes a target; these keep
//! them compiling and passing between nights.

use std::path::PathBuf;

use oc_testkit::fuzz_props;
use proptest::prelude::*;

fn seeds(target: &str) -> Vec<Vec<u8>> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fuzz/corpus")
        .join(target);
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("{}: {error}", dir.display()))
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_file())
        .collect();
    paths.sort();
    let seeds: Vec<Vec<u8>> = paths
        .iter()
        .filter_map(|path| std::fs::read(path).ok())
        .collect();
    assert!(!seeds.is_empty(), "no seeds in {}", dir.display());
    seeds
}

/// Row 14.13.
#[test]
fn fuzz_ir_deserialize_no_panic() {
    let mut read = 0;
    for seed in seeds("ir_deserialize") {
        fuzz_props::ir_deserialize(&seed);
        read += usize::from(serde_json::from_slice::<fuzz_props::SemanticIr>(&seed).is_ok());
    }
    assert!(
        read >= 3,
        "the fixture seeds are IR the property reads, not noise: {read}"
    );
    proptest!(ProptestConfig::with_cases(256), |(bytes in proptest::collection::vec(any::<u8>(), 0..512))| {
        fuzz_props::ir_deserialize(&bytes);
    });
}

/// Row 14.14.
#[test]
fn fuzz_job_spec_accepts_only_absolute_paths() {
    for seed in seeds("job_spec") {
        fuzz_props::job_spec(&seed);
    }
    // Specs that are valid but for their paths, with the paths drawn from nasty shapes.
    let path = prop_oneof![
        Just("/in/book.pdf".to_owned()),
        Just("book.pdf".to_owned()),
        Just("../book.pdf".to_owned()),
        Just("/a/../../etc/passwd".to_owned()),
        Just(String::new()),
        Just("./book.pdf".to_owned()),
        "[a-z./]{0,12}",
    ];
    proptest!(ProptestConfig::with_cases(256), |(input in path.clone(), output in path)| {
        let spec = serde_json::json!({
            "schema": "openconvert.job/1",
            "input": {"path": input},
            "output": {"path": output},
        });
        fuzz_props::job_spec(spec.to_string().as_bytes());
    });
}

/// Row 14.15.
#[test]
fn fuzz_xhtml_opf_roundtrip_fires_no_repair() {
    for seed in seeds("xhtml_opf_roundtrip") {
        fuzz_props::xhtml_opf_roundtrip(&seed);
    }
    let emitted = std::sync::atomic::AtomicUsize::new(0);
    proptest!(ProptestConfig::with_cases(128), |(bytes in proptest::collection::vec(any::<u8>(), 0..2048))| {
        if fuzz_props::xhtml_opf_roundtrip(&bytes) {
            emitted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    });
    // Not vacuous: most generated books are emitted, so the parse and Tier-1 checks ran on them.
    assert!(
        emitted.into_inner() > 32,
        "too few inputs produced a container"
    );
}

#![forbid(unsafe_code)]
//! Developer tasks (`cargo run -p xtask -- <task>`).
//!
//! Everything here is build- and test-time tooling. Nothing in `xtask` ships.

use xtask::{
    ci_lint, dom_fixtures, epubcheck_parity, fetch_epubcheck, fetch_epubcheck_corpus,
    fetch_isartor, fetch_llama_server, fixtures, fuzz_seeds, handmade_fixtures, isolate_parser,
    mutations, stage_sidecars, thresholds_lint, vendor_pdfium,
};

use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

const USAGE: &str = "\
usage: cargo run -p xtask -- <task>

tasks:
  vendor-pdfium     fetch the pinned PDFium binary for this host and unpack it to vendor/
  fixtures          compile the Typst fixture sources to target/fixtures/
                      --keep-structtree   emit the tagged variants instead (D18)
  fetch-epubcheck   fetch the pinned EPUBCheck release and unpack it to vendor/epubcheck/
  fetch-epubcheck-corpus
                    fetch EPUBCheck's own public test corpus to vendor/epubcheck-corpus/
  fetch-isartor     fetch the pinned Isartor suite (xtask/isartor.lock) to target/isartor/
                    for the crash-regression tier (PHASE 14 row 14.16)
  fetch-llama-server
                    fetch the pinned llama.cpp release for this host (xtask/llama.lock),
                    unpack it to vendor/llama-server/ and print llama-server's path
  epubcheck-parity  run Tier 1 over that corpus and write docs/TIER1_PARITY.md
                      --check             compare against the committed number instead of
                                          rewriting it; fails when parity has fallen
  fuzz-seeds        write the fuzz targets' seed corpora to fuzz/corpus/ from the fixtures
  isolate-parser-spike
                    measure --isolate-parser's cost: one child per page range, CBOR over a pipe,
                    against in-process extraction and the whole conversion (PHASE 14 detail 13)
                      --fixture <STEM>    one fixture only
                      --repeats <N>       repetitions per fixture (median reported)
                      --reference-book <PAGES>  the benchmark's synthetic book instead
  handmade-fixtures write the hand-made PDFs to corpus/fixtures/handmade/
  mutations         apply the mutation recipes to the fixtures they belong to and
                    write the results to corpus/fixtures/mutations/
  ci-lint           repository rules: no skipped tests, no unnumbered markers
                      --release-branch    also reject TODO_ placeholders in models.toml
  thresholds-lint   D17 provenance: every provisional threshold has an owner and a
                    review_by that has not passed
  dom-fixtures      convert every fixture and unpack its container to target/dom/ for the
                    Playwright DOM checks, with a manifest of spine, nav order and noterefs
  stage-sidecars    copy the built engine to apps/desktop/src-tauri/bin/ with the
                    target-triple suffix Tauri expects, plus a build stamp
                      --release           stage the release build instead of debug
";

fn main() -> Result<()> {
    let task = std::env::args().nth(1);
    let root = workspace_root()?;

    match task.as_deref() {
        Some("vendor-pdfium") => vendor_pdfium::run(&root),
        Some("fixtures") => {
            let keep_structtree = std::env::args().any(|a| a == "--keep-structtree");
            fixtures::run(&root, keep_structtree)
        }
        Some("fetch-epubcheck") => fetch_epubcheck::run(&root),
        Some("fetch-epubcheck-corpus") => fetch_epubcheck_corpus::run(&root),
        Some("fetch-isartor") => fetch_isartor::run(&root),
        Some("fetch-llama-server") => fetch_llama_server::run(&root),
        Some("epubcheck-parity") => {
            let check = std::env::args().any(|a| a == "--check");
            epubcheck_parity::run(&root, check)
        }
        Some("fuzz-seeds") => fuzz_seeds::run(&root),
        Some("isolate-parser-spike") => {
            let args: Vec<String> = std::env::args().collect();
            let value = |flag: &str| {
                args.windows(2)
                    .find(|pair| pair[0] == flag)
                    .map(|pair| pair[1].clone())
            };
            let repeats = value("--repeats").and_then(|n| n.parse().ok());
            // `--reference-book <PAGES>`: the benchmark's synthetic book (PHASE 7 row 7.11).
            let book = match value("--reference-book").and_then(|n| n.parse::<usize>().ok()) {
                Some(pages) => {
                    let path = root.join(format!("target/tmp/reference_book_{pages}.pdf"));
                    std::fs::create_dir_all(root.join("target/tmp"))?;
                    std::fs::write(&path, oc_testkit::handmade::reference_book(pages))?;
                    Some(path.display().to_string())
                }
                None => value("--fixture"),
            };
            isolate_parser::run(&root, book.as_deref(), repeats)
        }
        Some(isolate_parser::CHILD) => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            let [pdf, first, last] = args.as_slice() else {
                bail!("{} <pdf> <first> <last>", isolate_parser::CHILD);
            };
            isolate_parser::child(Path::new(pdf), first.parse()?, last.parse()?)
        }
        Some("handmade-fixtures") => handmade_fixtures::run(&root),
        Some("mutations") => mutations::run(&root),
        Some("ci-lint") => {
            let release_branch = std::env::args().any(|a| a == "--release-branch");
            ci_lint::run(&root, release_branch)
        }
        Some("thresholds-lint") => thresholds_lint::run(&root),
        Some("dom-fixtures") => dom_fixtures::run(&root),
        Some("stage-sidecars") => {
            let release = std::env::args().any(|a| a == "--release");
            stage_sidecars::run(&root, release)
        }
        Some(other) => {
            eprint!("{USAGE}");
            bail!("unknown task `{other}`")
        }
        None => {
            eprint!("{USAGE}");
            bail!("no task given")
        }
    }
}

/// The workspace root, found from this crate's manifest directory rather than from the
/// current directory, so a task behaves the same wherever it is invoked from.
fn workspace_root() -> Result<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir
        .parent()
        .map(Path::to_path_buf)
        .filter(|p| p.join("Cargo.toml").is_file());
    match root {
        Some(root) => Ok(root),
        None => bail!(
            "cannot locate the workspace root above {}",
            manifest_dir.display()
        ),
    }
}

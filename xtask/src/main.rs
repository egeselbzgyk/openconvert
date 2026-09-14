//! Developer tasks (`cargo run -p xtask -- <task>`).
//!
//! Everything here is build- and test-time tooling. Nothing in `xtask` ships.

use xtask::{
    ci_lint, epubcheck_parity, fetch_epubcheck, fetch_epubcheck_corpus, fixtures,
    handmade_fixtures, mutations, stage_sidecars, thresholds_lint, vendor_pdfium,
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
  epubcheck-parity  run Tier 1 over that corpus and write docs/TIER1_PARITY.md
                      --check             compare against the committed number instead of
                                          rewriting it; fails when parity has fallen
  handmade-fixtures write the hand-made PDFs to corpus/fixtures/handmade/
  mutations         apply the mutation recipes to the fixtures they belong to and
                    write the results to corpus/fixtures/mutations/
  ci-lint           repository rules: no skipped tests, no unnumbered markers
                      --release-branch    also reject TODO_ placeholders in models.toml
  thresholds-lint   D17 provenance: every provisional threshold has an owner and a
                    review_by that has not passed
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
        Some("epubcheck-parity") => {
            let check = std::env::args().any(|a| a == "--check");
            epubcheck_parity::run(&root, check)
        }
        Some("handmade-fixtures") => handmade_fixtures::run(&root),
        Some("mutations") => mutations::run(&root),
        Some("ci-lint") => {
            let release_branch = std::env::args().any(|a| a == "--release-branch");
            ci_lint::run(&root, release_branch)
        }
        Some("thresholds-lint") => thresholds_lint::run(&root),
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

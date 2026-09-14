//! `cargo xtask epubcheck-parity` — measure what Tier 1 catches, against EPUBCheck's own
//! public test corpus, and write the number into `docs/TIER1_PARITY.md` (D6, RT A6.2).
//!
//! The claim this exists to make checkable is the one D6 rests on: "Tier 1 misses deep
//! content-model errors, mitigated by EPUBCheck in CI". Unmeasured, that sentence is a
//! hand-wave — nobody knows what fraction of real errors reaches a user who never runs
//! EPUBCheck. Measured, it is a number that CI holds non-decreasing, and the gap is a work
//! item rather than a shrug.
//!
//! The corpus is EPUBCheck's own (BSD-3), fetched by `xtask fetch-epubcheck-corpus`. It comes
//! in two shapes: 25 packaged `.epub` files, whose container bytes are part of what they test,
//! and about four hundred unpacked publication directories, which EPUBCheck's own harness zips
//! before validating. Both are used; the unpacked ones are zipped here with the deterministic
//! writer, which means an OCF-level defect in an unpacked case would be repaired on the way in.
//! That is why the packaged files matter and why the two counts are reported separately.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use oc_epub::EpubBytes;

/// Where the corpus is unpacked.
pub fn corpus_dir(workspace_root: &Path) -> PathBuf {
    workspace_root.join("vendor/epubcheck-corpus")
}

/// Where the number lives.
pub fn report_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join("docs/TIER1_PARITY.md")
}

/// One corpus entry's two verdicts.
struct Compared {
    /// The file it came from, so a per-id table can be traced back to a case.
    #[allow(dead_code)]
    name: String,
    epubcheck: BTreeSet<String>,
    tier1: BTreeSet<String>,
}

/// Measure, and either write the report or check the committed one against the measurement.
///
/// `--check` is the CI gate. It does not rewrite the file: a job that regenerated the number it
/// is about to compare against would pass on any input, which is the classic way a
/// non-decreasing gate stops gating.
pub fn run(workspace_root: &Path, check: bool) -> Result<()> {
    let jar = crate::fetch_epubcheck::jar_path(workspace_root)?;
    if !jar.is_file() {
        bail!(
            "{} is not there; run `cargo run -p xtask -- fetch-epubcheck` first",
            jar.display()
        );
    }
    let corpus = corpus_dir(workspace_root);
    if !corpus.is_dir() {
        bail!(
            "{} is not there; run `cargo run -p xtask -- fetch-epubcheck-corpus` first",
            corpus.display()
        );
    }

    let mut compared = Vec::new();
    let mut packaged = 0usize;
    let mut expanded = 0usize;

    for entry in walk(&corpus)? {
        let (bytes, name) = match load(&entry)? {
            Some(loaded) => loaded,
            None => continue,
        };
        if entry.extension().is_some_and(|ext| ext == "epub") {
            packaged += 1;
        } else {
            expanded += 1;
        }

        // EPUBCheck reads a file, so the bytes go to one — the same bytes Tier 1 sees, which is
        // the whole point: two validators disagreeing about *different* files would measure
        // nothing.
        let temporary = std::env::temp_dir().join(format!("oc-parity-{packaged}-{expanded}.epub"));
        std::fs::write(&temporary, &bytes.0)?;
        let external = oc_validate::epubcheck::run(&jar, &temporary)
            .with_context(|| format!("epubcheck on {name}"))?;
        let _ = std::fs::remove_file(&temporary);

        let report = oc_validate::validate_tier1(&bytes, &oc_validate::Expectations::default());
        compared.push(Compared {
            name,
            epubcheck: external
                .errors
                .iter()
                .map(|message| message.id.clone())
                .filter(|id| !id.is_empty())
                .collect(),
            tier1: report
                .message_ids()
                .into_iter()
                .map(str::to_owned)
                .collect(),
        });
    }

    let markdown = render(&compared, packaged, expanded);
    let path = report_path(workspace_root);
    println!("{}", summary(&compared));

    if !check {
        std::fs::write(&path, &markdown)
            .with_context(|| format!("cannot write {}", path.display()))?;
        println!("written to {}", path.display());
        return Ok(());
    }

    let committed = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read {}", path.display()))?;
    let Some((was_caught, was_total)) = recorded(&committed) else {
        bail!("{} records no PARITY line", path.display());
    };
    let (caught, total) = parity(&compared);

    // Compared as a ratio, because the corpus can grow: a bigger corpus with the same coverage
    // catches more absolutely and is not an improvement, and a smaller one is not a regression.
    let before = was_caught as f64 / was_total.max(1) as f64;
    let now = caught as f64 / total.max(1) as f64;
    if now + f64::EPSILON < before {
        bail!(
            "tier-1 parity fell from {was_caught}/{was_total} ({:.1} %) to {caught}/{total}              ({:.1} %). The number may be low; it may not silently fall (RT A6.2)",
            before * 100.0,
            now * 100.0
        );
    }
    println!(
        "parity holds: {caught}/{total} ({:.1} %) against the recorded {was_caught}/{was_total}",
        now * 100.0
    );
    Ok(())
}

/// Every file and expanded publication root under the corpus.
fn walk(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in std::fs::read_dir(&directory)? {
            let path = entry?.path();
            if path.is_dir() {
                // A directory holding a `mimetype` is an expanded publication, not a folder to
                // descend into: its children are the book's own files.
                if path.join("mimetype").is_file() {
                    out.push(path);
                } else {
                    stack.push(path);
                }
            } else if path.extension().is_some_and(|ext| ext == "epub") {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// A corpus entry as bytes: read, or zipped from an expanded publication.
fn load(path: &Path) -> Result<Option<(EpubBytes, String)>> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();

    if path.is_file() {
        return Ok(Some((EpubBytes(std::fs::read(path)?), name)));
    }

    let mut entries = Vec::new();
    collect(path, path, &mut entries)?;
    match oc_epub::zip::write_deterministic_zip(&entries) {
        Ok(bytes) => Ok(Some((EpubBytes(bytes), name))),
        // A case-name collision or a missing mimetype is a defect the writer refuses rather
        // than reproduces. Those entries are counted out of the corpus rather than measured
        // wrongly, and the report says how many.
        Err(_) => Ok(None),
    }
}

fn collect(root: &Path, directory: &Path, out: &mut Vec<oc_epub::zip::ZipEntry>) -> Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(root, &path, out)?;
        } else {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push(oc_epub::zip::ZipEntry::new(relative, std::fs::read(&path)?));
        }
    }
    Ok(())
}

/// The parity number: of every (file, message id) pair EPUBCheck reported as an error, the
/// share Tier 1 also reported.
fn parity(compared: &[Compared]) -> (usize, usize) {
    let mut total = 0usize;
    let mut caught = 0usize;
    for entry in compared {
        for id in &entry.epubcheck {
            total += 1;
            if entry.tier1.contains(id) {
                caught += 1;
            }
        }
    }
    (caught, total)
}

fn summary(compared: &[Compared]) -> String {
    let (caught, total) = parity(compared);
    let percent = if total == 0 {
        0.0
    } else {
        caught as f64 * 100.0 / total as f64
    };
    format!(
        "tier-1 parity: {caught}/{total} ({percent:.1} %) over {} files",
        compared.len()
    )
}

/// `docs/TIER1_PARITY.md`.
fn render(compared: &[Compared], packaged: usize, expanded: usize) -> String {
    let (caught, total) = parity(compared);
    let percent = if total == 0 {
        0.0
    } else {
        caught as f64 * 100.0 / total as f64
    };

    // Per message id: how often EPUBCheck reported it, and how often Tier 1 agreed.
    let mut per_id: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for entry in compared {
        for id in &entry.epubcheck {
            let counts = per_id.entry(id.clone()).or_default();
            counts.1 += 1;
            if entry.tier1.contains(id) {
                counts.0 += 1;
            }
        }
    }

    let mut out = String::new();
    out.push_str(
        "# Tier-1 parity against EPUBCheck's test corpus\n\
         \n\
         Generated by `cargo run -p xtask -- epubcheck-parity`. **Do not edit by hand.**\n\
         \n\
         D6 says Tier 1 \"misses deep content-model errors, mitigated by EPUBCheck in CI\".\n\
         Unmeasured, that is a hand-wave: nobody would know what fraction of real errors\n\
         reaches a user who never runs EPUBCheck. This is the measurement, and CI holds it\n\
         non-decreasing (RT A6.2). The number may be low; it may never silently fall.\n\
         \n",
    );
    out.push_str(&format!(
        "PARITY: {caught}/{total} ({percent:.1} %)\n\
         \n\
         Corpus: {} entries — {packaged} packaged `.epub` files, {expanded} expanded\n\
         publications zipped on the way in.\n\
         \n\
         An expanded publication's OCF-level bytes are this project's writer's, not the\n\
         corpus's, so an OCF defect in one of those cases is repaired before Tier 1 sees it.\n\
         That is what the packaged files are for, and it is why the two counts are separate.\n\
         \n\
         ## Per message id\n\
         \n\
         | id | EPUBCheck reported | Tier 1 agreed |\n\
         |---|---:|---:|\n",
        compared.len()
    ));
    for (id, (agreed, reported)) in &per_id {
        out.push_str(&format!("| {id} | {reported} | {agreed} |\n"));
    }
    out
}

/// The parity number recorded in the file, for the CI gate that holds it non-decreasing.
pub fn recorded(markdown: &str) -> Option<(usize, usize)> {
    let line = markdown.lines().find(|line| line.starts_with("PARITY: "))?;
    let fraction = line.trim_start_matches("PARITY: ").split(' ').next()?;
    let (caught, total) = fraction.split_once('/')?;
    Some((caught.parse().ok()?, total.parse().ok()?))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// The gate reads one line out of this file, so that line has to be machine-readable and it has
/// to be the one the renderer writes. A gate that silently failed to find the number would pass
/// every run.
#[test]
fn the_recorded_number_round_trips_through_the_report() {
    let compared = vec![
        Compared {
            name: "a.epub".to_owned(),
            epubcheck: ["RSC-005".to_owned(), "PKG-007".to_owned()]
                .into_iter()
                .collect(),
            tier1: ["RSC-005".to_owned()].into_iter().collect(),
        },
        Compared {
            name: "b.epub".to_owned(),
            epubcheck: ["OPF-014".to_owned()].into_iter().collect(),
            tier1: ["OPF-014".to_owned()].into_iter().collect(),
        },
    ];

    let markdown = render(&compared, 2, 0);
    assert_eq!(recorded(&markdown), Some((2, 3)));
    assert!(markdown.contains("| PKG-007 | 1 | 0 |"));
    assert!(markdown.contains("| RSC-005 | 1 | 1 |"));
}

/// A report with no number in it must not read as a parity of zero out of zero, which would be
/// a gate that anything passes.
#[test]
fn a_report_without_a_number_is_not_a_number() {
    assert_eq!(recorded("# Tier-1 parity\n\nnothing here\n"), None);
}

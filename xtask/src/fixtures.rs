//! `cargo xtask fixtures` — compile the Typst fixture sources into PDFs.
//!
//! Typst is linked in and driven in-process (`DECISIONS.md` Appendix A, `TEST_CORPUS.md`
//! §6.1): regenerating a fixture needs no external tool, no font installed on the machine,
//! and no network. Only the fonts `typst-assets` embeds are visible to the compiler, so the
//! same source produces the same page on every platform.
//!
//! Struct trees are stripped by default (D18). Typst tags its PDFs, and the real world is
//! 12.6 % tagged — a fixture that hands the pipeline a structure tree tests a path most
//! books will never take. `--keep-structtree` emits the tagged variants instead, named
//! `<fixture>__tagged.pdf`, for the Phase 4 bucket that wants them — and only for the
//! fixtures [`TAGGED_FIXTURES`] names, so that the bucket stays the ~12.6 % share D18 asks
//! for rather than becoming half the synthetic corpus.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
// `Library::default()` and `Library::builder()` come from this extension trait, not from
// `Library` itself.
use typst::LibraryExt as _;

/// The Typst project root. Sources reach the shared assets as `../assets/...`, so the root
/// has to be the directory above them, exactly as the plan specifies
/// (`World { root: corpus/fixtures, .. }`).
const FIXTURE_ROOT: &str = "corpus/fixtures";

/// Where the `.typ` sources live, relative to the workspace root.
const SOURCE_DIR: &str = "corpus/fixtures/typst";

/// Where compiled PDFs are written. Git-ignored: they are build output, reproducible from
/// the sources beside them.
const OUTPUT_DIR: &str = "target/fixtures";

/// Suffix for the tagged variants (D18's separate bucket).
const TAGGED_SUFFIX: &str = "__tagged";

/// The fixtures that get a tagged variant, and the only ones.
///
/// D18 keeps tagged files a *separate ~12.6 % bucket* because Typst tags every PDF it writes
/// and the real world does not. Emitting a tagged variant of every fixture, which is what this
/// task used to do, makes the synthetic bucket 50 % tagged — the opposite of what the decision
/// asks for, and a corpus that teaches the pipeline a struct tree is the normal case.
///
/// Two names, out of ten sources, is 2/12 = 0.167: the widest coverage that still lands inside
/// `corpus.tagged_share_target` +/- `corpus.tagged_share_tolerance`. They are one English and
/// one German fixture, so the tagged path is exercised in both scripts the tagging affects.
const TAGGED_FIXTURES: &[&str] = &["f01_prose_single_column", "f04_german_prose"];

/// The corpus stratum every fixture this task produces belongs to (D18).
const STRATUM: &str = "ours(Typst)";

/// The virtual path the source is compiled under, relative to [`FIXTURE_ROOT`]. It sits in
/// `typst/` so that a source's own `../assets/...` reference resolves the same way it would
/// if the file were read from disk. Every fixture is compiled alone, so one name serves for
/// all of them.
const MAIN_VPATH: &str = "typst/main.typ";

/// A fixed date handed to Typst's `datetime`, so nothing in a fixture can depend on the day
/// it was built. The fixtures set `date: none` anyway; this closes the door rather than
/// trusting them to keep doing so.
const FIXED_DATE: (i32, u8, u8) = (2026, 1, 1);

/// Compile one `.typ` source and return the PDF bytes.
///
/// `tagged` maps straight onto `PdfOptions::tagged`, so the untagged variant never grows a
/// structure tree in the first place. That is a stronger guarantee than D18's "struct trees
/// are stripped": there is nothing left to strip, and no marked-content operators are left
/// behind in the content streams either.
pub fn compile_fixture(root: &Path, source_path: &Path, tagged: bool) -> Result<Vec<u8>> {
    let text = std::fs::read_to_string(source_path)
        .with_context(|| format!("cannot read {}", source_path.display()))?;
    let world = FixtureWorld::new(root.join(FIXTURE_ROOT), text)?;

    let compiled = typst::compile::<typst_layout::PagedDocument>(&world);
    let document = compiled.output.map_err(|errors| {
        anyhow!(
            "{} does not compile:\n{}",
            source_path.display(),
            errors
                .iter()
                .map(|e| format!("  {}", e.message))
                .collect::<Vec<_>>()
                .join("\n")
        )
    })?;

    let options = typst_pdf::PdfOptions {
        // A stable identifier keyed to the fixture, so the PDF's own id does not move
        // between runs. `Smart::Auto` would hash the title, and two of the three fixtures
        // would then share one.
        ident: typst::foundations::Smart::Custom(fixture_stem(source_path)?),
        creator: typst::foundations::Smart::Auto,
        // No creation timestamp: a fixture that changes with the clock is not a fixture.
        timestamp: None,
        page_ranges: None,
        standards: typst_pdf::PdfStandards::default(),
        tagged,
        pretty: false,
    };
    typst_pdf::pdf(&document, &options).map_err(|errors| {
        anyhow!(
            "{} does not export to PDF:\n{}",
            source_path.display(),
            errors
                .iter()
                .map(|e| format!("  {}", e.message))
                .collect::<Vec<_>>()
                .join("\n")
        )
    })
}

/// The `.typ` sources, sorted, so output order does not depend on the filesystem.
pub fn source_paths(root: &Path) -> Result<Vec<PathBuf>> {
    let dir = root.join(SOURCE_DIR);
    let mut sources: Vec<PathBuf> = std::fs::read_dir(&dir)
        .with_context(|| format!("cannot read {}", dir.display()))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "typ"))
        .collect();
    sources.sort();
    Ok(sources)
}

fn fixture_stem(source_path: &Path) -> Result<String> {
    source_path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("{} has no usable file stem", source_path.display()))
}

/// Compile every fixture and record it in the corpus manifest.
pub fn run(root: &Path, keep_structtree: bool) -> Result<()> {
    let sources = source_paths(root)?;
    if sources.is_empty() {
        bail!("no .typ sources in {}", root.join(SOURCE_DIR).display());
    }

    let out_dir = root.join(OUTPUT_DIR);
    std::fs::create_dir_all(&out_dir)?;

    let mut entries = BTreeMap::new();
    for source in &sources {
        let stem = fixture_stem(source)?;
        if keep_structtree && !TAGGED_FIXTURES.contains(&stem.as_str()) {
            continue;
        }
        let name = if keep_structtree {
            format!("{stem}{TAGGED_SUFFIX}")
        } else {
            stem
        };
        let bytes = compile_fixture(root, source, keep_structtree)?;
        let out_path = out_dir.join(format!("{name}.pdf"));
        std::fs::write(&out_path, &bytes)
            .with_context(|| format!("cannot write {}", out_path.display()))?;
        println!(
            "{} -> {} ({} bytes)",
            source.display(),
            out_path.display(),
            bytes.len()
        );
        entries.insert(name, (source.clone(), bytes));
    }

    update_manifest(root, &entries, keep_structtree)
}

/// Record the compiled fixtures in `corpus/manifest.json` (D18: every corpus file carries a
/// producer stratum and a licence, and `ours(*)` is capped at 40 % of the corpus).
fn update_manifest(
    root: &Path,
    entries: &BTreeMap<String, (PathBuf, Vec<u8>)>,
    tagged: bool,
) -> Result<()> {
    use sha2::{Digest, Sha256};

    let manifest_path = root.join("corpus/manifest.json");
    let text = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("cannot read {}", manifest_path.display()))?;
    let mut manifest: serde_json::Value =
        serde_json::from_str(&text).context("corpus/manifest.json is not valid JSON")?;

    let files = manifest
        .get_mut("files")
        .and_then(|f| f.as_array_mut())
        .ok_or_else(|| anyhow!("corpus/manifest.json has no `files` array"))?;

    for (name, (source, bytes)) in entries {
        let digest: String = Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let entry = serde_json::json!({
            "id": name,
            "title": name,
            "source": {
                "name": "synthetic-generator",
                "url": "https://github.com/openconvert/openconvert",
                "retrieved_date": "2026-09-09"
            },
            "license": {
                "name": "CC0-1.0",
                "url": "https://creativecommons.org/publicdomain/zero/1.0/",
                "verified_by": "maintainer",
                "verified_date": "2026-09-09"
            },
            "sha256": digest,
            "file_size_bytes": bytes.len(),
            "category": "simple",
            "producer_stratum": STRATUM,
            "producer_raw": "Typst 0.15.1",
            "tagged": tagged,
            "holdout": false,
            "ground_truth_type": "none",
            "generator": "typst-0.15.1",
            "defect_injection": [],
            "source_path": source
                .strip_prefix(root)
                .unwrap_or(source)
                .to_string_lossy()
                .replace('\\', "/")
        });
        // Replace in place if the id is already known, so re-running is idempotent.
        match files
            .iter()
            .position(|f| f.get("id").and_then(|i| i.as_str()) == Some(name.as_str()))
        {
            Some(index) => files[index] = entry,
            None => files.push(entry),
        }
    }

    sort_manifest_files(files);

    let mut out = serde_json::to_string_pretty(&manifest)?;
    out.push('\n');
    std::fs::write(&manifest_path, out)
        .with_context(|| format!("cannot write {}", manifest_path.display()))?;
    Ok(())
}

/// The manifest's one order: `files` sorted by `id`, byte-wise. `oc-eval` sorts the same way
/// (`manifest.dump`, `scan_sim`), so whichever writer ran last the file is the same.
fn sort_manifest_files(files: &mut [serde_json::Value]) {
    files.sort_by(|a, b| {
        a.get("id")
            .and_then(|i| i.as_str())
            .cmp(&b.get("id").and_then(|i| i.as_str()))
    });
}

// ---------------------------------------------------------------------------
// The compilation environment
// ---------------------------------------------------------------------------

/// A Typst `World` over exactly one source and the fixture asset directory.
///
/// Only the fonts `typst-assets` embeds are visible, and only files under
/// `corpus/fixtures` are readable. Nothing on the developer's machine can change what a
/// fixture compiles to, which is what makes "regenerate and diff" a meaningful check.
struct FixtureWorld {
    root: PathBuf,
    library: typst::utils::LazyHash<typst::Library>,
    book: typst::utils::LazyHash<typst::text::FontBook>,
    fonts: Vec<typst::text::Font>,
    main: typst::syntax::FileId,
    source: typst::syntax::Source,
}

impl FixtureWorld {
    fn new(root: PathBuf, text: String) -> Result<Self> {
        let fonts: Vec<typst::text::Font> = typst_assets::fonts()
            .flat_map(|data| {
                let bytes = typst::foundations::Bytes::new(data.to_vec());
                typst::text::Font::iter(bytes)
            })
            .collect();
        if fonts.is_empty() {
            bail!("typst-assets provided no fonts; the `fonts` feature must be enabled");
        }

        let book = typst::text::FontBook::from_fonts(&fonts);
        let vpath = typst::syntax::VirtualPath::new(MAIN_VPATH)
            .map_err(|e| anyhow!("{MAIN_VPATH} is not a usable virtual path: {e:?}"))?;
        let main =
            typst::syntax::RootedPath::new(typst::syntax::VirtualRoot::Project, vpath).intern();

        Ok(Self {
            root,
            library: typst::utils::LazyHash::new(typst::Library::default()),
            book: typst::utils::LazyHash::new(book),
            fonts,
            main,
            source: typst::syntax::Source::new(main, text),
        })
    }

    fn read(&self, id: typst::syntax::FileId) -> typst::diag::FileResult<Vec<u8>> {
        let path = id
            .vpath()
            .realize(&self.root)
            .map_err(|_| typst::diag::FileError::AccessDenied)?;
        std::fs::read(&path).map_err(|e| typst::diag::FileError::from_io(e, &path))
    }
}

impl typst::World for FixtureWorld {
    fn library(&self) -> &typst::utils::LazyHash<typst::Library> {
        &self.library
    }

    fn book(&self) -> &typst::utils::LazyHash<typst::text::FontBook> {
        &self.book
    }

    fn main(&self) -> typst::syntax::FileId {
        self.main
    }

    fn source(&self, id: typst::syntax::FileId) -> typst::diag::FileResult<typst::syntax::Source> {
        if id == self.main {
            return Ok(self.source.clone());
        }
        // A fixture that imports another file would need a real loader; none does, and a
        // clear error beats silently compiling something unexpected.
        Err(typst::diag::FileError::NotFound(
            id.vpath().get_without_slash().into(),
        ))
    }

    fn file(
        &self,
        id: typst::syntax::FileId,
    ) -> typst::diag::FileResult<typst::foundations::Bytes> {
        self.read(id).map(typst::foundations::Bytes::new)
    }

    fn font(&self, index: usize) -> Option<typst::text::Font> {
        self.fonts.get(index).cloned()
    }

    fn today(
        &self,
        _offset: Option<typst::foundations::Duration>,
    ) -> Option<typst::foundations::Datetime> {
        let (year, month, day) = FIXED_DATE;
        typst::foundations::Datetime::from_ymd(year, month, day)
    }
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Row 0.20 of the Phase 0 table.
// ---------------------------------------------------------------------------

/// The workspace root, from this crate's manifest directory, so the test does not depend on
/// the working directory a runner happens to choose.
#[cfg(test)]
pub(crate) fn workspace_root_for_test() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(std::path::Path::to_path_buf)
        .expect("xtask has a parent directory")
}

#[test]
fn typst_fixtures_are_reproducible() {
    use crate::fixtures::{compile_fixture, source_paths, workspace_root_for_test};

    let root = workspace_root_for_test();
    let sources = source_paths(&root).expect("the fixture sources are readable");
    // Every Typst fixture the repository has, whatever the count is today: the test's
    // subject is that compiling one twice gives the same bytes, and a hard-coded count turns
    // "a phase added a fixture" into a failure that says nothing about reproducibility.
    // The floor is what Phase 0 shipped, so an empty or missing directory still fails.
    assert!(
        sources.len() >= 5,
        "expected at least f01-f05 in {SOURCE_DIR}, found {sources:?}"
    );

    for source in &sources {
        let first = compile_fixture(&root, source, false).expect("first compilation");
        let second = compile_fixture(&root, source, false).expect("second compilation");
        assert_eq!(
            first,
            second,
            "compiling {} twice produced different bytes; if this is inherent to Typst the \
             fallback is R7 §D.2 — commit the PDFs as golden binaries and add a nightly \
             regenerate-and-diff job",
            source.display()
        );
        assert!(
            first.starts_with(b"%PDF-"),
            "{} did not produce a PDF",
            source.display()
        );
    }
}

#[test]
fn the_tagged_bucket_is_the_share_the_real_world_has() {
    use crate::fixtures::{source_paths, workspace_root_for_test, TAGGED_FIXTURES};
    use oc_core::thresholds::T;

    let root = workspace_root_for_test();
    let sources = source_paths(&root).expect("the fixture sources are readable");

    // Every source is emitted untagged; TAGGED_FIXTURES are emitted a second time with their
    // struct tree kept. The synthetic bucket is therefore both together.
    let bucket = sources.len() + TAGGED_FIXTURES.len();
    let share = TAGGED_FIXTURES.len() as f64 / bucket as f64;

    assert!(
        (share - T.corpus.tagged_share_target).abs() <= T.corpus.tagged_share_tolerance,
        "the synthetic bucket would be {share:.3} tagged; D18 wants \
         {target} +/- {tolerance}. Typst tags every PDF it writes, so a fixture set that keeps \
         every struct tree teaches the pipeline that tagged is the normal case",
        target = T.corpus.tagged_share_target,
        tolerance = T.corpus.tagged_share_tolerance,
    );
}

#[test]
fn every_named_tagged_fixture_is_a_fixture_that_exists() {
    use crate::fixtures::{fixture_stem, source_paths, workspace_root_for_test, TAGGED_FIXTURES};

    let root = workspace_root_for_test();
    let stems: Vec<String> = source_paths(&root)
        .expect("the fixture sources are readable")
        .iter()
        .map(|source| fixture_stem(source).expect("a fixture has a stem"))
        .collect();

    for name in TAGGED_FIXTURES {
        assert!(
            stems.iter().any(|stem| stem == name),
            "TAGGED_FIXTURES names {name:?}, which is not among {stems:?}"
        );
    }
}

/// `corpus/manifest.json` has three writers — this module, `oc-eval`'s harvest
/// (`manifest.dump`) and `oc-eval`'s scan simulator — and CI regenerates it with two of them
/// before checking the tree is clean. They agree only if every one writes the same order, so
/// the order is part of the file's contract: `files` sorted by `id`, byte-wise.
#[test]
fn the_committed_manifest_is_in_canonical_order() {
    use crate::fixtures::{sort_manifest_files, workspace_root_for_test};

    let root = workspace_root_for_test();
    let text = std::fs::read_to_string(root.join("corpus/manifest.json"))
        .expect("corpus/manifest.json is readable");
    let manifest: serde_json::Value = serde_json::from_str(&text).expect("the manifest is JSON");
    let files = manifest["files"]
        .as_array()
        .expect("the manifest has a files array");

    let committed: Vec<&str> = files.iter().filter_map(|f| f["id"].as_str()).collect();
    let mut sorted = files.clone();
    sort_manifest_files(&mut sorted);
    let canonical: Vec<&str> = sorted.iter().filter_map(|f| f["id"].as_str()).collect();
    assert_eq!(
        committed, canonical,
        "corpus/manifest.json is not sorted by id, so `xtask fixtures` would reorder it"
    );
}

#[test]
fn regenerating_the_manifest_from_the_committed_one_is_a_no_op() {
    use crate::fixtures::{sort_manifest_files, workspace_root_for_test};

    let root = workspace_root_for_test();
    let text = std::fs::read_to_string(root.join("corpus/manifest.json"))
        .expect("corpus/manifest.json is readable");
    let mut manifest: serde_json::Value =
        serde_json::from_str(&text).expect("the manifest is JSON");
    let files = manifest["files"]
        .as_array_mut()
        .expect("the manifest has a files array");
    sort_manifest_files(files);
    let rendered = serde_json::to_string_pretty(&manifest).expect("the manifest serialises") + "\n";
    assert!(
        rendered == text,
        "re-serialising corpus/manifest.json the way `xtask fixtures` does changes it"
    );
}

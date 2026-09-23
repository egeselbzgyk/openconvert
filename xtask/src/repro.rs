//! `cargo xtask repro` — the reproducibility gate (PHASE 15 detail 7, D13.8; row 15.13, A15.3).
//!
//! Each OS's release job converts the fast corpus with the engine it just built, `--no-ai`, OCR
//! off and `dcterms:modified` pinned, and writes a hash table: fixture → SHA-256 of the EPUB, or the
//! exit code when there was none (a PDF the engine refuses must be refused alike everywhere). The
//! compare step then requires every table to agree. A mismatch is a release blocker, and it is
//! reported as the first differing zip entry and the byte offset inside it, not as "hashes differ".
//!
//! ```text
//! repro hash --engine <openconvert> --os <os> --out <table.json> --epubs <dir>
//! repro compare <table.json>... [--epubs <os>=<dir>...]
//! ```
//!
//! The fast corpus is every PDF the repository can produce or holds without a download: the Typst
//! fixtures (`xtask fixtures` → `target/fixtures`), and the committed hand-made and mutation
//! fixtures (`corpus/fixtures/{handmade,mutations}`). The scanned fixtures are not in it: their
//! text comes from the OCR engine a machine has, which is not what this gate is about.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::release::{sha256_file, Os};

/// The `dcterms:modified` every conversion is pinned to, so the timestamp is not a difference.
pub const PINNED_MODIFIED: &str = "2026-01-01T00:00:00Z";

/// Where the fast corpus lives, relative to the workspace root.
const CORPUS_DIRS: [&str; 3] = [
    "target/fixtures",
    "corpus/fixtures/handmade",
    "corpus/fixtures/mutations",
];

/// One OS's results.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HashTable {
    pub os: Os,
    /// `openconvert --version` of the engine that produced it.
    pub engine: String,
    /// Fixture id → `sha256:<hex>` or `exit:<code>`.
    pub fixtures: BTreeMap<String, String>,
}

/// Every fast-corpus PDF, by fixture id (its file stem), in a fixed order.
pub fn fast_corpus(workspace_root: &Path) -> Result<BTreeMap<String, PathBuf>> {
    let mut corpus = BTreeMap::new();
    for dir in CORPUS_DIRS {
        let dir = workspace_root.join(dir);
        for entry in std::fs::read_dir(&dir)
            .with_context(|| format!("cannot list {} (run `xtask fixtures`)", dir.display()))?
        {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "pdf") {
                let id = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .context("a fixture name")?
                    .to_owned();
                if corpus.insert(id.clone(), path).is_some() {
                    bail!("two fast-corpus fixtures are both called {id}");
                }
            }
        }
    }
    if corpus.is_empty() {
        bail!("the fast corpus is empty");
    }
    Ok(corpus)
}

/// Convert the fast corpus with `engine` into `epubs`, and hash what came out.
pub fn hash(workspace_root: &Path, engine: &Path, os: Os, epubs: &Path) -> Result<HashTable> {
    std::fs::create_dir_all(epubs).with_context(|| format!("cannot create {}", epubs.display()))?;
    let version = Command::new(engine)
        .arg("--version")
        .output()
        .with_context(|| format!("cannot run {}", engine.display()))?;
    // The vendored PDFium, named outright, so the result does not depend on the directory this
    // runs from; an explicit `OC_PDFIUM_PATH` wins.
    let pdfium = std::env::var_os("OC_PDFIUM_PATH").unwrap_or_else(|| {
        workspace_root
            .join("vendor/pdfium")
            .join(env!("XTASK_HOST_TRIPLE"))
            .into_os_string()
    });
    let mut fixtures = BTreeMap::new();
    for (id, pdf) in fast_corpus(workspace_root)? {
        let out = epubs.join(format!("{id}.epub"));
        let _ = std::fs::remove_file(&out);
        let status = Command::new(engine)
            .env("OC_PDFIUM_PATH", &pdfium)
            .arg("convert")
            .arg(&pdf)
            .arg("-o")
            .arg(&out)
            .args(["--no-ai", "--ocr", "never", "--modified", PINNED_MODIFIED])
            .arg("--report")
            .arg(epubs.join(format!("{id}.report.json")))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .with_context(|| format!("cannot run {}", engine.display()))?;
        let outcome = if status.success() && out.is_file() {
            format!("sha256:{}", sha256_file(&out)?)
        } else {
            format!("exit:{}", status.code().unwrap_or(-1))
        };
        fixtures.insert(id, outcome);
    }
    Ok(HashTable {
        os,
        engine: String::from_utf8_lossy(&version.stdout).trim().to_owned(),
        fixtures,
    })
}

/// Where two EPUBs first differ.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZipDifference {
    /// The first entry, in archive order, whose name or bytes differ; or `<container>` when every
    /// entry agrees and the difference is in the zip structure itself.
    pub entry: String,
    /// The byte offset of the first difference inside that entry (inside the file, for
    /// `<container>`).
    pub offset: u64,
}

/// A table that does not agree with the others.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ReproMismatch {
    #[error("{0} OS table(s) given; the gate compares all three of linux, macos, windows")]
    MissingOs(usize),
    #[error("the fast corpus differs between {left:?} and {right:?}: {only}")]
    DifferentCorpus { left: Os, right: Os, only: String },
    #[error(
        "{fixture}: {left:?} made {left_outcome}, {right:?} made {right_outcome}{}",
        difference.as_ref().map(|d| format!(" — first difference in `{}` at byte {}", d.entry, d.offset)).unwrap_or_default()
    )]
    Differs {
        fixture: String,
        left: Os,
        right: Os,
        left_outcome: String,
        right_outcome: String,
        difference: Option<ZipDifference>,
    },
}

/// The first byte at which `a` and `b` differ, or `None` when they are equal.
fn first_byte(a: &[u8], b: &[u8]) -> Option<u64> {
    let common = a.len().min(b.len());
    let at = (0..common).find(|&i| a[i] != b[i]).or({
        if a.len() == b.len() {
            None
        } else {
            Some(common)
        }
    });
    at.map(|i| u64::try_from(i).unwrap_or(u64::MAX))
}

/// Where two EPUBs first differ: the first entry whose name or content differs, and the offset
/// inside it.
pub fn first_difference(a: &[u8], b: &[u8]) -> Result<Option<ZipDifference>> {
    if a == b {
        return Ok(None);
    }
    let mut left = zip::ZipArchive::new(std::io::Cursor::new(a)).context("not a zip")?;
    let mut right = zip::ZipArchive::new(std::io::Cursor::new(b)).context("not a zip")?;
    let entries = left.len().max(right.len());
    for index in 0..entries {
        let read = |archive: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>| -> Result<Option<(String, Vec<u8>)>> {
            if index >= archive.len() {
                return Ok(None);
            }
            let mut file = archive.by_index(index)?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            Ok(Some((file.name().to_owned(), bytes)))
        };
        match (read(&mut left)?, read(&mut right)?) {
            (Some((name_a, bytes_a)), Some((name_b, bytes_b))) => {
                if name_a != name_b {
                    return Ok(Some(ZipDifference {
                        entry: format!("{name_a} / {name_b}"),
                        offset: 0,
                    }));
                }
                if let Some(offset) = first_byte(&bytes_a, &bytes_b) {
                    return Ok(Some(ZipDifference {
                        entry: name_a,
                        offset,
                    }));
                }
            }
            (Some((name, _)), None) | (None, Some((name, _))) => {
                return Ok(Some(ZipDifference {
                    entry: name,
                    offset: 0,
                }));
            }
            (None, None) => {}
        }
    }
    Ok(Some(ZipDifference {
        entry: "<container>".to_owned(),
        offset: first_byte(a, b).unwrap_or_default(),
    }))
}

/// Require every table to agree (PHASE 15 Architecture's `repro_check`). `epubs` holds each OS's
/// EPUB directory when available, so a mismatch can name the first differing zip entry.
pub fn repro_check(
    tables: &BTreeMap<Os, BTreeMap<String, String>>,
    epubs: &BTreeMap<Os, PathBuf>,
) -> std::result::Result<(), ReproMismatch> {
    if tables.len() != [Os::Linux, Os::Macos, Os::Windows].len() {
        return Err(ReproMismatch::MissingOs(tables.len()));
    }
    let mut iter = tables.iter();
    let Some((first_os, first)) = iter.next() else {
        return Err(ReproMismatch::MissingOs(0));
    };
    for (os, table) in iter {
        let left_keys: Vec<&String> = first.keys().collect();
        let right_keys: Vec<&String> = table.keys().collect();
        if left_keys != right_keys {
            let only: Vec<&String> = first
                .keys()
                .filter(|k| !table.contains_key(*k))
                .chain(table.keys().filter(|k| !first.contains_key(*k)))
                .collect();
            return Err(ReproMismatch::DifferentCorpus {
                left: *first_os,
                right: *os,
                only: format!("{only:?}"),
            });
        }
        for (fixture, outcome) in first {
            let other = &table[fixture];
            if outcome != other {
                let difference = match (epubs.get(first_os), epubs.get(os)) {
                    (Some(a), Some(b)) => {
                        let name = format!("{fixture}.epub");
                        match (std::fs::read(a.join(&name)), std::fs::read(b.join(&name))) {
                            (Ok(a), Ok(b)) => first_difference(&a, &b).ok().flatten(),
                            _ => None,
                        }
                    }
                    _ => None,
                };
                return Err(ReproMismatch::Differs {
                    fixture: fixture.clone(),
                    left: *first_os,
                    right: *os,
                    left_outcome: outcome.clone(),
                    right_outcome: other.clone(),
                    difference,
                });
            }
        }
    }
    Ok(())
}

fn flag(args: &[String], name: &str) -> Result<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
        .with_context(|| format!("{name} <value> is required"))
}

pub fn run(workspace_root: &Path, args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("hash") => {
            let engine = PathBuf::from(flag(args, "--engine")?);
            let os = Os::parse(&flag(args, "--os")?)?;
            let epubs = PathBuf::from(flag(args, "--epubs")?);
            let table = hash(workspace_root, &engine, os, &epubs)?;
            let out = PathBuf::from(flag(args, "--out")?);
            std::fs::write(&out, serde_json::to_string_pretty(&table)? + "\n")
                .with_context(|| format!("cannot write {}", out.display()))?;
            for (id, outcome) in &table.fixtures {
                println!("{outcome}  {id}");
            }
            Ok(())
        }
        Some("compare") => {
            let mut tables = BTreeMap::new();
            let mut epubs = BTreeMap::new();
            let mut rest = args[1..].iter();
            while let Some(arg) = rest.next() {
                if arg == "--epubs" {
                    let spec = rest.next().context("--epubs <os>=<dir>")?;
                    let (os, dir) = spec.split_once('=').context("--epubs <os>=<dir>")?;
                    epubs.insert(Os::parse(os)?, PathBuf::from(dir));
                    continue;
                }
                let text =
                    std::fs::read_to_string(arg).with_context(|| format!("cannot read {arg}"))?;
                let table: HashTable = serde_json::from_str(&text)?;
                println!(
                    "{:?}: {} ({} fixtures)",
                    table.os,
                    table.engine,
                    table.fixtures.len()
                );
                tables.insert(table.os, table.fixtures);
            }
            repro_check(&tables, &epubs).map_err(|e| anyhow::anyhow!("{e} (D13.8)"))?;
            println!("the three operating systems made byte-identical EPUBs");
            Ok(())
        }
        other => bail!("repro: unknown step {other:?} (hash, compare)"),
    }
}

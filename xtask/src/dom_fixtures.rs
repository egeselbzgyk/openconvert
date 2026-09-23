//! `cargo xtask dom-fixtures` — convert the fixtures and unpack their containers for Playwright.
//!
//! The DOM checks are Layer 2 of D7: a browser loading the *emitted* XHTML and asserting
//! structurally — no horizontal overflow, headings in nav order, every note reference resolving.
//! Playwright cannot read a zip, so the containers are unpacked to `target/dom/<fixture>/` with
//! their paths intact, which is what makes a relative `href` and a relative `src` resolve the way
//! they will in a reading system.
//!
//! **A manifest is written beside them**, naming the spine and the note references each document
//! carries. Those are the two things a browser cannot enumerate from inside one document, and they
//! are all the manifest holds: the nav order is read *in the browser*, by the specs, so that both
//! sides of the heading-order comparison come from the parser the reader's software uses. A manifest
//! carrying our own view of the nav would make a spec that could not fail when the nav was wrong.
//!
//! `dcterms:modified` is pinned so that running the task twice leaves the directory unchanged.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use oc_core::thresholds::T;
use oc_epub::EpubOptions;
use oc_model::document::PresetName;
use oc_model::lang::LangTag;
use oc_pdf::inspect::PdfOpen;
use oc_pdf::pdfium::PdfiumBackend;
use openconvert::convert::{convert, sha256_hex, ConvertOptions};

/// Held still, so two runs of the task produce the same bytes (D13.8).
const MODIFIED: &str = "2026-01-01T00:00:00Z";

/// What one fixture's unpacked container offers a spec.
#[derive(serde::Serialize)]
struct Entry {
    fixture: String,
    /// The content documents, in spine order, relative to the fixture's directory.
    spine: Vec<String>,
    /// Every note reference in the book: `(document, target document, fragment)`.
    noterefs: Vec<NoteRef>,
}

#[derive(serde::Serialize)]
struct NoteRef {
    from: String,
    to: String,
    fragment: String,
}

pub fn run(root: &Path) -> Result<()> {
    let out = root.join("target/dom");
    if out.exists() {
        std::fs::remove_dir_all(&out).with_context(|| format!("cannot clear {}", out.display()))?;
    }
    std::fs::create_dir_all(&out).with_context(|| format!("cannot create {}", out.display()))?;

    let backend =
        PdfiumBackend::bind().context("PDFium is vendored (cargo xtask vendor-pdfium)")?;
    let mut manifest: Vec<Entry> = Vec::new();

    for pdf_path in built_fixtures(root)? {
        let stem = pdf_path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        let stem = stem.as_str();
        let bytes = std::fs::read(&pdf_path)
            .with_context(|| format!("cannot read {}", pdf_path.display()))?;

        let pdf = backend
            .open(&bytes, None)
            .with_context(|| format!("{stem} does not open"))?;
        let conversion = convert(
            pdf.as_ref(),
            &sha256_hex(&bytes),
            &ConvertOptions {
                filename: format!("{stem}.pdf"),
                language: Some(LangTag::EN),
                preset: PresetName::Auto,
                epub: EpubOptions {
                    split_bytes: usize::try_from(T.xhtml.split_bytes).unwrap_or(usize::MAX),
                    max_longest_side_px: u32::try_from(T.images.max_longest_side_px)
                        .unwrap_or(u32::MAX),
                    jpeg_quality: u8::try_from(T.images.jpeg_quality).unwrap_or(85),
                    warn_total_bytes: u64::try_from(T.epub.warn_total_bytes).unwrap_or(u64::MAX),
                    modified: MODIFIED.to_owned(),
                },
                overrides: None,
                cache_dir: None,
                ocr: openconvert::ocr::OcrOptions::off(),
            },
            &T,
        )
        .with_context(|| format!("{stem} does not convert"))?;

        let entries = oc_epub::read_entries(&conversion.built.bytes)
            .with_context(|| format!("{stem}'s container does not read back"))?;

        let directory = out.join(stem);
        for (path, bytes) in &entries {
            let target = directory.join(path);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("cannot create {}", parent.display()))?;
            }
            std::fs::write(&target, bytes)
                .with_context(|| format!("cannot write {}", target.display()))?;
        }

        manifest.push(entry(stem, &entries));
    }

    let manifest_path = out.join("manifest.json");
    let json = serde_json::to_string_pretty(&manifest)? + "\n";
    std::fs::write(&manifest_path, json)
        .with_context(|| format!("cannot write {}", manifest_path.display()))?;

    println!(
        "dom-fixtures: {} fixtures unpacked under {}",
        manifest.len(),
        out.display()
    );
    Ok(())
}

/// Every compiled fixture, untagged, in name order.
///
/// The tagged variants are D18's separate bucket and are the same books; loading both into a
/// browser would double the runtime of every spec to assert the same things twice.
fn built_fixtures(root: &Path) -> Result<Vec<std::path::PathBuf>> {
    let dir = root.join("target/fixtures");
    let entries = std::fs::read_dir(&dir).with_context(|| {
        format!(
            "cannot read {}; run `cargo run -p xtask -- fixtures` first",
            dir.display()
        )
    })?;
    let mut out: Vec<std::path::PathBuf> = entries
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "pdf"))
        .filter(|path| {
            !path
                .file_stem()
                .is_some_and(|stem| stem.to_string_lossy().contains("__tagged"))
        })
        .collect();
    out.sort();
    Ok(out)
}

fn entry(stem: &str, entries: &BTreeMap<String, Vec<u8>>) -> Entry {
    let text = |path: &str| -> String {
        entries
            .get(path)
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
            .unwrap_or_default()
    };

    let package = entries
        .get("META-INF/container.xml")
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .and_then(|container| attribute(&container, "full-path=\""))
        .unwrap_or_else(|| "content.opf".to_owned());
    let opf = text(&package);

    // Manifest id → href, then the spine's `itemref` order through it.
    let mut hrefs: BTreeMap<String, String> = BTreeMap::new();
    for tag in tags(&opf, "<item ") {
        if let (Some(id), Some(href)) = (attribute(&tag, "id=\""), attribute(&tag, "href=\"")) {
            hrefs.insert(id, href);
        }
    }
    let spine: Vec<String> = attributes(&opf, "<itemref idref=\"")
        .into_iter()
        .filter_map(|id| hrefs.get(&id).cloned())
        .collect();

    let mut noterefs = Vec::new();
    for document in &spine {
        let markup = text(document);
        for tag in tags(&markup, "<a ") {
            if !tag.contains("epub:type=\"noteref\"") {
                continue;
            }
            let Some(href) = attribute(&tag, "href=\"") else {
                continue;
            };
            let (target, fragment) = match href.split_once('#') {
                Some(("", fragment)) => (document.clone(), fragment.to_owned()),
                Some((path, fragment)) => (resolve(document, path), fragment.to_owned()),
                None => continue,
            };
            noterefs.push(NoteRef {
                from: document.clone(),
                to: target,
                fragment,
            });
        }
    }

    Entry {
        fixture: stem.to_owned(),
        spine,
        noterefs,
    }
}

/// A path relative to `from`'s directory, as a container path.
fn resolve(from: &str, path: &str) -> String {
    match from.rfind('/') {
        Some(slash) => format!("{}/{path}", &from[..slash]),
        None => path.to_owned(),
    }
}

fn attribute(text: &str, prefix: &str) -> Option<String> {
    let start = text.find(prefix)? + prefix.len();
    let rest = &text[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_owned())
}

fn attributes(text: &str, prefix: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(prefix) {
        let after = &rest[start + prefix.len()..];
        match after.find('"') {
            Some(end) => {
                out.push(after[..end].to_owned());
                rest = &after[end..];
            }
            None => break,
        }
    }
    out
}

fn tags(text: &str, open: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(open) {
        let after = &rest[start..];
        match after.find('>') {
            Some(end) => {
                out.push(after[..=end].to_owned());
                rest = &after[end..];
            }
            None => break,
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). The one piece of resolution the manifest does,
// and the one a browser would do differently if it were wrong.
// ---------------------------------------------------------------------------

/// A note reference's target is resolved against the document it is in, because that is how a
/// browser resolves it — and a manifest that named `images/…` where the browser reads
/// `text/images/…` is exactly the `RSC-007` defect EPUBCheck found in Phase 5.
#[test]
fn a_note_reference_is_resolved_against_the_document_it_is_in() {
    assert_eq!(
        resolve("text/c0001.xhtml", "c0002.xhtml"),
        "text/c0002.xhtml"
    );
    assert_eq!(resolve("c0001.xhtml", "c0002.xhtml"), "c0002.xhtml");
}

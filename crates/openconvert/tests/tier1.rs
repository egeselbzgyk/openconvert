//! Phase 5: the Tier-1 validator, against real output and against crafted bad output.
//!
//! Both halves matter and neither is enough alone. A validator that passes everything we emit
//! and has never been shown a broken book has not been tested — it may be returning an empty
//! report. So every fixture goes through it clean, *and* a container is broken in one specific
//! way per test and the matching message id has to come back.

mod common;

use std::io::Write;

use oc_epub::EpubBytes;
use oc_validate::{validate_tier1, Expectations};

const FIXTURES: [&str; 10] = [
    "f01_prose_single_column",
    "f02_two_column",
    "f03_image_only",
    "f04_german_prose",
    "f05_turkish_prose",
    "f06_hyphenation_de",
    "f07_verse_and_quote",
    "f08_footnotes",
    "f09_novel_structure",
    "f10_lists_and_table",
];

/// Acceptance criterion A5.4: the bijection, the page-list resolution, the alt text and the
/// no-script rule all pass on everything the emitter produces.
#[test]
fn tier1_passes_on_every_fixture() {
    for stem in FIXTURES {
        let built = common::build(stem);
        let images = u32::try_from(built.built.emitted.used_images.len()).unwrap_or(u32::MAX);
        let report = validate_tier1(
            &built.built.bytes,
            &Expectations {
                images: Some(images),
            },
        );
        assert!(
            report.is_valid(),
            "{stem}: {:#?}",
            report
                .findings
                .iter()
                .filter(|finding| finding.severity >= oc_validate::Severity::Error)
                .collect::<Vec<_>>()
        );
        assert!(
            report.checked.len() >= 10,
            "{stem}: only {} checks ran",
            report.checked.len()
        );
    }
}

/// Row 5.10, the second half. Tier 1 reports the bijection as true on `f06`'s output, and
/// breaking one link makes it fail — a validator that always returned an empty report would
/// pass the first assertion and not the second.
#[test]
fn tier1_reports_the_bijection_and_notices_when_it_is_broken() {
    let built = common::build("f08_footnotes");
    assert!(validate_tier1(&built.built.bytes, &Expectations::default()).is_valid());

    // Re-point one reference at a note that does not exist.
    let broken = rewrite(&built, |path, text| {
        if path.starts_with("text/") {
            text.replacen("href=\"#fn0\"", "href=\"#fn99\"", 1)
        } else {
            text
        }
    });
    let report = validate_tier1(&broken, &Expectations::default());
    assert!(report.has("RSC-012"), "{:#?}", report.findings);
    assert!(
        report.has("OC-NOTE-BIJECTION"),
        "the note nothing now refers to is reported too: {:#?}",
        report.findings
    );
}

/// Row 5.15. A content document that is not well-formed XML — the RSC-005 class, and the one
/// EPUBCheck error a hand-rolled emitter is most likely to produce by accident.
#[test]
fn tier1_catches_rsc005_malformed_xml() {
    let built = common::build("f01_prose_single_column");
    let broken = rewrite(&built, |path, text| {
        if path.starts_with("text/") {
            text.replacen("</p>", "</span>", 1)
        } else {
            text
        }
    });

    let report = validate_tier1(&broken, &Expectations::default());
    assert!(report.has("RSC-005"), "{:#?}", report.findings);
    assert!(!report.is_valid());
}

/// Row 5.16. The `mimetype` entry not first and not stored: PKG-007, the classic hand-rolled
/// zip mistake, and the reason `write_deterministic_zip` exists rather than a default writer.
#[test]
fn tier1_catches_pkg007_mimetype() {
    let built = common::build("f01_prose_single_column");

    // A perfectly ordinary zip of the same files — every entry deflated, in insertion order.
    // This is what a default writer produces, and it is invalid.
    let mut bytes = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (path, content) in &built.entries {
            writer.start_file(path, options).expect("entry starts");
            writer.write_all(content).expect("entry writes");
        }
        writer.finish().expect("the archive closes");
    }

    let report = validate_tier1(&EpubBytes(bytes), &Expectations::default());
    assert!(report.has("PKG-007"), "{:#?}", report.findings);
    assert!(!report.is_valid());
}

/// An image that never arrived is the one extraction failure nothing else notices: the book
/// reads, the figure is simply not there (R1 §A.6).
#[test]
fn tier1_catches_an_image_that_did_not_arrive() {
    let built = common::build("f10_lists_and_table");
    let carried = u32::try_from(built.built.emitted.used_images.len()).unwrap_or(0);
    assert!(carried > 0, "f10 draws figures");

    let honest = validate_tier1(
        &built.built.bytes,
        &Expectations {
            images: Some(carried),
        },
    );
    assert!(honest.is_valid(), "{:#?}", honest.findings);

    let short = validate_tier1(
        &built.built.bytes,
        &Expectations {
            images: Some(carried + 1),
        },
    );
    assert!(short.has("OC-IMAGE-PARITY"), "{:#?}", short.findings);
}

/// An `<img>` whose alt text was emptied — the ACC-001 class. `ImgRef` makes it unrepresentable
/// upstream, so this is the check proving Tier 1 would catch a future emitter that forgot.
#[test]
fn tier1_catches_empty_alt_text() {
    let built = common::build("f10_lists_and_table");
    let broken = rewrite(&built, |path, text| {
        if path.starts_with("text/") {
            replace_alt(&text)
        } else {
            text
        }
    });

    let report = validate_tier1(&broken, &Expectations::default());
    assert!(report.has("ACC-001"), "{:#?}", report.findings);
}

/// A script, and the manifest not declaring it. Two findings from one injection, because they
/// are two different failures: a converted book has no business carrying a script at all, and
/// a manifest that does not describe its document is OPF-014 whatever the feature is.
#[test]
fn tier1_catches_a_script_and_the_property_that_was_not_declared() {
    let built = common::build("f01_prose_single_column");
    let broken = rewrite(&built, |path, text| {
        if path.starts_with("text/") {
            text.replacen("<body>", "<body><script>alert(1)</script>", 1)
        } else {
            text
        }
    });

    let report = validate_tier1(&broken, &Expectations::default());
    assert!(report.has("OC-SCRIPT"), "{:#?}", report.findings);
    assert!(report.has("OPF-014"), "{:#?}", report.findings);
}

/// The package document missing its `dcterms:modified` — one of the four EPUB 3.3 §5.5.3
/// MUST-haves, and the one an emitter is most likely to forget because nothing visibly breaks.
#[test]
fn tier1_catches_missing_required_metadata() {
    let built = common::build("f01_prose_single_column");
    let broken = rewrite(&built, |path, text| {
        if path == "content.opf" {
            text.lines()
                .filter(|line| !line.contains("dcterms:modified"))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            text
        }
    });

    let report = validate_tier1(&broken, &Expectations::default());
    assert!(!report.is_valid(), "{:#?}", report.findings);
    assert!(report.has("RSC-005"));
}

// ---------------------------------------------------------------------------
// Breaking a container in one specific way
// ---------------------------------------------------------------------------

/// Rebuild the container with every text entry passed through `edit`.
///
/// Through the real writer, so the archive stays a valid EPUB in every respect except the one
/// the test is about — otherwise a finding could be the consequence of the crafting rather than
/// of the defect.
fn rewrite(built: &common::Built, edit: impl Fn(&str, String) -> String) -> EpubBytes {
    let entries: Vec<oc_epub::zip::ZipEntry> = built
        .entries
        .iter()
        .map(|(path, bytes)| {
            let content = match std::str::from_utf8(bytes) {
                Ok(text) => edit(path, text.to_owned()).into_bytes(),
                Err(_) => bytes.clone(),
            };
            oc_epub::zip::ZipEntry::new(path.clone(), content)
        })
        .collect();
    EpubBytes(oc_epub::zip::write_deterministic_zip(&entries).expect("the container rewrites"))
}

/// Empty the first `alt` attribute in a document.
fn replace_alt(text: &str) -> String {
    let Some(start) = text.find("alt=\"") else {
        return text.to_owned();
    };
    let rest = &text[start + 5..];
    let Some(end) = rest.find('"') else {
        return text.to_owned();
    };
    format!("{}alt=\"\"{}", &text[..start], &rest[end + 1..])
}

/// A resource reference that resolves to nothing — the RSC-007 class, and the bug EPUBCheck
/// found in this emitter that Tier 1 did not: `images/i0001.jpg` written from
/// `text/c0001.xhtml` means `text/images/i0001.jpg`, which is not there. The fragment check
/// cannot see it, because the href has no fragment at all.
#[test]
fn tier1_catches_a_resource_reference_that_resolves_to_nothing() {
    let built = common::build("f10_lists_and_table");
    assert!(validate_tier1(&built.built.bytes, &Expectations::default()).is_valid());

    let broken = rewrite(&built, |path, text| {
        if path.starts_with("text/") {
            text.replace("src=\"../images/", "src=\"images/")
        } else {
            text
        }
    });

    let report = validate_tier1(&broken, &Expectations::default());
    assert!(report.has("RSC-007"), "{:#?}", report.findings);
    assert!(!report.is_valid());
}

/// A book that yielded no text is not an empty book. `f03` is one image-only page: the spine
/// must still carry a document, the nav must still have an entry, and the page must be in it as
/// a picture (PIPELINE §10). An empty spine is not a valid publication and is not a book.
#[test]
fn a_document_with_no_text_still_carries_its_pages() {
    let built = common::build("f03_image_only");

    assert!(
        !built.built.emitted.files.is_empty(),
        "the spine carries a document"
    );
    assert!(
        !built.built.emitted.toc.is_empty(),
        "the nav carries an entry"
    );
    assert!(
        !built.built.emitted.used_images.is_empty(),
        "the page is in the book as a picture"
    );
    assert!(validate_tier1(&built.built.bytes, &Expectations::default()).is_valid());

    assert!(
        built
            .conversion
            .document
            .warnings
            .iter()
            .any(|warning| warning.code == openconvert::document::W_NO_TEXT_EXTRACTED),
        "and the report says so"
    );
}

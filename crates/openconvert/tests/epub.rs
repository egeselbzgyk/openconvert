//! Phase 5: the container, on real fixtures.
//!
//! Every test here converts a PDF the whole way and then reads the *archive back*, rather than
//! asking the emitter what it wrote. That is the same argument D6 makes about Tier 1: an
//! emitter checked against its own intentions is checked against nothing, and the bugs worth
//! catching here — a manifest that does not describe the file, a nav pointing at an id that is
//! not in the document it names — are precisely the ones an emitter cannot see in itself.

mod common;

use std::collections::{BTreeMap, BTreeSet};

/// Every fixture the builder produces.
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

/// Row 5.4. Two builds of one book differ in `dcterms:modified` and in nothing else — which is
/// why the emitter takes the timestamp as an argument rather than reading a clock. With it held
/// still the two runs are the same bytes, and that is what makes a golden file and a cross-OS
/// gate possible at all (D13.8).
#[test]
fn zip_is_byte_identical_across_runs() {
    for stem in FIXTURES {
        let first = common::build(stem);
        let second = common::build(stem);
        assert_eq!(
            first.built.bytes, second.built.bytes,
            "{stem}: two builds of one book are not the same bytes"
        );
    }

    // And the redaction is honest: a different timestamp does change the bytes, so the test
    // above is not passing because nothing varies.
    let later = common::build_with(
        "f01_prose_single_column",
        oc_epub::EpubOptions {
            modified: "2027-06-30T12:00:00Z".to_owned(),
            ..common::epub_options()
        },
    );
    let now = common::build("f01_prose_single_column");
    assert_ne!(later.built.bytes, now.built.bytes);
}

/// Row 5.6. The EPUB 3.3 §5.5.3 MUST-set — `dc:identifier` with the `unique-identifier`
/// pointing at it, `dc:title`, `dc:language`, `dcterms:modified` — plus the accessibility
/// metadata that is computed from what the pipeline actually did.
#[test]
fn opf_has_all_required_metadata() {
    let built = common::build("f09_novel_structure");
    let opf = built.text_file("content.opf");

    assert!(opf.contains("unique-identifier=\"pub-id\""));
    assert!(opf.contains("<dc:identifier id=\"pub-id\">urn:uuid:"));
    assert!(opf.contains("<dc:title>A Short Novel</dc:title>"), "{opf}");
    assert!(opf.contains("<dc:language>en</dc:language>"));
    assert!(opf.contains(&format!(
        "<meta property=\"dcterms:modified\">{}</meta>",
        common::FIXED_MODIFIED
    )));

    // Computed, not asserted: `f09` has headings, so it claims structural navigation; it has
    // no images, so it claims neither a visual access mode nor alternative text.
    assert!(opf.contains("<meta property=\"schema:accessMode\">textual</meta>"));
    assert!(opf.contains("structuralNavigation"));
    assert!(!opf.contains("alternativeText"), "f09 has no images");
    assert!(opf.contains("<meta property=\"schema:accessibilityHazard\">none</meta>"));

    // Never auto-claim conformance (PIPELINE §9.6): `dcterms:conformsTo` *is* the WCAG claim
    // in EPUB Accessibility 1.1, and a converter cannot guarantee it from PDF source.
    assert!(!opf.contains("dcterms:conformsTo"));

    insta::assert_snapshot!(redact(&opf));
}

/// Row 5.8. `nav.xhtml` and `toc.ncx` are derived from one tree, so they can only disagree
/// through a bug — and a reader on older e-ink firmware navigates by the NCX and would meet
/// the disagreement without any way to report it.
#[test]
fn nav_and_ncx_agree() {
    for stem in FIXTURES {
        let built = common::build(stem);
        let nav = built.text_file("nav.xhtml");
        let ncx = built.text_file("toc.ncx");

        let nav_titles = between(&nav, "<nav epub:type=\"toc\"", "</nav>");
        let nav_entries: Vec<String> = anchors(nav_titles);
        let ncx_entries: Vec<String> = tags(&ncx, "<text>", "</text>")
            .into_iter()
            // The first `<text>` is the `docTitle`, which the nav does not have.
            .skip(1)
            .collect();

        assert_eq!(
            nav_entries, ncx_entries,
            "{stem}: the nav and the NCX list different entries, or in a different order"
        );
    }
}

/// Row 5.9. A `page-list` entry that does not resolve is a citation that goes nowhere, and it
/// is the whole point of carrying `PageBreak` through the IR at all (R1 §C.4 #7).
#[test]
fn page_list_targets_all_resolve() {
    for stem in FIXTURES {
        let built = common::build(stem);
        let ids = declared_ids(&built);

        for target in &built.built.emitted.page_list {
            let (file, anchor) = target
                .href
                .split_once('#')
                .unwrap_or((target.href.as_str(), ""));
            assert!(
                built.entries.contains_key(file),
                "{stem}: page-list points at {file}, which is not in the container"
            );
            assert!(
                ids.get(file).is_some_and(|ids| ids.contains(anchor)),
                "{stem}: page-list anchor #{anchor} is not defined in {file}"
            );
        }
    }
}

/// Row 5.10. The bijection, in the *output*: every `noteref` names a footnote that exists and
/// every footnote is named. Merely resolving the fragment is not enough — a book where two
/// references point at one note reads correctly and is wrong.
#[test]
fn noteref_footnote_bijection_in_output() {
    let built = common::build("f08_footnotes");

    let mut refs: Vec<String> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    for (_, markup) in built.content_documents() {
        refs.extend(
            attribute_values(
                &markup,
                "epub:type=\"noteref\" role=\"doc-noteref\" href=\"",
            )
            .into_iter()
            .map(|href| href.trim_start_matches('#').to_owned()),
        );
        notes.extend(attribute_values(
            &markup,
            "<aside epub:type=\"footnote\" role=\"doc-footnote\" id=\"",
        ));
    }

    assert!(!refs.is_empty(), "f08 prints footnote markers");
    assert_eq!(
        refs.iter().collect::<BTreeSet<_>>(),
        notes.iter().collect::<BTreeSet<_>>(),
        "the references and the footnotes are not the same set"
    );
    assert_eq!(refs.len(), refs.iter().collect::<BTreeSet<_>>().len());
    assert_eq!(notes.len(), notes.iter().collect::<BTreeSet<_>>().len());
}

/// Row 5.11. The bound is on the *file*, and a paragraph is never half of anything: splitting
/// happens between pieces, so there is no way for a `<p>` to span two files.
#[test]
fn split_happens_on_paragraph_boundary() {
    // A bound small enough that `f09` — a whole book in miniature — has to be split several
    // times. The real bound is 260 000 bytes and no fixture comes near it, so a test at the
    // real number would assert nothing.
    let built = common::build_with(
        "f09_novel_structure",
        oc_epub::EpubOptions {
            split_bytes: 900,
            ..common::epub_options()
        },
    );

    let documents = built.content_documents();
    assert!(documents.len() > 3, "the bound actually split something");

    for (path, markup) in &documents {
        let opens = markup.matches("<p").count();
        let closes = markup.matches("</p>").count();
        assert_eq!(opens, closes, "{path}: a paragraph is split across files");
    }

    // Every file is within the bound, or is a single piece that was already over it.
    for (path, markup) in &documents {
        assert!(
            markup.len() <= 900 + wrapper_slack(markup) || pieces_in(markup) == 1,
            "{path} is {} bytes",
            markup.len()
        );
    }
}

/// Row 5.13. Tier 1 requires every `<img>` to carry at least one non-space character of alt
/// text (the ACC-001 class), and `ImgRef` makes an empty one unrepresentable — this is the
/// same claim, made against the bytes that reached the container.
#[test]
fn img_alt_is_never_empty() {
    for stem in FIXTURES {
        let built = common::build(stem);
        for (path, markup) in built.content_documents() {
            for image in markup.match_indices("<img ") {
                let tag = &markup[image.0..];
                let tag = &tag[..tag.find("/>").map(|end| end + 2).unwrap_or(tag.len())];
                let alt = attribute_values(tag, "alt=\"");
                assert_eq!(alt.len(), 1, "{stem} {path}: an img with no alt: {tag}");
                assert!(
                    alt[0].chars().any(|ch| !ch.is_whitespace()),
                    "{stem} {path}: empty alt text"
                );
            }
        }
    }
}

/// Row 5.14. Never `<script>`, never a remote resource (D5). A script in a converted book has
/// no business being there, and a remote resource turns an offline reader into a broken one.
#[test]
fn no_script_no_remote_resources() {
    for stem in FIXTURES {
        let built = common::build(stem);
        for (path, markup) in built.content_documents() {
            assert!(!markup.contains("<script"), "{stem} {path}");
            for attribute in ["src=\"", "href=\""] {
                for value in attribute_values(&markup, attribute) {
                    assert!(
                        !value.starts_with("http://")
                            && !value.starts_with("https://")
                            && !value.starts_with("//"),
                        "{stem} {path}: remote resource {value}"
                    );
                }
            }
        }
        // And no DOCTYPE with an internal subset, which is where an entity expansion attack
        // would live (D14).
        for (_, markup) in built.content_documents() {
            assert!(markup.contains("<!DOCTYPE html>"));
            assert!(!markup.contains("<!ENTITY"));
        }
    }
}

/// Every manifest item exists in the container, every spine `itemref` is in the manifest, and
/// every internal href resolves. The emitter's own postconditions (PIPELINE §10).
#[test]
fn every_manifest_item_and_internal_href_resolves() {
    for stem in FIXTURES {
        let built = common::build(stem);
        let opf = built.text_file("content.opf");

        let hrefs = attribute_values(&opf, "href=\"");
        for href in &hrefs {
            assert!(
                built.entries.contains_key(href.as_str()),
                "{stem}: manifest names {href}, which is not in the container"
            );
        }

        let ids: BTreeSet<String> = attribute_values(&opf, "<item id=\"").into_iter().collect();
        for idref in attribute_values(&opf, "<itemref idref=\"") {
            assert!(ids.contains(&idref), "{stem}: spine names unknown {idref}");
        }

        let declared = declared_ids(&built);
        for (path, markup) in built.content_documents() {
            for href in attribute_values(&markup, "href=\"") {
                let Some(anchor) = href.strip_prefix('#') else {
                    continue;
                };
                assert!(
                    declared.get(&path).is_some_and(|ids| ids.contains(anchor)),
                    "{stem} {path}: #{anchor} is not defined in this document"
                );
            }
        }
    }
}

/// Row 5.20. The golden bytes, which is what a `zip` major bump has to be diffed against
/// (RT B12). Held as a sha256 rather than the archive itself: the point is that the bytes did
/// not change, and a 14 KB binary in the repository would be reviewed by nobody.
#[test]
fn golden_epub_bytes_f01() {
    let built = common::build("f01_prose_single_column");
    insta::assert_snapshot!(format!(
        "sha256={}\nentries={:?}\nbytes={}",
        common::sha256_hex_of(built.built.bytes.as_slice()),
        built.paths(),
        built.built.bytes.len()
    ));
}

/// The stylesheet reaches the container and the content documents point at it. A book whose
/// `<link>` resolves to nothing looks like a plain-text dump, which is the failure a reader
/// notices first and a validator does not notice at all.
#[test]
fn the_stylesheet_is_in_the_container_and_every_document_points_at_it() {
    let built = common::build("f07_verse_and_quote");
    assert!(built.entries.contains_key("style.css"));
    for (path, markup) in built.content_documents() {
        assert!(
            markup.contains("href=\"../style.css\""),
            "{path} does not link the stylesheet"
        );
    }
    assert!(built.text_file("nav.xhtml").contains("href=\"style.css\""));
}

// ---------------------------------------------------------------------------
// Reading the output back
// ---------------------------------------------------------------------------

/// Every `id="…"` declared in each content document, plus the nav.
fn declared_ids(built: &common::Built) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (path, markup) in built.content_documents() {
        out.insert(
            path,
            attribute_values(&markup, "id=\"").into_iter().collect(),
        );
    }
    out.insert(
        "nav.xhtml".to_owned(),
        attribute_values(&built.text_file("nav.xhtml"), "id=\"")
            .into_iter()
            .collect(),
    );
    out
}

/// Every value of an attribute, by its literal `name="` prefix.
fn attribute_values(markup: &str, prefix: &str) -> Vec<String> {
    markup
        .match_indices(prefix)
        .filter_map(|(index, _)| {
            let rest = &markup[index + prefix.len()..];
            rest.find('"').map(|end| rest[..end].to_owned())
        })
        .collect()
}

/// The text between two literal markers, or the whole string when the open marker is absent.
fn between<'a>(markup: &'a str, open: &str, close: &str) -> &'a str {
    let Some(start) = markup.find(open) else {
        return markup;
    };
    let rest = &markup[start..];
    match rest.find(close) {
        Some(end) => &rest[..end],
        None => rest,
    }
}

/// Every anchor's text, in order.
fn anchors(markup: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = markup;
    while let Some(start) = rest.find("<a ") {
        rest = &rest[start..];
        let Some(open_end) = rest.find('>') else {
            break;
        };
        rest = &rest[open_end + 1..];
        let Some(end) = rest.find("</a>") else {
            break;
        };
        out.push(rest[..end].trim().to_owned());
        rest = &rest[end..];
    }
    out
}

/// Every stretch between an open and a close marker, in order.
fn tags(markup: &str, open: &str, close: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = markup;
    while let Some(start) = rest.find(open) {
        rest = &rest[start + open.len()..];
        let Some(end) = rest.find(close) else {
            break;
        };
        out.push(rest[..end].to_owned());
        rest = &rest[end..];
    }
    out
}

/// How many bytes of a file are the XHTML wrapper rather than its pieces.
fn wrapper_slack(markup: &str) -> usize {
    markup
        .find("<body>")
        .map(|index| index + "</body>\n</html>\n".len() + 200)
        .unwrap_or(600)
}

/// A crude count of the indivisible pieces in one file, for the "already over the bound" case.
fn pieces_in(markup: &str) -> usize {
    markup.matches("<p").count() + markup.matches("<figure").count() + markup.matches("<ol").count()
}

/// The `dcterms:modified` line and the identifier, redacted, so the snapshot is about the
/// shape of the package and not about when it was built.
fn redact(opf: &str) -> String {
    opf.lines()
        .map(|line| {
            if line.contains("dcterms:modified") {
                "<meta property=\"dcterms:modified\">[redacted]</meta>".to_owned()
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

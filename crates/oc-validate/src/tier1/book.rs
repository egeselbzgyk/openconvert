//! The checks a PDF-derived book specifically needs, which no generic validator has (RT A6).
//!
//! Four of the five are about a link that exists and points nowhere, which is the failure mode
//! a *generated* book has and a hand-authored one does not: nothing here is a typo, everything
//! here is a pipeline that lost one end of a pair. The fifth — image-count parity — is about a
//! resource that quietly never arrived: Marker loses about 14 % of images on some documents
//! with no error and no log line (R1 §A.6), and the only defence against that class is to count
//! in and count out.

use std::collections::{BTreeMap, BTreeSet};

use super::opf::Package;
use super::{Expectations, Finding, Severity, Tier1Report};

/// Run the book-specific checks.
pub fn check(
    entries: &BTreeMap<String, Vec<u8>>,
    package: &Package,
    expected: &Expectations,
    report: &mut Tier1Report,
) {
    let documents: Vec<(String, String)> = package
        .xhtml_paths()
        .into_iter()
        .filter_map(|path| {
            entries
                .get(&path)
                .map(|bytes| (path, String::from_utf8_lossy(bytes).into_owned()))
        })
        .collect();

    report.ran("book.note_bijection");
    bijection(&documents, report);

    report.ran("book.fragments");
    fragments(&documents, report);

    report.ran("book.resources");
    resources(entries, &documents, report);

    report.ran("book.alt_text");
    alt_text(&documents, report);

    report.ran("book.image_parity");
    image_parity(package, expected, report);
}

/// `noteref` ↔ `footnote` is a **bijection**, not merely a resolving fragment.
///
/// Two references pointing at one note read correctly and are wrong; a note nothing points at
/// is a detection failure hiding behind a link that happens to resolve. PIPELINE §8.3 asserts
/// the pairing upstream; this asserts it survived serialisation.
fn bijection(documents: &[(String, String)], report: &mut Tier1Report) {
    let mut refs: Vec<(String, String)> = Vec::new();
    let mut notes: Vec<(String, String)> = Vec::new();

    for (path, text) in documents {
        for tag in super::xhtml::tags(text, "<a ") {
            if !tag.contains("epub:type=\"noteref\"") {
                continue;
            }
            if let Some(href) = super::xhtml::attribute(&tag, "href=\"") {
                refs.push((path.clone(), href.trim_start_matches('#').to_owned()));
            }
        }
        for tag in super::xhtml::tags(text, "<aside ") {
            if !tag.contains("epub:type=\"footnote\"") {
                continue;
            }
            if let Some(id) = super::xhtml::attribute(&tag, "id=\"") {
                notes.push((path.clone(), id));
            }
        }
    }

    let referenced: BTreeSet<&String> = refs.iter().map(|(_, id)| id).collect();
    let defined: BTreeSet<&String> = notes.iter().map(|(_, id)| id).collect();

    if refs.len() != referenced.len() {
        report.push(Finding::new(
            "OC-NOTE-BIJECTION",
            Severity::Error,
            "",
            format!(
                "{} note references point at {} distinct notes",
                refs.len(),
                referenced.len()
            ),
        ));
    }
    for (path, id) in &refs {
        if !defined.contains(id) {
            report.push(Finding::new(
                "RSC-012",
                Severity::Error,
                format!("{path}#{id}"),
                "a note reference points at a footnote that does not exist",
            ));
        }
    }
    for (path, id) in &notes {
        if !referenced.contains(id) {
            report.push(Finding::new(
                "OC-NOTE-BIJECTION",
                Severity::Error,
                format!("{path}#{id}"),
                "a footnote that nothing refers to",
            ));
        }
    }
}

/// RSC-012: every fragment reference resolves — inside the document it names, and for a
/// cross-document href, inside the document it points at.
///
/// This is what the `page-list` check is: a page-list entry is an href into a content document,
/// and one that does not resolve is a citation that goes nowhere.
fn fragments(documents: &[(String, String)], report: &mut Tier1Report) {
    let ids: BTreeMap<&String, BTreeSet<String>> = documents
        .iter()
        .map(|(path, text)| {
            (
                path,
                super::xhtml::attributes(text, "id=\"")
                    .into_iter()
                    .collect(),
            )
        })
        .collect();

    for (path, text) in documents {
        let directory = path.rsplit_once('/').map(|(head, _)| head).unwrap_or("");
        for href in super::xhtml::attributes(text, "href=\"") {
            let Some((file, anchor)) = split_href(&href) else {
                continue;
            };
            let target = match file {
                "" => path.clone(),
                relative if directory.is_empty() => relative.to_owned(),
                relative => format!("{directory}/{relative}"),
            };
            let resolved = ids
                .iter()
                .find(|(known, _)| ***known == target)
                .is_some_and(|(_, ids)| ids.contains(anchor));
            if !resolved {
                report.push(Finding::new(
                    "RSC-012",
                    Severity::Error,
                    format!("{path} → {href}"),
                    "the fragment does not resolve to an id in the document it names",
                ));
            }
        }
    }
}

/// RSC-007: every resource a document names is in the container, at the path the document
/// spells it — resolved *relative to the document*, which is where this goes wrong.
///
/// `images/i0001.jpg` written from `text/c0001.xhtml` means `text/images/i0001.jpg`, and a
/// reader looking for that finds nothing and shows a broken figure. The fragment check above
/// would not notice: the href has no fragment at all.
fn resources(
    entries: &BTreeMap<String, Vec<u8>>,
    documents: &[(String, String)],
    report: &mut Tier1Report,
) {
    for (path, text) in documents {
        for attribute in ["src=\"", "href=\""] {
            for reference in super::xhtml::attributes(text, attribute) {
                let Some(target) = resolve(path, &reference) else {
                    continue;
                };
                if !entries.contains_key(&target) {
                    report.push(Finding::new(
                        "RSC-007",
                        Severity::Error,
                        format!("{path} → {reference}"),
                        format!("resolves to {target}, which is not in the container"),
                    ));
                }
            }
        }
    }
}

/// A reference resolved against the document that made it, or `None` when it is not a path
/// into this container — remote, a bare fragment, or a scheme of its own.
fn resolve(from: &str, reference: &str) -> Option<String> {
    if reference.starts_with('#')
        || reference.starts_with("http://")
        || reference.starts_with("https://")
        || reference.starts_with("//")
        || reference.starts_with("data:")
        || reference.starts_with("mailto:")
    {
        return None;
    }
    let path = reference.split('#').next().unwrap_or(reference);
    if path.is_empty() {
        return None;
    }

    let mut segments: Vec<&str> = from.split('/').collect();
    segments.pop();
    for segment in path.split('/') {
        match segment {
            "." | "" => {}
            ".." => {
                segments.pop()?;
            }
            other => segments.push(other),
        }
    }
    Some(segments.join("/"))
}

/// The file and the fragment of an href, when it has a fragment and is not remote.
fn split_href(href: &str) -> Option<(&str, &str)> {
    if href.starts_with("http://") || href.starts_with("https://") || href.starts_with("//") {
        return None;
    }
    href.split_once('#')
}

/// ACC-001: every `<img>` carries at least one non-space character of alt text.
///
/// `alt=""` is EPUB's way of marking an image decorative, and a converter that wrote it for
/// every unlabelled figure would be declaring a book's illustrations decorative on no evidence.
fn alt_text(documents: &[(String, String)], report: &mut Tier1Report) {
    for (path, text) in documents {
        for tag in super::xhtml::tags(text, "<img ") {
            match super::xhtml::attribute(&tag, "alt=\"") {
                Some(alt) if alt.chars().any(|ch| !ch.is_whitespace()) => {}
                _ => report.push(Finding::new(
                    "ACC-001",
                    Severity::Error,
                    path,
                    format!("an img with no alt text: {tag}"),
                )),
            }
        }
    }
}

/// Count in, count out. An image that silently never arrived is the one extraction failure
/// nothing else in the system would notice (R1 §A.6).
fn image_parity(package: &Package, expected: &Expectations, report: &mut Tier1Report) {
    let Some(expected) = expected.images else {
        return;
    };
    let carried = package
        .manifest
        .values()
        .filter(|item| item.media_type.starts_with("image/"))
        .count();
    if u32::try_from(carried).unwrap_or(u32::MAX) != expected {
        report.push(Finding::new(
            "OC-IMAGE-PARITY",
            Severity::Error,
            &package.path,
            format!("extraction produced {expected} images; the container carries {carried}"),
        ));
    }
}

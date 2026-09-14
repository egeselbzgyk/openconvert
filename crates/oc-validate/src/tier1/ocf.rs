//! The container: the `mimetype` entry, `META-INF/container.xml`, and entry names.

use std::collections::BTreeMap;

use oc_epub::zip::{CONTAINER_PATH, EPUB_MEDIA_TYPE, MIMETYPE_NAME_OFFSET, MIMETYPE_PATH};
use oc_epub::EpubBytes;

use super::{Finding, Severity, Tier1Report};

/// Check the OCF layer.
pub fn check(epub: &EpubBytes, entries: &BTreeMap<String, Vec<u8>>, report: &mut Tier1Report) {
    report.ran("ocf.mimetype");
    mimetype(epub, entries, report);

    report.ran("ocf.container");
    container(entries, report);

    report.ran("ocf.case_collisions");
    case_collisions(entries, report);
}

/// PKG-007: `mimetype` must be the first entry, stored, with no extra field, so that a reader
/// can identify the file by reading bytes 30..38 without parsing the archive.
fn mimetype(epub: &EpubBytes, entries: &BTreeMap<String, Vec<u8>>, report: &mut Tier1Report) {
    let bytes = epub.as_slice();
    let name_end = MIMETYPE_NAME_OFFSET + MIMETYPE_PATH.len();

    let first_is_mimetype = bytes
        .get(MIMETYPE_NAME_OFFSET..name_end)
        .is_some_and(|name| name == MIMETYPE_PATH.as_bytes());
    let stored = bytes
        .get(8..10)
        .is_some_and(|method| u16::from_le_bytes([method[0], method[1]]) == 0);
    let no_extra = bytes
        .get(28..30)
        .is_some_and(|extra| u16::from_le_bytes([extra[0], extra[1]]) == 0);

    if !first_is_mimetype || !stored || !no_extra {
        report.push(Finding::new(
            "PKG-007",
            Severity::Error,
            MIMETYPE_PATH,
            "the mimetype entry must come first, be stored uncompressed and carry no extra field",
        ));
        return;
    }

    match entries.get(MIMETYPE_PATH) {
        Some(content) if content == EPUB_MEDIA_TYPE.as_bytes() => {}
        Some(content) => report.push(Finding::new(
            "PKG-007",
            Severity::Error,
            MIMETYPE_PATH,
            format!(
                "the mimetype entry reads {:?}, not {EPUB_MEDIA_TYPE}",
                String::from_utf8_lossy(content)
            ),
        )),
        None => report.push(Finding::new(
            "PKG-007",
            Severity::Fatal,
            MIMETYPE_PATH,
            "the container has no mimetype entry",
        )),
    }
}

/// `META-INF/container.xml` must exist, parse, and name a package document that is in the
/// container.
fn container(entries: &BTreeMap<String, Vec<u8>>, report: &mut Tier1Report) {
    let Some(bytes) = entries.get(CONTAINER_PATH) else {
        report.push(Finding::new(
            "RSC-002",
            Severity::Fatal,
            CONTAINER_PATH,
            "the container descriptor is missing",
        ));
        return;
    };
    let text = String::from_utf8_lossy(bytes).into_owned();

    if let Err(error) = super::xhtml::well_formed(&text) {
        report.push(Finding::new(
            "RSC-005",
            Severity::Fatal,
            CONTAINER_PATH,
            format!("the container descriptor is not well-formed XML: {error}"),
        ));
        return;
    }

    match super::xhtml::attribute(&text, "full-path=\"") {
        Some(path) if entries.contains_key(&path) => {}
        Some(path) => report.push(Finding::new(
            "RSC-007",
            Severity::Fatal,
            CONTAINER_PATH,
            format!("the rootfile {path} is not in the container"),
        )),
        None => report.push(Finding::new(
            "RSC-005",
            Severity::Fatal,
            CONTAINER_PATH,
            "the container descriptor names no rootfile",
        )),
    }
}

/// Two entry names that differ only in case unpack to one file on Windows and macOS, so one of
/// the two chapters silently does not exist — with no error from any reading system.
fn case_collisions(entries: &BTreeMap<String, Vec<u8>>, report: &mut Tier1Report) {
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for name in entries.keys() {
        let folded = name.to_lowercase();
        match seen.get(&folded) {
            Some(other) => report.push(Finding::new(
                "OPF-060",
                Severity::Error,
                name,
                format!("{name} and {other} collide under a case-insensitive filesystem"),
            )),
            None => {
                seen.insert(folded, name.clone());
            }
        }
    }
}

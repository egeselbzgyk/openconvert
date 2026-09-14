//! Content documents: well-formedness, declared `properties`, and what must never be there.
//!
//! Also the small string helpers the rest of Tier 1 reads markup with. They are deliberately
//! literal rather than a second XML parser: the checks below either ask "is this well-formed"
//! — which is a parse — or "does this attribute say what the manifest says it says", which is
//! a substring on bytes that have already been proved well-formed.

use std::collections::BTreeMap;

use quick_xml::events::Event;
use quick_xml::Reader;

use super::opf::Package;
use super::{Finding, Severity, Tier1Report};

/// Check every content document.
pub fn check(entries: &BTreeMap<String, Vec<u8>>, package: &Package, report: &mut Tier1Report) {
    report.ran("xhtml.well_formed");
    report.ran("xhtml.properties");
    report.ran("xhtml.no_script_no_remote");
    report.ran("xhtml.no_entities");

    for path in package.xhtml_paths() {
        let Some(bytes) = entries.get(&path) else {
            continue;
        };
        let text = String::from_utf8_lossy(bytes).into_owned();

        if let Err(error) = well_formed(&text) {
            report.push(Finding::new(
                "RSC-005",
                Severity::Error,
                &path,
                format!("not well-formed XML: {error}"),
            ));
            // Everything below reads the markup; on a document that does not parse, each
            // check would report a consequence of the same failure.
            continue;
        }

        properties(&text, &path, package, report);
        forbidden(&text, &path, report);
    }
}

/// OPF-014: the manifest must declare every property the document actually uses.
///
/// Read off the bytes on both sides — what the document contains against what the manifest
/// says — because an emitter that computed the manifest from its own intent would agree with
/// itself and OPF-014 is a top error precisely because emitters do that.
fn properties(text: &str, path: &str, package: &Package, report: &mut Tier1Report) {
    let declared = package
        .manifest
        .values()
        .find(|item| item.href == path)
        .map(|item| item.properties.clone())
        .unwrap_or_default();

    for (needle, property) in [
        ("<svg", "svg"),
        ("<math", "mathml"),
        ("<script", "scripted"),
    ] {
        if text.contains(needle) && !declared.iter().any(|value| value == property) {
            report.push(Finding::new(
                "OPF-014",
                Severity::Error,
                path,
                format!(
                    "the document uses {needle}…> but the manifest does not declare {property}"
                ),
            ));
        }
    }

    if has_remote(text) && !declared.iter().any(|value| value == "remote-resources") {
        report.push(Finding::new(
            "OPF-014",
            Severity::Error,
            path,
            "the document loads a remote resource but the manifest does not declare it",
        ));
    }
}

/// What a converted book must never contain (D5, D14).
fn forbidden(text: &str, path: &str, report: &mut Tier1Report) {
    if text.contains("<script") {
        report.push(Finding::new(
            "OC-SCRIPT",
            Severity::Error,
            path,
            "v1 never emits a script",
        ));
    }
    if has_remote(text) {
        report.push(Finding::new(
            "OC-REMOTE",
            Severity::Error,
            path,
            "v1 never emits a remote resource",
        ));
    }
    // An internal subset with an entity declaration is the billion-laughs shape, and EPUB
    // allows only the bare `<!DOCTYPE html>`.
    if text.contains("<!ENTITY") {
        report.push(Finding::new(
            "OC-ENTITY",
            Severity::Error,
            path,
            "a DOCTYPE entity declaration is not allowed",
        ));
    }
}

/// Whether the markup loads anything over a network.
///
/// On `src=` and `href=` only: a URL in the *text* of a book is something the author wrote, not
/// a resource the document fetches.
pub fn has_remote(text: &str) -> bool {
    ["src=\"", "href=\""].iter().any(|attribute| {
        text.match_indices(attribute).any(|(index, _)| {
            let value = &text[index + attribute.len()..];
            ["http://", "https://", "//"]
                .iter()
                .any(|scheme| value.starts_with(scheme))
        })
    })
}

/// Whether a document parses as XML at all. The RSC-005 class.
pub fn well_formed(text: &str) -> Result<(), quick_xml::Error> {
    let mut reader = Reader::from_str(text);
    reader.config_mut().check_end_names = true;
    loop {
        match reader.read_event()? {
            Event::Eof => return Ok(()),
            _ => continue,
        }
    }
}

// ---------------------------------------------------------------------------
// Reading markup
// ---------------------------------------------------------------------------

/// The value of the first attribute with this literal `name="` prefix.
pub fn attribute(text: &str, prefix: &str) -> Option<String> {
    let index = text.find(prefix)?;
    let rest = &text[index + prefix.len()..];
    rest.find('"').map(|end| rest[..end].to_owned())
}

/// Every value of an attribute with this literal prefix, in document order.
pub fn attributes(text: &str, prefix: &str) -> Vec<String> {
    text.match_indices(prefix)
        .filter_map(|(index, _)| {
            let rest = &text[index + prefix.len()..];
            rest.find('"').map(|end| rest[..end].to_owned())
        })
        .collect()
}

/// Every element that starts with this literal opening, up to its `>`.
pub fn tags(text: &str, open: &str) -> Vec<String> {
    text.match_indices(open)
        .filter_map(|(index, _)| {
            let rest = &text[index..];
            rest.find('>').map(|end| rest[..=end].to_owned())
        })
        .collect()
}

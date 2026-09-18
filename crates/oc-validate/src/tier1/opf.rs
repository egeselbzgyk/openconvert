//! The package document: required metadata, manifest↔spine integrity, media types.

use std::collections::{BTreeMap, BTreeSet};

use oc_epub::zip::CONTAINER_PATH;

use super::{Finding, Severity, Tier1Report};

/// The package document, as much of it as the other checks need.
pub struct Package {
    pub path: String,
    /// Manifest id → (href, media-type, properties).
    pub manifest: BTreeMap<String, Item>,
    /// Spine `itemref` ids, in order.
    pub spine: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub href: String,
    pub media_type: String,
    pub properties: Vec<String>,
}

impl Package {
    /// Every content document, in spine order, as a path inside the container.
    pub fn spine_paths(&self) -> Vec<String> {
        self.spine
            .iter()
            .filter_map(|id| self.manifest.get(id))
            .map(|item| item.href.clone())
            .collect()
    }

    /// Every manifest item that is a content document, spine or not — the nav belongs to this
    /// set and is not in the spine.
    pub fn xhtml_paths(&self) -> Vec<String> {
        self.manifest
            .values()
            .filter(|item| item.media_type == "application/xhtml+xml")
            .map(|item| item.href.clone())
            .collect()
    }
}

/// Read the package document and nothing else — no checks, no findings.
///
/// The structural validator needs the spine in order to know which documents carry `C(EPUB)`,
/// and it is not in the business of reporting OPF defects: that is Tier 1's, which runs first.
/// `None` when there is no reachable package document.
pub fn parse(entries: &BTreeMap<String, Vec<u8>>) -> Option<Package> {
    let container = entries.get(CONTAINER_PATH)?;
    let path = super::xhtml::attribute(&String::from_utf8_lossy(container), "full-path=\"")?;
    let text = String::from_utf8_lossy(entries.get(&path)?).into_owned();
    Some(Package {
        manifest: manifest(&text),
        spine: super::xhtml::attributes(&text, "<itemref idref=\""),
        path,
    })
}

/// Parse and check the package document. `None` when it could not be read at all, in which
/// case every later check would be reporting the same failure again.
pub fn check(entries: &BTreeMap<String, Vec<u8>>, report: &mut Tier1Report) -> Option<Package> {
    report.ran("opf.parse");

    let package = parse(entries)?;
    let path = package.path.clone();
    let text = String::from_utf8_lossy(entries.get(&path)?).into_owned();

    if let Err(error) = super::xhtml::well_formed(&text) {
        report.push(Finding::new(
            "RSC-005",
            Severity::Fatal,
            &path,
            format!("the package document is not well-formed XML: {error}"),
        ));
        return None;
    }

    report.ran("opf.metadata");
    metadata(&text, &path, report);

    report.ran("opf.integrity");
    integrity(entries, &package, report);

    Some(package)
}

/// EPUB 3.3 §5.5.3's MUST-set: `dc:identifier` with `unique-identifier` pointing at it,
/// `dc:title`, `dc:language`, `dcterms:modified`.
fn metadata(text: &str, path: &str, report: &mut Tier1Report) {
    let unique = super::xhtml::attribute(text, "unique-identifier=\"");
    let identifiers = super::xhtml::attributes(text, "<dc:identifier id=\"");

    match unique {
        Some(id) if identifiers.contains(&id) => {}
        Some(id) => report.push(Finding::new(
            "OPF-030",
            Severity::Error,
            path,
            format!("unique-identifier names {id}, which is not a dc:identifier in this package"),
        )),
        None => report.push(Finding::new(
            "OPF-030",
            Severity::Error,
            path,
            "the package declares no unique-identifier",
        )),
    }

    for (element, id) in [
        ("<dc:title", "RSC-005"),
        ("<dc:language", "RSC-005"),
        ("<dc:identifier", "RSC-005"),
    ] {
        if !text.contains(element) {
            report.push(Finding::new(
                id,
                Severity::Error,
                path,
                format!("the package has no {}>", element.trim_start_matches('<')),
            ));
        }
    }

    if !text.contains("property=\"dcterms:modified\"") {
        report.push(Finding::new(
            "RSC-005",
            Severity::Error,
            path,
            "the package has no dcterms:modified",
        ));
    }
}

/// Every manifest item is in the container; every spine `itemref` is in the manifest; every
/// manifest item is reachable; media types match their extensions; exactly one `nav`.
fn integrity(entries: &BTreeMap<String, Vec<u8>>, package: &Package, report: &mut Tier1Report) {
    for (id, item) in &package.manifest {
        if !entries.contains_key(&item.href) {
            report.push(Finding::new(
                "RSC-007",
                Severity::Error,
                &package.path,
                format!(
                    "manifest item {id} names {}, which is not in the container",
                    item.href
                ),
            ));
        }
        if let Some(expected) = media_type_for(&item.href) {
            if item.media_type != expected {
                report.push(Finding::new(
                    "OPF-012",
                    Severity::Error,
                    &item.href,
                    format!(
                        "declared media-type {} does not match the extension (expected {expected})",
                        item.media_type
                    ),
                ));
            }
        }
    }

    let ids: BTreeSet<&String> = package.manifest.keys().collect();
    for idref in &package.spine {
        if !ids.contains(idref) {
            report.push(Finding::new(
                "RSC-005",
                Severity::Error,
                &package.path,
                format!("the spine names {idref}, which is not in the manifest"),
            ));
        }
    }

    let nav_items = package
        .manifest
        .values()
        .filter(|item| item.properties.iter().any(|property| property == "nav"))
        .count();
    if nav_items != 1 {
        report.push(Finding::new(
            "RSC-005",
            Severity::Error,
            &package.path,
            format!(
                "a package has exactly one item with the nav property; this one has {nav_items}"
            ),
        ));
    }

    // An item nothing points at is dead weight the reader downloads: OPF-003's class. A
    // warning rather than an error, because it breaks nothing — but it is always a bug here,
    // since the emitter lists only what it referenced.
    let referenced: BTreeSet<String> = package.spine_paths().into_iter().collect();
    for (id, item) in &package.manifest {
        let reachable = referenced.contains(&item.href)
            || item.properties.iter().any(|property| property == "nav")
            || item.media_type == "application/x-dtbncx+xml"
            || is_referenced_from_documents(entries, package, &item.href);
        if !reachable {
            report.push(Finding::new(
                "OPF-003",
                Severity::Warning,
                &item.href,
                format!("manifest item {id} is not referenced from any document"),
            ));
        }
    }
}

/// Whether any content document names this resource.
fn is_referenced_from_documents(
    entries: &BTreeMap<String, Vec<u8>>,
    package: &Package,
    href: &str,
) -> bool {
    let name = href.rsplit('/').next().unwrap_or(href);
    package.xhtml_paths().iter().any(|path| {
        entries
            .get(path)
            .map(|bytes| String::from_utf8_lossy(bytes).contains(name))
            .unwrap_or(false)
    })
}

/// The media type an extension implies, for the ones the emitter can produce.
fn media_type_for(href: &str) -> Option<&'static str> {
    let extension = href.rsplit('.').next()?.to_ascii_lowercase();
    match extension.as_str() {
        "xhtml" => Some("application/xhtml+xml"),
        "css" => Some("text/css"),
        "ncx" => Some("application/x-dtbncx+xml"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "opf" => Some("application/oebps-package+xml"),
        _ => None,
    }
}

/// The manifest, by id.
fn manifest(text: &str) -> BTreeMap<String, Item> {
    let mut out = BTreeMap::new();
    for tag in super::xhtml::tags(text, "<item ") {
        let Some(id) = super::xhtml::attribute(&tag, "id=\"") else {
            continue;
        };
        let Some(href) = super::xhtml::attribute(&tag, "href=\"") else {
            continue;
        };
        let media_type = super::xhtml::attribute(&tag, "media-type=\"").unwrap_or_default();
        let properties = super::xhtml::attribute(&tag, "properties=\"")
            .map(|value| {
                value
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        out.insert(
            id,
            Item {
                href,
                media_type,
                properties,
            },
        );
    }
    out
}

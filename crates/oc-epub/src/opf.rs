//! The package document (R5 §A2, §A3, §A12).
//!
//! Three things in here are the ones generated EPUBs get wrong.
//!
//! **Manifest `properties` are computed from the serialised bytes, not from intent** (OPF-014
//! is a top auto-generated-EPUB error). The emitter believes it never writes inline SVG and
//! never writes a script; the manifest says what is actually in the file it is describing, so
//! that a future change to the emitter cannot make the manifest a lie.
//!
//! **The accessibility metadata is computed from what the pipeline actually did** (PIPELINE
//! §9.6). `structuralNavigation` only when a real heading-based nav exists; `alternativeText`
//! only when alt text exists; `printPageNumbers` only when the page list has entries. The
//! converter knows all three from its own run, so saying them honestly is nearly free, and a
//! claim it cannot support would be worse than silence.
//!
//! **`dcterms:conformsTo` is not emitted.** In EPUB Accessibility 1.1 that property *is* the
//! WCAG conformance claim, and PIPELINE §9.6 is explicit: never auto-claim conformance, because
//! the tool cannot guarantee it from PDF source. IMPLEMENTATION_PLAN Phase 5 detail 2 lists it
//! among the required metadata; the authority order puts PIPELINE above the plan, and a false
//! conformance claim is a worse defect than a missing optional property.

use oc_model::document::Document;

use crate::content::{Emitted, XhtmlFile};
use crate::images::EncodedImage;
use crate::xhtml::escape;

/// Where the package document lives inside the container.
pub const OPF_PATH: &str = "content.opf";

/// The id of the `dc:identifier` the package is `unique-identifier`-ed by.
const PUB_ID: &str = "pub-id";

/// The manifest id of the navigation document.
pub const NAV_ID: &str = "nav";

/// The manifest id of the legacy NCX.
pub const NCX_ID: &str = "ncx";

/// Everything the package document needs that is not in the [`Document`].
pub struct PackageInput<'a> {
    pub emitted: &'a Emitted,
    pub images: &'a [EncodedImage],
    pub nav_path: &'a str,
    pub ncx_path: &'a str,
    pub style_path: &'a str,
    /// `dcterms:modified`, as `YYYY-MM-DDThh:mm:ssZ`. Passed in rather than read from a clock
    /// so that a test can hold it still: it is the one field that is different in two builds
    /// of the same book, and test 5.4 redacts exactly it.
    pub modified: String,
}

/// Serialise `content.opf`.
pub fn package(document: &Document, input: &PackageInput<'_>) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" \
         unique-identifier=\"{PUB_ID}\" xml:lang=\"{}\">\n",
        escape::attribute(document.language.as_str())
    ));

    out.push_str(&metadata(document, input));
    out.push_str(&manifest(input));
    out.push_str(&spine(input));

    out.push_str("</package>\n");
    out
}

fn metadata(document: &Document, input: &PackageInput<'_>) -> String {
    let meta = &document.meta;
    let mut out = String::from(
        "<metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\" \
         xmlns:dcterms=\"http://purl.org/dc/terms/\">\n",
    );

    out.push_str(&format!(
        "<dc:identifier id=\"{PUB_ID}\">{}</dc:identifier>\n",
        escape::text(&meta.identifier)
    ));
    // A book with no title still needs one: `dc:title` is required and must not be empty, and
    // the file's own name is the last thing `structure` falls back to, so an empty title here
    // means even that was empty.
    let title = meta.title.clone().unwrap_or_else(|| "Untitled".to_owned());
    out.push_str(&format!("<dc:title>{}</dc:title>\n", escape::text(&title)));
    out.push_str(&format!(
        "<dc:language>{}</dc:language>\n",
        escape::text(document.language.as_str())
    ));
    out.push_str(&format!(
        "<meta property=\"dcterms:modified\">{}</meta>\n",
        escape::text(&input.modified)
    ));

    for author in &meta.authors {
        out.push_str(&format!(
            "<dc:creator>{}</dc:creator>\n",
            escape::text(author)
        ));
    }
    if let Some(translator) = &meta.translator {
        out.push_str(&format!(
            "<dc:contributor>{}</dc:contributor>\n",
            escape::text(translator)
        ));
    }
    if let Some(publisher) = &meta.publisher {
        out.push_str(&format!(
            "<dc:publisher>{}</dc:publisher>\n",
            escape::text(publisher)
        ));
    }
    if let Some(date) = &meta.date {
        out.push_str(&format!("<dc:date>{}</dc:date>\n", escape::text(date)));
    }
    if let Some(subtitle) = &meta.subtitle {
        // A second `dc:title` refined as a subtitle, which is how EPUB 3 says it. Refining the
        // *identifier* with a title-type — the obvious-looking shortcut — points the property
        // at the wrong element, and a reading system then shows the subtitle nowhere.
        out.push_str(&format!(
            "<dc:title id=\"subtitle\">{}</dc:title>\n\
             <meta refines=\"#subtitle\" property=\"title-type\">subtitle</meta>\n",
            escape::text(subtitle)
        ));
    }

    out.push_str(&accessibility(document, input));
    out.push_str("</metadata>\n");
    out
}

/// EPUB Accessibility 1.1 metadata, computed from what the pipeline did (R5 §A12).
fn accessibility(document: &Document, input: &PackageInput<'_>) -> String {
    let has_images = !input.images.is_empty();
    let has_headings = document
        .walk()
        .iter()
        .any(|section| section.heading.is_some());
    let has_alt = has_images;
    let has_page_list = !input.emitted.page_list.is_empty();

    let mut out = String::new();
    out.push_str("<meta property=\"schema:accessMode\">textual</meta>\n");
    if has_images {
        out.push_str("<meta property=\"schema:accessMode\">visual</meta>\n");
    }
    // Sufficient on its own, and **unconditionally**: a book with no images is textual and
    // nothing else, and a book with images carries alt text on every one of them — the typed
    // builder cannot emit an `<img>` without it (`an_image_without_alt_text_cannot_be_built`) and
    // Tier 1 rejects an empty one. Either way a reader who cannot see has the whole book in text.
    //
    // This read `if has_alt`, which was the condition inverted: it claimed sufficiency only for
    // books that *had* images and withheld it from books that were pure text. Ace reported it as
    // `metadata-accessmodesufficient` on `f01` and `f08`, the two fixtures with no images at all
    // (nightly CI 35430065404).
    out.push_str("<meta property=\"schema:accessModeSufficient\">textual</meta>\n");

    out.push_str("<meta property=\"schema:accessibilityFeature\">readingOrder</meta>\n");
    if has_headings {
        out.push_str(
            "<meta property=\"schema:accessibilityFeature\">structuralNavigation</meta>\n\
             <meta property=\"schema:accessibilityFeature\">tableOfContents</meta>\n",
        );
    }
    if has_alt {
        out.push_str("<meta property=\"schema:accessibilityFeature\">alternativeText</meta>\n");
    }
    if has_page_list {
        out.push_str("<meta property=\"schema:accessibilityFeature\">printPageNumbers</meta>\n");
        // A book that publishes page numbers has to say where they came from, or a citation of
        // "p. 42" names a page in nothing in particular. EPUB Accessibility 1.1 defines
        // `pageBreakSource` for exactly this question (§pageSource), and Ace enforces it as
        // `epub-pagesource` — at `serious`, the severity the nightly gate bounds at zero, which is
        // how it was found (nightly CI 35430065404).
        //
        // The value is the source document's SHA-256, because that is the only handle we honestly
        // have: a PDF carries no ISBN and no edition statement this converter may rely on, and the
        // filename is the user's business and does not belong in a file they may hand to someone
        // else. A digest identifies the source exactly, is the same on every machine, and says
        // nothing about where it was kept.
        out.push_str(&format!(
            "<meta property=\"pageBreakSource\">urn:sha256:{}</meta>\n",
            escape::text(&document.source_sha256)
        ));
    }
    out.push_str("<meta property=\"schema:accessibilityHazard\">none</meta>\n");

    // A summary that says what was done and what was not. Metadata text is outside `C`
    // (ARCHITECTURE §5.2), so saying it costs the conservation law nothing — and saying it
    // plainly is the difference between a claim and a boast.
    let nav = if has_headings {
        "a heading-based table of contents"
    } else {
        "no heading structure the source made available"
    };
    let pages = if has_page_list {
        ", and the printed page numbers as a page list"
    } else {
        ""
    };
    out.push_str(&format!(
        "<meta property=\"schema:accessibilitySummary\">Converted from PDF by OpenConvert. \
         The reading order, {nav}{pages} come from the source document. No conformance to \
         WCAG is claimed: a converter cannot guarantee it from PDF source.</meta>\n"
    ));
    out
}

fn manifest(input: &PackageInput<'_>) -> String {
    let mut out = String::from("<manifest>\n");

    out.push_str(&format!(
        "<item id=\"{NAV_ID}\" href=\"{}\" media-type=\"application/xhtml+xml\" \
         properties=\"nav\"/>\n",
        escape::attribute(input.nav_path)
    ));
    out.push_str(&format!(
        "<item id=\"{NCX_ID}\" href=\"{}\" media-type=\"application/x-dtbncx+xml\"/>\n",
        escape::attribute(input.ncx_path)
    ));
    out.push_str(&format!(
        "<item id=\"css\" href=\"{}\" media-type=\"text/css\"/>\n",
        escape::attribute(input.style_path)
    ));

    for (index, file) in input.emitted.files.iter().enumerate() {
        let properties = properties_of(file);
        out.push_str(&format!(
            "<item id=\"x{:04}\" href=\"{}\" media-type=\"application/xhtml+xml\"{properties}/>\n",
            index + 1,
            escape::attribute(&file.path)
        ));
    }
    for (index, image) in input.images.iter().enumerate() {
        out.push_str(&format!(
            "<item id=\"img{:04}\" href=\"{}\" media-type=\"{}\"/>\n",
            index + 1,
            escape::attribute(&image.path),
            image.media_type
        ));
    }

    out.push_str("</manifest>\n");
    out
}

/// The `properties` attribute of one content document, read off its serialised bytes.
///
/// OPF-014 — a document that uses a feature the manifest does not declare — is a top
/// auto-generated-EPUB error precisely because emitters compute this from what they *meant* to
/// write. `scripted` and `remote-resources` never appear because v1 never emits either, and the
/// Tier-1 validator asserts their absence independently rather than trusting this function.
fn properties_of(file: &XhtmlFile) -> String {
    let mut properties: Vec<&str> = Vec::new();
    if file.markup.contains("<svg") {
        properties.push("svg");
    }
    if file.markup.contains("<math") {
        properties.push("mathml");
    }
    if file.markup.contains("<script") {
        properties.push("scripted");
    }
    if has_remote_resource(&file.markup) {
        properties.push("remote-resources");
    }
    if properties.is_empty() {
        return String::new();
    }
    format!(" properties=\"{}\"", properties.join(" "))
}

/// Whether the markup fetches anything over a network.
///
/// Checked on `src=` and `href=` rather than on the whole text, so that a URL quoted in the
/// *body* of a book — which is text the author wrote and not a resource the document loads —
/// does not make the manifest declare a remote resource the reader's device will not fetch.
fn has_remote_resource(markup: &str) -> bool {
    ["src=\"", "href=\""].iter().any(|attribute| {
        markup.match_indices(attribute).any(|(index, _)| {
            let value = &markup[index + attribute.len()..];
            ["http://", "https://", "//"]
                .iter()
                .any(|scheme| value.starts_with(scheme))
        })
    })
}

fn spine(input: &PackageInput<'_>) -> String {
    let mut out = format!("<spine toc=\"{NCX_ID}\">\n");
    for index in 0..input.emitted.files.len() {
        out.push_str(&format!("<itemref idref=\"x{:04}\"/>\n", index + 1));
    }
    out.push_str("</spine>\n");
    out
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Row 5.7; rows 5.6 and the rest are
// fixture tests in `crates/openconvert/tests/epub.rs`.
// ---------------------------------------------------------------------------

#[cfg(test)]
fn file(markup: &str) -> XhtmlFile {
    XhtmlFile {
        path: "text/c0001.xhtml".to_owned(),
        title: "One".to_owned(),
        markup: markup.to_owned(),
    }
}

/// Row 5.7. The manifest describes the file that exists, not the file the emitter meant to
/// write — which is the whole of why OPF-014 is a top error in generated EPUBs.
#[test]
fn manifest_properties_are_computed_from_bytes() {
    assert_eq!(properties_of(&file("<p>plain</p>")), "");
    assert_eq!(
        properties_of(&file("<p>a</p><svg xmlns=\"…\"><rect/></svg>")),
        " properties=\"svg\""
    );
    assert_eq!(
        properties_of(&file("<math><mi>x</mi></math>")),
        " properties=\"mathml\""
    );
    // Removing it clears the property: the function reads, it does not remember.
    assert_eq!(properties_of(&file("<p>a</p><p>b</p>")), "");
}

/// A quoted URL in the body of a book is text the author wrote, not a resource the document
/// loads — and declaring `remote-resources` for one would tell a reading system to expect a
/// network fetch that never happens.
#[test]
fn a_url_in_the_text_is_not_a_remote_resource() {
    assert!(!has_remote_resource(
        "<p>See https://example.org for more.</p>"
    ));
    assert!(has_remote_resource(
        "<img src=\"https://example.org/a.png\" alt=\"a\"/>"
    ));
    assert!(has_remote_resource(
        "<link href=\"//cdn.example.org/a.css\"/>"
    ));
    assert!(!has_remote_resource(
        "<img src=\"images/i0001.jpg\" alt=\"a\"/>"
    ));
}

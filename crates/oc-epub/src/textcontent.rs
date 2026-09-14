//! Reading the text back out of an emitted content document.
//!
//! The conservation law is stated over "all content-document text" (ARCHITECTURE §5.2), and
//! the only honest way to measure that on the output is to *parse the output*. Comparing what
//! the emitter believes it wrote would be comparing the emitter against itself, which is the
//! failure mode D6 exists to avoid — and it is the same argument that makes Tier 1 read the
//! archive rather than the intentions.
//!
//! Two exclusions, and both are definitional rather than convenient. `<head>` is metadata, and
//! metadata text is outside `C`. Attribute values — `alt`, `aria-label`, a page-list label —
//! are outside `C` too, which is exactly what makes "remove the printed folio from the flow,
//! keep it as a label" a clean `Removed{PageNumber}` rather than a paradox.

use quick_xml::events::Event;
use quick_xml::Reader;

/// Why the text could not be read back.
#[derive(Debug, thiserror::Error)]
pub enum TextError {
    #[error("the document is not well-formed XML: {0}")]
    Xml(#[from] quick_xml::Error),
    /// An entity reference the document declared no expansion for. v1 emits no internal
    /// subset, so this only happens on a document we did not write — which is exactly what the
    /// Tier-1 validator is handed when it reads a crafted bad EPUB.
    #[error("an entity reference could not be resolved: {0}")]
    Entity(String),
}

/// The text content of one XHTML document's `<body>`, in document order.
///
/// Entity references are resolved, so `&amp;` comes back as `&` and the multiset matches the
/// text that went in.
pub fn body_text(markup: &str) -> Result<String, TextError> {
    let mut reader = Reader::from_str(markup);

    let mut out = String::new();
    let mut depth = 0usize;

    loop {
        match reader.read_event()? {
            Event::Start(tag) if local_name(tag.name().as_ref()) == "body" => depth = 1,
            Event::Start(_) if depth > 0 => depth += 1,
            Event::End(tag) if local_name(tag.name().as_ref()) == "body" => depth = 0,
            Event::End(_) if depth > 0 => depth -= 1,
            // Text and entity references are separate events in quick-xml 0.42: `&amp;` does
            // not reach `Event::Text` at all, so a reader that only collected text would drop
            // every ampersand in the book and the conservation check would fail on `Q&A`.
            Event::Text(text) if depth > 0 => out.push_str(text.as_ref()),
            Event::GeneralRef(reference) if depth > 0 => match reference.resolve_char_ref() {
                Ok(Some(ch)) => out.push(ch),
                Ok(None) => out.push(named_entity(reference.as_ref())?),
                Err(error) => return Err(TextError::Entity(error.to_string())),
            },
            // A CDATA section's content is literal: it is not escaped and must not be
            // unescaped, or `&amp;` inside one would come back as `&`.
            Event::CData(text) if depth > 0 => out.push_str(text.as_ref()),
            Event::Eof => break,
            _ => {}
        }
    }

    Ok(out)
}

/// The five entities XML predeclares.
///
/// The only ones that may occur without a declaration, and v1 emits no internal subset — so
/// anything else in a document we wrote is a bug, and anything else in a document we did not
/// write is an unresolvable reference the validator has to report rather than guess at.
fn named_entity(name: &str) -> Result<char, TextError> {
    match name {
        "amp" => Ok('&'),
        "lt" => Ok('<'),
        "gt" => Ok('>'),
        "quot" => Ok('"'),
        "apos" => Ok('\''),
        other => Err(TextError::Entity(other.to_owned())),
    }
}

/// The part of a qualified name after the colon: `body` of `html:body`.
fn local_name(name: &str) -> &str {
    match name.rfind(':') {
        Some(index) => &name[index + 1..],
        None => name,
    }
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2)
// ---------------------------------------------------------------------------

/// The head is metadata and an attribute is a label. Counting either would make the
/// conservation check fail on a book that is perfectly conserved — or, worse, pass on one that
/// is not, because an invented `alt` would balance a dropped paragraph.
#[test]
fn the_head_and_every_attribute_are_outside_the_text() {
    let markup = "<?xml version=\"1.0\"?><html xmlns=\"x\"><head><title>A Title</title></head>\
                  <body><p>Body text.</p>\
                  <img src=\"i.png\" alt=\"A described picture\"/>\
                  <span aria-label=\"iv\"></span></body></html>";

    let text = body_text(markup).expect("parses");
    assert!(text.contains("Body text."));
    assert!(!text.contains("A Title"), "the head is metadata");
    assert!(!text.contains("A described picture"), "alt is a label");
    assert!(!text.contains("iv"), "a page label is a label");
}

/// An entity has to come back as the character it stands for, or the multiset of the output is
/// not the multiset of the input and every book with an ampersand in it fails the law.
#[test]
fn entities_are_resolved_back_to_their_characters() {
    let markup = "<html><body><p>Q&amp;A &lt;here&gt;</p></body></html>";
    assert_eq!(body_text(markup).expect("parses"), "Q&A <here>");
}

/// Nested elements contribute their text once, in document order.
#[test]
fn nested_markup_contributes_its_text_in_order() {
    let markup = "<html><body><blockquote><p>One <em>two</em> three</p></blockquote>\
                  <p>four</p></body></html>";
    assert_eq!(body_text(markup).expect("parses"), "One two threefour");
}

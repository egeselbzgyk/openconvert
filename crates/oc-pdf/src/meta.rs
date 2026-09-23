//! Document metadata read from the file's own object tree (Phase 1 detail 6).
//!
//! Three things PDFium either cannot answer or answers by guessing, so all three are read
//! with `lopdf` instead:
//!
//! - **Is it encrypted?** The trailer either has an `/Encrypt` entry or it does not.
//! - **Does it have a structure tree?** `/Root /StructTreeRoot` either resolves or it does
//!   not. A *hint only*, per D3: 12.6 % of PDFs are tagged and a good many of those are
//!   tagged wrongly, so its presence licenses looking, never believing.
//! - **What does the XMP packet say?** Kept as raw bytes and read for the three Dublin Core
//!   fields a book has. Which of XMP and `/Info` wins when they disagree is Phase 4's
//!   question; Phase 1 only has to make both available without losing either.

use lopdf::{Document, Object};

/// The Dublin Core fields worth extracting from an XMP packet.
///
/// Deliberately three. A full XMP parser is a large surface for a small return, and every
/// other property in the packet is either producer trivia or something `/Info` also carries.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct XmpMeta {
    pub title: Option<String>,
    /// `dc:creator` is an ordered sequence — a book can have several authors, and the order
    /// is theirs, not ours.
    pub creators: Vec<String>,
    pub language: Option<String>,
}

/// Whether the document was encrypted.
///
/// Two conditions, and the second is the one that fires. A byte search for `/Encrypt` finds
/// the string wherever it occurs — inside a content stream, inside a text string — and calls a
/// plain document encrypted; the trailer entry is the definition. But `lopdf` **decrypts on
/// load and removes that entry**, recording an `encryption_state` in its place, so by the time
/// the trailer can be read the evidence of encryption has been consumed by the act of reading
/// it. The state is what survives.
pub fn is_encrypted(document: &Document) -> bool {
    document.encryption_state.is_some() || document.trailer.has(b"Encrypt")
}

/// Whether the catalogue declares a structure tree.
///
/// Same argument as above, and one more: `/StructTreeRoot` is a phrase that appears in the
/// *text* of documents about PDF accessibility, which is exactly the corpus this project is
/// most likely to be pointed at.
pub fn has_struct_tree(document: &Document) -> bool {
    catalog(document).is_some_and(|catalog| catalog.has(b"StructTreeRoot"))
}

/// The raw XMP packet, if the document has one.
///
/// Returned as bytes rather than parsed, because XMP is RDF and a lossless round trip is not
/// something a three-field extractor can promise. Phase 4 keeps the packet when it copies
/// metadata forward.
pub fn xmp_packet(document: &Document, limits: &oc_core::limits::Limits) -> Option<Vec<u8>> {
    let stream = catalog(document)?
        .get(b"Metadata")
        .ok()
        .and_then(|object| resolve(document, object))?
        .as_stream()
        .ok()?;
    // XMP is usually stored uncompressed, but "usually" is not a decoder, and the cap applies
    // to this stream exactly as it does to a page's — through the same bounded chain.
    crate::filters::decode_stream(stream, limits, 0).ok()
}

/// The three Dublin Core fields, read out of an XMP packet.
///
/// A scanning reader rather than a parse: it finds `<dc:title>`'s first `<rdf:li>` and so on.
/// That is enough for well-formed XMP, which is what producers emit, and it cannot be led
/// anywhere expensive by a malformed packet — the alternative, a general RDF model, is a
/// dependency and an attack surface for three strings.
pub fn read_xmp(packet: &[u8]) -> XmpMeta {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    /// The elements we are looking for, and which field each fills.
    const TITLE: &str = "dc:title";
    const CREATOR: &str = "dc:creator";
    const LANGUAGE: &str = "dc:language";
    const ITEM: &str = "rdf:li";

    #[derive(Clone, Copy, PartialEq)]
    enum Field {
        None,
        Title,
        Creator,
        Language,
    }

    // XMP is required to be UTF-8. A packet that is not is a packet we decline to read,
    // rather than one we read approximately: a lossy decode of metadata produces plausible
    // wrong author names, which is worse than no author name.
    let Ok(text) = std::str::from_utf8(packet) else {
        return XmpMeta::default();
    };

    let mut reader = Reader::from_str(text);
    let mut meta = XmpMeta::default();
    let mut field = Field::None;
    let mut in_item = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) => match element.name().as_ref() {
                TITLE => field = Field::Title,
                CREATOR => field = Field::Creator,
                LANGUAGE => field = Field::Language,
                ITEM => in_item = true,
                _ => {}
            },
            Ok(Event::End(element)) => match element.name().as_ref() {
                TITLE | CREATOR | LANGUAGE => field = Field::None,
                ITEM => in_item = false,
                _ => {}
            },
            Ok(Event::Text(content)) if in_item => {
                let value = content.xml10_content().trim().to_owned();
                if value.is_empty() {
                    continue;
                }
                match field {
                    // First one wins: a packet with several `<rdf:li>` under `dc:title` is
                    // giving the same title in several languages, not several titles.
                    Field::Title => {
                        meta.title.get_or_insert(value);
                    }
                    Field::Language => {
                        meta.language.get_or_insert(value);
                    }
                    Field::Creator => meta.creators.push(value),
                    Field::None => {}
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            Ok(_) => {}
        }
    }
    meta
}

fn catalog(document: &Document) -> Option<&lopdf::Dictionary> {
    document
        .trailer
        .get(b"Root")
        .ok()
        .and_then(|object| resolve(document, object))
        .and_then(|object| object.as_dict().ok())
}

fn resolve<'a>(document: &'a Document, object: &'a Object) -> Option<&'a Object> {
    match object {
        Object::Reference(id) => document.get_object(*id).ok(),
        other => Some(other),
    }
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Not a numbered row: Phase 1 detail 6
// asks for this reading and the plan's table names no test for it, so these are the
// assertions that stop it being written on trust.
// ---------------------------------------------------------------------------

#[cfg(test)]
fn compiled(name: &str) -> Document {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/fixtures")
        .join(format!("{name}.pdf"));
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        // The tagged variants come from a *different* invocation, and a message that named
        // only the plain one sent CI's first red run looking in the wrong place.
        let flag = if name.ends_with("__tagged") {
            " --keep-structtree"
        } else {
            ""
        };
        panic!(
            "missing fixture {}: {error}; run `cargo run -p xtask -- fixtures{flag}`",
            path.display()
        )
    });
    Document::load_mem(&bytes).expect("the fixture parses")
}

/// The structure-tree flag comes from the catalogue, and `xtask fixtures` really does strip
/// the tree from the untagged variant (D18).
#[test]
fn struct_tree_is_read_from_the_catalogue() {
    assert!(!has_struct_tree(&compiled("f01_prose_single_column")));
    assert!(has_struct_tree(&compiled(
        "f01_prose_single_column__tagged"
    )));
}

/// Encryption comes from the trailer.
#[test]
fn encryption_is_read_from_the_trailer() {
    assert!(!is_encrypted(&compiled("f01_prose_single_column")));

    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/fixtures/mutations/h01__encrypted_empty_user.pdf");
    let bytes = std::fs::read(path).expect("the mutation is committed");
    let document = Document::load_mem(&bytes).expect("the fixture parses");
    assert!(is_encrypted(&document));
}

/// Typst writes an XMP packet, and the three fields come back out of it.
#[test]
fn xmp_dublin_core_is_extracted() {
    let document = compiled("f01_prose_single_column");
    let packet = xmp_packet(&document, &oc_core::limits::Limits::default())
        .expect("Typst writes an XMP packet");
    assert!(
        packet.windows(6).any(|w| w == b"<x:xmp"),
        "the packet really is XMP"
    );

    let meta = read_xmp(&packet);
    assert_eq!(meta.title.as_deref(), Some("The Test Book"));
    assert_eq!(meta.creators, vec!["O. Convert".to_owned()]);
}

/// A document with no XMP has none, and a malformed packet yields nothing rather than
/// anything — the reader is fed whatever is in the file.
#[test]
fn missing_or_malformed_xmp_is_not_an_error() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/fixtures/handmade/h01_two_glyphs.pdf");
    let bytes = std::fs::read(path).expect("h01 is committed");
    let document = Document::load_mem(&bytes).expect("the fixture parses");
    assert!(xmp_packet(&document, &oc_core::limits::Limits::default()).is_none());

    assert_eq!(read_xmp(b"not xml at all"), XmpMeta::default());
    assert_eq!(read_xmp(b"<dc:title><rdf:li>"), XmpMeta::default());
}

//! The container's block-level text, read back out of the XHTML.
//!
//! The structural checks that are about *the book* rather than the container — a heading tree
//! with a level skip in it, a paragraph emitted twice — are stated over blocks, and a block is
//! not something the archive hands over: it has to be parsed out. Doing it once here keeps the
//! heading walk and the duplicate count reading the same elements, in the same order, with the
//! same idea of what a block's text is.
//!
//! Nested block elements — a `<p>` inside a `<blockquote>`, a `<li>` inside a `<ul>` inside an
//! `<li>` — yield **one** block each, the innermost. A parent that also reported its text would
//! count every child's characters twice, and the duplicate statistic would then read a book with
//! one long quotation as a book that repeats itself.

use quick_xml::events::Event;
use quick_xml::Reader;

use oc_epub::textcontent::TextError;

/// One block-level element's text, and where it was.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    /// The container path of the document it came from.
    pub path: String,
    /// The element's local name: `p`, `h1`, `li`, `blockquote`, `figcaption`, `td`, …
    pub tag: String,
    /// The heading level, 1..=6, for `h1`…`h6`; `None` for everything else.
    pub heading_level: Option<u8>,
    /// The element's text, with runs of whitespace collapsed to one space and the ends trimmed.
    pub text: String,
}

/// The block-level element names the emitter can produce.
///
/// A closed list rather than "anything that is not phrasing", because the question this answers
/// is "what did the emitter write", and the emitter's vocabulary is finite and typed (D5).
const BLOCKS: [&str; 14] = [
    "p",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "li",
    "blockquote",
    "figcaption",
    "caption",
    "td",
    "th",
    "pre",
];

/// Every block of one content document, in document order.
pub fn blocks_of(path: &str, markup: &str) -> Result<Vec<Block>, TextError> {
    let mut reader = Reader::from_str(markup);
    let mut out: Vec<Block> = Vec::new();

    // The stack of block elements currently open, each with the text seen since it opened. Only
    // the innermost one is credited with a run of text, so a `<p>` inside a `<blockquote>`
    // yields the paragraph and not the quotation as well.
    let mut open: Vec<(String, String)> = Vec::new();
    let mut in_body = false;

    loop {
        match reader.read_event()? {
            Event::Start(tag) => {
                let name = local_name(tag.name().as_ref()).to_owned();
                if name == "body" {
                    in_body = true;
                } else if in_body && is_block(&name) {
                    open.push((name, String::new()));
                }
            }
            Event::End(tag) => {
                let raw = tag.name();
                let name = local_name(raw.as_ref());
                if name == "body" {
                    in_body = false;
                } else if in_body && is_block(name) {
                    // Popped by name rather than blindly, so that markup the emitter did not
                    // write — the crafted containers the validator is handed — cannot desync the
                    // stack and swallow the rest of the document.
                    if let Some(index) = open.iter().rposition(|(open, _)| open == name) {
                        let (tag, text) = open.remove(index);
                        let text = collapse(&text);
                        if !text.is_empty() {
                            out.push(Block {
                                path: path.to_owned(),
                                heading_level: heading_level(&tag),
                                tag,
                                text,
                            });
                        }
                    }
                }
            }
            Event::Text(text) if in_body => push_innermost(&mut open, text.as_ref()),
            Event::CData(text) if in_body => push_innermost(&mut open, text.as_ref()),
            Event::GeneralRef(reference) if in_body => {
                let ch = match reference.resolve_char_ref() {
                    Ok(Some(ch)) => ch,
                    Ok(None) => named_entity(reference.as_ref())?,
                    Err(error) => return Err(TextError::Entity(error.to_string())),
                };
                push_innermost(&mut open, &ch.to_string());
            }
            Event::Eof => break,
            _ => {}
        }
    }

    Ok(out)
}

fn push_innermost(open: &mut [(String, String)], text: &str) {
    if let Some((_, buffer)) = open.last_mut() {
        buffer.push_str(text);
    }
}

fn is_block(name: &str) -> bool {
    BLOCKS.contains(&name)
}

fn heading_level(tag: &str) -> Option<u8> {
    let rest = tag.strip_prefix('h')?;
    let level: u8 = rest.parse().ok()?;
    (1..=6).contains(&level).then_some(level)
}

/// Runs of whitespace to one space, ends trimmed — so that a paragraph the emitter wrapped over
/// three lines and the same paragraph on one line are the same block.
fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn local_name(name: &str) -> &str {
    match name.rfind(':') {
        Some(index) => &name[index + 1..],
        None => name,
    }
}

/// The five entities XML predeclares — the same set [`oc_epub::textcontent`] accepts, and for the
/// same reason: v1 emits no internal subset, so anything else is a document we did not write.
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

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Additions to the Phase 6 table: the
// heading-sanity and duplicate checks are stated over blocks, so what a block is has to be
// pinned before either of them means anything.
// ---------------------------------------------------------------------------

#[cfg(test)]
fn doc(body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\">\
         <head><title>t</title></head><body>{body}</body></html>"
    )
}

/// The head is outside the blocks, and so is every attribute: a `<title>` that counted as a
/// block would put the book's title into the duplicate statistic once per file.
#[test]
fn the_head_is_not_a_block() {
    let blocks = blocks_of("t/c1.xhtml", &doc("<p>One</p>")).expect("well-formed");
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].tag, "p");
    assert_eq!(blocks[0].text, "One");
    assert_eq!(blocks[0].path, "t/c1.xhtml");
}

/// A nested block yields the innermost element only. A `<blockquote>` that also reported its
/// text would double every quoted paragraph, and `dup_block_frac` would read a book with one
/// long quotation as a book that repeats itself.
#[test]
fn a_nested_block_is_counted_once_at_its_innermost_level() {
    let blocks = blocks_of(
        "t/c1.xhtml",
        &doc("<blockquote><p>Quoted</p></blockquote><ul><li>Item</li></ul>"),
    )
    .expect("well-formed");

    let tags: Vec<&str> = blocks.iter().map(|block| block.tag.as_str()).collect();
    assert_eq!(tags, vec!["p", "li"], "{blocks:?}");
    assert_eq!(blocks[0].text, "Quoted");
}

/// Phrasing markup inside a block contributes its text in order, and an entity comes back as
/// the character it stands for — the same rule [`oc_epub::textcontent::body_text`] follows, so
/// that a block's text and `C(EPUB)` cannot disagree about what is in the book.
#[test]
fn phrasing_and_entities_are_part_of_the_block() {
    let blocks = blocks_of(
        "t/c1.xhtml",
        &doc("<p>Q&amp;A with <em>emphasis</em> inside</p>"),
    )
    .expect("well-formed");
    assert_eq!(blocks[0].text, "Q&A with emphasis inside");
}

/// Whitespace is collapsed, so a paragraph the emitter wrapped and the same paragraph on one
/// line are one block and not two.
#[test]
fn whitespace_is_collapsed_so_wrapping_does_not_make_a_new_block() {
    let wrapped = blocks_of("t/c1.xhtml", &doc("<p>One\n  two\tthree </p>")).expect("well-formed");
    let flat = blocks_of("t/c1.xhtml", &doc("<p>One two three</p>")).expect("well-formed");
    assert_eq!(wrapped[0].text, flat[0].text);
    assert_eq!(wrapped[0].text, "One two three");
}

/// `h1`…`h6` carry their level and nothing else does.
#[test]
fn a_heading_carries_its_level() {
    let blocks = blocks_of(
        "t/c1.xhtml",
        &doc("<h1>One</h1><h3>Three</h3><p>Body</p><h7>Seven</h7>"),
    )
    .expect("well-formed");

    let levels: Vec<Option<u8>> = blocks.iter().map(|block| block.heading_level).collect();
    assert_eq!(levels, vec![Some(1), Some(3), None], "{blocks:?}");
}

/// An empty block is not a block. The emitter does not write one, and a container that does —
/// `<p></p>` between two paragraphs — would otherwise contribute an infinitely repeated empty
/// string to the duplicate count.
#[test]
fn an_empty_block_is_not_reported() {
    let blocks =
        blocks_of("t/c1.xhtml", &doc("<p>One</p><p></p><p>  </p><p>Two</p>")).expect("well-formed");
    assert_eq!(blocks.len(), 2, "{blocks:?}");
}

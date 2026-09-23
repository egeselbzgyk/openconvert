//! How many glyphs a page's content declares, counted before PDFium loads the page (PHASE 14
//! detail 10, SECURITY §4: "a 3-page PDF declaring 40 million glyphs must fail cleanly").
//!
//! PDFium parses a page's content when the page is loaded, and builds an object per text-showing
//! operator and a record per character when its text is read. Forty million one-glyph `Tj`s cost
//! gigabytes before any of this crate's code sees a character — measured: 13 GB of RSS and no end
//! in sight on three such pages. No cap in D13.2's list bounds that: the content streams are small
//! (they compress a thousand to one) and every page is legal. So the count is taken from the
//! content itself — the bytes of every string operand of `Tj`, `'`, `"` and `TJ`, which bound the
//! glyphs a font can show (one byte per glyph at most; two for a CID font) — and refused past
//! `limits.max_page_glyphs` before the page is handed to PDFium.
//!
//! A form XObject's glyphs count once per `Do` that paints it, followed with a visited set so a
//! form that paints itself is a refusal-free dead end rather than a loop. The content is read
//! through the bounded filter chain (`limits::read_page_content`), so counting costs at most what
//! the stream cap allows. Inline image data (`ID … EI`) is skipped, not scanned for strings.

use std::collections::{BTreeMap, BTreeSet};

use lopdf::{Document, Object, ObjectId};
use oc_core::limits::{CapViolation, Limits};

use crate::error::PdfError;

/// Count the glyphs page `page` declares, and refuse it past the cap.
pub fn check_page_glyphs(
    document: &Document,
    page: ObjectId,
    index: u32,
    caps: &Limits,
) -> Result<u64, PdfError> {
    let content = crate::limits::read_page_content(document, page, caps)?;
    let resources = page_xobjects(document, page);
    let mut forms = FormCounter {
        document,
        caps,
        memo: BTreeMap::new(),
        visiting: BTreeSet::new(),
    };
    let declared = forms.count(&content, &resources)?;
    if declared > caps.max_page_glyphs {
        return Err(CapViolation::PageGlyphs {
            declared,
            limit: caps.max_page_glyphs,
            page: index,
        }
        .into());
    }
    Ok(declared)
}

/// The page's `/XObject` resources, by name, following inherited resources up the page tree.
fn page_xobjects(document: &Document, page: ObjectId) -> BTreeMap<Vec<u8>, ObjectId> {
    let mut found = BTreeMap::new();
    let (resources, inherited) = document
        .get_page_resources(page)
        .unwrap_or((None, Vec::new()));
    let dictionaries = resources.into_iter().cloned().chain(
        inherited
            .into_iter()
            .filter_map(|id| document.get_dictionary(id).ok().cloned()),
    );
    for dictionary in dictionaries {
        collect_xobjects(document, &dictionary, &mut found);
    }
    found
}

fn collect_xobjects(
    document: &Document,
    resources: &lopdf::Dictionary,
    into: &mut BTreeMap<Vec<u8>, ObjectId>,
) {
    let xobjects = match resources.get(b"XObject") {
        Ok(Object::Dictionary(dictionary)) => Some(dictionary.clone()),
        Ok(Object::Reference(id)) => document.get_dictionary(*id).ok().cloned(),
        _ => None,
    };
    for (name, value) in xobjects.iter().flat_map(|dictionary| dictionary.iter()) {
        if let Ok(id) = value.as_reference() {
            into.entry(name.clone()).or_insert(id);
        }
    }
}

struct FormCounter<'a> {
    document: &'a Document,
    caps: &'a Limits,
    /// Each form's own count, once.
    memo: BTreeMap<ObjectId, u64>,
    /// Forms on the current `Do` path: one that paints itself contributes nothing more.
    visiting: BTreeSet<ObjectId>,
}

impl FormCounter<'_> {
    fn count(
        &mut self,
        content: &[u8],
        xobjects: &BTreeMap<Vec<u8>, ObjectId>,
    ) -> Result<u64, PdfError> {
        let mut total = 0_u64;
        let mut scanner = Scanner::new(content);
        // Operands since the last operator: string bytes seen, and the last name.
        let mut strings = 0_u64;
        let mut name: Option<Vec<u8>> = None;
        while let Some(token) = scanner.next() {
            match token {
                Token::String(bytes) => strings = strings.saturating_add(bytes),
                Token::Name(value) => name = Some(value),
                Token::Operator(op) => {
                    match op {
                        b"Tj" | b"TJ" | b"'" | b"\"" => total = total.saturating_add(strings),
                        b"Do" => {
                            if let Some(form) = name.as_ref().and_then(|name| xobjects.get(name)) {
                                total = total.saturating_add(self.form(*form)?);
                            }
                        }
                        _ => {}
                    }
                    strings = 0;
                    name = None;
                }
                Token::Other => {}
            }
            // Stop counting as soon as the answer is known: a hostile page is refused at the cap,
            // not after it has been scanned to the end.
            if total > self.caps.max_page_glyphs {
                return Ok(total);
            }
        }
        Ok(total)
    }

    fn form(&mut self, id: ObjectId) -> Result<u64, PdfError> {
        if let Some(count) = self.memo.get(&id) {
            return Ok(*count);
        }
        if !self.visiting.insert(id) {
            return Ok(0);
        }
        let Ok(stream) = self.document.get_object(id).and_then(Object::as_stream) else {
            self.visiting.remove(&id);
            return Ok(0);
        };
        if stream.dict.get(b"Subtype").and_then(Object::as_name).ok() != Some(b"Form".as_slice()) {
            self.visiting.remove(&id);
            return Ok(0);
        }
        let content = match crate::filters::decode_stream(stream, self.caps, id.0) {
            Ok(content) => content,
            Err(crate::filters::StreamError::Cap(violation)) => return Err(violation.into()),
            Err(_) => Vec::new(),
        };
        let mut xobjects = BTreeMap::new();
        if let Ok(resources) = stream.dict.get(b"Resources") {
            let resources = match resources {
                Object::Dictionary(dictionary) => Some(dictionary.clone()),
                Object::Reference(reference) => {
                    self.document.get_dictionary(*reference).ok().cloned()
                }
                _ => None,
            };
            if let Some(resources) = resources {
                collect_xobjects(self.document, &resources, &mut xobjects);
            }
        }
        let count = self.count(&content, &xobjects)?;
        self.visiting.remove(&id);
        self.memo.insert(id, count);
        Ok(count)
    }
}

/// What the counter needs from a content stream (PDF 32000-1 §7.8.2, §9.4).
enum Token {
    /// A string operand, by its byte length (a hex string's digits halved).
    String(u64),
    Name(Vec<u8>),
    Operator(&'static [u8]),
    Other,
}

/// The operators the counter acts on; everything else is `Other`.
const COUNTED: [&[u8]; 5] = [b"Tj", b"TJ", b"'", b"\"", b"Do"];
/// Inline image data begins after `ID` and one white-space byte, and ends at `EI`.
const INLINE_DATA: &[u8] = b"ID";
const INLINE_END: &[u8] = b"EI";
const HEX_DIGITS_PER_BYTE: u64 = 2;

struct Scanner<'a> {
    bytes: &'a [u8],
    pos: usize,
}

fn is_space(byte: u8) -> bool {
    matches!(byte, b'\0' | b'\t' | b'\n' | b'\x0c' | b'\r' | b' ')
}

fn is_delimiter(byte: u8) -> bool {
    matches!(
        byte,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

impl<'a> Scanner<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn next(&mut self) -> Option<Token> {
        loop {
            let byte = *self.bytes.get(self.pos)?;
            if is_space(byte) {
                self.pos += 1;
                continue;
            }
            if byte == b'%' {
                while self
                    .bytes
                    .get(self.pos)
                    .is_some_and(|b| *b != b'\n' && *b != b'\r')
                {
                    self.pos += 1;
                }
                continue;
            }
            return Some(match byte {
                b'(' => {
                    self.pos += 1;
                    Token::String(self.literal())
                }
                b'<' if self.bytes.get(self.pos + 1) == Some(&b'<') => {
                    self.pos += 2;
                    Token::Other
                }
                b'<' => {
                    self.pos += 1;
                    Token::String(self.hex())
                }
                b'/' => {
                    self.pos += 1;
                    Token::Name(self.word().to_vec())
                }
                b'[' | b']' | b'{' | b'}' | b'>' | b')' => {
                    self.pos += 1;
                    Token::Other
                }
                _ => {
                    let word = self.word();
                    if word == INLINE_DATA {
                        self.skip_inline_image();
                        Token::Other
                    } else if let Some(op) = COUNTED.iter().find(|op| **op == word) {
                        Token::Operator(op)
                    } else if word
                        .first()
                        .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'\'' || *b == b'"')
                    {
                        // Every other operator ends the operands too.
                        Token::Operator(b"")
                    } else {
                        Token::Other
                    }
                }
            });
        }
    }

    fn word(&mut self) -> &'a [u8] {
        let start = self.pos;
        while let Some(byte) = self.bytes.get(self.pos) {
            if is_space(*byte) || is_delimiter(*byte) {
                break;
            }
            self.pos += 1;
        }
        if self.pos == start {
            // A lone delimiter this scanner does not name: step over it.
            self.pos += 1;
        }
        &self.bytes[start..self.pos.min(self.bytes.len())]
    }

    /// A literal string's byte count, escapes counted as the byte they stand for.
    fn literal(&mut self) -> u64 {
        let mut depth = 1_u32;
        let mut count = 0_u64;
        while let Some(byte) = self.bytes.get(self.pos).copied() {
            self.pos += 1;
            match byte {
                b'\\' => {
                    self.pos += 1;
                    count += 1;
                }
                b'(' => {
                    depth += 1;
                    count += 1;
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    count += 1;
                }
                _ => count += 1,
            }
        }
        count
    }

    fn hex(&mut self) -> u64 {
        let mut digits = 0_u64;
        while let Some(byte) = self.bytes.get(self.pos).copied() {
            self.pos += 1;
            if byte == b'>' {
                break;
            }
            if byte.is_ascii_hexdigit() {
                digits += 1;
            }
        }
        digits.div_ceil(HEX_DIGITS_PER_BYTE)
    }

    /// Past `ID <data> EI`: the data is binary and may contain anything but a white-space-delimited
    /// `EI`.
    fn skip_inline_image(&mut self) {
        while self.pos < self.bytes.len() {
            let at_end = self.bytes[self.pos..].starts_with(INLINE_END)
                && self.pos > 0
                && is_space(self.bytes[self.pos - 1])
                && self
                    .bytes
                    .get(self.pos + INLINE_END.len())
                    .is_none_or(|b| is_space(*b));
            if at_end {
                self.pos += INLINE_END.len();
                return;
            }
            self.pos += 1;
        }
    }
}

// ---------------------------------------------------------------------------
// Tests. The end-to-end refusal is row 14.9 (`degenerate_40m_glyph_pdf_fails_cleanly`).
// ---------------------------------------------------------------------------

#[cfg(test)]
fn count_of(content: &[u8]) -> u64 {
    let document = Document::with_version("1.7");
    let caps = Limits::default();
    let mut counter = FormCounter {
        document: &document,
        caps: &caps,
        memo: BTreeMap::new(),
        visiting: BTreeSet::new(),
    };
    counter.count(content, &BTreeMap::new()).expect("counts")
}

#[test]
fn text_operators_count_their_string_bytes() {
    assert_eq!(count_of(b"BT /F1 12 Tf (Hello) Tj ET"), 5);
    assert_eq!(count_of(b"BT [(Hel) -20 (lo)] TJ ET"), 5);
    assert_eq!(count_of(b"BT (a\\)b) ' <00410042> Tj ET"), 3 + 4);
    assert_eq!(count_of(b"BT 1 2 (xy) \" ET"), 2);
    // Strings that are not shown are not glyphs, and inline image data is not scanned.
    assert_eq!(count_of(b"/Span <</ActualText (ignored)>> BDC EMC"), 0);
    assert_eq!(
        count_of(b"BI /W 1 /H 1 ID (not a string) Tj\nEI BT (ok) Tj ET"),
        2
    );
}

#[test]
fn every_committed_fixture_is_far_under_the_page_cap() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/fixtures");
    let caps = Limits::default();
    let mut pages = 0;
    let mut glyphs = 0;
    for entry in std::fs::read_dir(&root).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "pdf") {
            continue;
        }
        let bytes = std::fs::read(&path).expect("fixture");
        let Ok(document) = Document::load_mem(&bytes) else {
            continue;
        };
        for (number, id) in document.get_pages() {
            let count = check_page_glyphs(&document, id, number, &caps)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            assert!(
                count < caps.max_page_glyphs / 10,
                "{}: {count}",
                path.display()
            );
            glyphs += count;
            pages += 1;
        }
    }
    assert!(pages > 10, "only {pages} pages counted");
    assert!(glyphs > 1000, "the fixtures declare text: {glyphs}");
}

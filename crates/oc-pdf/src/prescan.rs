//! The structural pre-walk: what the file declares, read before any parser opens it (PHASE 14
//! details 3 and 4).
//!
//! PDFium and `lopdf` both walk the cross-reference chain as the first thing they do, and both
//! walk it to the end. A `/Prev` chain five hundred sections deep, or one that points back at
//! itself, costs whatever it costs before a line of this crate's code runs. So this module walks
//! it first, with two separate guards and two separate failures:
//!
//! - a **depth counter** against `limits.max_xref_chain`: a long chain stops at the cap with
//!   [`CapViolation::XrefDepth`];
//! - a **visited set** of byte offsets: a chain that returns to a section stops at once with
//!   [`CapViolation::XrefCycle`], whatever its length. A cycle is a malformed file, a long chain a
//!   suspicious one, and the report says which.
//!
//! The same two guards bound a chain of object streams (an object in a stream that is itself in a
//! stream), which a resolution meets on the way to the catalogue.
//!
//! The walk also answers the page-count question from the file's own words — the catalogue's
//! `/Pages` and its `/Count` — so `max_pages` refuses a document before a page object is
//! materialised by anything.
//!
//! **Lenient everywhere else.** A section that does not parse ends the walk without an error:
//! PDFium reconstructs broken cross-reference tables routinely, and a structural pre-check that
//! refused what the real parser can read would be a regression dressed as a defence. Only the
//! two guards and the page count ever refuse.

use std::collections::BTreeSet;

use lopdf::{Dictionary, Object, ObjectId, Stream, StringFormat};
use oc_core::limits::{CapViolation, Limits};

/// How far from the end `startxref` may sit (PDF 32000-1 §7.5.5 puts it in the last lines; the
/// window allows for trailing junk some producers append).
const TAIL_WINDOW: usize = 4096;
/// How far into the file the header may sit. Offsets are relative to it (§7.5.2).
const HEAD_WINDOW: usize = 1024;
/// A classic xref entry's fields (§7.5.4): ten digits, five digits, one type letter.
const OFFSET_DIGITS: usize = 10;
const XREF_STREAM_FIELDS: usize = 3;
/// A field wider than eight bytes cannot hold a `u64`.
const MAX_FIELD_BYTES: usize = 8;
const BITS_PER_BYTE: u32 = 8;
/// Cross-reference stream entry types (§7.5.8.3, Table 18).
const TYPE_FREE: u64 = 0;
const TYPE_IN_USE: u64 = 1;
const TYPE_COMPRESSED: u64 = 2;

/// One entry of the cross-reference data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Entry {
    Free,
    /// At a byte offset in the file.
    InUse {
        offset: u64,
    },
    /// The `index`-th object of the object stream `stream`.
    Compressed {
        stream: u32,
        index: u32,
    },
}

/// One cross-reference section, kept in the form it was found in rather than expanded: a table
/// is looked up by scanning its lines and a stream by indexing its rows, so a section that
/// claims a million objects costs its own bytes and nothing more.
enum Section {
    Table {
        /// `(first object, count, offset of the first entry line)`.
        subsections: Vec<(u32, u32, usize)>,
    },
    Stream {
        rows: Vec<u8>,
        widths: [usize; XREF_STREAM_FIELDS],
        /// `(first object, count)` pairs; the rows follow in this order.
        index: Vec<(u32, u32)>,
    },
}

/// What the walk found.
pub struct XrefWalk {
    /// Newest first, as the chain is walked.
    sections: Vec<Section>,
    /// The newest trailer's `/Root`.
    root: Option<ObjectId>,
    /// Where offsets are measured from: the `%PDF-` header's position.
    base: usize,
    /// How many sections the chain has.
    pub depth: u32,
}

/// Walk the cross-reference chain from `startxref`, refusing a chain that is too long or that
/// cycles. `Ok` for anything else, including a file whose chain cannot be read at all.
pub fn walk_xref_chain(bytes: &[u8], caps: &Limits) -> Result<XrefWalk, CapViolation> {
    let base = find(&bytes[..bytes.len().min(HEAD_WINDOW)], b"%PDF-").unwrap_or(0);
    let mut walk = XrefWalk {
        sections: Vec::new(),
        root: None,
        base,
        depth: 0,
    };
    let Some(start) = startxref(bytes) else {
        return Ok(walk);
    };

    let mut budget = caps.max_decompressed_stream_bytes;
    let mut visited = BTreeSet::new();
    let mut pending = vec![start];
    while let Some(offset) = pending.pop() {
        if !visited.insert(offset) {
            return Err(CapViolation::XrefCycle { offset });
        }
        walk.depth = walk.depth.saturating_add(1);
        if walk.depth > caps.max_xref_chain {
            return Err(CapViolation::XrefDepth {
                depth: walk.depth,
                limit: caps.max_xref_chain,
            });
        }
        let Some(position) = usize::try_from(offset)
            .ok()
            .and_then(|offset| offset.checked_add(base))
        else {
            break;
        };
        let Some((section, trailer)) = read_section(bytes, position, caps, &mut budget)? else {
            break;
        };
        walk.sections.push(section);
        if walk.root.is_none() {
            walk.root = trailer.get(b"Root").and_then(Object::as_reference).ok();
        }
        // `/Prev` continues the chain. A hybrid file's `/XRefStm` is one more section of the
        // same revision: walked (and counted, and remembered) before the older revision.
        if let Some(prev) = offset_value(&trailer, b"Prev") {
            pending.push(prev);
        }
        if let Some(stream) = offset_value(&trailer, b"XRefStm") {
            pending.push(stream);
        }
    }
    Ok(walk)
}

impl XrefWalk {
    /// The page count the catalogue declares: `/Root` → `/Pages` → `/Count`.
    ///
    /// `Ok(None)` when the chain does not lead there — a reconstructed or encrypted object stream,
    /// a catalogue somewhere unexpected. The page cap is then checked on PDFium's count instead,
    /// which is still before any page loads. `Err` only for a chain of object streams that is
    /// too deep or that cycles.
    pub fn declared_page_count(
        &self,
        bytes: &[u8],
        caps: &Limits,
    ) -> Result<Option<u64>, CapViolation> {
        let Some(root) = self.root else {
            return Ok(None);
        };
        let Some(catalog) = self.resolve(bytes, root.0, caps)? else {
            return Ok(None);
        };
        let Some(pages) = catalog
            .as_dict()
            .ok()
            .and_then(|catalog| catalog.get(b"Pages").ok())
            .and_then(|pages| pages.as_reference().ok())
        else {
            return Ok(None);
        };
        let Some(pages) = self.resolve(bytes, pages.0, caps)? else {
            return Ok(None);
        };
        Ok(pages
            .as_dict()
            .ok()
            .and_then(|pages| pages.get(b"Count").ok())
            .and_then(|count| count.as_i64().ok())
            .and_then(|count| u64::try_from(count).ok()))
    }

    /// The newest entry for object `number`.
    fn entry(&self, bytes: &[u8], number: u32) -> Option<Entry> {
        self.sections
            .iter()
            .find_map(|section| section_entry(bytes, section, number))
    }

    /// Object `number`'s value, following it into its object stream — and that stream into its
    /// own, if a hostile file nests them — under the same two guards as the chain.
    fn resolve(
        &self,
        bytes: &[u8],
        number: u32,
        caps: &Limits,
    ) -> Result<Option<Object>, CapViolation> {
        let mut chain = Vec::new();
        let mut visited = BTreeSet::new();
        let mut current = number;
        let offset = loop {
            match self.entry(bytes, current) {
                Some(Entry::InUse { offset }) => break offset,
                Some(Entry::Compressed { stream, index }) => {
                    if !visited.insert(stream) {
                        return Err(CapViolation::ObjStmCycle { obj: stream });
                    }
                    chain.push((current, index));
                    let depth = u32::try_from(chain.len()).unwrap_or(u32::MAX);
                    if depth > caps.max_xref_chain {
                        return Err(CapViolation::XrefDepth {
                            depth,
                            limit: caps.max_xref_chain,
                        });
                    }
                    current = stream;
                }
                Some(Entry::Free) | None => return Ok(None),
            }
        };
        let Some(position) = usize::try_from(offset)
            .ok()
            .and_then(|offset| offset.checked_add(self.base))
        else {
            return Ok(None);
        };
        let Some((_, mut object)) = parse_indirect(bytes, position, caps) else {
            return Ok(None);
        };
        // Unwind: each level is an object stream holding the next object down.
        let mut budget = caps.max_decompressed_stream_bytes;
        while let Some((wanted, index)) = chain.pop() {
            let Object::Stream(stream) = object else {
                return Ok(None);
            };
            let Some(found) = object_in_stream(&stream, wanted, index, caps, &mut budget)? else {
                return Ok(None);
            };
            object = found;
        }
        Ok(Some(object))
    }
}

/// The offset after the last `startxref`.
fn startxref(bytes: &[u8]) -> Option<u64> {
    let tail_start = bytes.len().saturating_sub(TAIL_WINDOW);
    let at = rfind(&bytes[tail_start..], b"startxref")? + tail_start;
    let mut lexer = Lexer::new(bytes, at + b"startxref".len());
    match lexer.token()? {
        Token::Int(value) => u64::try_from(value).ok(),
        _ => None,
    }
}

fn offset_value(dictionary: &Dictionary, key: &[u8]) -> Option<u64> {
    dictionary
        .get(key)
        .ok()
        .and_then(|value| value.as_i64().ok())
        .and_then(|value| u64::try_from(value).ok())
}

/// The section at `position` and its trailer dictionary. `Ok(None)` when it does not parse.
fn read_section(
    bytes: &[u8],
    position: usize,
    caps: &Limits,
    budget: &mut u64,
) -> Result<Option<(Section, Dictionary)>, CapViolation> {
    if position >= bytes.len() {
        return Ok(None);
    }
    let mut lexer = Lexer::new(bytes, position);
    lexer.skip_space();
    if bytes[lexer.pos..].starts_with(b"xref") {
        lexer.pos += b"xref".len();
        return Ok(read_table(&mut lexer, caps));
    }
    // A cross-reference stream is an indirect object whose dictionary says `/Type /XRef`.
    let Some((_, Object::Stream(stream))) = parse_indirect(bytes, position, caps) else {
        return Ok(None);
    };
    if stream.dict.get(b"Type").and_then(Object::as_name).ok() != Some(b"XRef".as_slice()) {
        return Ok(None);
    }
    let decoded = match crate::filters::decode_with_budget(&stream, *budget, 0) {
        Ok(decoded) => decoded,
        Err(crate::filters::StreamError::Cap(violation)) => return Err(violation),
        Err(_) => return Ok(None),
    };
    *budget = budget.saturating_sub(u64::try_from(decoded.len()).unwrap_or(u64::MAX));
    let Some(widths) = stream_widths(&stream.dict) else {
        return Ok(None);
    };
    let size = stream
        .dict
        .get(b"Size")
        .and_then(Object::as_i64)
        .ok()
        .and_then(|size| u32::try_from(size).ok())
        .unwrap_or_default();
    let index = match stream.dict.get(b"Index").and_then(Object::as_array) {
        Ok(pairs) => pairs
            .chunks(2)
            .filter_map(|pair| {
                let first = pair
                    .first()?
                    .as_i64()
                    .ok()
                    .and_then(|v| u32::try_from(v).ok())?;
                let count = pair
                    .get(1)?
                    .as_i64()
                    .ok()
                    .and_then(|v| u32::try_from(v).ok())?;
                Some((first, count))
            })
            .collect(),
        Err(_) => vec![(0, size)],
    };
    Ok(Some((
        Section::Stream {
            rows: decoded,
            widths,
            index,
        },
        stream.dict,
    )))
}

/// `/W` of a cross-reference stream: three field widths, none wider than a `u64`.
fn stream_widths(dict: &Dictionary) -> Option<[usize; XREF_STREAM_FIELDS]> {
    let array = dict.get(b"W").and_then(Object::as_array).ok()?;
    let mut widths = [0; XREF_STREAM_FIELDS];
    for (slot, value) in widths.iter_mut().zip(array) {
        *slot = usize::try_from(value.as_i64().ok()?).ok()?;
        if *slot > MAX_FIELD_BYTES {
            return None;
        }
    }
    (widths.iter().sum::<usize>() > 0).then_some(widths)
}

/// A classic table: subsection headers, the entry lines skipped, then `trailer` and its
/// dictionary.
fn read_table(lexer: &mut Lexer<'_>, caps: &Limits) -> Option<(Section, Dictionary)> {
    let mut subsections = Vec::new();
    loop {
        let before = lexer.pos;
        match lexer.token()? {
            Token::Keyword(word) if word == b"trailer" => break,
            Token::Int(first) => {
                let Token::Int(count) = lexer.token()? else {
                    return None;
                };
                let first = u32::try_from(first).ok()?;
                let count = u32::try_from(count).ok()?;
                lexer.skip_space();
                let entries = lexer.pos;
                // Skip `count` entry lines by line, not by twenty bytes: producers that write
                // nineteen-byte lines exist, and PDFium reads them.
                for _ in 0..count {
                    lexer.skip_line()?;
                }
                subsections.push((first, count, entries));
            }
            _ => {
                lexer.pos = before;
                return None;
            }
        }
    }
    let Object::Dictionary(trailer) = lexer.object(caps.max_object_nesting)? else {
        return None;
    };
    Some((Section::Table { subsections }, trailer))
}

fn section_entry(bytes: &[u8], section: &Section, number: u32) -> Option<Entry> {
    match section {
        Section::Table { subsections } => {
            let (first, _, start) = subsections
                .iter()
                .find(|(first, count, _)| number >= *first && number - first < *count)?;
            let mut lexer = Lexer::new(bytes, *start);
            for _ in 0..(number - first) {
                lexer.skip_line()?;
            }
            lexer.skip_space();
            let line = &bytes[lexer.pos..];
            let offset: u64 = std::str::from_utf8(line.get(..OFFSET_DIGITS)?)
                .ok()?
                .parse()
                .ok()?;
            let mut lexer = Lexer::new(bytes, lexer.pos + OFFSET_DIGITS);
            let _generation = lexer.token()?;
            match lexer.token()? {
                Token::Keyword(kind) if kind == b"n" => Some(Entry::InUse { offset }),
                Token::Keyword(kind) if kind == b"f" => Some(Entry::Free),
                _ => None,
            }
        }
        Section::Stream {
            rows,
            widths,
            index,
        } => {
            let width: usize = widths.iter().sum();
            let mut row = 0_usize;
            for (first, count) in index {
                if number >= *first && number - first < *count {
                    row = row.checked_add(usize::try_from(number - first).ok()?)?;
                    let start = row.checked_mul(width)?;
                    let fields = rows.get(start..start.checked_add(width)?)?;
                    let (kind, rest) = fields.split_at(widths[0]);
                    let (second, third) = rest.split_at(widths[1]);
                    // A zero-width type field means type 1 (§7.5.8.2, Table 17).
                    let kind = if widths[0] == 0 {
                        TYPE_IN_USE
                    } else {
                        be(kind)
                    };
                    return match kind {
                        TYPE_FREE => Some(Entry::Free),
                        TYPE_IN_USE => Some(Entry::InUse { offset: be(second) }),
                        TYPE_COMPRESSED => Some(Entry::Compressed {
                            stream: u32::try_from(be(second)).ok()?,
                            index: u32::try_from(be(third)).ok()?,
                        }),
                        _ => None,
                    };
                }
                row = row.checked_add(usize::try_from(*count).ok()?)?;
            }
            None
        }
    }
}

/// A big-endian unsigned field of at most eight bytes.
fn be(field: &[u8]) -> u64 {
    field.iter().fold(0_u64, |value, byte| {
        (value << BITS_PER_BYTE) | u64::from(*byte)
    })
}

/// Object `wanted`, the `index`-th entry of an object stream.
fn object_in_stream(
    stream: &Stream,
    wanted: u32,
    index: u32,
    caps: &Limits,
    budget: &mut u64,
) -> Result<Option<Object>, CapViolation> {
    let decoded = match crate::filters::decode_with_budget(stream, *budget, wanted) {
        Ok(decoded) => decoded,
        Err(crate::filters::StreamError::Cap(violation)) => return Err(violation),
        Err(_) => return Ok(None),
    };
    *budget = budget.saturating_sub(u64::try_from(decoded.len()).unwrap_or(u64::MAX));
    let Some(first) = stream
        .dict
        .get(b"First")
        .and_then(Object::as_i64)
        .ok()
        .and_then(|first| usize::try_from(first).ok())
    else {
        return Ok(None);
    };
    // The header is `index` pairs of (object number, offset from /First) before ours.
    let mut lexer = Lexer::new(&decoded, 0);
    for _ in 0..index {
        let (Some(Token::Int(_)), Some(Token::Int(_))) = (lexer.token(), lexer.token()) else {
            return Ok(None);
        };
    }
    let (Some(Token::Int(number)), Some(Token::Int(offset))) = (lexer.token(), lexer.token())
    else {
        return Ok(None);
    };
    if u32::try_from(number).ok() != Some(wanted) {
        return Ok(None);
    }
    let Some(position) = usize::try_from(offset)
        .ok()
        .and_then(|offset| offset.checked_add(first))
        .filter(|position| *position < decoded.len())
    else {
        return Ok(None);
    };
    Ok(Lexer::new(&decoded, position).object(caps.max_object_nesting))
}

/// `N G obj <value> [stream … endstream]` at `position`.
fn parse_indirect(bytes: &[u8], position: usize, caps: &Limits) -> Option<(ObjectId, Object)> {
    let mut lexer = Lexer::new(bytes, position);
    let Token::Int(number) = lexer.token()? else {
        return None;
    };
    let Token::Int(generation) = lexer.token()? else {
        return None;
    };
    if lexer.token()? != Token::Keyword(b"obj".to_vec()) {
        return None;
    }
    let id = (u32::try_from(number).ok()?, u16::try_from(generation).ok()?);
    let value = lexer.object(caps.max_object_nesting)?;
    let Object::Dictionary(dict) = value else {
        return Some((id, value));
    };
    if lexer.token() != Some(Token::Keyword(b"stream".to_vec())) {
        return Some((id, Object::Dictionary(dict)));
    }
    // The data begins after the EOL that follows `stream` (§7.3.8.1).
    let mut start = lexer.pos;
    if bytes.get(start) == Some(&b'\r') {
        start += 1;
    }
    if bytes.get(start) == Some(&b'\n') {
        start += 1;
    }
    // `/Length` only *locates* the end, and only when `endstream` is really there; it sizes
    // nothing. Otherwise the end is found by looking for it.
    let declared_end = dict
        .get(b"Length")
        .and_then(Object::as_i64)
        .ok()
        .and_then(|length| usize::try_from(length).ok())
        .and_then(|length| start.checked_add(length))
        .filter(|end| {
            let mut probe = Lexer::new(bytes, *end);
            probe.token() == Some(Token::Keyword(b"endstream".to_vec()))
        });
    let end = match declared_end {
        Some(end) => end,
        None => {
            let found = find(&bytes[start..], b"endstream")? + start;
            // Drop the EOL before `endstream`, which is not data.
            let mut end = found;
            if end > start && bytes[end - 1] == b'\n' {
                end -= 1;
            }
            if end > start && bytes[end - 1] == b'\r' {
                end -= 1;
            }
            end
        }
    };
    let content = bytes.get(start..end)?.to_vec();
    let mut stream = Stream::new(dict, content);
    // `Stream::new` rewrites `/Length`; the decoder does not read it either way.
    stream.dict.remove(b"Length");
    Some((id, Object::Stream(stream)))
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn rfind(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .rposition(|window| window == needle)
}

// ---------------------------------------------------------------------------
// A small PDF lexer: enough of §7.2–§7.3 for trailers, catalogues and page-tree roots.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Int(i64),
    Real(f32),
    Name(Vec<u8>),
    String(Vec<u8>, StringFormat),
    Keyword(Vec<u8>),
    DictOpen,
    DictClose,
    ArrayOpen,
    ArrayClose,
}

struct Lexer<'a> {
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

impl<'a> Lexer<'a> {
    fn new(bytes: &'a [u8], pos: usize) -> Self {
        Self { bytes, pos }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn skip_space(&mut self) {
        while let Some(byte) = self.peek() {
            if is_space(byte) {
                self.pos += 1;
            } else if byte == b'%' {
                while let Some(byte) = self.peek() {
                    if byte == b'\n' || byte == b'\r' {
                        break;
                    }
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    /// Past the next end of line. `None` at the end of the data.
    fn skip_line(&mut self) -> Option<()> {
        let rest = self.bytes.get(self.pos..)?;
        let eol = rest
            .iter()
            .position(|byte| *byte == b'\n' || *byte == b'\r')?;
        self.pos += eol;
        if self.peek() == Some(b'\r') {
            self.pos += 1;
        }
        if self.peek() == Some(b'\n') {
            self.pos += 1;
        }
        Some(())
    }

    fn token(&mut self) -> Option<Token> {
        self.skip_space();
        let byte = self.peek()?;
        match byte {
            b'<' if self.bytes.get(self.pos + 1) == Some(&b'<') => {
                self.pos += 2;
                Some(Token::DictOpen)
            }
            b'>' if self.bytes.get(self.pos + 1) == Some(&b'>') => {
                self.pos += 2;
                Some(Token::DictClose)
            }
            b'[' => {
                self.pos += 1;
                Some(Token::ArrayOpen)
            }
            b']' => {
                self.pos += 1;
                Some(Token::ArrayClose)
            }
            b'/' => {
                self.pos += 1;
                Some(Token::Name(self.name()))
            }
            b'(' => {
                self.pos += 1;
                self.literal_string()
            }
            b'<' => {
                self.pos += 1;
                self.hex_string()
            }
            _ => {
                let start = self.pos;
                while let Some(byte) = self.peek() {
                    if is_space(byte) || is_delimiter(byte) {
                        break;
                    }
                    self.pos += 1;
                }
                if self.pos == start {
                    // A lone delimiter this lexer has no token for: consume it so no caller spins.
                    self.pos += 1;
                    return Some(Token::Keyword(vec![byte]));
                }
                let word = &self.bytes[start..self.pos];
                let text = std::str::from_utf8(word).ok();
                if let Some(value) = text.and_then(|text| text.parse::<i64>().ok()) {
                    return Some(Token::Int(value));
                }
                if let Some(value) = text
                    .filter(|text| text.bytes().any(|b| b == b'.'))
                    .and_then(|text| text.parse::<f32>().ok())
                {
                    return Some(Token::Real(value));
                }
                Some(Token::Keyword(word.to_vec()))
            }
        }
    }

    fn name(&mut self) -> Vec<u8> {
        let mut name = Vec::new();
        while let Some(byte) = self.peek() {
            if is_space(byte) || is_delimiter(byte) {
                break;
            }
            self.pos += 1;
            if byte == b'#' {
                let hex = self.bytes.get(self.pos..self.pos + 2);
                if let Some(value) = hex
                    .and_then(|hex| std::str::from_utf8(hex).ok())
                    .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                {
                    name.push(value);
                    self.pos += 2;
                    continue;
                }
            }
            name.push(byte);
        }
        name
    }

    /// A literal string, with balanced parentheses and backslash escapes kept as written: the
    /// walk never reads a string's value, only needs to get past it.
    fn literal_string(&mut self) -> Option<Token> {
        let mut depth = 1_usize;
        let start = self.pos;
        while let Some(byte) = self.peek() {
            self.pos += 1;
            match byte {
                b'\\' => self.pos += 1,
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        let raw = self.bytes.get(start..self.pos - 1)?.to_vec();
                        return Some(Token::String(raw, StringFormat::Literal));
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn hex_string(&mut self) -> Option<Token> {
        let start = self.pos;
        let end = self
            .bytes
            .get(start..)?
            .iter()
            .position(|byte| *byte == b'>')?
            + start;
        self.pos = end + 1;
        Some(Token::String(
            self.bytes[start..end].to_vec(),
            StringFormat::Hexadecimal,
        ))
    }

    /// One object, nested at most `depth` levels. Deeper nesting is refused (`None`) rather than
    /// recursed into: the bound is on this parser's stack, which a file must not choose.
    fn object(&mut self, depth: u32) -> Option<Object> {
        let token = self.token()?;
        self.object_from(token, depth)
    }

    fn object_from(&mut self, token: Token, depth: u32) -> Option<Object> {
        match token {
            Token::Int(number) => {
                // `N G R` is a reference: look two tokens ahead and rewind if it is not.
                let save = self.pos;
                if let (Some(Token::Int(generation)), Some(Token::Keyword(r))) =
                    (self.token(), self.token())
                {
                    if r == b"R" {
                        if let (Ok(number), Ok(generation)) =
                            (u32::try_from(number), u16::try_from(generation))
                        {
                            return Some(Object::Reference((number, generation)));
                        }
                    }
                }
                self.pos = save;
                Some(Object::Integer(number))
            }
            Token::Real(value) => Some(Object::Real(value)),
            Token::Name(name) => Some(Object::Name(name)),
            Token::String(value, format) => Some(Object::String(value, format)),
            Token::Keyword(word) => match word.as_slice() {
                b"true" => Some(Object::Boolean(true)),
                b"false" => Some(Object::Boolean(false)),
                b"null" => Some(Object::Null),
                _ => None,
            },
            Token::ArrayOpen => {
                let inner = depth.checked_sub(1)?;
                let mut items = Vec::new();
                loop {
                    match self.token()? {
                        Token::ArrayClose => return Some(Object::Array(items)),
                        token => items.push(self.object_from(token, inner)?),
                    }
                }
            }
            Token::DictOpen => {
                let inner = depth.checked_sub(1)?;
                let mut dictionary = Dictionary::new();
                loop {
                    match self.token()? {
                        Token::DictClose => return Some(Object::Dictionary(dictionary)),
                        Token::Name(key) => {
                            let value = self.object(inner)?;
                            dictionary.set(key, value);
                        }
                        _ => return None,
                    }
                }
            }
            Token::DictClose | Token::ArrayClose => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Rows 14.4, 14.5 and 14.6.
// ---------------------------------------------------------------------------

/// A one-page file whose trailer chain is `sections` classic xref sections long. Each section
/// after the first is an incremental update that re-lists object 1 and points `/Prev` at the
/// one before; `prev_of_last` overrides the newest section's `/Prev` (to make a cycle).
/// Chooses the newest section's `/Prev` from the offsets of the sections before it.
#[cfg(test)]
type PickPrev<'a> = Option<&'a dyn Fn(&[usize]) -> usize>;

#[cfg(test)]
fn chained(sections: usize, prev_of_last: PickPrev<'_>) -> Vec<u8> {
    chained_declaring(sections, prev_of_last, 1)
}

/// [`chained`], with the page-tree root declaring `count` pages over its one kid.
#[cfg(test)]
fn chained_declaring(sections: usize, prev_of_last: PickPrev<'_>, count: usize) -> Vec<u8> {
    let mut pdf = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    let pages = format!("<< /Type /Pages /Count {count} /Kids [3 0 R] >>");
    for (number, body) in [
        "<< /Type /Catalog /Pages 2 0 R >>",
        pages.as_str(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
    ]
    .iter()
    .enumerate()
    {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", number + 1).as_bytes());
    }
    let mut sections_at = Vec::new();
    for index in 0..sections {
        let at = pdf.len();
        pdf.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
        for offset in &offsets {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        let prev = if index + 1 == sections {
            prev_of_last.map(|choose| choose(&sections_at))
        } else {
            None
        }
        .or_else(|| sections_at.last().copied());
        let prev = prev
            .map(|prev| format!(" /Prev {prev}"))
            .unwrap_or_default();
        pdf.extend_from_slice(format!("trailer\n<< /Size 4 /Root 1 0 R{prev} >>\n").as_bytes());
        sections_at.push(at);
    }
    let last = sections_at.last().copied().unwrap_or_default();
    pdf.extend_from_slice(format!("startxref\n{last}\n%%EOF\n").as_bytes());
    pdf
}

/// Test 14.4. Five hundred sections against a cap of 128: refused at the 129th, by depth.
#[test]
fn xref_chain_depth_is_capped() {
    let caps = Limits::default();
    match walk_xref_chain(&chained(500, None), &caps) {
        Err(CapViolation::XrefDepth { depth, limit }) => {
            assert_eq!(limit, caps.max_xref_chain);
            assert_eq!(
                depth,
                caps.max_xref_chain + 1,
                "the walk stops one past the cap"
            );
        }
        Err(other) => panic!("a long chain is a depth failure, not {other}"),
        Ok(walk) => panic!("a 500-section chain walked to depth {}", walk.depth),
    }

    // Exactly the cap is allowed, and a normal file is one section.
    let at_cap = usize::try_from(caps.max_xref_chain).unwrap_or(usize::MAX);
    let walk = walk_xref_chain(&chained(at_cap, None), &caps).expect("the cap itself is legal");
    assert_eq!(walk.depth, caps.max_xref_chain);

    // And the refusal happens before PDFium parses a byte: through the backend it is the cap,
    // not whatever PDFium would have made of the file.
    use crate::inspect::PdfOpen;
    let backend = crate::pdfium::PdfiumBackend::bind().expect("PDFium is vendored");
    match backend.open_with_limits(&chained(500, None), None, &caps) {
        Err(crate::error::PdfError::Cap(CapViolation::XrefDepth { .. })) => {}
        Err(other) => panic!("expected the depth cap at the door, got {other}"),
        Ok(_) => panic!("a 500-section chain opened"),
    }
}

/// Test 14.5. A section whose `/Prev` is itself, and a two-section loop: both end at once by the
/// visited set and say *cycle* — a depth counter alone would also stop them, eventually, and
/// report the wrong thing.
#[test]
fn xref_cycle_terminates_via_visited_set() {
    let caps = Limits::default();
    let self_loop = chained(1, Some(&|_: &[usize]| usize::MAX));
    // `usize::MAX` stands for "my own offset", which the builder cannot know in advance; patch it.
    let own = {
        let text = String::from_utf8_lossy(&self_loop);
        let start = text.rfind("startxref\n").expect("startxref") + "startxref\n".len();
        text[start..].lines().next().expect("offset").to_owned()
    };
    let self_loop = String::from_utf8_lossy(&self_loop)
        .replace(&format!("/Prev {}", usize::MAX), &format!("/Prev {own}"))
        .into_bytes();
    match walk_xref_chain(&self_loop, &caps) {
        Err(CapViolation::XrefCycle { offset }) => {
            assert_eq!(
                offset.to_string(),
                own,
                "the cycle names the section it returned to"
            );
        }
        other => panic!(
            "a self-referential /Prev is a cycle, got {:?}",
            other.map(|w| w.depth)
        ),
    }

    // Three sections, the newest pointing back at the oldest's successor: 3 -> 2 -> 1 is the
    // chain, and the newest's /Prev is the middle one, so the walk meets it twice.
    let two_loop = chained(3, Some(&|earlier: &[usize]| earlier[1]));
    let text = String::from_utf8_lossy(&two_loop).into_owned();
    // Make the middle section point at the newest, closing the loop.
    let newest = text.rfind("xref\n0 4").expect("newest section");
    let middle_trailer = text[..newest].rfind("/Prev").expect("middle /Prev");
    let end = middle_trailer + text[middle_trailer..].find(" >>").expect("end of trailer");
    let looped = format!("{}/Prev {newest}{}", &text[..middle_trailer], &text[end..]);
    match walk_xref_chain(looped.as_bytes(), &caps) {
        Err(CapViolation::XrefCycle { .. }) => {}
        other => panic!(
            "a two-section loop is a cycle, got {:?}",
            other.map(|w| w.depth)
        ),
    }
}

/// Test 14.6. The catalogue declares 5 000 pages and holds one. The refusal comes from the
/// declaration, before anything counts or loads a page — and the file is broken past its page
/// tree root in a way PDFium itself refuses, so a check that ran after PDFium's open would report
/// PDFium's error instead of the cap.
#[test]
fn page_cap_checked_before_first_page_load() {
    use crate::inspect::PdfOpen;

    let caps = Limits::default();
    let pdf = chained_declaring(1, None, 5000);
    let walk = walk_xref_chain(&pdf, &caps).expect("one section");
    assert_eq!(walk.declared_page_count(&pdf, &caps), Ok(Some(5000)));

    let backend = crate::pdfium::PdfiumBackend::bind().expect("PDFium is vendored");
    match backend.open_with_limits(&pdf, None, &caps) {
        Err(crate::error::PdfError::Cap(CapViolation::Pages { declared, limit })) => {
            assert_eq!((declared, limit), (5000, caps.max_pages));
        }
        Err(other) => panic!("expected the page cap, got {other}"),
        Ok(_) => panic!(
            "5 000 declared pages opened against a cap of {}",
            caps.max_pages
        ),
    }

    // Before PDFium: truncate the file after the page-tree root. PDFium cannot open what is
    // left; the cap still answers, because it only needed the catalogue and the root.
    let text = String::from_utf8_lossy(&pdf).into_owned();
    let page_object = text.find("3 0 obj").expect("the page object");
    let mut broken = pdf.clone();
    for byte in &mut broken[page_object..page_object + "3 0 obj".len()] {
        *byte = b'#';
    }
    match backend.open_with_limits(&broken, None, &caps) {
        Err(crate::error::PdfError::Cap(CapViolation::Pages { declared, .. })) => {
            assert_eq!(declared, 5000);
        }
        Err(other) => panic!("the cap must answer before PDFium does, got {other}"),
        Ok(_) => panic!("a broken 5 000-page claim opened"),
    }
}

/// Every committed fixture walks cleanly, and where the walk can read the page count it agrees
/// with PDFium's. The pre-walk is on the path of every conversion; a false refusal here would be a
/// book that no longer converts.
#[test]
fn every_fixture_walks_and_agrees_with_pdfium_on_its_page_count() {
    use crate::inspect::PdfOpen;

    let backend = crate::pdfium::PdfiumBackend::bind().expect("PDFium is vendored");
    let caps = Limits::default();
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus/fixtures");
    let typst = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/fixtures");
    let mut read = 0;
    for directory in [
        root.join("handmade"),
        root.join("mutations"),
        root.join("scanned"),
        typst,
    ] {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        let mut paths: Vec<_> = entries
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .collect();
        paths.sort();
        for path in paths
            .iter()
            .filter(|path| path.extension().is_some_and(|ext| ext == "pdf"))
        {
            let bytes = std::fs::read(path).expect("fixture");
            let walk = walk_xref_chain(&bytes, &caps)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            let Ok(Some(declared)) = walk.declared_page_count(&bytes, &caps) else {
                continue;
            };
            let Ok(document) = backend.open_with_limits(&bytes, None, &caps) else {
                continue;
            };
            assert_eq!(
                declared,
                u64::from(document.page_count()),
                "{}: the catalogue and PDFium disagree",
                path.display()
            );
            read += 1;
        }
    }
    assert!(read > 30, "only {read} fixtures had a readable page count");
}

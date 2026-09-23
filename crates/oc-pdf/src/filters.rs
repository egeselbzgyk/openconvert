//! Stream filters, decoded through [`BoundedInflate`] (PHASE 14 detail 2).
//!
//! Every filter chain this crate decodes itself — page content, the XMP packet, the structural
//! flags read from content — goes through [`decode_stream`]: `FlateDecode`, `LZWDecode`,
//! `RunLengthDecode`, `ASCII85Decode`, `ASCIIHexDecode`, in any nesting. Each is a `Read`
//! adapter, and each layer is read through a `BoundedInflate` whose ceiling is what the chain
//! has left of `limits.max_decompressed_stream_bytes`, so the budget is shared by the whole
//! chain: a hundred nested `FlateDecode`s cost what one does. The layer fails closed at the
//! ceiling and its output buffer never grows past it.
//!
//! **`/Length` is not read here.** The stream's bytes are what the parser found between
//! `stream` and `endstream`; the dictionary's `/Length` is a claim by whoever wrote the file,
//! and a decoder that sizes anything from it has trusted an attacker. The one pre-check is on
//! the raw bytes actually held: raw bytes already past the ceiling are refused without decoding.
//!
//! Image codecs (`DCTDecode`, `JPXDecode`, `CCITTFaxDecode`, `JBIG2Decode`) are not decoded here;
//! images reach PDFium's decoders only through the pixel cap (`limits::decode_image_checked`).

use std::io::Read;

use lopdf::{Dictionary, Object, Stream};
use oc_core::limits::{CapViolation, Limits};

use crate::limits::{BoundedInflate, CeilingReached};

/// How much is asked of a decoder per read. Not a limit — the ceiling is — only the grain at
/// which the output buffer grows, so it can stop exactly at the ceiling instead of doubling past
/// it.
const READ_GRAIN: usize = 64 * 1024;

/// PDF 32000-1 §7.4.4.4: `/Predictor` values. 1 is none, 2 is TIFF, 10–15 are the PNG family.
const PREDICTOR_TIFF: i64 = 2;
const PREDICTOR_PNG_FIRST: i64 = 10;
const PREDICTOR_PNG_LAST: i64 = 15;
/// The `/DecodeParms` defaults (§7.4.4.4, Table 8).
const DEFAULT_COLUMNS: i64 = 1;
const DEFAULT_COLORS: i64 = 1;
const DEFAULT_BITS: i64 = 8;
const BITS_PER_BYTE: usize = 8;
/// LZW's initial code size less one, as `weezl` counts it (§7.4.4.2: 9-bit codes to start).
const LZW_MIN_SIZE: u8 = 8;
/// `RunLengthDecode`'s end-of-data length byte and the literal/run boundary (§7.4.5).
const RUN_LENGTH_EOD: u8 = 128;
const RUN_LENGTH_REPEAT_BASE: usize = 257;
/// ASCII85 (§7.4.3): the digit offset, the base, the all-zero shorthand and the group length.
const A85_FIRST: u8 = b'!';
const A85_LAST: u8 = b'u';
const A85_ZERO_GROUP: u8 = b'z';
const A85_BASE: u32 = 85;
const A85_GROUP: usize = 5;
const A85_EOD: u8 = b'~';
const HEX_EOD: u8 = b'>';
/// A zlib header is two bytes; the lenient fallback reads the deflate data behind it.
const ZLIB_HEADER_LEN: usize = 2;

/// Why a stream could not be decoded.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum StreamError {
    /// The chain expanded past its ceiling. The one outcome a caller must never swallow.
    #[error(transparent)]
    Cap(#[from] CapViolation),
    /// A filter this decoder does not implement (an image codec, or a name no spec defines).
    #[error("unsupported stream filter {0}")]
    Unsupported(String),
    /// The data is not what the filter says it is.
    #[error("corrupt {filter} data: {message}")]
    Corrupt {
        filter: &'static str,
        message: String,
    },
}

/// Decode `stream`'s filter chain within `caps.max_decompressed_stream_bytes`.
///
/// `obj` is the object number, carried into the refusal so a report can say which stream.
pub fn decode_stream(stream: &Stream, caps: &Limits, obj: u32) -> Result<Vec<u8>, StreamError> {
    decode_with_budget(stream, caps.max_decompressed_stream_bytes, obj)
}

/// [`decode_stream`] with an explicit ceiling: what a caller has left of a shared budget.
pub fn decode_with_budget(stream: &Stream, limit: u64, obj: u32) -> Result<Vec<u8>, StreamError> {
    let raw = stream.content.as_slice();
    // The fast pre-check, on bytes actually held rather than on the declared `/Length`.
    let raw_len = u64::try_from(raw.len()).unwrap_or(u64::MAX);
    if raw_len > limit {
        return Err(CapViolation::StreamBytes {
            produced: raw_len,
            limit,
            obj,
        }
        .into());
    }

    let filters = filter_names(&stream.dict)?;
    if filters.is_empty() {
        return Ok(raw.to_vec());
    }
    let params = decode_params(&stream.dict, filters.len());

    let mut data = raw.to_vec();
    let mut spent = 0_u64;
    for (filter, params) in filters.iter().zip(params) {
        let remaining = limit.saturating_sub(spent);
        let decoded =
            decode_layer(filter, &data, params, remaining).map_err(|error| match error {
                StreamError::Cap(CapViolation::StreamBytes { produced, .. }) => {
                    StreamError::Cap(CapViolation::StreamBytes {
                        produced: spent.saturating_add(produced),
                        limit,
                        obj,
                    })
                }
                other => other,
            })?;
        spent = spent.saturating_add(u64::try_from(decoded.len()).unwrap_or(u64::MAX));
        data = decoded;
    }
    Ok(data)
}

/// The `/Filter` entry as a list of names, in decoding order. Absent is empty.
fn filter_names(dict: &Dictionary) -> Result<Vec<Vec<u8>>, StreamError> {
    let Ok(filter) = dict.get(b"Filter") else {
        return Ok(Vec::new());
    };
    match filter {
        Object::Name(name) => Ok(vec![name.clone()]),
        Object::Array(items) => items
            .iter()
            .map(|item| {
                item.as_name().map(<[u8]>::to_vec).map_err(|_| {
                    StreamError::Unsupported("a /Filter entry that is not a name".into())
                })
            })
            .collect(),
        _ => Err(StreamError::Unsupported(
            "a /Filter that is neither name nor array".into(),
        )),
    }
}

/// `/DecodeParms`, one entry per filter: a dictionary applies to a single filter, an array is
/// parallel to `/Filter`, and `null` or a missing entry is "no parameters".
fn decode_params(dict: &Dictionary, filters: usize) -> Vec<Option<&Dictionary>> {
    let entry = dict.get(b"DecodeParms").or_else(|_| dict.get(b"DP")).ok();
    match entry {
        Some(Object::Dictionary(params)) => {
            let mut all = vec![None; filters];
            if let Some(first) = all.first_mut() {
                *first = Some(params);
            }
            all
        }
        Some(Object::Array(items)) => (0..filters)
            .map(|index| items.get(index).and_then(|item| item.as_dict().ok()))
            .collect(),
        _ => vec![None; filters],
    }
}

fn decode_layer(
    filter: &[u8],
    input: &[u8],
    params: Option<&Dictionary>,
    limit: u64,
) -> Result<Vec<u8>, StreamError> {
    match filter {
        b"FlateDecode" | b"Fl" => {
            let decoded = inflate(input, limit)?;
            predict(decoded, params, "FlateDecode")
        }
        b"LZWDecode" | b"LZW" => {
            let early_change = params
                .and_then(|p| p.get(b"EarlyChange").ok())
                .and_then(|value| value.as_i64().ok())
                .is_none_or(|value| value != 0);
            let decoded = read_bounded(LzwReader::new(input, early_change), limit, "LZWDecode")?;
            predict(decoded, params, "LZWDecode")
        }
        b"RunLengthDecode" | b"RL" => {
            read_bounded(RunLengthReader::new(input), limit, "RunLengthDecode")
        }
        b"ASCII85Decode" | b"A85" => {
            read_bounded(Ascii85Reader::new(input), limit, "ASCII85Decode")
        }
        b"ASCIIHexDecode" | b"AHx" => {
            read_bounded(AsciiHexReader::new(input), limit, "ASCIIHexDecode")
        }
        other => Err(StreamError::Unsupported(
            String::from_utf8_lossy(other).into_owned(),
        )),
    }
}

/// `FlateDecode`, leniently: a stream whose zlib header or checksum is damaged (common in
/// encrypted files) is retried as raw deflate, and data after a mid-stream error is dropped
/// rather than failing the stream — what `lopdf` and PDFium both do. The ceiling is never
/// lenient.
fn inflate(input: &[u8], limit: u64) -> Result<Vec<u8>, StreamError> {
    let mut output = Vec::new();
    let zlib = flate2::read::ZlibDecoder::new(input);
    match read_into(BoundedInflate::new(zlib, limit), &mut output) {
        Ok(()) => return Ok(output),
        Err(ReadStop::Ceiling(produced)) => return Err(ceiling(produced, limit)),
        Err(ReadStop::Corrupt(_)) if !output.is_empty() => return Ok(output),
        Err(ReadStop::Corrupt(_)) => {}
    }
    let raw = input.get(ZLIB_HEADER_LEN..).unwrap_or_default();
    let deflate = flate2::read::DeflateDecoder::new(raw);
    match read_into(BoundedInflate::new(deflate, limit), &mut output) {
        Ok(()) | Err(ReadStop::Corrupt(_)) => Ok(output),
        Err(ReadStop::Ceiling(produced)) => Err(ceiling(produced, limit)),
    }
}

fn ceiling(produced: u64, limit: u64) -> StreamError {
    StreamError::Cap(CapViolation::StreamBytes {
        produced,
        limit,
        // Filled in by `decode_with_budget`, which knows the object.
        obj: 0,
    })
}

/// Read `reader` to its end through a `BoundedInflate`, failing closed at the ceiling.
fn read_bounded<R: Read>(
    reader: R,
    limit: u64,
    filter: &'static str,
) -> Result<Vec<u8>, StreamError> {
    let mut output = Vec::new();
    match read_into(BoundedInflate::new(reader, limit), &mut output) {
        Ok(()) => Ok(output),
        Err(ReadStop::Ceiling(produced)) => Err(ceiling(produced, limit)),
        Err(ReadStop::Corrupt(message)) => Err(StreamError::Corrupt { filter, message }),
    }
}

enum ReadStop {
    Ceiling(u64),
    Corrupt(String),
}

/// Append everything `reader` yields to `output`, growing it one grain at a time so its
/// capacity never runs ahead of what the ceiling allows.
fn read_into<R: Read>(mut reader: BoundedInflate<R>, output: &mut Vec<u8>) -> Result<(), ReadStop> {
    loop {
        let grain = usize::try_from(reader.remaining())
            .unwrap_or(usize::MAX)
            .clamp(1, READ_GRAIN);
        let start = output.len();
        if output.try_reserve_exact(grain).is_err() {
            return Err(ReadStop::Ceiling(reader.produced()));
        }
        output.resize(start + grain, 0);
        match reader.read(&mut output[start..]) {
            Ok(0) => {
                output.truncate(start);
                return Ok(());
            }
            Ok(read) => output.truncate(start + read),
            Err(error) => {
                output.truncate(start);
                if let Some(reached) = CeilingReached::find(&error) {
                    return Err(ReadStop::Ceiling(reached.produced));
                }
                if error.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(ReadStop::Corrupt(error.to_string()));
            }
        }
    }
}

/// Undo a `/Predictor`. Neither family makes data larger, so the layer's ceiling still holds.
fn predict(
    data: Vec<u8>,
    params: Option<&Dictionary>,
    filter: &'static str,
) -> Result<Vec<u8>, StreamError> {
    let Some(params) = params else {
        return Ok(data);
    };
    let int = |key: &[u8], default: i64| {
        params
            .get(key)
            .ok()
            .and_then(|value| value.as_i64().ok())
            .unwrap_or(default)
    };
    let predictor = int(b"Predictor", 1);
    if predictor != PREDICTOR_TIFF
        && !(PREDICTOR_PNG_FIRST..=PREDICTOR_PNG_LAST).contains(&predictor)
    {
        return Ok(data);
    }
    let corrupt = |message: &str| StreamError::Corrupt {
        filter,
        message: message.to_owned(),
    };
    let positive = |value: i64| usize::try_from(value).ok().filter(|value| *value > 0);
    let columns =
        positive(int(b"Columns", DEFAULT_COLUMNS)).ok_or_else(|| corrupt("bad /Columns"))?;
    let colors = positive(int(b"Colors", DEFAULT_COLORS)).ok_or_else(|| corrupt("bad /Colors"))?;
    let bits = positive(int(b"BitsPerComponent", DEFAULT_BITS))
        .ok_or_else(|| corrupt("bad /BitsPerComponent"))?;
    // The row width is the file's claim; a row longer than the data cannot be a row, and is
    // refused before anything is sized from it.
    let row_bits = columns
        .checked_mul(colors)
        .and_then(|samples| samples.checked_mul(bits))
        .ok_or_else(|| corrupt("predictor row width overflows"))?;
    let row = row_bits.div_ceil(BITS_PER_BYTE);
    if row > data.len() {
        return Err(corrupt("predictor row is wider than the data"));
    }
    let pixel = (colors * bits).div_ceil(BITS_PER_BYTE).max(1);

    if predictor == PREDICTOR_TIFF {
        return tiff_predictor(data, row, colors, bits).ok_or_else(|| corrupt("TIFF predictor"));
    }
    lopdf::filters::png::decode_frame(&data, pixel, row)
        .map_err(|error| corrupt(&format!("PNG predictor: {error}")))
}

/// TIFF predictor 2 at 8 or 16 bits per component: each sample is the running sum of the
/// differences along its row. Other depths are refused (`None`) rather than guessed at.
fn tiff_predictor(mut data: Vec<u8>, row: usize, colors: usize, bits: usize) -> Option<Vec<u8>> {
    const BYTE: usize = 8;
    const WORD: usize = 16;
    match bits {
        BYTE => {
            for line in data.chunks_mut(row) {
                for index in colors..line.len() {
                    line[index] = line[index].wrapping_add(line[index - colors]);
                }
            }
            Some(data)
        }
        WORD => {
            let stride = colors * 2;
            for line in data.chunks_mut(row) {
                let mut index = stride;
                while index + 1 < line.len() {
                    let previous =
                        u16::from_be_bytes([line[index - stride], line[index - stride + 1]]);
                    let current = u16::from_be_bytes([line[index], line[index + 1]]);
                    let [high, low] = current.wrapping_add(previous).to_be_bytes();
                    line[index] = high;
                    line[index + 1] = low;
                    index += 2;
                }
            }
            Some(data)
        }
        _ => None,
    }
}

/// `LZWDecode` as a `Read`: `weezl` decodes into whatever buffer the caller offers, which is
/// exactly the shape `BoundedInflate` needs.
struct LzwReader<'a> {
    decoder: weezl::decode::Decoder,
    input: &'a [u8],
    done: bool,
}

impl<'a> LzwReader<'a> {
    fn new(input: &'a [u8], early_change: bool) -> Self {
        let decoder = if early_change {
            weezl::decode::Decoder::with_tiff_size_switch(weezl::BitOrder::Msb, LZW_MIN_SIZE)
        } else {
            weezl::decode::Decoder::new(weezl::BitOrder::Msb, LZW_MIN_SIZE)
        };
        Self {
            decoder,
            input,
            done: false,
        }
    }
}

impl Read for LzwReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.done || buf.is_empty() {
            return Ok(0);
        }
        loop {
            let result = self.decoder.decode_bytes(self.input, buf);
            self.input = self.input.get(result.consumed_in..).unwrap_or_default();
            match result.status {
                // A damaged tail ends the data, as it does in `lopdf`: what decoded is kept.
                Err(_) | Ok(weezl::LzwStatus::Done) => {
                    self.done = true;
                    return Ok(result.consumed_out);
                }
                Ok(weezl::LzwStatus::NoProgress) => {
                    self.done = true;
                    return Ok(result.consumed_out);
                }
                Ok(weezl::LzwStatus::Ok) if result.consumed_out > 0 => {
                    return Ok(result.consumed_out);
                }
                Ok(weezl::LzwStatus::Ok) if self.input.is_empty() => {
                    self.done = true;
                    return Ok(0);
                }
                Ok(weezl::LzwStatus::Ok) => {}
            }
        }
    }
}

/// `RunLengthDecode` as a `Read`. A run of up to 128 copies costs two input bytes, so this is a
/// bomb multiplier of 64 on its own — which is why it is bounded like the others.
struct RunLengthReader<'a> {
    input: &'a [u8],
    /// What is left of the current run: the byte and how many more copies, or a literal slice.
    pending: Pending<'a>,
}

enum Pending<'a> {
    None,
    Repeat(u8, usize),
    Literal(&'a [u8]),
}

impl<'a> RunLengthReader<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            pending: Pending::None,
        }
    }
}

impl Read for RunLengthReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut written = 0;
        while written < buf.len() {
            match &mut self.pending {
                Pending::Repeat(byte, count) if *count > 0 => {
                    let take = (*count).min(buf.len() - written);
                    buf[written..written + take].fill(*byte);
                    *count -= take;
                    written += take;
                }
                Pending::Literal(bytes) if !bytes.is_empty() => {
                    let take = bytes.len().min(buf.len() - written);
                    buf[written..written + take].copy_from_slice(&bytes[..take]);
                    *bytes = &bytes[take..];
                    written += take;
                }
                _ => {
                    let Some((&length, rest)) = self.input.split_first() else {
                        break;
                    };
                    if length == RUN_LENGTH_EOD {
                        self.input = &[];
                        break;
                    }
                    if length < RUN_LENGTH_EOD {
                        // A literal of `length + 1` bytes; a truncated one keeps what is there.
                        let take = (usize::from(length) + 1).min(rest.len());
                        self.pending = Pending::Literal(&rest[..take]);
                        self.input = &rest[take..];
                    } else {
                        let Some((&byte, rest)) = rest.split_first() else {
                            self.input = &[];
                            break;
                        };
                        self.pending =
                            Pending::Repeat(byte, RUN_LENGTH_REPEAT_BASE - usize::from(length));
                        self.input = rest;
                    }
                }
            }
        }
        Ok(written)
    }
}

/// `ASCIIHexDecode` as a `Read`: whitespace skipped, `>` ends the data, a final odd digit is
/// followed by an implied `0`.
struct AsciiHexReader<'a> {
    input: &'a [u8],
    ended: bool,
}

impl<'a> AsciiHexReader<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            ended: false,
        }
    }

    fn next_digit(&mut self) -> std::io::Result<Option<u8>> {
        while let Some((&byte, rest)) = self.input.split_first() {
            self.input = rest;
            if byte == HEX_EOD {
                self.ended = true;
                return Ok(None);
            }
            if byte.is_ascii_whitespace() {
                continue;
            }
            return char::from(byte)
                .to_digit(16)
                .and_then(|digit| u8::try_from(digit).ok())
                .map(Some)
                .ok_or_else(|| std::io::Error::other("invalid hexadecimal digit"));
        }
        self.ended = true;
        Ok(None)
    }
}

impl Read for AsciiHexReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        const NIBBLE: u32 = 4;
        let mut written = 0;
        while written < buf.len() && !self.ended {
            let Some(high) = self.next_digit()? else {
                break;
            };
            let low = self.next_digit()?.unwrap_or(0);
            buf[written] = (high << NIBBLE) | low;
            written += 1;
        }
        Ok(written)
    }
}

/// `ASCII85Decode` as a `Read`: groups of five digits to four bytes, `z` for four zeros, `~>`
/// ends the data, a final partial group yields one byte fewer than its digits.
struct Ascii85Reader<'a> {
    input: &'a [u8],
    out: [u8; 4],
    out_len: usize,
    out_pos: usize,
    ended: bool,
}

impl<'a> Ascii85Reader<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            out: [0; 4],
            out_len: 0,
            out_pos: 0,
            ended: false,
        }
    }

    /// Decode the next group into `out`. `Ok(false)` at the end of the data.
    fn fill(&mut self) -> std::io::Result<bool> {
        let mut digits = [0_u8; A85_GROUP];
        let mut count = 0;
        while count < A85_GROUP {
            let Some((&byte, rest)) = self.input.split_first() else {
                self.ended = true;
                break;
            };
            self.input = rest;
            match byte {
                A85_ZERO_GROUP if count == 0 => {
                    self.out = [0; 4];
                    self.out_len = 4;
                    self.out_pos = 0;
                    return Ok(true);
                }
                A85_EOD => {
                    self.ended = true;
                    break;
                }
                A85_FIRST..=A85_LAST => {
                    digits[count] = byte - A85_FIRST;
                    count += 1;
                }
                byte if byte.is_ascii_whitespace() => {}
                _ => return Err(std::io::Error::other("invalid ASCII85 digit")),
            }
        }
        if count == 0 {
            return Ok(false);
        }
        if count == 1 {
            return Err(std::io::Error::other("a final ASCII85 group of one digit"));
        }
        // A partial group is padded with the highest digit, and yields `count - 1` bytes.
        let highest = A85_LAST - A85_FIRST;
        for digit in digits.iter_mut().skip(count) {
            *digit = highest;
        }
        let mut value: u32 = 0;
        for digit in digits {
            value = value
                .checked_mul(A85_BASE)
                .and_then(|value| value.checked_add(u32::from(digit)))
                .ok_or_else(|| std::io::Error::other("an ASCII85 group past 2^32"))?;
        }
        self.out = value.to_be_bytes();
        self.out_len = count - 1;
        self.out_pos = 0;
        Ok(true)
    }
}

impl Read for Ascii85Reader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut written = 0;
        while written < buf.len() {
            if self.out_pos < self.out_len {
                let take = (self.out_len - self.out_pos).min(buf.len() - written);
                buf[written..written + take]
                    .copy_from_slice(&self.out[self.out_pos..self.out_pos + take]);
                self.out_pos += take;
                written += take;
                continue;
            }
            if self.ended || !self.fill()? {
                break;
            }
        }
        Ok(written)
    }
}

// ---------------------------------------------------------------------------
// Tests. Rows 14.2 and 14.3 are in `limits.rs`, beside `BoundedInflate`; these hold the decoders
// to the answers `lopdf` gives, so bounding them did not change what they decode.
// ---------------------------------------------------------------------------

#[cfg(test)]
fn stream_with(filters: &[&str], content: Vec<u8>) -> Stream {
    let mut dictionary = Dictionary::new();
    if !filters.is_empty() {
        let names: Vec<Object> = filters
            .iter()
            .map(|name| Object::Name(name.as_bytes().to_vec()))
            .collect();
        dictionary.set("Filter", Object::Array(names));
    }
    Stream::new(dictionary, content)
}

/// Each filter against a known vector, and the chain in nesting.
#[test]
fn every_filter_decodes_its_known_vector() {
    let caps = Limits::default();
    let decode = |filters: &[&str], content: &[u8]| {
        decode_stream(&stream_with(filters, content.to_vec()), &caps, 1).expect("decodes")
    };

    assert_eq!(decode(&["ASCIIHexDecode"], b"48 65 6C6c6F>"), b"Hello");
    assert_eq!(
        decode(&["ASCIIHexDecode"], b"4>"),
        b"@",
        "a final odd digit is padded with 0"
    );
    assert_eq!(
        decode(&["ASCII85Decode"], b"87cURD]i,\"Ebo80~>"),
        b"Hello World!"
    );
    assert_eq!(decode(&["ASCII85Decode"], b"z~>"), [0_u8; 4]);
    assert_eq!(
        decode(
            &["RunLengthDecode"],
            &[2, b'a', b'b', b'c', 254, b'x', 128, b'!']
        ),
        b"abcxxx",
        "a literal of three, a run of three, then end-of-data"
    );

    // LZW, with and without the early change, against weezl's own encoder.
    let text = b"TOBEORNOTTOBEORTOBEORNOT".repeat(20);
    let lzw = weezl::encode::Encoder::with_tiff_size_switch(weezl::BitOrder::Msb, LZW_MIN_SIZE)
        .encode(&text)
        .expect("encodes");
    assert_eq!(decode(&["LZWDecode"], &lzw), text);

    // Nesting: hex around flate around the text.
    let mut flate = stream_with(&[], text.clone());
    flate.compress().expect("compresses");
    let hex: Vec<u8> = flate
        .content
        .iter()
        .flat_map(|byte| format!("{byte:02X}").into_bytes())
        .collect();
    assert_eq!(decode(&["ASCIIHexDecode", "FlateDecode"], &hex), text);
}

/// A chain shares one budget: nesting a bomb does not multiply the ceiling.
#[test]
fn a_filter_chain_shares_one_budget() {
    let tight = Limits {
        max_decompressed_stream_bytes: 1000,
        ..Limits::default()
    };
    // Incompressible text: deflate cannot shrink it, so the hex layer yields about 510 bytes and
    // the flate layer 500 — each inside 1000, both together not.
    let mut state = 0x2545_f491_u32;
    let inner: Vec<u8> = (0..500)
        .map(|_| {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            state.to_be_bytes()[0]
        })
        .collect();
    let mut flate = stream_with(&["FlateDecode"], Vec::new());
    flate.dict.remove(b"Filter");
    flate.set_plain_content(inner.clone());
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    std::io::Write::write_all(&mut encoder, &inner).expect("in memory");
    flate.content = encoder.finish().expect("in memory");
    assert!(flate.content.len() > 500, "{}", flate.content.len());
    let hex: Vec<u8> = flate
        .content
        .iter()
        .flat_map(|byte| format!("{byte:02X}").into_bytes())
        .collect();
    let stream = stream_with(&["ASCIIHexDecode", "FlateDecode"], hex);
    match decode_stream(&stream, &tight, 9) {
        Err(StreamError::Cap(CapViolation::StreamBytes { limit, obj, .. })) => {
            assert_eq!((limit, obj), (1000, 9));
        }
        other => panic!("two layers past one budget must be refused: {other:?}"),
    }
}

/// Regression: on every committed fixture, every content stream decodes to exactly what `lopdf`
/// decodes it to. The bounded chain is a replacement for `lopdf`'s decoder on these paths, and a
/// replacement that changed a byte would change a book.
#[test]
fn our_filter_chain_agrees_with_lopdf_on_every_fixture() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus/fixtures");
    let mut compared = 0;
    for directory in ["handmade", "mutations", "scanned"] {
        let Ok(entries) = std::fs::read_dir(root.join(directory)) else {
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
            let bytes = std::fs::read(path).expect("committed fixture");
            let Ok(document) = lopdf::Document::load_mem(&bytes) else {
                continue;
            };
            for (id, object) in &document.objects {
                let Ok(stream) = object.as_stream() else {
                    continue;
                };
                let Ok(theirs) = stream.decompressed_content() else {
                    continue;
                };
                let ours = decode_stream(stream, &Limits::default(), id.0);
                assert_eq!(
                    ours.as_deref().ok(),
                    Some(theirs.as_slice()),
                    "{} object {id:?} decodes differently",
                    path.display()
                );
                compared += 1;
            }
        }
    }
    assert!(compared > 50, "only {compared} streams compared");
}

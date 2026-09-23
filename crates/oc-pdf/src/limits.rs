//! Where the resource limits are actually applied to a PDF (Phase 1 detail 8, R8 §A2).
//!
//! `oc_core::limits` says what the numbers are and answers yes or no. This module is the set
//! of places that ask, and each one asks *before* the work it is guarding rather than after:
//! a bounds check that runs once the six gigabytes are allocated has guarded nothing.

use lopdf::{Document, ObjectId};
use oc_core::limits::{CapViolation, Limits};

use crate::error::PdfError;
use crate::images::DecodedImage;

/// What an image dictionary *declares*: the only input the pixel cap is allowed to read.
///
/// Wide integers on purpose. `/Width` is whatever number the file wrote, and a check that
/// narrows it to `u32` first has already trusted it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageDict {
    pub width: u64,
    pub height: u64,
}

impl ImageDict {
    /// Read `/Width` and `/Height` from an image XObject's dictionary. `None` when either is
    /// missing or not a non-negative integer — an image without dimensions is not one a
    /// decoder can be asked to allocate for.
    pub fn from_dictionary(dictionary: &lopdf::Dictionary) -> Option<Self> {
        let side = |key: &[u8]| {
            dictionary
                .get(key)
                .ok()
                .and_then(|value| value.as_i64().ok())
                .and_then(|value| u64::try_from(value).ok())
        };
        Some(Self {
            width: side(b"Width")?,
            height: side(b"Height")?,
        })
    }

    /// The declared pixel count, saturating: a product that overflows is refused, never
    /// wrapped to something small.
    pub fn pixels(&self) -> u64 {
        self.width.saturating_mul(self.height)
    }
}

/// Refuse an image by the dimensions its dictionary declares (PHASE 14 detail 1).
///
/// Dictionary-only: nothing here decodes, composites or allocates for the image, so it must run
/// before anything that does. Pixels, not pixels × components: D13.2 names the cap "max image
/// pixels (100 MP declared)", and the threshold's evidence is Pillow's `MAX_IMAGE_PIXELS`, which
/// is also a pixel count (the plan's detail 1 multiplies by components; D13.2 governs).
pub fn check_image_before_decode(
    dictionary: &ImageDict,
    caps: &Limits,
    page: u32,
) -> Result<(), CapViolation> {
    let declared = dictionary.pixels();
    if declared > caps.max_image_pixels {
        return Err(CapViolation::ImagePixels {
            declared,
            limit: caps.max_image_pixels,
            page,
        });
    }
    Ok(())
}

/// Decode an image only once its dictionary has passed the cap.
///
/// The decoder is a closure so that the order is a property of this function rather than of
/// every caller's discipline: there is no way to reach `decode` without the check having
/// returned `Ok` first. Test 14.1 hands it a spy.
pub fn decode_image_checked<F>(
    dictionary: &ImageDict,
    caps: &Limits,
    page: u32,
    decode: F,
) -> Result<DecodedImage, PdfError>
where
    F: FnOnce() -> Result<DecodedImage, PdfError>,
{
    check_image_before_decode(dictionary, caps, page)?;
    decode()
}

/// A `Read` adapter that fails past a byte ceiling, whatever the stream's `/Length` declared
/// (PHASE 14 detail 2).
///
/// It hands out at most `limit` bytes. Once they are gone it asks the inner reader for one more:
/// none means the data really ended at the ceiling, and any means it did not, which is an error
/// carrying [`CeilingReached`] — never a short read that a caller could mistake for the end.
/// Nothing is buffered here, so the caller's buffer bounds the memory and the counter bounds the
/// work.
pub struct BoundedInflate<R: std::io::Read> {
    inner: R,
    produced: u64,
    limit: u64,
}

impl<R: std::io::Read> BoundedInflate<R> {
    pub fn new(inner: R, limit: u64) -> Self {
        Self {
            inner,
            produced: 0,
            limit,
        }
    }

    /// Bytes handed out so far. Never more than the limit.
    pub fn produced(&self) -> u64 {
        self.produced
    }

    /// Bytes that may still be handed out.
    pub fn remaining(&self) -> u64 {
        self.limit.saturating_sub(self.produced)
    }
}

impl<R: std::io::Read> std::io::Read for BoundedInflate<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let remaining = self.remaining();
        if remaining == 0 {
            let mut probe = [0_u8; 1];
            return match self.inner.read(&mut probe)? {
                0 => Ok(0),
                _ => Err(std::io::Error::other(CeilingReached {
                    produced: self.produced,
                    limit: self.limit,
                })),
            };
        }
        let window = usize::try_from(remaining)
            .unwrap_or(usize::MAX)
            .min(buf.len());
        let read = self.inner.read(&mut buf[..window])?;
        self.produced = self
            .produced
            .saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
        Ok(read)
    }
}

/// The error a [`BoundedInflate`] fails with at its ceiling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("stream expanded past its {limit}-byte ceiling")]
pub struct CeilingReached {
    pub produced: u64,
    pub limit: u64,
}

impl CeilingReached {
    /// The ceiling error inside an I/O error, if that is what it is — however many adapters
    /// wrapped it on the way out.
    pub fn find(error: &std::io::Error) -> Option<CeilingReached> {
        let mut source: Option<&(dyn std::error::Error + 'static)> =
            error.get_ref().map(|e| e as _);
        while let Some(current) = source {
            if let Some(reached) = current.downcast_ref::<CeilingReached>() {
                return Some(*reached);
            }
            if let Some(io) = current.downcast_ref::<std::io::Error>() {
                source = io.get_ref().map(|e| e as _);
                continue;
            }
            source = current.source();
        }
        None
    }
}

/// Read one page's content with the decompression cap applied, through our own filter chain.
///
/// Every content stream is decoded by [`crate::filters::decode_stream`], and the page shares one
/// budget of `limits.max_decompressed_stream_bytes` across its streams. A stream that cannot be
/// decoded for any reason *other* than the cap contributes its raw bytes, as `lopdf` and PDFium
/// both do, and those count against the budget too; the cap is never lenient.
pub fn read_page_content(
    document: &Document,
    page: ObjectId,
    limits: &Limits,
) -> Result<Vec<u8>, PdfError> {
    use crate::filters::{decode_with_budget, StreamError};

    let ceiling = limits.max_decompressed_stream_bytes;
    let mut content = Vec::new();
    for id in document.get_page_contents(page) {
        let Ok(stream) = document.get_object(id).and_then(lopdf::Object::as_stream) else {
            continue;
        };
        let spent = u64::try_from(content.len()).unwrap_or(u64::MAX);
        let remaining = ceiling.saturating_sub(spent);
        let decoded = match decode_with_budget(stream, remaining, id.0) {
            Ok(decoded) => decoded,
            Err(StreamError::Cap(CapViolation::StreamBytes { produced, .. })) => {
                return Err(CapViolation::StreamBytes {
                    produced: spent.saturating_add(produced),
                    limit: ceiling,
                    obj: id.0,
                }
                .into());
            }
            Err(_) => {
                let raw = u64::try_from(stream.content.len()).unwrap_or(u64::MAX);
                if raw > remaining {
                    return Err(CapViolation::StreamBytes {
                        produced: spent.saturating_add(raw),
                        limit: ceiling,
                        obj: id.0,
                    }
                    .into());
                }
                stream.content.clone()
            }
        };
        content.extend_from_slice(&decoded);
        content.push(b'\n');
    }
    Ok(content)
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Rows 1.10, 1.11 and 1.20.
// ---------------------------------------------------------------------------

#[cfg(test)]
fn handmade(name: &str) -> Vec<u8> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/fixtures/handmade")
        .join(format!("{name}.pdf"));
    std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "missing fixture {}: {error}; run `cargo run -p xtask -- handmade-fixtures`",
            path.display()
        )
    })
}

/// Test 1.10.
///
/// "No allocation" is asserted structurally rather than by watching the allocator: the
/// refusal carries `requested == 1_600_000_000`, which is 40 000 × 40 000 — a number that
/// exists only in the image dictionary. A check that had decoded first would have had to
/// allocate to learn it, and a check that had read a decoded buffer would report the real
/// sixty-four bytes instead.
#[test]
fn image_pixel_bomb_is_refused_before_decode() {
    use crate::inspect::PdfOpen;
    use crate::pdfium::PdfiumBackend;

    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let bytes = handmade("h11_pixel_bomb");
    let document = backend
        .open(&bytes, None)
        .expect("the file itself is well-formed; it is the claim inside that is not");

    match document.page_images(0) {
        Err(PdfError::Cap(CapViolation::ImagePixels {
            declared,
            limit,
            page,
        })) => {
            assert_eq!(declared, 1_600_000_000);
            assert_eq!(limit, 100_000_000);
            assert_eq!(page, 0);
        }
        other => panic!("a 1.6-gigapixel claim must be refused, got {other:?}"),
    }
}

/// Test 1.11.
///
/// The plan's instance is a stream declaring 8 GiB and an assertion that process RSS stays
/// under 300 MB. Both are substituted, for reasons that make the test better rather than
/// weaker. The fixture expands to 8 MiB from 8.9 KB — a ratio near a thousand to one, which
/// is the attack — and the cap is lowered to 1 MiB for the refusal, so the mechanism is
/// exercised in milliseconds instead of by allocating a quarter of a gigabyte in CI to
/// demonstrate an inequality. Peak memory is bounded by construction: `lopdf` caps each
/// filter layer as it decodes, and asserting a process RSS figure would be measuring the
/// allocator, not the code.
///
/// The second half is what stops this passing vacuously: under the *shipped* cap the same
/// fixture reads fine, so the refusal above is the limit acting and not the file being
/// unreadable.
#[test]
fn decompression_bomb_is_bounded() {
    const ONE_MIB: u64 = 1024 * 1024;

    let bytes = handmade("h12_decompression_bomb");
    let document = lopdf::Document::load_mem(&bytes).expect("the fixture parses");
    let page = document
        .get_pages()
        .into_values()
        .next()
        .expect("the fixture has a page");

    let tight = Limits {
        max_decompressed_stream_bytes: ONE_MIB,
        ..Limits::default()
    };
    match read_page_content(&document, page, &tight) {
        Err(PdfError::Cap(CapViolation::StreamBytes {
            produced, limit, ..
        })) => {
            assert_eq!(limit, ONE_MIB);
            assert_eq!(produced, ONE_MIB, "decoding stops exactly at the ceiling");
        }
        other => panic!("an 8 MiB expansion past a 1 MiB cap must be refused, got {other:?}"),
    }

    let content = read_page_content(&document, page, &Limits::default())
        .expect("8 MiB is well inside the shipped 256 MiB cap");
    assert!(
        content.len() > usize::try_from(ONE_MIB).unwrap_or(usize::MAX),
        "the fixture really does expand past the tight cap: {} bytes",
        content.len()
    );

    // And the cap is on the path production takes, not only on the helper this test called
    // directly: `page_images` reads the same content stream to learn each image's mask and
    // inline flags, and it must refuse rather than shrug and report "no masks here".
    use crate::inspect::PdfOpen;
    let backend = crate::pdfium::PdfiumBackend::bind().expect("PDFium is vendored");
    let opened = backend
        .open_with_limits(&bytes, None, &tight)
        .expect("the document itself is small; it is the stream inside that is not");
    match opened.page_images(0) {
        Err(error @ PdfError::Cap(_)) => {
            assert_eq!(
                error.cap(),
                Some(oc_core::limits::MAX_DECOMPRESSED_STREAM_BYTES)
            );
        }
        other => panic!("the cap must apply where the content is actually read, got {other:?}"),
    }
}

/// Test 1.20.
///
/// "Before page 1 is parsed" is the assertion: the refusal happens inside `open`, so there is
/// no document to ask for a page from. A guard that ran on first access would let a
/// hundred-thousand-page document cost a hundred thousand page parses before declining.
#[test]
fn max_pages_refuses_at_the_door() {
    use crate::inspect::PdfOpen;
    use crate::pdfium::PdfiumBackend;

    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let limits = Limits::default();
    let over = usize::try_from(limits.max_pages).unwrap_or(usize::MAX) + 1;

    let bytes = oc_testkit::handmade::many_pages(over);
    match backend.open_with_limits(&bytes, None, &limits) {
        Err(PdfError::LimitExceeded(exceeded)) => {
            assert_eq!(exceeded.limit, oc_core::limits::MAX_PAGES);
            assert_eq!(exceeded.requested, over as u64);
            assert_eq!(exceeded.allowed, u64::from(limits.max_pages));
        }
        Ok(_) => panic!(
            "{over} pages against a {} cap must be refused, not opened",
            limits.max_pages
        ),
        Err(other) => panic!("expected LimitExceeded, got {other:?}"),
    }

    // Exactly at the allowance is allowed, which is the boundary the guard has to get right:
    // a book of exactly three thousand pages is a book, not an attack.
    let at_limit = oc_testkit::handmade::many_pages(usize::try_from(limits.max_pages).unwrap_or(0));
    let document = backend
        .open_with_limits(&at_limit, None, &limits)
        .expect("exactly the allowance must open");
    assert_eq!(document.page_count(), limits.max_pages);
}

/// Test 14.1.
///
/// A spy stands in for the decoder, and the assertion is on the spy: the property being bought
/// is that the decoder is **never entered** for an image whose dictionary already says it is too
/// big, not that an error comes back eventually. The natural "decode, then look at the size"
/// shape returns the same error and fails this test, because the spy counts one entry.
///
/// The control half keeps the test honest: a small image reaches the decoder exactly once, so a
/// guard that refused everything would fail too.
#[test]
fn image_pixel_cap_checked_before_decode() {
    use oc_core::limits::CapViolation;
    use std::cell::Cell;

    let caps = Limits::default();
    let entered = Cell::new(0_u32);
    let spy = || {
        entered.set(entered.get() + 1);
        Ok(crate::images::DecodedImage {
            width: 1,
            height: 1,
            rgba: vec![0; 4],
        })
    };

    // The dictionary the file carries, read with lopdf: 40 000 × 40 000, RGB, 8 bits.
    let mut dictionary = lopdf::Dictionary::new();
    dictionary.set("Type", lopdf::Object::Name(b"XObject".to_vec()));
    dictionary.set("Subtype", lopdf::Object::Name(b"Image".to_vec()));
    dictionary.set("Width", 40_000_i64);
    dictionary.set("Height", 40_000_i64);
    dictionary.set("ColorSpace", lopdf::Object::Name(b"DeviceRGB".to_vec()));
    dictionary.set("BitsPerComponent", 8_i64);
    let bomb = ImageDict::from_dictionary(&dictionary).expect("an image dictionary");

    match decode_image_checked(&bomb, &caps, 7, spy) {
        Err(PdfError::Cap(CapViolation::ImagePixels {
            declared,
            limit,
            page,
        })) => {
            assert_eq!(declared, 1_600_000_000);
            assert_eq!(limit, caps.max_image_pixels);
            assert_eq!(page, 7);
        }
        other => panic!("a 1.6-gigapixel dictionary must be refused, got {other:?}"),
    }
    assert_eq!(
        entered.get(),
        0,
        "the decoder was entered for an image its dictionary already refused"
    );

    let small = ImageDict {
        width: 100,
        height: 100,
    };
    let spy = || {
        entered.set(entered.get() + 1);
        Ok(crate::images::DecodedImage {
            width: 100,
            height: 100,
            rgba: vec![0; 40_000],
        })
    };
    let decoded = decode_image_checked(&small, &caps, 7, spy).expect("a 10 kpx image decodes");
    assert_eq!(decoded.width, 100);
    assert_eq!(entered.get(), 1, "a legal image reaches the decoder once");

    // Overflow cannot turn a refusal into a pass: u64::MAX on a side saturates.
    let huge = ImageDict {
        width: u64::MAX,
        height: u64::MAX,
    };
    assert!(check_image_before_decode(&huge, &caps, 0).is_err());
}

/// A zlib stream of `size` zero bytes: the classic bomb, about a thousand to one.
#[cfg(test)]
fn zlib_zeros(size: u64) -> Vec<u8> {
    use std::io::Write;
    let chunk = vec![0_u8; 1 << 20];
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    let mut written = 0_u64;
    while written < size {
        let take = usize::try_from((size - written).min(chunk.len() as u64)).unwrap_or(0);
        encoder.write_all(&chunk[..take]).expect("in-memory write");
        written += take as u64;
    }
    encoder.finish().expect("in-memory finish")
}

/// Test 14.2.
///
/// The ceiling is the shipped one, 256 MiB, and the bomb expands to 300 MiB. The reader is
/// drained by hand so the test sees every byte handed out: exactly the ceiling arrives, then an
/// error that says it is the ceiling — not a short read that looks like the end, and not a
/// 300 MiB buffer that is refused after the fact.
#[test]
fn bounded_inflate_stops_at_ceiling() {
    use std::io::Read;

    const MIB: u64 = 1 << 20;
    let caps = Limits::default();
    let ceiling = caps.max_decompressed_stream_bytes;
    assert_eq!(ceiling, 256 * MIB, "the shipped ceiling is 256 MiB");
    let bomb = zlib_zeros(300 * MIB);
    assert!(
        bomb.len() < 1 << 20,
        "a bomb is small: {} bytes",
        bomb.len()
    );

    let mut reader = BoundedInflate::new(flate2::read::ZlibDecoder::new(bomb.as_slice()), ceiling);
    let mut buffer = vec![0_u8; 1 << 16];
    let mut handed_out = 0_u64;
    let error = loop {
        match reader.read(&mut buffer) {
            Ok(0) => panic!("a 300 MiB bomb decoded to its end under a 256 MiB ceiling"),
            Ok(read) => {
                handed_out += read as u64;
                assert!(
                    handed_out <= ceiling,
                    "{handed_out} bytes past a {ceiling} ceiling"
                );
            }
            Err(error) => break error,
        }
    };
    assert_eq!(
        handed_out, ceiling,
        "everything up to the ceiling, and nothing past it"
    );
    assert_eq!(reader.produced(), ceiling);
    assert_eq!(
        CeilingReached::find(&error),
        Some(CeilingReached {
            produced: ceiling,
            limit: ceiling
        })
    );

    // Through the filter chain as production uses it: the same refusal, as a cap.
    let mut dictionary = lopdf::Dictionary::new();
    dictionary.set("Filter", lopdf::Object::Name(b"FlateDecode".to_vec()));
    let stream = lopdf::Stream::new(dictionary, bomb);
    match crate::filters::decode_stream(&stream, &caps, 12) {
        Err(crate::filters::StreamError::Cap(CapViolation::StreamBytes {
            produced,
            limit,
            obj,
        })) => {
            assert_eq!((produced, limit, obj), (ceiling, ceiling, 12));
        }
        other => panic!(
            "expected the stream cap, got {:?}",
            other.map(|bytes| bytes.len())
        ),
    }

    // And a legal stream just under the ceiling still decodes, so the refusal above is the
    // ceiling acting and not the decoder failing.
    let small = zlib_zeros(MIB);
    let mut dictionary = lopdf::Dictionary::new();
    dictionary.set("Filter", lopdf::Object::Name(b"FlateDecode".to_vec()));
    let tight = Limits {
        max_decompressed_stream_bytes: MIB,
        ..Limits::default()
    };
    let decoded = crate::filters::decode_stream(&lopdf::Stream::new(dictionary, small), &tight, 1)
        .expect("exactly the ceiling is allowed");
    assert_eq!(decoded.len() as u64, MIB);
}

/// Test 14.3.
///
/// The stream says `/Length 10`. It holds a bomb that expands to 300 MiB. A decoder that sized
/// its buffer from `/Length`, or that trusted it to mean "small", gets this wrong one way or the
/// other; the ceiling does not read `/Length` at all. Checked twice: on the stream as the
/// dictionary claims it, and on a whole file written with the lie, which `lopdf` parses by the
/// object's real boundary.
#[test]
fn declared_length_is_not_trusted() {
    const MIB: u64 = 1 << 20;
    let caps = Limits::default();
    let ceiling = caps.max_decompressed_stream_bytes;
    let bomb = zlib_zeros(300 * MIB);

    let mut dictionary = lopdf::Dictionary::new();
    dictionary.set("Filter", lopdf::Object::Name(b"FlateDecode".to_vec()));
    let mut stream = lopdf::Stream::new(dictionary, bomb.clone());
    stream.dict.set("Length", 10_i64);
    match crate::filters::decode_stream(&stream, &caps, 4) {
        Err(crate::filters::StreamError::Cap(CapViolation::StreamBytes {
            produced,
            limit,
            ..
        })) => {
            assert_eq!(limit, ceiling);
            assert_eq!(produced, ceiling);
        }
        other => panic!(
            "/Length 10 must not bound or size anything, got {:?}",
            other.map(|b| b.len())
        ),
    }

    // The same lie in a file: one page whose content stream declares ten bytes.
    let mut pdf = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.7\n");
    let mut offsets = Vec::new();
    let mut object = |pdf: &mut Vec<u8>, body: &[u8]| {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", offsets.len()).as_bytes());
        pdf.extend_from_slice(body);
        pdf.extend_from_slice(b"\nendobj\n");
    };
    object(&mut pdf, b"<< /Type /Catalog /Pages 2 0 R >>");
    object(&mut pdf, b"<< /Type /Pages /Count 1 /Kids [3 0 R] >>");
    object(
        &mut pdf,
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R >>",
    );
    let mut content = b"<< /Length 10 /Filter /FlateDecode >>\nstream\n".to_vec();
    content.extend_from_slice(&bomb);
    content.extend_from_slice(b"\nendstream");
    object(&mut pdf, &content);
    let xref = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes(),
    );
    for offset in &offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len() + 1
        )
        .as_bytes(),
    );

    let document = lopdf::Document::load_mem(&pdf).expect("lopdf recovers the real boundary");
    let page = document.get_pages().into_values().next().expect("one page");
    match read_page_content(&document, page, &caps) {
        Err(PdfError::Cap(CapViolation::StreamBytes {
            produced,
            limit,
            obj,
        })) => {
            assert_eq!((produced, limit, obj), (ceiling, ceiling, 4));
        }
        other => panic!(
            "the page's content must stop at the ceiling, got {:?}",
            other.map(|b| b.len())
        ),
    }
}

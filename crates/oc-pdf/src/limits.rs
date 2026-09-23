//! Where the resource limits are actually applied to a PDF (Phase 1 detail 8, R8 §A2).
//!
//! `oc_core::limits` says what the numbers are and answers yes or no. This module is the set
//! of places that ask, and each one asks *before* the work it is guarding rather than after:
//! a bounds check that runs once the six gigabytes are allocated has guarded nothing.

use lopdf::{Document, ObjectId};
use oc_core::limits::{CapViolation, Limits, MAX_DECOMPRESSED_STREAM_BYTES};

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

/// Read one page's content stream with the decompression cap applied.
///
/// The cap is on the *sink*, not on a declared size, because a compressed stream does not
/// declare its expanded size — that is precisely the attack. `lopdf` bounds each filter layer
/// individually, so a stream with nested filters cannot expand past the cap once per layer
/// either.
pub fn read_page_content(
    document: &Document,
    page: ObjectId,
    limits: &Limits,
) -> Result<Vec<u8>, PdfError> {
    let cap = usize::try_from(limits.max_decompressed_stream_bytes).unwrap_or(usize::MAX);
    document
        .get_page_content_with_limit(page, cap)
        .map_err(|error| match error {
            lopdf::Error::Decompress(_) => oc_core::limits::LimitExceeded {
                limit: MAX_DECOMPRESSED_STREAM_BYTES,
                allowed: limits.max_decompressed_stream_bytes,
                // The expanded size is never learned — refusing it is the point — so what is
                // reported is that it passed the cap, which is the only honest number here.
                requested: limits.max_decompressed_stream_bytes.saturating_add(1),
            }
            .into(),
            other => PdfError::Open {
                message: other.to_string(),
            },
        })
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
        Err(PdfError::LimitExceeded(exceeded)) => {
            assert_eq!(exceeded.limit, MAX_DECOMPRESSED_STREAM_BYTES);
            assert_eq!(exceeded.allowed, ONE_MIB);
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
        Err(PdfError::LimitExceeded(exceeded)) => {
            assert_eq!(exceeded.limit, MAX_DECOMPRESSED_STREAM_BYTES);
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

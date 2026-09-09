//! VD-d: does PDFium composite masks and colour spaces, or must we? (Phase 1 failure modes.)
//!
//! The plan proposes comparing `get_processed_image()` against "a `pypdfium2` reference in
//! `eval/`". That comparison cannot answer the question. **`pypdfium2` wraps the same PDFium
//! library** — it is a different binding, not a different implementation — so the two agreeing
//! tells us only that `pdfium-render` and `pypdfium2` call the same C function correctly, and
//! nothing at all about whether the compositing is right.
//!
//! What answers it is a **known-answer test**. We author the fixtures, so we know what the
//! composited pixels must be: `h09`'s soft mask is opaque over the bottom half of the image
//! and fully transparent over the top half, because the builder writes exactly that. If
//! `get_processed_image` returns those alphas, it applied the mask; if it returns all-opaque,
//! it did not, and the image policy needs its own compositing step.
//!
//! Coverage against the plan's ten cases is recorded in `docs/DECISIONS_LOG.md`: this file
//! covers what can be authored honestly with `pdf-writer`, and names what cannot.

use oc_model::extract::ImageId;
use oc_pdf::inspect::PdfOpen;
use oc_pdf::pdfium::PdfiumBackend;

fn decoded(name: &str) -> oc_pdf::images::DecodedImage {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/fixtures/handmade")
        .join(format!("{name}.pdf"));
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "missing fixture {}: {error}; run `cargo run -p xtask -- handmade-fixtures`",
            path.display()
        )
    });

    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend.open(&bytes, None).expect("the fixture opens");
    document
        .image_bytes(0, ImageId(0))
        .expect("the page's first image decodes")
}

/// The question VD-d exists to answer, asked as a known answer.
///
/// `h09`'s soft mask is written by `oc_testkit::handmade`: the first half of its samples are
/// 0 and the rest are 255, over an 8×8 image stored row-major — so the top four rows are
/// transparent and the bottom four are opaque. Anything else means the mask was not applied.
#[test]
fn processed_image_applies_the_soft_mask() {
    let image = decoded("h09_image_smask");
    assert_eq!((image.width, image.height), (8, 8), "{image:?}");

    assert!(
        image.has_transparency(),
        "the soft mask was not applied: every pixel came back opaque"
    );

    // The exact shape, not merely "some transparency": a compositor that inverted the mask
    // would also report transparency, and would be wrong in the way that loses a figure.
    let alpha = |x, y| image.pixel(x, y).map(|px| px[3]);
    for x in 0..image.width {
        for y in 0..4 {
            assert_eq!(
                alpha(x, y),
                Some(0),
                "pixel ({x},{y}) should be transparent"
            );
        }
        for y in 4..8 {
            assert_eq!(alpha(x, y), Some(255), "pixel ({x},{y}) should be opaque");
        }
    }
}

/// The negative case, through the same code path.
///
/// Without it, the test above would pass against a decoder that made everything transparent.
#[test]
fn an_unmasked_image_comes_back_fully_opaque() {
    let image = decoded("h05_invisible_layer");
    assert!(
        !image.has_transparency(),
        "an image with no mask must not acquire one"
    );
    assert_eq!(image.pixel(0, 0).map(|px| px[3]), Some(255));
}

/// The grey the builder writes survives the round trip.
///
/// A DeviceGray image is stored as one byte per pixel and comes back as RGBA, so this is the
/// colour-space half of the spike: 235 grey must arrive as (235, 235, 235), not as 235 in one
/// channel and zero in the others.
#[test]
fn device_gray_is_expanded_to_rgb() {
    let image = decoded("h05_invisible_layer");
    // The builder fills the whole image with a single grey level.
    let pixel = image.pixel(3, 3).expect("the image has pixels");
    assert_eq!([pixel[0], pixel[1], pixel[2]], [235, 235, 235], "{pixel:?}");
}

/// The pixel limit is checked before the decode, on this path too.
///
/// `image_bytes` is the one entry point that actually allocates for an image, so the guard
/// matters more here than anywhere: `page_images` only reads a dictionary.
#[test]
fn decoding_a_pixel_bomb_is_refused() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/fixtures/handmade/h11_pixel_bomb.pdf");
    let bytes = std::fs::read(path).expect("h11 is committed");

    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend.open(&bytes, None).expect("the fixture opens");

    match document.image_bytes(0, ImageId(0)) {
        Err(oc_pdf::error::PdfError::LimitExceeded(exceeded)) => {
            assert_eq!(exceeded.limit, oc_core::limits::MAX_IMAGE_PIXELS);
        }
        other => panic!("a 1.6-gigapixel claim must be refused before decoding, got {other:?}"),
    }
}

/// A stencil mask (`/ImageMask true`) — the other way a PDF makes part of an image
/// transparent, and the older one.
///
/// One bit per pixel, and **the polarity is the opposite of the obvious one**: PDF 32000-1
/// §8.9.6.2 says a sample of *0* paints with the current colour and a sample of *1* leaves the
/// page unchanged. This test asserted it backwards on the first run and PDFium was right.
///
/// `h14`'s top four rows are 0 and its bottom four are 1, so the top must come back painted
/// and the bottom transparent. A scanned book's stamps and logos are usually stencils rather
/// than soft masks, so this is not an exotic case.
#[test]
fn processed_image_applies_a_stencil_mask() {
    let image = decoded("h14_stencil_mask");
    assert_eq!((image.width, image.height), (8, 8), "{image:?}");
    assert!(
        image.has_transparency(),
        "the stencil mask was not applied: every pixel came back opaque"
    );

    let alpha = |x, y| image.pixel(x, y).map(|px| px[3]);
    for x in 0..image.width {
        for y in 0..4 {
            assert_eq!(alpha(x, y), Some(255), "sample 0 paints: ({x},{y})");
        }
        for y in 4..8 {
            assert_eq!(alpha(x, y), Some(0), "sample 1 masks out: ({x},{y})");
        }
    }

    // Painted with the current fill colour, which this content stream never sets — so black.
    assert_eq!(
        image.pixel(0, 0).map(|px| [px[0], px[1], px[2]]),
        Some([0, 0, 0])
    );
}

/// An Indexed colour space is resolved through its palette.
///
/// `h15` declares a two-entry palette — entry 0 red, entry 1 blue — and fills the top half
/// with index 0 and the bottom with index 1. A decoder that ignored the palette and read the
/// sample as a grey level would return near-black for both halves, which this tells apart
/// from the right answer at a glance.
#[test]
fn processed_image_resolves_an_indexed_palette() {
    use oc_testkit::handmade::INDEXED_PALETTE;

    let image = decoded("h15_indexed_colour");
    let rgb = |x, y| image.pixel(x, y).map(|px| [px[0], px[1], px[2]]);

    assert_eq!(rgb(2, 1), Some(INDEXED_PALETTE[0]), "top half is palette 0");
    assert_eq!(
        rgb(2, 6),
        Some(INDEXED_PALETTE[1]),
        "bottom half is palette 1"
    );
}

/// An inline image decodes through the same call as an XObject.
///
/// `h10` writes its image with `BI … ID … EI` rather than as a resource, and PDFium presents
/// it as a page object like any other — so `image_bytes` needs no special case. Asserted
/// rather than assumed, because "no special case needed" is exactly the kind of claim that is
/// true until it is not.
#[test]
fn an_inline_image_decodes_like_any_other() {
    let image = decoded("h10_inline_image");
    assert_eq!((image.width, image.height), (8, 8), "{image:?}");
    assert!(!image.has_transparency());
}

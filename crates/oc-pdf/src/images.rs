//! Image extraction: what a page draws that is not text (IMPLEMENTATION_PLAN Phase 1 detail 4).
//!
//! Two backends answer between them, because neither can answer alone. PDFium knows where an
//! image landed on the page and how many pixels it has; only the file itself knows whether
//! that image carries a soft mask or was written inline in the content stream, and PDFium
//! exposes neither. So geometry and pixels come from PDFium and the two structural flags come
//! from `lopdf`, matched by draw order.

use oc_model::extract::ImageKind;
#[cfg(test)]
use oc_model::extract::ImageRef;

/// One image, decoded and composited: straight RGBA8 pixels, ready to encode.
///
/// Not part of the serialised IR — pixels do not belong in canonical JSON — so this is an
/// `oc-pdf` type rather than an `oc-model` one. What reaches the IR is `ImageRef`, which
/// describes the image; this is the image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    /// Four bytes per pixel, row-major, top-left origin.
    pub rgba: Vec<u8>,
}

impl DecodedImage {
    /// The pixel at `(x, y)`, or `None` outside the image.
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let start = usize::try_from((y * self.width + x) * 4).ok()?;
        self.rgba.get(start..start + 4)?.try_into().ok()
    }

    /// Whether any pixel is less than fully opaque.
    ///
    /// The question the SMask spike (VD-d) exists to answer: did compositing actually happen?
    pub fn has_transparency(&self) -> bool {
        self.rgba
            .as_chunks::<4>()
            .0
            .iter()
            .any(|px| px[3] != u8::MAX)
    }
}

/// Classify an image by how it sits on the page.
///
/// The order matters and is the plan's: a full-page background is checked before a strip,
/// because a tall narrow page's background is both. `Unknown` is not produced here — every
/// image has a bbox, so every image is classifiable; the variant exists for a backend that
/// cannot place one.
pub fn classify_image(
    area_ratio: f32,
    width_pt: f32,
    height_pt: f32,
    thresholds: &oc_core::thresholds::Thresholds,
) -> ImageKind {
    let images = &thresholds.images;
    if f64::from(area_ratio) >= images.full_page_area_ratio {
        return ImageKind::FullPageBackground;
    }
    let (long, short) = if width_pt >= height_pt {
        (width_pt, height_pt)
    } else {
        (height_pt, width_pt)
    };
    // A zero-height rule would divide by zero; it is also a strip by any reading.
    if short <= 0.0 || f64::from(long / short) > images.strip_aspect_ratio {
        return ImageKind::Strip;
    }
    if f64::from(long) < images.ornament_max_side_pt {
        return ImageKind::Ornament;
    }
    ImageKind::Figure
}

/// The DPI an image is effectively reproduced at: its pixels spread over the space it
/// occupies on the page.
///
/// This is what decides whether an image is worth keeping at full size, downsampling, or
/// re-rendering, so it is the number Phase 4's image policy is written against rather than
/// the `/Width` alone.
pub fn effective_dpi(intrinsic_px_width: u32, width_pt: f32) -> f32 {
    /// Points per inch. The one number in PDF that is not a threshold — it is the unit.
    const POINTS_PER_INCH: f32 = 72.0;

    if width_pt <= 0.0 {
        return 0.0;
    }
    intrinsic_px_width as f32 / (width_pt / POINTS_PER_INCH)
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Row 1.9 of the Phase 1 table.
// ---------------------------------------------------------------------------

#[cfg(test)]
fn images_of(name: &str, page: u32) -> Vec<ImageRef> {
    use crate::inspect::PdfOpen;
    use crate::pdfium::PdfiumBackend;

    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/fixtures")
        .join(format!("{name}.pdf"));
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "missing fixture {}: {error}; run `cargo run -p xtask -- fixtures`",
            path.display()
        )
    });

    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend.open(&bytes, None).expect("the fixture opens");
    document.page_images(page).expect("the page extracts")
}

/// Test 1.9.
///
/// The plan's assertion band for `effective_dpi` is [140, 160], which assumes `f03` is A4:
/// `scan_page_01.png` is 1240 × 1754, which is A4 at 150 dpi. The fixture's own Typst source —
/// also given verbatim by the plan — sets `page(width: 148mm, height: 210mm)`, which is A5, so
/// the same asset is reproduced at 1240 px / (419.53 pt / 72) = 212.8 dpi. The two halves of
/// the plan contradict each other; the fixture is the one that exists, and 212 dpi is a
/// perfectly ordinary scan resolution, so the band moves rather than the page size.
///
/// `intrinsic_px` is asserted alongside, so this pins both inputs to the division and its
/// result, rather than a number that could come out right from wrong parts.
#[test]
fn image_only_page_extracts_one_image_with_dpi() {
    /// 1240 px over the 419.53 pt width of an A5 page.
    const EXPECTED_DPI: f32 = 212.81;
    const DPI_TOLERANCE: f32 = 0.5;

    let images = images_of("f03_image_only", 0);
    assert_eq!(images.len(), 1, "{images:?}");

    let image = &images[0];
    assert_eq!(image.kind, ImageKind::FullPageBackground, "{image:?}");
    assert_eq!(image.intrinsic_px, (1240, 1754));
    assert!(
        (image.effective_dpi - EXPECTED_DPI).abs() < DPI_TOLERANCE,
        "effective_dpi {} is not {EXPECTED_DPI}",
        image.effective_dpi
    );

    // The image is placed to fill the page, so its box is the page's.
    assert!(image.bbox.x1 - image.bbox.x0 > 419.0, "{:?}", image.bbox);
    assert!(image.bbox.y1 - image.bbox.y0 > 595.0, "{:?}", image.bbox);

    // Structural flags: a Typst-placed PNG is a plain XObject with no mask.
    assert!(!image.has_smask, "{image:?}");
    assert!(!image.is_inline, "{image:?}");
    assert!(!image.colorspace.is_empty(), "{image:?}");

    assert_eq!(image.page.index, 0);
}

/// The soft-mask flag, in the direction that can actually fail.
///
/// `f03` has no mask, so test 1.9's `!has_smask` would pass just as well if the flag were
/// hard-wired to `false` — which is exactly what the fallback does when the object tree and
/// PDFium disagree about how many images a page has. `h09` is the only fixture with a mask,
/// so this is the assertion that proves the flag is read at all, and with it that the
/// draw-order match between the two backends lines up.
#[test]
fn soft_mask_is_read_from_the_file() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/fixtures/handmade/h09_image_smask.pdf");
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "missing fixture {}: {error}; run `cargo run -p xtask -- handmade-fixtures`",
            path.display()
        )
    });

    let backend = crate::pdfium::PdfiumBackend::bind().expect("PDFium is vendored");
    let document = {
        use crate::inspect::PdfOpen;
        backend.open(&bytes, None).expect("the fixture opens")
    };
    let images = document.page_images(0).expect("the page extracts");

    assert_eq!(images.len(), 1, "{images:?}");
    assert!(images[0].has_smask, "{:?}", images[0]);
    assert!(!images[0].is_inline, "{:?}", images[0]);

    // And the negative case through the same code path, so the two are told apart rather
    // than one of them being the only answer the function can give.
    let plain = std::fs::read(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/fixtures/handmade/h05_invisible_layer.pdf"),
    )
    .expect("h05 is committed");
    let document = {
        use crate::inspect::PdfOpen;
        backend.open(&plain, None).expect("the fixture opens")
    };
    let images = document.page_images(0).expect("the page extracts");
    assert_eq!(images.len(), 1, "{images:?}");
    assert!(!images[0].has_smask, "{:?}", images[0]);
}

/// The inline-image flag, likewise in the direction that can fail.
///
/// An inline image is in no resource dictionary — it is written where it is drawn — so it is
/// reachable only by walking the content stream. `h10` is the only fixture that has one.
#[test]
fn inline_image_is_recognised() {
    use crate::inspect::PdfOpen;

    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/fixtures/handmade/h10_inline_image.pdf");
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "missing fixture {}: {error}; run `cargo run -p xtask -- handmade-fixtures`",
            path.display()
        )
    });

    let backend = crate::pdfium::PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend.open(&bytes, None).expect("the fixture opens");
    let images = document.page_images(0).expect("the page extracts");

    assert_eq!(images.len(), 1, "{images:?}");
    assert!(images[0].is_inline, "{:?}", images[0]);
    assert!(!images[0].has_smask, "{:?}", images[0]);
}

/// The four `ImageKind` arms, at the boundaries the thresholds put them.
///
/// Written because `classify_image` is pure and its arms are otherwise reachable only through
/// a fixture each, and three of the four would need a fixture that does not exist.
#[test]
fn image_kind_covers_every_arm() {
    use oc_core::thresholds::T;

    // A page-filling scan.
    assert_eq!(
        classify_image(1.0, 419.0, 595.0, &T),
        ImageKind::FullPageBackground
    );
    // A rule or a banner: long and thin, whichever way round.
    assert_eq!(classify_image(0.05, 400.0, 10.0, &T), ImageKind::Strip);
    assert_eq!(classify_image(0.05, 10.0, 400.0, &T), ImageKind::Strip);
    // A bullet or a logo.
    assert_eq!(classify_image(0.01, 20.0, 20.0, &T), ImageKind::Ornament);
    // Everything else is a figure, which is the only kind that becomes a `<figure>`.
    assert_eq!(classify_image(0.30, 300.0, 200.0, &T), ImageKind::Figure);

    // The full-page test wins over the strip test on a tall narrow page, which is why it is
    // first: a page-shaped background is not a banner.
    assert_eq!(
        classify_image(0.99, 100.0, 900.0, &T),
        ImageKind::FullPageBackground
    );
}

/// The side of the grid a perceptual hash is computed over: 8 × 8 = 64 bits.
const AHASH_SIDE: u32 = 8;

/// A perceptual hash of a decoded image — the average hash, 64 bits.
///
/// Downscale to 8 × 8 greyscale by box-averaging, then set a bit where a cell is at or above
/// the mean. It is the simplest hash in the family and the right one for the job D13.11 gives
/// it: deciding whether *the same ornament* was drawn on thirty pages. Two placements of one
/// XObject are byte-identical, so an exact hash would do; aHash also survives the case where
/// a producer re-encoded the ornament per page, which some do.
///
/// It is not a similarity measure and must not be used as one. Equality is the only predicate
/// read from it.
pub fn perceptual_hash(image: &DecodedImage) -> u64 {
    if image.width == 0 || image.height == 0 {
        return 0;
    }
    let mut cells = [0f64; (AHASH_SIDE * AHASH_SIDE) as usize];
    for (index, cell) in cells.iter_mut().enumerate() {
        let index = u32::try_from(index).unwrap_or_default();
        let (cx, cy) = (index % AHASH_SIDE, index / AHASH_SIDE);
        // The source rectangle this cell averages, at least one pixel on a side.
        let x0 = cx * image.width / AHASH_SIDE;
        let x1 = ((cx + 1) * image.width / AHASH_SIDE)
            .max(x0 + 1)
            .min(image.width);
        let y0 = cy * image.height / AHASH_SIDE;
        let y1 = ((cy + 1) * image.height / AHASH_SIDE)
            .max(y0 + 1)
            .min(image.height);

        let mut total = 0f64;
        let mut count = 0f64;
        for y in y0..y1 {
            for x in x0..x1 {
                let Some(pixel) = image.pixel(x, y) else {
                    continue;
                };
                // Rec. 601 luma, and the alpha channel folded in as a white matte, so a
                // transparent ornament and a white one hash alike — which is what a reader
                // sees.
                let alpha = f64::from(pixel[3]) / 255.0;
                let luma = 0.299 * f64::from(pixel[0])
                    + 0.587 * f64::from(pixel[1])
                    + 0.114 * f64::from(pixel[2]);
                total += luma * alpha + 255.0 * (1.0 - alpha);
                count += 1.0;
            }
        }
        *cell = if count > 0.0 { total / count } else { 0.0 };
    }

    let mean = cells.iter().sum::<f64>() / cells.len() as f64;
    cells
        .iter()
        .enumerate()
        .filter(|(_, cell)| **cell >= mean)
        .fold(0u64, |hash, (index, _)| hash | (1u64 << index))
}

/// Two images drawn from one XObject hash alike; two different ones do not.
#[test]
fn the_perceptual_hash_separates_two_greys_and_joins_two_copies() {
    let flat = |grey: u8| DecodedImage {
        width: 8,
        height: 8,
        rgba: (0..64).flat_map(|_| [grey, grey, grey, 255]).collect(),
    };
    // A flat image has every cell at the mean, so every bit is set: identical by
    // construction, which is the property the ornament rule needs.
    assert_eq!(perceptual_hash(&flat(96)), perceptual_hash(&flat(96)));

    // A gradient and a flat field differ.
    let gradient = DecodedImage {
        width: 8,
        height: 8,
        rgba: (0..64)
            .flat_map(|index| {
                let grey = u8::try_from(index * 4).unwrap_or(255);
                [grey, grey, grey, 255]
            })
            .collect(),
    };
    assert_ne!(perceptual_hash(&gradient), perceptual_hash(&flat(96)));

    // A zero-sized image hashes to zero rather than panicking on a divide.
    assert_eq!(
        perceptual_hash(&DecodedImage {
            width: 0,
            height: 0,
            rgba: Vec::new(),
        }),
        0
    );
}

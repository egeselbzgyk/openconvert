//! Page and region rasterization for OCR (PHASE 13 detail 3).
//!
//! **Rasterization is `oc-pdf`'s job and OCR is not.** This module turns a region of a page into a
//! grayscale raster at a stated resolution and says exactly which rectangle of the page that raster
//! covers; everything about reading it is `oc_core::ocr`'s.
//!
//! **Grayscale, not RGB.** Tesseract binarizes anyway, and grayscale is a third of the temp file.
//!
//! **The raster is snapped to whole pixels, and the snapped rectangle is what is returned.** A
//! region's edges rarely fall on pixel boundaries, and a caller that mapped Tesseract's boxes back
//! through the rectangle it *asked* for, rather than the one it *got*, would shift every word by up
//! to a pixel — the silent geometry error the plan names as this phase's dominant risk. So the
//! returned [`RenderedRegion::region_pt`] is the rectangle the raster's pixel grid really spans.

use image::GrayImage;
use oc_model::geom::Rect;

use crate::error::PdfError;

/// PostScript points per inch: the normalised page space's unit (D13.3).
const POINTS_PER_INCH: f32 = 72.0;

/// A region of a page, rasterized.
#[derive(Clone, Debug)]
pub struct RenderedRegion {
    pub raster: GrayImage,
    /// The rectangle of the page the raster covers, in normalised page space — the requested
    /// region snapped outward to whole pixels and clipped to the page.
    pub region_pt: Rect,
    pub dpi: u32,
}

/// The pixel rectangle `[x0, x1) × [y0, y1)` of a page raster at `dpi` that covers `region`,
/// snapped outward to whole pixels and clipped to a `width_px × height_px` raster.
pub fn pixel_window(region: Rect, dpi: u32, width_px: u32, height_px: u32) -> (u32, u32, u32, u32) {
    let scale = dpi as f32 / POINTS_PER_INCH;
    let clamp = |value: f32, max: u32| value.max(0.0).min(max as f32) as u32;
    let x0 = clamp((region.x0 * scale).floor(), width_px);
    let y0 = clamp((region.y0 * scale).floor(), height_px);
    let x1 = clamp((region.x1 * scale).ceil(), width_px).max(x0);
    let y1 = clamp((region.y1 * scale).ceil(), height_px).max(y0);
    (x0, y0, x1, y1)
}

/// The page rectangle a pixel window spans.
pub fn window_rect(window: (u32, u32, u32, u32), dpi: u32) -> Rect {
    let factor = POINTS_PER_INCH / dpi as f32;
    Rect {
        x0: window.0 as f32 * factor,
        y0: window.1 as f32 * factor,
        x1: window.2 as f32 * factor,
        y1: window.3 as f32 * factor,
    }
}

/// Cut `region` out of page `index`'s whole-page raster, rendered at `dpi`.
pub fn crop(
    page: &GrayImage,
    index: u32,
    region: Rect,
    dpi: u32,
) -> Result<RenderedRegion, PdfError> {
    let window = pixel_window(region, dpi, page.width(), page.height());
    let (x0, y0, x1, y1) = window;
    if x1 <= x0 || y1 <= y0 {
        return Err(PdfError::Page {
            index,
            message: format!("the region {region:?} covers no pixel of the page"),
        });
    }
    let raster = image::imageops::crop_imm(page, x0, y0, x1 - x0, y1 - y0).to_image();
    Ok(RenderedRegion {
        raster,
        region_pt: window_rect(window, dpi),
        dpi,
    })
}

#[cfg(test)]
fn open(path: &std::path::Path) -> Box<dyn crate::inspect::PdfDoc> {
    use crate::inspect::PdfOpen;
    let backend = crate::pdfium::PdfiumBackend::bind().expect("PDFium is vendored");
    let bytes = std::fs::read(path).unwrap_or_else(|error| {
        panic!(
            "missing fixture {}: {error}; run `cargo run -p xtask -- fixtures`",
            path.display()
        )
    });
    backend.open(&bytes, None).expect("the fixture opens")
}

#[cfg(test)]
fn fixture(dir: &str, name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(dir)
        .join(format!("{name}.pdf"))
}

/// The whole page of `f03` at 300 dpi is the page's size in points times 300/72, in grayscale, with
/// the scan's dark text on light paper — and a region of it is exactly the pixels the snapped
/// rectangle says.
#[test]
fn render_region_is_the_page_at_the_asked_resolution() {
    let document = open(&fixture("target/fixtures", "f03_image_only"));
    let geometry = document.page_geometry(0).expect("geometry");
    let whole = Rect {
        x0: 0.0,
        y0: 0.0,
        x1: geometry.width_pt(),
        y1: geometry.height_pt(),
    };
    let page = document.render_region(0, whole, 300).expect("renders");
    let expected_w = (geometry.width_pt() * 300.0 / 72.0).ceil();
    let expected_h = (geometry.height_pt() * 300.0 / 72.0).ceil();
    assert!(
        (page.raster.width() as f32 - expected_w).abs() <= 1.0,
        "{} vs {expected_w}",
        page.raster.width()
    );
    assert!(
        (page.raster.height() as f32 - expected_h).abs() <= 1.0,
        "{} vs {expected_h}",
        page.raster.height()
    );
    let dark = page
        .raster
        .pixels()
        .filter(|pixel| pixel.0[0] < 128)
        .count();
    let share = dark as f64 / (page.raster.width() * page.raster.height()) as f64;
    assert!(
        share > 0.001 && share < 0.2,
        "a page of text on paper, not a blank or a black page: {share}"
    );

    // A region snaps outward to whole pixels and reports the rectangle it really covers.
    let asked = Rect {
        x0: 10.1,
        y0: 20.2,
        x1: 110.3,
        y1: 60.4,
    };
    let region = document.render_region(0, asked, 300).expect("a region");
    assert!(region.region_pt.x0 <= asked.x0 && region.region_pt.y0 <= asked.y0);
    assert!(region.region_pt.x1 >= asked.x1 && region.region_pt.y1 >= asked.y1);
    let scale = 300.0 / 72.0;
    assert!(
        ((region.region_pt.x1 - region.region_pt.x0) * scale - region.raster.width() as f32).abs()
            < 1e-3
    );
    assert!(
        ((region.region_pt.y1 - region.region_pt.y0) * scale - region.raster.height() as f32).abs()
            < 1e-3
    );
}

/// A rotated page renders in the normalised space every bbox is in: `h02` is `/Rotate 90`, so its
/// raster is as wide as the page's *normalised* width, not its MediaBox width.
#[test]
fn a_rotated_page_renders_in_normalised_space() {
    let document = open(&fixture("corpus/fixtures/handmade", "h02_rotate_90"));
    let geometry = document.page_geometry(0).expect("geometry");
    let whole = Rect {
        x0: 0.0,
        y0: 0.0,
        x1: geometry.width_pt(),
        y1: geometry.height_pt(),
    };
    let page = document.render_region(0, whole, 72).expect("renders");
    assert!(
        (page.raster.width() as f32 - geometry.width_pt()).abs() <= 1.0,
        "{} vs {}",
        page.raster.width(),
        geometry.width_pt()
    );
    assert!((page.raster.height() as f32 - geometry.height_pt()).abs() <= 1.0);

    // And the glyph PDFium reports is where the render draws ink.
    let glyphs = document.page_glyphs(0).expect("glyphs");
    let first = glyphs.glyphs.first().expect("h02 has text");
    let (x0, y0, x1, y1) = pixel_window(first.bbox, 72, page.raster.width(), page.raster.height());
    let ink = (y0..y1)
        .flat_map(|y| (x0..x1).map(move |x| (x, y)))
        .filter(|(x, y)| page.raster.get_pixel(*x, *y).0[0] < 128)
        .count();
    assert!(ink > 0, "no ink under the glyph's box {:?}", first.bbox);
}

/// A pixel window never leaves the raster, and an empty region is refused rather than read.
#[test]
fn pixel_windows_are_clipped_to_the_page() {
    let off = Rect {
        x0: -50.0,
        y0: -50.0,
        x1: 10_000.0,
        y1: 10_000.0,
    };
    assert_eq!(pixel_window(off, 72, 100, 200), (0, 0, 100, 200));
    let page = GrayImage::new(100, 100);
    let nothing = Rect {
        x0: 200.0,
        y0: 200.0,
        x1: 300.0,
        y1: 300.0,
    };
    assert!(crop(&page, 0, nothing, 72).is_err());
}

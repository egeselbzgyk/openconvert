//! Per-page classification (D13.10, IMPLEMENTATION_PLAN Phase 0 detail 3).
//!
//! A page's class decides how it is routed: `broken_text` and `image_only` go to OCR when
//! it is available, `ocr_sandwich` reuses the layer that is already there, `mixed` OCRs
//! only the image regions no text covers. It is a first-class, user-overridable verdict, so
//! it is computed by one pure function over counters that can be tested without a PDF.

use oc_core::thresholds::Thresholds;
use serde::Serialize;

/// What a page is, and therefore how it is routed (D13.10).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PageClass {
    /// Born-digital text. The extraction path.
    Text,
    /// Text objects are present but do not decode to usable characters — a broken CMap or a
    /// subset font with no `ToUnicode`. Routed to OCR when it is available.
    BrokenText,
    /// A scan with an invisible OCR text layer already drawn over it. The layer is reused
    /// with `provenance = ocr-layer`, never discarded.
    OcrSandwich,
    /// A scan with no text layer at all.
    ImageOnly,
    /// Text and images together; only the image regions no text covers are OCR'd.
    Mixed,
    /// Nothing worth extracting.
    Blank,
}

/// Per-page character counters. Generated spaces are excluded upstream (D3), so `visible`
/// counts characters the document actually draws.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PageCharStats {
    pub visible: u32,
    /// Render mode 3, or a fill colour with zero alpha: drawn but not shown.
    pub invisible: u32,
    /// U+FFFD, the replacement character.
    pub replacement: u32,
    /// Private-use code points, which a subset font with no `ToUnicode` map produces.
    pub pua: u32,
    /// Whether any font on the page is a `GlyphLessFont`, the marker Tesseract's PDF output
    /// leaves behind.
    pub glyphless_font: bool,
}

/// Per-page image counters.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PageImageStats {
    pub count: u32,
    /// Share of the page area covered by images, in `[0, 1]`.
    pub covered_area_ratio: f32,
    pub max_pixels: u64,
}

/// Classify one page from its counters, with the confidence of the verdict.
///
/// The arms are tried in the order D13.10 and Phase 0 detail 3 fix, and the order carries
/// meaning: an OCR sandwich must be recognised before `image_only` would claim the same
/// page, or an existing text layer would be thrown away and re-OCR'd.
///
/// `dictionary_hit_rate` is the second arm of the broken-text test (R10 §4.4): characters
/// that decode cleanly but form no words. Phase 0 always passes `None` — Phase 2 computes
/// it, once word assembly exists. It is wired now so that the arm exists and is tested,
/// rather than being bolted on later.
///
/// Note this takes no `PageGeometry`: detail 3 defines classification as a pure function of
/// the counters, and an unused parameter would only invite one.
pub fn classify_page(
    chars: &PageCharStats,
    images: &PageImageStats,
    dictionary_hit_rate: Option<f32>,
    thresholds: &Thresholds,
) -> (PageClass, f32) {
    let pageclass = &thresholds.pageclass;
    let confidence = &pageclass.confidence;
    let image_area = f64::from(images.covered_area_ratio);
    let mostly_image = image_area >= pageclass.image_area_ratio_min;
    let visible = i64::from(chars.visible);

    if chars.invisible > 0 && chars.glyphless_font && mostly_image {
        return (PageClass::OcrSandwich, confidence.ocr_sandwich as f32);
    }
    if visible <= pageclass.image_only_max_visible_chars && mostly_image {
        return (PageClass::ImageOnly, confidence.image_only as f32);
    }
    if chars.visible == 0 && !mostly_image {
        return (PageClass::Blank, confidence.blank as f32);
    }
    if is_broken_text(chars, dictionary_hit_rate, thresholds) {
        return (PageClass::BrokenText, confidence.broken_text as f32);
    }
    if visible >= pageclass.text_min_visible_chars {
        if mostly_image {
            return (PageClass::Mixed, confidence.mixed as f32);
        }
        return (PageClass::Text, confidence.text as f32);
    }
    (PageClass::Blank, confidence.fallback_blank as f32)
}

/// Two independent signals of a broken CMap: characters that decode to nothing usable, and
/// characters that decode but spell nothing.
fn is_broken_text(
    chars: &PageCharStats,
    dictionary_hit_rate: Option<f32>,
    thresholds: &Thresholds,
) -> bool {
    let pageclass = &thresholds.pageclass;
    let undecodable = f64::from(chars.replacement + chars.pua);
    // A page with no visible characters cannot be judged by a share of them; the divisor
    // guards the ratio, it does not stand in for missing evidence.
    let share = undecodable / f64::from(chars.visible.max(1));
    if share >= pageclass.broken_text_replacement_share {
        return true;
    }
    dictionary_hit_rate.is_some_and(|rate| f64::from(rate) < pageclass.broken_text_dict_hit_min)
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Rows 0.9–0.12 of the Phase 0 table,
// plus one test covering the three branches those four leave untouched.
// ---------------------------------------------------------------------------

#[cfg(test)]
fn no_images() -> PageImageStats {
    PageImageStats {
        count: 0,
        covered_area_ratio: 0.0,
        max_pixels: 0,
    }
}

#[cfg(test)]
fn full_page_image() -> PageImageStats {
    PageImageStats {
        count: 1,
        covered_area_ratio: 1.0,
        max_pixels: 1_240 * 1_754,
    }
}

#[cfg(test)]
fn chars(visible: u32) -> PageCharStats {
    PageCharStats {
        visible,
        invisible: 0,
        replacement: 0,
        pua: 0,
        glyphless_font: false,
    }
}

#[test]
fn classify_text_page() {
    use crate::classify::{classify_page, PageClass};
    use oc_core::thresholds::T;

    // The first page of f01_prose_single_column: born-digital text, no images.
    let (class, confidence) = classify_page(&chars(1_180), &no_images(), None, &T);
    assert_eq!(class, PageClass::Text);
    assert_eq!(confidence, 1.0);
}

#[test]
fn classify_image_only_page() {
    use crate::classify::{classify_page, PageClass};
    use oc_core::thresholds::T;

    // A scanned page: pixels of text, but no PDF text objects at all.
    let (class, confidence) = classify_page(&chars(0), &full_page_image(), None, &T);
    assert_eq!(class, PageClass::ImageOnly);
    assert_eq!(confidence, 0.9);
}

#[test]
fn classify_ocr_sandwich_page() {
    use crate::classify::{classify_page, PageCharStats, PageClass};
    use oc_core::thresholds::T;

    // A scan with an invisible OCR text layer drawn over it in a glyph-less font.
    let stats = PageCharStats {
        visible: 0,
        invisible: 900,
        replacement: 0,
        pua: 0,
        glyphless_font: true,
    };
    let (class, confidence) = classify_page(&stats, &full_page_image(), None, &T);
    assert_eq!(class, PageClass::OcrSandwich);
    assert_eq!(confidence, 0.95);

    // The sandwich is recognised before `image_only` would fire on the same page, which is
    // the whole point of the ordering: the existing layer must not be thrown away.
    assert_ne!(class, PageClass::ImageOnly);
}

#[test]
fn classify_broken_text_page() {
    use crate::classify::{classify_page, PageCharStats, PageClass};
    use oc_core::thresholds::T;

    // A third of the glyphs decode to U+FFFD: the CMap is broken, so this text is not text.
    let stats = PageCharStats {
        visible: 900,
        replacement: 300,
        ..chars(900)
    };
    let (class, confidence) = classify_page(&stats, &no_images(), None, &T);
    assert_eq!(class, PageClass::BrokenText);
    assert_eq!(confidence, 0.8);

    // Private-use characters count towards the same share: a subsetted font with no
    // ToUnicode map produces PUA code points rather than U+FFFD.
    let pua = PageCharStats {
        visible: 900,
        pua: 300,
        ..chars(900)
    };
    assert_eq!(
        classify_page(&pua, &no_images(), None, &T).0,
        PageClass::BrokenText
    );

    // Below the share, the same page is ordinary text.
    let few = PageCharStats {
        visible: 900,
        replacement: 10,
        ..chars(900)
    };
    assert_eq!(
        classify_page(&few, &no_images(), None, &T).0,
        PageClass::Text
    );
}

#[test]
fn classify_mixed_blank_and_dictionary_arm() {
    use crate::classify::{classify_page, PageClass};
    use oc_core::thresholds::T;

    // Text and a large image on the same page: OCR only the regions the text does not cover.
    let (class, confidence) = classify_page(&chars(1_180), &full_page_image(), None, &T);
    assert_eq!(class, PageClass::Mixed);
    assert_eq!(confidence, 0.7);

    // Nothing on the page at all.
    let (class, confidence) = classify_page(&chars(0), &no_images(), None, &T);
    assert_eq!(class, PageClass::Blank);
    assert_eq!(confidence, 0.9);

    // Some text, but less than a text page carries and not enough image to be anything
    // else: the fallback, and it says so with a low confidence.
    let (class, confidence) = classify_page(&chars(20), &no_images(), None, &T);
    assert_eq!(class, PageClass::Blank);
    assert_eq!(confidence, 0.5);

    // The dictionary-hit-rate arm: characters that decode cleanly but form no words are
    // the other face of a broken CMap. Phase 0 always passes `None` here; Phase 2 supplies
    // the rate, so the wiring is exercised now rather than discovered later.
    assert_eq!(
        classify_page(&chars(900), &no_images(), Some(0.05), &T).0,
        PageClass::BrokenText
    );
    assert_eq!(
        classify_page(&chars(900), &no_images(), Some(0.85), &T).0,
        PageClass::Text
    );
}

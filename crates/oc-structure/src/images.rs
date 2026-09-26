//! Which images survive into the book (D13.11, IMPLEMENTATION_PLAN Phase 4 detail 11).
//!
//! One decision here, and it is the only one that *removes* anything in the whole stage:
//! **a small identical image that repeats on most of a book's pages is an ornament**, not a
//! figure — a rule under every chapter title, a printer's flower at every section break — and
//! a reflowable EPUB carrying thirty copies of it is worse than one carrying none.
//!
//! It is not a conservation-law removal, and the distinction matters. `C` is a multiset of
//! *characters*; images are outside it, so dropping one cites no `Reason` and spends no
//! budget. What it does instead is warn, because a removal nobody can see in the ledger has
//! to be visible somewhere.

use oc_core::thresholds::Thresholds;
use oc_model::doc::{Severity, Warning};
use oc_model::extract::{ImageId, ImageRef};
use serde::Serialize;

/// A repeated ornament was dropped (D13.11).
pub const W_ORNAMENT_DROPPED: &str = "W_ORNAMENT_DROPPED";

/// What the ornament rule decided.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ImagePolicy {
    /// The images that survive, in document order.
    pub kept: Vec<ImageId>,
    pub dropped: Vec<ImageId>,
    pub warnings: Vec<Warning>,
}

/// Decide which images are ornaments.
///
/// `hashes` is index-aligned with `images`: the caller decodes and hashes, because decoding
/// is `oc-pdf`'s and this crate must not depend on a backend.
pub fn drop_ornaments(
    images: &[ImageRef],
    hashes: &[Option<u64>],
    page_count: u32,
    t: &Thresholds,
) -> ImagePolicy {
    let min_pages = u32::try_from(t.images.ornament_min_pages).unwrap_or(3);

    // Per hash, the distinct pages it appears on. Pages, not placements: an ornament drawn
    // twice on one page is still on one page, and the rule is about how much of the *book*
    // carries it.
    let mut pages_of: std::collections::BTreeMap<u64, std::collections::BTreeSet<u32>> =
        std::collections::BTreeMap::new();
    for (index, image) in images.iter().enumerate() {
        if !needs_hash(image, t) {
            continue;
        }
        let Some(Some(hash)) = hashes.get(index) else {
            continue;
        };
        pages_of.entry(*hash).or_default().insert(image.page.index);
    }

    let ornaments: std::collections::BTreeSet<u64> = pages_of
        .iter()
        .filter(|(_, pages)| {
            let count = u32::try_from(pages.len()).unwrap_or(u32::MAX);
            count >= min_pages
                && page_count > 0
                && f64::from(count) / f64::from(page_count) >= t.images.ornament_page_share
        })
        .map(|(hash, _)| *hash)
        .collect();

    let mut policy = ImagePolicy {
        kept: Vec::new(),
        dropped: Vec::new(),
        warnings: Vec::new(),
    };
    for (index, image) in images.iter().enumerate() {
        let ornament = needs_hash(image, t)
            && hashes
                .get(index)
                .copied()
                .flatten()
                .is_some_and(|hash| ornaments.contains(&hash));
        if ornament {
            policy.dropped.push(image.id);
        } else {
            policy.kept.push(image.id);
        }
    }

    for hash in &ornaments {
        let pages = pages_of
            .get(hash)
            .map_or(0, std::collections::BTreeSet::len);
        policy.warnings.push(
            Warning::new(W_ORNAMENT_DROPPED, Severity::Info)
                .with_arg("pages", pages.to_string())
                .with_arg("page_count", page_count.to_string()),
        );
    }
    policy
}

/// A page scan was dropped because its page's text layer carries the page.
pub const W_PAGE_SCAN_DROPPED: &str = "W_PAGE_SCAN_DROPPED";

/// The pages whose full-page background is the scan of the text over it.
///
/// A page qualifies when it carries at least `images.background_min_page_chars` of text and a
/// full-page background, and either its text was read off a scan — the file's own OCR layer or
/// this pipeline's OCR — or the book is a scan: at least `images.scan_book_min_share` of its
/// text pages carry such a background. A novel that sets a few chapter openings over an
/// illustration is not a scan, and keeps those pictures (2026-09-26: dropping them lost the
/// pictures and let the chapter numbers read as one list).
pub fn scan_pages(
    images: &[ImageRef],
    blocks: &[crate::view::BlockView],
    runs: &[oc_model::text::Run],
    t: &Thresholds,
) -> std::collections::BTreeSet<u32> {
    use std::collections::{BTreeMap, BTreeSet};
    let min = usize::try_from(t.images.background_min_page_chars.max(0)).unwrap_or(usize::MAX);
    let count = |text: &str| text.chars().filter(|ch| !ch.is_whitespace()).count();
    let mut text_chars: BTreeMap<u32, usize> = BTreeMap::new();
    for block in blocks {
        *text_chars.entry(block.page).or_default() += count(&block.text);
    }
    let mut ocr_chars: BTreeMap<u32, usize> = BTreeMap::new();
    for run in runs
        .iter()
        .filter(|run| run.provenance != oc_model::text::TextProvenance::Pdf)
    {
        *ocr_chars.entry(run.page.index).or_default() += count(&run.text);
    }
    let text_pages: BTreeSet<u32> = text_chars
        .iter()
        .filter(|(_, chars)| **chars >= min)
        .map(|(page, _)| *page)
        .collect();
    let backed: BTreeSet<u32> = images
        .iter()
        .filter(|image| image.kind == oc_model::extract::ImageKind::FullPageBackground)
        .map(|image| image.page.index)
        .filter(|page| text_pages.contains(page))
        .collect();
    let scanned_book = !text_pages.is_empty()
        && backed.len() as f64 >= t.images.scan_book_min_share * text_pages.len() as f64;
    backed
        .into_iter()
        .filter(|page| scanned_book || ocr_chars.get(page).copied().unwrap_or(0) >= min)
        .collect()
}

/// Drop the full-page backgrounds of the [`scan_pages`]: the page's text is there as text.
///
/// A scanned book with a recognised text layer draws each page as a picture and lays the
/// invisible text over it. The text is the page's content; the picture of the same words is
/// not a figure, and emitting it put every page of such a book into the EPUB twice — once as
/// text and once as a picture of it — and spent minutes encoding them (2026-09-26: 520 page
/// scans in a 260-page book).
pub fn drop_text_backgrounds(
    policy: &mut ImagePolicy,
    images: &[ImageRef],
    scan_pages: &std::collections::BTreeSet<u32>,
) {
    let scans: Vec<ImageId> = images
        .iter()
        .filter(|image| image.kind == oc_model::extract::ImageKind::FullPageBackground)
        .filter(|image| scan_pages.contains(&image.page.index))
        .map(|image| image.id)
        .filter(|id| policy.kept.contains(id))
        .collect();
    if scans.is_empty() {
        return;
    }
    policy.kept.retain(|id| !scans.contains(id));
    policy.warnings.push(
        Warning::new(W_PAGE_SCAN_DROPPED, Severity::Info)
            .with_arg("images", scans.len().to_string()),
    );
    policy.dropped.extend(scans);
    policy.dropped.sort();
}

/// Whether an image is small enough to be an ornament at all.
///
/// A full-page background repeated on every page of a scan is not an ornament — it *is* the
/// page — so the size test comes before the repetition test rather than after it.
/// Whether this image's perceptual hash is ever read — the **one** place that decides it.
///
/// Only an image small enough to be an ornament is compared with the others, so only such an
/// image needs decoding. The decoder asks this function rather than keeping its own copy of
/// the rule: decoding every image in the book cost ~70 s of a 101 s run on a 131-page scan
/// whose full-page images were decoded, hashed and never looked at (PHASE 7.5, the timeout
/// class). Two copies of this predicate would be the defect this phase keeps finding — a
/// second rule that agrees with the first until the day one of them changes.
pub fn needs_hash(image: &ImageRef, t: &Thresholds) -> bool {
    is_small(image, t.images.ornament_max_side_pt as f32)
}

fn is_small(image: &ImageRef, max_side_pt: f32) -> bool {
    let width = image.bbox.x1 - image.bbox.x0;
    let height = image.bbox.y1 - image.bbox.y0;
    width <= max_side_pt && height <= max_side_pt
}

#[cfg(test)]
mod tests {
    use super::*;
    use oc_model::extract::{ImageKind, PageRef};
    use oc_model::geom::Rect;

    fn image(id: u32, page: u32, side: f32) -> ImageRef {
        ImageRef {
            id: ImageId(id),
            page: PageRef::new(page),
            bbox: Rect {
                x0: 10.0,
                y0: 10.0,
                x1: 10.0 + side,
                y1: 10.0 + side,
            },
            intrinsic_px: (8, 8),
            has_smask: false,
            is_inline: false,
            colorspace: "DeviceGray".to_owned(),
            effective_dpi: 72.0,
            kind: ImageKind::Figure,
        }
    }

    /// A big image that repeats is not an ornament: it is a background, and it *is* the page.
    #[test]
    fn a_large_repeated_image_is_never_an_ornament() {
        let t = &oc_core::thresholds::T;
        let images: Vec<ImageRef> = (0..5).map(|page| image(page, page, 400.0)).collect();
        let policy = drop_ornaments(&images, &[Some(7); 5], 5, t);
        assert!(policy.dropped.is_empty());
        assert_eq!(policy.kept.len(), 5);
    }

    /// Two pages is not "most of a book". Without the floor, one image in a two-page document
    /// is on half the pages and every share threshold below a half fires.
    #[test]
    fn a_short_document_has_no_ornaments() {
        let t = &oc_core::thresholds::T;
        let images = vec![image(0, 0, 20.0), image(1, 1, 20.0)];
        let policy = drop_ornaments(&images, &[Some(7), Some(7)], 2, t);
        assert!(policy.dropped.is_empty(), "{policy:?}");
    }

    /// The scan behind a page's text layer goes; a full-page picture on a page with only a
    /// caption stays, and so does the scan of a page with no text at all.
    #[test]
    fn a_page_scan_under_its_text_is_dropped() {
        let t = &oc_core::thresholds::T;
        let scan = |id: u32, page: u32| {
            let mut image = image(id, page, 500.0);
            image.kind = ImageKind::FullPageBackground;
            image
        };
        let images = vec![scan(0, 0), scan(1, 1), scan(2, 2), image(3, 0, 200.0)];
        let pages: std::collections::BTreeSet<u32> = [0].into_iter().collect();
        let mut policy = drop_ornaments(&images, &[None; 4], 3, t);
        drop_text_backgrounds(&mut policy, &images, &pages);
        assert_eq!(policy.dropped, vec![ImageId(0)]);
        assert_eq!(policy.kept, vec![ImageId(1), ImageId(2), ImageId(3)]);
        assert!(policy
            .warnings
            .iter()
            .any(|warning| warning.code == W_PAGE_SCAN_DROPPED));
    }

    /// Distinct images are counted separately even when they sit in the same place.
    #[test]
    fn two_different_images_do_not_add_up_to_one_ornament() {
        let t = &oc_core::thresholds::T;
        let images: Vec<ImageRef> = (0..6).map(|page| image(page, page, 20.0)).collect();
        let hashes = [1u64, 2, 3, 4, 5, 6].map(Some);
        let policy = drop_ornaments(&images, &hashes, 6, t);
        assert!(policy.dropped.is_empty());
    }
}

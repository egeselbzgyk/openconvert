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
    hashes: &[u64],
    page_count: u32,
    t: &Thresholds,
) -> ImagePolicy {
    let max_side = t.images.ornament_max_side_pt as f32;
    let min_pages = u32::try_from(t.images.ornament_min_pages).unwrap_or(3);

    // Per hash, the distinct pages it appears on. Pages, not placements: an ornament drawn
    // twice on one page is still on one page, and the rule is about how much of the *book*
    // carries it.
    let mut pages_of: std::collections::BTreeMap<u64, std::collections::BTreeSet<u32>> =
        std::collections::BTreeMap::new();
    for (index, image) in images.iter().enumerate() {
        if !is_small(image, max_side) {
            continue;
        }
        let Some(hash) = hashes.get(index) else {
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
        let ornament = is_small(image, max_side)
            && hashes
                .get(index)
                .is_some_and(|hash| ornaments.contains(hash));
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

/// Whether an image is small enough to be an ornament at all.
///
/// A full-page background repeated on every page of a scan is not an ornament — it *is* the
/// page — so the size test comes before the repetition test rather than after it.
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
        let policy = drop_ornaments(&images, &[7; 5], 5, t);
        assert!(policy.dropped.is_empty());
        assert_eq!(policy.kept.len(), 5);
    }

    /// Two pages is not "most of a book". Without the floor, one image in a two-page document
    /// is on half the pages and every share threshold below a half fires.
    #[test]
    fn a_short_document_has_no_ornaments() {
        let t = &oc_core::thresholds::T;
        let images = vec![image(0, 0, 20.0), image(1, 1, 20.0)];
        let policy = drop_ornaments(&images, &[7, 7], 2, t);
        assert!(policy.dropped.is_empty(), "{policy:?}");
    }

    /// Distinct images are counted separately even when they sit in the same place.
    #[test]
    fn two_different_images_do_not_add_up_to_one_ornament() {
        let t = &oc_core::thresholds::T;
        let images: Vec<ImageRef> = (0..6).map(|page| image(page, page, 20.0)).collect();
        let hashes = [1u64, 2, 3, 4, 5, 6];
        let policy = drop_ornaments(&images, &hashes, 6, t);
        assert!(policy.dropped.is_empty());
    }
}

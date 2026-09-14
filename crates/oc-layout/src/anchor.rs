//! Image anchoring, and drop caps (PIPELINE §6 steps 5 and 6, R10 §6.15, R2 §B.6).
//!
//! **Anchoring.** An EPUB is reflowable: the reading system decides where a figure lands on a
//! screen it has never seen, at a font size the author never chose. So precision beyond
//! "correct position in the flow" is unobservable (R10 §6.15), and the whole job is to say
//! which block an image comes before. Each image is anchored at the nearest block boundary in
//! reading order, figure and caption stay adjacent for Phase 4 to pair up, and **nothing is
//! dropped** — an image whose page has no text at all still gets an anchor.
//!
//! **Drop caps.** A single large glyph at the start of a chapter, with the first lines set
//! around it. It has to be found because failing to produces a stray one-character paragraph,
//! which is one of the most visible defects an EPUB can have; and it has to be found *once*,
//! because emitting it twice is exactly the bug the conservation law catches as an unexplained
//! `Added`. Here it is only detected and flagged — the `dropcap` CSS class is Phase 5's, and
//! the character stays where it is either way.

use oc_core::thresholds::Thresholds;
use oc_model::extract::{ImageId, ImageRef};
use oc_model::geom::Rect;
use oc_model::ids::BlockId;
use oc_model::layout::Block;

use crate::blocks::LayoutPage;
use crate::columns::median_height;

/// Where one image sits in the flow.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Anchor {
    pub image: ImageId,
    /// The block this image is placed *before*. `None` means the end of its page's flow —
    /// an image below everything, on a page whose text has run out, or on a page with no
    /// text at all.
    pub before: Option<BlockId>,
    /// The reading index the image takes, which is the anchor block's or one past the last.
    pub reading_index: u32,
}

/// Anchor a page's images into its block flow.
///
/// The rule is one line long: an image goes before the first block, in reading order, that
/// starts below it. Everything else — a figure at the foot of a page, a page of nothing but
/// figures — falls out of that rule rather than needing one of its own.
pub fn anchor_images(images: &[ImageRef], blocks: &[Block]) -> Vec<Anchor> {
    let mut anchors: Vec<Anchor> = images
        .iter()
        .map(|image| {
            let after = blocks
                .iter()
                .filter(|block| block.bbox.y0 >= image.bbox.y1)
                .min_by_key(|block| block.reading_index);
            match after {
                Some(block) => Anchor {
                    image: image.id,
                    before: Some(block.id),
                    reading_index: block.reading_index,
                },
                None => Anchor {
                    image: image.id,
                    before: None,
                    reading_index: blocks
                        .iter()
                        .map(|block| block.reading_index.saturating_add(1))
                        .max()
                        .unwrap_or(0),
                },
            }
        })
        .collect();
    // Two images anchored at the same block keep the order the page drew them in, which for
    // a figure and its plate is the order a reader meets them.
    anchors.sort_by_key(|anchor| anchor.reading_index);
    anchors
}

/// A drop cap: one oversized glyph opening a paragraph.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct DropCap {
    /// The block whose first line is the drop cap's own.
    pub block: BlockId,
    /// The character, which stays in the text exactly where it was.
    pub text: String,
    pub bbox: Rect,
}

/// Find the page's drop caps.
///
/// Three conditions, all from R2 §B.6, and all three are needed. The line holds a single
/// character; it is at least `dropcap.min_height_lines` times the body line height; and there
/// is text to its right or below it, because a single large character with nothing around it
/// is a display initial on a title page, not a drop cap.
pub fn drop_caps(page: &LayoutPage, blocks: &[Block], t: &Thresholds) -> Vec<DropCap> {
    let body = median_height(
        &page
            .lines
            .iter()
            .map(|line| line.line.bbox)
            .collect::<Vec<_>>(),
    );
    if body <= 0.0 {
        return Vec::new();
    }
    let minimum = body * t.dropcap.min_height_lines as f32;

    let mut found = Vec::new();
    for block in blocks {
        for line in &block.lines {
            let text = page
                .lines
                .iter()
                .find(|candidate| candidate.line.runs == line.runs)
                .map(|candidate| candidate.text.trim().to_owned())
                .unwrap_or_default();
            if text.chars().count() != 1 {
                continue;
            }
            let height = line.bbox.y1 - line.bbox.y0;
            if height < minimum {
                continue;
            }
            let has_neighbour = page.lines.iter().any(|other| {
                other.line.runs != line.runs
                    && other.line.bbox.x0 >= line.bbox.x1
                    && other.line.bbox.y1 > line.bbox.y0
                    && other.line.bbox.y0 < line.bbox.y1
            });
            if !has_neighbour {
                continue;
            }
            found.push(DropCap {
                block: block.id,
                text,
                bbox: line.bbox,
            });
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::TEST_SIZE_PT;
    use oc_core::thresholds::T;
    use oc_model::extract::{ImageKind, PageRef};
    use oc_model::layout::BlockKindHint;
    use oc_model::text::{Line, RunId};

    use crate::blocks::{LayoutLine, Segment};

    fn block_at(y0: f32, y1: f32, reading_index: u32) -> Block {
        let bbox = Rect {
            x0: 50.0,
            y0,
            x1: 250.0,
            y1,
        };
        Block {
            id: BlockId::derive(0, bbox, ""),
            page: PageRef::new(0),
            bbox,
            lines: Vec::new(),
            column: 0,
            kind_hint: BlockKindHint::Text,
            furniture: None,
            reading_index,
        }
    }

    fn image_at(id: u32, y0: f32, y1: f32) -> ImageRef {
        ImageRef {
            id: ImageId(id),
            page: PageRef::new(0),
            bbox: Rect {
                x0: 50.0,
                y0,
                x1: 250.0,
                y1,
            },
            intrinsic_px: (100, 100),
            has_smask: false,
            is_inline: false,
            colorspace: "DeviceRGB".to_owned(),
            effective_dpi: 72.0,
            kind: ImageKind::Figure,
        }
    }

    #[test]
    fn an_image_is_anchored_before_the_block_below_it() {
        let blocks = vec![block_at(50.0, 100.0, 0), block_at(200.0, 250.0, 1)];
        let anchors = anchor_images(&[image_at(1, 120.0, 180.0)], &blocks);

        assert_eq!(anchors.len(), 1);
        assert_eq!(anchors[0].before, Some(blocks[1].id));
        assert_eq!(anchors[0].reading_index, 1);
    }

    /// An image at the foot of a page has nothing below it, and goes at the end of the flow
    /// rather than nowhere: "drop nothing yet" is the rule for this whole stage.
    #[test]
    fn an_image_below_everything_goes_last() {
        let blocks = vec![block_at(50.0, 100.0, 0)];
        let anchors = anchor_images(&[image_at(1, 200.0, 260.0)], &blocks);

        assert_eq!(anchors[0].before, None);
        assert_eq!(anchors[0].reading_index, 1);
    }

    /// A page of nothing but a figure still anchors it.
    #[test]
    fn an_image_on_a_page_with_no_text_is_still_anchored() {
        let anchors = anchor_images(&[image_at(1, 50.0, 700.0)], &[]);
        assert_eq!(anchors.len(), 1);
        assert_eq!(anchors[0].reading_index, 0);
    }

    /// Two images between the same pair of blocks keep the page's own order.
    #[test]
    fn two_images_at_one_anchor_keep_their_order() {
        let blocks = vec![block_at(300.0, 350.0, 0)];
        let anchors = anchor_images(
            &[image_at(1, 50.0, 120.0), image_at(2, 150.0, 220.0)],
            &blocks,
        );
        assert_eq!(anchors[0].image, ImageId(1));
        assert_eq!(anchors[1].image, ImageId(2));
    }

    fn line(x0: f32, y0: f32, x1: f32, y1: f32, text: &str, run: u32) -> LayoutLine {
        let bbox = Rect { x0, y0, x1, y1 };
        LayoutLine {
            line: Line {
                runs: vec![RunId(run)],
                bbox,
                baseline_y: y1,
                ends_with_hyphen: false,
                indent_pt: 0.0,
                right_gap_pt: 0.0,
            },
            text: text.to_owned(),
            segments: vec![Segment {
                run: RunId(run),
                bbox,
                text: text.to_owned(),
                size_pt: TEST_SIZE_PT,
            }],
        }
    }

    fn page_of(lines: Vec<LayoutLine>) -> LayoutPage {
        LayoutPage {
            page: PageRef::new(0),
            width_pt: 400.0,
            height_pt: 600.0,
            lines,
        }
    }

    /// The shape R2 §B.6 describes: one big letter, ordinary lines set beside it.
    #[test]
    fn a_large_single_glyph_beside_text_is_a_drop_cap() {
        let page = page_of(vec![
            line(50.0, 50.0, 78.0, 86.0, "I", 0),
            line(80.0, 50.0, 250.0, 60.0, "t was a dark and stormy", 1),
            line(80.0, 62.0, 250.0, 72.0, "night, and the rain fell", 2),
            line(50.0, 74.0, 250.0, 84.0, "in torrents.", 3),
        ]);
        let (blocks, _) = crate::blocks::segment_blocks(
            &page,
            &crate::columns::ColumnLayout::single(0.0, 400.0),
            &T,
        );
        let found = drop_caps(&page, &blocks, &T);

        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(found[0].text, "I");
    }

    /// A single large character with nothing beside it is a display initial on a title page,
    /// and calling it a drop cap would wrap a title in a paragraph.
    #[test]
    fn a_lone_large_glyph_is_not_a_drop_cap() {
        let page = page_of(vec![
            line(50.0, 50.0, 78.0, 86.0, "I", 0),
            line(50.0, 200.0, 250.0, 210.0, "well below it", 1),
        ]);
        let (blocks, _) = crate::blocks::segment_blocks(
            &page,
            &crate::columns::ColumnLayout::single(0.0, 400.0),
            &T,
        );
        assert!(drop_caps(&page, &blocks, &T).is_empty());
    }

    /// And an ordinary one-letter line — a list marker, a stray initial — is not one either.
    #[test]
    fn a_body_sized_single_glyph_is_not_a_drop_cap() {
        let page = page_of(vec![
            line(50.0, 50.0, 58.0, 60.0, "A", 0),
            line(60.0, 50.0, 250.0, 60.0, "list marker perhaps", 1),
            line(50.0, 62.0, 250.0, 72.0, "and a line under it", 2),
        ]);
        let (blocks, _) = crate::blocks::segment_blocks(
            &page,
            &crate::columns::ColumnLayout::single(0.0, 400.0),
            &T,
        );
        assert!(drop_caps(&page, &blocks, &T).is_empty());
    }
}

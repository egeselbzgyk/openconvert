//! Captions, and which figure each one belongs to (PIPELINE §8.4).
//!
//! The governing fact is that this is ambiguous **even for humans**: DocLayNet's `Caption`
//! class has inter-annotator agreement of 84–89, and GROBID's figure-title F1 of 69.03 % is
//! the realistic upper bound for this class of cue (R10 §6.10). So the rule is not "find the
//! nearest figure"; it is "associate only when the nearest figure is *clearly* nearer than
//! the next one, and abstain otherwise". Abstaining costs a caption that reads as a
//! paragraph. Guessing costs a caption attached to the wrong picture, which is worse than
//! having none, because a reader believes it.

use oc_core::thresholds::Thresholds;
use oc_model::confidence::{Confidence, Signal};
use oc_model::doc::{Figure, Severity, Span, Warning};
use oc_model::extract::ImageRef;
use oc_model::geom::Rect;
use oc_model::ids::{BlockId, FigureId};
use oc_model::lang::LangTag;
use oc_text::fold::fold_key;
use serde::Serialize;

use crate::view::BlockView;

/// A caption could not be assigned to one figure rather than another (PIPELINE §8.4).
pub const W_CAPTION_AMBIGUOUS: &str = "W_CAPTION_AMBIGUOUS";

/// The localized caption prefixes, folded (PIPELINE §8.4's table).
///
/// Folded rather than matched case-sensitively, and folded per language rather than
/// invariantly: `ŞEKIL` lowercases to `şekil` only under Turkish rules (R10 §6.3).
const CAPTION_WORDS: [&str; 17] = [
    // EN
    "fig",
    "figure",
    "table",
    "plate",
    "listing",
    "chart",
    "scheme", // DE
    "abb",
    "abbildung",
    "tabelle",
    "tab",
    "tafel", // TR
    "sekil",
    "şekil",
    "tablo",
    "resim",
    "cizelge",
];

/// A block that reads as a caption, before it is known which figure it belongs to.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CaptionCandidate {
    pub block: BlockId,
    pub page: u32,
    pub text: String,
    pub bbox: Rect,
    /// Whether the localized prefix matched. A caption with a prefix is a caption; one
    /// without is a small block next to a picture, which is weaker evidence and is recorded
    /// as such.
    pub has_prefix: bool,
}

/// Associate captions with figures (PIPELINE §8.4).
///
/// Returns one `Figure` per image, in image order, plus the warnings for the captions that
/// could not be placed. Every image becomes a figure whether or not it found a caption: an
/// uncaptioned picture is still a picture, and dropping it would be a conservation failure
/// in the one part of the document the conservation law does not cover.
pub fn associate_captions(
    images: &[ImageRef],
    blocks: &[BlockView],
    skip: &std::collections::BTreeSet<oc_model::ids::BlockId>,
    body_size_pt: f32,
    lang: &LangTag,
    t: &Thresholds,
) -> (
    Vec<Figure>,
    Vec<CaptionCandidate>,
    Vec<Warning>,
    std::collections::BTreeMap<oc_model::ids::BlockId, FigureId>,
) {
    // A block a higher-precedence structure owns is not a caption candidate: a caption bound
    // to a figure *and* read into a table's cells was emitted twice (PHASE 7.5,
    // `structure/appeared/contested-claim`).
    let captions: Vec<CaptionCandidate> = caption_candidates(blocks, body_size_pt, lang, t)
        .into_iter()
        .filter(|caption| !skip.contains(&caption.block))
        .collect();
    let gap = body_size_pt * t.caption.max_gap_em as f32;

    let mut warnings = Vec::new();
    // Which caption each figure took, by index into `captions`.
    let mut taken: Vec<Option<usize>> = vec![None; images.len()];

    for (index, caption) in captions.iter().enumerate() {
        // Distance to every figure on the same page, nearest first. "Below then above" is the
        // typographic convention, applied as a tie-break rather than as a filter: a caption
        // set above its figure is unusual and not wrong.
        let mut ranked: Vec<(usize, f32)> = images
            .iter()
            .enumerate()
            .filter(|(_, image)| image.page.index == caption.page)
            .map(|(position, image)| {
                (
                    position,
                    edge_distance(caption.bbox, image.bbox)
                        + above_penalty(caption.bbox, image.bbox),
                )
            })
            .filter(|(_, distance)| *distance <= gap)
            .collect();
        ranked.sort_by(|(left_index, left), (right_index, right)| {
            left.total_cmp(right).then(left_index.cmp(right_index))
        });

        let Some((best, best_distance)) = ranked.first().copied() else {
            continue;
        };
        let second = ranked.get(1).map(|(_, distance)| *distance);
        // No second figure in range is not ambiguity: it is the ordinary case, and the ratio
        // is unbounded. Only a *contest* can be too close to call.
        let decisive = match second {
            None => true,
            Some(second) if best_distance <= 0.0 => second > 0.0,
            Some(second) => f64::from(second / best_distance) >= t.caption.distance_ratio_min,
        };

        if !decisive {
            warnings.push(
                Warning::new(W_CAPTION_AMBIGUOUS, Severity::Warn)
                    .with_arg("caption", caption.text.clone())
                    .with_arg("best", format!("{best_distance:.2}"))
                    .with_arg("second", format!("{:.2}", second.unwrap_or_default()))
                    .with_blocks(vec![caption.block])
                    .with_page(oc_model::extract::PageRef::new(caption.page)),
            );
            continue;
        }
        // A figure keeps the first caption that chose it. Two captions for one figure is
        // itself a detection failure, and the second is warned rather than silently dropped.
        match taken.get_mut(best) {
            Some(slot @ None) => *slot = Some(index),
            _ => warnings.push(
                Warning::new(W_CAPTION_AMBIGUOUS, Severity::Warn)
                    .with_arg("caption", caption.text.clone())
                    .with_arg("reason", "figure already captioned")
                    .with_blocks(vec![caption.block]),
            ),
        }
    }

    // **Which block** each figure took, recorded here where the decision is made. A figure
    // keeps only its caption's *text*, and the claim used to be re-derived by searching for
    // that text — so a chapter title repeated as a running head on fifteen pages, bound once
    // to one figure, had all fifteen copies claimed and one emitted (PHASE 7.5). The same
    // shape as the list and table claims before it: a second predicate standing in for a
    // decision that had already been made.
    let bound: std::collections::BTreeMap<oc_model::ids::BlockId, FigureId> = taken
        .iter()
        .enumerate()
        .filter_map(|(position, index)| {
            let caption = index.and_then(|index| captions.get(index))?;
            Some((
                caption.block,
                FigureId(u32::try_from(position).unwrap_or(u32::MAX)),
            ))
        })
        .collect();

    let figures = images
        .iter()
        .enumerate()
        .map(|(position, image)| {
            let caption = taken
                .get(position)
                .copied()
                .flatten()
                .and_then(|index| captions.get(index));
            Figure {
                id: FigureId(u32::try_from(position).unwrap_or(u32::MAX)),
                image: image.id,
                caption: caption.map(|caption| vec![Span::plain(caption.text.clone())]),
                // Empty rather than invented. An empty `alt` is what marks an image
                // decorative in EPUB, and 95 % of the alt text that exists in real PDFs is
                // the literal word "Image" (R1 §A.10) — writing that would be worse than
                // writing nothing, because it claims a description was made.
                alt: String::new(),
                anchor: anchor_for(image, blocks),
                confidence: Confidence::deterministic(vec![
                    Signal::new("has_caption", f32::from(u8::from(caption.is_some()))),
                    Signal::new(
                        "caption_has_prefix",
                        f32::from(u8::from(caption.is_some_and(|c| c.has_prefix))),
                    ),
                ]),
            }
        })
        .collect();

    (figures, captions, warnings, bound)
}

/// The blocks that read as captions.
pub fn caption_candidates(
    blocks: &[BlockView],
    body_size_pt: f32,
    lang: &LangTag,
    t: &Thresholds,
) -> Vec<CaptionCandidate> {
    blocks
        .iter()
        .filter_map(|block| {
            let text = block.text.trim();
            if text.is_empty() {
                return None;
            }
            let has_prefix = starts_with_caption_prefix(text, lang);
            // Either the prefix, or a small-or-italic block — the two cues PIPELINE §8.4
            // names. Nothing else: a full-size roman paragraph beside a picture is a
            // paragraph beside a picture.
            let small = body_size_pt > 0.0 && block.size_pt() < body_size_pt;
            let italic = block.runs().next().is_some_and(|run| run.italic);
            if !has_prefix && !(small || italic) {
                return None;
            }
            if f64::from(block.width_ratio()) > t.caption.max_width_ratio {
                return None;
            }
            Some(CaptionCandidate {
                block: block.id,
                page: block.page,
                text: text.to_owned(),
                bbox: block.bbox,
                has_prefix,
            })
        })
        .collect()
}

/// `Figure 1`, `Abb. 2`, `Şekil 3` — a localized prefix followed by a number.
///
/// The number is required. "Table manners" is not a caption, and a rule that matched the
/// word alone would make one of every paragraph that opens with it.
pub fn starts_with_caption_prefix(text: &str, lang: &LangTag) -> bool {
    let mut words = text.split_whitespace();
    let Some(first) = words.next() else {
        return false;
    };
    // The leading letters of the first token, so that `Fig.3`, `Fig. 3` and `Figure 3` are
    // one case rather than three.
    let word: String = first.chars().take_while(|c| c.is_alphabetic()).collect();
    if !CAPTION_WORDS.contains(&fold_key(&word, lang.clone()).as_str()) {
        return false;
    }
    // The number may be glued to the prefix (`Fig.1`) or the next token (`Fig. 1`).
    let tail = first.get(word.len()..).unwrap_or_default();
    let number = tail.trim_start_matches(['.', ':']).trim().to_owned();
    let number = if number.is_empty() {
        words.next().unwrap_or_default().to_owned()
    } else {
        number
    };
    let number = number.trim_end_matches(['.', ':', ')', '\u{2014}']);
    !number.is_empty()
        && (number.chars().all(|c| c.is_ascii_digit())
            || crate::headings::numbering::is_roman(number)
            || (number.chars().count() == 1 && number.chars().all(char::is_uppercase)))
}

/// The distance between the nearest edges of two boxes, zero when they overlap.
fn edge_distance(a: Rect, b: Rect) -> f32 {
    let dx = (b.x0 - a.x1).max(a.x0 - b.x1).max(0.0);
    let dy = (b.y0 - a.y1).max(a.y0 - b.y1).max(0.0);
    dx.hypot(dy)
}

/// The typographic convention, as a tie-break: a caption belongs below its figure.
///
/// A penalty rather than a filter, and a small one — half a point, far below any real gap —
/// so that it separates two figures at *identical* distances and changes nothing otherwise.
/// A caption set above its figure is unusual, not wrong.
fn above_penalty(caption: Rect, image: Rect) -> f32 {
    // Page space has y pointing down: the caption is below the image when its top is past
    // the image's bottom.
    if caption.y0 >= image.y1 {
        0.0
    } else {
        0.5
    }
}

/// The block a figure is placed before, in reading order.
///
/// The first block on the page that starts below the image, or — for an image at the foot of
/// a page — the last block on it. A figure with no block to anchor to at all takes the first
/// block of the document, because `Figure::anchor` is not optional: a figure that cannot say
/// where it goes cannot be emitted, and dropping it would lose a picture.
fn anchor_for(image: &ImageRef, blocks: &[BlockView]) -> BlockId {
    let on_page: Vec<&BlockView> = blocks
        .iter()
        .filter(|block| block.page == image.page.index)
        .collect();
    on_page
        .iter()
        .find(|block| block.bbox.y0 >= image.bbox.y1)
        .or_else(|| on_page.last())
        .map(|block| block.id)
        .or_else(|| blocks.first().map(|block| block.id))
        .unwrap_or_else(|| {
            BlockId::derive(
                image.page.index,
                image.bbox,
                &format!("figure {}", image.id.0),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localized_prefixes_are_recognised_with_their_number() {
        for text in [
            "Figure 1: a picture",
            "Fig. 2 — a picture",
            "Fig.3 a picture",
            "Table 4",
            "Abbildung 5: ein Bild",
            "Abb. 6",
            "Şekil 7: bir resim",
            "Tablo 8",
            "Plate IV",
            "Listing A",
        ] {
            assert!(
                starts_with_caption_prefix(text, &LangTag::EN)
                    || starts_with_caption_prefix(text, &LangTag::DE)
                    || starts_with_caption_prefix(text, &LangTag::TR),
                "{text} should read as a caption"
            );
        }
    }

    /// The number is what makes it a caption. Without it the rule would turn one paragraph in
    /// every cookery book into a caption.
    #[test]
    fn a_prefix_word_without_a_number_is_not_a_caption() {
        for text in [
            "Table manners are a whole subject.",
            "Figure it out for yourself.",
            "Charter of the United Nations",
            "",
        ] {
            assert!(!starts_with_caption_prefix(text, &LangTag::EN), "{text}");
        }
    }

    #[test]
    fn edge_distance_is_zero_for_overlapping_boxes_and_grows_with_the_gap() {
        let a = Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 10.0,
            y1: 10.0,
        };
        assert_eq!(edge_distance(a, a), 0.0);
        assert_eq!(
            edge_distance(
                a,
                Rect {
                    x0: 0.0,
                    y0: 20.0,
                    x1: 10.0,
                    y1: 30.0
                }
            ),
            10.0
        );
        // Diagonal: 3-4-5.
        assert_eq!(
            edge_distance(
                a,
                Rect {
                    x0: 13.0,
                    y0: 14.0,
                    x1: 20.0,
                    y1: 20.0
                }
            ),
            5.0
        );
    }
}

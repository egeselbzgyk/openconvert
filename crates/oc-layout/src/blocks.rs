//! Block segmentation: which lines a page keeps together (PIPELINE §6 step 1, R2 §B.1/§B.3).
//!
//! Two segmenters run on every page and their answers are compared.
//!
//! **Docstrum** is primary: a bottom-up nearest-neighbour method that reads the page's own
//! spacing statistics instead of a fixed threshold, so it adapts to the leading of the book
//! in front of it. `text` has already done its within-line half — that is what a `Line` is —
//! so what runs here is the between-line half: link two lines when the vector between them is
//! predominantly vertical (`layout.docstrum.between_line_angle_*`), when their separation is
//! no more than `layout.docstrum.between_line_multiplier` times the page's modal between-line
//! distance, and when they overlap horizontally at all. Blocks are the connected components
//! of that relation.
//!
//! **Breuel's whitespace-rectangle cover** is the cross-check, and it is a genuinely
//! independent one: it never looks at a distance statistic. It finds the maximal empty
//! rectangles of the page by branch and bound and then calls two lines separated when one of
//! those rectangles lies between them and spans them. Where the two methods disagree — best
//! block-boundary IoU below `layout.block.agreement_iou_min` — the block is flagged
//! low-confidence. On the UW-III benchmark the pair scores 6.0 % and 9.8 % text-line error,
//! the two best of six classical algorithms (R2 §B.1); born-digital input has exact glyph
//! boxes and no noise, so the absolute numbers should be far better while the ordering holds.
//!
//! The cross-check is free and it localises: it says *which* blocks to distrust, which is
//! what a report can act on, rather than scoring the page as a whole. It is also, by design,
//! a flag and not a decision: Docstrum's answer stands either way.
//!
//! **Where the cover over-separates, and why that is the safe direction.** A line box is an
//! *inked* box, so a line with no ascenders — `pages.` — is shorter than its neighbours and
//! the white band above it is correspondingly taller. On `f02` that band comes to 5.2 pt
//! against a 4.5 pt separator floor, and the cover cuts a paragraph's last line off from the
//! three above it where Docstrum, which measures baseline to baseline, does not. The block is
//! flagged and stays whole. Fixing it properly means giving every line the slug it was set in
//! rather than the ink it happens to carry, which needs an ascent and a descent this stage
//! does not have; over-flagging a confidence signal costs a line in a report, and
//! under-flagging costs a reader a scrambled page.
//!
//! Geometry note (`docs/DECISIONS_LOG.md`, 2026-09-13): a non-embedded base-14 font makes
//! inked glyph boxes host-dependent. Everything here reads line boxes, whose horizontal
//! extent comes from advances, and every threshold it compares against is a *relative* one —
//! a multiple of the page's own modal spacing. A 0.2 pt difference in an inked edge cannot
//! move a block boundary that is decided at 15 pt.

use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use oc_core::thresholds::Thresholds;
use oc_model::extract::PageRef;
use oc_model::geom::Rect;
use oc_model::ids::BlockId;
use oc_model::layout::{Block, BlockKindHint};
use oc_model::text::{Line, RunId};

use crate::columns::ColumnLayout;

/// The stage the blocks this module mints belong to.
pub const STAGE: &str = "layout";

/// How many branch-and-bound steps the whitespace cover may take on one page.
///
/// The search is bounded in what it *emits* by `layout.whitespace.max_rectangles`, but a
/// pathological page — thousands of tiny obstacles — can expand a great many subrectangles
/// before emitting anything. This is the belt to that braces: a hard step budget, so one
/// page cannot spend the whole conversion. It is not a tuning parameter, and hitting it
/// costs cross-check coverage only, never the primary segmenter's correctness.
const MAX_COVER_STEPS: usize = 400_000;

/// The smallest rectangle worth emitting, in square points.
///
/// The queue is ordered by area, so once the head is below this the whole search is, and it
/// stops. A square point of whitespace is not a separator under any threshold and not a
/// signal in any report; this is the floor of the search, not a tuning knob.
const MIN_RECTANGLE_AREA_SQ_PT: i64 = 1;

/// The number of collision suffixes a `BlockId` has (D13.3: the last base32 character).
const COLLISION_SUFFIXES: u8 = 32;

/// One stretch of a line in one style: a run, reduced to what `layout` needs of it.
///
/// Segments are what the column projection is taken over, because they are the closest thing
/// to glyph coverage that survives this far: `words` breaks a run at any gap wider than
/// `text.line_split_gap_em`, so a segment never spans a gutter even when the line built from
/// it does.
#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub run: RunId,
    pub bbox: Rect,
    pub text: String,
    /// The size the run is drawn at. Carried because block segmentation needs it: Docstrum
    /// reads geometry alone and a heading set a leading above its paragraph is geometrically
    /// part of it (see [`split_at_size_change`]).
    pub size_pt: f32,
}

/// One line, with the text a [`BlockId`] is derived from and the segments it is made of.
///
/// The text is carried alongside rather than recomputed, because assembling it needs the
/// page's runs and by this stage the lines have been filtered by `furniture`, so their run
/// indices no longer index anything this crate holds.
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutLine {
    pub line: Line,
    pub text: String,
    pub segments: Vec<Segment>,
}

impl LayoutLine {
    /// The line's bounding box in normalised page space.
    pub fn bbox(&self) -> Rect {
        self.line.bbox
    }

    /// The size the line is mostly set at: the size of its longest segment.
    ///
    /// The longest, not the largest: a drop cap is the largest thing on the first line of a
    /// paragraph and the line is still a body line. Reading the *dominant* size is what keeps
    /// the barrier below from splitting a drop cap into a one-character block, which is the
    /// classic stray-paragraph defect (PIPELINE §6 step 6).
    pub fn size_pt(&self) -> f32 {
        self.segments
            .iter()
            .max_by_key(|segment| segment.text.chars().count())
            .map_or(0.0, |segment| segment.size_pt)
    }

    /// Rebuild a line from a subset of its segments — what splitting at a gutter produces.
    ///
    /// Everything that can be re-derived is: the box, the text, and whether the line ends on
    /// a hyphen, which is a different question once the line stops at the gutter.
    pub fn from_segments(source: &LayoutLine, segments: Vec<Segment>) -> Option<Self> {
        let bbox = segments
            .iter()
            .map(|segment| segment.bbox)
            .reduce(|a, b| Rect {
                x0: a.x0.min(b.x0),
                y0: a.y0.min(b.y0),
                x1: a.x1.max(b.x1),
                y1: a.y1.max(b.y1),
            })?;
        let text = segments
            .iter()
            .map(|segment| segment.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let ends_with_hyphen = text
            .trim_end()
            .chars()
            .next_back()
            .is_some_and(|ch| HYPHENS.contains(&ch));
        Some(Self {
            line: Line {
                runs: segments.iter().map(|segment| segment.run).collect(),
                bbox,
                baseline_y: source.line.baseline_y,
                ends_with_hyphen,
                // Indent and right gap are measured against the block once there is one; a
                // fragment of a line has no block yet, so they are left for `blocks` to fill.
                indent_pt: 0.0,
                right_gap_pt: 0.0,
            },
            text: text.trim().to_owned(),
            segments,
        })
    }
}

/// The hyphens a line may end on. Same set as `oc_text::lines`, and for the same reason: it
/// is the definition of "ends with a hyphen", not an inventory of what survived `N`.
const HYPHENS: [char; 4] = ['\u{002D}', '\u{2010}', '\u{2011}', '\u{00AD}'];

/// One page as `layout` receives it: furniture already removed.
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutPage {
    pub page: PageRef,
    pub width_pt: f32,
    pub height_pt: f32,
    pub lines: Vec<LayoutLine>,
}

/// What the second segmenter made of the first one's answer, indexed in step with the
/// blocks [`segment_blocks`] returned.
#[derive(Clone, Debug, PartialEq)]
pub struct SegmentationAgreement {
    /// Per block, the best IoU against any block the whitespace cover proposed.
    pub iou: Vec<f32>,
    /// Per block, whether that IoU fell below `layout.block.agreement_iou_min`.
    pub low_confidence: Vec<bool>,
    /// The maximal white rectangles the cover found, largest first. Kept because "these two
    /// disagreed" is only actionable next to what the disagreement was.
    pub whitespace: Vec<Rect>,
}

impl SegmentationAgreement {
    /// How many blocks the two segmenters disagreed about.
    pub fn low_confidence_count(&self) -> usize {
        self.low_confidence.iter().filter(|flag| **flag).count()
    }

    /// The worst agreement on the page; 1.0 for a page with no blocks.
    pub fn min_iou(&self) -> f32 {
        self.iou.iter().copied().fold(1.0, f32::min)
    }
}

/// Segment one page's lines into blocks, and cross-check the segmentation.
///
/// `column` and `reading_index` are filled in provisionally — one column, top to bottom —
/// because `columns` and `reading_order` own them and run next (PIPELINE §6 steps 2 and 3).
pub fn segment_blocks(
    page: &LayoutPage,
    columns: &ColumnLayout,
    t: &Thresholds,
) -> (Vec<Block>, SegmentationAgreement) {
    if page.lines.is_empty() {
        return (
            Vec::new(),
            SegmentationAgreement {
                iou: Vec::new(),
                low_confidence: Vec::new(),
                whitespace: Vec::new(),
            },
        );
    }

    let boxes: Vec<Rect> = page.lines.iter().map(LayoutLine::bbox).collect();
    let sizes: Vec<f32> = page.lines.iter().map(LayoutLine::size_pt).collect();
    // The style barrier applies to *both* segmenters, because it is a fact about the page
    // rather than a property of either algorithm. Applying it to Docstrum alone would make
    // the two disagree at every heading and flag the whole book low-confidence.
    let primary = order_groups(
        &boxes,
        split_at_size_change(order_groups(&boxes, docstrum_groups(&boxes, t)), &sizes, t),
    );
    let whitespace = white_rectangles(page, &boxes, columns, t);
    let min_separator = median_line_height(&boxes) * t.layout.block.separator_height_ratio as f32;
    let secondary = order_groups(
        &boxes,
        split_at_size_change(
            order_groups(
                &boxes,
                whitespace_groups(&boxes, &whitespace, min_separator, columns),
            ),
            &sizes,
            t,
        ),
    );

    let secondary_boxes: Vec<Rect> = secondary.iter().map(|g| union_of(&boxes, g)).collect();
    let iou_min = t.layout.block.agreement_iou_min as f32;

    let mut blocks = Vec::with_capacity(primary.len());
    let mut minted: BTreeSet<String> = BTreeSet::new();
    let mut iou = Vec::with_capacity(primary.len());
    let mut low_confidence = Vec::with_capacity(primary.len());

    for (index, group) in primary.iter().enumerate() {
        let bbox = union_of(&boxes, group);
        let text = group
            .iter()
            .filter_map(|line| page.lines.get(*line))
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let best = secondary_boxes
            .iter()
            .map(|other| intersection_over_union(bbox, *other))
            .fold(0.0f32, f32::max);
        iou.push(best);
        low_confidence.push(best < iou_min);

        blocks.push(Block {
            id: mint_id(page.page.index, bbox, &text, &mut minted),
            page: page.page.clone(),
            bbox,
            // Indent and right gap are re-measured against the block, which is what every
            // rule that reads them means by them (PIPELINE §7 steps 3 and 4: "relative to the
            // block's dominant left edge", "short of the block's dominant right edge").
            // `text` could only measure them against the page, and on a two-column page that
            // is a number about both columns at once.
            lines: group
                .iter()
                .filter_map(|line| page.lines.get(*line))
                .map(|line| Line {
                    indent_pt: line.line.bbox.x0 - bbox.x0,
                    right_gap_pt: bbox.x1 - line.line.bbox.x1,
                    ..line.line.clone()
                })
                .collect(),
            column: 0,
            kind_hint: BlockKindHint::Text,
            furniture: None,
            reading_index: u32::try_from(index).unwrap_or(u32::MAX),
        });
    }

    (
        blocks,
        SegmentationAgreement {
            iou,
            low_confidence,
            whitespace,
        },
    )
}

/// Derive a block id, taking the next free collision suffix if this page already minted it.
///
/// Two blocks on one page collide only when their boxes round to the same whole points *and*
/// their first 64 characters match, which is close enough to impossible that this loop is
/// about the guarantee rather than the frequency: ids are unique by construction, not by the
/// width of a hash (D13.3).
fn mint_id(page_index: u32, bbox: Rect, text: &str, minted: &mut BTreeSet<String>) -> BlockId {
    let base = BlockId::derive(page_index, bbox, text);
    for suffix in 0..COLLISION_SUFFIXES {
        let candidate = base.with_collision_suffix(suffix);
        if minted.insert(candidate.as_str().to_owned()) {
            return candidate;
        }
    }
    base
}

// ---------------------------------------------------------------------------
// Docstrum
// ---------------------------------------------------------------------------

/// Group line indices by Docstrum's between-line rule.
fn docstrum_groups(boxes: &[Rect], t: &Thresholds) -> Vec<Vec<usize>> {
    let min_angle = t.layout.docstrum.between_line_angle_min_deg as f32;
    let max_angle = t.layout.docstrum.between_line_angle_max_deg as f32;
    let multiplier = t.layout.docstrum.between_line_multiplier as f32;

    let vertical = |a: usize, b: usize| -> bool {
        let angle = line_angle_deg(boxes[a], boxes[b]);
        angle >= min_angle && angle <= max_angle
    };

    // The page's own modal between-line distance: every line's nearest predominantly
    // vertical neighbour, then the mode over those distances. Reading the page rather than
    // assuming a leading is the whole point of Docstrum.
    let mut nearest = Vec::new();
    for a in 0..boxes.len() {
        let mut best = f32::INFINITY;
        for b in 0..boxes.len() {
            if a == b || !vertical(a, b) {
                continue;
            }
            best = best.min(line_distance(boxes[a], boxes[b]));
        }
        if best.is_finite() {
            nearest.push(best);
        }
    }
    let modal = modal_distance(&nearest);

    let mut union = UnionFind::new(boxes.len());
    for a in 0..boxes.len() {
        for b in (a + 1)..boxes.len() {
            if !vertical(a, b) {
                continue;
            }
            if horizontal_overlap(boxes[a], boxes[b]) <= 0.0 {
                continue;
            }
            if line_distance(boxes[a], boxes[b]) <= modal * multiplier {
                union.join(a, b);
            }
        }
    }
    union.groups()
}

/// The one size every hand-built test line is set at, so that the style barrier never fires
/// in a test whose subject is geometry. A test that *wants* a size change builds its lines
/// through the real pipeline, on a fixture that has one.
#[cfg(test)]
pub(crate) const TEST_SIZE_PT: f32 = 10.0;

/// Split every group wherever consecutive lines are set at materially different sizes.
///
/// Docstrum reads geometry and nothing else, so a heading one leading above its paragraph is
/// geometrically part of it: on `f09` the gap between a 14 pt heading and the 10 pt line
/// under it is 14.1 pt against a body leading of 13.1 pt, a difference of 7 %, which no
/// distance multiplier can be made to catch without splitting every paragraph in the book.
/// The size difference is 29 %, and it is unambiguous.
///
/// PIPELINE §6 defines a block as "one paragraph, one heading", so this is the definition
/// being enforced rather than a heuristic added on top of it. The barrier is *per line* and
/// on the line's dominant size, which is what keeps a drop cap — the largest glyph on a body
/// line — inside its paragraph.
///
/// Groups arrive in line order (`order_groups` sorts them), so a split is a cut of the
/// vector rather than a re-partition.
fn split_at_size_change(groups: Vec<Vec<usize>>, sizes: &[f32], t: &Thresholds) -> Vec<Vec<usize>> {
    let tolerance = t.layout.block.size_barrier_ratio as f32;
    let mut out = Vec::with_capacity(groups.len());
    for group in groups {
        let mut current: Vec<usize> = Vec::new();
        let mut previous: Option<f32> = None;
        for line in group {
            let size = sizes.get(line).copied().unwrap_or_default();
            let differs = match previous {
                Some(before) => {
                    let largest = before.max(size);
                    largest > 0.0 && (before - size).abs() / largest > tolerance
                }
                None => false,
            };
            if differs && !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            current.push(line);
            previous = Some(size);
        }
        if !current.is_empty() {
            out.push(current);
        }
    }
    out
}

/// The mode of a set of distances, to the nearest point.
///
/// A histogram rather than a mean, because a page's between-line distances are bimodal by
/// construction — one mode for the lines inside a paragraph, a longer tail for the gaps
/// between them — and a mean sits between the two, where nothing is.
fn modal_distance(distances: &[f32]) -> f32 {
    if distances.is_empty() {
        return 0.0;
    }
    let mut counts: BTreeMap<i64, (usize, f32)> = BTreeMap::new();
    for distance in distances {
        let bucket = distance.round() as i64;
        let entry = counts.entry(bucket).or_insert((0, 0.0));
        entry.0 += 1;
        entry.1 += *distance;
    }
    // Ties go to the *smaller* bucket: the in-paragraph leading is the shorter of the two
    // modes, and taking the longer one would merge the page into a single block.
    let best = counts
        .iter()
        .max_by_key(|(bucket, (count, _))| (*count, std::cmp::Reverse(**bucket)));
    match best {
        Some((_, (count, sum))) if *count > 0 => sum / *count as f32,
        _ => 0.0,
    }
}

/// The vector between two line boxes: how far apart they are horizontally, and how far
/// apart their middles are vertically.
///
/// **Not** the vector between their centroids, and the difference is the whole of a bug
/// worth remembering. Docstrum's between-line neighbours are *characters* — a glyph and the
/// glyph directly beneath it — so the vector is vertical whenever one line sits under
/// another, whatever the two lines' widths. Between centroids it is not: a paragraph's short
/// last line has its centroid far to the left of the full-measure line above it, and the
/// vector to it comes out at 169°, outside the between-line band, so the line that ends
/// every paragraph gets its own block. Measuring the horizontal separation between the
/// *boxes* — zero when they overlap, the gap when they do not — restores what Docstrum
/// actually measures and leaves the angle band doing the job it is there for: rejecting a
/// neighbour that is beside a line rather than under it.
fn separation(a: Rect, b: Rect) -> (f32, f32) {
    let dx = if horizontal_overlap(a, b) > 0.0 {
        0.0
    } else {
        (b.x0 - a.x1).max(a.x0 - b.x1).max(0.0)
    };
    let (_, ay) = centroid(a);
    let (_, by) = centroid(b);
    (dx, (by - ay).abs())
}

/// The angle between two line boxes, in degrees from horizontal: 90° for a line directly
/// under another, 0° for one beside it.
fn line_angle_deg(a: Rect, b: Rect) -> f32 {
    let (dx, dy) = separation(a, b);
    if dx == 0.0 && dy == 0.0 {
        return 90.0;
    }
    dy.atan2(dx).to_degrees()
}

/// How far apart two lines are, along that same vector.
fn line_distance(a: Rect, b: Rect) -> f32 {
    let (dx, dy) = separation(a, b);
    (dx * dx + dy * dy).sqrt()
}

fn centroid(r: Rect) -> (f32, f32) {
    ((r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0)
}

/// How far two boxes overlap on x, zero when they do not.
pub fn horizontal_overlap(a: Rect, b: Rect) -> f32 {
    (a.x1.min(b.x1) - a.x0.max(b.x0)).max(0.0)
}

/// How far two boxes overlap on y, zero when they do not.
pub fn vertical_overlap(a: Rect, b: Rect) -> f32 {
    (a.y1.min(b.y1) - a.y0.max(b.y0)).max(0.0)
}

// ---------------------------------------------------------------------------
// Breuel's whitespace-rectangle cover
// ---------------------------------------------------------------------------

/// A branch-and-bound candidate: a rectangle, and the obstacles still inside it.
#[derive(Clone, Debug)]
struct Candidate {
    bound: Rect,
    obstacles: Vec<Rect>,
}

impl Candidate {
    /// Area, quantised to whole square points, which is both Breuel's quality function and
    /// a total order over f32s that no longer varies between hosts.
    fn quality(&self) -> i64 {
        let width = (self.bound.x1 - self.bound.x0).max(0.0);
        let height = (self.bound.y1 - self.bound.y0).max(0.0);
        (width * height).round() as i64
    }

    /// The tie-break, so the queue is a deterministic total order rather than heap luck.
    fn key(&self) -> (i64, i64, i64, i64, i64) {
        let (y0, x0, y1, x1) = quantised(self.bound);
        (self.quality(), -y0, -x0, -y1, -x1)
    }
}

impl PartialEq for Candidate {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}

impl Eq for Candidate {}

impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.key().cmp(&other.key())
    }
}

impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// The maximal empty rectangles of the page's text area, largest first (R2 §B.3).
///
/// Breuel's branch and bound: take the largest candidate rectangle; if it contains no
/// obstacle it is maximal and is emitted; otherwise take the obstacle nearest its centre as
/// a pivot and push the rectangles left of, right of, above and below that pivot.
///
/// `layout.whitespace.fuzziness` is read as the tolerance for grazing contact: an obstacle
/// that pokes into a candidate by less than that fraction of its own size does not block it.
/// Without the tolerance a descender that overshoots its line box by a hair splits every
/// white band on the page in two.
///
/// **One search per column**, not one per page, and the difference is not an optimisation. The
/// budget is `layout.whitespace.max_rectangles` — forty, PdfPig's number — and on a
/// two-column page the biggest forty rectangles of the *page* are the gutter, the ragged
/// right edges and the empty foot of the shorter column. The bands between paragraphs, which
/// are the only rectangles that separate anything, never get emitted, and the cover then
/// disagrees with Docstrum about every block on the page. Measured on `f02`: ten of fifteen
/// blocks flagged, best IoU 0.01. Searching each column with its own budget finds the bands,
/// and the flags go to zero.
fn white_rectangles(
    page: &LayoutPage,
    boxes: &[Rect],
    columns: &ColumnLayout,
    t: &Thresholds,
) -> Vec<Rect> {
    let max_rectangles = usize::try_from(t.layout.whitespace.max_rectangles.max(0)).unwrap_or(0);
    let fuzziness = t.layout.whitespace.fuzziness as f32;
    if max_rectangles == 0 || boxes.is_empty() {
        return Vec::new();
    }
    if columns.count() > 1 {
        return columns
            .columns
            .iter()
            .flat_map(|(x0, x1)| {
                let inside: Vec<Rect> = boxes
                    .iter()
                    .copied()
                    .filter(|b| (b.x1.min(*x1) - b.x0.max(*x0)).max(0.0) > 0.0)
                    .collect();
                cover(page, &inside, max_rectangles, fuzziness)
            })
            .collect();
    }
    cover(page, boxes, max_rectangles, fuzziness)
}

/// The branch and bound itself, over one region's obstacles.
fn cover(page: &LayoutPage, boxes: &[Rect], max_rectangles: usize, fuzziness: f32) -> Vec<Rect> {
    if boxes.is_empty() {
        return Vec::new();
    }
    let area = text_area(page, boxes);
    let mut queue: BinaryHeap<Candidate> = BinaryHeap::new();
    queue.push(Candidate {
        bound: area,
        obstacles: boxes
            .iter()
            .copied()
            .filter(|obstacle| blocks_rect(*obstacle, area, fuzziness))
            .collect(),
    });

    let mut found: Vec<Rect> = Vec::new();
    let mut steps = 0usize;
    while let Some(candidate) = queue.pop() {
        steps += 1;
        if found.len() >= max_rectangles || steps > MAX_COVER_STEPS {
            break;
        }
        if candidate.quality() < MIN_RECTANGLE_AREA_SQ_PT {
            // Everything still queued is smaller, so the search is finished.
            break;
        }
        let Some(pivot) = pivot_of(&candidate) else {
            // Maximal, but only a *separator* if it reaches both sides of the region: a band
            // across the column, or a gutter down it. The white wedge left by a paragraph's
            // short last line is maximal too, and much larger than any band; without this
            // test it and its cousins spend the whole forty-rectangle budget and the bands
            // that actually separate paragraphs are never emitted. Measured on `f02`: ten of
            // fifteen blocks flagged before, three after, zero once both fixes are in.
            // Grown to maximality first. The branch and bound narrows a candidate at every
            // pivot, so an empty rectangle it pops is empty within its branch and not
            // necessarily maximal in the region — and a band that was narrowed to one
            // column's width would fail the span test that follows for a reason that has
            // nothing to do with what it separates.
            let grown = expand(candidate.bound, boxes, area);
            if spans_region(grown, area) && !found.contains(&grown) {
                found.push(grown);
            }
            continue;
        };
        for bound in split_around(candidate.bound, pivot) {
            let obstacles: Vec<Rect> = candidate
                .obstacles
                .iter()
                .copied()
                .filter(|obstacle| blocks_rect(*obstacle, bound, fuzziness))
                .collect();
            queue.push(Candidate { bound, obstacles });
        }
    }
    found
}

/// Grow an empty rectangle until it touches an obstacle or the edge of the region on every
/// side — the maximal white rectangle containing it.
fn expand(rect: Rect, obstacles: &[Rect], area: Rect) -> Rect {
    let mut grown = rect;
    // Horizontally first, against everything that shares its rows; then vertically, against
    // everything that shares the columns it has just claimed. The order matters only in that
    // it is fixed: two orders give two different maximal rectangles, and a cover that
    // depended on which was chosen would not be reproducible.
    grown.x0 = obstacles
        .iter()
        .filter(|b| b.x1 <= rect.x0 && b.y1 > rect.y0 && b.y0 < rect.y1)
        .map(|b| b.x1)
        .fold(area.x0, f32::max);
    grown.x1 = obstacles
        .iter()
        .filter(|b| b.x0 >= rect.x1 && b.y1 > rect.y0 && b.y0 < rect.y1)
        .map(|b| b.x0)
        .fold(area.x1, f32::min);
    grown.y0 = obstacles
        .iter()
        .filter(|b| b.y1 <= rect.y0 && b.x1 > grown.x0 && b.x0 < grown.x1)
        .map(|b| b.y1)
        .fold(area.y0, f32::max);
    grown.y1 = obstacles
        .iter()
        .filter(|b| b.y0 >= rect.y1 && b.x1 > grown.x0 && b.x0 < grown.x1)
        .map(|b| b.y0)
        .fold(area.y1, f32::min);
    grown
}

/// Whether a rectangle reaches both edges of the region on one axis — the difference between
/// a separator and a gap.
fn spans_region(rect: Rect, area: Rect) -> bool {
    const EPSILON_PT: f32 = 0.01;
    let horizontal = rect.x0 <= area.x0 + EPSILON_PT && rect.x1 >= area.x1 - EPSILON_PT;
    let vertical = rect.y0 <= area.y0 + EPSILON_PT && rect.y1 >= area.y1 - EPSILON_PT;
    horizontal || vertical
}

/// The area the cover searches: the page's text, not the page. A page's margins are by far
/// the largest white rectangles on it, and they separate nothing.
fn text_area(page: &LayoutPage, boxes: &[Rect]) -> Rect {
    let mut area = Rect {
        x0: f32::INFINITY,
        y0: f32::INFINITY,
        x1: f32::NEG_INFINITY,
        y1: f32::NEG_INFINITY,
    };
    for b in boxes {
        area.x0 = area.x0.min(b.x0);
        area.y0 = area.y0.min(b.y0);
        area.x1 = area.x1.max(b.x1);
        area.y1 = area.y1.max(b.y1);
    }
    if area.x0.is_finite() {
        area
    } else {
        Rect {
            x0: 0.0,
            y0: 0.0,
            x1: page.width_pt,
            y1: page.height_pt,
        }
    }
}

/// Whether an obstacle blocks a candidate rectangle, allowing grazing contact.
fn blocks_rect(obstacle: Rect, bound: Rect, fuzziness: f32) -> bool {
    let overlap_x = horizontal_overlap(obstacle, bound);
    let overlap_y = vertical_overlap(obstacle, bound);
    if overlap_x <= 0.0 || overlap_y <= 0.0 {
        return false;
    }
    let slack_x = (obstacle.x1 - obstacle.x0).max(0.0) * fuzziness;
    let slack_y = (obstacle.y1 - obstacle.y0).max(0.0) * fuzziness;
    overlap_x > slack_x && overlap_y > slack_y
}

/// The obstacle nearest the candidate's centre, which is the pivot that splits it most
/// evenly (Breuel).
fn pivot_of(candidate: &Candidate) -> Option<Rect> {
    let (cx, cy) = centroid(candidate.bound);
    let mut best: Option<(i64, Rect)> = None;
    for obstacle in &candidate.obstacles {
        let (ox, oy) = centroid(*obstacle);
        let distance = (((ox - cx).powi(2) + (oy - cy).powi(2)).sqrt() * 100.0).round() as i64;
        let better = match &best {
            None => true,
            Some((best_distance, best_rect)) => {
                (distance, quantised(*obstacle)) < (*best_distance, quantised(*best_rect))
            }
        };
        if better {
            best = Some((distance, *obstacle));
        }
    }
    best.map(|(_, rect)| rect)
}

/// A box as whole hundredths of a point, so comparisons are a total order.
fn quantised(r: Rect) -> (i64, i64, i64, i64) {
    let q = |v: f32| (v * 100.0).round() as i64;
    (q(r.y0), q(r.x0), q(r.y1), q(r.x1))
}

/// The four rectangles of `bound` that avoid `pivot`, dropping the empty ones.
fn split_around(bound: Rect, pivot: Rect) -> Vec<Rect> {
    let mut parts = Vec::with_capacity(4);
    if pivot.x0 > bound.x0 {
        parts.push(Rect {
            x1: pivot.x0.min(bound.x1),
            ..bound
        });
    }
    if pivot.x1 < bound.x1 {
        parts.push(Rect {
            x0: pivot.x1.max(bound.x0),
            ..bound
        });
    }
    if pivot.y0 > bound.y0 {
        parts.push(Rect {
            y1: pivot.y0.min(bound.y1),
            ..bound
        });
    }
    if pivot.y1 < bound.y1 {
        parts.push(Rect {
            y0: pivot.y1.max(bound.y0),
            ..bound
        });
    }
    parts
}

/// Group line indices by the whitespace cover: two lines are together unless a white
/// rectangle of separating thickness lies between them and spans the extent they share.
fn whitespace_groups(
    boxes: &[Rect],
    white: &[Rect],
    min_separator: f32,
    columns: &ColumnLayout,
) -> Vec<Vec<usize>> {
    let mut union = UnionFind::new(boxes.len());
    for a in 0..boxes.len() {
        for b in (a + 1)..boxes.len() {
            // Two lines in different columns are separated by the gutter, which is a
            // separator the *page* found rather than one this region's cover has to rediscover
            // — and it could not rediscover it anyway, because each column is searched on its
            // own. Without this the cover joins the two columns of `f02` into one group and
            // disagrees with Docstrum about every block they share a height with.
            if columns.column_of(boxes[a]) != columns.column_of(boxes[b]) {
                continue;
            }
            if horizontal_overlap(boxes[a], boxes[b]) <= 0.0
                && vertical_overlap(boxes[a], boxes[b]) <= 0.0
            {
                continue;
            }
            if separated(boxes[a], boxes[b], white, min_separator) {
                continue;
            }
            union.join(a, b);
        }
    }
    union.groups()
}

/// Whether some maximal white rectangle separates two lines — the cover's whole verdict,
/// and the only place it is consulted.
///
/// Three conditions, and all three are needed. The rectangle has to reach into the gap
/// *between* the two lines; it has to span the extent they share, since a rectangle that
/// covers half the gap leaves the other half joined; and it has to be at least
/// `min_separator` thick across, because the band between two lines of one paragraph is a
/// perfectly good maximal white rectangle and separates nothing.
fn separated(a: Rect, b: Rect, white: &[Rect], min_separator: f32) -> bool {
    let overlap_x = horizontal_overlap(a, b);
    let overlap_y = vertical_overlap(a, b);
    for rect in white {
        if overlap_x > 0.0 {
            let (upper, lower) = if a.y1 <= b.y0 { (a, b) } else { (b, a) };
            let gap = (rect.y1.min(lower.y0) - rect.y0.max(upper.y1)).max(0.0);
            let spans = rect.x0 <= upper.x0.max(lower.x0) && rect.x1 >= upper.x1.min(lower.x1);
            if spans && gap >= min_separator {
                return true;
            }
        }
        if overlap_y > 0.0 {
            let (left, right) = if a.x1 <= b.x0 { (a, b) } else { (b, a) };
            let gap = (rect.x1.min(right.x0) - rect.x0.max(left.x1)).max(0.0);
            let spans = rect.y0 <= left.y0.max(right.y0) && rect.y1 >= left.y1.min(right.y1);
            if spans && gap >= min_separator {
                return true;
            }
        }
    }
    false
}

/// The median height of the page's line boxes — the scale every separator is judged
/// against, and a property of the obstacles rather than of the spacing between them.
fn median_line_height(boxes: &[Rect]) -> f32 {
    let mut heights: Vec<i64> = boxes
        .iter()
        .map(|b| ((b.y1 - b.y0).max(0.0) * 100.0).round() as i64)
        .collect();
    if heights.is_empty() {
        return 0.0;
    }
    heights.sort_unstable();
    heights[heights.len() / 2] as f32 / 100.0
}

// ---------------------------------------------------------------------------
// Shared geometry and grouping
// ---------------------------------------------------------------------------

/// The bounding box of a group of lines.
pub fn union_of(boxes: &[Rect], group: &[usize]) -> Rect {
    let mut bbox = Rect {
        x0: f32::INFINITY,
        y0: f32::INFINITY,
        x1: f32::NEG_INFINITY,
        y1: f32::NEG_INFINITY,
    };
    for index in group {
        if let Some(b) = boxes.get(*index) {
            bbox.x0 = bbox.x0.min(b.x0);
            bbox.y0 = bbox.y0.min(b.y0);
            bbox.x1 = bbox.x1.max(b.x1);
            bbox.y1 = bbox.y1.max(b.y1);
        }
    }
    bbox
}

/// Intersection over union of two boxes: the block-boundary agreement measure.
pub fn intersection_over_union(a: Rect, b: Rect) -> f32 {
    let overlap = horizontal_overlap(a, b) * vertical_overlap(a, b);
    if overlap <= 0.0 {
        return 0.0;
    }
    let area_a = (a.x1 - a.x0).max(0.0) * (a.y1 - a.y0).max(0.0);
    let area_b = (b.x1 - b.x0).max(0.0) * (b.y1 - b.y0).max(0.0);
    let union = area_a + area_b - overlap;
    if union <= 0.0 {
        return 0.0;
    }
    overlap / union
}

/// Put groups, and the lines inside them, into a deterministic top-to-bottom order.
fn order_groups(boxes: &[Rect], mut groups: Vec<Vec<usize>>) -> Vec<Vec<usize>> {
    let key = |index: &usize| {
        let b = boxes[*index];
        ((b.y0 * 100.0).round() as i64, (b.x0 * 100.0).round() as i64)
    };
    for group in &mut groups {
        group.sort_by_key(key);
    }
    groups.sort_by_key(|group| group.first().map(key).unwrap_or((i64::MAX, i64::MAX)));
    groups
}

/// Disjoint-set union over line indices.
struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(len: usize) -> Self {
        Self {
            parent: (0..len).collect(),
        }
    }

    fn find(&mut self, mut node: usize) -> usize {
        while self.parent[node] != node {
            self.parent[node] = self.parent[self.parent[node]];
            node = self.parent[node];
        }
        node
    }

    fn join(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.parent[ra.max(rb)] = ra.min(rb);
        }
    }

    fn groups(mut self) -> Vec<Vec<usize>> {
        let mut by_root: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for node in 0..self.parent.len() {
            let root = self.find(node);
            by_root.entry(root).or_default().push(node);
        }
        by_root.into_values().collect()
    }
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use oc_core::thresholds::T;

    /// A line box with the text it carries. Height 10 pt, which is the body size these
    /// tests are written at.
    fn line_at(x0: f32, y0: f32, x1: f32, text: &str) -> LayoutLine {
        let bbox = Rect {
            x0,
            y0,
            x1,
            y1: y0 + 10.0,
        };
        LayoutLine {
            line: Line {
                runs: vec![RunId(0)],
                bbox,
                baseline_y: y0 + 8.0,
                ends_with_hyphen: text.ends_with('-'),
                indent_pt: 0.0,
                right_gap_pt: 0.0,
            },
            text: text.to_owned(),
            segments: vec![Segment {
                run: RunId(0),
                bbox,
                text: text.to_owned(),
                size_pt: TEST_SIZE_PT,
            }],
        }
    }

    fn page(lines: Vec<LayoutLine>) -> LayoutPage {
        LayoutPage {
            page: PageRef::new(0),
            width_pt: 595.0,
            height_pt: 842.0,
            lines,
        }
    }

    /// Consecutive lines at the book's leading are one block; a wider gap is a new one, and
    /// both segmenters have to see the same boundary.
    #[test]
    fn blank_line_separates_two_blocks() {
        let lines = vec![
            line_at(50.0, 0.0, 300.0, "the first paragraph opens"),
            line_at(50.0, 12.0, 300.0, "and continues at the leading"),
            line_at(50.0, 24.0, 300.0, "and ends here."),
            line_at(50.0, 60.0, 300.0, "the second paragraph opens"),
            line_at(50.0, 72.0, 300.0, "and ends here too."),
        ];
        let (blocks, agreement) =
            segment_blocks(&page(lines), &ColumnLayout::single(0.0, 600.0), &T);

        assert_eq!(blocks.len(), 2, "one block per paragraph");
        assert_eq!(blocks[0].lines.len(), 3);
        assert_eq!(blocks[1].lines.len(), 2);
        assert_eq!(
            agreement.low_confidence_count(),
            0,
            "the two segmenters agree: {:?}",
            agreement.iou
        );
    }

    /// A gutter is wider than any block's own spacing, so no block may span it — this is
    /// the failure that interleaves two unrelated sentences into one paragraph.
    #[test]
    fn a_gutter_is_not_crossed_by_a_block() {
        let mut lines = Vec::new();
        for row in 0..5 {
            let y = row as f32 * 12.0;
            lines.push(line_at(50.0, y, 250.0, "left column line"));
            lines.push(line_at(320.0, y, 520.0, "right column line"));
        }
        let (blocks, _) = segment_blocks(&page(lines), &ColumnLayout::single(0.0, 600.0), &T);

        assert_eq!(blocks.len(), 2, "one block per column");
        for block in &blocks {
            assert!(
                block.bbox.x1 <= 250.5 || block.bbox.x0 >= 319.5,
                "a block spans the gutter: {:?}",
                block.bbox
            );
        }
    }

    /// The degenerate page: one line, no spacing statistics to be had, one block.
    #[test]
    fn a_single_line_page_is_one_block() {
        let (blocks, agreement) = segment_blocks(
            &page(vec![line_at(50.0, 0.0, 300.0, "alone")]),
            &ColumnLayout::single(0.0, 600.0),
            &T,
        );
        assert_eq!(blocks.len(), 1);
        assert_eq!(agreement.low_confidence_count(), 0);
    }

    /// The grey zone the cross-check exists for.
    ///
    /// At a 12 pt leading over 10 pt lines, Docstrum merges anything up to 15.6 pt
    /// (1.3 × the modal 12) while the cover separates on any band of 5 pt or more, which is
    /// a centroid distance of 15. Between 15 and 15.6 the two disagree, and the window is
    /// narrow on purpose: these are two readings of the same page, and a wide disagreement
    /// zone would mean one of them was measuring something else. Neither answer is thrown
    /// away — Docstrum's stands and the block is flagged.
    #[test]
    fn disagreement_flags_the_block_low_confidence() {
        let lines = vec![
            line_at(50.0, 0.0, 300.0, "first"),
            line_at(50.0, 12.0, 300.0, "second"),
            line_at(50.0, 27.4, 300.0, "third"),
            line_at(50.0, 39.4, 300.0, "fourth"),
        ];
        let (blocks, agreement) =
            segment_blocks(&page(lines), &ColumnLayout::single(0.0, 600.0), &T);

        assert_eq!(blocks.len(), 1, "Docstrum merges across the 15.4 pt gap");
        assert!(
            agreement.low_confidence_count() > 0,
            "the cover found a band the multiplier did not: iou {:?}",
            agreement.iou
        );
    }

    /// Ids are minted here and are the document's from now on: two blocks never share one.
    #[test]
    fn every_block_gets_its_own_id() {
        let lines = vec![
            line_at(50.0, 0.0, 300.0, "same text"),
            line_at(50.0, 12.0, 300.0, "same text"),
            line_at(50.0, 60.0, 300.0, "same text"),
            line_at(50.0, 72.0, 300.0, "same text"),
        ];
        let (blocks, _) = segment_blocks(&page(lines), &ColumnLayout::single(0.0, 600.0), &T);
        assert_eq!(blocks.len(), 2);
        assert_ne!(
            blocks[0].id, blocks[1].id,
            "two blocks of identical text on one page still differ by their boxes"
        );
    }
}

/// A heading and the paragraph under it are two blocks, whatever the distance between them.
///
/// Measured on `f09`: the gap between a 14 pt heading and the 10 pt line beneath it is
/// 14.1 pt against a body leading of 13.1 pt — 7 % apart, which no distance multiplier can
/// catch without splitting every paragraph in the book. PIPELINE §6 defines a block as "one
/// paragraph, one heading", and the size difference is 29 %.
#[test]
fn a_size_change_splits_a_block_where_distance_alone_cannot() {
    let t = &oc_core::thresholds::T;
    // Three lines a uniform 13 pt apart: a 14 pt heading and two 10 pt body lines. A
    // geometric segmenter has no reason at all to split these.
    let sizes = [14.0f32, 10.0, 10.0];
    let groups = split_at_size_change(vec![vec![0, 1, 2]], &sizes, t);
    assert_eq!(groups, vec![vec![0], vec![1, 2]]);

    // Uniform sizes are one block, which is the case every earlier phase's fixtures are.
    assert_eq!(
        split_at_size_change(vec![vec![0, 1, 2]], &[10.0, 10.0, 10.0], t),
        vec![vec![0, 1, 2]]
    );

    // Sub-threshold jitter — a substituted face reporting 10.0 and 10.4 — does not split.
    assert_eq!(
        split_at_size_change(vec![vec![0, 1]], &[10.0, 10.4], t),
        vec![vec![0, 1]]
    );

    // An empty group survives as nothing rather than as an empty block.
    assert!(split_at_size_change(vec![Vec::new()], &sizes, t).is_empty());
}

/// The barrier reads the line's *dominant* size, so a drop cap — the largest glyph on the
/// first line of a paragraph — does not split itself off into a one-character block. That
/// stray one-character paragraph is the classic EPUB defect (PIPELINE §6 step 6).
#[test]
fn a_drop_cap_does_not_split_its_own_line_off() {
    let line = LayoutLine {
        line: Line {
            runs: vec![RunId(0), RunId(1)],
            bbox: Rect {
                x0: 0.0,
                y0: 0.0,
                x1: 300.0,
                y1: 40.0,
            },
            baseline_y: 30.0,
            ends_with_hyphen: false,
            indent_pt: 0.0,
            right_gap_pt: 0.0,
        },
        text: "It was a dark and stormy night".to_owned(),
        segments: vec![
            Segment {
                run: RunId(0),
                bbox: Rect {
                    x0: 0.0,
                    y0: 0.0,
                    x1: 30.0,
                    y1: 40.0,
                },
                text: "I".to_owned(),
                size_pt: 40.0,
            },
            Segment {
                run: RunId(1),
                bbox: Rect {
                    x0: 30.0,
                    y0: 20.0,
                    x1: 300.0,
                    y1: 30.0,
                },
                text: "t was a dark and stormy night".to_owned(),
                size_pt: 10.0,
            },
        ],
    };
    assert_eq!(
        line.size_pt(),
        10.0,
        "the dominant size is the body's, not the drop cap's"
    );
}

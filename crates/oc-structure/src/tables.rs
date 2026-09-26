//! Tables: ruling lines into a grid, or the image fallback (PIPELINE §8.7).
//!
//! **v1 detects tables well enough not to destroy them and does not invest in high-fidelity
//! structure recovery** (R2 §B.9). The evidence for that scope is that Camelot's lattice
//! parser — the state of the deterministic art — reaches F1 0.778 / TEDS 0.789 on ICDAR-2013's
//! 67 *ruled* PDFs and is materially worse on borderless ones, and that a reflowable EPUB
//! renders a wide table poorly on a 6″ screen however faithfully it was recovered. Table
//! over-investment is named as a trap.
//!
//! What is **not** optional is the fallback's shape. Accessibility settles it rather than
//! engineering taste: an image of a table "takes the content away from anyone who cannot see
//! it", tabular data must use real table markup, and styling a `td` to look like a header is
//! called out as a common bad practice (DAISY, R10 §6.12). So HTML is the default, the image
//! is an explicit warned fallback, **and even the fallback carries the data** — which is why
//! a fallback table here still has its rows filled in.
//!
//! The gate onto the HTML path is a conservation check in miniature: the multiset of the
//! cells' text must equal the multiset of the text inside the table's region. It catches a
//! grid that dropped a cell, a grid that duplicated one, and — the reason PIPELINE §8.7 names
//! it — a hallucinated cell, if a vision model is ever added.
//!
//! Before either path, a region has to be a table at all. Rules bound it; they do not make it
//! one. A rule printed in the same place on a large share of the pages is the page's furniture
//! and bounds nothing, and a region must show columns — vertical rules dividing it, or rows of cells that
//! repeat one gutter — or its text stays in the flow as the prose it is.
//!
//! And a rule is read whole. Books draw a table's rules in pieces — a segment per column, four
//! lines per cell, a filled box per header cell — so the pieces are joined, and the sides of
//! abutting cell boxes read as rules, before any region is looked for.

use oc_core::thresholds::Thresholds;
use oc_model::confidence::{Confidence, Signal};
use oc_model::doc::{Cell, Severity, Span, Table, Warning};
use oc_model::extract::{ImageId, VectorRegion};
use oc_model::geom::Rect;
use oc_model::ids::{BlockId, TableId};
use serde::Serialize;

use crate::view::BlockView;

/// A table could not be emitted as markup and became an image plus its text (PIPELINE §8.7).
pub const W_TABLE_AS_IMAGE: &str = "W_TABLE_AS_IMAGE";

/// What table extraction produced.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TableOutcome {
    pub tables: Vec<Table>,
    /// Per table, the region it occupies, for the rasteriser that Phase 5 runs over a
    /// fallback.
    pub regions: Vec<TableRegion>,
    /// The blocks whose text is inside a table, so the flow does not emit them twice.
    pub consumed: Vec<BlockId>,
    /// Which table took each consumed block. Derived where the text was taken, so it cannot
    /// disagree with `consumed` (PHASE 7.5).
    pub claimed_by: std::collections::BTreeMap<BlockId, TableId>,
    pub warnings: Vec<Warning>,
}

/// Where a table sits, and what it was made of.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TableRegion {
    pub id: TableId,
    pub page: u32,
    pub bbox: Rect,
    pub rows: u32,
    pub columns: u32,
    /// Whether the grid passed both validations and was emitted as markup.
    pub gridded: bool,
}

/// Find the tables on a document's pages.
///
/// `page_count` is the document's, for telling a page's furniture from a table's rules: a
/// rule printed in the same place on a large enough share of the pages is the former.
///
/// `first_fallback_image` is where fallback image ids start: a rasterised table is an image
/// the document did not contain, so it cannot reuse an `ImageId` that names one that it did.
pub fn extract_tables(
    vectors: &[VectorRegion],
    page_count: u32,
    blocks: &[BlockView],
    skip: &std::collections::BTreeSet<BlockId>,
    first_fallback_image: u32,
    t: &Thresholds,
) -> TableOutcome {
    let mut outcome = TableOutcome {
        tables: Vec::new(),
        regions: Vec::new(),
        consumed: Vec::new(),
        claimed_by: std::collections::BTreeMap::new(),
        warnings: Vec::new(),
    };
    let rules = assemble_rules(vectors, t);
    let mut pages: Vec<u32> = rules.iter().map(|rule| rule.page).collect();
    pages.sort_unstable();
    pages.dedup();
    let furniture = furniture_rules(&rules, page_count, t);

    for page in pages {
        for region in regions_on(&rules, &furniture, page, blocks, t) {
            let id = TableId(u32::try_from(outcome.tables.len()).unwrap_or(u32::MAX));
            // What is no longer available: blocks a higher-precedence structure owns, and
            // blocks an earlier region on this page already took. Two overlapping regions
            // both taking one block emitted it twice (PHASE 7.5,
            // `structure/appeared/contested-claim`).
            let unavailable: std::collections::BTreeSet<BlockId> = skip
                .iter()
                .chain(outcome.consumed.iter())
                .copied()
                .collect();
            let Some((table, gridded, took)) = build_table(
                id,
                &region,
                blocks,
                &unavailable,
                page,
                first_fallback_image.saturating_add(id.0),
                t,
                &mut outcome.warnings,
            ) else {
                continue;
            };
            // Exactly the blocks whose text went into this table — not every block whose
            // box sits inside the region. The two predicates were different, and the blocks
            // in the gap were claimed and never emitted (PHASE 7.5).
            outcome.consumed.extend(took.iter().copied());
            outcome
                .claimed_by
                .extend(took.into_iter().map(|block| (block, id)));
            outcome.regions.push(TableRegion {
                id,
                page,
                bbox: region.bbox,
                rows: u32::try_from(table.rows.len()).unwrap_or(u32::MAX),
                columns: u32::try_from(table.rows.first().map_or(0, Vec::len)).unwrap_or(u32::MAX),
                gridded,
            });
            outcome.tables.push(table);
        }
    }
    outcome
}

/// One candidate table: the rules that bound it.
struct Lattice {
    bbox: Rect,
    /// The y of each horizontal rule, ascending. `n` rules bound `n - 1` rows.
    rows: Vec<f32>,
    /// The x of each vertical rule, ascending.
    columns: Vec<f32>,
}

/// A rule as a table's lattice reads it: one thin, axis-aligned line on one page, whole.
///
/// Not a `VectorRegion`, because a region is one path object and a table's rule is often
/// several. Measured on four technical books, not one table rule was a single path: one
/// strokes a separate segment per column along every row boundary, another strokes each
/// cell's four sides as four separate lines, and all of them shade the header row as one
/// filled box per cell. Read path by path, the sweep that groups overlapping rules met the
/// pieces of one row boundary side by side rather than over each other and broke at every
/// one, no region formed, and every table's cells were run together into a paragraph
/// (2026-09-26).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Rule {
    page: u32,
    bbox: Rect,
}

impl Rule {
    fn is_horizontal(&self) -> bool {
        lies_across(self.bbox)
    }
}

/// Whether a box is wider than it is tall: a horizontal rule, when it is a rule at all.
fn lies_across(bbox: Rect) -> bool {
    (bbox.x1 - bbox.x0) >= (bbox.y1 - bbox.y0)
}

/// The rules on the document's pages, whole: the pieces a rule was drawn in joined into one,
/// and the sides of a grid of cell boxes read as the rules they draw.
///
/// Pieces are one rule when they lie on one line — centres within `table.grid_snap_pt`, the
/// same tolerance that makes two coordinates one cell edge — and meet end to end, overlapping
/// or at most `table.rule_join_gap_pt` apart.
fn assemble_rules(vectors: &[VectorRegion], t: &Thresholds) -> Vec<Rule> {
    let mut pieces: std::collections::BTreeMap<u32, Vec<Rect>> = std::collections::BTreeMap::new();
    let mut boxes: std::collections::BTreeMap<u32, Vec<Rect>> = std::collections::BTreeMap::new();
    let thickest = t.vector.rule_max_thickness_pt as f32;
    for region in vectors {
        let bbox = region.bbox;
        if region.is_rule {
            pieces.entry(region.page.index).or_default().push(bbox);
        } else if (bbox.x1 - bbox.x0).min(bbox.y1 - bbox.y0) > thickest {
            // Thicker than any rule both ways: a box that may be a cell. Anything thinner
            // that is not a rule is a mark too short for its thickness — a dash, a dot.
            boxes.entry(region.page.index).or_default().push(bbox);
        }
    }
    for (page, found) in boxes {
        let edges = cell_edges(&found, t);
        if !edges.is_empty() {
            pieces.entry(page).or_default().extend(edges);
        }
    }

    let snap = t.table.grid_snap_pt as f32;
    let gap = t.table.rule_join_gap_pt as f32;
    pieces
        .into_iter()
        .flat_map(|(page, found)| {
            let (across, down): (Vec<Rect>, Vec<Rect>) =
                found.into_iter().partition(|bbox| lies_across(*bbox));
            // A vertical is joined as the horizontal it is when the page is turned on its side.
            let down = join_collinear(down.into_iter().map(transpose).collect(), snap, gap)
                .into_iter()
                .map(transpose);
            join_collinear(across, snap, gap)
                .into_iter()
                .chain(down)
                .map(move |bbox| Rule { page, bbox })
        })
        .collect()
}

/// Join horizontal pieces that lie on one line and meet end to end into one rule each.
fn join_collinear(mut pieces: Vec<Rect>, snap: f32, gap: f32) -> Vec<Rect> {
    pieces.sort_by(|a, b| {
        centre_y(*a)
            .total_cmp(&centre_y(*b))
            .then(a.x0.total_cmp(&b.x0))
    });
    // A line is the pieces whose centres are within `snap` of its first piece's.
    let mut lines: Vec<Vec<Rect>> = Vec::new();
    for piece in pieces {
        match lines.last_mut() {
            Some(line)
                if line
                    .first()
                    .is_some_and(|first| centre_y(piece) - centre_y(*first) <= snap) =>
            {
                line.push(piece);
            }
            _ => lines.push(vec![piece]),
        }
    }
    let mut joined = Vec::new();
    for mut line in lines {
        line.sort_by(|a, b| a.x0.total_cmp(&b.x0));
        let mut open: Option<Rect> = None;
        for piece in line {
            open = match open {
                Some(rule) if piece.x0 <= rule.x1 + gap => Some(union(rule, piece)),
                Some(rule) => {
                    joined.push(rule);
                    Some(piece)
                }
                None => Some(piece),
            };
        }
        joined.extend(open);
    }
    joined
}

/// A box with its axes exchanged.
fn transpose(bbox: Rect) -> Rect {
    Rect {
        x0: bbox.y0,
        y0: bbox.x0,
        x1: bbox.y1,
        y1: bbox.x1,
    }
}

/// The four sides of every box that abuts another, as rules.
///
/// A table drawn as cells is a grid of boxes, each sharing a whole side with its neighbour:
/// one top and one bottom with the box beside it, one left and one right with the box above
/// or below. Their sides are the table's rules. A box that abuts nothing is a frame — a code
/// listing's shading, a sidebar's border — or one of a diagram's shapes, which stand apart;
/// it bounds no grid and is left alone, as it was before boxes were read at all.
fn cell_edges(boxes: &[Rect], t: &Thresholds) -> Vec<Rect> {
    let snap = t.table.grid_snap_pt as f32;
    let near = |a: f32, b: f32| (a - b).abs() <= snap;
    let mut cell = vec![false; boxes.len()];
    let mut order: Vec<usize> = (0..boxes.len()).collect();

    // Side by side: the same top and bottom, one's right side on the other's left. Sorted by
    // top, a box need only be compared with the boxes whose tops are within `snap` of its own.
    order.sort_by(|&a, &b| boxes[a].y0.total_cmp(&boxes[b].y0));
    for (at, &a) in order.iter().enumerate() {
        for &b in order
            .iter()
            .skip(at + 1)
            .take_while(|&&b| boxes[b].y0 - boxes[a].y0 <= snap)
        {
            let (one, other) = (boxes[a], boxes[b]);
            if near(one.y1, other.y1) && (near(one.x1, other.x0) || near(other.x1, one.x0)) {
                cell[a] = true;
                cell[b] = true;
            }
        }
    }
    // Stacked: the same left and right, one's foot on the other's head.
    order.sort_by(|&a, &b| boxes[a].x0.total_cmp(&boxes[b].x0));
    for (at, &a) in order.iter().enumerate() {
        for &b in order
            .iter()
            .skip(at + 1)
            .take_while(|&&b| boxes[b].x0 - boxes[a].x0 <= snap)
        {
            let (one, other) = (boxes[a], boxes[b]);
            if near(one.x1, other.x1) && (near(one.y1, other.y0) || near(other.y1, one.y0)) {
                cell[a] = true;
                cell[b] = true;
            }
        }
    }

    boxes
        .iter()
        .zip(cell)
        .filter(|(_, cell)| *cell)
        .flat_map(|(bbox, _)| {
            [
                Rect {
                    y1: bbox.y0,
                    ..*bbox
                },
                Rect {
                    y0: bbox.y1,
                    ..*bbox
                },
                Rect {
                    x1: bbox.x0,
                    ..*bbox
                },
                Rect {
                    x0: bbox.x1,
                    ..*bbox
                },
            ]
        })
        .collect()
}

/// The rules that are part of the pages' furniture, by their index in `rules`.
///
/// A rule printed in the same place on a large share of the book's pages — the rule over a
/// running foot, under a running head — belongs to the page, not to anything on it. This is
/// the criterion the furniture stage deletes repeated text by (R2 §B.4 step 4: at least
/// `layout.furniture.min_repeat_pages` pages, and at least `layout.furniture.min_repeat_page_share`
/// of them), applied to rules. Kept as a table's rule, the rule over a running foot bounded a
/// "table" with any rule above it on the page — a chapter's opening rule, a sidebar's frame,
/// the last rule of a real table — and took everything printed in between: whole contents,
/// index and glossary pages, and the prose under a table (2026-09-26).
///
/// "The same place" is every edge within about `table.grid_snap_pt`: edges are counted on a
/// grid of that pitch, and a rule in a neighbouring cell counts as the same rule, so two rules
/// a hair apart that straddle a cell boundary still match. Unlike the furniture stage, the
/// count is not capped at the page count: in a document of two pages, the same rule on both is
/// as likely one table set twice as it is furniture.
///
/// Asked of the rules whole, after their pieces are joined: a running head's rule drawn in
/// pieces is still one rule in one place on every page.
fn furniture_rules(
    rules: &[Rule],
    page_count: u32,
    t: &Thresholds,
) -> std::collections::BTreeSet<usize> {
    let snap = t.table.grid_snap_pt as f32;
    let place = |bbox: Rect| -> [i64; 4] {
        [bbox.x0, bbox.y0, bbox.x1, bbox.y1].map(|edge| (edge / snap).round() as i64)
    };
    let mut pages_at: std::collections::BTreeMap<[i64; 4], std::collections::BTreeSet<u32>> =
        std::collections::BTreeMap::new();
    for rule in rules {
        pages_at
            .entry(place(rule.bbox))
            .or_default()
            .insert(rule.page);
    }
    let by_count = usize::try_from(t.layout.furniture.min_repeat_pages).unwrap_or(usize::MAX);
    let by_share =
        (f64::from(page_count) * t.layout.furniture.min_repeat_page_share).ceil() as usize;
    let required = by_count.max(by_share);

    // Every cell within one step of `at` on each of the four edges, `at` itself included.
    let near = |at: [i64; 4]| {
        let steps = [-1i64, 0, 1];
        steps.into_iter().flat_map(move |a| {
            steps.into_iter().flat_map(move |b| {
                steps.into_iter().flat_map(move |c| {
                    steps
                        .into_iter()
                        .map(move |d| [at[0] + a, at[1] + b, at[2] + c, at[3] + d])
                })
            })
        })
    };
    let repeated: std::collections::BTreeSet<[i64; 4]> = pages_at
        .keys()
        .filter(|at| {
            let mut pages = std::collections::BTreeSet::new();
            for cell in near(**at) {
                if let Some(found) = pages_at.get(&cell) {
                    pages.extend(found.iter().copied());
                }
            }
            pages.len() >= required
        })
        .copied()
        .collect();

    rules
        .iter()
        .enumerate()
        .filter(|(_, rule)| repeated.contains(&place(rule.bbox)))
        .map(|(index, _)| index)
        .collect()
}

/// Group a page's rules into candidate tables. `furniture` names the rules, by index into
/// `rules`, that belong to the page rather than to anything on it; they bound nothing.
fn regions_on(
    rules: &[Rule],
    furniture: &std::collections::BTreeSet<usize>,
    page: u32,
    blocks: &[BlockView],
    t: &Thresholds,
) -> Vec<Lattice> {
    let on_page: Vec<&Rule> = rules
        .iter()
        .enumerate()
        .filter(|(index, rule)| rule.page == page && !furniture.contains(index))
        .map(|(_, rule)| rule)
        .collect();
    let horizontals: Vec<&Rule> = on_page
        .iter()
        .copied()
        .filter(|rule| rule.is_horizontal())
        .collect();
    let verticals: Vec<&Rule> = on_page
        .iter()
        .copied()
        .filter(|rule| !rule.is_horizontal())
        .collect();

    let minimum = usize::try_from(t.table.min_row_rules).unwrap_or(2);
    let snap = t.table.grid_snap_pt as f32;

    // Horizontals that overlap in x belong to one table. A greedy sweep down the page: each
    // rule joins the open group when it overlaps it, and starts a new one otherwise.
    let mut sorted: Vec<&Rule> = horizontals.clone();
    sorted.sort_by(|a, b| a.bbox.y0.total_cmp(&b.bbox.y0));

    let mut groups: Vec<Vec<&Rule>> = Vec::new();
    for rule in sorted {
        match groups.last_mut() {
            Some(open)
                if open.last().is_some_and(|last| {
                    overlap_share(last.bbox, rule.bbox) >= t.table.rule_overlap_min as f32
                }) =>
            {
                open.push(rule);
            }
            _ => groups.push(vec![rule]),
        }
    }

    let runs: Vec<&oc_model::text::Run> = blocks
        .iter()
        .filter(|block| block.page == page)
        .flat_map(BlockView::runs)
        .filter(|run| !run.text.trim().is_empty())
        .collect();

    groups
        .into_iter()
        .flat_map(|group| split_at_prose(group, &verticals, &runs, t))
        .filter(|group| group.len() >= minimum)
        .map(|group| {
            let bbox = bounds(&group);
            let rows = snapped(group.iter().map(|rule| centre_y(rule.bbox)), snap);
            let columns = snapped(
                columns_across(&verticals, bbox, bbox.y0, bbox.y1, snap)
                    .into_iter()
                    .map(|rule| centre_x(rule.bbox)),
                snap,
            );
            Lattice {
                bbox,
                rows,
                columns,
            }
        })
        .collect()
}

/// The box a group of rules spans.
fn bounds(group: &[&Rule]) -> Rect {
    group
        .iter()
        .map(|rule| rule.bbox)
        .reduce(union)
        .unwrap_or(Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 0.0,
            y1: 0.0,
        })
}

/// The verticals that divide a region's width between `top` and `bottom`: inside its x-range,
/// and reaching from the one to the other. A vertical that crosses only part of the height is
/// a cell divider in a merged row — or the side of a header cell's shading — and v1 does not
/// read merges.
fn columns_across<'a>(
    verticals: &[&'a Rule],
    region: Rect,
    top: f32,
    bottom: f32,
    snap: f32,
) -> Vec<&'a Rule> {
    verticals
        .iter()
        .copied()
        .filter(|rule| {
            rule.bbox.x0 >= region.x0 - snap
                && rule.bbox.x1 <= region.x1 + snap
                && rule.bbox.y0 <= top + snap
                && rule.bbox.y1 >= bottom - snap
                && bottom > top
        })
        .collect()
}

/// Split a group of horizontals wherever the band between two of them holds text that is not
/// set in cells.
///
/// Overlapping in x is what puts rules in one group, and a table's last rule overlaps
/// whatever rule comes next below it within its width — a footnote separator, a sidebar's
/// frame — however much running text lies between. Kept together, the two bounded one region,
/// the prose between them went into it, and the table was lost with it, because the region as a
/// whole no longer showed columns. So a band belongs to the table when vertical rules divide it
/// into columns, or when one of its printed rows sets cells apart at a gutter that another row
/// of the group shares — one, not all: a row whose cell wraps onto a second line prints that
/// line in one column only — and no more than one of its rows reaches past the rules' ends, as
/// a line of running text wider than the table does and no cell of it can. A table set with no
/// rule over its first row showed why: that row sat in the band between the table above and
/// its own rules, with the paragraph and the caption between the two, and was reason enough to
/// take them both (2026-09-26).
///
/// And a band of fewer rows than `table.min_aligned_rows` belongs to the table whatever they
/// show: too few rows to repeat a column structure are too few to refute one, and a single row
/// whose cells stand closer than a gutter — run together into one run by `text` — split a
/// table in two at every such row.
fn split_at_prose<'a>(
    group: Vec<&'a Rule>,
    verticals: &[&Rule],
    runs: &[&oc_model::text::Run],
    t: &Thresholds,
) -> Vec<Vec<&'a Rule>> {
    let snap = t.table.grid_snap_pt as f32;
    let region = bounds(&group);
    // Every run printed between the group's rules and across its width — not only those whose
    // centre is inside it: a line of running text is often wider than the table over it, and
    // its centre then falls outside the rules while its words run right across them.
    let across_region: Vec<&oc_model::text::Run> = runs
        .iter()
        .copied()
        .filter(|run| {
            let centre = centre_y(run.bbox);
            centre >= region.y0
                && centre <= region.y1
                && run.bbox.x1 > region.x0
                && run.bbox.x0 < region.x1
        })
        .collect();
    let rows = cell_rows(&across_region, t);

    let mut parts: Vec<Vec<&'a Rule>> = Vec::new();
    for rule in group {
        let open = parts.last_mut().and_then(|part| {
            let above = part.last()?;
            let (top, bottom) = (centre_y(above.bbox), centre_y(rule.bbox));
            let in_band: Vec<&CellRow> = rows
                .iter()
                .filter(|row| {
                    let centre = (row.top + row.bottom) / 2.0;
                    centre > top && centre < bottom
                })
                .collect();
            let ruled = snapped(
                columns_across(verticals, region, top, bottom, snap)
                    .into_iter()
                    .map(|rule| centre_x(rule.bbox)),
                snap,
            )
            .len()
            .saturating_sub(1);
            let few =
                |count: usize| i64::try_from(count).unwrap_or(i64::MAX) < t.table.min_aligned_rows;
            let overhanging = in_band
                .iter()
                .filter(|row| row.left < region.x0 - snap || row.right > region.x1 + snap)
                .count();
            let holds_cells = few(in_band.len())
                || i64::try_from(ruled).unwrap_or(i64::MAX) >= t.table.min_columns
                || (in_band.iter().any(|row| row.aligned) && few(overhanging));
            holds_cells.then_some(part)
        });
        match open {
            Some(part) => part.push(rule),
            None => parts.push(vec![rule]),
        }
    }
    parts
}

/// Build one table, as a grid if the grid validates and as the image fallback otherwise — or
/// nothing, when the region shows no column structure and so is not a table at all.
#[allow(clippy::too_many_arguments)]
fn build_table(
    id: TableId,
    lattice: &Lattice,
    blocks: &[BlockView],
    unavailable: &std::collections::BTreeSet<BlockId>,
    page: u32,
    fallback_image: u32,
    t: &Thresholds,
    warnings: &mut Vec<Warning>,
) -> Option<(
    Table,
    bool,
    std::collections::BTreeSet<oc_model::ids::BlockId>,
)> {
    // Everything printed inside the table's box, in reading order.
    //
    // *Runs*, not lines. A table row is one baseline, so `text` assembles its cells into a
    // single line — `"StageKindBudgetReason"` — and a grid filled from lines puts the whole
    // row in whichever cell its midpoint happens to fall in. The cells are separate runs,
    // because `words` breaks a run at a gap of `text.line_split_gap_em` and a column gutter
    // is many times that.
    // **The unit of taking and the unit of claiming must be the same unit.** The table used
    // to read text at *run* granularity and claim it at *block* granularity, and every block
    // that straddled the region's edge fell in the gap: claimed and half-emitted (text lost),
    // or emitted and not claimed (text duplicated). Both directions of
    // `structure/lost/claim-without-emission` come from that one mismatch.
    //
    // So a straddling block is left alone entirely. The table is built from the blocks that
    // are wholly inside it, and a block with a foot outside stays in the flow with all of its
    // text. The grid check below still has to pass on what remains, and the fallback still
    // keeps every character when it does not.
    let whole: std::collections::BTreeSet<oc_model::ids::BlockId> = blocks
        .iter()
        .filter(|block| block.page == page)
        .filter(|block| !unavailable.contains(&block.id))
        .filter(|block| {
            let runs: Vec<_> = block
                .runs()
                .filter(|run| !run.text.trim().is_empty())
                .collect();
            !runs.is_empty() && runs.iter().all(|run| inside(run.bbox, lattice.bbox))
        })
        .map(|block| block.id)
        .collect();
    let attributed: Vec<(oc_model::ids::BlockId, String)> =
        runs_inside_attributed(blocks, page, lattice.bbox)
            .into_iter()
            .filter(|(block, _)| whole.contains(block))
            .collect();
    let took: std::collections::BTreeSet<oc_model::ids::BlockId> =
        attributed.iter().map(|(block, _)| *block).collect();
    let source: Vec<String> = attributed.into_iter().map(|(_, text)| text).collect();
    // A region that takes no text has no data to read: a drawing's tiles, a form's empty
    // boxes. Emitted, it was an empty `<table>`; left alone, it is nothing, as before its
    // boxes were read as rules.
    if took.is_empty() {
        return None;
    }
    // And most of what is printed inside it must be in the blocks it takes. A block that
    // reaches outside stays in the flow whole, so a table built from what is left is a table
    // missing those rows. When `layout` had set a table's rows in one block with the paragraph
    // under it, what was left was a lone note mark, emitted as a one-cell table beside the
    // rows it belonged to (2026-09-26).
    let printed = |block: &BlockView| -> usize {
        block
            .runs()
            .filter(|run| inside(run.bbox, lattice.bbox))
            .map(|run| run.text.chars().filter(|c| !c.is_whitespace()).count())
            .sum()
    };
    let (taken, left) = blocks
        .iter()
        .filter(|block| block.page == page && !unavailable.contains(&block.id))
        .fold((0usize, 0usize), |(taken, left), block| {
            if took.contains(&block.id) {
                (taken + printed(block), left)
            } else {
                (taken, left + printed(block))
            }
        });
    if (taken as f64) < t.table.min_taken_text_share * ((taken + left) as f64) {
        return None;
    }

    // **Rules bound a region; they do not make it a table.** An underline under a line of
    // text, a running head's rule, a footnote separator and the frame of a box are all long
    // thin paths, and any two of them that overlap in x bounded a "table" — which took every
    // block between them out of the flow. A novel with no tables at all came out with 74, most
    // of them a chapter's number cut from its opener; a scanned book lost whole pages of prose
    // between its running head's rule and its footnote separator (2026-09-26).
    //
    // So the region must show columns. Vertical rules that divide it into columns are that
    // structure on their own. Otherwise its text must: enough of its rows set as cells apart at
    // a gutter that other rows share. Running prose is one run per line; a stretched justified
    // space breaks a run on a stray line, never on most of them.
    let ruled_columns = i64::try_from(lattice.columns.len().saturating_sub(1)).unwrap_or(0);
    if ruled_columns < t.table.min_columns {
        let runs: Vec<&oc_model::text::Run> = blocks
            .iter()
            .filter(|block| block.page == page && took.contains(&block.id))
            .flat_map(BlockView::runs)
            .filter(|run| inside(run.bbox, lattice.bbox))
            .collect();
        if !repeats_a_column_structure(cell_evidence(&runs, t), t) {
            return None;
        }
    }

    let enough_columns =
        i64::try_from(lattice.columns.len()).unwrap_or(0) >= t.table.min_column_rules;
    // A run whose box crosses a column rule cannot be placed in one cell. It happens when the
    // gutter between two cells is narrower than `text.line_split_gap_em`, so `words` kept the
    // two cells in one run: `"layoutConserving"`. There is no sound way to split it here — a
    // `Run` carries a box and its text, not its glyphs' positions — so the table takes the
    // fallback, which loses the grid and keeps every character. That is the direction
    // PIPELINE §8.7 prescribes for everything it cannot read cleanly.
    let straddles = enough_columns
        && blocks
            .iter()
            .filter(|block| block.page == page)
            .flat_map(BlockView::runs)
            .filter(|run| inside(run.bbox, lattice.bbox))
            .any(|run| crosses_a_rule(run.bbox, &lattice.columns));
    let rows = if enough_columns && !straddles {
        Some(fill_grid(lattice, blocks, page))
    } else {
        None
    };

    // The conservation check in miniature: the cells must hold exactly the text the region
    // holds, as a multiset. A grid that dropped, duplicated or invented a cell fails it.
    //
    // Of *words*, not of runs. A cell that wraps holds two runs joined into one string, which
    // no single run equals, so a multiset of runs refused every table with a cell set on two
    // lines — and technical books set their definitions and descriptions that way in table
    // after table (2026-09-26). A word dropped, duplicated or invented still fails it.
    let gridded = rows.as_ref().is_some_and(|rows| {
        !rows.is_empty()
            && rows.iter().all(|row| row.len() == rows[0].len())
            && multiset(rows.iter().flat_map(|row| row.iter().cloned()))
                == multiset(source.iter().cloned())
    });

    let signals = vec![
        Signal::new("rows", lattice.rows.len().saturating_sub(1) as f32),
        Signal::new("columns", lattice.columns.len().saturating_sub(1) as f32),
        Signal::new("cell_text_conserved", f32::from(u8::from(gridded))),
    ];

    if gridded {
        let rows = rows.unwrap_or_default();
        return Some((
            Table {
                id,
                rows: rows
                    .into_iter()
                    .map(|row| {
                        row.into_iter()
                            .map(|text| Cell::new(vec![Span::plain(text)]))
                            .collect()
                    })
                    .collect(),
                // Not guessed. DAISY names styling a `td` to look like a header as a common
                // bad practice (R10 §6.12); asserting a header row the document did not mark
                // is the same error in the other direction.
                header_rows: 0,
                caption: None,
                fallback_image: None,
                confidence: Confidence::deterministic(signals),
            },
            true,
            took,
        ));
    }

    warnings.push(
        Warning::new(W_TABLE_AS_IMAGE, Severity::Warn)
            .with_arg("rows", lattice.rows.len().to_string())
            .with_arg("columns", lattice.columns.len().to_string())
            .with_arg(
                "reason",
                if !enough_columns {
                    "no vertical rules"
                } else if straddles {
                    "a run crosses a column rule"
                } else {
                    "cell text is not the region's text"
                },
            )
            .with_page(oc_model::extract::PageRef::new(page)),
    );
    Some((
        Table {
            id,
            // The data is still here. An image of a table takes the content away from anyone
            // who cannot see it, so the fallback carries one row per printed line and the
            // reader gets it in a `<details>` (R10 §6.12).
            rows: source
                .into_iter()
                .map(|text| vec![Cell::new(vec![Span::plain(text)])])
                .collect(),
            header_rows: 0,
            caption: None,
            fallback_image: Some(ImageId(fallback_image)),
            confidence: Confidence::fallback(signals),
        },
        false,
        took,
    ))
}

/// Every run printed inside a box, in reading order, non-empty.
/// The text inside a region, and **which block each piece came from**.
///
/// The block travels with the text because that is what lets the caller claim exactly the
/// blocks whose text the table took, instead of claiming every block that happens to sit
/// inside the region's box and hoping the two sets agree (PHASE 7.5,
/// `structure/lost/claim-without-emission`).
fn runs_inside_attributed(
    blocks: &[BlockView],
    page: u32,
    bbox: Rect,
) -> Vec<(oc_model::ids::BlockId, String)> {
    let mut out = Vec::new();
    for block in blocks.iter().filter(|block| block.page == page) {
        for run in block.runs() {
            if !inside(run.bbox, bbox) {
                continue;
            }
            let text = run.text.trim().to_owned();
            if !text.is_empty() {
                out.push((block.id, text));
            }
        }
    }
    out
}

/// Put every run into the cell whose box contains its centre.
fn fill_grid(lattice: &Lattice, blocks: &[BlockView], page: u32) -> Vec<Vec<String>> {
    let rows = lattice.rows.len().saturating_sub(1);
    let columns = lattice.columns.len().saturating_sub(1);
    let mut grid = vec![vec![String::new(); columns]; rows];

    for run in blocks
        .iter()
        .filter(|block| block.page == page)
        .flat_map(BlockView::runs)
    {
        let text = run.text.trim();
        if text.is_empty() {
            continue;
        }
        let centre = (
            (run.bbox.x0 + run.bbox.x1) / 2.0,
            (run.bbox.y0 + run.bbox.y1) / 2.0,
        );
        let Some(row) = band_of(&lattice.rows, centre.1) else {
            continue;
        };
        let Some(column) = band_of(&lattice.columns, centre.0) else {
            continue;
        };
        if let Some(cell) = grid.get_mut(row).and_then(|row| row.get_mut(column)) {
            if !cell.is_empty() {
                cell.push(' ');
            }
            cell.push_str(text);
        }
    }
    grid
}

/// Which band of a sorted ladder a coordinate falls into.
fn band_of(ladder: &[f32], value: f32) -> Option<usize> {
    if ladder.len() < 2 || value < *ladder.first()? || value > *ladder.last()? {
        return None;
    }
    ladder
        .windows(2)
        .position(|pair| value >= pair[0] && value <= pair[1])
}

/// Collapse coordinates that are within `snap` of each other, ascending.
fn snapped(values: impl Iterator<Item = f32>, snap: f32) -> Vec<f32> {
    let mut sorted: Vec<f32> = values.collect();
    sorted.sort_by(f32::total_cmp);
    sorted.dedup_by(|a, b| (*a - *b).abs() <= snap);
    sorted
}

fn centre_x(bbox: Rect) -> f32 {
    (bbox.x0 + bbox.x1) / 2.0
}

fn centre_y(bbox: Rect) -> f32 {
    (bbox.y0 + bbox.y1) / 2.0
}

fn union(a: Rect, b: Rect) -> Rect {
    Rect {
        x0: a.x0.min(b.x0),
        y0: a.y0.min(b.y0),
        x1: a.x1.max(b.x1),
        y1: a.y1.max(b.y1),
    }
}

/// Whether a box crosses any of the ladder's rules with real width on both sides.
fn crosses_a_rule(bbox: Rect, ladder: &[f32]) -> bool {
    ladder.iter().any(|rule| bbox.x0 < *rule && bbox.x1 > *rule)
}

/// Whether `inner`'s centre lies inside `outer`.
fn inside(inner: Rect, outer: Rect) -> bool {
    let x = (inner.x0 + inner.x1) / 2.0;
    let y = (inner.y0 + inner.y1) / 2.0;
    x >= outer.x0 && x <= outer.x1 && y >= outer.y0 && y <= outer.y1
}

/// How much of the shorter box's width the two share.
fn overlap_share(a: Rect, b: Rect) -> f32 {
    let overlap = (a.x1.min(b.x1) - a.x0.max(b.x0)).max(0.0);
    let shortest = (a.x1 - a.x0).min(b.x1 - b.x0);
    if shortest <= 0.0 {
        return 0.0;
    }
    overlap / shortest
}

/// What the printed text inside a candidate region says about whether it is a table.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CellEvidence {
    /// Printed rows: the runs grouped by the band they sit in.
    rows: usize,
    /// Rows set as cells apart at a gutter that another such row shares: a repeated column
    /// structure.
    aligned: usize,
}

/// Whether a region's text is set in columns: enough rows, and a large enough share of all of
/// its rows, whose cells stand apart at a gutter another row shares.
fn repeats_a_column_structure(evidence: CellEvidence, t: &Thresholds) -> bool {
    i64::try_from(evidence.aligned).unwrap_or(i64::MAX) >= t.table.min_aligned_rows
        && evidence.aligned as f64 >= t.table.min_aligned_row_share * evidence.rows as f64
}

/// Measure the column structure of a region's runs.
fn cell_evidence(runs: &[&oc_model::text::Run], t: &Thresholds) -> CellEvidence {
    let rows = cell_rows(runs, t);
    CellEvidence {
        rows: rows.len(),
        aligned: rows.iter().filter(|row| row.aligned).count(),
    }
}

/// One printed row of a region, and whether it is set as cells on a shared gutter.
#[derive(Clone, Copy, Debug, PartialEq)]
struct CellRow {
    top: f32,
    bottom: f32,
    /// How far left and right its ink reaches.
    left: f32,
    right: f32,
    aligned: bool,
}

/// A region's printed rows, top to bottom, each with whether it is set as cells.
///
/// Rows are the runs grouped by the vertical band they share. A row's cells are its runs taken
/// left to right, split wherever the gap to the next run is wide enough to be a gutter rather
/// than a word space — `text.line_split_gap_em` of the size before it, the same test `words`
/// breaks a run by, so a run broken only at a change of style does not count as two cells. A
/// gutter is *shared* when another row has a gutter overlapping it in x: that holds for a
/// column set flush left, flush right or centred alike, because a table's columns keep one
/// strip of white between them on every row, and it does not hold for the stretched spaces of
/// justified prose, which fall wherever the line's words put them.
fn cell_rows(runs: &[&oc_model::text::Run], t: &Thresholds) -> Vec<CellRow> {
    let mut sorted: Vec<&oc_model::text::Run> = runs
        .iter()
        .copied()
        .filter(|run| !run.text.trim().is_empty())
        .collect();
    sorted.sort_by(|a, b| {
        centre_y(a.bbox)
            .total_cmp(&centre_y(b.bbox))
            .then(a.bbox.x0.total_cmp(&b.bbox.x0))
    });
    // A run is in the row whose band holds its centre.
    let mut rows: Vec<(f32, f32, Vec<&oc_model::text::Run>)> = Vec::new();
    for run in sorted {
        let centre = centre_y(run.bbox);
        match rows.last_mut() {
            Some((top, bottom, members)) if centre >= *top && centre <= *bottom => {
                *top = top.min(run.bbox.y0);
                *bottom = bottom.max(run.bbox.y1);
                members.push(run);
            }
            _ => rows.push((run.bbox.y0, run.bbox.y1, vec![run])),
        }
    }
    // Each row's gutters: the gaps between neighbouring runs too wide to be a word space.
    let gutters: Vec<Vec<(f32, f32)>> = rows
        .iter_mut()
        .map(|(_, _, members)| {
            members.sort_by(|a, b| a.bbox.x0.total_cmp(&b.bbox.x0));
            let mut gaps = Vec::new();
            // How far right the row's ink reaches so far, and the size of the run that took it
            // there: the em a gap after it is measured in.
            let mut reach: Option<(f32, f32)> = None;
            for run in members.iter() {
                match reach {
                    Some((right, size)) if run.bbox.x1 <= right => reach = Some((right, size)),
                    Some((right, size)) => {
                        if run.bbox.x0 - right >= t.text.line_split_gap_em as f32 * size {
                            gaps.push((right, run.bbox.x0));
                        }
                        reach = Some((run.bbox.x1, run.size_pt));
                    }
                    None => reach = Some((run.bbox.x1, run.size_pt)),
                }
            }
            gaps
        })
        .collect();
    rows.iter()
        .zip(gutters.iter())
        .enumerate()
        .map(|(index, ((top, bottom, members), gaps))| CellRow {
            top: *top,
            bottom: *bottom,
            left: members
                .iter()
                .map(|run| run.bbox.x0)
                .fold(f32::INFINITY, f32::min),
            right: members
                .iter()
                .map(|run| run.bbox.x1)
                .fold(f32::NEG_INFINITY, f32::max),
            aligned: gaps.iter().any(|gap| {
                gutters.iter().enumerate().any(|(other, theirs)| {
                    other != index
                        && theirs
                            .iter()
                            .any(|their| gap.1.min(their.1) > gap.0.max(their.0))
                })
            }),
        })
        .collect()
}

/// The multiset of the words in some strings, for the cell-text conservation check.
fn multiset(texts: impl Iterator<Item = String>) -> std::collections::BTreeMap<String, u32> {
    let mut counts = std::collections::BTreeMap::new();
    for text in texts {
        for word in text.split_whitespace() {
            let entry = counts.entry(word.to_owned()).or_default();
            *entry += 1u32;
        }
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ladder_places_a_value_in_its_band() {
        let ladder = [0.0f32, 10.0, 20.0, 30.0];
        assert_eq!(band_of(&ladder, 5.0), Some(0));
        assert_eq!(band_of(&ladder, 15.0), Some(1));
        assert_eq!(band_of(&ladder, 29.9), Some(2));
        assert_eq!(band_of(&ladder, -1.0), None);
        assert_eq!(band_of(&ladder, 31.0), None);
        assert_eq!(band_of(&[0.0], 0.0), None, "one rule bounds no band");
    }

    /// Two rules a hair apart are one cell edge. A filled rule has a thickness, so a cell
    /// boundary reaches the extractor as a band rather than a line.
    #[test]
    fn coordinates_within_the_snap_collapse_to_one() {
        assert_eq!(
            snapped([10.0f32, 10.5, 40.0, 41.0, 100.0].into_iter(), 3.0),
            vec![10.0, 40.0, 100.0]
        );
    }

    /// The gate onto the markup path: the cells must hold exactly the region's text.
    #[test]
    fn the_cell_multiset_check_counts_repeats() {
        let cells = multiset(["a".to_owned(), "b".to_owned(), "a".to_owned()].into_iter());
        let same = multiset(["a".to_owned(), "a".to_owned(), "b".to_owned()].into_iter());
        let dropped = multiset(["a".to_owned(), "b".to_owned()].into_iter());
        assert_eq!(cells, same, "order is not part of a multiset");
        assert_ne!(cells, dropped, "a dropped repeat must fail the check");
    }

    // ---------------------------------------------------------------------------------------
    // Rules that are not a table's. Every shape below is one a real book printed, and every one
    // came out as a `<table>` holding running text: a novel with no tables at all came out with
    // 74, most of them a chapter's number cut from its opener (2026-09-26).
    // ---------------------------------------------------------------------------------------

    use oc_core::thresholds::T;
    use oc_model::extract::{PageRef, VecId};
    use oc_model::layout::BlockKindHint;
    use oc_model::text::{Line, Run, RunId, TextProvenance};

    /// The size every synthetic line is set at, and the height of its box.
    const SIZE_PT: f32 = 10.0;
    /// How far apart two lines' tops are.
    const LEADING_PT: f32 = 13.0;
    const PAGE_HEIGHT_PT: f32 = 792.0;

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect { x0, y0, x1, y1 }
    }

    /// One run of `text` whose top is `y`, starting at `x0` and `width` wide.
    fn run(page: u32, text: &str, x0: f32, width: f32, y: f32) -> Run {
        Run {
            id: RunId(0),
            page: PageRef::new(page),
            text: text.to_owned(),
            bbox: rect(x0, y, x0 + width, y + SIZE_PT),
            baseline_y: y + SIZE_PT,
            font: oc_model::extract::FontId(0),
            size_pt: SIZE_PT,
            weight: 400,
            italic: false,
            superscript: false,
            subscript: false,
            provenance: TextProvenance::Pdf,
            glyph_range: (0, 0),
        }
    }

    /// A block whose lines are the given runs, one `Vec` per printed line.
    fn block(page: u32, order: u32, lines: Vec<Vec<Run>>) -> BlockView {
        let views: Vec<crate::view::LineView> = lines
            .into_iter()
            .map(|runs| {
                let bbox = runs
                    .iter()
                    .map(|run| run.bbox)
                    .reduce(union)
                    .unwrap_or(rect(0.0, 0.0, 0.0, 0.0));
                let text = runs
                    .iter()
                    .map(|run| run.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                crate::view::LineView {
                    line: Line {
                        runs: Vec::new(),
                        bbox,
                        baseline_y: bbox.y1,
                        ends_with_hyphen: false,
                        indent_pt: 0.0,
                        right_gap_pt: 0.0,
                    },
                    text,
                    runs,
                    glue: false,
                }
            })
            .collect();
        let bbox = views
            .iter()
            .map(crate::view::LineView::bbox)
            .reduce(union)
            .unwrap_or(rect(0.0, 0.0, 0.0, 0.0));
        let text = views
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        BlockView {
            id: BlockId::derive(page, bbox, &format!("{order}:{text}")),
            page,
            order,
            bbox,
            column: 0,
            kind_hint: BlockKindHint::Text,
            text,
            lines: views,
            column_width_pt: bbox.x1 - bbox.x0,
            space_above_pt: 0.0,
            page_height_pt: PAGE_HEIGHT_PT,
            para_starts: Vec::new(),
            continues: None,
        }
    }

    /// Lines of running text: one run each, the measure's full width, from `top` down.
    fn prose(page: u32, order: u32, x0: f32, x1: f32, top: f32, count: usize) -> BlockView {
        let lines = (0..count)
            .map(|index| {
                vec![run(
                    page,
                    "and so the sentence ran on across the whole of the measure",
                    x0,
                    x1 - x0,
                    top + LEADING_PT * index as f32,
                )]
            })
            .collect();
        block(page, order, lines)
    }

    /// A horizontal hairline.
    fn hrule(page: u32, x0: f32, x1: f32, y: f32) -> VectorRegion {
        VectorRegion {
            id: VecId(0),
            page: PageRef::new(page),
            bbox: rect(x0, y, x1, y + 0.5),
            path_count: 1,
            is_rule: true,
        }
    }

    /// A vertical hairline.
    fn vrule(page: u32, x: f32, y0: f32, y1: f32) -> VectorRegion {
        VectorRegion {
            id: VecId(0),
            page: PageRef::new(page),
            bbox: rect(x, y0, x + 0.5, y1),
            path_count: 1,
            is_rule: true,
        }
    }

    fn tables_in(vectors: &[VectorRegion], blocks: &[BlockView]) -> TableOutcome {
        extract_tables(vectors, 1, blocks, &Default::default(), 0, &T)
    }

    /// The novel's chapter opener: a centred word and the chapter's number under it, each
    /// underlined — the underline of a link, drawn as a hairline under the line's own ink. The
    /// two underlines overlap in x, so they bounded a "table" whose one cell was the number,
    /// and the chapter lost the number it is found by.
    #[test]
    fn an_underlined_chapter_number_is_not_a_table() {
        let word = block(0, 0, vec![vec![run(0, "CHAPTER", 280.0, 52.0, 183.0)]]);
        let number = block(0, 1, vec![vec![run(0, "10", 298.0, 16.0, 214.0)]]);
        let vectors = [hrule(0, 280.0, 332.0, 194.0), hrule(0, 298.0, 314.0, 225.5)];

        let outcome = tables_in(&vectors, &[word, number]);

        assert!(outcome.tables.is_empty(), "{:?}", outcome.tables);
        assert!(outcome.consumed.is_empty(), "{:?}", outcome.consumed);
    }

    /// A scanned page: the running head's rule across the top and the footnote separator near
    /// the foot overlap in x, so the whole page of prose between them was one "table".
    #[test]
    fn prose_between_a_head_rule_and_a_note_rule_is_not_a_table() {
        let page = prose(0, 0, 20.0, 290.0, 30.0, 34);
        let vectors = [hrule(0, 5.0, 303.0, 15.0), hrule(0, 5.0, 83.0, 490.0)];

        let outcome = tables_in(&vectors, &[page]);

        assert!(outcome.tables.is_empty(), "{:?}", outcome.tables);
        assert!(outcome.consumed.is_empty());
    }

    /// A sidebar: prose in a ruled box. Two vertical rules bound one column, and one column of
    /// lines is a box around text, not a table.
    #[test]
    fn prose_in_a_ruled_box_is_not_a_table() {
        let boxed = prose(0, 0, 80.0, 424.0, 110.0, 14);
        let vectors = [
            hrule(0, 72.0, 432.0, 100.0),
            hrule(0, 72.0, 432.0, 300.0),
            vrule(0, 72.0, 100.0, 300.5),
            vrule(0, 432.0, 100.0, 300.5),
        ];

        let outcome = tables_in(&vectors, &[boxed]);

        assert!(outcome.tables.is_empty(), "{:?}", outcome.tables);
    }

    /// Justified prose whose word spaces are stretched wide enough, on a few lines, to break a
    /// run — at the very same place on each, the worst case. A handful of lines that happen to
    /// share a gap is not a column structure when the region's other lines have none.
    #[test]
    fn justified_prose_with_a_few_wide_gaps_is_not_a_table() {
        let mut lines: Vec<Vec<Run>> = (0..20)
            .map(|index| {
                vec![run(
                    0,
                    "and so the sentence ran on across the whole of the measure",
                    72.0,
                    360.0,
                    110.0 + LEADING_PT * index as f32,
                )]
            })
            .collect();
        for index in [3usize, 9, 15] {
            let y = 110.0 + LEADING_PT * index as f32;
            lines[index] = vec![
                run(0, "a line whose spaces were", 72.0, 150.0, y),
                run(0, "stretched to fill it", 240.0, 192.0, y),
            ];
        }
        let page = block(0, 0, lines);
        let vectors = [hrule(0, 72.0, 432.0, 100.0), hrule(0, 72.0, 432.0, 380.0)];

        let outcome = tables_in(&vectors, &[page]);

        assert!(outcome.tables.is_empty(), "{:?}", outcome.tables);
    }

    /// An index set in two columns: every row sets two entries apart at one gutter.
    fn two_column_index(page: u32, top: f32, rows: usize) -> BlockView {
        let lines = (0..rows)
            .map(|row| {
                let y = top + LEADING_PT * row as f32;
                vec![
                    run(page, "an entry, 12", 72.0, 120.0, y),
                    run(page, "another entry, 40", 260.0, 150.0, y),
                ]
            })
            .collect();
        block(page, 0, lines)
    }

    /// The rule over a running foot, printed in the same place on nearly every page, and the
    /// rule that opens a chapter overlap in x: every page that had both bounded one "table"
    /// from the chapter's top to its foot — a contents page, an index in two columns, whose
    /// text even shows a column structure. A rule that recurs in one place across the book is
    /// the page's furniture, not a table's.
    #[test]
    fn a_rule_repeated_on_every_page_does_not_bound_a_table() {
        let pages = 5u32;
        let mut vectors: Vec<VectorRegion> = (0..pages)
            .map(|page| hrule(page, 72.0, 432.0, 607.0))
            .collect();
        vectors.push(hrule(0, 72.0, 432.0, 72.0));
        let index = two_column_index(0, 90.0, 30);

        let outcome = extract_tables(&vectors, pages, &[index], &Default::default(), 0, &T);

        assert!(outcome.tables.is_empty(), "{:?}", outcome.regions);
    }

    /// The guard for that: the same table printed in the same place on two pages of a longer
    /// book is still a table on each. Repeating on a few pages is not repeating on most.
    #[test]
    fn a_table_repeated_on_a_few_pages_is_still_found_on_each() {
        let pages = 20u32;
        let vectors: Vec<VectorRegion> = (0..2u32)
            .flat_map(|page| {
                [
                    hrule(page, 60.0, 440.0, 80.0),
                    hrule(page, 60.0, 440.0, 480.0),
                ]
            })
            .collect();
        let blocks = [two_column_index(0, 90.0, 10), two_column_index(1, 90.0, 10)];

        let outcome = extract_tables(&vectors, pages, &blocks, &Default::default(), 0, &T);

        assert_eq!(outcome.tables.len(), 2, "{:?}", outcome.regions);
    }

    /// The guard: a table ruled above, under its header and below, with no vertical rules —
    /// short cells, and every row setting them on the same column starts. It is still a table.
    #[test]
    fn a_borderless_table_with_repeated_columns_is_still_found() {
        let rows = [
            ["Stage", "Kind", "Budget"],
            ["text", "Budgeted", "0.005"],
            ["layout", "Conserving", "0.000"],
            ["structure", "Budgeted", "0.010"],
        ];
        let columns = [60.0f32, 160.0, 260.0];
        let lines = rows
            .iter()
            .enumerate()
            .map(|(row, cells)| {
                cells
                    .iter()
                    .zip(columns)
                    .map(|(cell, x0)| run(0, cell, x0, 50.0, 600.0 + LEADING_PT * row as f32))
                    .collect()
            })
            .collect();
        let table = block(0, 0, lines);
        let vectors = [
            hrule(0, 55.0, 345.0, 596.0),
            hrule(0, 55.0, 345.0, 611.5),
            hrule(0, 55.0, 345.0, 652.0),
        ];

        let outcome = tables_in(&vectors, std::slice::from_ref(&table));

        assert_eq!(outcome.tables.len(), 1, "{:?}", outcome.warnings);
        assert_eq!(outcome.consumed, vec![table.id]);
        let text = outcome.tables[0].cell_texts().join(" ");
        for cell in rows.iter().flatten() {
            assert!(text.contains(cell), "{cell:?} is missing from {text:?}");
        }
    }

    /// The other guard: a grid ruled into two columns is a table on its rules alone, whatever
    /// its cells hold — the vertical rules are the column structure.
    #[test]
    fn a_grid_ruled_into_columns_is_still_found() {
        let cells = block(
            0,
            0,
            vec![
                vec![
                    run(0, "Latency", 80.0, 60.0, 110.0),
                    run(0, "the time a request waits", 210.0, 210.0, 110.0),
                ],
                vec![
                    run(0, "Throughput", 80.0, 60.0, 130.0),
                    run(0, "requests handled a second", 210.0, 210.0, 130.0),
                ],
            ],
        );
        let vectors = [
            hrule(0, 72.0, 432.0, 104.0),
            hrule(0, 72.0, 432.0, 124.0),
            hrule(0, 72.0, 432.0, 144.0),
            vrule(0, 72.0, 104.0, 144.5),
            vrule(0, 200.0, 104.0, 144.5),
            vrule(0, 432.0, 104.0, 144.5),
        ];

        let outcome = tables_in(&vectors, &[cells]);

        assert_eq!(outcome.tables.len(), 1);
        assert!(
            outcome.regions[0].gridded,
            "a clean lattice is markup: {:?}",
            outcome.warnings
        );
    }

    // ---------------------------------------------------------------------------------------
    // Rules drawn in pieces. Real tables in technical books came out as paragraphs — a caption
    // followed by the cells run together, `"ModelMeanScore…"` — because no single long rule
    // bounded them: one book strokes a separate segment per column along every row boundary,
    // another strokes every cell's four sides as four separate lines, and all of them shade the
    // header row as one filled box per cell (2026-09-26).
    // ---------------------------------------------------------------------------------------

    /// A filled or stroked box that is not a rule: a table cell, a shaded header cell.
    fn cell_box(page: u32, x0: f32, y0: f32, x1: f32, y1: f32) -> VectorRegion {
        VectorRegion {
            id: VecId(0),
            page: PageRef::new(page),
            bbox: rect(x0, y0, x1, y1),
            path_count: 1,
            is_rule: false,
        }
    }

    /// One block holding a table's printed rows: each row a list of `(x0, text)` cells, one
    /// row per `LEADING_PT` from `top`.
    fn table_block(page: u32, order: u32, top: f32, rows: &[&[(f32, &str)]]) -> BlockView {
        let lines = rows
            .iter()
            .enumerate()
            .map(|(index, cells)| {
                let y = top + LEADING_PT * index as f32;
                cells
                    .iter()
                    .map(|(x0, text)| run(page, text, *x0, 6.0 * text.len() as f32, y))
                    .collect()
            })
            .collect();
        block(page, order, lines)
    }

    /// The horizontal rules of one page, as `(x0, x1, y)` rounded to a tenth of a point.
    fn horizontals(rules: &[Rule]) -> Vec<(f32, f32, f32)> {
        let tenth = |value: f32| (value * 10.0).round() / 10.0;
        let mut out: Vec<(f32, f32, f32)> = rules
            .iter()
            .filter(|rule| rule.is_horizontal())
            .map(|rule| {
                (
                    tenth(rule.bbox.x0),
                    tenth(rule.bbox.x1),
                    tenth(centre_y(rule.bbox)),
                )
            })
            .collect();
        out.sort_by(|a, b| a.2.total_cmp(&b.2).then(a.0.total_cmp(&b.0)));
        out
    }

    /// Pieces of one rule, meeting end to end at the same height — overlapping by a hair,
    /// or a hair apart — are one rule. A piece well apart along the same line is not part of
    /// it, and neither is a rule at another height.
    #[test]
    fn rule_pieces_meeting_end_to_end_join_into_one_rule() {
        let vectors = [
            hrule(0, 72.0, 214.5, 322.25),
            hrule(0, 213.8, 288.4, 322.25),
            hrule(0, 289.0, 432.0, 322.25),
            hrule(0, 450.0, 480.0, 322.25),
            hrule(0, 72.0, 432.0, 499.75),
            vrule(0, 72.0, 322.4, 336.2),
            vrule(0, 72.0, 336.0, 349.6),
            vrule(0, 72.0, 349.4, 362.9),
        ];

        let rules = assemble_rules(&vectors, &T);

        assert_eq!(
            horizontals(&rules),
            vec![
                (72.0, 432.0, 322.5),
                (450.0, 480.0, 322.5),
                (72.0, 432.0, 500.0)
            ]
        );
        let verticals: Vec<Rect> = rules
            .iter()
            .filter(|rule| !rule.is_horizontal())
            .map(|rule| rule.bbox)
            .collect();
        assert_eq!(verticals, vec![rect(72.0, 322.4, 72.5, 362.9)]);
    }

    /// A table whose every cell is stroked as four separate lines and whose header row is
    /// shaded as one filled box per cell: no rule on the page is longer than one cell, and
    /// the table is still a ruled grid, read into its rows and columns.
    #[test]
    fn a_table_ruled_in_pieces_is_a_grid() {
        let (left, middle, right) = (72.0f32, 214.0f32, 288.0f32);
        let edges = [309.0f32, 322.0, 335.0, 348.0, 361.0];
        let mut vectors = vec![
            cell_box(0, left, edges[0], middle, edges[1]),
            cell_box(0, middle, edges[0], right, edges[1]),
        ];
        for pair in edges[1..].windows(2) {
            for (x0, x1) in [(left, middle), (middle, right)] {
                vectors.push(hrule(0, x0 - 0.3, x1 + 0.3, pair[0]));
                vectors.push(vrule(0, x0, pair[0] - 0.2, pair[1] + 0.2));
                vectors.push(hrule(0, x0 - 0.3, x1 + 0.3, pair[1]));
                vectors.push(vrule(0, x1, pair[0] - 0.2, pair[1] + 0.2));
            }
        }
        let rows: [[&str; 2]; 4] = [
            ["Input", "Output"],
            ["<BOS>", "I"],
            ["<BOS>, I", "love"],
            ["<BOS>, I, love", "food"],
        ];
        let lines: Vec<Vec<(f32, &str)>> = rows
            .iter()
            .map(|[a, b]| vec![(left + 4.0, *a), (middle + 4.0, *b)])
            .collect();
        let slices: Vec<&[(f32, &str)]> = lines.iter().map(Vec::as_slice).collect();
        let table = table_block(0, 0, edges[0] + 1.5, &slices);

        let outcome = tables_in(&vectors, std::slice::from_ref(&table));

        assert_eq!(outcome.tables.len(), 1, "{:?}", outcome.warnings);
        assert!(outcome.regions[0].gridded, "{:?}", outcome.warnings);
        assert_eq!(
            (outcome.regions[0].rows, outcome.regions[0].columns),
            (4, 2)
        );
        let cells: Vec<Vec<String>> = outcome.tables[0]
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| oc_model::doc::spans_text(&cell.spans))
                    .collect()
            })
            .collect();
        assert_eq!(
            cells,
            rows.iter()
                .map(|row| row.iter().map(|cell| (*cell).to_owned()).collect())
                .collect::<Vec<Vec<String>>>()
        );
        assert_eq!(outcome.consumed, vec![table.id]);
    }

    /// A table drawn as one closed box per cell, abutting: the boxes' sides are its rules.
    #[test]
    fn a_grid_of_cell_boxes_is_a_table() {
        let columns = [60.0f32, 160.0, 260.0, 360.0];
        let edges = [100.0f32, 116.0, 132.0, 148.0];
        let mut vectors = Vec::new();
        for pair in edges.windows(2) {
            for x in columns.windows(2) {
                vectors.push(cell_box(0, x[0], pair[0], x[1], pair[1]));
            }
        }
        let rows: [[&str; 3]; 3] = [
            ["Model", "Size", "Score"],
            ["small", "1B", "41"],
            ["large", "70B", "68"],
        ];
        let lines: Vec<Vec<Run>> = rows
            .iter()
            .zip(edges)
            .map(|(cells, top)| {
                cells
                    .iter()
                    .zip(columns)
                    .map(|(cell, x0)| run(0, cell, x0 + 4.0, 30.0, top + 3.0))
                    .collect()
            })
            .collect();
        let table = block(0, 0, lines);

        let outcome = tables_in(&vectors, std::slice::from_ref(&table));

        assert_eq!(outcome.tables.len(), 1, "{:?}", outcome.warnings);
        let region = &outcome.regions[0];
        assert!(region.gridded, "{:?}", outcome.warnings);
        assert_eq!((region.rows, region.columns), (3, 3));
        assert_eq!(outcome.tables[0].cell_texts()[3], "small");
    }

    /// The header shaded one box per cell, the other rows ruled one piece per column, and no
    /// vertical rules at all: the rules bound one region from the header's top to the last
    /// rule, and the text's columns make it a table.
    #[test]
    fn a_shaded_header_and_rules_in_pieces_bound_a_table() {
        let columns = [(72.0f32, 150.0f32), (150.0, 271.0)];
        let mut vectors: Vec<VectorRegion> = columns
            .iter()
            .map(|(x0, x1)| cell_box(0, *x0, 157.5, *x1, 171.3))
            .collect();
        for y in [185.3f32, 199.5, 213.6] {
            for (x0, x1) in columns {
                vectors.push(hrule(0, x0 - 0.3, x1 + 0.3, y));
            }
        }
        let table = table_block(
            0,
            0,
            159.0,
            &[
                &[(76.0, "Topic"), (154.0, "Trade-off")],
                &[(76.0, "Design"), (154.0, "Simple against flexible")],
                &[(76.0, "Scale"), (154.0, "Cost against speed")],
                &[(76.0, "Data"), (154.0, "Fresh against cheap")],
            ],
        );

        let outcome = tables_in(&vectors, std::slice::from_ref(&table));

        assert_eq!(outcome.tables.len(), 1, "{:?}", outcome.regions);
        assert_eq!(outcome.consumed, vec![table.id]);
        assert!(
            outcome.regions[0].bbox.y0 <= 157.5,
            "the header row is inside the table: {:?}",
            outcome.regions[0].bbox
        );
        let text = outcome.tables[0].cell_texts().join(" ");
        for cell in ["Topic", "Trade-off", "Fresh against cheap"] {
            assert!(text.contains(cell), "{cell:?} is missing from {text:?}");
        }
    }

    /// A cell whose text wraps onto a second line is still one cell of the grid: the grid's
    /// words are the region's words, however many printed lines a cell took.
    #[test]
    fn a_cell_that_wraps_onto_two_lines_is_still_a_grid() {
        let vectors = [
            hrule(0, 72.0, 432.0, 104.0),
            hrule(0, 72.0, 432.0, 120.0),
            hrule(0, 72.0, 432.0, 150.0),
            vrule(0, 72.0, 104.0, 150.5),
            vrule(0, 150.0, 104.0, 150.5),
            vrule(0, 432.0, 104.0, 150.5),
        ];
        let table = block(
            0,
            0,
            vec![
                vec![
                    run(0, "Term", 76.0, 30.0, 106.0),
                    run(0, "Definition", 154.0, 60.0, 106.0),
                ],
                vec![
                    run(0, "Availability", 76.0, 60.0, 122.0),
                    run(0, "How much of the time the system", 154.0, 200.0, 122.0),
                ],
                vec![run(0, "must be available.", 154.0, 110.0, 135.0)],
            ],
        );

        let outcome = tables_in(&vectors, &[table]);

        assert_eq!(outcome.tables.len(), 1);
        assert!(outcome.regions[0].gridded, "{:?}", outcome.warnings);
        assert_eq!(
            outcome.tables[0].cell_texts(),
            vec![
                "Term",
                "Definition",
                "Availability",
                "How much of the time the system must be available."
            ]
        );
    }

    /// A table ruled in pieces with running text under it, and a footnote separator at the
    /// foot of the page: the separator lies within the table's width, so the two used to be
    /// one region, and the prose between them went into the table. The prose stays in the
    /// flow, and the table is still found.
    #[test]
    fn prose_under_a_table_is_not_taken_into_it() {
        let columns = [(72.0f32, 214.0f32), (214.0, 288.0)];
        let mut vectors = Vec::new();
        for y in [309.0f32, 322.0, 335.0, 348.0] {
            for (x0, x1) in columns {
                vectors.push(hrule(0, x0 - 0.3, x1 + 0.3, y));
            }
        }
        vectors.push(hrule(0, 72.0, 162.0, 527.0));
        let table = table_block(
            0,
            0,
            310.5,
            &[
                &[(76.0, "Input"), (218.0, "Output")],
                &[(76.0, "<BOS>"), (218.0, "I")],
                &[(76.0, "<BOS>, I"), (218.0, "love")],
            ],
        );
        let after = prose(0, 1, 72.0, 432.0, 360.0, 12);

        let outcome = tables_in(&vectors, &[table.clone(), after]);

        assert_eq!(outcome.tables.len(), 1, "{:?}", outcome.regions);
        assert_eq!(outcome.consumed, vec![table.id]);
        assert!(outcome.regions[0].bbox.y1 < 360.0, "{:?}", outcome.regions);
    }

    /// A row whose cells stand so close that `text` ran them into one run shows no gutter,
    /// and neither does a line of prose. One such row between two of a table's rules is not
    /// prose under the table: a band too short to repeat a column structure cannot refute one,
    /// and the table stays whole.
    #[test]
    fn a_row_run_together_does_not_split_its_table() {
        let columns = [(72.0f32, 150.0f32), (150.0, 271.0)];
        let mut vectors: Vec<VectorRegion> = columns
            .iter()
            .map(|(x0, x1)| cell_box(0, *x0, 100.0, *x1, 113.0))
            .collect();
        for y in [126.0f32, 139.0, 152.0] {
            for (x0, x1) in columns {
                vectors.push(hrule(0, x0 - 0.3, x1 + 0.3, y));
            }
        }
        let table = table_block(
            0,
            0,
            101.5,
            &[
                &[(76.0, "Topic"), (154.0, "Trade-off")],
                &[(76.0, "Design"), (154.0, "Simple against flexible")],
                &[(76.0, "Architectural extensibilityData access")],
                &[(76.0, "Data"), (154.0, "Fresh against cheap")],
            ],
        );

        let outcome = tables_in(&vectors, std::slice::from_ref(&table));

        assert_eq!(outcome.tables.len(), 1, "{:?}", outcome.regions);
        let bbox = outcome.regions[0].bbox;
        assert!(bbox.y0 <= 100.0 && bbox.y1 >= 152.0, "{bbox:?}");
        assert_eq!(outcome.consumed, vec![table.id]);
    }

    /// A ruled grid whose rows `layout` set in one block with the paragraph under the table,
    /// leaving inside it only a lone block — a note's superscript mark. The rows stay in the
    /// flow with the paragraph, as blocks that reach outside a table always do, and the mark
    /// is not a one-cell table of its own.
    #[test]
    fn a_region_whose_text_is_mostly_in_blocks_reaching_outside_is_not_a_table() {
        let vectors = [
            hrule(0, 72.0, 432.0, 100.0),
            hrule(0, 72.0, 432.0, 116.0),
            hrule(0, 72.0, 432.0, 132.0),
            hrule(0, 72.0, 432.0, 148.0),
            vrule(0, 72.0, 100.0, 148.5),
            vrule(0, 200.0, 100.0, 148.5),
            vrule(0, 432.0, 100.0, 148.5),
        ];
        let rows_and_prose = block(
            0,
            0,
            vec![
                vec![
                    run(0, "Category", 76.0, 60.0, 103.0),
                    run(0, "Building with models", 204.0, 120.0, 103.0),
                ],
                vec![
                    run(0, "Modeling", 76.0, 60.0, 119.0),
                    run(0, "A nice-to-have", 204.0, 90.0, 119.0),
                ],
                vec![
                    run(0, "Inference", 76.0, 60.0, 135.0),
                    run(0, "Even more important", 204.0, 110.0, 135.0),
                ],
                vec![run(
                    0,
                    "Many people would dispute this claim, and so the sentence ran on",
                    72.0,
                    360.0,
                    152.0,
                )],
            ],
        );
        let mark = block(0, 1, vec![vec![run(0, "a", 300.0, 4.0, 118.0)]]);

        let outcome = tables_in(&vectors, &[rows_and_prose, mark]);

        assert!(outcome.tables.is_empty(), "{:?}", outcome.tables);
        assert!(outcome.consumed.is_empty(), "{:?}", outcome.consumed);
    }

    /// The guard for that: a table whose last row `layout` ran into the paragraph under it is
    /// still found from the rows that are wholly inside it. The row stays in the flow.
    #[test]
    fn a_table_with_one_row_run_into_the_text_under_it_is_still_found() {
        let vectors = [
            hrule(0, 72.0, 432.0, 100.0),
            hrule(0, 72.0, 432.0, 116.0),
            hrule(0, 72.0, 432.0, 132.0),
            hrule(0, 72.0, 432.0, 148.0),
            hrule(0, 72.0, 432.0, 164.0),
            vrule(0, 72.0, 100.0, 164.5),
            vrule(0, 200.0, 100.0, 164.5),
            vrule(0, 432.0, 100.0, 164.5),
        ];
        let rows = block(
            0,
            0,
            vec![
                vec![
                    run(0, "Category", 76.0, 60.0, 103.0),
                    run(0, "Building with models", 204.0, 120.0, 103.0),
                ],
                vec![
                    run(0, "Modeling", 76.0, 60.0, 119.0),
                    run(0, "A nice-to-have", 204.0, 90.0, 119.0),
                ],
                vec![
                    run(0, "Datasets", 76.0, 60.0, 135.0),
                    run(0, "More about deduplication", 204.0, 150.0, 135.0),
                ],
            ],
        );
        let last_and_prose = block(
            0,
            1,
            vec![
                vec![
                    run(0, "Inference", 76.0, 60.0, 151.0),
                    run(0, "More", 204.0, 30.0, 151.0),
                ],
                vec![run(0, "The sentence after", 72.0, 120.0, 168.0)],
            ],
        );

        let outcome = tables_in(&vectors, &[rows.clone(), last_and_prose]);

        assert_eq!(outcome.tables.len(), 1, "{:?}", outcome.regions);
        assert_eq!(outcome.consumed, vec![rows.id]);
    }

    /// Two tables ruled in pieces on one page, their columns on the same line, with a paragraph
    /// and the second table's caption between them — and the second table's first row set
    /// above its first rule, so the band between the two tables held one row set in their
    /// columns. The paragraph and the caption stay in the flow.
    #[test]
    fn prose_and_a_caption_between_two_tables_stay_in_the_flow() {
        let columns = [(72.0f32, 150.0f32), (150.0, 250.0)];
        let mut vectors = Vec::new();
        for y in [100.0f32, 113.0, 126.0, 139.0, 240.0, 253.0, 266.0] {
            for (x0, x1) in columns {
                vectors.push(hrule(0, x0 - 0.3, x1 + 0.3, y));
            }
        }
        let first = table_block(
            0,
            0,
            101.5,
            &[
                &[(76.0, "Instances:"), (154.0, "5")],
                &[(76.0, "Cache size:"), (154.0, "50,000 rows")],
                &[(76.0, "Latency:"), (154.0, "1 millisecond")],
            ],
        );
        let between = prose(0, 1, 72.0, 432.0, 150.0, 5);
        let caption = block(
            0,
            2,
            vec![vec![run(
                0,
                "Table 9-9. The impact of fewer instances",
                72.0,
                240.0,
                215.0,
            )]],
        );
        let heading_row = table_block(
            0,
            3,
            227.0,
            &[&[(76.0, "Updates:"), (154.0, "20 a second")]],
        );
        let second = table_block(
            0,
            4,
            241.5,
            &[
                &[(76.0, "Instances:"), (154.0, "2")],
                &[(76.0, "Cache size:"), (154.0, "50,000 rows")],
            ],
        );

        let outcome = tables_in(
            &vectors,
            &[
                first.clone(),
                between.clone(),
                caption.clone(),
                heading_row,
                second,
            ],
        );

        assert!(
            !outcome.consumed.contains(&between.id) && !outcome.consumed.contains(&caption.id),
            "{:?}",
            outcome.regions
        );
        assert!(
            outcome.consumed.contains(&first.id),
            "{:?}",
            outcome.regions
        );
    }

    /// The guard, with the rules in pieces: prose between a running head's rule drawn in
    /// three pieces and a note's rule is still not a table.
    #[test]
    fn prose_between_rules_drawn_in_pieces_is_not_a_table() {
        let page = prose(0, 0, 20.0, 290.0, 30.0, 34);
        let vectors = [
            hrule(0, 5.0, 105.2, 15.0),
            hrule(0, 105.0, 205.2, 15.0),
            hrule(0, 205.0, 303.0, 15.0),
            hrule(0, 5.0, 83.0, 490.0),
        ];

        let outcome = tables_in(&vectors, &[page]);

        assert!(outcome.tables.is_empty(), "{:?}", outcome.tables);
        assert!(outcome.consumed.is_empty());
    }

    /// A lone shaded box — a code listing's background, a sidebar's — is a frame, not a grid
    /// of cells, even when the lines inside it line up in columns.
    #[test]
    fn a_lone_shaded_box_is_not_a_table() {
        let listing = two_column_index(0, 110.0, 12);
        let vectors = [cell_box(0, 66.0, 104.0, 438.0, 270.0)];

        let outcome = tables_in(&vectors, &[listing]);

        assert!(outcome.tables.is_empty(), "{:?}", outcome.regions);
    }

    /// A diagram's boxes set apart from each other, each with its label, are not cells: cells
    /// abut.
    #[test]
    fn boxes_set_apart_are_not_cells() {
        let mut vectors = Vec::new();
        let mut lines = Vec::new();
        for y in [100.0f32, 160.0] {
            let mut labels = Vec::new();
            for x in [72.0f32, 200.0, 328.0] {
                vectors.push(cell_box(0, x, y, x + 100.0, y + 30.0));
                labels.push(run(0, "Service", x + 20.0, 42.0, y + 10.0));
            }
            lines.push(labels);
        }
        let diagram = block(0, 0, lines);

        let outcome = tables_in(&vectors, &[diagram]);

        assert!(outcome.tables.is_empty(), "{:?}", outcome.regions);
    }

    /// A grid of abutting boxes with nothing printed in it — a drawing's tiles — has no data
    /// to read and is not a table.
    #[test]
    fn a_grid_with_no_text_is_not_a_table() {
        let mut vectors = Vec::new();
        for y in [100.0f32, 110.0, 120.0] {
            for x in [100.0f32, 110.0, 120.0] {
                vectors.push(cell_box(0, x, y, x + 10.0, y + 10.0));
            }
        }

        let outcome = tables_in(&vectors, &[]);

        assert!(outcome.tables.is_empty(), "{:?}", outcome.regions);
    }
}

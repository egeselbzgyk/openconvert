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
/// `first_fallback_image` is where fallback image ids start: a rasterised table is an image
/// the document did not contain, so it cannot reuse an `ImageId` that names one that it did.
pub fn extract_tables(
    vectors: &[VectorRegion],
    blocks: &[BlockView],
    first_fallback_image: u32,
    t: &Thresholds,
) -> TableOutcome {
    let mut outcome = TableOutcome {
        tables: Vec::new(),
        regions: Vec::new(),
        consumed: Vec::new(),
        warnings: Vec::new(),
    };
    let mut pages: Vec<u32> = vectors.iter().map(|rule| rule.page.index).collect();
    pages.sort_unstable();
    pages.dedup();

    for page in pages {
        for region in regions_on(vectors, page, t) {
            let id = TableId(u32::try_from(outcome.tables.len()).unwrap_or(u32::MAX));
            let (table, gridded) = build_table(
                id,
                &region,
                blocks,
                page,
                first_fallback_image.saturating_add(id.0),
                t,
                &mut outcome.warnings,
            );
            outcome.consumed.extend(
                blocks
                    .iter()
                    .filter(|block| block.page == page && inside(block.bbox, region.bbox))
                    .map(|block| block.id),
            );
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

/// Group a page's rules into candidate tables.
fn regions_on(vectors: &[VectorRegion], page: u32, t: &Thresholds) -> Vec<Lattice> {
    let on_page: Vec<&VectorRegion> = vectors
        .iter()
        .filter(|rule| rule.is_rule && rule.page.index == page)
        .collect();
    let horizontals: Vec<&&VectorRegion> =
        on_page.iter().filter(|rule| rule.is_horizontal()).collect();
    let verticals: Vec<&&VectorRegion> = on_page
        .iter()
        .filter(|rule| !rule.is_horizontal())
        .collect();

    let minimum = usize::try_from(t.table.min_row_rules).unwrap_or(2);
    let snap = t.table.grid_snap_pt as f32;

    // Horizontals that overlap in x belong to one table. A greedy sweep down the page: each
    // rule joins the open group when it overlaps it, and starts a new one otherwise.
    let mut sorted: Vec<&&VectorRegion> = horizontals.clone();
    sorted.sort_by(|a, b| a.bbox.y0.total_cmp(&b.bbox.y0));

    let mut groups: Vec<Vec<&&VectorRegion>> = Vec::new();
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

    groups
        .into_iter()
        .filter(|group| group.len() >= minimum)
        .map(|group| {
            let bbox = group
                .iter()
                .map(|rule| rule.bbox)
                .reduce(union)
                .unwrap_or(Rect {
                    x0: 0.0,
                    y0: 0.0,
                    x1: 0.0,
                    y1: 0.0,
                });
            let rows = snapped(group.iter().map(|rule| centre_y(rule.bbox)), snap);
            // A vertical belongs to this table when it lies inside the horizontals' box and
            // spans most of their height. A vertical that crosses only one row is a cell
            // divider in a merged row, and v1 does not read merges.
            let height = bbox.y1 - bbox.y0;
            let columns = snapped(
                verticals
                    .iter()
                    .filter(|rule| {
                        rule.bbox.x0 >= bbox.x0 - snap
                            && rule.bbox.x1 <= bbox.x1 + snap
                            && rule.bbox.y0 <= bbox.y0 + snap
                            && rule.bbox.y1 >= bbox.y1 - snap
                            && height > 0.0
                    })
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

/// Build one table, as a grid if the grid validates and as the image fallback otherwise.
fn build_table(
    id: TableId,
    lattice: &Lattice,
    blocks: &[BlockView],
    page: u32,
    fallback_image: u32,
    t: &Thresholds,
    warnings: &mut Vec<Warning>,
) -> (Table, bool) {
    // Everything printed inside the table's box, in reading order.
    //
    // *Runs*, not lines. A table row is one baseline, so `text` assembles its cells into a
    // single line — `"StageKindBudgetReason"` — and a grid filled from lines puts the whole
    // row in whichever cell its midpoint happens to fall in. The cells are separate runs,
    // because `words` breaks a run at a gap of `text.line_split_gap_em` and a column gutter
    // is many times that.
    let source: Vec<String> = runs_inside(blocks, page, lattice.bbox);

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
        return (
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
        );
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
    (
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
    )
}

/// Every run printed inside a box, in reading order, non-empty.
fn runs_inside(blocks: &[BlockView], page: u32, bbox: Rect) -> Vec<String> {
    blocks
        .iter()
        .filter(|block| block.page == page)
        .flat_map(BlockView::runs)
        .filter(|run| inside(run.bbox, bbox))
        .map(|run| run.text.trim().to_owned())
        .filter(|text| !text.is_empty())
        .collect()
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

/// A multiset of strings, for the cell-text conservation check.
fn multiset(texts: impl Iterator<Item = String>) -> std::collections::BTreeMap<String, u32> {
    let mut counts = std::collections::BTreeMap::new();
    for text in texts {
        let text = text.trim().to_owned();
        if text.is_empty() {
            continue;
        }
        let entry = counts.entry(text).or_default();
        *entry += 1u32;
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
}

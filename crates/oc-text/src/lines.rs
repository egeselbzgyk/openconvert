//! Line assembly: clustering things that sit on the same baseline (PIPELINE §4 step 1).
//!
//! The rule is one sentence long and the reason for its shape is the whole of it: cluster by
//! baseline `y` with a tolerance of `0.3 × font size`, **a fraction of size rather than an
//! absolute**, because superscripts pull the baseline and a fixed tolerance either splits
//! every footnote marker onto its own line or merges 8 pt notes into 18 pt headings.
//!
//! And the fraction is taken of the *larger* of the two sizes being compared. A 7 pt
//! superscript raised 3 pt above a 12 pt body is 3 pt from its own line's baseline: against
//! `0.3 × 7 = 2.1` it does not belong, against `0.3 × 12 = 3.6` it does, and it does.
//!
//! Columns are not this stage's business. Two columns printed at the same height cluster into
//! one line here, and `layout` splits them in Phase 3 — the order is deliberate (R2 §D.3).

use oc_core::thresholds::Thresholds;
use oc_model::geom::Rect;
use oc_model::text::{Line, Run, RunId};

/// The hyphens a line may end on, any of which means a word *might* be broken across it.
///
/// U+00AD cannot appear — `N` stripped it — but is listed because the set is the definition
/// of "ends with a hyphen", not an inventory of what survived.
const HYPHENS: [char; 4] = ['\u{002D}', '\u{2010}', '\u{2011}', '\u{00AD}'];

/// Group indices of `(baseline_y, size_pt)` items into lines, top to bottom.
///
/// The returned clusters are in reading order down the page and each cluster's indices are in
/// the order the items were given. Exposed because both glyph assembly and run assembly need
/// exactly this and must agree: a superscript that joined a line of glyphs and then failed to
/// join the same line of runs would be a line that exists in one representation and not the
/// other.
pub fn cluster_baselines(items: &[(f32, f32)], t: &Thresholds) -> Vec<Vec<u32>> {
    let ratio = t.text.line_baseline_tolerance_ratio as f32;

    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|a, b| {
        items[*a]
            .0
            .partial_cmp(&items[*b].0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(b))
    });

    let mut clusters: Vec<Cluster> = Vec::new();
    for index in order {
        let (baseline, size) = items[index];
        match clusters.last_mut() {
            // The anchor is the baseline of the largest item so far, and the tolerance is
            // taken of the larger size, so a small raised glyph is measured against the body
            // it belongs to rather than against itself.
            Some(cluster)
                if (baseline - cluster.anchor).abs() <= ratio * cluster.max_size.max(size) =>
            {
                if size > cluster.max_size {
                    cluster.max_size = size;
                    cluster.anchor = baseline;
                }
                cluster.members.push(index);
            }
            _ => clusters.push(Cluster {
                anchor: baseline,
                max_size: size,
                members: vec![index],
            }),
        }
    }

    clusters
        .into_iter()
        .map(|cluster| {
            let mut members = cluster.members;
            members.sort_unstable();
            members
                .into_iter()
                .map(|index| u32::try_from(index).unwrap_or(u32::MAX))
                .collect()
        })
        .collect()
}

struct Cluster {
    /// The baseline of the largest item in the cluster: what everything else is measured
    /// against.
    anchor: f32,
    max_size: f32,
    members: Vec<usize>,
}

/// Group runs into lines.
///
/// `indent_pt` and `right_gap_pt` are measured against the extent of *all* the runs given,
/// which on a single-column page is the text block. On a multi-column page it is the span of
/// both columns and therefore wrong, which is one more thing Phase 3 recomputes once columns
/// exist; it is recorded now because the paragraph rules that consume it are written against
/// the field, not against the stage that filled it in.
pub fn assemble_lines(runs: &[Run], t: &Thresholds) -> Vec<Line> {
    if runs.is_empty() {
        return Vec::new();
    }

    let items: Vec<(f32, f32)> = runs
        .iter()
        .map(|run| (run.baseline_y, run.size_pt))
        .collect();

    let left = runs.iter().map(|run| run.bbox.x0).fold(f32::MAX, f32::min);
    let right = runs.iter().map(|run| run.bbox.x1).fold(f32::MIN, f32::max);

    cluster_baselines(&items, t)
        .into_iter()
        .map(|members| {
            let members: Vec<&Run> = members
                .iter()
                .filter_map(|index| runs.get(*index as usize))
                .collect();
            let bbox = members
                .iter()
                .map(|run| run.bbox)
                .reduce(union)
                .unwrap_or(Rect {
                    x0: 0.0,
                    y0: 0.0,
                    x1: 0.0,
                    y1: 0.0,
                });
            // The baseline of the largest run: a line's baseline is its body's, not its
            // footnote marker's.
            let baseline_y = members
                .iter()
                .max_by(|a, b| {
                    a.size_pt
                        .partial_cmp(&b.size_pt)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|run| run.baseline_y)
                .unwrap_or_default();
            let ends_with_hyphen = members
                .last()
                .and_then(|run| run.text.trim_end().chars().next_back())
                .is_some_and(|ch| HYPHENS.contains(&ch));

            Line {
                runs: members.iter().map(|run| run.id).collect::<Vec<RunId>>(),
                bbox,
                baseline_y,
                ends_with_hyphen,
                indent_pt: bbox.x0 - left,
                right_gap_pt: right - bbox.x1,
            }
        })
        .collect()
}

fn union(a: Rect, b: Rect) -> Rect {
    Rect {
        x0: a.x0.min(b.x0),
        y0: a.y0.min(b.y0),
        x1: a.x1.max(b.x1),
        y1: a.y1.max(b.y1),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn a_superscript_joins_the_line_it_is_raised_from() {
    // 12 pt body at y = 100, 7 pt marker at y = 97. One line.
    let clusters = cluster_baselines(
        &[(100.0, 12.0), (97.0, 7.0), (100.0, 12.0)],
        &oc_core::thresholds::T,
    );
    assert_eq!(clusters, vec![vec![0, 1, 2]]);
}

#[test]
fn a_heading_does_not_join_the_body_below_it() {
    let clusters = cluster_baselines(
        &[(70.0, 18.0), (100.0, 12.0), (97.0, 7.0)],
        &oc_core::thresholds::T,
    );
    assert_eq!(clusters, vec![vec![0], vec![1, 2]]);
}

#[test]
fn lines_come_back_top_to_bottom_whatever_order_they_arrived_in() {
    // The backend's glyph order is not reading order (Phase 1 item 1.3), so the clusterer is
    // handed unsorted input on purpose.
    let clusters = cluster_baselines(
        &[(300.0, 12.0), (100.0, 12.0), (200.0, 12.0)],
        &oc_core::thresholds::T,
    );
    assert_eq!(clusters, vec![vec![1], vec![2], vec![0]]);
}

#[test]
fn nothing_in_nothing_out() {
    assert!(cluster_baselines(&[], &oc_core::thresholds::T).is_empty());
    assert!(assemble_lines(&[], &oc_core::thresholds::T).is_empty());
}

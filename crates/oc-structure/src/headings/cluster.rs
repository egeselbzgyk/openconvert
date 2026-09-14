//! Style clustering: the histogram that finds the body font and, with it, the candidates
//! for everything set larger (PIPELINE §8.2).
//!
//! The whole method rests on one observation: a book is typographically consistent, so the
//! *mode* of its character-weighted style histogram is its body text, and a heading is
//! something set differently from the mode. That is stronger than any absolute size rule —
//! a 10 pt book and a 14 pt large-print edition have different body sizes and the same
//! structure — and it survives font substitution, which absolute sizes do not (R1 §A.11 #23).
//!
//! Sizes are quantised to `headings.size_quantum_pt` before clustering rather than compared
//! exactly, for the same reason: a non-embedded face substituted by the host reports sizes
//! that differ in the second decimal between machines, and a cluster set that differs
//! between machines is not a cluster set.
//!
//! **Calibrate expectations.** GROBID reaches 76.43 % F1 on section titles and DocLayNet's
//! `Title` class has inter-annotator agreement of 60–72 % (R2 §B.5, R10 §6.7). Around a
//! quarter error is the ceiling for deterministic *and* learned methods alike. What this
//! module is for is not beating that; it is producing a scaffolding honest enough that the
//! outline and TOC fast paths, which do score ≥ 0.9, have something to bind to.

use std::collections::BTreeMap;

use oc_core::thresholds::Thresholds;
use oc_model::confidence::{Confidence, Signal};
use oc_model::doc::{Severity, Warning};
use oc_model::extract::FontInfo;
use oc_model::ids::ClusterId;
use oc_model::text::Run;
use serde::Serialize;

/// The style inventory is unusable: too many clusters, or no cluster holds enough of the
/// book to be its body text (IMPLEMENTATION_PLAN Phase 4 detail 3, RT A8.3).
pub const W_STYLE_INVENTORY_INVALID: &str = "W_STYLE_INVENTORY_INVALID";

/// One style the document sets text in, and what the document does with it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StyleCluster {
    pub id: ClusterId,
    /// Quantised to `headings.size_quantum_pt`. Comparisons between clusters are exact
    /// because of it.
    pub size_pt: f32,
    /// The heaviest weight seen in the cluster. The cluster *key* carries only the bold bit
    /// (PDF producers report 400 and 700 and little between), but the weight itself is worth
    /// reporting.
    pub weight: u16,
    pub italic: bool,
    pub family_key: String,
    /// Non-whitespace characters: the same quantity the conservation law counts.
    pub char_count: u64,
    /// The size's z-score against the body mode, over the character-weighted size
    /// distribution. Zero when the book sets everything at one size — no dispersion, no
    /// z-score, and inventing a large number there would be a false precision.
    pub size_z: f32,
    /// Of the pages this cluster appears on, the share on which it holds the first run in
    /// reading order. A chapter heading scores 1.0; body text scores whatever share of pages
    /// open without a heading.
    pub starts_page_ratio: f32,
    /// The share of this cluster's runs centred within the page's text extent.
    pub centered_ratio: f32,
    /// Up to `inventory.max_examples` of the cluster's text, in document order — what a
    /// report shows a user and what ARCHITECTURE §9.6 task 2 would show a model.
    pub examples: Vec<String>,
}

impl StyleCluster {
    /// Whether the cluster is set bold, by `headings.bold_weight_min`.
    pub fn is_bold(&self, t: &Thresholds) -> bool {
        i64::from(self.weight) >= t.headings.bold_weight_min
    }

    /// This cluster's share of the document's characters.
    pub fn char_share(&self, total: u64) -> f32 {
        if total == 0 {
            return 0.0;
        }
        (self.char_count as f64 / total as f64) as f32
    }
}

/// Every style the document sets text in, ranked, with the body identified.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StyleInventory {
    /// Ranked: largest size first, bold before regular at equal size, then italic, then
    /// family. `ClusterId` is the rank, so `ClusterId(0)` is the largest style in the book.
    pub clusters: Vec<StyleCluster>,
    /// The mode — the cluster holding the most characters. `None` only for a document with
    /// no text at all.
    pub body: Option<ClusterId>,
    pub total_chars: u64,
    /// Whether the inventory is usable as scaffolding at all (detail 3).
    pub valid: bool,
    /// Whether an LLM escalation may ever be attempted for this book's heading roles.
    /// Once false it is false for the whole book: an invalid inventory is not a hard case a
    /// model can rescue, it is a book whose typography carries no structure to read.
    pub escalation_allowed: bool,
    pub warnings: Vec<Warning>,
    pub confidence: Confidence,
}

impl StyleInventory {
    pub fn body_cluster(&self) -> Option<&StyleCluster> {
        let body = self.body?;
        self.clusters.iter().find(|cluster| cluster.id == body)
    }

    /// The body size, or zero for a document with no text.
    pub fn body_size_pt(&self) -> f32 {
        self.body_cluster().map_or(0.0, |cluster| cluster.size_pt)
    }

    /// The clusters that could be headings (PIPELINE §8.2, detail 2).
    ///
    /// Set larger than body, or bold at body's size; and holding less than
    /// `headings.candidate_max_char_share` of the book. The share condition is what keeps a
    /// bold-set-throughout children's book from declaring its whole text a heading.
    ///
    /// Returned in rank order, which is the order `assign_levels` reads as h1…h6.
    pub fn candidates(&self, t: &Thresholds) -> Vec<ClusterId> {
        let Some(body) = self.body_cluster() else {
            return Vec::new();
        };
        let body_bold = body.is_bold(t);
        self.clusters
            .iter()
            .filter(|cluster| cluster.id != body.id)
            .filter(|cluster| {
                cluster.size_pt > body.size_pt
                    || (cluster.size_pt == body.size_pt && cluster.is_bold(t) && !body_bold)
            })
            .filter(|cluster| {
                f64::from(cluster.char_share(self.total_chars))
                    < t.headings.candidate_max_char_share
            })
            .map(|cluster| cluster.id)
            .collect()
    }
}

/// The tuple a run is clustered on (PIPELINE §8.2).
///
/// The size is held in quanta rather than points so that it is an integer and the key is
/// `Ord` — which makes the histogram a `BTreeMap` and the iteration order deterministic,
/// as every output path must be.
///
/// Alignment is in PIPELINE's tuple and is not here: alignment is a property of a *line*
/// within its block, and this function sees runs, which do not know their block. It is
/// recovered as `centered_ratio`, a per-cluster signal, instead of as part of the key —
/// which is better anyway, because a heading that happens to be flush left and one that is
/// centred are one style used two ways, not two styles.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct StyleKey {
    /// Negated so that `Ord` puts the largest size first: the rank is the id.
    minus_size_quanta: i64,
    /// Bold before regular, hence negated as well.
    not_bold: bool,
    italic: bool,
    family_key: String,
}

/// What is accumulated for one key while the histogram is built.
#[derive(Default)]
struct Accumulator {
    char_count: u64,
    weight: u16,
    /// Character-weighted, so the mean and variance are over characters rather than runs.
    size_sum: f64,
    centered: u64,
    runs: u64,
    pages: std::collections::BTreeSet<u32>,
    pages_started: std::collections::BTreeSet<u32>,
    examples: Vec<String>,
}

/// Cluster a document's runs by style (PIPELINE §8.2).
///
/// `runs` are the runs that survive into the body flow — `furniture` has already taken the
/// running heads and page numbers out. Clustering before that removal would put an 8 pt
/// running head in the inventory as a style of its own on every page of the book, and the
/// validity gate counts clusters.
pub fn cluster_styles(runs: &[Run], fonts: &[FontInfo], t: &Thresholds) -> StyleInventory {
    let extents = page_text_extents(runs);
    let first_runs = page_first_runs(runs);
    let quantum = t.headings.size_quantum_pt as f32;
    let max_examples = usize::try_from(t.inventory.max_examples).unwrap_or_default();

    let mut histogram: BTreeMap<StyleKey, Accumulator> = BTreeMap::new();
    for (index, run) in runs.iter().enumerate() {
        let chars = u64::try_from(run.text.chars().filter(|c| !c.is_whitespace()).count())
            .unwrap_or_default();
        if chars == 0 {
            continue;
        }
        let family_key = fonts
            .get(usize::from(run.font.0))
            .map(|font| font.family_key.clone())
            .unwrap_or_default();
        let size = quantise(run.size_pt, quantum);
        let key = StyleKey {
            minus_size_quanta: -quanta(size, quantum),
            not_bold: i64::from(run.weight) < t.headings.bold_weight_min,
            italic: run.italic,
            family_key,
        };

        let entry = histogram.entry(key).or_default();
        entry.char_count = entry.char_count.saturating_add(chars);
        entry.weight = entry.weight.max(run.weight);
        entry.size_sum += f64::from(size) * chars as f64;
        entry.runs = entry.runs.saturating_add(1);
        entry.pages.insert(run.page.index);
        if is_centered(run, &extents, t) {
            entry.centered = entry.centered.saturating_add(1);
        }
        if first_runs.get(&run.page.index) == Some(&index) {
            entry.pages_started.insert(run.page.index);
        }
        if entry.examples.len() < max_examples {
            let text = run.text.trim();
            if !text.is_empty() {
                entry.examples.push(text.to_owned());
            }
        }
    }

    let total_chars: u64 = histogram
        .values()
        .map(|entry| entry.char_count)
        .fold(0u64, u64::saturating_add);

    let mut clusters: Vec<StyleCluster> = histogram
        .into_iter()
        .enumerate()
        .map(|(rank, (key, entry))| StyleCluster {
            id: ClusterId(u32::try_from(rank).unwrap_or(u32::MAX)),
            size_pt: dequantise(-key.minus_size_quanta, quantum),
            weight: entry.weight,
            italic: key.italic,
            family_key: key.family_key,
            char_count: entry.char_count,
            // Filled below: a z-score needs the whole distribution.
            size_z: 0.0,
            starts_page_ratio: ratio(
                entry.pages_started.len() as u64,
                entry.pages.len().max(1) as u64,
            ),
            centered_ratio: ratio(entry.centered, entry.runs.max(1)),
            examples: entry.examples,
        })
        .collect();

    let body = clusters
        .iter()
        // `max_by_key` returns the *last* maximum; a book with two styles holding exactly the
        // same number of characters must pick the same one on every machine, and the smaller
        // rank is the larger style, which is the less likely body. Reversed so ties go to the
        // later — that is, smaller — style, which is the one a book sets its text in.
        .max_by_key(|cluster| (cluster.char_count, cluster.id.0))
        .map(|cluster| cluster.id);

    let body_size = body
        .and_then(|id| clusters.iter().find(|cluster| cluster.id == id))
        .map(|cluster| f64::from(cluster.size_pt));
    let sigma = size_sigma(&clusters, total_chars);
    if let (Some(body_size), true) = (body_size, sigma > 0.0) {
        for cluster in &mut clusters {
            cluster.size_z = ((f64::from(cluster.size_pt) - body_size) / sigma) as f32;
        }
    }

    let body_share = body
        .and_then(|id| clusters.iter().find(|cluster| cluster.id == id))
        .map_or(0.0, |cluster| cluster.char_share(total_chars));
    let too_many = i64::try_from(clusters.len()).unwrap_or(i64::MAX) > t.inventory.max_clusters;
    let too_thin = f64::from(body_share) < t.inventory.min_body_char_share;
    let valid = !too_many && !too_thin;

    let mut warnings = Vec::new();
    if !valid {
        warnings.push(
            Warning::new(W_STYLE_INVENTORY_INVALID, Severity::Warn)
                .with_arg("clusters", clusters.len().to_string())
                .with_arg("body_char_share", format!("{body_share:.3}")),
        );
    }

    let signals = vec![
        Signal::new("cluster_count", clusters.len() as f32),
        Signal::new("body_char_share", body_share),
        Signal::new("size_sigma", sigma as f32),
    ];
    StyleInventory {
        clusters,
        body,
        total_chars,
        valid,
        // Detail 3: an invalid inventory disables escalation for the whole book, not for the
        // call that noticed. No model can rescue scaffolding that is not there.
        escalation_allowed: valid,
        warnings,
        confidence: if valid {
            Confidence::deterministic(signals)
        } else {
            Confidence::fallback(signals)
        },
    }
}

/// Round a size to the nearest quantum.
fn quantise(size_pt: f32, quantum: f32) -> f32 {
    dequantise(quanta(size_pt, quantum), quantum)
}

/// How many quanta a size is, rounded. Saturating, so a non-finite size cannot panic.
fn quanta(size_pt: f32, quantum: f32) -> i64 {
    if quantum <= 0.0 || !size_pt.is_finite() {
        return 0;
    }
    (f64::from(size_pt) / f64::from(quantum)).round() as i64
}

fn dequantise(quanta: i64, quantum: f32) -> f32 {
    (quanta as f64 * f64::from(quantum)) as f32
}

/// The character-weighted standard deviation of the size distribution.
fn size_sigma(clusters: &[StyleCluster], total_chars: u64) -> f64 {
    if total_chars == 0 {
        return 0.0;
    }
    let total = total_chars as f64;
    let mean: f64 = clusters
        .iter()
        .map(|cluster| f64::from(cluster.size_pt) * cluster.char_count as f64)
        .sum::<f64>()
        / total;
    let variance: f64 = clusters
        .iter()
        .map(|cluster| {
            let delta = f64::from(cluster.size_pt) - mean;
            delta * delta * cluster.char_count as f64
        })
        .sum::<f64>()
        / total;
    variance.max(0.0).sqrt()
}

/// Per page, the horizontal extent of the page's text.
fn page_text_extents(runs: &[Run]) -> BTreeMap<u32, (f32, f32)> {
    let mut extents: BTreeMap<u32, (f32, f32)> = BTreeMap::new();
    for run in runs {
        let entry = extents
            .entry(run.page.index)
            .or_insert((run.bbox.x0, run.bbox.x1));
        entry.0 = entry.0.min(run.bbox.x0);
        entry.1 = entry.1.max(run.bbox.x1);
    }
    extents
}

/// Per page, the index of the run that comes first in reading order.
///
/// Topmost, and leftmost among equals. Not the backend's order: no stage may treat the
/// backend's glyph order as reading order (Phase 1 item 1.3), and this is a stage.
fn page_first_runs(runs: &[Run]) -> BTreeMap<u32, usize> {
    let mut first: BTreeMap<u32, usize> = BTreeMap::new();
    for (index, run) in runs.iter().enumerate() {
        let better = match first.get(&run.page.index).and_then(|i| runs.get(*i)) {
            None => true,
            Some(current) => (run.bbox.y0, run.bbox.x0) < (current.bbox.y0, current.bbox.x0),
        };
        if better {
            first.insert(run.page.index, index);
        }
    }
    first
}

/// Whether a run sits centred within its page's text extent.
fn is_centered(run: &Run, extents: &BTreeMap<u32, (f32, f32)>, t: &Thresholds) -> bool {
    let Some((x0, x1)) = extents.get(&run.page.index) else {
        return false;
    };
    let width = x1 - x0;
    if width <= 0.0 {
        return false;
    }
    let page_center = (x0 + x1) / 2.0;
    let run_center = (run.bbox.x0 + run.bbox.x1) / 2.0;
    f64::from((run_center - page_center).abs())
        <= t.headings.centered_tolerance_ratio * f64::from(width)
}

fn ratio(numerator: u64, denominator: u64) -> f32 {
    if denominator == 0 {
        return 0.0;
    }
    (numerator as f64 / denominator as f64) as f32
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Row 4.6 of the Phase 4 table, plus the
// properties of the histogram the row relies on.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use oc_model::extract::PageRef;
    use oc_model::geom::Rect;
    use oc_model::text::{RunId, TextProvenance};

    /// One run at a given style, laid out left to right at a nominal position.
    fn run(index: u32, page: u32, text: &str, size_pt: f32, weight: u16) -> Run {
        Run {
            id: RunId(index),
            page: PageRef::new(page),
            text: text.to_owned(),
            bbox: Rect {
                x0: 72.0,
                y0: 100.0 + index as f32,
                x1: 72.0 + text.chars().count() as f32 * size_pt * 0.5,
                y1: 100.0 + index as f32 + size_pt,
            },
            baseline_y: 100.0 + index as f32 + size_pt,
            font: oc_model::extract::FontId(0),
            size_pt,
            weight,
            italic: false,
            superscript: false,
            subscript: false,
            provenance: TextProvenance::Pdf,
            glyph_range: (0, 0),
        }
    }

    fn one_font() -> Vec<FontInfo> {
        vec![FontInfo {
            id: oc_model::extract::FontId(0),
            name: "Test-Regular".to_owned(),
            family_key: "test".to_owned(),
            serif: true,
            fixed_pitch: false,
            symbolic: false,
            type3: false,
            embedded: true,
        }]
    }

    /// Row 4.6. Forty distinct sizes make forty clusters; `inventory.max_clusters` is 24, so
    /// the inventory is invalid, it says so, and escalation is off for the whole book.
    ///
    /// The anthology case (RT A8.2): a book that reprints forty sources keeps forty
    /// typographies, and a size rank over forty clusters is noise. What the gate buys is that
    /// the noise is *named* rather than silently emitted as a six-level heading tree.
    #[test]
    fn style_inventory_invalid_above_24_clusters() {
        let t = &oc_core::thresholds::T;
        let runs: Vec<Run> = (0..40u32)
            .map(|index| run(index, 0, "some text here", 6.0 + index as f32, 400))
            .collect();
        let inventory = cluster_styles(&runs, &one_font(), t);

        assert_eq!(inventory.clusters.len(), 40);
        assert!(!inventory.valid);
        assert!(
            inventory
                .warnings
                .iter()
                .any(|warning| warning.code == W_STYLE_INVENTORY_INVALID),
            "an invalid inventory must say so: {:?}",
            inventory.warnings
        );
        assert!(
            !inventory.escalation_allowed,
            "detail 3: no escalation is ever attempted for this book"
        );
        assert!(inventory.confidence.fallback_used);

        // Size-rank only: the ranking still exists and is still by size descending, because
        // that is the fallback the gate falls back *to* (detail 3).
        let sizes: Vec<f32> = inventory
            .clusters
            .iter()
            .map(|cluster| cluster.size_pt)
            .collect();
        let mut descending = sizes.clone();
        descending.sort_by(|a, b| b.total_cmp(a));
        assert_eq!(sizes, descending);
        assert_eq!(inventory.clusters.first().map(|c| c.id), Some(ClusterId(0)));
    }

    /// The other half of the gate: few clusters, but no cluster holds enough of the book to
    /// be its body. Two styles at half the characters each is a document whose typography
    /// says nothing about which text is the text.
    #[test]
    fn style_inventory_invalid_when_no_cluster_is_the_body() {
        let t = &oc_core::thresholds::T;
        let runs = vec![
            run(0, 0, "exactly twelve chars", 10.0, 400),
            run(1, 0, "exactly twelve chars", 14.0, 400),
        ];
        let inventory = cluster_styles(&runs, &one_font(), t);

        assert_eq!(inventory.clusters.len(), 2);
        assert!(
            !inventory.valid,
            "a 50 % body share is below the 60 % floor"
        );
        assert!(!inventory.escalation_allowed);
    }

    /// The ordinary case, and the three signals a heading candidate is judged on.
    ///
    /// Body is the mode by character count even though the heading is set larger; the
    /// heading is a candidate because it is larger and holds under 15 % of the book; and the
    /// heading opens its page, which is what `starts_page_ratio` is for.
    #[test]
    fn the_mode_is_body_and_the_larger_short_style_is_the_candidate() {
        let t = &oc_core::thresholds::T;
        let mut runs = vec![run(0, 0, "Chapter One", 18.0, 700)];
        runs.extend((1..12u32).map(|index| {
            run(
                index,
                0,
                "a line of ordinary body text set at ten point",
                10.0,
                400,
            )
        }));
        let inventory = cluster_styles(&runs, &one_font(), t);

        assert!(inventory.valid);
        assert_eq!(inventory.body_size_pt(), 10.0);
        assert_eq!(inventory.candidates(t).len(), 1);
        // The heading is the larger style, so it ranks first and is `ClusterId(0)`.
        assert_eq!(inventory.candidates(t).first(), Some(&ClusterId(0)));

        let heading = &inventory.clusters[0];
        assert!(heading.is_bold(t));
        assert_eq!(heading.starts_page_ratio, 1.0);
        assert!(heading.size_z > 0.0, "a larger style has a positive z");
        assert_eq!(heading.examples, vec!["Chapter One".to_owned()]);
    }

    /// Sizes are quantised before clustering, so the jitter a substituted face introduces
    /// does not split one style into three (R1 §A.11 #23).
    #[test]
    fn sizes_within_one_quantum_cluster_together() {
        let t = &oc_core::thresholds::T;
        let runs = vec![
            run(0, 0, "aaaaaaaaaa", 10.0, 400),
            run(1, 0, "bbbbbbbbbb", 10.1, 400),
            run(2, 0, "cccccccccc", 9.9, 400),
        ];
        let inventory = cluster_styles(&runs, &one_font(), t);
        assert_eq!(inventory.clusters.len(), 1);
        assert_eq!(inventory.clusters[0].size_pt, 10.0);
    }

    /// A document with no text has no body and no clusters, and does not panic on the way to
    /// saying so. `metadata`'s heuristic fallback calls this on a scanned book.
    #[test]
    fn an_empty_document_has_no_body_cluster() {
        let t = &oc_core::thresholds::T;
        let inventory = cluster_styles(&[], &[], t);
        assert!(inventory.clusters.is_empty());
        assert_eq!(inventory.body, None);
        assert_eq!(inventory.body_size_pt(), 0.0);
        assert!(inventory.candidates(t).is_empty());
    }
}

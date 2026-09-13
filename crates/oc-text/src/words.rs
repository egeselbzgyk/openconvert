//! Word assembly and run building: glyphs to `Run`s, with the spaces the document did not
//! write (PIPELINE §4 steps 2–4).
//!
//! **A space in PDF is a kerning number, not a character.** Most producers move the text
//! cursor and draw the next word; nothing in the file says "space". So the space has to be
//! inferred from the advance gaps, and the wrong rule is the obvious one: "a gap wider than
//! the narrowest gap is a space" turns every letter-spaced heading into `H a l l o`.
//!
//! The rule used here is the one R10 §6.2 recommends. Collect the observed gaps per
//! `(font, size)` across the page, fit 2-means to them, and threshold at the midpoint of the
//! two centroids — but **only if the two clusters are actually separated**, measured as the
//! centroid distance over the within-cluster spread. Letter-spaced text has one mode and no
//! separation, so the fit is rejected and the threshold falls back to a metric space width;
//! 1.2 pt of tracking is then narrower than a space and the word stays whole.
//!
//! Superscript and subscript are read from geometry **before** normalisation, because NFKC
//! would map `¹` to `1` and destroy the signal — which is why NFKC is banned pipeline-wide
//! (R2 §B.6, §B.8, RT C1).

use oc_core::thresholds::Thresholds;
use oc_model::extract::{FontId, Glyph, PageRef};
use oc_model::geom::Rect;
use oc_model::text::{Run, RunId, TextProvenance};

use crate::lines::cluster_baselines;

/// Runs, and the glyph order they index.
///
/// The order matters enough to be returned rather than recomputed: the backend's glyph order
/// is not reading order (Phase 1 item 1.3), so `Run::glyph_range` cannot index the input
/// slice and a caller that wants a run's glyphs needs the permutation that produced it.
#[derive(Clone, Debug, PartialEq)]
pub struct RunAssembly {
    pub runs: Vec<Run>,
    /// Indices into the glyph slice, in assembly order: line by line down the page, left to
    /// right within a line.
    pub order: Vec<u32>,
}

/// Assemble a page's glyphs into runs.
pub fn assemble_runs(glyphs: &[Glyph], page: PageRef, t: &Thresholds) -> RunAssembly {
    if glyphs.is_empty() {
        return RunAssembly {
            runs: Vec::new(),
            order: Vec::new(),
        };
    }

    let thresholds = space_thresholds(glyphs, t);

    let items: Vec<(f32, f32)> = glyphs
        .iter()
        .map(|glyph| (glyph.origin.1, glyph.size_pt))
        .collect();

    let mut runs: Vec<Run> = Vec::new();
    let mut order: Vec<u32> = Vec::new();

    for line in cluster_baselines(&items, t) {
        let mut line: Vec<u32> = line;
        line.sort_by(|a, b| {
            let (left, right) = (&glyphs[*a as usize], &glyphs[*b as usize]);
            left.origin
                .0
                .partial_cmp(&right.origin.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.cmp(b))
        });

        let body_size = modal_size(&line, glyphs);
        let baseline = line_baseline(&line, glyphs, body_size);
        let flags: Vec<Vertical> = line
            .iter()
            .map(|index| vertical_of(&glyphs[*index as usize], baseline, body_size, t))
            .collect();

        let mut open: Option<OpenRun> = None;
        for (position, index) in line.iter().enumerate() {
            let glyph = &glyphs[*index as usize];
            let vertical = flags[position];

            // A gap wider than this (font, size)'s space threshold is a space the document
            // did not write. It is whitespace, so it is outside `C` and never ledgered
            // (ARCHITECTURE §5.2).
            let spaced = position > 0 && {
                let previous = &glyphs[line[position - 1] as usize];
                let gap = glyph.loose_bbox.x0 - previous.loose_bbox.x1;
                gap > thresholds.for_glyph(previous)
            };

            let style = Style::of(glyph, vertical);
            match open.as_mut() {
                Some(current) if current.style == style && !current.text.is_empty() => {
                    if spaced {
                        current.text.push(' ');
                    }
                    current.push(glyph, order.len());
                }
                _ => {
                    if let Some(mut finished) = open.take() {
                        // The space belongs between the two runs; it is written at the end of
                        // the first so that concatenating a line's runs reproduces the line.
                        if spaced {
                            finished.text.push(' ');
                        }
                        runs.push(finished.finish(RunId(runs.len() as u32), page.clone()));
                    }
                    let mut started = OpenRun::new(style, order.len());
                    started.push(glyph, order.len());
                    open = Some(started);
                }
            }
            order.push(*index);
        }
        if let Some(finished) = open.take() {
            runs.push(finished.finish(RunId(runs.len() as u32), page.clone()));
        }
    }

    RunAssembly { runs, order }
}

/// Whether a glyph sits above, on, or below its line's baseline.
///
/// Signature as PIPELINE §4 step 4 states the rule: raised (or dropped) by at least
/// `footnote.superscript_rise_ratio × size_pt`, **and** set smaller than
/// `text.superscript_size_ratio_max × body`. Both conditions, because a whole line set 3 pt
/// higher than its neighbour is a line, not a page of superscripts.
pub fn superscript_flags(glyphs: &[Glyph], baseline: f32, body_size: f32) -> Vec<bool> {
    let t = &oc_core::thresholds::T;
    glyphs
        .iter()
        .map(|glyph| vertical_of(glyph, baseline, body_size, t) == Vertical::Superscript)
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Vertical {
    Superscript,
    Baseline,
    Subscript,
}

fn vertical_of(glyph: &Glyph, baseline: f32, body_size: f32, t: &Thresholds) -> Vertical {
    // Page space has y pointing down, so a raised glyph has the *smaller* origin.
    let offset = baseline - glyph.origin.1;
    let rise = t.footnote.superscript_rise_ratio as f32 * glyph.size_pt;
    let small = glyph.size_pt < t.text.superscript_size_ratio_max as f32 * body_size;
    if !small {
        return Vertical::Baseline;
    }
    if offset >= rise {
        Vertical::Superscript
    } else if offset <= -rise {
        Vertical::Subscript
    } else {
        Vertical::Baseline
    }
}

/// What makes two adjacent glyphs part of the same run.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Style {
    font: FontId,
    size_bits: u32,
    weight: u16,
    italic: bool,
    vertical: Vertical,
}

impl Style {
    fn of(glyph: &Glyph, vertical: Vertical) -> Self {
        Self {
            font: glyph.font,
            // Compared as bits, so two glyphs at the same size are the same style however
            // the size was computed and without an epsilon nobody can justify.
            size_bits: glyph.size_pt.to_bits(),
            weight: glyph.weight,
            italic: glyph.italic,
            vertical,
        }
    }
}

struct OpenRun {
    style: Style,
    text: String,
    bbox: Option<Rect>,
    baseline_sum: f32,
    glyph_count: u32,
    size_pt: f32,
    first_glyph: usize,
}

impl OpenRun {
    fn new(style: Style, first_glyph: usize) -> Self {
        Self {
            style,
            text: String::new(),
            bbox: None,
            baseline_sum: 0.0,
            glyph_count: 0,
            size_pt: 0.0,
            first_glyph,
        }
    }

    fn push(&mut self, glyph: &Glyph, _at: usize) {
        self.text.push(glyph.ch);
        self.bbox = Some(match self.bbox {
            Some(current) => Rect {
                x0: current.x0.min(glyph.bbox.x0),
                y0: current.y0.min(glyph.bbox.y0),
                x1: current.x1.max(glyph.bbox.x1),
                y1: current.y1.max(glyph.bbox.y1),
            },
            None => glyph.bbox,
        });
        self.baseline_sum += glyph.origin.1;
        self.glyph_count += 1;
        self.size_pt = glyph.size_pt;
    }

    fn finish(self, id: RunId, page: PageRef) -> Run {
        let count = self.glyph_count.max(1);
        Run {
            id,
            page,
            text: self.text,
            bbox: self.bbox.unwrap_or(Rect {
                x0: 0.0,
                y0: 0.0,
                x1: 0.0,
                y1: 0.0,
            }),
            baseline_y: self.baseline_sum / count as f32,
            font: self.style.font,
            size_pt: self.size_pt,
            weight: self.style.weight,
            italic: self.style.italic,
            superscript: self.style.vertical == Vertical::Superscript,
            subscript: self.style.vertical == Vertical::Subscript,
            provenance: TextProvenance::Pdf,
            glyph_range: (
                self.first_glyph as u32,
                self.first_glyph as u32 + self.glyph_count,
            ),
        }
    }
}

/// The most common size on a line, which is its body size.
fn modal_size(line: &[u32], glyphs: &[Glyph]) -> f32 {
    let mut sizes: Vec<u32> = line
        .iter()
        .map(|index| glyphs[*index as usize].size_pt.to_bits())
        .collect();
    sizes.sort_unstable();
    let mut best = (0usize, sizes.first().copied().unwrap_or_default());
    let mut run = (0usize, u32::MAX);
    for bits in sizes {
        if bits == run.1 {
            run.0 += 1;
        } else {
            run = (1, bits);
        }
        if run.0 > best.0 {
            best = run;
        }
    }
    f32::from_bits(best.1)
}

/// A line's baseline: the median origin of the glyphs set at its body size.
fn line_baseline(line: &[u32], glyphs: &[Glyph], body_size: f32) -> f32 {
    let mut baselines: Vec<f32> = line
        .iter()
        .map(|index| &glyphs[*index as usize])
        .filter(|glyph| glyph.size_pt.to_bits() == body_size.to_bits())
        .map(|glyph| glyph.origin.1)
        .collect();
    if baselines.is_empty() {
        baselines = line
            .iter()
            .map(|index| glyphs[*index as usize].origin.1)
            .collect();
    }
    baselines.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    baselines
        .get(baselines.len() / 2)
        .copied()
        .unwrap_or_default()
}

/// The space threshold for each `(font, size)` on the page.
struct SpaceThresholds {
    /// `(font, size bits, threshold in points)`, in insertion order; a page has a handful of
    /// styles, so a scan beats a map and keeps the iteration order deterministic.
    per_style: Vec<(FontId, u32, f32)>,
    fallback_ratio: f32,
}

impl SpaceThresholds {
    fn for_glyph(&self, glyph: &Glyph) -> f32 {
        self.per_style
            .iter()
            .find(|(font, size, _)| *font == glyph.font && *size == glyph.size_pt.to_bits())
            .map(|(_, _, threshold)| *threshold)
            .unwrap_or(self.fallback_ratio * glyph.size_pt)
    }
}

fn space_thresholds(glyphs: &[Glyph], t: &Thresholds) -> SpaceThresholds {
    let fallback_ratio = t.words.fallback_space_ratio as f32;

    // Gap samples keyed by the *left* glyph's style: the advance being measured is that
    // glyph's, so it is that glyph's font and size the distribution belongs to.
    let mut samples: Vec<(FontId, u32, Vec<f32>)> = Vec::new();
    let items: Vec<(f32, f32)> = glyphs
        .iter()
        .map(|glyph| (glyph.origin.1, glyph.size_pt))
        .collect();
    for line in cluster_baselines(&items, t) {
        let mut line = line;
        line.sort_by(|a, b| {
            glyphs[*a as usize]
                .origin
                .0
                .partial_cmp(&glyphs[*b as usize].origin.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.cmp(b))
        });
        for pair in line.windows(2) {
            let left = &glyphs[pair[0] as usize];
            let right = &glyphs[pair[1] as usize];
            let gap = right.loose_bbox.x0 - left.loose_bbox.x1;
            if gap < 0.0 {
                continue;
            }
            let key = (left.font, left.size_pt.to_bits());
            match samples
                .iter_mut()
                .find(|(font, size, _)| (*font, *size) == key)
            {
                Some((_, _, list)) => list.push(gap),
                None => samples.push((key.0, key.1, vec![gap])),
            }
        }
    }

    let per_style = samples
        .into_iter()
        .map(|(font, size_bits, gaps)| {
            let size = f32::from_bits(size_bits);
            let threshold = two_means_threshold(&gaps, t).unwrap_or(fallback_ratio * size);
            (font, size_bits, threshold)
        })
        .collect();

    SpaceThresholds {
        per_style,
        fallback_ratio,
    }
}

/// Fit 2-means to a gap distribution and return the midpoint of the centroids, or `None` if
/// the two clusters are not separated well enough to believe.
///
/// Separation is the centroid distance over the mean within-cluster deviation, which is the
/// quantity PIPELINE §4 step 3 calls "the cluster separation divided by the within-cluster
/// spread". Uniform tracking gives a distance of zero and is rejected however tight the
/// clusters are; clean prose gives a distance of a few points against a spread of a fraction
/// of one and is accepted.
fn two_means_threshold(gaps: &[f32], t: &Thresholds) -> Option<f32> {
    if gaps.len() < 2 {
        return None;
    }
    let low_start = gaps.iter().copied().fold(f32::MAX, f32::min);
    let high_start = gaps.iter().copied().fold(f32::MIN, f32::max);
    let resolution = t.words.gap_resolution_pt as f32;
    if high_start - low_start <= resolution {
        return None;
    }

    let (mut low, mut high) = (low_start, high_start);
    for _ in 0..LLOYD_ITERATIONS {
        let mut low_sum = 0.0f32;
        let mut low_count = 0u32;
        let mut high_sum = 0.0f32;
        let mut high_count = 0u32;
        for gap in gaps {
            if (gap - low).abs() <= (gap - high).abs() {
                low_sum += gap;
                low_count += 1;
            } else {
                high_sum += gap;
                high_count += 1;
            }
        }
        if low_count == 0 || high_count == 0 {
            return None;
        }
        let (next_low, next_high) = (low_sum / low_count as f32, high_sum / high_count as f32);
        if (next_low - low).abs() <= resolution && (next_high - high).abs() <= resolution {
            low = next_low;
            high = next_high;
            break;
        }
        low = next_low;
        high = next_high;
    }

    let mut deviation = 0.0f32;
    for gap in gaps {
        deviation += (gap - low).abs().min((gap - high).abs());
    }
    deviation /= gaps.len() as f32;

    // The spread is floored at the resolution below which two gaps are the same gap, so a
    // pair of clusters a thousandth of a point apart cannot report perfect separation.
    let separation = (high - low) / deviation.max(resolution);
    if separation < t.words.gap_separation_ratio_min as f32 {
        return None;
    }
    Some((low + high) / 2.0)
}

/// Lloyd's algorithm on one dimension over a handful of clusters converges in a few passes;
/// the cap is a bound, not a tuning parameter, and the loop breaks out when it settles.
const LLOYD_ITERATIONS: usize = 16;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
fn gap_threshold(gaps: &[f32]) -> Option<f32> {
    two_means_threshold(gaps, &oc_core::thresholds::T)
}

#[test]
fn uniform_tracking_has_no_space_to_find() {
    // `H a l l o` set with Tc 1.2: four identical gaps.
    assert_eq!(gap_threshold(&[1.2, 1.2, 1.2, 1.2]), None);
}

#[test]
fn prose_gaps_threshold_between_the_two_modes() {
    // Intra-word gaps near zero, word gaps near a space width.
    let gaps = [0.02, 0.0, 3.3, 0.05, 0.01, 3.4, 0.0, 3.2];
    let threshold = gap_threshold(&gaps).expect("prose is bimodal");
    assert!(threshold > 0.1 && threshold < 3.0, "{threshold}");
}

#[test]
fn a_single_gap_cannot_be_clustered() {
    assert_eq!(gap_threshold(&[6.1]), None);
    assert_eq!(gap_threshold(&[]), None);
}

#[test]
fn a_flat_distribution_is_rejected_even_when_tight() {
    // Every gap within a hundredth of a point of every other: separated by nothing.
    assert_eq!(gap_threshold(&[2.0, 2.001, 2.0, 2.001]), None);
}

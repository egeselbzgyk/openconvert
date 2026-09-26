//! Running heads, running feet and page numbers, removed **before** segmentation
//! (PIPELINE §5, R2 §B.4, R10 §6.6).
//!
//! This is the highest-confidence verdict in the whole pipeline — cross-page repetition is
//! definitionally a cross-page signal, and a deterministic detector scores 92.8 on the
//! category where a 3B vision model scores 32.1 — and simultaneously the largest text-loss
//! risk in it. Calibre auto-detects a pixel threshold from the first twenty pages and then
//! eats the only line on an atypical one; Marker deletes body text outright in the same class
//! of bug (R1 §C.2 #7, §C.3). So the detector is built the other way round from the obvious
//! one: it looks for **evidence to delete**, and every rule that fires has to survive a set of
//! rules that refuse.
//!
//! The evidence, in the order it is gathered:
//!
//! 1. **Bands.** The outer `layout.furniture.band_ratio` of page height, top and bottom. A
//!    fraction rather than PyMuPDF's fixed 50 pt, because a fraction generalises across page
//!    sizes (R2 §B.4).
//! 2. **Keys.** Band text is digit-masked (`12` → `#`), folded in the document's own locale
//!    and stripped of punctuation and space. Digit masking is what lets `Page 12` and
//!    `Page 137` be the same running foot, and R2 §B.4 is explicit that skipping it is the
//!    difference between 80 % and near-perfect on real books. Keys that differ by no more than
//!    `layout.furniture.ned_max` in normalised edit distance are the same key.
//! 3. **Position.** Candidates sharing a key must also share a height, to within
//!    `layout.furniture.y_cluster_ratio` of body line height.
//! 4. **Scopes.** The repetition ratio is computed globally, per parity, and over a sliding
//!    window of `layout.furniture.window_pages`, and the best of the four is used. Parity
//!    because books alternate book title and chapter title between verso and recto, and a
//!    parity-blind detector sees two half-strength patterns instead of two strong ones. The
//!    window because a chapter-title head repeats strongly inside its chapter and weakly
//!    across the book.
//! 5. **Page numbers.** A band line whose masked form is entirely numeric — arabic or roman,
//!    upper or lower, because front matter — and whose values across pages form a monotone
//!    arithmetic progression. That test is what separates a page number from a chapter number
//!    and it is cheap and highly reliable. The value goes to `PageRef.label` and from there to
//!    `page-list` nav, which is outside `C`, so removing it from the flow is a clean
//!    `PageNumber` entry rather than a paradox.
//!
//! And the rules that refuse:
//!
//! - a document with fewer than two pages has no cross-page evidence and gets no furniture
//!   detection at all;
//! - a band line that is the only content on its page is never removed;
//! - a band line set in body type, ending without terminal punctuation, followed by a line
//!   that starts lower-case, is a sentence in progress and is never removed;
//! - a ratio inside the grey zone `[0.30, 0.70)` is an abstention: the verdict is marked
//!   `uncertain` and the line stays;
//! - an all-numeric band line that is not a progression stays, and is not reconsidered as a
//!   running head — a chapter number repeats perfectly and is not furniture.

use std::collections::BTreeMap;

use oc_core::thresholds::Thresholds;
use oc_model::extract::PageRef;
use oc_model::geom::Rect;
use oc_model::lang::LangTag;
use oc_model::ledger::{LedgerDelta, LedgerEntry, Reason};
use oc_model::text::{FurnitureKind, Line, Run};
use oc_text::fold::fold_key;
use oc_text::similarity::normalised_edit_distance;

/// The stage name every ledger entry from this module carries.
pub const STAGE: &str = "furniture";

/// Cross-page repetition needs at least two pages to be cross-page. On a one-page document
/// the detector is switched off entirely rather than run with a lowered bar: "this line
/// appears on every page" is a true statement about a single page and it means nothing.
const MIN_PAGES_FOR_EVIDENCE: usize = 2;

/// One line, reduced to what furniture detection needs to know about it.
#[derive(Clone, Debug, PartialEq)]
pub struct LineText {
    pub text: String,
    pub bbox: Rect,
    pub baseline_y: f32,
    pub size_pt: f32,
}

/// One page's lines, with the page height the bands are a fraction of.
#[derive(Clone, Debug, PartialEq)]
pub struct PageLines {
    pub page: PageRef,
    pub page_height_pt: f32,
    pub lines: Vec<LineText>,
}

impl PageLines {
    /// Flatten a page's assembled runs and lines into the shape this stage reads.
    pub fn from_runs(page: PageRef, page_height_pt: f32, runs: &[Run], lines: &[Line]) -> Self {
        let lines = lines
            .iter()
            .map(|line| LineText {
                text: line
                    .runs
                    .iter()
                    .filter_map(|id| runs.get(id.0 as usize))
                    .map(|run| run.text.as_str())
                    .collect::<String>()
                    .trim()
                    .to_owned(),
                bbox: line.bbox,
                baseline_y: line.baseline_y,
                size_pt: line
                    .runs
                    .iter()
                    .filter_map(|id| runs.get(id.0 as usize))
                    .map(|run| run.size_pt)
                    .fold(0.0f32, f32::max),
            })
            .collect();
        Self {
            page,
            page_height_pt,
            lines,
        }
    }
}

/// Where on the page a line sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Band {
    Top,
    Bottom,
    Body,
}

/// Which pages the repetition ratio was computed over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Every eligible page.
    Global,
    /// Odd-numbered pages only — recto, in a book that starts on one.
    Odd,
    /// Even-numbered pages only.
    Even,
    /// The best window of `layout.furniture.window_pages` consecutive pages.
    Window,
}

/// Why the detector decided what it decided, kept whether or not it fired.
///
/// An abstention with its measured ratio recorded is worth as much as a deletion: it is what
/// the report shows when a reader asks why the running head is still there.
#[derive(Clone, Debug, PartialEq)]
pub struct RepetitionEvidence {
    /// The masked, folded, punctuation-stripped form the candidates were grouped on.
    pub key: String,
    pub band: Band,
    pub scope: Scope,
    /// Pages in the scope.
    pub scope_pages: u32,
    /// Pages in the scope carrying this key in this band at this height.
    pub hit_pages: u32,
    pub ratio: f32,
    /// The ratio landed inside the grey zone, so the detector abstained.
    pub uncertain: bool,
    /// The step of the arithmetic progression, when the candidate is a page number.
    pub progression_step: Option<i64>,
    /// The printed page number this line carries, when it is one.
    pub label: Option<String>,
}

impl RepetitionEvidence {
    fn body(key: String) -> Self {
        Self {
            key,
            band: Band::Body,
            scope: Scope::Global,
            scope_pages: 0,
            hit_pages: 0,
            ratio: 0.0,
            uncertain: false,
            progression_step: None,
            label: None,
        }
    }
}

/// One line's verdict. `kind` is `None` for everything that stays.
#[derive(Clone, Debug, PartialEq)]
pub struct FurnitureVerdict {
    pub kind: Option<FurnitureKind>,
    pub evidence: RepetitionEvidence,
}

/// Classify every line of every page.
///
/// The returned vector is page-major and line-aligned with the input: verdict `i` belongs to
/// the `i`-th line counted across pages in order. `lang` is not decoration — folding the key
/// is the one place casing happens, and Turkish pairs its dotted and dotless i its own way
/// (R10 §6.3), so a Turkish running head folded under invariant rules stops matching itself.
pub fn detect_furniture(
    pages: &[PageLines],
    lang: LangTag,
    t: &Thresholds,
) -> Vec<FurnitureVerdict> {
    let mut verdicts: Vec<FurnitureVerdict> = pages
        .iter()
        .flat_map(|page| page.lines.iter())
        .map(|_| FurnitureVerdict {
            kind: None,
            evidence: RepetitionEvidence::body(String::new()),
        })
        .collect();

    let eligible: Vec<usize> = pages
        .iter()
        .enumerate()
        .filter(|(_, page)| !page.lines.is_empty())
        .map(|(index, _)| index)
        .collect();
    if eligible.len() < MIN_PAGES_FOR_EVIDENCE {
        return verdicts;
    }

    let body_size = modal_body_size(pages);
    let offsets = line_offsets(pages);

    // Page numbers first, by the one property only a page number has: it is the page's index
    // plus a constant. Found here they are not grouped again below, where a folio that moved
    // from two digits to three, or that a chapter opening skipped, broke the old test.
    let folios = find_folios(pages, body_size, t);
    for folio in &folios {
        verdicts[offsets[folio.page] + folio.line] = FurnitureVerdict {
            kind: Some(FurnitureKind::PageNumber),
            evidence: RepetitionEvidence {
                progression_step: Some(1),
                label: Some(folio.label.clone()),
                band: folio.band,
                ..RepetitionEvidence::body("#".to_owned())
            },
        };
    }
    let taken: std::collections::BTreeSet<(usize, usize)> = folios
        .iter()
        .map(|folio| (folio.page, folio.line))
        .collect();
    let candidates: Vec<Candidate> = gather(pages, lang, body_size, t)
        .into_iter()
        .filter(|candidate| !taken.contains(&(candidate.page, candidate.line)))
        .collect();

    for group in group_candidates(candidates, body_size, t) {
        let evidence = best_evidence(&group, &eligible, pages, t);
        let kind = decide(&group, &evidence, t);
        for member in &group.members {
            let slot = offsets[member.page] + member.line;
            verdicts[slot] = FurnitureVerdict {
                kind,
                evidence: RepetitionEvidence {
                    label: member.label.clone(),
                    ..evidence.clone()
                },
            };
        }
    }

    // The refusals that depend on the page rather than on the pattern.
    for (page_index, page) in pages.iter().enumerate() {
        let base = offsets[page_index];
        let surviving = (0..page.lines.len())
            .filter(|line| verdicts[base + line].kind.is_none())
            .count();
        if surviving > 0 {
            continue;
        }
        // Removing everything would empty the page, so nothing is removed from it.
        for line in 0..page.lines.len() {
            verdicts[base + line].kind = None;
        }
    }

    verdicts
}

/// What survived, what left, and what each page is now called.
#[derive(Clone, Debug, PartialEq)]
pub struct FurnitureOutcome {
    /// The pages with their furniture lines gone.
    pub pages: Vec<PageLines>,
    pub delta: LedgerDelta,
    /// The printed page number recovered for each input page, where one was found. It becomes
    /// `PageRef.label`, and from there the `page-list` nav target.
    pub labels: Vec<Option<String>>,
}

/// Remove what `detect_furniture` condemned, and record every character of it.
pub fn apply_furniture(pages: &[PageLines], verdicts: &[FurnitureVerdict]) -> FurnitureOutcome {
    let offsets = line_offsets(pages);
    let mut delta = LedgerDelta::default();
    let mut labels = vec![None; pages.len()];
    let mut kept = Vec::with_capacity(pages.len());

    for (page_index, page) in pages.iter().enumerate() {
        let mut lines = Vec::new();
        let mut removed_chars: u32 = 0;
        for (line_index, line) in page.lines.iter().enumerate() {
            let Some(verdict) = verdicts.get(offsets[page_index] + line_index) else {
                lines.push(line.clone());
                continue;
            };
            let Some(kind) = verdict.kind else {
                lines.push(line.clone());
                continue;
            };
            let width = u32::try_from(line.text.chars().count()).unwrap_or(u32::MAX);
            delta.push(LedgerEntry::removed(
                STAGE,
                reason_of(kind),
                page.page.index,
                (removed_chars, removed_chars.saturating_add(width)),
                line.text.clone(),
            ));
            removed_chars = removed_chars.saturating_add(width);
            if kind == FurnitureKind::PageNumber {
                labels[page_index] = verdict.evidence.label.clone();
            }
        }
        kept.push(PageLines {
            page: page.page.clone(),
            page_height_pt: page.page_height_pt,
            lines,
        });
    }

    FurnitureOutcome {
        pages: kept,
        delta,
        labels,
    }
}

fn reason_of(kind: FurnitureKind) -> Reason {
    match kind {
        FurnitureKind::RunningHeader => Reason::RunningHeader,
        FurnitureKind::RunningFooter => Reason::RunningFooter,
        FurnitureKind::PageNumber => Reason::PageNumber,
    }
}

// ---------------------------------------------------------------------------
// Candidates
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct Candidate {
    page: usize,
    line: usize,
    band: Band,
    key: String,
    baseline_y: f32,
    /// The numeric value of an all-numeric band line, arabic or roman.
    value: Option<i64>,
    label: Option<String>,
    /// This line is the only thing on its page.
    sole_content: bool,
    /// Set in body type, unpunctuated, and followed by a lower-case line: a sentence in
    /// progress, whatever the repetition says.
    continues_a_sentence: bool,
}

#[derive(Clone, Debug)]
struct Group {
    band: Band,
    key: String,
    members: Vec<Candidate>,
    all_numeric: bool,
}

fn gather(pages: &[PageLines], lang: LangTag, body_size: f32, t: &Thresholds) -> Vec<Candidate> {
    let band_ratio = t.layout.furniture.band_ratio as f32;
    let size_tolerance = t.layout.furniture.body_size_tolerance_ratio as f32;

    let mut out = Vec::new();
    for (page_index, page) in pages.iter().enumerate() {
        let band_height = band_ratio * page.page_height_pt;
        for (line_index, line) in page.lines.iter().enumerate() {
            let band = if line.bbox.y1 <= band_height {
                Band::Top
            } else if line.bbox.y0 >= page.page_height_pt - band_height {
                Band::Bottom
            } else {
                continue;
            };
            if line.text.trim().is_empty() {
                continue;
            }

            let key = mask(&fold_key(&line.text, lang.clone()));
            if key.is_empty() {
                continue;
            }
            let value = numeric_value(&line.text);

            let body_type = (line.size_pt - body_size).abs() <= size_tolerance * body_size;
            let unpunctuated = !line
                .text
                .trim_end()
                .chars()
                .next_back()
                .is_some_and(is_terminal_punctuation);
            let next_starts_lowercase = page
                .lines
                .get(line_index + 1)
                .and_then(|next| next.text.trim_start().chars().next())
                .is_some_and(char::is_lowercase);

            out.push(Candidate {
                page: page_index,
                line: line_index,
                band,
                key,
                baseline_y: line.baseline_y,
                value,
                label: value.map(|_| line.text.trim().to_owned()),
                sole_content: page.lines.len() == 1,
                continues_a_sentence: body_type && unpunctuated && next_starts_lowercase,
            });
        }
    }
    out
}

/// Group candidates by band, then by key, merging keys within `ned_max`, then split each group
/// by height so the same string printed at two heights is two patterns.
fn group_candidates(candidates: Vec<Candidate>, body_size: f32, t: &Thresholds) -> Vec<Group> {
    let ned_max = t.layout.furniture.ned_max as f32;
    let y_tolerance = t.layout.furniture.y_cluster_ratio as f32 * body_size;

    let mut groups: Vec<Group> = Vec::new();
    for candidate in candidates {
        let existing = groups.iter_mut().find(|group| {
            group.band == candidate.band
                && normalised_edit_distance(&group.key, &candidate.key) <= ned_max
        });
        match existing {
            Some(group) => group.members.push(candidate),
            None => groups.push(Group {
                band: candidate.band,
                key: candidate.key.clone(),
                all_numeric: true,
                members: vec![candidate],
            }),
        }
    }

    let mut split = Vec::new();
    for group in groups {
        let mut by_height: Vec<Group> = Vec::new();
        for member in group.members {
            let slot = by_height.iter_mut().find(|other| {
                other.members.first().is_some_and(|first| {
                    (first.baseline_y - member.baseline_y).abs() <= y_tolerance
                })
            });
            match slot {
                Some(other) => other.members.push(member),
                None => by_height.push(Group {
                    band: group.band,
                    key: group.key.clone(),
                    all_numeric: true,
                    members: vec![member],
                }),
            }
        }
        for mut piece in by_height {
            piece.all_numeric = piece.members.iter().all(|member| member.value.is_some());
            split.push(piece);
        }
    }
    split
}

// ---------------------------------------------------------------------------
// Evidence
// ---------------------------------------------------------------------------

fn best_evidence(
    group: &Group,
    eligible: &[usize],
    pages: &[PageLines],
    t: &Thresholds,
) -> RepetitionEvidence {
    let hits: Vec<usize> = {
        let mut hits: Vec<usize> = group.members.iter().map(|member| member.page).collect();
        hits.sort_unstable();
        hits.dedup();
        hits
    };

    let window = usize::try_from(t.layout.furniture.window_pages.max(1)).unwrap_or(usize::MAX);
    let mut best = measure(Scope::Global, eligible, &hits);

    for (scope, parity) in [(Scope::Even, 0u32), (Scope::Odd, 1u32)] {
        let scoped: Vec<usize> = eligible
            .iter()
            .copied()
            .filter(|index| pages[*index].page.index % 2 == parity)
            .collect();
        let candidate = measure(scope, &scoped, &hits);
        if candidate.ratio > best.ratio {
            best = candidate;
        }
    }

    let width = window.min(eligible.len());
    for start in 0..=eligible.len().saturating_sub(width) {
        let scoped = &eligible[start..start + width];
        let candidate = measure(Scope::Window, scoped, &hits);
        if candidate.ratio > best.ratio {
            best = candidate;
        }
    }

    let grey_min = t.layout.furniture.repetition_ratio_grey_min as f32;
    let fire = t.layout.furniture.repetition_ratio_min as f32;
    RepetitionEvidence {
        key: group.key.clone(),
        band: group.band,
        scope: best.scope,
        scope_pages: u32::try_from(best.scope_pages).unwrap_or(u32::MAX),
        hit_pages: u32::try_from(best.hit_pages).unwrap_or(u32::MAX),
        ratio: best.ratio,
        uncertain: best.ratio >= grey_min && best.ratio < fire,
        progression_step: progression_step(group),
        label: None,
    }
}

struct Measurement {
    scope: Scope,
    scope_pages: usize,
    hit_pages: usize,
    ratio: f32,
}

fn measure(scope: Scope, scoped: &[usize], hits: &[usize]) -> Measurement {
    let hit_pages = scoped.iter().filter(|index| hits.contains(index)).count();
    let scope_pages = scoped.len();
    Measurement {
        scope,
        scope_pages,
        hit_pages,
        ratio: if scope_pages == 0 {
            0.0
        } else {
            hit_pages as f32 / scope_pages as f32
        },
    }
}

/// How many pages of a scope must carry a pattern before it may be deleted.
///
/// `max(min_repeat_pages, share × scope)`, capped at the scope — a scope of two pages cannot
/// produce three — and floored at two, because one page is not cross-page evidence.
fn required_pages(scope_pages: u32, t: &Thresholds) -> u32 {
    let by_count = u32::try_from(t.layout.furniture.min_repeat_pages.max(0)).unwrap_or(u32::MAX);
    let by_share =
        (f64::from(scope_pages) * t.layout.furniture.min_repeat_page_share).ceil() as u32;
    by_count
        .max(by_share)
        .min(scope_pages)
        .max(MIN_PAGES_FOR_EVIDENCE as u32)
}

fn decide(group: &Group, evidence: &RepetitionEvidence, t: &Thresholds) -> Option<FurnitureKind> {
    if group.members.iter().any(|member| member.sole_content) {
        // Rule one, and it is absolute: whatever the pattern says, a page whose only line is
        // in the band keeps it. Applied to the whole group rather than to the one member,
        // because a rule that deletes a head from every page but one leaves a book whose
        // pages disagree about what they are.
        if group.members.iter().all(|member| member.sole_content) {
            return None;
        }
    }
    if group
        .members
        .iter()
        .any(|member| member.continues_a_sentence)
    {
        return None;
    }
    if evidence.hit_pages < required_pages(evidence.scope_pages, t) {
        return None;
    }
    if evidence.ratio < t.layout.furniture.repetition_ratio_min as f32 {
        return None;
    }

    if group.all_numeric {
        // An all-numeric band line is a page number or it is a chapter number, and only the
        // arithmetic-progression test tells them apart. Failing it is not a licence to
        // reconsider the line as a running head: a chapter number repeats perfectly, which is
        // exactly why the test exists (PIPELINE §5 step 6).
        return evidence.progression_step.map(|_| FurnitureKind::PageNumber);
    }

    Some(match group.band {
        Band::Top => FurnitureKind::RunningHeader,
        Band::Bottom | Band::Body => FurnitureKind::RunningFooter,
    })
}

/// The common difference of the group's values across pages, if they form a monotone
/// arithmetic progression with a non-zero step.
fn progression_step(group: &Group) -> Option<i64> {
    if !group.all_numeric || group.members.len() < MIN_PAGES_FOR_EVIDENCE {
        return None;
    }
    let mut by_page: BTreeMap<usize, i64> = BTreeMap::new();
    for member in &group.members {
        let value = member.value?;
        // Two different numbers in the same band at the same height on one page is not a
        // progression, it is a table.
        if by_page
            .insert(member.page, value)
            .is_some_and(|old| old != value)
        {
            return None;
        }
    }
    let values: Vec<i64> = by_page.values().copied().collect();
    let step = values.get(1)?.checked_sub(*values.first()?)?;
    if step == 0 {
        return None;
    }
    for pair in values.windows(2) {
        if pair[1].checked_sub(pair[0]) != Some(step) {
            return None;
        }
    }
    Some(step)
}

// ---------------------------------------------------------------------------
// Page numbers
// ---------------------------------------------------------------------------

/// How long a line must be to say where the text column's edges are: a full line of prose,
/// not a heading or a number.
const MARGIN_REFERENCE_CHARS: usize = 30;

/// How far inside the text column's edge a margin folio may reach: a digit's side bearing.
const MARGIN_SLACK_PT: f32 = 1.0;

/// One line found to be the page's printed number.
#[derive(Clone, Debug, PartialEq)]
struct Folio {
    page: usize,
    line: usize,
    band: Band,
    label: String,
}

/// A band line that is nothing but a number, or what a scan made of one.
struct Numeral {
    page: usize,
    line: usize,
    band: Band,
    baseline_y: f32,
    /// Arabic or roman, and its value; `None` for a token that is only digit-shaped.
    reading: Option<(bool, i64)>,
    text: String,
}

/// Find the page numbers: band lines holding a number that is the page's index plus a
/// constant.
///
/// The constant is what tells a folio from every other number a page carries in its margins —
/// a chapter number, a footnote marker, a year in a running head — and it is language-free.
/// It is taken per numbering system, arabic and roman apart, so a book numbered `i`–`xii` and
/// then `1`–`300` has two, and it may change where a book skips numbers over unnumbered
/// plates: every constant that holds on `layout.furniture.min_repeat_pages` pages is one.
/// A folio is kept however its value compares with its neighbours', so a chapter opening
/// that prints no number costs that one page its label and nothing else.
///
/// A scanned book's text layer misreads digits (`2ı`, `3l`). A digit-shaped line at the height
/// the book prints its folios, on a page next to pages whose folios were read, is taken as the
/// folio the constant says it is.
fn find_folios(pages: &[PageLines], body_size: f32, t: &Thresholds) -> Vec<Folio> {
    let band_ratio = t.layout.furniture.band_ratio as f32;
    let min_pages = usize::try_from(t.layout.furniture.min_repeat_pages.max(2)).unwrap_or(2);
    let y_tolerance = t.layout.furniture.y_cluster_ratio as f32 * body_size.max(1.0);

    let mut numerals: Vec<Numeral> = Vec::new();
    for (page_index, page) in pages.iter().enumerate() {
        let band_height = band_ratio * page.page_height_pt;
        // The text column's own edges, from its long lines: a folio set in the outer margin,
        // beside the text rather than above or below it, is outside them.
        let long: Vec<&LineText> = page
            .lines
            .iter()
            .filter(|line| line.text.chars().count() >= MARGIN_REFERENCE_CHARS)
            .collect();
        let column = (!long.is_empty()).then(|| {
            (
                long.iter()
                    .map(|line| line.bbox.x0)
                    .fold(f32::MAX, f32::min),
                long.iter()
                    .map(|line| line.bbox.x1)
                    .fold(f32::MIN, f32::max),
            )
        });
        for (line_index, line) in page.lines.iter().enumerate() {
            let in_margin = column.is_some_and(|(left, right)| {
                line.bbox.x1 <= left + MARGIN_SLACK_PT || line.bbox.x0 >= right - MARGIN_SLACK_PT
            });
            let band = if line.bbox.y1 <= band_height {
                Band::Top
            } else if line.bbox.y0 >= page.page_height_pt - band_height {
                Band::Bottom
            } else if in_margin {
                Band::Body
            } else {
                continue;
            };
            let token: String = line
                .text
                .chars()
                .filter(|ch| ch.is_alphanumeric())
                .collect();
            // The decoration a folio is printed with — `- 12 -`, `[12]`, `12 |` — is dropped;
            // anything with a letter in it that is not a numeral is a running head.
            if token.is_empty() || token.chars().count() > 5 {
                continue;
            }
            let reading = if token.chars().all(|ch| ch.is_ascii_digit()) {
                token.parse().ok().map(|value| (true, value))
            } else if token.chars().all(|ch| ch.is_ascii_lowercase())
                || token.chars().all(|ch| ch.is_ascii_uppercase())
            {
                roman_value(&token).map(|value| (false, value))
            } else {
                None
            };
            if reading.is_none() && !digit_shaped(&token) {
                continue;
            }
            numerals.push(Numeral {
                page: page_index,
                line: line_index,
                band,
                baseline_y: line.baseline_y,
                reading,
                text: line.text.trim().to_owned(),
            });
        }
    }

    // Every constant, per numbering system, that enough pages agree on.
    let mut support: BTreeMap<(bool, i64), usize> = BTreeMap::new();
    for numeral in &numerals {
        if let Some((arabic, value)) = numeral.reading {
            let page = i64::try_from(numeral.page).unwrap_or(i64::MAX);
            *support.entry((arabic, value - page)).or_default() += 1;
        }
    }
    // A constant has to hold on enough pages, and on a real share of its numbering system's
    // readings: three footnote markers that happen to count up with the page are not a
    // pagination.
    let share = t.layout.furniture.folio_constant_min_share;
    let mut readings: BTreeMap<bool, usize> = BTreeMap::new();
    for ((arabic, _), count) in &support {
        *readings.entry(*arabic).or_default() += count;
    }
    let accepted: std::collections::BTreeSet<(bool, i64)> = support
        .iter()
        .filter(|((arabic, _), count)| {
            let total = readings.get(arabic).copied().unwrap_or_default();
            **count >= min_pages && (**count as f64) >= share * total as f64
        })
        .map(|(key, _)| *key)
        .collect();

    let mut folios: Vec<Folio> = Vec::new();
    let mut per_page: BTreeMap<usize, (Band, f32, i64)> = BTreeMap::new();
    for numeral in &numerals {
        let Some((arabic, value)) = numeral.reading else {
            continue;
        };
        let page = i64::try_from(numeral.page).unwrap_or(i64::MAX);
        if !accepted.contains(&(arabic, value - page)) || per_page.contains_key(&numeral.page) {
            continue;
        }
        per_page.insert(
            numeral.page,
            (numeral.band, numeral.baseline_y, value - page),
        );
        folios.push(Folio {
            page: numeral.page,
            line: numeral.line,
            band: numeral.band,
            label: numeral.text.clone(),
        });
    }
    if folios.is_empty() {
        return folios;
    }

    // Misread folios: digit-shaped, where the neighbours print theirs, labelled by the
    // neighbours' constant.
    let reach = 2usize;
    for numeral in &numerals {
        if per_page.contains_key(&numeral.page) || numeral.reading.is_some() {
            continue;
        }
        let neighbour = (numeral.page.saturating_sub(reach)..=numeral.page + reach)
            .filter(|page| *page != numeral.page)
            .filter_map(|page| per_page.get(&page).copied())
            .find(|(band, baseline, _)| {
                *band == numeral.band && (baseline - numeral.baseline_y).abs() <= y_tolerance
            });
        let Some((band, _, constant)) = neighbour else {
            continue;
        };
        let value = i64::try_from(numeral.page).unwrap_or(i64::MAX) + constant;
        if value <= 0 {
            continue;
        }
        per_page.insert(numeral.page, (band, numeral.baseline_y, constant));
        folios.push(Folio {
            page: numeral.page,
            line: numeral.line,
            band,
            label: value.to_string(),
        });
    }
    folios.sort_by_key(|folio| (folio.page, folio.line));
    folios
}

/// A short token made of digits and of the letters a text layer reads digits as — `ı`, `l`,
/// `I`, `O`, `o`, `S` — with at least one real digit in it.
fn digit_shaped(token: &str) -> bool {
    token.chars().any(|ch| ch.is_ascii_digit())
        && token.chars().all(|ch| {
            ch.is_ascii_digit() || matches!(ch, 'ı' | 'l' | 'I' | 'i' | 'O' | 'o' | 'S' | 's' | 'B')
        })
}

// ---------------------------------------------------------------------------
// Text handling
// ---------------------------------------------------------------------------

/// The folded form with digit runs collapsed to `#` and everything that is not a letter,
/// a digit marker or a mark thrown away.
///
/// Masking is the step R2 §B.4 singles out: without it `Page 12` and `Page 137` are different
/// strings and a running foot is invisible.
fn mask(text: &str) -> String {
    let mut out = String::new();
    let mut in_digits = false;
    for ch in text.chars() {
        if ch.is_numeric() {
            if !in_digits {
                out.push('#');
                in_digits = true;
            }
            continue;
        }
        in_digits = false;
        if ch.is_alphabetic() {
            out.push(ch);
        }
    }
    out
}

/// The value of a band line that is nothing but a number, arabic or roman.
///
/// Roman numerals are not an ornament here: front matter is numbered in them and restarts the
/// count, so a detector that only knows arabic leaves every preface page numbered.
fn numeric_value(text: &str) -> Option<i64> {
    let trimmed: String = text
        .chars()
        .filter(|ch| !ch.is_whitespace() && *ch != '.')
        .collect();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.chars().all(|ch| ch.is_ascii_digit()) {
        return trimmed.parse().ok();
    }
    roman_value(&trimmed)
}

fn roman_value(text: &str) -> Option<i64> {
    const DIGITS: [(char, i64); 7] = [
        ('i', 1),
        ('v', 5),
        ('x', 10),
        ('l', 50),
        ('c', 100),
        ('d', 500),
        ('m', 1000),
    ];
    let lowered = text.to_lowercase();
    let mut values = Vec::new();
    for ch in lowered.chars() {
        let (_, value) = DIGITS.iter().find(|(digit, _)| *digit == ch)?;
        values.push(*value);
    }
    if values.is_empty() {
        return None;
    }
    let mut total = 0i64;
    for (index, value) in values.iter().enumerate() {
        let subtractive = values[index + 1..].iter().any(|later| later > value);
        total += if subtractive { -value } else { *value };
    }
    Some(total)
}

fn is_terminal_punctuation(ch: char) -> bool {
    matches!(
        ch,
        '.' | '!' | '?' | '…' | '"' | '\u{201D}' | '\u{00BB}' | ':' | ';'
    )
}

/// The document's body size: the size the most characters are set in.
fn modal_body_size(pages: &[PageLines]) -> f32 {
    let mut weights: BTreeMap<u32, usize> = BTreeMap::new();
    for page in pages {
        for line in &page.lines {
            *weights.entry(line.size_pt.to_bits()).or_default() += line.text.chars().count();
        }
    }
    weights
        .into_iter()
        .max_by_key(|(_, weight)| *weight)
        .map(|(bits, _)| f32::from_bits(bits))
        .unwrap_or_default()
}

/// Where each page's lines start in the flattened verdict vector.
fn line_offsets(pages: &[PageLines]) -> Vec<usize> {
    let mut offsets = Vec::with_capacity(pages.len());
    let mut total = 0usize;
    for page in pages {
        offsets.push(total);
        total += page.lines.len();
    }
    offsets
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn digits_mask_so_page_twelve_and_page_one_hundred_match() {
    assert_eq!(mask("page 12"), "page#");
    assert_eq!(mask("page 137"), "page#");
    assert_eq!(mask("12"), "#");
    assert_eq!(mask("the test book"), "thetestbook");
}

#[test]
fn roman_numerals_are_page_numbers_too() {
    assert_eq!(numeric_value("iv"), Some(4));
    assert_eq!(numeric_value("XII"), Some(12));
    assert_eq!(numeric_value("ix."), Some(9));
    assert_eq!(numeric_value("7"), Some(7));
    assert_eq!(numeric_value("Chapter"), None);
    // `mix` is a word and also, read as a numeral, 1009. Front matter does not reach 1009,
    // but nothing here needs it to: the progression test is what decides, and a book whose
    // band lines read `mix`, `did`, `mild` will not produce one.
    assert_eq!(numeric_value("Chapter 3"), None);
}

#[test]
fn edit_distance_is_normalised_by_length() {
    assert_eq!(normalised_edit_distance("abc", "abc"), 0.0);
    assert!(normalised_edit_distance("chapterone", "chaptertwo") > 0.15);
    assert!(normalised_edit_distance("thetestbook", "thetestbok") < 0.15);
}

#[cfg(test)]
mod folio_tests {
    use super::*;
    use oc_core::thresholds::T;

    fn page(index: u32, lines: &[(String, f32)]) -> PageLines {
        PageLines {
            page: PageRef::new(index),
            page_height_pt: 500.0,
            lines: lines
                .iter()
                .map(|(text, y)| LineText {
                    text: text.clone(),
                    bbox: Rect {
                        x0: 100.0,
                        y0: *y,
                        x1: 140.0,
                        y1: y + 9.0,
                    },
                    baseline_y: y + 8.0,
                    size_pt: 10.0,
                })
                .collect(),
        }
    }

    /// Folios at the foot, two digits then three, one chapter opening with none and one misread
    /// by the text layer: every printed one is found, and the misread one is labelled by the
    /// constant its neighbours agree on.
    #[test]
    fn page_numbers_are_the_page_index_plus_a_constant() {
        let pages: Vec<PageLines> = (0..120u32)
            .map(|index| {
                let folio = match index {
                    40 => None,
                    60 => Some("6\u{131}".to_owned()),
                    _ => Some((index + 5).to_string()),
                };
                let mut lines = vec![("body text of the page, set full.".to_owned(), 100.0)];
                if let Some(folio) = folio {
                    lines.push((folio, 480.0));
                }
                page(index, &lines)
            })
            .collect();
        let folios = find_folios(&pages, 10.0, &T);
        assert_eq!(folios.len(), 119, "every page but the opening");
        let misread = folios
            .iter()
            .find(|folio| folio.page == 60)
            .expect("the misread folio");
        assert_eq!(misread.label, "65");
        assert!(folios.iter().any(|folio| folio.label == "124"));
    }

    /// Numbers in the margin that are not the page's index plus a constant — footnote markers,
    /// a chapter number — are not page numbers.
    #[test]
    fn numbers_that_do_not_track_the_page_are_not_folios() {
        let pages: Vec<PageLines> = (0..30u32)
            .map(|index| {
                let marker = ["1", "2", "3"][(index % 3) as usize];
                page(
                    index,
                    &[("body".to_owned(), 100.0), (marker.to_owned(), 480.0)],
                )
            })
            .collect();
        assert!(find_folios(&pages, 10.0, &T).is_empty());
    }
}

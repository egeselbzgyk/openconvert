//! The `text` and `furniture` stages, driven end to end under the conservation law.
//!
//! Each stage is run and then immediately checked: `C(D_before) ⊎ Added == C(D_after) ⊎
//! Removed`, the reasons it cited are ones it declared, and its cumulative removals are
//! inside their budgets (ARCHITECTURE §5.4). The check is not an assertion in a test — it
//! runs on every conversion, and a violation is an error rather than a warning, because a
//! conservation law that can be disabled is a preference (D13.4).

use oc_core::ledger_check::{c_of, check_invariants, ConservationError, ReasonTotals};
use oc_core::stages;
use oc_core::thresholds::Thresholds;
use oc_layout::furniture::{apply_furniture, detect_furniture, PageLines};
use oc_model::extract::{CharHistogram, Glyph, PageRef};
use oc_model::lang::LangTag;
use oc_model::ledger::{LedgerDelta, StageCheck};
use oc_model::text::{Line, Run};
use oc_text::lines::assemble_lines;
use oc_text::normalize::{normalize, LedgerSite};
use oc_text::words::assemble_runs;

/// One page as `ingest` leaves it.
#[derive(Clone, Debug, PartialEq)]
pub struct PageInput {
    pub page: PageRef,
    pub height_pt: f32,
    pub glyphs: Vec<Glyph>,
}

/// One page as `text` leaves it.
#[derive(Clone, Debug, PartialEq)]
pub struct TextPage {
    pub page: PageRef,
    pub height_pt: f32,
    pub runs: Vec<Run>,
    pub lines: Vec<Line>,
}

/// What `text` produced, and the check that says it did not lose anything.
#[derive(Clone, Debug)]
pub struct TextStage {
    pub pages: Vec<TextPage>,
    pub delta: LedgerDelta,
    /// `C_0`, the retention denominator: the multiset after `N` (ARCHITECTURE §5.2). Overdraw
    /// and OCR-layer dedup, the other two things folded into `C_0`, happen in `ingest` and in
    /// Phase 13; when they arrive they subtract from this same quantity.
    pub c_0: CharHistogram,
    pub check: StageCheck,
}

/// What `furniture` produced.
#[derive(Clone, Debug)]
pub struct FurnitureStage {
    pub pages: Vec<PageLines>,
    pub delta: LedgerDelta,
    /// The printed page number recovered per page, bound for `PageRef.label` and `page-list`.
    pub labels: Vec<Option<String>>,
    pub check: StageCheck,
}

/// Run `text`: assemble glyphs into runs and lines, then apply `N` exactly once.
///
/// The order is the one PIPELINE §4 insists on. Superscript and subscript are read from
/// geometry during assembly, *before* normalisation, because a normalisation that folded `¹`
/// to `1` would destroy the signal in the same pass — which is why NFKC is banned and why
/// this ordering is a property of the stage rather than an implementation detail.
pub fn text_stage(
    input: &[PageInput],
    totals: &mut ReasonTotals,
    t: &Thresholds,
) -> Result<TextStage, ConservationError> {
    let before = glyph_chars(input);

    let mut delta = LedgerDelta::default();
    let mut pages = Vec::with_capacity(input.len());
    for page in input {
        let mut assembly = assemble_runs(&page.glyphs, page.page.clone(), t);
        let mut offset: u32 = 0;
        for run in &mut assembly.runs {
            let width = u32::try_from(run.text.chars().count()).unwrap_or(u32::MAX);
            let site = LedgerSite {
                stage: stages::TEXT.name,
                page: page.page.index,
                char_offset: offset,
            };
            run.text = normalize(&run.text, &mut delta, site).into_string();
            offset = offset.saturating_add(width);
        }
        let lines = assemble_lines(&assembly.runs, t);
        pages.push(TextPage {
            page: page.page.clone(),
            height_pt: page.height_pt,
            runs: assembly.runs,
            lines,
        });
    }

    let after = run_chars(&pages);
    // `C_0` is defined *after* `N`, so the account the budgets are fractions of is opened
    // here rather than at extraction (ARCHITECTURE §5.2).
    *totals = ReasonTotals::new(&after);
    let check = check_invariants(&before, &after, &delta, stages::TEXT, totals)?;

    Ok(TextStage {
        pages,
        delta,
        c_0: after,
        check,
    })
}

/// Run `furniture`: find the running heads, feet and page numbers, and remove them.
pub fn furniture_stage(
    text: &TextStage,
    lang: LangTag,
    totals: &mut ReasonTotals,
    t: &Thresholds,
) -> Result<FurnitureStage, ConservationError> {
    let before = run_chars(&text.pages);

    let pages: Vec<PageLines> = text
        .pages
        .iter()
        .map(|page| {
            PageLines::from_runs(page.page.clone(), page.height_pt, &page.runs, &page.lines)
        })
        .collect();
    let verdicts = detect_furniture(&pages, lang, t);
    let outcome = apply_furniture(&pages, &verdicts);

    let after = line_chars(&outcome.pages);
    let check = check_invariants(&before, &after, &outcome.delta, stages::FURNITURE, totals)?;

    Ok(FurnitureStage {
        pages: outcome.pages,
        delta: outcome.delta,
        labels: outcome.labels,
        check,
    })
}

/// `C` of the glyph stream: the document as extraction left it.
pub fn glyph_chars(input: &[PageInput]) -> CharHistogram {
    let mut histogram = CharHistogram::new();
    for page in input {
        for glyph in &page.glyphs {
            if !glyph.ch.is_whitespace() {
                histogram.add(glyph.ch);
            }
        }
    }
    histogram
}

/// `C` of the assembled runs.
pub fn run_chars(pages: &[TextPage]) -> CharHistogram {
    let mut histogram = CharHistogram::new();
    for page in pages {
        for run in &page.runs {
            histogram = histogram.union(&c_of(&run.text));
        }
    }
    histogram
}

/// `C` of the surviving body flow.
pub fn line_chars(pages: &[PageLines]) -> CharHistogram {
    let mut histogram = CharHistogram::new();
    for page in pages {
        for line in &page.lines {
            histogram = histogram.union(&c_of(&line.text));
        }
    }
    histogram
}

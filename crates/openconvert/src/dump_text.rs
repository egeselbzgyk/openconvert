//! The text layer as canonical JSON: `dump-stage text` (PIPELINE §4, Phase 2 test 2.21).
//!
//! The counterpart to `oc_pdf::dump` one stage later. Where the `ingest` dump answers "what
//! did the backend give us", this answers the next question a wrong conversion raises: **what
//! did we make of it** — which glyphs became which run, where the spaces came from, what `N`
//! took out and put back, and how the page scored on the statistics that route it.
//!
//! Unlike the `ingest` dump this one cannot stream from the first page. Two of the things it
//! reports are document-wide by definition — `dc:language` is detected over the whole body,
//! and `C_0` is not final until every page has been through `N` — so the stage runs to
//! completion and the pages are written from the result. The cost is bounded and much smaller
//! than the ingest dump's: a book's runs are a fraction of its glyphs, since a run is one
//! string and a bounding box where the glyphs were one of each per character.

use serde::Serialize;

use oc_core::ledger_check::{ConservationError, ReasonTotals};
use oc_core::thresholds::Thresholds;
use oc_model::extract::CharHistogram;
use oc_model::lang::LangTag;
use oc_model::ledger::{LedgerEntry, StageCheck};
use oc_model::text::{Line, Run};
use oc_text::lang::detect_document;
use oc_text::stats::{quality_stats, QualityStats, Region, Verdict};

use crate::pipeline::{text_stage, PageInput, TextStage};

/// The stage name this dump belongs to. One of the twelve fixed stage names.
pub const STAGE: &str = "text";

/// The schema tag every dump carries, so a consumer can refuse a shape it does not know.
const SCHEMA: &str = "openconvert.dump.text/1";

/// What the document as a whole contributes.
#[derive(Clone, Debug, Serialize)]
pub struct DumpHeader {
    pub schema: &'static str,
    pub stage: &'static str,
    /// First in the output, by D13.3 — a reader has to know the version before the shape.
    pub ir_version: u32,
    pub pages: u32,
    /// `dc:language`, detected over the concatenated body (D13.11).
    pub language: LangTag,
    pub language_confidence: f32,
    /// The detector declined and the job's locale was used.
    pub language_fell_back: bool,
    /// `|C_0|`: the retention denominator, final only once every page has been through `N`.
    pub c_0_total: u64,
    /// Whether a frequency list exists for the detected language, and so whether the
    /// dictionary hit rate below is a measurement or an absence.
    pub have_frequency_list: bool,
    /// What the conservation checker found after this stage.
    pub check: StageCheck,
}

/// One page's text layer.
#[derive(Clone, Debug, Serialize)]
pub struct DumpPage {
    pub index: u32,
    pub height_pt: f32,
    pub runs: Vec<Run>,
    pub lines: Vec<Line>,
    /// What `N` removed from this page and added to it, in document order.
    pub ledger: Vec<LedgerEntry>,
    pub quality: QualityStats,
    pub verdict: Verdict,
}

/// Run the stage and describe it.
///
/// `fallback` is the job's locale, used for `dc:language` when nothing can be detected —
/// EPUB 3.3 requires the field, so there is no "unknown" to emit.
pub fn dump(
    input: &[PageInput],
    fallback: LangTag,
    t: &Thresholds,
) -> Result<(DumpHeader, Vec<DumpPage>), ConservationError> {
    let mut totals = ReasonTotals::default();
    let stage = text_stage(input, &mut totals, t)?;

    let body = body_text(&stage);
    let language = detect_document(&body, fallback);
    let have_list = oc_text::freq::have_list(&language.lang);

    let header = DumpHeader {
        schema: SCHEMA,
        stage: STAGE,
        ir_version: oc_model::IR_VERSION,
        pages: u32::try_from(stage.pages.len()).unwrap_or(u32::MAX),
        language: language.lang.clone(),
        language_confidence: language.confidence,
        language_fell_back: language.fell_back,
        c_0_total: stage.c_0.total(),
        have_frequency_list: have_list,
        check: stage.check.clone(),
    };

    let pages = stage
        .pages
        .iter()
        .map(|page| {
            let text = page_text(page);
            let quality = quality_stats(
                &text,
                language.lang.clone(),
                Region::Page(page.page.index),
                oc_text::freq::dict_hit_rate(&text, &language.lang),
            );
            DumpPage {
                index: page.page.index,
                height_pt: page.height_pt,
                runs: page.runs.clone(),
                lines: page.lines.clone(),
                ledger: stage
                    .delta
                    .entries()
                    .iter()
                    .filter(|entry| entry.page == page.page.index)
                    .cloned()
                    .collect(),
                verdict: quality.verdict(t),
                quality,
            }
        })
        .collect();

    Ok((header, pages))
}

/// `C_0` for a finished stage, for a caller that wants the histogram rather than its total.
pub fn c_0(stage: &TextStage) -> &CharHistogram {
    &stage.c_0
}

/// One page's text, lines joined — the form the statistics and the language detector read.
pub fn page_text(page: &crate::pipeline::TextPage) -> String {
    page.lines
        .iter()
        .map(|line| {
            line.runs
                .iter()
                .filter_map(|id| page.runs.get(id.0 as usize))
                .map(|run| run.text.as_str())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn body_text(stage: &TextStage) -> String {
    stage
        .pages
        .iter()
        .map(page_text)
        .collect::<Vec<_>>()
        .join("\n")
}

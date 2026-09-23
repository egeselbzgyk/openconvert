//! OCR inside `ingest` (PIPELINE §3, IMPLEMENTATION_PLAN PHASE 13 details 3, 7–11).
//!
//! **Routing follows the page class, never guesswork** (D13.10):
//!
//! | class | what is read |
//! |---|---|
//! | `ImageOnly` | the whole page, `--psm 1`, in bands that avoid any stray PDF glyph |
//! | `Mixed` | each image region no text run touches, `--psm 6` |
//! | `OcrSandwich` | nothing by default — the file's own layer is used, `provenance = OcrLayer`; with `--re-ocr` the page is read whole and the old layer is removed under `OcrLayerDuplicate` |
//! | `BrokenText` | nothing in v1 — see `docs/DECISIONS_LOG.md`, 2026-09-23 (PROVISIONAL) |
//! | `Text`, `Blank` | nothing, unless `--ocr always` |
//!
//! **Every OCR entry is `Added` and region-scoped.** The stage is checked like every other: I-1
//! over the characters before and after it, I-2 against `ingest`'s declared reasons, and I-6 —
//! every `Ocr` region contains no text run the page had before (ratified note N-1).
//!
//! **No engine, a hung engine, a crashed engine: the book still converts** (D4, A13.2, A13.5). The
//! regions that would have been read stay pictures, a warning says why, and the exit code is 0.
//!
//! **No language model is anywhere on this path** (D16, R10 §6.14). Nothing here takes a provider,
//! and `ocr_never_calls_the_llm` holds the module to it.

use std::collections::BTreeSet;
use std::sync::Arc;

use oc_core::ledger_check::{c_of, check_i6, check_invariants, ConservationError, ReasonTotals};
use oc_core::ocr::discover::{engine_install_hint, OcrUnavailable};
use oc_core::ocr::invoke::{OcrEngine, OcrRequest};
use oc_core::ocr::lang::{select_explicit, select_langs, stack_second, LangSpec};
use oc_core::ocr::merge::{
    clean_bands, merge_ocr_runs, region_confidence, split_by_bands, OcrPage,
};
use oc_core::ocr::W_OCR_LOW_CONFIDENCE;
use oc_core::ocr::{OcrMode, OcrScope, OcrWord, Os, ReOcr, W_OCR_ENGINE_MISSING, W_OCR_FAILED};
use oc_core::stages;
use oc_core::thresholds::Thresholds;
use oc_model::doc::{Severity, Warning};
use oc_model::extract::{FontId, FontInfo, Glyph, ImageId, ImageKind};
use oc_model::geom::Rect;
use oc_model::lang::LangTag;
use oc_model::ledger::{LedgerDelta, LedgerEntry, Reason, StageCheck};
use oc_pdf::classify::PageClass;
use oc_pdf::inspect::PdfDoc;
use serde::Serialize;

use crate::pipeline::PageInput;

/// The name of the synthetic font every OCR run points at. OCR recovers text, not typefaces.
pub const OCR_FONT: &str = "OCR";

/// What the caller chose about OCR.
#[derive(Clone)]
pub struct OcrOptions {
    pub mode: OcrMode,
    pub re_ocr: ReOcr,
    /// The engine, or why there is none. Discovery happened once, in the caller (detail 1).
    pub engine: Result<Arc<dyn OcrEngine>, OcrUnavailable>,
    /// `--ocr-lang`, when given. Otherwise the document's language decides (detail 5).
    pub langs: Option<LangSpec>,
    /// `ocr.render_dpi`.
    pub dpi: u32,
}

impl OcrOptions {
    /// No OCR at all: what every test that is not about OCR converts with, so that no result in
    /// the suite depends on whether the machine running it has Tesseract installed.
    pub fn off() -> Self {
        Self {
            mode: OcrMode::Never,
            re_ocr: ReOcr::Never,
            engine: Err(OcrUnavailable::NotFound),
            langs: None,
            dpi: 0,
        }
    }

    /// Auto routing with `engine`, at `ocr.render_dpi`.
    pub fn auto(engine: Result<Arc<dyn OcrEngine>, OcrUnavailable>, t: &Thresholds) -> Self {
        Self {
            mode: OcrMode::Auto,
            re_ocr: ReOcr::Never,
            engine,
            langs: None,
            dpi: u32::try_from(t.ocr.render_dpi).unwrap_or_default(),
        }
    }
}

/// How one region ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionOutcome {
    /// Read; the text replaces the picture.
    Text,
    /// Read under `ocr.region_conf_min`: the text is kept and the picture is emitted beside it.
    TextAndImage,
    /// Nothing legible in it: the picture stays and nothing is added.
    Empty,
    /// Not read — no engine, or the engine failed. The picture stays.
    Image,
}

/// One region, as the report states it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RegionReport {
    /// Zero-based page index.
    pub page: u32,
    pub scope: &'static str,
    pub region: [f32; 4],
    pub words: usize,
    /// Mean word confidence, `0.0..=1.0`.
    pub mean_confidence: f32,
    /// Words under `ocr.word_conf_min`, kept and flagged.
    pub below_floor: usize,
    /// Words dropped because they sat on a line of text the PDF already carries (I-6 banding).
    pub dropped_words: usize,
    pub outcome: RegionOutcome,
}

/// The OCR half of the report.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OcrReport {
    pub mode: &'static str,
    pub re_ocr: &'static str,
    /// `ocr:tesseract-5.3.4`, or `None` with `unavailable` saying why.
    pub engine: Option<String>,
    pub unavailable: Option<String>,
    /// The `-l` value used, when anything was read.
    pub langs: Option<String>,
    pub regions: Vec<RegionReport>,
    /// Zero-based indices of the pages whose scanned content stayed a picture.
    pub pages_as_images: Vec<u32>,
    /// Characters OCR added, excluded from source retention (I-6).
    pub ocr_chars: u64,
}

/// What OCR did to the pages, and the check that says it broke no law.
#[derive(Clone, Debug)]
pub struct OcrStage {
    pub delta: LedgerDelta,
    pub check: StageCheck,
    pub warnings: Vec<Warning>,
    /// `None` when no page needed OCR, so a born-digital book's report is what it always was.
    pub report: Option<OcrReport>,
}

/// One planned OCR call.
struct Plan {
    page: usize,
    scope: OcrScope,
    /// The region to read.
    region: Rect,
    /// For an image region, which image it is (its page-local id).
    image: Option<ImageId>,
    /// Re-OCR of a sandwich: the layer is replaced if the call succeeds.
    replaces_layer: bool,
}

/// Run OCR over `input` in place: add runs, drop the pictures they replace, and check the result.
pub fn ocr_stage(
    pdf: &dyn PdfDoc,
    input: &mut [PageInput],
    options: &OcrOptions,
    language: Option<&LangTag>,
    t: &Thresholds,
) -> Result<OcrStage, ConservationError> {
    let before = c_of(&all_glyph_text(input));
    let plans = plan(input, options, language);

    let mut delta = LedgerDelta::default();
    let mut warnings = Vec::new();
    let mut regions = Vec::new();
    let mut as_images: BTreeSet<u32> = BTreeSet::new();
    let mut langs_used: Option<String> = None;

    if !plans.is_empty() {
        match &options.engine {
            Err(why) => {
                for plan in &plans {
                    as_images.insert(page_index(input, plan.page));
                    regions.push(unread(input, plan, RegionOutcome::Image));
                }
                warnings.push(
                    Warning::new(W_OCR_ENGINE_MISSING, Severity::Warn)
                        .with_arg("reason", why.to_string())
                        .with_arg("hint", engine_install_hint(Os::current()))
                        .with_arg("pages", page_list(&as_images)),
                );
            }
            Ok(engine) => {
                let (langs, lang_warnings) =
                    choose_langs(input, options, language, engine.as_ref(), t);
                warnings.extend(lang_warnings);
                langs_used = Some(langs.arg());
                for plan in &plans {
                    let outcome =
                        read_region(pdf, input, plan, engine.as_ref(), &langs, options, t);
                    match outcome {
                        Ok(read) => {
                            if matches!(read.report.outcome, RegionOutcome::Empty) {
                                as_images.insert(page_index(input, plan.page));
                            }
                            if matches!(read.report.outcome, RegionOutcome::TextAndImage) {
                                warnings.push(
                                    Warning::new(W_OCR_LOW_CONFIDENCE, Severity::Warn)
                                        .with_arg(
                                            "page",
                                            (page_index(input, plan.page) + 1).to_string(),
                                        )
                                        .with_arg(
                                            "confidence",
                                            format!("{:.2}", read.report.mean_confidence),
                                        )
                                        .with_arg("floor", format!("{:.2}", t.ocr.region_conf_min)),
                                );
                            }
                            for entry in read.entries {
                                delta.push(entry);
                            }
                            regions.push(read.report);
                        }
                        Err(reason) => {
                            as_images.insert(page_index(input, plan.page));
                            warnings.push(
                                Warning::new(W_OCR_FAILED, Severity::Warn)
                                    .with_arg(
                                        "page",
                                        (page_index(input, plan.page) + 1).to_string(),
                                    )
                                    .with_arg("reason", reason),
                            );
                            regions.push(unread(input, plan, RegionOutcome::Image));
                        }
                    }
                }
            }
        }
    }

    // The stage is checked like every other: I-1 and I-2 over what it did, then I-6 over where.
    // Budgets are not charged here: `C_0` does not exist until `text` (PIPELINE §3), so the account
    // is an empty one. The retention it records is the extracted text's — what is left of the
    // glyphs, OCR's additions taken out — over what extraction found.
    let after = c_of(&all_text(input));
    let mut account = ReasonTotals::default();
    let mut check = check_invariants(&before, &after, &delta, stages::INGEST, &mut account)?;
    check.retention = if before.total() == 0 {
        0.0
    } else {
        after.total().saturating_sub(account.ocr_added()) as f32 / before.total() as f32
    };
    let pre_existing: Vec<(u32, Rect)> = input
        .iter()
        .flat_map(|page| {
            page.glyphs
                .iter()
                .map(move |glyph| (page.page.index, glyph.bbox))
        })
        .collect();
    check_i6(delta.entries(), &pre_existing)?;

    let ocr_chars = delta.reason_added(Reason::Ocr).total();
    let report = (!plans.is_empty() || options.mode == OcrMode::Always).then(|| OcrReport {
        mode: options.mode.as_str(),
        re_ocr: options.re_ocr.as_str(),
        engine: options
            .engine
            .as_ref()
            .ok()
            .map(|engine| engine.capability()),
        unavailable: options.engine.as_ref().err().map(ToString::to_string),
        langs: langs_used,
        regions,
        pages_as_images: as_images.into_iter().collect(),
        ocr_chars,
    });
    Ok(OcrStage {
        delta,
        check,
        warnings,
        report,
    })
}

/// Which regions of which pages to read, before any engine is asked.
fn plan(input: &[PageInput], options: &OcrOptions, language: Option<&LangTag>) -> Vec<Plan> {
    if options.mode == OcrMode::Never {
        return Vec::new();
    }
    let mut plans = Vec::new();
    for (index, page) in input.iter().enumerate() {
        let whole = Rect {
            x0: 0.0,
            y0: 0.0,
            x1: page.width_pt,
            y1: page.height_pt,
        };
        let full = |replaces_layer| Plan {
            page: index,
            scope: OcrScope::FullPage,
            region: whole,
            image: None,
            replaces_layer,
        };
        match (options.mode, page.class) {
            (_, PageClass::OcrSandwich) => {
                if re_ocr_sandwich(page, options.re_ocr, language) {
                    plans.push(full(true));
                } else if options.mode == OcrMode::Always {
                    // The layer is the page's text; `always` reads what it does not cover.
                    plans.push(full(false));
                }
            }
            (OcrMode::Always, _) | (_, PageClass::ImageOnly) => plans.push(full(false)),
            (_, PageClass::Mixed) => {
                for image in &page.images {
                    if !matches!(
                        image.kind,
                        ImageKind::Figure | ImageKind::FullPageBackground | ImageKind::Unknown
                    ) {
                        continue;
                    }
                    let region = clip(image.bbox, whole);
                    if region.x1 <= region.x0 || region.y1 <= region.y0 {
                        continue;
                    }
                    let covered = page
                        .glyphs
                        .iter()
                        .any(|glyph| oc_core::ledger_check::overlaps(&glyph.bbox, &region));
                    if !covered {
                        plans.push(Plan {
                            page: index,
                            scope: OcrScope::ImageRegion,
                            region,
                            image: Some(image.id),
                            replaces_layer: false,
                        });
                    }
                }
            }
            // `BrokenText` is not read in v1: its broken layer is visible text, so a whole-page OCR
            // region would contain it and fail I-6, and no declared reason removes visible text for
            // being unreadable. PROVISIONAL — `docs/DECISIONS_LOG.md`, 2026-09-23.
            (_, PageClass::BrokenText | PageClass::Text | PageClass::Blank) => {}
        }
    }
    plans
}

/// Whether a sandwich page's layer is to be replaced (detail 10).
fn re_ocr_sandwich(page: &PageInput, re_ocr: ReOcr, language: Option<&LangTag>) -> bool {
    match re_ocr {
        ReOcr::Never => false,
        ReOcr::Always => true,
        ReOcr::Auto => {
            let layer: String = page
                .glyphs
                .iter()
                .filter(|glyph| is_layer(glyph))
                .map(|glyph| glyph.ch)
                .collect();
            let lang = language.cloned().unwrap_or(LangTag::EN);
            // No frequency list for the language is no evidence the layer is bad: it is kept.
            oc_text::freq::dict_hit_rate(&layer, &lang).is_some_and(|rate| {
                f64::from(rate) < oc_core::thresholds::T.pageclass.broken_text_dict_hit_min
            })
        }
    }
}

/// A glyph of an OCR layer: drawn in render mode 3, or with a fully transparent fill. On a page that
/// is not a sandwich such glyphs were already removed as `HiddenText`, so only a layer is left.
pub fn is_layer(glyph: &Glyph) -> bool {
    /// PDF text rendering mode 3: neither filled nor stroked.
    const RENDER_MODE_INVISIBLE: u8 = 3;
    /// A fill with no opacity at all.
    const TRANSPARENT: u8 = 0;
    let [_, _, _, alpha] = glyph.fill;
    glyph.render_mode == RENDER_MODE_INVISIBLE || alpha == TRANSPARENT
}

/// What one region's read produced.
struct Read {
    entries: Vec<LedgerEntry>,
    report: RegionReport,
}

/// Read one region and merge it into its page. `Err` is a reason, for `W_OCR_FAILED`.
fn read_region(
    pdf: &dyn PdfDoc,
    input: &mut [PageInput],
    plan: &Plan,
    engine: &dyn OcrEngine,
    langs: &LangSpec,
    options: &OcrOptions,
    t: &Thresholds,
) -> Result<Read, String> {
    let Some(page) = input.get(plan.page) else {
        return Err("no such page".to_owned());
    };
    let index = page.page.index;
    let rendered = pdf
        .render_region(index, plan.region, options.dpi)
        .map_err(|error| error.to_string())?;
    let request = OcrRequest {
        raster: rendered.raster,
        dpi: rendered.dpi,
        psm: plan.scope.psm(),
        langs: langs.clone(),
        region_pt: rendered.region_pt,
        page_index: index,
    };
    let words = engine
        .recognize(&request)
        .map_err(|error| error.to_string())?;

    let confidence = region_confidence(&words, t.ocr.word_conf_min as f32);
    let mut report = RegionReport {
        page: index,
        scope: scope_name(plan.scope),
        region: [
            plan.region.x0,
            plan.region.y0,
            plan.region.x1,
            plan.region.y1,
        ],
        words: confidence.words,
        mean_confidence: confidence.mean,
        below_floor: confidence.below_floor,
        dropped_words: 0,
        outcome: RegionOutcome::Empty,
    };
    if words.is_empty() {
        return Ok(Read {
            entries: Vec::new(),
            report,
        });
    }

    let Some(page) = input.get_mut(plan.page) else {
        return Err("no such page".to_owned());
    };
    let mut entries = Vec::new();
    // Re-OCR: the old layer goes, under `OcrLayerDuplicate`, only once the new reading exists —
    // an engine that failed must not leave the page with neither.
    if plan.replaces_layer {
        let (layer, kept): (Vec<Glyph>, Vec<Glyph>) = std::mem::take(&mut page.glyphs)
            .into_iter()
            .partition(is_layer);
        page.glyphs = kept;
        let text: String = layer.iter().map(|glyph| glyph.ch).collect();
        if !text.is_empty() {
            let width = u32::try_from(text.chars().count()).unwrap_or(u32::MAX);
            entries.push(LedgerEntry::removed(
                stages::INGEST.name,
                Reason::OcrLayerDuplicate,
                index,
                (0, width),
                text,
            ));
        }
    }

    // A region is read in bands that avoid every glyph the page still has, so that no `Ocr` region
    // contains pre-existing text (I-6). For an image region nothing touches it and the band is the
    // region itself.
    let obstacles: Vec<Rect> = page.glyphs.iter().map(|glyph| glyph.bbox).collect();
    let bands = match plan.scope {
        OcrScope::FullPage => clean_bands(plan.region, &obstacles),
        OcrScope::ImageRegion => vec![plan.region],
    };
    let (per_band, dropped) = split_by_bands(words, &bands);
    report.dropped_words = dropped;

    let font = ocr_font(page);
    let mut ocr_page = OcrPage::new(page.page.clone(), font);
    for (band, band_words) in bands.iter().zip(per_band) {
        let merged: Vec<OcrWord> = band_words;
        entries.extend(merge_ocr_runs(&mut ocr_page, merged, *band).map_err(|e| e.to_string())?);
    }
    let offset = u32::try_from(page.ocr_runs.len()).unwrap_or(u32::MAX);
    for mut run in ocr_page.runs {
        run.id = oc_model::text::RunId(run.id.0.saturating_add(offset));
        page.ocr_runs.push(run);
    }

    // Detail 9: under the floor, the picture stays beside the text for a reader who does not trust
    // it; otherwise the text replaces the picture.
    if confidence.mean < t.ocr.region_conf_min as f32 {
        report.outcome = RegionOutcome::TextAndImage;
    } else {
        report.outcome = RegionOutcome::Text;
        match plan.image {
            Some(id) => page.images.retain(|image| image.id != id),
            None => page.images.clear(),
        }
    }
    Ok(Read { entries, report })
}

/// The page's synthetic OCR font, added on first use.
fn ocr_font(page: &mut PageInput) -> FontId {
    if let Some(position) = page.fonts.iter().position(|font| font.name == OCR_FONT) {
        return FontId(u16::try_from(position).unwrap_or(u16::MAX));
    }
    let id = FontId(u16::try_from(page.fonts.len()).unwrap_or(u16::MAX));
    page.fonts.push(FontInfo {
        id,
        name: OCR_FONT.to_owned(),
        family_key: OCR_FONT.to_lowercase(),
        serif: false,
        fixed_pitch: false,
        symbolic: false,
        type3: false,
        embedded: false,
    });
    id
}

/// The language data for this book (detail 5): `--ocr-lang` when given, else the document's
/// language, with a second one stacked only past `ocr.second_lang_block_share` of the pages that
/// have text.
fn choose_langs(
    input: &[PageInput],
    options: &OcrOptions,
    language: Option<&LangTag>,
    engine: &dyn OcrEngine,
    t: &Thresholds,
) -> (LangSpec, Vec<Warning>) {
    let installed = engine.langs();
    if let Some(spec) = &options.langs {
        return select_explicit(spec, installed);
    }
    let text = all_glyph_text(input);
    let detected = language
        .cloned()
        .filter(|tag| *tag != LangTag::UND)
        .or_else(|| {
            let found = oc_text::lang::detect_document(&text, LangTag::UND);
            (!found.fell_back).then_some(found.lang)
        });
    let (spec, warnings) = select_langs(detected.as_ref(), installed);
    let page_langs: Vec<LangTag> = input
        .iter()
        .map(|page| page.glyphs.iter().map(|glyph| glyph.ch).collect::<String>())
        .filter(|text| !text.trim().is_empty())
        .filter_map(|text| {
            let found = oc_text::lang::detect_document(&text, LangTag::UND);
            (!found.fell_back).then_some(found.lang)
        })
        .collect();
    (
        stack_second(spec, &page_langs, installed, t.ocr.second_lang_block_share),
        warnings,
    )
}

fn unread(input: &[PageInput], plan: &Plan, outcome: RegionOutcome) -> RegionReport {
    RegionReport {
        page: page_index(input, plan.page),
        scope: scope_name(plan.scope),
        region: [
            plan.region.x0,
            plan.region.y0,
            plan.region.x1,
            plan.region.y1,
        ],
        words: 0,
        mean_confidence: 0.0,
        below_floor: 0,
        dropped_words: 0,
        outcome,
    }
}

fn scope_name(scope: OcrScope) -> &'static str {
    match scope {
        OcrScope::FullPage => "full_page",
        OcrScope::ImageRegion => "image_region",
    }
}

fn page_index(input: &[PageInput], position: usize) -> u32 {
    input
        .get(position)
        .map_or(u32::try_from(position).unwrap_or(u32::MAX), |page| {
            page.page.index
        })
}

/// One-based page numbers, as a reader counts them: `1, 2, 5`.
fn page_list(pages: &BTreeSet<u32>) -> String {
    pages
        .iter()
        .map(|page| page.saturating_add(1).to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn clip(rect: Rect, to: Rect) -> Rect {
    Rect {
        x0: rect.x0.max(to.x0),
        y0: rect.y0.max(to.y0),
        x1: rect.x1.min(to.x1),
        y1: rect.y1.min(to.y1),
    }
}

/// The glyph text of every page, concatenated — `C` of it is the stage's `before`.
fn all_glyph_text(input: &[PageInput]) -> String {
    input
        .iter()
        .flat_map(|page| page.glyphs.iter().map(|glyph| glyph.ch))
        .collect()
}

/// The glyph text and the OCR text of every page — `C` of it is the stage's `after`.
fn all_text(input: &[PageInput]) -> String {
    let mut text = all_glyph_text(input);
    for page in input {
        for run in &page.ocr_runs {
            text.push_str(&run.text);
        }
    }
    text
}

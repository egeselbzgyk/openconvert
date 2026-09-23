//! The versioned conversion report (PIPELINE §13, RT D9).
//!
//! **Its job is to tell the user what happened in terms they can act on, and to give the
//! maintainer enough to reproduce a bug offline.** Those are different audiences and one document,
//! which is why the headline is a single number — the retention ratio — and the rest is the
//! evidence behind it.
//!
//! Two properties are load-bearing and neither is obvious from the shape:
//!
//! - **It is versioned.** `schema: "openconvert.report/1"` is the first key, so a desktop app or a
//!   script reading a report written by a newer engine can refuse rather than misread it.
//! - **Warnings travel as `code` + `args`, never as prose.** A warning is a factual claim about
//!   what the software did, and a paraphrase that misstates it is a trust bug rather than a style
//!   one. The localised templates are in `oc-core::warnings` and the *reader* fills them
//!   (R10 §6.20).
//!
//! The report lives here rather than in `oc-core` as the plan's file list has it: assembling it
//! needs `Tier1Report`, `StructuralReport` and `RepairOutcome`, which are `oc-validate`'s, and
//! `oc-validate` depends on `oc-core`. `openconvert` is the crate that sees every part — it is the
//! orchestrator in this tree, since Phase 5 moved the pipeline into the library.

use std::collections::BTreeMap;

use oc_core::thresholds::{Provenance, PROVENANCE};
use oc_model::doc::Warning;
use oc_model::ledger::{Ledger, Reason, StageCheck};
use serde::Serialize;

use crate::convert::Conversion;

/// The schema this writer produces. Read it before anything else.
pub const SCHEMA: &str = "openconvert.report/1";

/// Whether the conversion produced a book a reading system will accept.
///
/// `invalid` does **not** mean no EPUB was written: after the repair cap "the EPUB is still
/// written, the report is marked `invalid`, and the UI says so plainly rather than reporting
/// success" (D13.7). A slightly invalid EPUB is more useful than none; silently calling it fine is
/// what must not happen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Ok,
    Invalid,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Invalid => "invalid",
        }
    }
}

/// What ran, and with what.
#[derive(Clone, Debug, Serialize)]
pub struct Engine {
    pub version: &'static str,
    pub ir_version: u32,
    pub pdfium_version: String,
    /// The prompt-pack version, when the LLM path ran. `None` is the v1 default, because
    /// `ai.enabled` is false (D17).
    pub prompt_version: Option<String>,
}

/// The file that went in.
#[derive(Clone, Debug, Serialize)]
pub struct Input {
    pub sha256: String,
    pub filename: String,
    pub pages: u32,
    /// The producer family, which is also the stratum the corpus is reported by (D18).
    pub producer_family: String,
}

/// What the pipeline decided the book is.
#[derive(Clone, Debug, Serialize)]
pub struct DocumentSummary {
    pub classification: &'static str,
    pub preset: &'static str,
    pub language: String,
    pub title: Option<String>,
    pub sections: usize,
    pub figures: usize,
    pub tables: usize,
    pub notes: usize,
}

/// One reason's ledger total, with the budget it draws on and what is left of it.
///
/// The headroom is the part a user can act on: "furniture removal is at 3.1 % of a 4 % allowance"
/// says whether the next page of running heads will be kept or dropped, and a bare count does not.
#[derive(Clone, Debug, Serialize)]
pub struct ReasonTotal {
    pub reason: Reason,
    pub removed_chars: u64,
    pub added_chars: u64,
    pub net_removed: u64,
    /// The budget group this reason draws on, and its bound as a fraction of `|C_0|`.
    pub budget_group: Option<&'static str>,
    pub budget: Option<f64>,
    /// `net_removed / |C_0|`.
    pub measured: f64,
    /// `budget − measured`, floored at zero.
    pub headroom: Option<f64>,
}

/// The conservation law's whole account of the conversion.
#[derive(Clone, Debug, Serialize)]
pub struct ConservationReport {
    /// `|C_raw|`, the multiset extraction produced.
    pub c_raw_chars: u64,
    /// `|C_0|`, the retention denominator (ARCHITECTURE §5.2).
    pub c0_chars: u64,
    /// `|C(EPUB)|`.
    pub epub_chars: u64,
    /// The headline number: `|C(EPUB)| / |C_0|` (PIPELINE §13).
    pub retention: f32,
    pub per_stage: Vec<StageCheck>,
    pub per_reason: Vec<ReasonTotal>,
    pub i7: I7Report,
}

/// I-7, as the report states it. The multisets themselves are summarised rather than dumped: a
/// report is not the place to put the book back together, and the count plus a sample is what names
/// the defect.
#[derive(Clone, Debug, Serialize)]
pub struct I7Report {
    pub holds: bool,
    pub missing_chars: u64,
    pub added_chars: u64,
    /// Up to twelve of the missing scalars with their counts, so a maintainer can see *what* went.
    pub missing_sample: Vec<(char, u32)>,
}

/// What each tier of validation said.
#[derive(Clone, Debug, Serialize)]
pub struct ValidationReport {
    pub tier1: Tier1Summary,
    /// Whether EPUBCheck ran. Off the default path by D6 — it is Java — so `false` here is the
    /// expected value and not a failure.
    pub tier2_ran: bool,
    /// Whether Ace by DAISY ran. Nightly CI only in v1.
    pub tier3_ran: bool,
    pub structural: oc_validate::structural::StructuralReport,
}

/// Tier 1's findings and, as importantly, what it looked at.
#[derive(Clone, Debug, Serialize)]
pub struct Tier1Summary {
    pub valid: bool,
    pub checked: Vec<&'static str>,
    pub findings: Vec<oc_validate::Finding>,
}

/// The loop's account of itself.
#[derive(Clone, Debug, Serialize)]
pub struct RepairReport {
    pub status: &'static str,
    pub iterations: usize,
    /// Per repair id, how many times it fired. **Target zero** (RT A10.4): every entry here is an
    /// emitter bug.
    pub fires: BTreeMap<&'static str, u32>,
    pub log: Vec<oc_validate::repair::RepairStep>,
    /// The message ids still in the container after the loop stopped, verbatim.
    pub remaining: Vec<oc_validate::Finding>,
}

/// The whole report.
#[derive(Clone, Debug, Serialize)]
pub struct Report {
    /// First key, and the reason a reader can trust the rest (ARCHITECTURE §4.5 makes the same
    /// argument about `ir_version`).
    pub schema: &'static str,
    pub status: Status,
    pub engine: Engine,
    pub input: Input,
    pub document: DocumentSummary,
    /// Per stage, **in stage order**, plus `total`. A list of pairs rather than a map, because a map
    /// would be printed in alphabetical order and nobody reads a pipeline that way.
    pub timings_ms: Vec<(String, u64)>,
    pub conservation: ConservationReport,
    /// How many pages of each class the document has (D13.10).
    pub page_classes: BTreeMap<String, u32>,
    pub validation: ValidationReport,
    pub repair: RepairReport,
    /// Every warning, as `code` + `args`. Never prose: the reader localises (R10 §6.20).
    pub warnings: Vec<Warning>,
    /// Every `Decision` the pipeline made, with its `LlmTrace` where a model was consulted. Empty
    /// while `ai.enabled` is false, which is the v1 default.
    pub decisions: Vec<oc_model::decision::Decision>,
    /// Every choice the deterministic evidence could not settle, with the predicate that fired and
    /// the signals it read — whether or not a model was asked. The first books converted are the
    /// calibration corpus, and this is what they contribute to it (PHASE 10 detail 1, RT A7.2).
    pub escalations: Vec<oc_structure::escalate::EscalationRecord>,
    /// What the AI step did, when it ran: absent with AI off, the v1 default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai: Option<AiReport>,
    /// That text from this book was allowed to leave the machine, to which host, and when (D10,
    /// PHASE 11 detail 4). Absent unless the endpoint was off this machine and consent named it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consent: Option<ConsentReport>,
    /// The provenance of every threshold, so a user can see which numbers were provisional at
    /// conversion time (D17).
    pub thresholds: Vec<ThresholdProvenance>,
}

/// The AI step, as the report prints it: which model, how many calls, how much of the time.
#[derive(Clone, Debug, Serialize)]
pub struct AiReport {
    /// Which adapter answered (PHASE 11): `local_sidecar`, `ollama`, `openai_compatible`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<oc_ai::provider::ProviderKind>,
    pub model_id: String,
    /// `--ai-all-tasks`: the language gate was set aside.
    pub all_tasks: bool,
    pub calls: usize,
    pub cached_calls: usize,
    pub llm_ms: u64,
}

/// A consent, as the report prints it: the host, the moment, the scope.
#[derive(Clone, Debug, Serialize)]
pub struct ConsentReport {
    pub host: String,
    /// RFC 3339, UTC, to the second.
    pub granted_at: String,
    /// `run`: this conversion and no other.
    pub scope: &'static str,
}

impl From<&oc_net::consent::ConsentRecord> for ConsentReport {
    fn from(record: &oc_net::consent::ConsentRecord) -> Self {
        Self {
            host: record.host.clone(),
            granted_at: record.granted_at_rfc3339(),
            scope: record.scope.as_str(),
        }
    }
}

/// One threshold's provenance, as the report prints it.
#[derive(Clone, Debug, Serialize)]
pub struct ThresholdProvenance {
    pub key: &'static str,
    pub source: &'static str,
    pub evidence: &'static str,
    pub owner: &'static str,
    pub review_by: &'static str,
}

impl From<&Provenance> for ThresholdProvenance {
    fn from(entry: &Provenance) -> Self {
        Self {
            key: entry.key,
            source: entry.source,
            evidence: entry.evidence,
            owner: entry.owner,
            review_by: entry.review_by,
        }
    }
}

/// What the caller knows that the conversion does not.
pub struct ReportInput<'a> {
    pub filename: &'a str,
    pub pdfium_version: &'a str,
    pub producer_family: oc_pdf::producer::ProducerFamily,
    pub pages: u32,
    pub page_classes: BTreeMap<String, u32>,
    /// Which adapter answered, when AI ran.
    pub provider: Option<oc_ai::provider::ProviderKind>,
    /// The consent an endpoint off this machine needed.
    pub consent: Option<&'a oc_net::consent::ConsentRecord>,
}

/// Build the report for one conversion.
pub fn report(conversion: &Conversion, input: ReportInput<'_>) -> Report {
    let document = &conversion.document;
    let i7 = &conversion.structural.i7;

    let mut timings: Vec<(String, u64)> = conversion
        .timings
        .as_slice()
        .iter()
        .map(|(stage, ms)| ((*stage).to_owned(), *ms))
        .collect();
    let total: u64 = timings.iter().map(|(_, ms)| *ms).sum();
    timings.push(("total".to_owned(), total));

    // `invalid` is the loop's verdict and nothing else's. A low-retention flag is a warning about a
    // book that may be missing text; an `invalid` report is a container a reading system may
    // refuse, and conflating the two would make the word useless.
    let status = if conversion.repair.is_invalid() {
        Status::Invalid
    } else {
        Status::Ok
    };

    Report {
        schema: SCHEMA,
        status,
        engine: Engine {
            version: env!("CARGO_PKG_VERSION"),
            ir_version: document.ir_version,
            pdfium_version: input.pdfium_version.to_owned(),
            prompt_version: conversion
                .ai
                .as_ref()
                .map(|_| oc_ai::prompt::PROMPT_VERSION.to_string()),
        },
        input: Input {
            sha256: document.source_sha256.clone(),
            filename: input.filename.to_owned(),
            pages: input.pages,
            producer_family: serde_json::to_value(input.producer_family)
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned))
                .unwrap_or_else(|| "unknown".to_owned()),
        },
        document: DocumentSummary {
            classification: document.classification.as_str(),
            preset: document.presets.as_str(),
            language: document.language.as_str().to_owned(),
            title: document.meta.title.clone(),
            sections: document.walk().len(),
            figures: document.figures.len(),
            tables: document.tables.len(),
            notes: document.notes.len(),
        },
        timings_ms: timings,
        conservation: ConservationReport {
            c_raw_chars: document.ledger.c_raw.total(),
            c0_chars: i7.c0_chars,
            epub_chars: i7.epub_chars,
            retention: i7.retention(),
            per_stage: document.ledger.per_stage_checks.clone(),
            per_reason: reason_totals(&document.ledger),
            i7: I7Report {
                holds: i7.holds(),
                missing_chars: i7.missing.total(),
                added_chars: i7.extra.total(),
                missing_sample: i7.missing.iter().take(12).collect(),
            },
        },
        page_classes: input.page_classes,
        validation: ValidationReport {
            tier1: Tier1Summary {
                valid: conversion.tier1.is_valid(),
                checked: conversion.tier1.checked.clone(),
                findings: conversion.tier1.findings.clone(),
            },
            tier2_ran: false,
            tier3_ran: false,
            structural: conversion.structural.clone(),
        },
        repair: RepairReport {
            status: conversion.repair.status.as_str(),
            iterations: conversion.repair.log.len(),
            fires: conversion.repair.fires.clone(),
            log: conversion.repair.log.clone(),
            remaining: conversion.repair.remaining.clone(),
        },
        warnings: document.warnings.clone(),
        decisions: document.decisions.clone(),
        escalations: conversion.escalations.clone(),
        ai: conversion.ai.as_ref().map(|outcome| AiReport {
            provider: input.provider,
            model_id: outcome.model_id.clone(),
            all_tasks: outcome.all_tasks,
            calls: outcome.calls.len(),
            cached_calls: outcome.calls.iter().filter(|call| call.cached).count(),
            llm_ms: outcome.llm_ms,
        }),
        consent: input.consent.map(ConsentReport::from),
        thresholds: PROVENANCE.iter().map(ThresholdProvenance::from).collect(),
    }
}

/// Per-`Reason` totals with each reason's budget and remaining headroom (PIPELINE §13).
fn reason_totals(ledger: &Ledger) -> Vec<ReasonTotal> {
    let c0 = ledger.c_0.total();
    let mut per_reason: BTreeMap<Reason, (u64, u64)> = BTreeMap::new();
    for entry in &ledger.entries {
        let slot = per_reason.entry(entry.reason).or_default();
        let chars = entry.text.chars().filter(|ch| !ch.is_whitespace()).count() as u64;
        if entry.added {
            slot.1 += chars;
        } else {
            slot.0 += chars;
        }
    }

    per_reason
        .into_iter()
        .map(|(reason, (removed, added))| {
            let net = removed.saturating_sub(added);
            let group = oc_core::ledger_check::budget_group(reason);
            let measured = if c0 == 0 { 0.0 } else { net as f64 / c0 as f64 };
            ReasonTotal {
                reason,
                removed_chars: removed,
                added_chars: added,
                net_removed: net,
                budget_group: group.map(|group| group.name),
                budget: group.map(|group| group.fraction),
                measured,
                headroom: group.map(|group| (group.fraction - measured).max(0.0)),
            }
        })
        .collect()
}

/// The report as pretty JSON with a trailing newline.
///
/// Pretty rather than compact, because the first reader of a report is a person looking at a book
/// that came out wrong, and `serde_json`'s `preserve_order` feature keeps the key order this file
/// declares (IMPLEMENTATION_PLAN §1.2).
pub fn to_json(report: &Report) -> Result<String, serde_json::Error> {
    let mut text = serde_json::to_string_pretty(report)?;
    text.push('\n');
    Ok(text)
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). The two properties that are about the report
// as a document rather than about the conversion behind it; row 6.12 snapshots a real one.
// ---------------------------------------------------------------------------

/// Every threshold's provenance is in the report, because D17's promise is that a user can see
/// which numbers were provisional at conversion time — and a report carrying only the ones some
/// code path happened to read would make that unanswerable.
#[test]
fn the_report_carries_every_thresholds_provenance() {
    let carried: Vec<ThresholdProvenance> =
        PROVENANCE.iter().map(ThresholdProvenance::from).collect();

    assert_eq!(carried.len(), PROVENANCE.len());
    assert!(
        carried
            .iter()
            .any(|entry| entry.key == "repair.max_iterations"),
        "a key the repair loop reads is in the list"
    );
    assert!(
        carried.iter().any(
            |entry| entry.key == "validate.min_char_retention" && entry.source == "provisional"
        ),
        "and a provisional one says so"
    );
}

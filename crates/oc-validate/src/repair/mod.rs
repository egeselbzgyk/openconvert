//! The validate → repair loop (D13.7, ARCHITECTURE §7, PIPELINE §12).
//!
//! A repair loop without a termination argument is the classic infinite-oscillation bug (RT A10),
//! and the answer is three mechanisms that have to hold together:
//!
//! | Mechanism | Rule |
//! |---|---|
//! | Measure | `M = (fatal, error, warning)`, lexicographic, must **strictly decrease** |
//! | No new ids | a repair introducing a message id absent before is reverted |
//! | Cycle detection | the container's content is hashed each iteration; a repeat halts |
//! | Confluence | at most one repair per `(file, node)`, in a fixed total order |
//! | Cap | `repair.max_iterations`, a **safety bound and not the argument** |
//!
//! **A repair edits the `Document` and the book is re-emitted.** It never patches the zip: "a
//! corrective patch to the zip is untyped, order-dependent, and leaves the emitter broken"
//! (ARCHITECTURE §7.3). Re-emitting keeps the typed builder (D5) the only writer of markup, so a
//! repair cannot produce a content-model error of its own.
//!
//! **Every repair that fires is a bug in our emitter.** The corpus-wide fire rate is a release gate
//! with target zero (RT A10.4), which is why [`RepairOutcome::fires`] is counted per repair id and
//! reported rather than logged and forgotten.

pub mod measure;
pub mod table;

use std::collections::{BTreeMap, BTreeSet};

use oc_model::doc::{Content, Severity as WarnSeverity, Warning};
use oc_model::document::Document;
use oc_model::ids::NoteId;

use crate::tier1::Finding;

pub use measure::Measure;
pub use table::{plan_repairs, Fix, Remedy, RepairAction, RepairPlan};

/// A validator reported a message id the repair table does not cover (R10 §6.19).
pub const W_UNMAPPED_VALIDATION_ID: &str = "W_UNMAPPED_VALIDATION_ID";
/// A real defect the table covers and for which no character-conserving structural fix exists.
pub const W_VALIDATION_UNREPAIRABLE: &str = "W_VALIDATION_UNREPAIRABLE";
/// The loop stopped with errors still in the container. The EPUB is still written (D13.7).
pub const W_EPUB_INVALID: &str = "W_EPUB_INVALID";
/// Two repairs undid each other: the container's content hash repeated.
pub const W_REPAIR_OSCILLATION: &str = "W_REPAIR_OSCILLATION";
/// A repair fired. Every one of these is an emitter bug (ARCHITECTURE §7.3).
pub const W_REPAIR_FIRED: &str = "W_REPAIR_FIRED";

/// How the loop ended.
///
/// The names are the report's `status` values, and `repair_oscillation` is D13.7's own word.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairStatus {
    /// The first emission validated clean: nothing to repair, and the expected outcome.
    Clean,
    /// Repairs fired and the container validates clean now.
    Repaired,
    /// Findings remain and the table has no repair for any of them.
    NoRepairAvailable,
    /// A repair was attempted and did not strictly decrease `M`, or introduced a new message id.
    /// Reverted, and the loop stopped.
    NoProgress,
    /// The container's content hash repeated: two repairs are undoing each other.
    RepairOscillation,
    /// `repair.max_iterations` reached with findings left.
    CapReached,
}

impl RepairStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            RepairStatus::Clean => "clean",
            RepairStatus::Repaired => "repaired",
            RepairStatus::NoRepairAvailable => "no_repair_available",
            RepairStatus::NoProgress => "no_progress",
            RepairStatus::RepairOscillation => "repair_oscillation",
            RepairStatus::CapReached => "cap_reached",
        }
    }
}

/// One iteration, as the report records it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct RepairStep {
    /// 1-based.
    pub iteration: u32,
    pub before: Measure,
    pub after: Measure,
    pub applied: Vec<RepairAction>,
    /// Ids that were absent before this iteration and present after it. Non-empty means the step
    /// was reverted.
    pub new_ids: Vec<String>,
    pub accepted: bool,
}

/// What the loop produced.
#[derive(Clone, Debug)]
pub struct RepairOutcome {
    pub status: RepairStatus,
    /// The document as the loop left it — the original when nothing was accepted.
    pub document: Document,
    /// The findings of the emission the loop settled on.
    pub remaining: Vec<Finding>,
    pub measure: Measure,
    pub log: Vec<RepairStep>,
    /// How many times each repair id fired, the release metric with target zero (RT A10.4).
    pub fires: BTreeMap<&'static str, u32>,
    pub warnings: Vec<Warning>,
}

impl RepairOutcome {
    /// Whether the report should be marked `invalid`: errors remain in the container the loop
    /// settled on. The EPUB is written either way — "a slightly invalid EPUB is more useful than
    /// none" (D13.7).
    pub fn is_invalid(&self) -> bool {
        self.measure.has_errors()
    }

    /// How many repairs fired in total.
    pub fn fire_count(&self) -> u32 {
        self.fires.values().sum()
    }
}

/// One emit-and-validate pass, as the loop sees it.
#[derive(Clone, Debug)]
pub struct Emission {
    /// The container's content hash with timestamps excluded (ARCHITECTURE §7.2).
    pub hash: [u8; 32],
    pub findings: Vec<Finding>,
}

/// What the loop needs from the world: a way to turn a document into a validated container.
///
/// A trait rather than a concrete emitter so that the *control logic* — strict decrease, new-id
/// rejection, oscillation detection — is testable without a PDF, a zip or a validator. Those three
/// rules are the whole safety argument of this module, and a test that had to build a real
/// container to exercise them could not construct the cases that matter: this emitter cannot be
/// made to oscillate, which is exactly why the loop's handling of oscillation needs a double.
pub trait Emit {
    type Error;

    fn emit_and_validate(&mut self, document: &Document) -> Result<Emission, Self::Error>;

    /// Apply one repair to a candidate document, returning whether anything changed.
    ///
    /// Provided, and [`apply_fix`] is what it provides: a host overrides it only to separate the
    /// loop's control logic from the table's edits. The oscillation and cap rules cannot be
    /// exercised through the real edits at all — `Fix::FigureAlt` describes every undescribed
    /// figure on its first pass, so a second iteration would have nothing to change and the loop
    /// would correctly report `no_repair_available` before it ever reached the cap.
    fn apply(&mut self, document: &mut Document, fix: Fix) -> bool {
        apply_fix(document, fix)
    }
}

/// The loop's bounds.
#[derive(Clone, Copy, Debug)]
pub struct RepairOpts {
    /// `repair.max_iterations`. A safety bound; the termination argument is the strict decrease.
    pub max_iterations: u32,
    /// `repair.require_strict_decrease`. Always true in shipped configurations; a knob only so that
    /// the report can say which rule was in force.
    pub require_strict_decrease: bool,
}

/// Run the loop.
///
/// Returns the document the loop settled on, the findings of that emission, and the log. The caller
/// re-emits that document to get the bytes it writes — or, more cheaply, keeps the emission it
/// already made; the loop deliberately does not own the container, because what it decides is which
/// *document* to serialise.
pub fn repair_loop<E: Emit>(
    document: &Document,
    emitter: &mut E,
    opts: &RepairOpts,
) -> Result<RepairOutcome, E::Error> {
    let mut current = document.clone();
    let mut emission = emitter.emit_and_validate(&current)?;
    let mut measure = Measure::of(&emission.findings);

    let mut seen: BTreeSet<[u8; 32]> = BTreeSet::new();
    seen.insert(emission.hash);

    let mut log: Vec<RepairStep> = Vec::new();
    let mut fires: BTreeMap<&'static str, u32> = BTreeMap::new();
    let mut warnings: Vec<Warning> = Vec::new();
    let mut unmapped_seen: BTreeSet<String> = BTreeSet::new();
    let mut warn_only_seen: BTreeSet<&'static str> = BTreeSet::new();

    if measure.is_zero() {
        return Ok(finish(
            RepairStatus::Clean,
            current,
            emission,
            measure,
            log,
            fires,
            warnings,
        ));
    }

    let mut status = RepairStatus::CapReached;
    for iteration in 1..=opts.max_iterations {
        let plan = plan_repairs(&emission.findings);
        record_reported(
            &plan,
            &mut warnings,
            &mut unmapped_seen,
            &mut warn_only_seen,
        );

        if plan.is_empty() {
            status = RepairStatus::NoRepairAvailable;
            break;
        }

        let mut candidate = current.clone();
        let mut applied: Vec<RepairAction> = Vec::new();
        for action in &plan.actions {
            if emitter.apply(&mut candidate, action.fix) {
                applied.push(action.clone());
            }
        }
        if applied.is_empty() {
            // Every planned repair found nothing to change. The finding is about something the
            // document does not describe — the emitter, or the zip writer — so there is no repair
            // available whatever the table says.
            status = RepairStatus::NoRepairAvailable;
            break;
        }

        let before_ids = measure::message_ids(&emission.findings);
        let next = emitter.emit_and_validate(&candidate)?;
        let next_measure = Measure::of(&next.findings);
        let new_ids: Vec<String> =
            measure::new_ids(&before_ids, &measure::message_ids(&next.findings))
                .into_iter()
                .map(str::to_owned)
                .collect();

        let decreased = !opts.require_strict_decrease || next_measure < measure;
        let accepted = decreased && new_ids.is_empty();

        log.push(RepairStep {
            iteration,
            before: measure,
            after: next_measure,
            applied: applied.clone(),
            new_ids: new_ids.clone(),
            accepted,
        });

        if !accepted {
            // Reverted: `current` is untouched, and the loop stops rather than trying the same plan
            // again. Trying again would be the oscillation the measure exists to prevent.
            status = RepairStatus::NoProgress;
            break;
        }

        for action in &applied {
            *fires.entry(action.fix.id()).or_default() += 1;
            warnings.push(
                Warning::new(W_REPAIR_FIRED, WarnSeverity::Warn)
                    .with_arg("repair", action.fix.id())
                    .with_arg("message_id", action.message_id)
                    .with_arg("location", action.location.clone()),
            );
        }

        current = candidate;
        // Cycle detection before the next plan: strict decrease alone does not catch every cycle
        // when two repairs interact, and a repeated container is the observable form of one.
        let repeated = !seen.insert(next.hash);
        emission = next;
        measure = next_measure;

        if repeated {
            warnings.push(Warning::new(W_REPAIR_OSCILLATION, WarnSeverity::Error));
            status = RepairStatus::RepairOscillation;
            break;
        }
        if measure.is_zero() {
            status = RepairStatus::Repaired;
            break;
        }
    }

    if measure.has_errors() {
        warnings.push(
            Warning::new(W_EPUB_INVALID, WarnSeverity::Error)
                .with_arg("status", status.as_str())
                .with_arg("fatal", measure.fatal.to_string())
                .with_arg("error", measure.error.to_string())
                .with_arg(
                    "ids",
                    measure::message_ids(&emission.findings)
                        .into_iter()
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
        );
    }

    Ok(finish(
        status, current, emission, measure, log, fires, warnings,
    ))
}

fn finish(
    status: RepairStatus,
    document: Document,
    emission: Emission,
    measure: Measure,
    log: Vec<RepairStep>,
    fires: BTreeMap<&'static str, u32>,
    warnings: Vec<Warning>,
) -> RepairOutcome {
    RepairOutcome {
        status,
        document,
        remaining: emission.findings,
        measure,
        log,
        fires,
        warnings,
    }
}

/// Turn the plan's `WarnUser` and unmapped ids into warnings, once each.
fn record_reported(
    plan: &RepairPlan,
    warnings: &mut Vec<Warning>,
    unmapped_seen: &mut BTreeSet<String>,
    warn_only_seen: &mut BTreeSet<&'static str>,
) {
    for id in &plan.unmapped {
        if unmapped_seen.insert(id.clone()) {
            warnings.push(
                Warning::new(W_UNMAPPED_VALIDATION_ID, WarnSeverity::Warn)
                    .with_arg("message_id", id.clone()),
            );
        }
    }
    for id in &plan.warn_only {
        if warn_only_seen.insert(id) {
            warnings.push(
                Warning::new(W_VALIDATION_UNREPAIRABLE, WarnSeverity::Warn)
                    .with_arg("message_id", *id),
            );
        }
    }
}

/// The fallback description an image with no `alt` is given.
///
/// Deliberately a statement of ignorance rather than a description. R1 §A.10 measured that 95 % of
/// real alt text is the literal word "Image"; inventing something better would be inventing.
const ALT_FALLBACK: &str = "Image from the source document; no description was available.";

/// Apply one repair to a document. Returns whether anything changed.
///
/// A repair that finds nothing to change is not an error: the finding may be about the emitter or
/// the zip writer, neither of which the document describes. The loop treats a plan that changed
/// nothing as "no repair available", which is the truth.
pub fn apply_fix(document: &mut Document, fix: Fix) -> bool {
    match fix {
        Fix::FigureAlt => {
            let mut changed = false;
            for figure in &mut document.figures {
                if figure.alt.trim().is_empty() {
                    figure.alt = ALT_FALLBACK.to_owned();
                    changed = true;
                }
            }
            changed
        }
        Fix::DemoteDanglingNoteref => {
            let known: BTreeSet<NoteId> = document.notes.iter().map(|note| note.id).collect();
            let mut changed = false;
            for section in &mut document.sections {
                changed |= demote_in_section(section, &known);
            }
            for note in &mut document.notes {
                changed |= demote_in_content(&mut note.body, &known);
            }
            changed
        }
        Fix::DropUnreferencedFigure => {
            let referenced = referenced_figures(document);
            let before = document.figures.len();
            document
                .figures
                .retain(|figure| referenced.contains(&figure.id) || figure.caption.is_some());
            document.figures.len() != before
        }
    }
}

fn referenced_figures(document: &Document) -> BTreeSet<oc_model::ids::FigureId> {
    let mut out = BTreeSet::new();
    for section in document.walk() {
        collect_figures(&section.content, &mut out);
    }
    for note in &document.notes {
        collect_figures(&note.body, &mut out);
    }
    out
}

fn collect_figures(content: &[Content], out: &mut BTreeSet<oc_model::ids::FigureId>) {
    for item in content {
        match item {
            Content::Figure(id) => {
                out.insert(*id);
            }
            Content::BlockQuote(inner) | Content::Epigraph(inner) => collect_figures(inner, out),
            _ => {}
        }
    }
}

fn demote_in_section(section: &mut oc_model::doc::Section, known: &BTreeSet<NoteId>) -> bool {
    let mut changed = demote_in_content(&mut section.content, known);
    if let Some(heading) = &mut section.heading {
        for span in &mut heading.spans {
            changed |= demote_span(span, known);
        }
    }
    for child in &mut section.children {
        changed |= demote_in_section(child, known);
    }
    changed
}

/// Strip a `noteref` whose target does not exist, and drop the anchor that went with it.
///
/// The marker's text stays exactly where it was — this removes a link, not a character — which is
/// what makes the repair Conserving.
fn demote_in_content(content: &mut Vec<Content>, known: &BTreeSet<NoteId>) -> bool {
    let mut changed = false;
    content.retain(|item| match item {
        Content::NoteRefAnchor(id) => {
            let keep = known.contains(id);
            changed |= !keep;
            keep
        }
        _ => true,
    });
    for item in content.iter_mut() {
        match item {
            Content::Paragraph(para) => {
                for span in &mut para.spans {
                    changed |= demote_span(span, known);
                }
            }
            Content::Heading(heading) => {
                for span in &mut heading.spans {
                    changed |= demote_span(span, known);
                }
            }
            Content::BlockQuote(inner) | Content::Epigraph(inner) => {
                changed |= demote_in_content(inner, known);
            }
            _ => {}
        }
    }
    changed
}

fn demote_span(span: &mut oc_model::doc::Span, known: &BTreeSet<NoteId>) -> bool {
    match span.noteref {
        Some(id) if !known.contains(&id) => {
            span.noteref = None;
            true
        }
        _ => false,
    }
}

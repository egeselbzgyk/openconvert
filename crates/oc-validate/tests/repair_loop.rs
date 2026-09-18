//! Rows 6.4, 6.5, 6.6 and 6.9: the repair loop's control logic, against a scripted emitter.
//!
//! The three rules tested here — strict decrease, no new message id, oscillation by content hash —
//! are the whole safety argument of the loop, and none of them can be exercised through the real
//! emitter. That is not a gap in the tests; it is the premise of the phase: the emitter passes
//! EPUBCheck clean on every fixture, so a real run produces `RepairStatus::Clean` and touches none
//! of this code. A scripted [`Emit`] is how the cases that matter get constructed at all.
//!
//! The plan's expected behaviour for these rows is that they "fail on a naive *apply all repairs,
//! re-run* loop". Each test says which naive behaviour it would catch.

use std::collections::BTreeMap;

use oc_model::document::Document;
use oc_validate::repair::{repair_loop, Emission, Emit, RepairOpts, RepairStatus};
use oc_validate::tier1::{Finding, Severity};

/// An emitter that returns whatever the script says, keyed by how many times it has been called.
///
/// The hash is scripted too, because oscillation is a statement about the *container* repeating and
/// the double has no container. A test that wanted to see the hash rule fire would otherwise have to
/// construct two documents that emit to the same bytes, which is a different claim.
struct Scripted {
    /// One entry per call: the findings that call reports, and the container hash.
    script: Vec<(Vec<Finding>, u8)>,
    calls: usize,
    /// When set, every planned repair reports that it changed something, whatever the table's edit
    /// would actually have done. The oscillation and cap rules are about the loop's control flow and
    /// cannot be reached through the real edits at all: `Fix::FigureAlt` describes every undescribed
    /// figure on its first pass, so a second iteration has nothing to change and the loop correctly
    /// reports `no_repair_available` long before the cap. Overriding the edit is how the control
    /// flow gets tested on its own.
    always_applies: bool,
}

impl Scripted {
    fn new(script: Vec<(Vec<Finding>, u8)>) -> Self {
        Self {
            script,
            calls: 0,
            always_applies: false,
        }
    }

    /// The same, with the table's edits stubbed out.
    fn control_flow_only(script: Vec<(Vec<Finding>, u8)>) -> Self {
        Self {
            script,
            calls: 0,
            always_applies: true,
        }
    }

    fn calls(&self) -> usize {
        self.calls
    }
}

impl Emit for Scripted {
    type Error = std::convert::Infallible;

    fn emit_and_validate(&mut self, _document: &Document) -> Result<Emission, Self::Error> {
        let index = self.calls.min(self.script.len().saturating_sub(1));
        self.calls += 1;
        let (findings, hash) = self.script[index].clone();
        Ok(Emission {
            hash: [hash; 32],
            findings,
        })
    }

    fn apply(&mut self, document: &mut Document, fix: oc_validate::repair::Fix) -> bool {
        if self.always_applies {
            return true;
        }
        oc_validate::repair::apply_fix(document, fix)
    }
}

fn finding(id: &'static str, severity: Severity, location: &str) -> Finding {
    Finding {
        id,
        severity,
        location: location.to_owned(),
        detail: String::new(),
    }
}

/// A document with one figure carrying no alt text, so that `Fix::FigureAlt` has something to do.
///
/// The loop refuses a plan that changed nothing — a finding about the emitter is not a finding the
/// document can answer — so a scripted test needs a document a repair can actually edit.
fn document_with_an_undescribed_figure() -> Document {
    use oc_model::confidence::Confidence;
    use oc_model::doc::{Content, Figure, MetaSource, Metadata, Section, SectionRole};
    use oc_model::extract::ImageId;
    use oc_model::geom::Rect;
    use oc_model::ids::{BlockId, FigureId};
    use oc_model::lang::LangTag;
    use oc_model::ledger::Ledger;

    let rect = Rect {
        x0: 0.0,
        y0: 0.0,
        x1: 10.0,
        y1: 10.0,
    };
    let anchor = BlockId::derive(0, rect, "anchor");

    Document {
        ir_version: oc_model::IR_VERSION,
        source_sha256: "0".repeat(64),
        meta: Metadata {
            title: Some("A Book".to_owned()),
            subtitle: None,
            authors: Vec::new(),
            translator: None,
            publisher: None,
            date: None,
            identifier: "urn:uuid:00000000-0000-4000-8000-000000000000".to_owned(),
            language: LangTag::EN,
            source: MetaSource::Heuristic,
        },
        language: LangTag::EN,
        sections: vec![Section {
            id: anchor,
            role: SectionRole::Chapter,
            level: 1,
            heading: None,
            content: vec![Content::Figure(FigureId(1))],
            children: Vec::new(),
            source_pages: (0, 0),
            confidence: Confidence::fallback(Vec::new()),
        }],
        notes: Vec::new(),
        figures: vec![Figure {
            id: FigureId(1),
            image: ImageId(0),
            caption: None,
            alt: String::new(),
            anchor,
            confidence: Confidence::fallback(Vec::new()),
        }],
        tables: Vec::new(),
        page_breaks: Vec::new(),
        ledger: Ledger::default(),
        decisions: Vec::new(),
        warnings: Vec::new(),
        classification: oc_model::document::DocClass::BookProse,
        presets: oc_model::document::PresetName::Novel,
    }
}

fn opts() -> RepairOpts {
    RepairOpts {
        max_iterations: u32::try_from(oc_core::thresholds::T.repair.max_iterations).unwrap_or(3),
        require_strict_decrease: oc_core::thresholds::T.repair.require_strict_decrease,
    }
}

/// A container that validates clean needs no loop at all, and that is the expected outcome on every
/// fixture. `Clean` rather than `Repaired`, so the fire-rate metric can tell the two apart.
#[test]
fn a_clean_container_is_not_repaired() {
    let mut emitter = Scripted::new(vec![(Vec::new(), 1)]);
    let outcome = repair_loop(
        &document_with_an_undescribed_figure(),
        &mut emitter,
        &opts(),
    )
    .expect("the double cannot fail");

    assert_eq!(outcome.status, RepairStatus::Clean);
    assert_eq!(emitter.calls(), 1, "no repair pass was attempted");
    assert_eq!(outcome.fire_count(), 0);
    assert!(outcome.log.is_empty());
    assert!(!outcome.is_invalid());
}

/// Row 6.4. A repair that leaves `M` unchanged is rejected and the loop halts.
///
/// A naive "apply all repairs, re-run" loop would re-plan the same repair against the same findings
/// and run until the cap, reporting three wasted iterations and the same container. The measure is
/// what turns that into one attempt and a named status.
#[test]
fn repair_requires_strict_decrease() {
    let unchanged = vec![finding("ACC-001", Severity::Warning, "text/c1.xhtml#f1")];
    let mut emitter = Scripted::new(vec![
        (unchanged.clone(), 1),
        // The repair fired and changed nothing the validator can see.
        (unchanged.clone(), 2),
    ]);

    let document = document_with_an_undescribed_figure();
    let outcome = repair_loop(&document, &mut emitter, &opts()).expect("the double cannot fail");

    assert_eq!(outcome.status, RepairStatus::NoProgress);
    assert_eq!(emitter.calls(), 2, "one attempt, then the loop stopped");
    assert_eq!(outcome.log.len(), 1);
    assert!(!outcome.log[0].accepted);
    assert_eq!(outcome.log[0].before, outcome.log[0].after);
    assert_eq!(
        outcome.fire_count(),
        0,
        "a reverted repair did not fire: {:?}",
        outcome.fires
    );
    assert_eq!(
        outcome.document, document,
        "the rejected repair was reverted"
    );
}

/// Row 6.5. A repair that fixes one thing and breaks another is reverted even though `M` went down.
///
/// Strict decrease alone would accept trading an `ACC-001` warning for an `OPF-003` error — the
/// measure would read it as progress if the counts happened to fall — and the loop would then walk
/// sideways through the message space. The id set is the second half of the rule for exactly this.
#[test]
fn repair_rejects_new_message_id() {
    let mut emitter = Scripted::new(vec![
        (
            vec![
                finding("ACC-001", Severity::Warning, "text/c1.xhtml#f1"),
                finding("ACC-001", Severity::Warning, "text/c1.xhtml#f2"),
            ],
            1,
        ),
        // One warning fewer — `M` strictly decreased — and a message id that was not there before.
        (
            vec![finding("OPF-003", Severity::Warning, "content.opf")],
            2,
        ),
    ]);

    let document = document_with_an_undescribed_figure();
    let outcome = repair_loop(&document, &mut emitter, &opts()).expect("the double cannot fail");

    assert_eq!(outcome.status, RepairStatus::NoProgress);
    assert_eq!(outcome.log.len(), 1);
    assert!(
        outcome.log[0].after < outcome.log[0].before,
        "the measure did decrease: {:?} → {:?}",
        outcome.log[0].before,
        outcome.log[0].after
    );
    assert_eq!(
        outcome.log[0].new_ids,
        vec!["OPF-003"],
        "and the new id is what rejected it"
    );
    assert!(!outcome.log[0].accepted);
    assert_eq!(outcome.document, document, "reverted");
}

/// Row 6.6. An A→B→A pair halts with `repair_oscillation`.
///
/// Two repairs undoing each other can each strictly decrease the measure on the step they run,
/// because the messages they fix are different ones. Hashing the container with timestamps excluded
/// is what sees the cycle, and it is why the hash rule exists alongside the measure rather than
/// instead of it.
#[test]
fn repair_detects_oscillation_by_hash() {
    let three = vec![
        finding("ACC-001", Severity::Warning, "text/c1.xhtml#f1"),
        finding("ACC-001", Severity::Warning, "text/c1.xhtml#f2"),
        finding("ACC-001", Severity::Warning, "text/c1.xhtml#f3"),
    ];
    let two = three[..2].to_vec();
    let one = three[..1].to_vec();

    // Hashes 1, 2, then 1 again: the third emission is byte for byte the first container, with
    // fewer findings reported. The measure is decreasing the whole way, so nothing but the hash
    // would stop this.
    let mut emitter = Scripted::control_flow_only(vec![(three, 1), (two, 2), (one, 1)]);

    let outcome = repair_loop(
        &document_with_an_undescribed_figure(),
        &mut emitter,
        &opts(),
    )
    .expect("the double cannot fail");

    assert_eq!(outcome.status, RepairStatus::RepairOscillation);
    assert!(
        outcome
            .warnings
            .iter()
            .any(|warning| warning.code == oc_validate::repair::W_REPAIR_OSCILLATION),
        "{:?}",
        outcome.warnings
    );
    assert_eq!(
        outcome.log.len(),
        2,
        "both steps were accepted: {:?}",
        outcome.log
    );
    assert!(outcome.log.iter().all(|step| step.accepted));
}

/// Row 6.9. An id the table does not cover produces `W_UNMAPPED_VALIDATION_ID` and no repair.
///
/// "Logged verbatim, surfaced to the user, and counted as a coverage metric" (R10 §6.19) — never
/// guessed at. The status is `no_repair_available` rather than a failure, because a validator
/// finding something this table has not met is expected and is information.
#[test]
fn unmapped_epubcheck_id_is_logged_not_guessed() {
    let mut emitter = Scripted::new(vec![(
        vec![
            finding("CSS-008", Severity::Error, "style.css"),
            finding("MED-003", Severity::Error, "text/c1.xhtml"),
        ],
        1,
    )]);

    let document = document_with_an_undescribed_figure();
    let outcome = repair_loop(&document, &mut emitter, &opts()).expect("the double cannot fail");

    assert_eq!(outcome.status, RepairStatus::NoRepairAvailable);
    assert_eq!(emitter.calls(), 1, "nothing was attempted");
    assert_eq!(outcome.fire_count(), 0);
    assert_eq!(outcome.document, document);

    let unmapped: Vec<&str> = outcome
        .warnings
        .iter()
        .filter(|warning| warning.code == oc_validate::repair::W_UNMAPPED_VALIDATION_ID)
        .filter_map(|warning| warning.args.get("message_id").map(String::as_str))
        .collect();
    assert_eq!(unmapped, vec!["CSS-008", "MED-003"]);

    // And the ids are still in the report, verbatim, with the container marked invalid.
    assert!(outcome.is_invalid());
    let remaining: Vec<&str> = outcome.remaining.iter().map(|f| f.id).collect();
    assert_eq!(remaining, vec!["CSS-008", "MED-003"]);
}

/// A covered id with no safe fix is reported as unrepairable, which is a different claim from
/// "unmapped": the defect is understood and the answer is that no repair may be made.
#[test]
fn a_covered_id_with_no_safe_fix_says_so_rather_than_saying_nothing() {
    let mut emitter = Scripted::new(vec![(
        vec![finding(
            "OC-NOTE-BIJECTION",
            Severity::Error,
            "text/c1.xhtml#n7",
        )],
        1,
    )]);

    let outcome = repair_loop(
        &document_with_an_undescribed_figure(),
        &mut emitter,
        &opts(),
    )
    .expect("the double cannot fail");

    assert_eq!(outcome.status, RepairStatus::NoRepairAvailable);
    assert!(outcome
        .warnings
        .iter()
        .any(|warning| warning.code == oc_validate::repair::W_VALIDATION_UNREPAIRABLE));
    assert!(!outcome
        .warnings
        .iter()
        .any(|warning| warning.code == oc_validate::repair::W_UNMAPPED_VALIDATION_ID));
}

/// A repair that fires is logged and counted under its own id, because the corpus-wide fire rate is
/// a release gate with target zero (RT A10.4) and a metric that only counted totals could not name
/// the emitter bug to open an issue against.
#[test]
fn an_accepted_repair_is_counted_under_its_own_id() {
    let mut emitter = Scripted::new(vec![
        (
            vec![finding("ACC-001", Severity::Warning, "text/c1.xhtml#f1")],
            1,
        ),
        (Vec::new(), 2),
    ]);

    let document = document_with_an_undescribed_figure();
    let outcome = repair_loop(&document, &mut emitter, &opts()).expect("the double cannot fail");

    assert_eq!(outcome.status, RepairStatus::Repaired);
    assert_eq!(
        outcome.fires,
        BTreeMap::from([("fix.figure_alt", 1)]),
        "{:?}",
        outcome.fires
    );
    assert!(!outcome.is_invalid());
    assert_ne!(outcome.document, document, "the repair was kept");
    assert!(
        !outcome.document.figures[0].alt.is_empty(),
        "the figure was described"
    );
    assert!(outcome
        .warnings
        .iter()
        .any(|warning| warning.code == oc_validate::repair::W_REPAIR_FIRED));
}

/// The cap. `repair.max_iterations` is a safety bound and not the termination argument, so a script
/// that decreases the measure for ever is stopped by it and says so.
#[test]
fn the_cap_stops_a_loop_that_would_otherwise_keep_going() {
    // Findings shrink by one each time and the hash never repeats, so neither the measure nor the
    // cycle rule stops this. Only the cap does.
    let script: Vec<(Vec<Finding>, u8)> = (0..8)
        .map(|call| {
            let count = 8 - call;
            let findings: Vec<Finding> = (0..count)
                .map(|index| {
                    finding(
                        "ACC-001",
                        Severity::Error,
                        &format!("text/c1.xhtml#f{index}"),
                    )
                })
                .collect();
            (findings, u8::try_from(call).unwrap_or(0) + 1)
        })
        .collect();

    let mut emitter = Scripted::control_flow_only(script);
    let outcome = repair_loop(
        &document_with_an_undescribed_figure(),
        &mut emitter,
        &opts(),
    )
    .expect("the double cannot fail");

    let cap = u32::try_from(oc_core::thresholds::T.repair.max_iterations).unwrap_or(3);
    assert_eq!(outcome.status, RepairStatus::CapReached);
    assert_eq!(outcome.log.len() as u32, cap);
    assert!(outcome.is_invalid(), "errors remain after the cap");
    assert!(outcome
        .warnings
        .iter()
        .any(|warning| warning.code == oc_validate::repair::W_EPUB_INVALID));
}

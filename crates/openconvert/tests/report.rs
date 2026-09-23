//! Rows 6.8 and 6.12: the conversion report.
//!
//! Two claims, and the second is the one that matters to a user who has been handed a broken book:
//! after the repair cap **the EPUB is still written** and the report says `invalid` (D13.7). A
//! slightly invalid EPUB is more useful than none; reporting success is what must not happen.

mod common;

use std::collections::BTreeMap;

use openconvert::report::{report, ReportInput, Status, SCHEMA};

/// Row 6.12. The report for `f07`, with the timings redacted — they are wall-clock and would differ
/// on every run and every machine.
#[test]
fn report_schema_is_valid_and_snapshotted() {
    let built = common::build("f07_verse_and_quote");
    let report = report(
        &built.conversion,
        ReportInput {
            filename: "f07_verse_and_quote.pdf",
            pdfium_version: "pinned-for-the-snapshot",
            producer_family: built.producer_family,
            pages: u32::try_from(built.document.page_breaks.len()).unwrap_or(u32::MAX),
            page_classes: built.page_classes.clone(),
            provider: None,
            consent: None,
        },
    );

    assert_eq!(report.schema, SCHEMA, "the version is the first key");
    assert_eq!(report.status, Status::Ok);

    let json = openconvert::report::to_json(&report).expect("the report serialises");
    let mut value: serde_json::Value = serde_json::from_str(&json).expect("and parses back");

    // Timings are wall-clock. Redacted rather than rounded: a snapshot that held a duration would
    // fail on a slower machine and teach everyone to accept snapshot churn.
    value["timings_ms"] = serde_json::json!("[redacted]");
    // The provenance table is asserted by `oc-core`'s own test, entry by entry, and putting 180
    // entries in this snapshot would bury the report in them. Its *length* is what this file is
    // claiming: that every threshold is carried.
    let thresholds = value["thresholds"].as_array().map(Vec::len).unwrap_or(0);
    assert_eq!(
        thresholds,
        oc_core::thresholds::PROVENANCE.len(),
        "every threshold's provenance is in the report"
    );
    value["thresholds"] = serde_json::json!(format!("[{thresholds} entries, redacted]"));

    insta::assert_json_snapshot!("report_f07", value);
}

/// Row 6.8, in the half a single machine can demonstrate: the loop's post-cap policy. Given an
/// outcome with errors left, the report says `invalid` — and the CLI test below shows the EPUB is
/// written anyway.
///
/// The cap itself is exercised by `repair_loop::the_cap_stops_a_loop_that_would_otherwise_keep_going`
/// against a scripted emitter. It cannot be reached through the real one: the emitter passes
/// EPUBCheck clean on every fixture, so a real conversion reports `Clean` before any repair runs.
/// Forcing three failing iterations out of a correct emitter would mean breaking the emitter, and
/// the assertion would then be about the break rather than about the policy.
#[test]
fn repair_cap_writes_epub_and_marks_invalid() {
    use oc_validate::repair::{Measure, RepairOutcome, RepairStatus};
    use oc_validate::{Finding, Severity};

    let built = common::build("f07_verse_and_quote");
    let mut conversion = built.conversion;

    // The outcome the cap produces: three iterations, errors remaining, the container written.
    let remaining = vec![Finding {
        id: "CSS-008",
        severity: Severity::Error,
        location: "style.css".to_owned(),
        detail: "an id this table does not cover".to_owned(),
    }];
    conversion.repair = RepairOutcome {
        status: RepairStatus::CapReached,
        document: conversion.document.clone(),
        remaining: remaining.clone(),
        measure: Measure {
            fatal: 0,
            error: 1,
            warning: 0,
        },
        log: Vec::new(),
        fires: BTreeMap::new(),
        warnings: Vec::new(),
    };

    let report = report(
        &conversion,
        ReportInput {
            filename: "f07_verse_and_quote.pdf",
            pdfium_version: "pinned",
            producer_family: conversion.producer_family,
            pages: 1,
            page_classes: BTreeMap::new(),
            provider: None,
            consent: None,
        },
    );

    assert_eq!(report.status, Status::Invalid);
    assert_eq!(report.repair.status, "cap_reached");
    assert_eq!(
        report
            .repair
            .remaining
            .iter()
            .map(|f| f.id)
            .collect::<Vec<_>>(),
        vec!["CSS-008"],
        "the remaining ids are listed verbatim (D13.7)"
    );
    // And the bytes are still there to be written: the loop does not withhold the book.
    assert!(!conversion.built.bytes.as_slice().is_empty());
}

/// The report's headline is the retention ratio and its evidence is the ledger (PIPELINE §13). Every
/// part the plan's detail 6 names is present, asserted by name rather than by eye, so that a field
/// quietly dropped in a refactor fails a test instead of disappearing from a user's report.
#[test]
fn the_report_carries_every_part_the_plan_names() {
    let built = common::build("f01_prose_single_column");
    let report = report(
        &built.conversion,
        ReportInput {
            filename: "f01_prose_single_column.pdf",
            pdfium_version: "pinned",
            producer_family: built.producer_family,
            pages: 2,
            page_classes: built.page_classes.clone(),
            provider: None,
            consent: None,
        },
    );

    // Input sha256, engine and IR versions.
    assert_eq!(report.input.sha256.len(), 64);
    assert_eq!(report.engine.ir_version, oc_model::IR_VERSION);
    assert!(!report.engine.version.is_empty());
    assert_eq!(report.engine.prompt_version, None, "ai.enabled is false");

    // Per-stage timings, in stage order, with a total.
    let stages: Vec<&str> = report
        .timings_ms
        .iter()
        .map(|(stage, _)| stage.as_str())
        .collect();
    assert_eq!(
        stages,
        vec![
            "ingest",
            "text",
            "furniture",
            "layout",
            "structure",
            "document",
            "epub+validate+repair",
            "total",
        ]
    );

    // Ledger totals per reason, each with the budget it draws on and what is left of it.
    assert!(
        !report.conservation.per_reason.is_empty(),
        "f01 removes furniture"
    );
    let furniture = report
        .conservation
        .per_reason
        .iter()
        .find(|total| total.budget_group == Some("furniture"))
        .expect("the furniture reasons draw on the furniture budget");
    assert_eq!(furniture.budget, Some(0.04));
    assert!(furniture.headroom.is_some_and(|headroom| headroom > 0.0));

    // The per-stage checks, all eight of them.
    assert_eq!(report.conservation.per_stage.len(), 8);

    // The page-class histogram, the producer stratum, the retention ratio.
    assert!(!report.page_classes.is_empty());
    assert_eq!(report.input.producer_family, "Typst");
    assert!(report.conservation.retention > 0.9);
    assert!(report.conservation.i7.holds);

    // Warnings with codes and args; validation results; the repair log.
    assert!(report.warnings.iter().any(|warning| warning.code
        == oc_validate::structural::W_LOW_RETENTION
        && warning.args.contains_key("retention")));
    assert!(report.validation.tier1.valid);
    assert!(!report.validation.tier1.checked.is_empty());
    assert!(
        !report.validation.tier2_ran,
        "EPUBCheck is off the default path"
    );
    assert_eq!(report.repair.status, "clean");
    assert!(report.repair.fires.is_empty());

    // Every `Decision`, and none of them with an `LlmTrace`: the deterministic path still makes
    // decisions and still records them, which is what makes "no model ran" a statement the report
    // supports rather than an absence a reader has to infer.
    assert!(
        !report.decisions.is_empty(),
        "the deterministic path decides too"
    );
    assert!(
        report
            .decisions
            .iter()
            .all(|decision| decision.llm.is_none()),
        "ai.enabled is false, so no decision carries a trace"
    );
    assert_eq!(
        report.thresholds.len(),
        oc_core::thresholds::PROVENANCE.len()
    );
    assert!(report
        .thresholds
        .iter()
        .any(|entry| entry.key == "repair.max_iterations" && entry.source == "provisional"));
}

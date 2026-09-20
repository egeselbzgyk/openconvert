//! `openconvert diff-stage <stage> <input>` — what a stage did to the text (PHASE 7.5).
//!
//! `dump-stage` answers "what did the stage produce", and it cannot be used on a stage that
//! fails the conservation law, because the driver refuses before anything is written. That is
//! the wrong way round: the dump is *most* wanted exactly when the stage did not balance.
//! Phase 7.5's first defect took forty minutes of hand-written binary search over page
//! prefixes to localise, and this command exists so that nobody does that again.
//!
//! So `diff-stage` runs the stage **outside** the check, and reports the difference instead
//! of refusing on it: which characters left, which appeared, and — the part that makes it
//! actionable — which input blocks the output does not contain and which structure had
//! claimed each one.
//!
//! It reports the difference twice, against two different notions of "what the stage
//! produced", and the gap between them is itself a defect class:
//!
//! - **declared** — `StructureOutput::emitted_text`, every container the stage built;
//! - **reachable** — `StructureOutput::reachable_text`, only the containers the flow points
//!   at, which is what a reader will actually see.
//!
//! A stage can balance against the first and be short against the second. That is a silent
//! loss, and it is invisible to I-1 as I-1 is currently computed.

use std::io::Write;

use oc_core::cancel::Cancel;
use oc_core::conservation_diff::{diff, Diff, Unit};
use oc_core::events::EventSink;
use oc_core::exit::ExitCode;
use oc_core::thresholds::T;
use oc_model::lang::LangTag;
use oc_pdf::inspect::PdfOpen;
use oc_pdf::pdfium::PdfiumBackend;
use oc_structure::meta::{InfoDict, MetaSources};
use oc_structure::stage::StructureInput;

use crate::cli::DiffStageArgs;
use openconvert::pipeline::{body_runs, furniture_stage, layout_stage, text_stage};
use openconvert::structure_input::{block_views, document_images};

const E_PDFIUM: &str = "E_PDFIUM_ABI";
const E_INPUT: &str = "E_INPUT";
const E_PDF: &str = "E_PDF";
const E_STAGE: &str = "E_UNKNOWN_STAGE";

/// How much of a block's text a report line carries. A diagnostic that prints a whole
/// paragraph per dropped block is one nobody reads to the end of.
const EXCERPT_CHARS: usize = 72;

/// The stages this command can diff so far.
///
/// `structure` first because it is where every one of Phase 7's eleven refusals landed. The
/// others arrive as the classes that need them do: a command that claimed to diff twelve
/// stages and did one would be a worse lie than a command that says which one.
const IMPLEMENTED: &[&str] = &[oc_core::stages::STRUCTURE.name];

pub fn run<W: Write>(
    args: &DiffStageArgs,
    events: &mut EventSink<W>,
    stdout: &mut dyn Write,
) -> ExitCode {
    let cancel = Cancel::new();
    crate::control::listen(cancel.clone());

    if !IMPLEMENTED.contains(&args.stage.as_str()) {
        events.fatal(
            E_STAGE,
            &format!(
                "`{}` cannot be diffed yet; {} are implemented",
                args.stage,
                IMPLEMENTED.join(", ")
            ),
        );
        return ExitCode::Usage;
    }

    let backend = match PdfiumBackend::bind() {
        Ok(backend) => backend,
        Err(error) => {
            events.fatal(E_PDFIUM, &error.to_string());
            return ExitCode::Usage;
        }
    };
    let bytes = match std::fs::read(&args.input) {
        Ok(bytes) => bytes,
        Err(error) => {
            events.fatal(
                E_INPUT,
                &format!("cannot read {}: {error}", args.input.display()),
            );
            return ExitCode::Usage;
        }
    };
    let document = match backend.open_with_limits(&bytes, args.password.as_deref(), &args.limits) {
        Ok(document) => document,
        Err(error) => {
            events.fatal(E_PDF, &error.to_string());
            return ExitCode::Failed;
        }
    };

    match structure_diff(document.as_ref(), &args.input, stdout) {
        Ok(balanced) => {
            // The command succeeded whatever the document did. A diagnostic that exits
            // non-zero on the defect it was run to investigate cannot be used in a loop over
            // a corpus, which is the only way this one is useful.
            let _ = balanced;
            ExitCode::Ok
        }
        Err(message) => {
            events.fatal(E_PDF, &message);
            ExitCode::Failed
        }
    }
}

/// Run everything up to `structure`, then diff its input against its output both ways.
fn structure_diff(
    pdf: &dyn oc_pdf::inspect::PdfDoc,
    path: &std::path::Path,
    stdout: &mut dyn Write,
) -> Result<bool, String> {
    let t = &T;
    let input = openconvert::input::page_inputs(pdf).map_err(|error| error.to_string())?;
    let mut totals = oc_core::ledger_check::ReasonTotals::default();

    let text = text_stage(&input, &mut totals, t).map_err(|error| format!("text: {error}"))?;
    let language = LangTag::UND;
    let furniture = furniture_stage(&text, language.clone(), &mut totals, t)
        .map_err(|error| format!("furniture: {error}"))?;
    let layout = layout_stage(&text, &furniture, &mut totals, t)
        .map_err(|error| format!("layout: {error}"))?;

    let images = document_images(&text);
    let hashes = openconvert::convert::image_hashes(pdf, &images);
    let vectors = (0..pdf.page_count())
        .filter_map(|page| pdf.page_vectors(page).ok())
        .flatten()
        .collect();
    let doc_info = pdf.doc_info();
    let blocks = block_views(&text, &layout);

    let stage_input = StructureInput {
        blocks: blocks.clone(),
        runs: body_runs(&text, &furniture),
        fonts: text.fonts.clone(),
        images,
        image_hashes: hashes,
        vectors,
        outline: pdf.outline(),
        labels: furniture.labels.clone(),
        drop_caps: layout.drop_caps.iter().flatten().cloned().collect(),
        page_count: pdf.page_count(),
        meta: MetaSources {
            xmp: pdf.xmp(),
            info: InfoDict {
                title: doc_info.title.clone(),
                author: doc_info.author.clone(),
            },
            filename: path.display().to_string(),
            source_sha256: String::new(),
            language: language.clone(),
        },
        lang: language,
    };

    // Deliberately not `structure_stage`: that one checks and refuses, and refusing is what
    // this command exists to look inside.
    let output = oc_structure::stage::structure(&stage_input, t);

    let in_units: Vec<Unit> = blocks
        .iter()
        .map(|block| {
            let unit = Unit::new(
                format!("block {}", block.id.as_str()),
                Some(block.page),
                block.text.clone(),
            );
            match output.claims.claimant_of(block.id) {
                Some(claimant) => unit.claimed_by(claimant.label()),
                None => unit,
            }
        })
        .collect();

    let declared: Vec<Unit> = output
        .emitted_text()
        .into_iter()
        .map(|text| Unit::new("declared", None, text))
        .collect();
    let reachable: Vec<Unit> = output
        .reachable_text()
        .into_iter()
        .map(|text| Unit::new("reachable", None, text))
        .collect();

    let against_declared = diff(&in_units, &declared);
    let against_reachable = diff(&in_units, &reachable);

    writeln!(stdout, "diff-stage structure {}", path.display()).map_err(io)?;
    writeln!(stdout).map_err(io)?;
    writeln!(
        stdout,
        "  in        {:>9} characters over {} blocks",
        in_units
            .iter()
            .map(|unit| oc_model::ledger::c_of(&unit.text).total())
            .sum::<u64>(),
        in_units.len()
    )
    .map_err(io)?;

    report(stdout, "declared", &declared, &against_declared)?;
    report(stdout, "reachable", &reachable, &against_reachable)?;

    // The gap between the two is the silent half. `emitted_text` counting a container the
    // flow never points at is a loss that I-1 cannot see, so it is stated on its own line
    // rather than left for a reader to subtract.
    let silent = against_reachable
        .lost
        .total()
        .saturating_sub(against_declared.lost.total());
    writeln!(stdout).map_err(io)?;
    if silent > 0 {
        writeln!(
            stdout,
            "  SILENT LOSS  {silent} characters are in a container the flow does not reach:"
        )
        .map_err(io)?;
        writeln!(
            stdout,
            "               the stage balances against its own bag and the book is short."
        )
        .map_err(io)?;
    } else {
        writeln!(
            stdout,
            "  no silent loss: every container the stage built is reachable"
        )
        .map_err(io)?;
    }

    // Contested blocks: two claimants for one block means one of them will not emit it, and
    // which one is an accident of iteration order.
    let contested = output.claims.contested();
    if !contested.is_empty() {
        writeln!(stdout).map_err(io)?;
        writeln!(
            stdout,
            "  CONTESTED  {} blocks are claimed by more than one structure:",
            contested.len()
        )
        .map_err(io)?;
        for (block, claimants) in contested.iter().take(10) {
            let names: Vec<String> = claimants.iter().map(|by| by.label()).collect();
            writeln!(
                stdout,
                "    block {}  {}",
                block.as_str(),
                names.join(" + ")
            )
            .map_err(io)?;
        }
    }

    unaccounted(stdout, &against_reachable)?;

    Ok(against_reachable.balances())
}

fn report(stdout: &mut dyn Write, what: &str, units: &[Unit], result: &Diff) -> Result<(), String> {
    writeln!(
        stdout,
        "  {what:<9} {:>9} characters over {} units   lost {}  appeared {}",
        units
            .iter()
            .map(|unit| oc_model::ledger::c_of(&unit.text).total())
            .sum::<u64>(),
        units.len(),
        result.lost.total(),
        result.appeared.total(),
    )
    .map_err(io)
}

/// The blocks the output does not contain, with the claimant that took each one.
fn unaccounted(stdout: &mut dyn Write, result: &Diff) -> Result<(), String> {
    if result.unaccounted.is_empty() {
        return Ok(());
    }
    writeln!(stdout).map_err(io)?;
    writeln!(
        stdout,
        "  {} blocks are in the input and not in the book, longest first:",
        result.unaccounted.len()
    )
    .map_err(io)?;

    // By claimant kind first, because that is the defect class: a hundred blocks all taken by
    // lists is one bug and a hundred lines in a report is not.
    let mut by_claimant: std::collections::BTreeMap<String, (usize, u64)> =
        std::collections::BTreeMap::new();
    for unit in &result.unaccounted {
        let key = unit
            .claimed_by
            .clone()
            .map(|by| by.split_whitespace().next().unwrap_or("?").to_owned())
            .unwrap_or_else(|| "(nothing claimed it)".to_owned());
        let slot = by_claimant.entry(key).or_default();
        slot.0 += 1;
        slot.1 += oc_model::ledger::c_of(&unit.text).total();
    }
    writeln!(stdout).map_err(io)?;
    for (claimant, (count, chars)) in &by_claimant {
        writeln!(
            stdout,
            "    {claimant:<22} {count:>5} blocks  {chars:>9} characters"
        )
        .map_err(io)?;
    }
    writeln!(stdout).map_err(io)?;

    for unit in result.unaccounted.iter().take(20) {
        let page = unit
            .page
            .map(|page| format!("p{}", page + 1))
            .unwrap_or_else(|| "p?".to_owned());
        let claimant = unit.claimed_by.as_deref().unwrap_or("nothing claimed it");
        writeln!(
            stdout,
            "    {:<18} {page:<6} {claimant:<24} {:?}",
            unit.label,
            excerpt(&unit.text)
        )
        .map_err(io)?;
    }
    Ok(())
}

fn excerpt(text: &str) -> String {
    let normalised: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalised.chars().count() <= EXCERPT_CHARS {
        return normalised;
    }
    let head: String = normalised.chars().take(EXCERPT_CHARS).collect();
    format!("{head}…")
}

fn io(error: std::io::Error) -> String {
    error.to_string()
}

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

/// How many unaccounted blocks the report names for each kind of claimant.
const EXAMPLES_PER_KIND: usize = 6;

/// The stages this command can diff so far.
///
/// `structure` first because it is where every one of Phase 7's eleven refusals landed. The
/// others arrive as the classes that need them do: a command that claimed to diff twelve
/// stages and did one would be a worse lie than a command that says which one.
const IMPLEMENTED: &[&str] = &[oc_core::stages::TEXT.name, oc_core::stages::STRUCTURE.name];

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

    let result = if args.stage == oc_core::stages::TEXT.name {
        text_diff(document.as_ref(), &args.input, stdout)
    } else {
        structure_diff(document.as_ref(), &args.input, stdout)
    };
    match result {
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

/// Diff `text`: the glyph stream against the runs `N` produced from it.
///
/// The interesting failures here are **substitutions** — equal counts left and appeared —
/// because a substitution is a character the pipeline silently replaced with a different
/// one. The histograms name them exactly, which a stage error stating only two totals cannot.
fn text_diff(
    pdf: &dyn oc_pdf::inspect::PdfDoc,
    path: &std::path::Path,
    stdout: &mut dyn Write,
) -> Result<bool, String> {
    let t = &T;
    let input = openconvert::input::page_inputs(pdf).map_err(|error| error.to_string())?;

    let in_units: Vec<Unit> = input
        .iter()
        .map(|page| {
            let text: String = page
                .glyphs
                .iter()
                .filter(|glyph| !glyph.ch.is_whitespace())
                .map(|glyph| glyph.ch)
                .collect();
            Unit::new(
                format!("page {} glyphs", page.page.index + 1),
                Some(page.page.index),
                text,
            )
        })
        .collect();

    // Deliberately not `text_stage`: that one checks and refuses.
    let mut totals = oc_core::ledger_check::ReasonTotals::default();
    let staged = text_stage(&input, &mut totals, t);
    let (out_units, ledgered) = match &staged {
        Ok(stage) => (
            stage
                .pages
                .iter()
                .map(|page| {
                    let text: String = page.runs.iter().map(|run| run.text.as_str()).collect();
                    Unit::new(
                        format!("page {} runs", page.page.index + 1),
                        Some(page.page.index),
                        text,
                    )
                })
                .collect::<Vec<_>>(),
            0u64,
        ),
        // The stage refused, so nothing is returned to diff against. Re-running the pure
        // transform is what is left, and it is exactly what the check compared.
        Err(_) => (Vec::new(), 0u64),
    };
    let _ = ledgered;

    writeln!(stdout, "diff-stage text {}", path.display()).map_err(io)?;
    writeln!(stdout).map_err(io)?;

    if out_units.is_empty() {
        // Re-derive the output the way the stage does, without the check, so the difference
        // can be named rather than merely counted.
        let mut after = oc_model::extract::CharHistogram::new();
        let mut before = oc_model::extract::CharHistogram::new();
        for unit in &in_units {
            before = before.union(&oc_model::ledger::c_of(&unit.text));
        }
        for page in &input {
            for run in oc_text::words::assemble_runs(&page.glyphs, page.page.clone(), t).runs {
                after = after.union(&oc_model::ledger::c_of(&oc_text::normalize::normalized(
                    &run.text,
                )));
            }
        }
        let lost = before.difference(&after);
        let appeared = after.difference(&before);
        writeln!(
            stdout,
            "  the stage refused; the pure transform loses {} and invents {}",
            lost.total(),
            appeared.total()
        )
        .map_err(io)?;
        writeln!(stdout).map_err(io)?;
        report_chars(stdout, "LEFT    ", &lost)?;
        report_chars(stdout, "APPEARED", &appeared)?;
        return Ok(false);
    }

    let result = diff(&in_units, &out_units);
    writeln!(
        stdout,
        "  lost {}  appeared {}",
        result.lost.total(),
        result.appeared.total()
    )
    .map_err(io)?;
    report_chars(stdout, "LEFT    ", &result.lost)?;
    report_chars(stdout, "APPEARED", &result.appeared)?;
    Ok(result.balances())
}

/// Name the characters themselves, with their code points. "14 characters left" is a count;
/// "U+FB01 LATIN SMALL LIGATURE FI x14" is a diagnosis.
fn report_chars(
    stdout: &mut dyn Write,
    label: &str,
    histogram: &oc_model::extract::CharHistogram,
) -> Result<(), String> {
    if histogram.is_empty() {
        return Ok(());
    }
    let mut rows: Vec<(char, u32)> = histogram.iter().collect();
    rows.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    for (ch, count) in rows.iter().take(20) {
        writeln!(
            stdout,
            "  {label}  U+{:04X} {:?}  x{count}",
            u32::from(*ch),
            ch
        )
        .map_err(io)?;
    }
    Ok(())
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

    // Orphaned claimants: structures that took blocks out of the flow and were never placed
    // in it themselves. Grouped by kind, because the kind is the defect class and the ids are
    // only where to look.
    let orphaned = output.orphaned_claims();
    if !orphaned.is_empty() {
        let mut by_kind: std::collections::BTreeMap<
            &str,
            (usize, u64, std::collections::BTreeSet<String>),
        > = std::collections::BTreeMap::new();
        for claim in &orphaned {
            let slot = by_kind.entry(claim.by.kind()).or_default();
            slot.0 += 1;
            slot.1 += oc_model::ledger::c_of(&claim.text).total();
            slot.2.insert(claim.by.label());
        }
        writeln!(stdout).map_err(io)?;
        writeln!(
            stdout,
            "  ORPHANED  {} claimed blocks belong to a structure the book never reaches:",
            orphaned.len()
        )
        .map_err(io)?;
        for (kind, (blocks, chars, who)) in &by_kind {
            writeln!(
                stdout,
                "    {kind:<10} {blocks:>5} blocks  {chars:>9} characters  in {} structures",
                who.len()
            )
            .map_err(io)?;
        }
    }

    // Why a list is orphaned. A list enters the flow only when the loop reaches its first
    // item's first block *and that block is claimed*; the question worth one line of output is
    // whether that block is claimed, and what share of it the list actually took.
    let orphan_lists: std::collections::BTreeSet<String> = orphaned
        .iter()
        .filter(|claim| claim.by.kind() == "list")
        .filter_map(|claim| claim.by.id.clone())
        .collect();
    if !orphan_lists.is_empty() {
        let mut first_unclaimed = 0usize;
        for list in output
            .lists
            .iter()
            .filter(|l| orphan_lists.contains(l.id.as_str()))
        {
            let first = list
                .items
                .first()
                .and_then(|item| item.content.first())
                .and_then(|content| match content {
                    oc_model::doc::Content::Paragraph(para) => para.blocks.first().copied(),
                    _ => None,
                });
            if first.is_some_and(|block| !output.claims.contains(block)) {
                first_unclaimed += 1;
            }
        }
        writeln!(
            stdout,
            "    of {} orphaned lists, {} have a first block that is not itself claimed",
            orphan_lists.len(),
            first_unclaimed
        )
        .map_err(io)?;
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

    // The longest few *of each kind*, not the longest twenty overall. Captions and note
    // markers are short, so a single ranking by length hid them entirely behind paragraphs —
    // and a class that cannot be seen cannot be counted.
    let mut shown: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for unit in result.unaccounted.iter().filter(|unit| {
        let kind = unit
            .claimed_by
            .as_deref()
            .and_then(|by| by.split_whitespace().next())
            .unwrap_or("nothing")
            .to_owned();
        let count = shown.entry(kind).or_default();
        *count += 1;
        *count <= EXAMPLES_PER_KIND
    }) {
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

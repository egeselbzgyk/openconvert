//! The structural digest of a document tree (Phase 4 test 4.20).
//!
//! Counts and shapes, **no geometry and nothing derived from it**. That rule was learned in
//! Phase 3: a snapshot carrying an IoU in thousandths differed between Ubuntu and Windows
//! because an IoU is a ratio of areas and a substituted base-14 face gives host-dependent
//! glyph boxes (`docs/DECISIONS_LOG.md`, 2026-09-13). What is asserted across machines is the
//! *shape* of the answer.
//!
//! IR_SKETCH specifies the digest's contents: counts per `Content` variant and the heading
//! tree as `(level, text[..40])`. Both are here, plus the per-rule totals a Phase 4 regression
//! would move — notes and their match rate, figures, tables, lists, escalations.

use oc_model::doc::{Content, Section};
use oc_structure::stage::StructureOutput;
use serde::Serialize;

/// How many characters of a heading's text enter the digest (IR_SKETCH: `text[..40]`).
const HEADING_TEXT_CHARS: usize = 40;

/// The digest of a structured document.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Digest {
    /// Counts per `Content` variant, by name, every variant present with a zero.
    pub content: std::collections::BTreeMap<String, u32>,
    /// The heading tree, depth first: `(level, first 40 characters)`.
    pub headings: Vec<(u8, String)>,
    pub sections: u32,
    pub notes: u32,
    /// `NoteLinkStats.match_rate` in hundredths, as an integer — a bijection is 100 and
    /// anything else is a defect, and an integer cannot differ between hosts in its last bit.
    pub note_match_percent: u32,
    pub figures: u32,
    pub captioned_figures: u32,
    pub tables: u32,
    pub tables_as_image: u32,
    pub lists: u32,
    pub list_items: u32,
    pub images_kept: u32,
    pub images_dropped: u32,
    pub escalations: u32,
    pub warnings: Vec<String>,
    pub style_clusters: u32,
    pub inventory_valid: bool,
    pub metadata_source: String,
}

/// Reduce a structured document to its digest.
pub fn digest(output: &StructureOutput) -> Digest {
    let mut content: std::collections::BTreeMap<String, u32> = Content::VARIANTS
        .iter()
        .map(|name| ((*name).to_owned(), 0))
        .collect();
    let mut headings = Vec::new();
    let mut sections = 0u32;

    for root in &output.sections {
        for section in root.walk() {
            sections = sections.saturating_add(1);
            if let Some(heading) = &section.heading {
                headings.push((heading.level, clip(&heading.text())));
                // A section's heading is on the `Section`, not in its `content`, so counting
                // only the content would report a book of seven chapters as having no
                // headings at all.
                let entry = content.entry("heading".to_owned()).or_default();
                *entry = entry.saturating_add(1);
            }
            count(&section.content, &mut content);
        }
    }

    let mut warnings: Vec<String> = output
        .warnings
        .iter()
        .map(|warning| warning.code.to_owned())
        .collect();
    warnings.sort();

    Digest {
        content,
        headings,
        sections,
        notes: u32::try_from(output.notes.len()).unwrap_or(u32::MAX),
        note_match_percent: (f64::from(output.note_stats.match_rate) * 100.0).round() as u32,
        figures: u32::try_from(output.figures.len()).unwrap_or(u32::MAX),
        captioned_figures: u32::try_from(
            output
                .figures
                .iter()
                .filter(|figure| figure.caption.is_some())
                .count(),
        )
        .unwrap_or(u32::MAX),
        tables: u32::try_from(output.tables.len()).unwrap_or(u32::MAX),
        tables_as_image: u32::try_from(
            output
                .tables
                .iter()
                .filter(|table| table.fallback_image.is_some())
                .count(),
        )
        .unwrap_or(u32::MAX),
        lists: u32::try_from(output.lists.len()).unwrap_or(u32::MAX),
        list_items: u32::try_from(
            output
                .lists
                .iter()
                .map(|list| list.items.len())
                .sum::<usize>(),
        )
        .unwrap_or(u32::MAX),
        images_kept: u32::try_from(output.images.kept.len()).unwrap_or(u32::MAX),
        images_dropped: u32::try_from(output.images.dropped.len()).unwrap_or(u32::MAX),
        escalations: u32::try_from(output.escalations.len()).unwrap_or(u32::MAX),
        warnings,
        style_clusters: u32::try_from(output.inventory.clusters.len()).unwrap_or(u32::MAX),
        inventory_valid: output.inventory.valid,
        metadata_source: format!("{:?}", output.metadata.source).to_lowercase(),
    }
}

/// Count a content list's variants, recursing into the three that wrap others.
fn count(items: &[Content], into: &mut std::collections::BTreeMap<String, u32>) {
    for item in items {
        let entry = into.entry(item.variant().to_owned()).or_default();
        *entry = entry.saturating_add(1);
        match item {
            Content::BlockQuote(inner) | Content::Epigraph(inner) => count(inner, into),
            Content::List(list) => count_list(list, into),
            _ => {}
        }
    }
}

fn count_list(list: &oc_model::doc::List, into: &mut std::collections::BTreeMap<String, u32>) {
    for item in &list.items {
        count(&item.content, into);
        if let Some(nested) = &item.nested {
            count_list(nested, into);
        }
    }
}

/// The first [`HEADING_TEXT_CHARS`] characters, by character rather than by byte.
fn clip(text: &str) -> String {
    text.chars().take(HEADING_TEXT_CHARS).collect()
}

/// Every section of a document tree, depth first — re-exported because a caller that wants
/// the tree rather than its digest should not have to reimplement the walk.
pub fn walk(roots: &[Section]) -> Vec<&Section> {
    roots.iter().flat_map(Section::walk).collect()
}

/// The stage name, as `--dump-stage` spells it.
pub const STAGE: &str = "structure";

/// The dump's header: what the stage decided about the document as a whole.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Header {
    pub ir_version: u32,
    pub schema: &'static str,
    pub stage: &'static str,
    pub check: oc_model::ledger::StageCheck,
    pub digest: Digest,
    pub metadata: oc_model::doc::Metadata,
    pub note_match_rate: f32,
    pub escalation_candidates: u32,
    /// Whether an LLM escalation may ever be attempted for this book's heading roles
    /// (IMPLEMENTATION_PLAN Phase 4 detail 3).
    pub escalation_allowed: bool,
}

/// One top-level section, as a line of the dump.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SectionLine {
    pub id: String,
    pub role: oc_model::doc::SectionRole,
    pub level: u8,
    pub heading: Option<String>,
    pub source_pages: (u32, u32),
    pub children: u32,
    pub content: u32,
}

/// Run every stage up to and including `structure`, and reduce the result to a dump.
///
/// Like the `text` dump and unlike `ingest`'s, this cannot start writing at page one: the
/// style histogram is over the whole book, the outline binds across it, and the section tree
/// is not a section tree until the last heading has been seen.
pub fn dump(
    document: &dyn oc_pdf::inspect::PdfDoc,
    filename: &str,
    source_sha256: &str,
    lang: oc_model::lang::LangTag,
    t: &oc_core::thresholds::Thresholds,
) -> Result<(Header, Vec<SectionLine>), String> {
    use oc_core::ledger_check::ReasonTotals;
    use oc_structure::meta::{InfoDict, MetaSources};
    use oc_structure::stage::StructureInput;

    let input = crate::input::page_inputs(document).map_err(|error| error.to_string())?;
    let mut totals = ReasonTotals::default();
    let text = crate::pipeline::text_stage(&input, &mut totals, t).map_err(fail)?;
    let furniture =
        crate::pipeline::furniture_stage(&text, lang.clone(), &mut totals, t).map_err(fail)?;
    let layout = crate::pipeline::layout_stage(&text, &furniture, &mut totals, t).map_err(fail)?;

    let images = crate::structure_input::document_images(&text);
    // The one decoder, which hashes only what the ornament rule reads. This site used to keep
    // its own copy of the decode-and-hash loop, which is how a change to one would have left
    // `dump-stage structure` disagreeing with `convert` about which images are ornaments.
    let image_hashes = crate::convert::image_hashes(
        document,
        &images,
        &crate::structure_input::image_slots(&text),
        t,
    );
    let doc_info = document.doc_info();

    let stage_input = StructureInput {
        blocks: crate::structure_input::block_views(&text, &layout),
        runs: crate::pipeline::body_runs(&text, &furniture),
        fonts: text.fonts.clone(),
        images,
        image_hashes,
        vectors: (0..document.page_count())
            .filter_map(|page| document.page_vectors(page).ok())
            .flatten()
            .collect(),
        outline: document.outline(),
        labels: furniture.labels.clone(),
        drop_caps: layout.drop_caps.iter().flatten().cloned().collect(),
        page_count: document.page_count(),
        meta: MetaSources {
            xmp: document.xmp(),
            info: InfoDict {
                title: doc_info.title.clone(),
                author: doc_info.author.clone(),
            },
            filename: filename.to_owned(),
            source_sha256: source_sha256.to_owned(),
            language: lang.clone(),
        },
        lang,
    };
    let stage =
        crate::pipeline::structure_stage(&layout, &stage_input, &mut totals, t).map_err(fail)?;

    let sections = stage
        .output
        .sections
        .iter()
        .map(|section| SectionLine {
            id: section.id.to_string(),
            role: section.role,
            level: section.level,
            heading: section.heading.as_ref().map(oc_model::doc::Heading::text),
            source_pages: section.source_pages,
            children: u32::try_from(section.children.len()).unwrap_or(u32::MAX),
            content: u32::try_from(section.content.len()).unwrap_or(u32::MAX),
        })
        .collect();

    Ok((
        Header {
            ir_version: oc_model::IR_VERSION,
            schema: "openconvert.dump.structure/1",
            stage: STAGE,
            check: stage.check.clone(),
            digest: digest(&stage.output),
            metadata: stage.output.metadata.clone(),
            note_match_rate: stage.output.note_stats.match_rate,
            escalation_candidates: u32::try_from(stage.output.escalations.len())
                .unwrap_or(u32::MAX),
            escalation_allowed: stage.output.inventory.escalation_allowed,
        },
        sections,
    ))
}

fn fail(error: oc_core::ledger_check::ConservationError) -> String {
    error.to_string()
}

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

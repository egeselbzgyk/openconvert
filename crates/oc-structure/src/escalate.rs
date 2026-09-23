//! Which of the four LLM tasks this book escalates, and on what evidence (PHASE 10 detail 1,
//! ARCHITECTURE §6.1, D13.5's gate D).
//!
//! **Gathering, not judging.** Each predicate is `oc_core::escalation`'s — pure, table-tested in
//! Phase 8, and the one definition of when the deterministic evidence is too weak to settle a
//! choice. What this module adds is the evidence: which title the file declares, how many entries
//! the outline and the printed contents page have, how many heading styles the inventory offers
//! and how much of the book numbering orders, which indented blocks `quotes` could not settle. The
//! stage owns that evidence, so the stage gathers it; `oc-core` judges it.
//!
//! **Every escalation is a record, whether or not a model is ever asked.** v1 has no calibration,
//! so the first books converted *are* the calibration corpus (RT A7.2): a record carries the task,
//! the predicate that fired, the signals it read, and a hash of that evidence, and it is written to
//! the report with `ai.enabled = false` exactly as with it on. The model is one consumer of these
//! records; `eval calibrate` is the other.

use oc_core::escalation::{
    self, BookStructureEvidence, HeadingRolesEvidence, MetadataEvidence, Verdict,
    VerseQuoteEvidence,
};
use oc_core::thresholds::Thresholds;
use oc_model::confidence::Signal;
use oc_model::extract::OutlineEntry;
use oc_model::ids::BlockId;
use serde::Serialize;

use crate::headings::cluster::StyleInventory;
use crate::headings::levels::HeadingAssignment;
use crate::headings::toc_page::TocPage;
use crate::meta::{is_boilerplate, MetaSources};
use crate::quotes::EscalationCandidate;

/// Task 1, as ARCHITECTURE §9.6 names it.
pub const TASK_METADATA: &str = "metadata";
/// Task 2.
pub const TASK_HEADING_ROLES: &str = "heading_roles";
/// Task 3.
pub const TASK_BOOK_STRUCTURE: &str = "book_structure";
/// Task 4.
pub const TASK_VERSE_QUOTE: &str = crate::quotes::TASK_VERSE_QUOTE;

/// Whether a choice is escalated: why not, or the record of why.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Escalation {
    /// The deterministic evidence settles it, and this is why.
    No(&'static str),
    Yes(EscalationRecord),
}

impl Escalation {
    pub fn record(&self) -> Option<&EscalationRecord> {
        match self {
            Escalation::Yes(record) => Some(record),
            Escalation::No(_) => None,
        }
    }

    pub fn is_escalated(&self) -> bool {
        self.record().is_some()
    }
}

/// One escalated choice, as the report and the calibration corpus carry it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EscalationRecord {
    /// The task, as ARCHITECTURE §9.6 names it.
    pub task: &'static str,
    /// The block the choice is about, for a per-block task; `None` for a choice about the book.
    pub subject: Option<BlockId>,
    /// Why the predicate fired, in its own words.
    pub predicate: &'static str,
    /// What it read. The data a later calibration consumes (ARCHITECTURE §6.1).
    pub signals: Vec<Signal>,
    /// A hash of the task, the subject and the signals: two records with the same hash were
    /// escalated on the same evidence. Hex, BLAKE3.
    pub input_hash: String,
}

impl EscalationRecord {
    fn new(
        task: &'static str,
        subject: Option<BlockId>,
        predicate: &'static str,
        signals: Vec<Signal>,
    ) -> Self {
        let mut hasher = blake3::Hasher::new();
        for part in [
            task.as_bytes(),
            subject
                .as_ref()
                .map_or(&b""[..], |id| id.as_str().as_bytes()),
        ] {
            hasher.update(&u64::try_from(part.len()).unwrap_or(u64::MAX).to_le_bytes());
            hasher.update(part);
        }
        for signal in &signals {
            hasher.update(
                &u64::try_from(signal.name.len())
                    .unwrap_or(u64::MAX)
                    .to_le_bytes(),
            );
            hasher.update(signal.name.as_bytes());
            hasher.update(&signal.value.to_bits().to_le_bytes());
        }
        Self {
            task,
            subject,
            predicate,
            signals,
            input_hash: hasher.finalize().to_hex().to_string(),
        }
    }
}

/// The verdict as an escalation, with the record built only when it fired.
fn escalation(
    task: &'static str,
    subject: Option<BlockId>,
    verdict: Verdict,
    signals: Vec<Signal>,
) -> Escalation {
    match verdict {
        Verdict::Fires(because) => {
            Escalation::Yes(EscalationRecord::new(task, subject, because, signals))
        }
        Verdict::Abstains(because) => Escalation::No(because),
    }
}

fn flag(value: bool) -> f32 {
    f32::from(u8::from(value))
}

/// Task 1: does the file declare a title that is not boilerplate?
///
/// XMP and the Info dictionary are both declarations. The title counts as declared when either
/// names one, and as boilerplate only when every one they name is: a Word export whose XMP packet
/// carries the real title has a real title (PIPELINE §8.8).
pub fn metadata(sources: &MetaSources) -> Escalation {
    let declared: Vec<&str> = [sources.xmp.title.as_deref(), sources.info.title.as_deref()]
        .into_iter()
        .flatten()
        .filter(|title| !title.trim().is_empty())
        .collect();
    let evidence = MetadataEvidence {
        title_present: !declared.is_empty(),
        title_is_boilerplate: !declared.is_empty()
            && declared.iter().all(|title| is_boilerplate(title)),
    };
    escalation(
        TASK_METADATA,
        None,
        escalation::metadata(&evidence),
        vec![
            Signal::new("title_present", flag(evidence.title_present)),
            Signal::new("title_is_boilerplate", flag(evidence.title_is_boilerplate)),
        ],
    )
}

/// Task 3: does the book say where its parts are? An outline does, and so does a printed contents
/// page of at least `toc.min_entries` entries.
pub fn book_structure(
    outline: &[OutlineEntry],
    toc: Option<&TocPage>,
    t: &Thresholds,
) -> Escalation {
    let evidence = BookStructureEvidence {
        outline_entries: u32::try_from(outline.len()).unwrap_or(u32::MAX),
        toc_entries: toc.map_or(0, |toc| {
            u32::try_from(toc.entries.len()).unwrap_or(u32::MAX)
        }),
    };
    escalation(
        TASK_BOOK_STRUCTURE,
        None,
        escalation::book_structure(&evidence, t),
        vec![
            Signal::new("outline_entries", evidence.outline_entries as f32),
            Signal::new("toc_entries", evidence.toc_entries as f32),
        ],
    )
}

/// Task 2: are there several heading styles, and does numbering leave some of them unordered?
pub fn heading_roles(
    inventory: &StyleInventory,
    headings: &[HeadingAssignment],
    t: &Thresholds,
) -> Escalation {
    let evidence = HeadingRolesEvidence {
        candidate_clusters: u32::try_from(inventory.candidates(t).len()).unwrap_or(u32::MAX),
        headings: u32::try_from(headings.len()).unwrap_or(u32::MAX),
        numbered_headings: u32::try_from(
            headings
                .iter()
                .filter(|heading| heading.numbering.is_some())
                .count(),
        )
        .unwrap_or(u32::MAX),
    };
    escalation(
        TASK_HEADING_ROLES,
        None,
        escalation::heading_roles(&evidence),
        vec![
            Signal::new("candidate_clusters", evidence.candidate_clusters as f32),
            Signal::new("headings", evidence.headings as f32),
            Signal::new("numbered_headings", evidence.numbered_headings as f32),
            Signal::new("cluster_count", inventory.clusters.len() as f32),
        ],
    )
}

/// Task 4: each indented block `quotes` could not settle, in reading order, against the book's
/// block budget of `llm.max_blocks_per_book`.
///
/// The budget is spent in reading order, so the block that finds it spent is the same block on
/// every run, and every block after it takes the deterministic default without a record of an
/// escalation — the predicate abstains on it, and says why.
pub fn verse_quote(
    candidates: &[EscalationCandidate],
    t: &Thresholds,
) -> Vec<(BlockId, Escalation)> {
    let cap = u32::try_from(t.llm.max_blocks_per_book).unwrap_or_default();
    let mut escalated: u32 = 0;
    candidates
        .iter()
        .map(|candidate| {
            let signal = |name: &str| {
                candidate
                    .signals
                    .iter()
                    .find(|signal| signal.name == name)
                    .map_or(0.0, |signal| signal.value)
            };
            let evidence = VerseQuoteEvidence {
                // A candidate is a block `quotes` found set in from the margin: that is what
                // made it a candidate at all.
                indented: true,
                short_line_ratio: signal("short_line_ratio"),
                blocks_remaining: cap.saturating_sub(escalated),
            };
            let result = escalation(
                TASK_VERSE_QUOTE,
                Some(candidate.block),
                escalation::verse_quote(&evidence, t),
                candidate.signals.clone(),
            );
            if result.is_escalated() {
                escalated = escalated.saturating_add(1);
            }
            (candidate.block, result)
        })
        .collect()
}

/// All four, for one book.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Escalations {
    pub metadata: Escalation,
    pub heading_roles: Escalation,
    pub book_structure: Escalation,
    /// Per ambiguous indented block, in reading order.
    pub verse_quote: Vec<(BlockId, Escalation)>,
}

impl Escalations {
    /// Gather the evidence of a finished `structure` run and ask the four predicates.
    pub fn gather(
        input: &crate::stage::StructureInput,
        output: &crate::stage::StructureOutput,
        t: &Thresholds,
    ) -> Self {
        Self {
            metadata: metadata(&input.meta),
            heading_roles: heading_roles(&output.inventory, &output.headings, t),
            book_structure: book_structure(&input.outline, output.toc.as_ref(), t),
            verse_quote: verse_quote(&output.escalations, t),
        }
    }

    /// Every record, in task order and then in reading order: what the report carries.
    pub fn records(&self) -> Vec<EscalationRecord> {
        [&self.metadata, &self.heading_roles, &self.book_structure]
            .into_iter()
            .chain(self.verse_quote.iter().map(|(_, escalation)| escalation))
            .filter_map(Escalation::record)
            .cloned()
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use oc_core::thresholds::T;
    use oc_model::confidence::Confidence;
    use oc_model::ids::ClusterId;
    use oc_model::lang::LangTag;

    use crate::headings::cluster::StyleCluster;
    use crate::headings::levels::LevelSource;
    use crate::headings::numbering::{Numbering, NumberingKind};
    use crate::headings::toc_page::TocEntry;
    use crate::meta::{oc_pdf_meta::XmpMeta, InfoDict};
    use crate::quotes::IndentedKind;

    fn sources(xmp: Option<&str>, info: Option<&str>) -> MetaSources {
        MetaSources {
            xmp: XmpMeta {
                title: xmp.map(str::to_owned),
                ..XmpMeta::default()
            },
            info: InfoDict {
                title: info.map(str::to_owned),
                author: None,
            },
            filename: "book.pdf".to_owned(),
            source_sha256: "0".repeat(64),
            language: LangTag::EN,
        }
    }

    fn outline(entries: usize) -> Vec<OutlineEntry> {
        (0..entries)
            .map(|index| OutlineEntry {
                title: format!("Chapter {index}"),
                level: 0,
                page: Some(u32::try_from(index).unwrap_or_default()),
            })
            .collect()
    }

    fn toc(entries: usize) -> TocPage {
        TocPage {
            page: 1,
            entries: (0..entries)
                .map(|index| TocEntry {
                    title: format!("Chapter {index}"),
                    folio: index.to_string(),
                    level: 1,
                })
                .collect(),
            line_share: 1.0,
        }
    }

    fn cluster(id: u32, size_pt: f32, chars: u64) -> StyleCluster {
        StyleCluster {
            id: ClusterId(id),
            size_pt,
            weight: 400,
            italic: false,
            family_key: "serif".to_owned(),
            char_count: chars,
            size_z: 0.0,
            starts_page_ratio: 0.0,
            centered_ratio: 0.0,
            examples: Vec::new(),
        }
    }

    /// An inventory with a 10 pt body and `extra` larger styles, each a heading candidate.
    fn inventory(extra: u32) -> StyleInventory {
        let mut clusters: Vec<StyleCluster> = (0..extra)
            .map(|rank| cluster(rank, 20.0 - rank as f32, 40))
            .collect();
        clusters.push(cluster(extra, 10.0, 10_000));
        StyleInventory {
            body: Some(ClusterId(extra)),
            total_chars: clusters.iter().map(|cluster| cluster.char_count).sum(),
            clusters,
            valid: true,
            escalation_allowed: true,
            warnings: Vec::new(),
            confidence: Confidence::deterministic(Vec::new()),
        }
    }

    fn headings(count: usize, numbered: usize) -> Vec<HeadingAssignment> {
        (0..count)
            .map(|index| HeadingAssignment {
                block: BlockId::derive(
                    u32::try_from(index).unwrap_or_default(),
                    oc_model::geom::Rect {
                        x0: 0.0,
                        y0: 0.0,
                        x1: 10.0,
                        y1: 10.0,
                    },
                    "heading",
                ),
                order: u32::try_from(index).unwrap_or_default(),
                page: u32::try_from(index).unwrap_or_default(),
                text: format!("Heading {index}"),
                level: 1,
                cluster: ClusterId(0),
                numbering: (index < numbered).then(|| Numbering {
                    kind: NumberingKind::Chapter,
                    value: index.to_string(),
                    level: 1,
                }),
                source: LevelSource::SizeRank,
            })
            .collect()
    }

    fn candidate(index: u32, ratio: f32) -> EscalationCandidate {
        EscalationCandidate {
            block: BlockId::derive(
                index,
                oc_model::geom::Rect {
                    x0: 40.0,
                    y0: 0.0,
                    x1: 300.0,
                    y1: 80.0,
                },
                "an indented block",
            ),
            task: TASK_VERSE_QUOTE,
            chosen: IndentedKind::BlockQuote,
            alternatives: vec![IndentedKind::Verse, IndentedKind::Paragraph],
            signals: vec![
                Signal::new("indent_pt", 40.0),
                Signal::new("short_line_ratio", ratio),
                Signal::new("lines", 6.0),
            ],
        }
    }

    /// Row 10.1. Each of the four predicates, fed the evidence this stage gathers, fires on its
    /// named case and abstains on the negative one — and a record exists exactly when it fires.
    #[test]
    fn escalation_predicates_fire_and_abstain() {
        // (task, case, escalation, fires)
        let mut table: Vec<(&str, &str, Escalation, bool)> = vec![
            (
                TASK_METADATA,
                "no title declared",
                metadata(&sources(None, None)),
                true,
            ),
            (
                TASK_METADATA,
                "only a Word export's title",
                metadata(&sources(None, Some("Microsoft Word - draft.docx"))),
                true,
            ),
            (
                TASK_METADATA,
                "a boilerplate Info title, the real one in XMP",
                metadata(&sources(
                    Some("Die Verwandlung"),
                    Some("Microsoft Word - draft.docx"),
                )),
                false,
            ),
            (
                TASK_METADATA,
                "a declared title",
                metadata(&sources(None, Some("Die Verwandlung"))),
                false,
            ),
            (
                TASK_BOOK_STRUCTURE,
                "no outline, no contents page",
                book_structure(&[], None, &T),
                true,
            ),
            (
                TASK_BOOK_STRUCTURE,
                "no outline, a two-line contents page",
                book_structure(&[], Some(&toc(2)), &T),
                true,
            ),
            (
                TASK_BOOK_STRUCTURE,
                "an outline",
                book_structure(&outline(12), None, &T),
                false,
            ),
            (
                TASK_BOOK_STRUCTURE,
                "a contents page",
                book_structure(&[], Some(&toc(9)), &T),
                false,
            ),
            (
                TASK_HEADING_ROLES,
                "three heading styles, a third numbered",
                heading_roles(&inventory(3), &headings(9, 3), &T),
                true,
            ),
            (
                TASK_HEADING_ROLES,
                "three heading styles, all numbered",
                heading_roles(&inventory(3), &headings(9, 9), &T),
                false,
            ),
            (
                TASK_HEADING_ROLES,
                "one heading style",
                heading_roles(&inventory(1), &headings(9, 0), &T),
                false,
            ),
        ];
        let blocks = verse_quote(
            &[candidate(0, 0.5), candidate(1, 0.9), candidate(2, 0.1)],
            &T,
        );
        table.push((TASK_VERSE_QUOTE, "ratio 0.5", blocks[0].1.clone(), true));
        table.push((
            TASK_VERSE_QUOTE,
            "ratio 0.9: verse",
            blocks[1].1.clone(),
            false,
        ));
        table.push((
            TASK_VERSE_QUOTE,
            "ratio 0.1: a quotation",
            blocks[2].1.clone(),
            false,
        ));

        for (task, case, escalation, fires) in &table {
            assert_eq!(
                escalation.is_escalated(),
                *fires,
                "{task} on {case}: {escalation:?}"
            );
            if let Escalation::Yes(record) = escalation {
                assert_eq!(record.task, *task);
                assert!(!record.predicate.is_empty());
                assert!(!record.signals.is_empty(), "a record carries its evidence");
                assert_eq!(record.input_hash.len(), 64);
            }
        }
        for task in [
            TASK_METADATA,
            TASK_HEADING_ROLES,
            TASK_BOOK_STRUCTURE,
            TASK_VERSE_QUOTE,
        ] {
            let outcomes: std::collections::BTreeSet<bool> = table
                .iter()
                .filter(|(name, ..)| *name == task)
                .map(|(.., fires)| *fires)
                .collect();
            assert_eq!(outcomes.len(), 2, "{task} is not shown both ways");
        }

        // A per-block record names its block; a book-level one names none.
        assert_eq!(
            blocks[0].1.record().and_then(|record| record.subject),
            Some(blocks[0].0)
        );
        assert!(metadata(&sources(None, None))
            .record()
            .is_some_and(|record| record.subject.is_none()));
    }

    /// The block budget is spent in reading order: with `llm.max_blocks_per_book` ambiguous blocks
    /// already escalated, the next one is not, and says why.
    #[test]
    fn the_block_budget_is_spent_in_reading_order() {
        let cap = usize::try_from(T.llm.max_blocks_per_book).unwrap_or_default();
        let candidates: Vec<EscalationCandidate> = (0..=cap)
            .map(|index| candidate(u32::try_from(index).unwrap_or_default(), 0.5))
            .collect();
        let blocks = verse_quote(&candidates, &T);
        assert!(blocks[..cap]
            .iter()
            .all(|(_, escalation)| escalation.is_escalated()));
        assert_eq!(
            blocks[cap].1,
            Escalation::No("the book's block budget is spent")
        );
    }

    /// The hash names the evidence: the same evidence, the same hash; other evidence, another.
    #[test]
    fn a_record_hash_is_a_function_of_its_evidence() {
        let first = verse_quote(&[candidate(0, 0.5)], &T);
        let again = verse_quote(&[candidate(0, 0.5)], &T);
        let other = verse_quote(&[candidate(0, 0.6)], &T);
        let hash = |blocks: &[(BlockId, Escalation)]| {
            blocks[0]
                .1
                .record()
                .map(|record| record.input_hash.clone())
                .unwrap_or_default()
        };
        assert_eq!(hash(&first), hash(&again));
        assert_ne!(hash(&first), hash(&other));
    }
}

//! The AI step: the four tasks, asked where their evidence is and applied the way the
//! deterministic answer was (PHASE 10, D13.5, D13.6).
//!
//! `oc-ai` owns the questions, the gates and each task's validation; `oc-structure` owns the
//! evidence and the code that turns a label into a book. This module is where the two meet: it
//! builds each task's payload from what `structure` measured, turns each admitted answer into a
//! [`StructureEdits`](oc_structure::stage::StructureEdits), and — nowhere else — applies it.

use oc_ai::task::heading_roles::{Demotion, RoleEdit};
use oc_model::ids::ClusterId;
use oc_structure::headings::levels::{HeadingDemotion, HeadingEdits};

/// Task 2's edit, in `oc-structure`'s terms. A running-head proposal has no counterpart: it is a
/// proposal, and furniture has already decided (D13.5).
pub fn heading_edits(edit: &RoleEdit) -> HeadingEdits {
    HeadingEdits {
        levels: edit
            .levels
            .iter()
            .map(|(cluster, level)| (ClusterId(*cluster), *level))
            .collect(),
        demote: edit
            .demote
            .iter()
            .map(|(cluster, demotion)| {
                (
                    ClusterId(*cluster),
                    match demotion {
                        Demotion::Paragraph => HeadingDemotion::Paragraph,
                        Demotion::Epigraph => HeadingDemotion::Epigraph,
                    },
                )
            })
            .collect(),
    }
}

/// Task 3's edit, in `oc-structure`'s terms.
pub fn zone_edits(edit: &oc_ai::task::book_structure::ZoneEdit) -> oc_structure::book::ZoneEdits {
    use oc_ai::task::book_structure::Zone as AiZone;
    use oc_model::doc::Zone;
    oc_structure::book::ZoneEdits {
        labels: edit
            .labels
            .iter()
            .map(|(index, label)| {
                (
                    *index,
                    oc_structure::book::ZoneLabel {
                        zone: match label.zone {
                            AiZone::Front => Zone::Front,
                            AiZone::Body => Zone::Body,
                            AiZone::Back => Zone::Back,
                        },
                        part: label.part,
                    },
                )
            })
            .collect(),
    }
}

/// Task 4's labels, in `oc-structure`'s terms.
pub fn indented_kinds(
    edit: &oc_ai::task::verse_quote::KindEdit,
) -> std::collections::BTreeMap<oc_model::ids::BlockId, oc_structure::quotes::IndentedKind> {
    use oc_ai::prompt::v1::verse_quote::BlockKind;
    use oc_structure::quotes::IndentedKind;
    edit.kinds
        .iter()
        .map(|(block, kind)| {
            (
                *block,
                match kind {
                    BlockKind::Verse => IndentedKind::Verse,
                    BlockKind::Blockquote => IndentedKind::BlockQuote,
                    BlockKind::Preformatted => IndentedKind::Pre,
                    BlockKind::Paragraph => IndentedKind::Paragraph,
                },
            )
        })
        .collect()
}

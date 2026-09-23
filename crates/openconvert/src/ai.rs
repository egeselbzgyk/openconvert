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

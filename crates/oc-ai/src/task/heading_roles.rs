//! Task 2, `heading_roles`: the pre-gate, the held-out check, the role rules, and the edit an
//! admitted mapping makes (PHASE 10 detail 3, ARCHITECTURE §9.6, R10 §6.7, RT A8).
//!
//! **The pre-gate** refuses to ask at all when the scaffolding is invalid — more than
//! `inventory.max_clusters` clusters, or a body cluster under `inventory.min_body_char_share` of
//! the characters. No model can rescue a clustering that is not there (RT A8.3).
//!
//! **The held-out check** is a free oracle for "the clustering was wrong": 8–10 single lines
//! sampled from the clusters, not shown as examples, ride along in the same call; if the labels
//! the model gives them disagree with the labels it gave their clusters on more than
//! `inventory.holdout_disagree_max` of them, the whole mapping is rejected (RT A8.2).
//!
//! **Label authority is not deletion authority** (D13.5, RT A8.1). A `running_head` label is a
//! *proposal*: only the deterministic furniture remover deletes, and only on its own cross-page
//! repetition evidence — which, for anything still in the flow when this task runs, has already
//! said no. So the edit this module makes has no way to say "delete": it holds heading levels,
//! a demotion from heading to paragraph or epigraph, and proposals, and nothing else — the
//! property test 10.8 holds over two thousand generated mappings.

use std::collections::{BTreeMap, BTreeSet};

use crate::gates::GateFailure;
use crate::prompt::v1::heading_roles::{HeadingRole, HeadingRolesAnswer, HeadingRolesInput};
use crate::task::{InventoryFacts, PreGateFailure};

/// The numbers the task reads, from `thresholds.toml`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeadingRolesLimits {
    /// `inventory.max_clusters`.
    pub max_clusters: usize,
    /// `inventory.min_body_char_share`.
    pub min_body_char_share: f32,
    /// `inventory.holdout_min_probes`.
    pub min_probes: usize,
    /// `inventory.holdout_disagree_max`.
    pub holdout_disagree_max: f32,
    /// `inventory.chapter_cluster_min_count` and `…_max_count`: ARCHITECTURE §9.6's
    /// "chapter_heading clusters must have count ≥ 2 and ≤ 200".
    pub chapter_min_count: u32,
    pub chapter_max_count: u32,
}

/// Refuse the call when the inventory is not scaffolding a model could label (RT A8.3).
///
/// ARCHITECTURE §9.6 names a third condition, a silhouette floor. No silhouette is computed by
/// the clustering and no floor is in `thresholds.toml`, so it is not checked here
/// (`docs/DECISIONS_LOG.md`, 2026-09-23).
pub fn inventory_pre_gate(
    facts: &InventoryFacts,
    limits: &HeadingRolesLimits,
) -> Result<(), PreGateFailure> {
    if facts.clusters > limits.max_clusters {
        return Err(PreGateFailure::TooManyClusters(*facts));
    }
    if facts.body_char_share < limits.min_body_char_share {
        return Err(PreGateFailure::BodyTooThin(*facts));
    }
    Ok(())
}

/// Refuse the call when too few held-out lines could be sampled to check the mapping against
/// itself.
pub fn probe_pre_gate(
    probes: &[HoldoutProbe],
    limits: &HeadingRolesLimits,
) -> Result<(), PreGateFailure> {
    if probes.len() < limits.min_probes {
        return Err(PreGateFailure::TooFewProbes {
            probes: probes.len(),
            min: limits.min_probes,
        });
    }
    Ok(())
}

/// One held-out line of the payload, with the cluster it was sampled from — which the model is
/// not told.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HoldoutProbe {
    /// The probe's index in the payload.
    pub i: u32,
    pub cluster: u32,
}

/// The share of held-out lines whose label disagrees with their cluster's. Zero probes is zero
/// disagreement, which the probe pre-gate makes unreachable in practice.
pub fn holdout_check(answer: &HeadingRolesAnswer, probes: &[HoldoutProbe]) -> f32 {
    if probes.is_empty() {
        return 0.0;
    }
    let disagreeing = probes
        .iter()
        .filter(|probe| answer.probes.get(&probe.i) != answer.clusters.get(&probe.cluster))
        .count();
    disagreeing as f32 / probes.len() as f32
}

/// The task's validation of a mapping gate S admitted: the held-out check first, then the role
/// rules. Any failure rejects the whole mapping.
///
/// `heading_clusters` are the clusters that hold at least one heading the deterministic path
/// found; `input` is the payload the model was shown.
pub fn validate_roles(
    answer: &HeadingRolesAnswer,
    input: &HeadingRolesInput,
    probes: &[HoldoutProbe],
    heading_clusters: &BTreeSet<u32>,
    limits: &HeadingRolesLimits,
) -> Result<(), GateFailure> {
    let rate = holdout_check(answer, probes);
    if rate > limits.holdout_disagree_max {
        return Err(GateFailure::HoldoutDisagrees { rate });
    }

    for cluster in &input.clusters {
        if answer.clusters.get(&cluster.c) == Some(&HeadingRole::ChapterHeading)
            && (cluster.count < limits.chapter_min_count
                || cluster.count > limits.chapter_max_count)
        {
            return Err(GateFailure::RoleRule(
                "a chapter_heading cluster holds too few or too many runs",
            ));
        }
    }

    // `epigraph` is never the most frequent large-font cluster: a style the book sets over and
    // over at a large size is structure, not a quotation before a chapter.
    let most_frequent_large = input
        .clusters
        .iter()
        .filter(|cluster| cluster.size_z > 0.0)
        .max_by_key(|cluster| (cluster.count, std::cmp::Reverse(cluster.c)));
    if let Some(cluster) = most_frequent_large {
        if answer.clusters.get(&cluster.c) == Some(&HeadingRole::Epigraph) {
            return Err(GateFailure::RoleRule(
                "epigraph is the most frequent large-font cluster",
            ));
        }
    }

    // A mapping that demotes every cluster holding a heading would take the book's whole
    // navigation away on one answer. The deterministic path found headings; a mapping that
    // leaves none is refused rather than trusted.
    if !heading_clusters.is_empty()
        && heading_clusters
            .iter()
            .all(|cluster| demotion(answer.clusters.get(cluster)).is_some())
    {
        return Err(GateFailure::RoleRule("the mapping demotes every heading"));
    }
    Ok(())
}

/// What a demoted heading becomes. Its text stays exactly where it was.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Demotion {
    /// An ordinary paragraph: the model said the style is body text.
    Paragraph,
    /// An epigraph: a quotation set before a chapter's body.
    Epigraph,
}

/// The edit an admitted mapping makes: per cluster, a heading level or a demotion, and the
/// clusters it proposed as running heads. There is no variant that removes anything.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoleEdit {
    /// Heading level, 1..=6, for the headings of each cluster.
    pub levels: BTreeMap<u32, u8>,
    /// Headings of these clusters are not headings.
    pub demote: BTreeMap<u32, Demotion>,
    /// Clusters the model called running heads. A proposal only: furniture decides, and has.
    pub running_head_proposals: BTreeSet<u32>,
}

fn demotion(role: Option<&HeadingRole>) -> Option<Demotion> {
    match role {
        Some(HeadingRole::Body) => Some(Demotion::Paragraph),
        Some(HeadingRole::Epigraph) => Some(Demotion::Epigraph),
        _ => None,
    }
}

/// The deepest heading level XHTML has.
const MAX_LEVEL: u8 = 6;
/// A part heading is the top of the tree.
const PART_LEVEL: u8 = 1;
/// A chapter is the top of the tree in a book without parts, and one below a part in a book with
/// them.
const CHAPTER_LEVEL: u8 = 1;
/// How far below a chapter a section and a subsection sit.
const SECTION_DEPTH: u8 = 1;
const SUBSECTION_DEPTH: u8 = 2;

/// The edit an admitted mapping makes, for the clusters that hold headings.
///
/// Levels: a part is level 1; a chapter is level 1, or 2 when the book has parts; a section one
/// below the chapter, a subsection two. `caption`, `other` and `running_head` change nothing —
/// `other` is the prompt's own "the evidence is weak" answer (Appendix A.1 rule 3), and abstaining
/// is not a verdict — and a running head is recorded as a proposal.
pub fn role_edit(answer: &HeadingRolesAnswer, heading_clusters: &BTreeSet<u32>) -> RoleEdit {
    let mut edit = RoleEdit::default();
    let has_parts = heading_clusters
        .iter()
        .any(|cluster| answer.clusters.get(cluster) == Some(&HeadingRole::PartHeading));
    let chapter = if has_parts {
        PART_LEVEL.saturating_add(CHAPTER_LEVEL)
    } else {
        CHAPTER_LEVEL
    };

    for cluster in heading_clusters {
        let role = answer.clusters.get(cluster);
        let level = match role {
            Some(HeadingRole::PartHeading) => Some(PART_LEVEL),
            Some(HeadingRole::ChapterHeading) => Some(chapter),
            Some(HeadingRole::SectionHeading) => Some(chapter.saturating_add(SECTION_DEPTH)),
            Some(HeadingRole::SubsectionHeading) => Some(chapter.saturating_add(SUBSECTION_DEPTH)),
            _ => None,
        };
        if let Some(level) = level {
            edit.levels.insert(*cluster, level.min(MAX_LEVEL));
        }
        if let Some(demoted) = demotion(role) {
            edit.demote.insert(*cluster, demoted);
        }
        if role == Some(&HeadingRole::RunningHead) {
            edit.running_head_proposals.insert(*cluster);
        }
    }
    edit
}

/// Ask the heading-roles question and judge the answer, up to the edit it would make.
///
/// The pre-gates run first and a refusal makes **no call**. The edit is returned for the caller
/// to apply through the structure stage and put to gates L and V.
#[allow(clippy::too_many_arguments)]
pub fn run(
    asker: &mut dyn crate::session::Asker,
    facts: &InventoryFacts,
    input: &HeadingRolesInput,
    probes: &[HoldoutProbe],
    heading_clusters: &BTreeSet<u32>,
    limits: &HeadingRolesLimits,
    max_tokens: u32,
) -> crate::session::TaskResult<RoleEdit> {
    use crate::session::TaskResult;

    if let Err(refusal) =
        inventory_pre_gate(facts, limits).and_then(|()| probe_pre_gate(probes, limits))
    {
        return TaskResult::Refused(refusal);
    }
    let request = match crate::prompt::v1::heading_roles::request(input, max_tokens) {
        Ok(request) => request,
        Err(error) => {
            return TaskResult::Unasked(crate::session::Unasked::Unavailable(
                crate::provider::LlmError::Protocol(error.to_string()),
            ))
        }
    };
    let asked = match asker.ask(&request) {
        Ok(asked) => asked,
        Err(unasked) => return TaskResult::Unasked(unasked),
    };
    let verdict = crate::gates::schema::gate_response::<HeadingRolesAnswer>(&asked.response, input)
        .and_then(|answer| {
            validate_roles(&answer, input, probes, heading_clusters, limits)?;
            Ok(answer)
        });
    match verdict {
        Ok(answer) => TaskResult::Admitted {
            trace: asked.trace,
            answer: role_edit(&answer, heading_clusters),
        },
        Err(failure) => TaskResult::Rejected {
            trace: asked.trace,
            failure,
        },
    }
}

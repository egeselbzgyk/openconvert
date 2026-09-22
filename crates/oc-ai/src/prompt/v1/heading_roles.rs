//! Task 2, `heading_roles`, version 1 (ARCHITECTURE §9.6, R10 §6.7).
//!
//! The input is the style inventory — one summary per cluster, with a handful of verbatim examples
//! — plus the held-out probe: 8–10 individual lines sampled from those clusters and **not** shown
//! as examples. The probe rides along in the same call (ARCHITECTURE §9.6, PIPELINE §8), and a
//! mapping whose per-line labels disagree with its per-cluster labels on more than
//! `inventory.holdout_disagree_max` of the probe is rejected whole: a free oracle for "the
//! clustering was wrong", with no gold set.
//!
//! IMPLEMENTATION_PLAN Appendix A.3 sends the probe as a second call. ARCHITECTURE and PIPELINE
//! both put it in the same one, they outrank the plan, and a second call would spend one of the
//! book's eight on a question the first could have carried (`docs/DECISIONS_LOG.md`, 2026-09-22).
//!
//! A probe line is identified by its index in this payload rather than by its run id. A run id is
//! page-local (`docs/DECISIONS_LOG.md`, 2026-09-14), so it would not name one line of a book, and
//! a payload index is exactly as small as the grammar's `index` rule.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::gates::schema::{bijection, closed, Answer};
use crate::gates::GateFailure;
use crate::prompt::render::{self, RenderError};
use crate::prompt::Artifacts;
use crate::provider::{LlmRequest, Purpose};

pub const ARTIFACTS: Artifacts = Artifacts {
    purpose: Purpose::HeadingRoles,
    system: include_str!("../../../prompts/heading_roles/v1/system.md"),
    user_template: include_str!("../../../prompts/heading_roles/v1/user.tmpl"),
    grammar: include_str!("../../../prompts/heading_roles/v1/grammar.gbnf"),
    schema: include_str!("../../../prompts/heading_roles/v1/schema.json"),
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Weight {
    Regular,
    Bold,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    Left,
    Right,
    Centered,
    Justified,
}

/// One style cluster, as the model sees it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ClusterSummary {
    /// The cluster's id: what the answer's `c` must name.
    pub c: u32,
    /// Size as a z-score against the body mode — a relative measure, not a point size.
    pub size_z: f32,
    pub weight: Weight,
    pub italic: bool,
    pub align: Align,
    /// How many runs the cluster holds.
    pub count: u32,
    /// The share of its runs that open a page.
    pub starts_page_ratio: f32,
    /// At most `inventory.max_examples` verbatim strings.
    pub examples: Vec<String>,
}

/// One held-out line.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Probe {
    /// Its index in this payload: what the answer's `i` must name.
    pub i: u32,
    pub text: String,
    pub size_z: f32,
    pub weight: Weight,
    pub italic: bool,
    pub align: Align,
}

/// Everything the heading-roles question is about.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HeadingRolesInput {
    /// The book's language, as a BCP-47 primary subtag.
    pub language: String,
    pub clusters: Vec<ClusterSummary>,
    pub holdout: Vec<Probe>,
}

/// The heading-roles request for this inventory.
pub fn request(input: &HeadingRolesInput, max_tokens: u32) -> Result<LlmRequest, RenderError> {
    for cluster in &input.clusters {
        render::finite("size_z", cluster.size_z)?;
        render::finite("starts_page_ratio", cluster.starts_page_ratio)?;
    }
    for probe in &input.holdout {
        render::finite("size_z", probe.size_z)?;
    }
    let payload = render::json(input)?;
    let user = render::fill(ARTIFACTS.user_template, &[("payload", &payload)])?;
    Ok(ARTIFACTS.request(user, max_tokens))
}

/// The roles a cluster or a line may be given: the taxonomy of the shared prefix (Appendix A.1).
///
/// `RunningHead` is a *proposal*. Label authority is not deletion authority (D13.5): only the
/// deterministic furniture remover deletes, and only when its own cross-page repetition evidence
/// agrees.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum HeadingRole {
    ChapterHeading,
    PartHeading,
    SectionHeading,
    SubsectionHeading,
    RunningHead,
    Epigraph,
    Body,
    Caption,
    Other,
}

impl HeadingRole {
    /// Every role, in the order the grammar lists them.
    pub const ALL: [HeadingRole; 9] = [
        HeadingRole::ChapterHeading,
        HeadingRole::PartHeading,
        HeadingRole::SectionHeading,
        HeadingRole::SubsectionHeading,
        HeadingRole::RunningHead,
        HeadingRole::Epigraph,
        HeadingRole::Body,
        HeadingRole::Caption,
        HeadingRole::Other,
    ];

    /// The role as the grammar spells it.
    pub fn as_str(self) -> &'static str {
        match self {
            HeadingRole::ChapterHeading => "chapter_heading",
            HeadingRole::PartHeading => "part_heading",
            HeadingRole::SectionHeading => "section_heading",
            HeadingRole::SubsectionHeading => "subsection_heading",
            HeadingRole::RunningHead => "running_head",
            HeadingRole::Epigraph => "epigraph",
            HeadingRole::Body => "body",
            HeadingRole::Caption => "caption",
            HeadingRole::Other => "other",
        }
    }

    /// The role a name spells, exactly — `chapter_headings` is not `chapter_heading`.
    pub fn from_name(name: &str) -> Option<HeadingRole> {
        HeadingRole::ALL
            .into_iter()
            .find(|role| role.as_str() == name)
    }
}

/// The answer, as gate S admits it: one role per cluster and one per held-out line, each keyed by
/// the identifier the payload gave it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeadingRolesAnswer {
    pub clusters: BTreeMap<u32, HeadingRole>,
    pub probes: BTreeMap<u32, HeadingRole>,
}

/// The answer as JSON spells it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeadingRolesWire {
    m: Vec<ClusterRole>,
    h: Vec<ProbeRole>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClusterRole {
    c: u32,
    r: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProbeRole {
    i: u32,
    r: String,
}

impl Answer for HeadingRolesAnswer {
    type Context = HeadingRolesInput;
    type Wire = HeadingRolesWire;

    fn check(wire: HeadingRolesWire, input: &HeadingRolesInput) -> Result<Self, GateFailure> {
        let mut clusters = BTreeMap::new();
        for entry in &wire.m {
            clusters.insert(entry.c, closed("r", &entry.r, HeadingRole::from_name)?);
        }
        let mut probes = BTreeMap::new();
        for entry in &wire.h {
            probes.insert(entry.i, closed("r", &entry.r, HeadingRole::from_name)?);
        }
        bijection(
            "cluster",
            input.clusters.iter().map(|cluster| cluster.c),
            wire.m.iter().map(|entry| entry.c),
        )?;
        bijection(
            "probe",
            input.holdout.iter().map(|probe| probe.i),
            wire.h.iter().map(|entry| entry.i),
        )?;
        Ok(HeadingRolesAnswer { clusters, probes })
    }
}

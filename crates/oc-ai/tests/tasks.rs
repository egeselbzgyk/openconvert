//! PHASE 10: the four tasks' own validations and edits, over answers replayed from cassettes.
//!
//! **Where the answers come from.** No model is reachable where Phase 10 was built, so every
//! cassette under `tests/cassettes/<task>/` that a test here names was recorded through the stub:
//! a scripted answer, filed by the production cache key exactly as a model's would be, with
//! `model_id = "stub"` and no llama.cpp build or thread count — honest about what it is. Each test
//! both checks that the committed cassette is what recording its scripted answer would write now,
//! and replays it through [`Replay`] — the provider seam every test tier uses, which holds no
//! transport. Regenerate after a prompt-version bump with
//! `OC_AI_RECORD_SEEDS=1 cargo nextest run -p oc-ai --test tasks` and review the diff; the nightly
//! live job re-records them from a model.

mod common;

use std::path::{Path, PathBuf};

use oc_ai::cassette::{record, Cassette, Params, Recording, Replay, STUB_MODEL};
use oc_ai::digest::hex;
use oc_ai::gates::schema::gate_response;
use oc_ai::gates::GateFailure;
use oc_ai::prompt::v1::heading_roles::{Align, ClusterSummary, HeadingRolesInput, Probe, Weight};
use oc_ai::prompt::v1::metadata::{self, MetadataAnswer, MetadataInput, Size, StyledLine};
use oc_ai::provider::{LlmProvider, LlmRequest, LlmResponse};
use oc_ai::session::{Asked, Asker, TaskResult, Unasked};
use oc_ai::task::heading_roles::{self as roles, Demotion, HeadingRolesLimits, HoldoutProbe};
use oc_ai::task::metadata::{apply_metadata, validate_metadata, MetadataLimits};
use oc_ai::task::{InventoryFacts, PreGateFailure};
use oc_core::thresholds::T;
use oc_model::doc::{MetaSource, Metadata};
use oc_model::lang::LangTag;

fn cassettes() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cassettes")
}

/// The answer `answer` to `request`, as the committed cassette `<task>__<fixture>__v1` replays it.
///
/// Fails when the committed cassette is not what recording the scripted answer would write now —
/// a stale or hand-edited cassette — and rewrites it instead under `OC_AI_RECORD_SEEDS=1`.
fn replayed(fixture: &str, request: &LlmRequest, answer: &str) -> LlmResponse {
    let fresh = record(
        request,
        &LlmResponse {
            text: answer.to_owned(),
            reasoning: None,
            tokens_in: 0,
            tokens_out: 0,
            cached: false,
            cached_tokens: None,
            finish_reason: Some("stop".to_owned()),
        },
        Recording {
            fixture,
            model_id: STUB_MODEL,
            params: Params {
                temperature: T.llm.temperature as f32,
                max_tokens: request.max_tokens,
                enable_thinking: false,
                top_p: None,
                seed: None,
                n_ctx: None,
                n_parallel: None,
            },
            recorded_at: "2026-09-23T00:00:00Z".to_owned(),
            llama_cpp_build: None,
            threads: None,
            machine: "stub".to_owned(),
        },
    );
    if std::env::var("OC_AI_RECORD_SEEDS").is_ok_and(|value| value == "1") {
        fresh.write(&cassettes()).expect("the cassette is written");
    }
    let task = request.purpose.as_str();
    let key = hex(&request.cache_key(STUB_MODEL));
    let committed: Cassette = serde_json::from_str(
        &std::fs::read_to_string(cassettes().join(task).join(format!("{key}.json")))
            .unwrap_or_else(|_| panic!("no cassette {}; set OC_AI_RECORD_SEEDS=1", fresh.name())),
    )
    .expect("a cassette");
    assert_eq!(
        Cassette {
            recorded_at: fresh.recorded_at.clone(),
            ..committed
        },
        fresh,
        "the cassette {} is stale; set OC_AI_RECORD_SEEDS=1 and review the diff",
        fresh.name()
    );
    Replay::new(cassettes(), STUB_MODEL)
        .complete(request)
        .unwrap_or_else(|error| panic!("{} did not replay: {error}", fresh.name()))
}

fn max_tokens() -> u32 {
    common::max_tokens()
}

/// An [`Asker`] that answers every question with one scripted answer, replayed from its committed
/// cassette, and counts the questions — so "no call was made" is a number a test reads.
struct Scripted {
    fixture: &'static str,
    answer: String,
    calls: usize,
}

impl Scripted {
    fn new(fixture: &'static str, answer: impl Into<String>) -> Self {
        Self {
            fixture,
            answer: answer.into(),
            calls: 0,
        }
    }
}

impl Asker for Scripted {
    fn ask(&mut self, request: &LlmRequest) -> Result<Asked, Unasked> {
        self.calls += 1;
        let response = replayed(self.fixture, request, &self.answer);
        let trace = oc_ai::provider::trace(STUB_MODEL, request, &response.text, true, 0);
        Ok(Asked { response, trace })
    }
}

/// An [`Asker`] that must never be asked.
struct Never;

impl Asker for Never {
    fn ask(&mut self, request: &LlmRequest) -> Result<Asked, Unasked> {
        panic!(
            "{:?} was asked, and no call should have been made",
            request.purpose
        )
    }
}

// ---------------------------------------------------------------------------
// Task 1 — metadata
// ---------------------------------------------------------------------------

fn metadata_limits() -> MetadataLimits {
    MetadataLimits {
        title_max_chars: usize::try_from(T.metadata.llm_title_max_chars).unwrap_or(usize::MAX),
    }
}

/// A title page whose Info dictionary said `Microsoft Word - Kapitel.docx`: the case the
/// metadata predicate escalates.
fn title_page() -> MetadataInput {
    let line = |size, centered, text: &str| StyledLine {
        size,
        centered,
        text: text.to_owned(),
    };
    MetadataInput {
        lines: vec![
            line(Some(Size::Large), true, "Der Prozess"),
            line(Some(Size::Medium), true, "Roman"),
            line(Some(Size::Small), true, "Franz  Kafka"),
            line(None, false, "Erstes Kapitel"),
            line(
                None,
                false,
                "Jemand mußte Josef K. verleumdet haben, denn ohne daß er",
            ),
        ],
    }
}

fn deterministic_metadata() -> Metadata {
    Metadata {
        title: Some("Kapitel".to_owned()),
        subtitle: None,
        authors: Vec::new(),
        translator: None,
        publisher: None,
        date: None,
        identifier: "urn:uuid:00000000-0000-5000-8000-000000000000".to_owned(),
        language: LangTag::DE,
        source: MetaSource::Heuristic,
    }
}

/// Row 10.3. The model answers with the right title, author and date, and a publisher that is
/// printed nowhere on pages 1–3. The verbatim check refuses the **whole** response — the true title with
/// it — and names the field that was invented.
#[test]
fn metadata_verbatim_substring_rejects_invention() {
    // The same title page with its imprint line: a place and a year, and no publisher.
    let mut input = title_page();
    input.lines.insert(
        3,
        StyledLine {
            size: Some(Size::Small),
            centered: true,
            text: "Berlin 1925".to_owned(),
        },
    );
    let request = metadata::request(&input, max_tokens()).expect("renders");
    let response = replayed(
        "fabricated_publisher",
        &request,
        concat!(
            r#"{"title":"Der Prozess","subtitle":"Roman","authors":["Franz Kafka"],"#,
            r#""translator":null,"publisher":"S. Fischer Verlag","date":"1925"}"#
        ),
    );
    let answer = gate_response::<MetadataAnswer>(&response, &input)
        .expect("the shape is right: gate S admits it");

    let verdict = validate_metadata(&answer, &input.verbatim_text(), &metadata_limits());
    assert_eq!(
        verdict,
        Err(GateFailure::NotVerbatim { field: "publisher" })
    );
    assert_eq!(
        verdict.map_err(|failure| failure.code()),
        Err("V.verbatim"),
        "the code the Decision records"
    );
}

/// Row 10.4. A verbatim title — and an author the page prints with two spaces, which the
/// whitespace normalisation reads as the same name — is accepted, and becomes the book's
/// `dc:title`, with `source = llm` so it is never mistaken for a title the file declared.
#[test]
fn metadata_accepts_exact_title() {
    let input = title_page();
    let request = metadata::request(&input, max_tokens()).expect("renders");
    let response = replayed(
        "exact_title",
        &request,
        concat!(
            r#"{"title":"Der Prozess","subtitle":"Roman","authors":["Franz Kafka"],"#,
            r#""translator":null,"publisher":null,"date":null}"#
        ),
    );
    let answer = gate_response::<MetadataAnswer>(&response, &input).expect("gate S admits it");
    validate_metadata(&answer, &input.verbatim_text(), &metadata_limits())
        .expect("every field is on the page");

    let applied = apply_metadata(&answer, &deterministic_metadata());
    assert_eq!(applied.title.as_deref(), Some("Der Prozess"));
    assert_eq!(applied.subtitle.as_deref(), Some("Roman"));
    assert_eq!(applied.authors, vec!["Franz Kafka".to_owned()]);
    assert_eq!(applied.source, MetaSource::Llm);
    // Never the model's: the identity and the language.
    assert_eq!(applied.identifier, deterministic_metadata().identifier);
    assert_eq!(applied.language, LangTag::DE);
}

/// The check compares the lines' text, not the tags the model was shown beside it, and holds
/// the title to its length bound.
#[test]
fn metadata_check_ignores_the_tags_and_bounds_the_title() {
    let input = title_page();
    let answer = |title: &str| MetadataAnswer {
        title: Some(title.to_owned()),
        subtitle: None,
        authors: Vec::new(),
        translator: None,
        publisher: None,
        date: None,
    };
    assert_eq!(
        validate_metadata(
            &answer("[LARGE][CENTERED] Der Prozess"),
            &input.verbatim_text(),
            &metadata_limits()
        ),
        Err(GateFailure::NotVerbatim { field: "title" })
    );
    // Case-insensitive, as the design says.
    assert!(validate_metadata(
        &answer("DER PROZESS"),
        &input.verbatim_text(),
        &metadata_limits()
    )
    .is_ok());

    let long = "Jemand mußte Josef K. verleumdet haben, denn ohne daß er";
    let tight = MetadataLimits {
        title_max_chars: 10,
    };
    assert!(matches!(
        validate_metadata(&answer(long), &input.verbatim_text(), &tight),
        Err(GateFailure::OutOfRange { field: "title", .. })
    ));
}

// ---------------------------------------------------------------------------
// Task 2 — heading_roles
// ---------------------------------------------------------------------------

fn roles_limits() -> HeadingRolesLimits {
    HeadingRolesLimits {
        max_clusters: usize::try_from(T.inventory.max_clusters).unwrap_or_default(),
        min_body_char_share: T.inventory.min_body_char_share as f32,
        min_probes: usize::try_from(T.inventory.holdout_min_probes).unwrap_or_default(),
        holdout_disagree_max: T.inventory.holdout_disagree_max as f32,
        chapter_min_count: u32::try_from(T.inventory.chapter_cluster_min_count).unwrap_or_default(),
        chapter_max_count: u32::try_from(T.inventory.chapter_cluster_max_count).unwrap_or_default(),
    }
}

fn summary(c: u32, size_z: f32, weight: Weight, count: u32, example: &str) -> ClusterSummary {
    ClusterSummary {
        c,
        size_z,
        weight,
        italic: false,
        align: Align::Left,
        count,
        starts_page_ratio: 0.0,
        examples: vec![example.to_owned()],
    }
}

fn probe(i: u32, text: &str, size_z: f32, weight: Weight) -> Probe {
    Probe {
        i,
        text: text.to_owned(),
        size_z,
        weight,
        italic: false,
        align: Align::Left,
    }
}

/// A three-style book — chapters, sections, body — with ten held-out lines: four chapter
/// headings, three section headings and three body lines, none of them an example.
fn three_styles() -> (HeadingRolesInput, Vec<HoldoutProbe>) {
    let input = HeadingRolesInput {
        language: "en".to_owned(),
        clusters: vec![
            summary(0, 2.4, Weight::Bold, 12, "Chapter One"),
            summary(1, 1.1, Weight::Bold, 30, "The Road North"),
            summary(
                2,
                0.0,
                Weight::Regular,
                2400,
                "It was late in the year when",
            ),
        ],
        holdout: vec![
            probe(0, "Chapter Two", 2.4, Weight::Bold),
            probe(1, "Chapter Three", 2.4, Weight::Bold),
            probe(2, "Chapter Four", 2.4, Weight::Bold),
            probe(3, "Chapter Five", 2.4, Weight::Bold),
            probe(4, "At the Crossing", 1.1, Weight::Bold),
            probe(5, "A Letter", 1.1, Weight::Bold),
            probe(6, "Winter", 1.1, Weight::Bold),
            probe(7, "and the snow had not yet come", 0.0, Weight::Regular),
            probe(8, "she said nothing for a while", 0.0, Weight::Regular),
            probe(9, "until the lamps were lit", 0.0, Weight::Regular),
        ],
    };
    let probes = [0, 0, 0, 0, 1, 1, 1, 2, 2, 2]
        .into_iter()
        .enumerate()
        .map(|(i, cluster)| HoldoutProbe {
            i: u32::try_from(i).unwrap_or_default(),
            cluster,
        })
        .collect();
    (input, probes)
}

fn heading_clusters() -> std::collections::BTreeSet<u32> {
    [0, 1].into_iter().collect()
}

/// Row 10.5. Forty style clusters is not a hierarchy a model can label: the pre-gate refuses,
/// **no call is made at all**, and the report hears `W_STYLE_INVENTORY_INVALID`.
#[test]
fn inventory_pre_gate_blocks_40_clusters() {
    let (input, probes) = three_styles();
    let facts = InventoryFacts {
        clusters: 40,
        body_char_share: 0.8,
    };
    let result = roles::run(
        &mut Never,
        &facts,
        &input,
        &probes,
        &heading_clusters(),
        &roles_limits(),
        max_tokens(),
    );
    let TaskResult::Refused(refusal) = result else {
        panic!("forty clusters were asked about: {result:?}");
    };
    assert_eq!(refusal, PreGateFailure::TooManyClusters(facts));
    assert_eq!(refusal.code(), "pregate.inventory");
    assert_eq!(
        refusal.warning().map(|warning| warning.code),
        Some("W_STYLE_INVENTORY_INVALID")
    );
}

/// Row 10.6. A body cluster holding 45 % of the characters is below
/// `inventory.min_body_char_share`: there is no body text to measure every other style against,
/// and no call is made.
#[test]
fn inventory_pre_gate_blocks_low_body_share() {
    let (input, probes) = three_styles();
    let facts = InventoryFacts {
        clusters: 3,
        body_char_share: 0.45,
    };
    let result = roles::run(
        &mut Never,
        &facts,
        &input,
        &probes,
        &heading_clusters(),
        &roles_limits(),
        max_tokens(),
    );
    assert_eq!(
        result,
        TaskResult::Refused(PreGateFailure::BodyTooThin(facts))
    );
    // And a book at the bound is asked about: the refusal is the threshold's, not a guess.
    let at_bound = InventoryFacts {
        clusters: 3,
        body_char_share: T.inventory.min_body_char_share as f32,
    };
    assert!(roles::inventory_pre_gate(&at_bound, &roles_limits()).is_ok());
}

/// Row 10.7. The model maps the clusters sensibly — and then labels three of its ten held-out
/// lines against their own clusters' labels. 30 % is over `inventory.holdout_disagree_max`, so
/// the clustering (or the model) is wrong and **the whole mapping is rejected**; the size-rank
/// levels stand, and the `Decision` records the fallback with its code.
#[test]
fn holdout_disagreement_rejects_mapping() {
    let (input, probes) = three_styles();
    let facts = InventoryFacts {
        clusters: 3,
        body_char_share: 0.9,
    };
    let mut asker = Scripted::new(
        "holdout_disagrees",
        concat!(
            r#"{"m":[{"c":0,"r":"chapter_heading"},{"c":1,"r":"section_heading"},"#,
            r#"{"c":2,"r":"body"}],"h":[{"i":0,"r":"chapter_heading"},"#,
            r#"{"i":1,"r":"chapter_heading"},{"i":2,"r":"section_heading"},"#,
            r#"{"i":3,"r":"chapter_heading"},{"i":4,"r":"section_heading"},"#,
            r#"{"i":5,"r":"body"},{"i":6,"r":"section_heading"},{"i":7,"r":"body"},"#,
            r#"{"i":8,"r":"other"},{"i":9,"r":"body"}]}"#
        ),
    );
    let result = roles::run(
        &mut asker,
        &facts,
        &input,
        &probes,
        &heading_clusters(),
        &roles_limits(),
        max_tokens(),
    );
    assert_eq!(
        asker.calls, 1,
        "one call: the probe rides along with the mapping"
    );
    let TaskResult::Rejected { trace, failure } = result else {
        panic!("a mapping its own probe contradicts was admitted: {result:?}");
    };
    assert_eq!(failure, GateFailure::HoldoutDisagrees { rate: 0.3 });
    assert_eq!(failure.code(), "S.holdout");

    // The size-rank answer stands, recorded as a fallback with the model's trace.
    let decision = oc_ai::gates::fallback::settle(
        oc_ai::gates::fallback::Choice {
            stage: "structure",
            kind: "heading_roles",
            subject: None,
            deterministic: "size_rank".to_owned(),
            alternatives: vec!["llm_mapping".to_owned()],
        },
        trace,
        Err(failure),
    );
    assert!(decision.fallback_used());
    assert_eq!(decision.chosen, "size_rank");
    assert_eq!(decision.fallback, Some("S.holdout"));
    assert!(
        decision.llm.is_some(),
        "the model was asked, and the report says so"
    );
}

/// The converse: two disagreements in ten is 20 %, at the bound and not over it, and the mapping
/// is admitted — as levels and a demotion, and nothing that could remove a block.
#[test]
fn a_mapping_within_the_disagreement_bound_is_admitted_as_levels() {
    // Another book set the same way — another question, so another cassette.
    let (mut input, probes) = three_styles();
    input.clusters[0].examples = vec!["Part One".to_owned()];
    input.clusters[1].examples = vec!["Chapter One".to_owned()];
    let facts = InventoryFacts {
        clusters: 3,
        body_char_share: 0.9,
    };
    let mut asker = Scripted::new(
        "holdout_agrees",
        concat!(
            r#"{"m":[{"c":0,"r":"part_heading"},{"c":1,"r":"chapter_heading"},"#,
            r#"{"c":2,"r":"body"}],"h":[{"i":0,"r":"part_heading"},"#,
            r#"{"i":1,"r":"part_heading"},{"i":2,"r":"part_heading"},"#,
            r#"{"i":3,"r":"part_heading"},{"i":4,"r":"chapter_heading"},"#,
            r#"{"i":5,"r":"chapter_heading"},{"i":6,"r":"other"},{"i":7,"r":"body"},"#,
            r#"{"i":8,"r":"other"},{"i":9,"r":"body"}]}"#
        ),
    );
    let result = roles::run(
        &mut asker,
        &facts,
        &input,
        &probes,
        &heading_clusters(),
        &roles_limits(),
        max_tokens(),
    );
    let TaskResult::Admitted { answer: edit, .. } = result else {
        panic!("a mapping at the bound was refused: {result:?}");
    };
    assert_eq!(edit.levels.get(&0), Some(&1), "a part is level 1");
    assert_eq!(
        edit.levels.get(&1),
        Some(&2),
        "a chapter under parts is level 2"
    );
    assert!(edit.demote.is_empty());
}

/// ARCHITECTURE §9.6's role rules, and the one this phase adds: a chapter style must hold 2–200
/// runs, `epigraph` is never the most frequent large-font style, and a mapping that demotes every
/// heading the book has is refused. `running_head` is a proposal and changes nothing.
#[test]
fn role_rules_refuse_what_the_design_forbids() {
    use oc_ai::prompt::v1::heading_roles::{HeadingRole, HeadingRolesAnswer};

    let (input, probes) = three_styles();
    let limits = roles_limits();
    let answer = |roles: [HeadingRole; 3]| HeadingRolesAnswer {
        clusters: roles
            .iter()
            .enumerate()
            .map(|(c, role)| (u32::try_from(c).unwrap_or_default(), *role))
            .collect(),
        // Every probe agrees with its cluster, so only the rule under test can refuse.
        probes: probes
            .iter()
            .map(|probe| {
                (
                    probe.i,
                    roles[usize::try_from(probe.cluster).unwrap_or_default()],
                )
            })
            .collect(),
    };
    let check = |roles| {
        roles::validate_roles(
            &answer(roles),
            &input,
            &probes,
            &heading_clusters(),
            &limits,
        )
    };
    use HeadingRole::*;

    assert!(check([ChapterHeading, SectionHeading, Body]).is_ok());
    // Cluster 2 holds 2 400 runs: not chapters.
    assert!(matches!(
        check([Other, SectionHeading, ChapterHeading]),
        Err(GateFailure::RoleRule(_))
    ));
    // Cluster 1 is the most frequent large-font style.
    assert!(matches!(
        check([ChapterHeading, Epigraph, Body]),
        Err(GateFailure::RoleRule(_))
    ));
    // Both heading styles demoted.
    assert!(matches!(
        check([Body, Body, Body]),
        Err(GateFailure::RoleRule(_))
    ));

    let edit = roles::role_edit(&answer([RunningHead, Epigraph, Body]), &heading_clusters());
    assert!(edit.levels.is_empty());
    assert_eq!(edit.demote.get(&1), Some(&Demotion::Epigraph));
    assert!(edit.running_head_proposals.contains(&0));
    assert!(
        !edit.demote.contains_key(&0),
        "a running head is proposed, never demoted"
    );
}

/// Too few held-out lines to check the mapping against itself: no call.
#[test]
fn too_few_probes_is_no_call() {
    let (input, probes) = three_styles();
    let facts = InventoryFacts {
        clusters: 3,
        body_char_share: 0.9,
    };
    let result = roles::run(
        &mut Never,
        &facts,
        &input,
        &probes[..3],
        &heading_clusters(),
        &roles_limits(),
        max_tokens(),
    );
    assert!(matches!(
        result,
        TaskResult::Refused(PreGateFailure::TooFewProbes { probes: 3, .. })
    ));
}

/// `inventory.holdout_max_probes` is the grammar's own bound on the `"h"` array: a payload with
/// more probes than the grammar can answer would fail gate S on every call.
#[test]
fn the_probe_bound_is_the_grammars() {
    let grammar = oc_ai::prompt::v1::heading_roles::ARTIFACTS.grammar;
    let max = T.inventory.holdout_max_probes;
    assert!(
        grammar.contains(&format!("( \",\" probe ){{0,{}}}", max - 1)),
        "the grammar admits {max} probes"
    );
    assert!(T.inventory.holdout_min_probes <= max);
}

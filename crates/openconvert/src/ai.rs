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

// ---------------------------------------------------------------------------
// The step
// ---------------------------------------------------------------------------

use oc_ai::cache::FileCache;
use oc_ai::gates::fallback::{settle, unasked, Choice};
use oc_ai::gates::validity::{Region, Tolerance};
use oc_ai::gates::GateFailure;
use oc_ai::plan::{plan, Demand, LanguageGates, LANGUAGE_GATE};
use oc_ai::prompt::v1::{book_structure, heading_roles, metadata, verse_quote};
use oc_ai::provider::{LlmProvider, Purpose};
use oc_ai::session::{Asker, CallRecord, Clock, Session, TaskResult, Unasked};
use oc_core::thresholds::Thresholds;
use oc_model::decision::{Decision, LlmTrace};
use oc_model::doc::{Severity, Warning};
use oc_model::document::Document;
use oc_structure::escalate::{Escalation, Escalations};
use oc_structure::stage::{StructureEdits, StructureInput, StructureOutput};

/// What the AI step is given: a provider, where answers are cached, a clock, and the switches.
pub struct AiContext<'a> {
    pub provider: &'a dyn LlmProvider,
    pub cache: Option<&'a FileCache>,
    pub clock: &'a dyn Clock,
    /// `--ai-all-tasks`: the language gate is set aside.
    pub all_tasks: bool,
    /// `--ai-mode`: how the model is asked and how long a book may take.
    pub mode: oc_core::jobspec::AiMode,
}

/// What the AI step did, for the document and the report.
#[derive(Clone, Debug, Default)]
pub struct AiOutcome {
    pub model_id: String,
    pub all_tasks: bool,
    /// One per escalated choice: settled by the model, refused by a gate, or never asked.
    pub decisions: Vec<Decision>,
    pub warnings: Vec<Warning>,
    /// Every call, cached ones included, in order: the NDJSON `llm` events.
    pub calls: Vec<CallRecord>,
    pub llm_ms: u64,
    /// The edits every gate admitted, as the structure stage applies them.
    pub edits: StructureEdits,
}

/// The model time a book of `pages` pages may take in `mode`.
pub fn time_budget_ms(mode: oc_core::jobspec::AiMode, pages: u32, t: &Thresholds) -> u64 {
    use oc_core::jobspec::AiMode;
    let secs = |value: i64| u64::try_from(value).unwrap_or_default();
    let (per_page, min, max) = match mode {
        AiMode::Fast => (
            t.llm.fast_seconds_per_page,
            t.llm.fast_min_budget_secs,
            t.llm.fast_max_budget_secs,
        ),
        AiMode::Quality => (
            t.llm.quality_seconds_per_page,
            t.llm.quality_min_budget_secs,
            t.llm.quality_max_budget_secs,
        ),
    };
    oc_ai::session::time_budget_ms(pages, per_page, secs(min), secs(max))
}

/// The pages the metadata task reads: 1–3 (PIPELINE §8.8), a definition and not a threshold.
const METADATA_PAGES: u32 = 3;

/// Run the four tasks over one book's `structure` result, and return what every gate admitted.
///
/// `det` is the deterministic run. Each admitted edit is applied by re-running the stage with
/// every edit admitted so far plus this one, and gates L and V compare that run with the one
/// before it; a refusal leaves the book as it was. The order is metadata, heading roles, verse
/// or quote, and book structure last — hosted by `document` (PIPELINE §0.4) and read off the
/// heading list the earlier edits left.
pub fn run(
    ctx: &AiContext<'_>,
    input: &StructureInput,
    det: StructureOutput,
    escalations: &Escalations,
    t: &Thresholds,
) -> (StructureOutput, AiOutcome) {
    let mut session = Session::new(
        ctx.provider,
        ctx.cache,
        ctx.clock,
        u32::try_from(t.llm.max_calls_per_book).unwrap_or_default(),
        time_budget_ms(ctx.mode, input.page_count, t),
    );
    let mut step = Step {
        input,
        t,
        current: det,
        accepted: StructureEdits::default(),
        outcome: AiOutcome {
            model_id: ctx.provider.id().to_owned(),
            all_tasks: ctx.all_tasks,
            ..AiOutcome::default()
        },
        tolerance: Tolerance {
            ratio: t.llm.gate_v_ratio_eps as f32,
            violations: u32::try_from(t.llm.gate_v_violations_eps).unwrap_or_default(),
        },
        max_tokens: u32::try_from(t.llm.max_output_tokens_per_call).unwrap_or_default(),
    };
    let language = input.lang.as_str().to_owned();
    let gates = LanguageGates {
        metadata: t.ai.task.metadata.languages,
        heading_roles: t.ai.task.heading_roles.languages,
        book_structure: t.ai.task.book_structure.languages,
        verse_quote: t.ai.task.verse_quote.languages,
        front_page: t.ai.task.front_page.languages,
    };
    let allowed = |purpose| gates.allows(purpose, &language, ctx.all_tasks);

    // --- What the escalations want, after the language gate and the pre-gates.
    let mut demand = Demand::default();
    if escalations.metadata.is_escalated() {
        if allowed(Purpose::Metadata) {
            demand.metadata = true;
        } else {
            step.unasked(metadata_choice(&step.current), LANGUAGE_GATE);
        }
    }

    let roles = RolesQuestion::build(input, &step.current, t);
    if escalations.heading_roles.is_escalated() {
        if !allowed(Purpose::HeadingRoles) {
            step.unasked(roles_choice(), LANGUAGE_GATE);
        } else if let Err(refusal) =
            oc_ai::task::heading_roles::inventory_pre_gate(&roles.facts, &roles.limits)
                .and_then(|()| {
                    oc_ai::task::heading_roles::headings_pre_gate(&roles.heading_clusters)
                })
                .and_then(|()| {
                    oc_ai::task::heading_roles::probe_pre_gate(&roles.probes, &roles.limits)
                })
        {
            step.outcome.warnings.extend(refusal.warning());
            step.unasked(roles_choice(), refusal.code());
        } else {
            demand.heading_roles = true;
        }
    }

    let verse = VerseQuestion::build(input, &step.current, escalations, t);
    if !verse.blocks.is_empty() {
        if allowed(Purpose::VerseQuote) {
            demand.verse_quote_batches =
                oc_ai::task::verse_quote::batches(&verse.blocks, &verse.limits).len();
        } else {
            for block in &verse.blocks {
                step.unasked(verse_choice(block), LANGUAGE_GATE);
            }
        }
    }

    let structure_limits = oc_ai::task::book_structure::BookStructureLimits {
        chunk_headings: usize::try_from(t.llm.book_structure_chunk_headings).unwrap_or(1),
        chunk_overlap: usize::try_from(t.llm.book_structure_chunk_overlap).unwrap_or_default(),
    };
    if escalations.book_structure.is_escalated() {
        let headings = heading_list(&step.current);
        if headings.is_empty() {
            // Nothing to place: the predicate fired on a book with no headings at all.
        } else if allowed(Purpose::BookStructure) {
            demand.book_structure_chunks =
                oc_ai::task::book_structure::chunks(headings.len(), &structure_limits).len();
        } else {
            step.unasked(structure_choice(), LANGUAGE_GATE);
        }
    }

    // --- The grant: cut to the calls the book has, in the degradation order (N-4).
    let grant = plan(
        &demand,
        usize::try_from(session.remaining()).unwrap_or_default(),
    );
    if demand.calls() > grant.calls() {
        step.outcome.warnings.push(
            Warning::new(oc_ai::budget::W_LLM_BUDGET_EXHAUSTED, Severity::Warn)
                .with_arg("task", dropped_first(&demand, &grant).as_str())
                .with_arg("calls", t.llm.max_calls_per_book.to_string()),
        );
    }
    if demand.metadata && !grant.metadata {
        step.unasked(metadata_choice(&step.current), oc_ai::budget::BUDGET_CALLS);
    }
    if demand.heading_roles && !grant.heading_roles {
        step.unasked(roles_choice(), oc_ai::budget::BUDGET_CALLS);
    }
    if demand.book_structure_chunks > 0 && grant.book_structure_chunks == 0 {
        step.unasked(structure_choice(), oc_ai::budget::BUDGET_CALLS);
    }

    // --- Task 1: metadata.
    if grant.metadata {
        step.metadata(&mut session);
    }
    // --- Task 2: heading roles.
    if grant.heading_roles {
        step.heading_roles(&mut session, &roles);
    }
    // --- Task 4: verse or quote.
    if demand.verse_quote_batches > 0 {
        step.verse_quote(&mut session, &verse, grant.verse_quote_batches);
    }
    // --- Task 3: book structure, over the headings as the edits so far left them.
    if grant.book_structure_chunks > 0 {
        step.book_structure(
            &mut session,
            &language,
            &structure_limits,
            grant.book_structure_chunks,
        );
    }
    // --- The pages before the first chapter the rules could not type: a decision per page, and
    // in quality mode a second one with the kinds in the opposite order that has to agree.
    if allowed(Purpose::FrontPage) {
        step.front_pages(&mut session, ctx.mode == oc_core::jobspec::AiMode::Quality);
    }

    step.outcome
        .warnings
        .extend(session.warnings().iter().cloned());
    if let Some(Unasked::Unavailable(error)) = session.stopped() {
        step.outcome
            .warnings
            .push(unavailable(unavailable_reason(error)));
    }
    step.outcome.calls = session.calls().to_vec();
    step.outcome.llm_ms = session.llm_ms();
    step.outcome.edits = step.accepted.clone();
    (step.current, step.outcome)
}

/// The banner: AI was asked for and no model answered (RT D20).
pub fn unavailable(reason: &str) -> Warning {
    Warning::new(oc_ai::session::W_LLM_UNAVAILABLE, Severity::Warn).with_arg("reason", reason)
}

/// A reason a user can read, and that carries nothing of the endpoint's reply.
fn unavailable_reason(error: &oc_ai::provider::LlmError) -> &'static str {
    use oc_ai::provider::LlmError;
    use oc_ai::transport::TransportError;
    match error {
        LlmError::Transport(TransportError::Timeout(_)) => "the endpoint did not reply in time",
        LlmError::Transport(TransportError::Unreachable(_)) => "the endpoint could not be reached",
        LlmError::Transport(TransportError::Status { .. }) => "the endpoint refused the request",
        LlmError::Protocol(_) => "the endpoint's reply was not a chat completion",
        LlmError::CassetteMiss { .. } | LlmError::StaleCassette { .. } | LlmError::Cassette(_) => {
            "no recorded answer"
        }
    }
}

/// The first task the degradation order dropped, for the warning.
fn dropped_first(demand: &Demand, grant: &oc_ai::plan::Grant) -> Purpose {
    if grant.verse_quote_batches < demand.verse_quote_batches {
        Purpose::VerseQuote
    } else if grant.book_structure_chunks < demand.book_structure_chunks {
        Purpose::BookStructure
    } else if grant.heading_roles != demand.heading_roles {
        Purpose::HeadingRoles
    } else {
        Purpose::Metadata
    }
}

/// The step's state: the book as the admitted edits have it, and what has been recorded.
struct Step<'a> {
    input: &'a StructureInput,
    t: &'a Thresholds,
    current: StructureOutput,
    accepted: StructureEdits,
    outcome: AiOutcome,
    tolerance: Tolerance,
    max_tokens: u32,
}

impl Step<'_> {
    fn unasked(&mut self, choice: Choice, why: &'static str) {
        self.outcome.decisions.push(unasked(choice, why));
    }

    fn settled(&mut self, choice: Choice, trace: LlmTrace, verdict: Result<String, GateFailure>) {
        self.outcome.decisions.push(settle(choice, trace, verdict));
    }

    /// Apply `candidate` by re-running the stage, and put the result to gates L and V against
    /// the book as it stands. On success the candidate is the book.
    fn try_edit(&mut self, candidate: StructureEdits, region: &Region) -> Result<(), GateFailure> {
        let after = oc_structure::stage::structure_with(self.input, self.t, &candidate);
        oc_ai::gates::gate_edit(
            &gate_view(&self.current),
            &gate_view(&after),
            region,
            &self.tolerance,
        )?;
        self.current = after;
        self.accepted = candidate;
        Ok(())
    }

    fn metadata(&mut self, session: &mut Session<'_>) {
        let question = metadata_input(self.input, &self.current, self.t);
        let choice = metadata_choice(&self.current);
        let request = match metadata::request(&question, self.max_tokens) {
            Ok(request) => request,
            Err(_) => return self.unasked(choice, oc_ai::session::LLM_UNAVAILABLE),
        };
        let asked = match session.ask(&request) {
            Ok(asked) => asked,
            Err(why) => return self.unasked(choice, why.code()),
        };
        let limits = oc_ai::task::metadata::MetadataLimits {
            title_max_chars: usize::try_from(self.t.metadata.llm_title_max_chars)
                .unwrap_or(usize::MAX),
            author_min_words: usize::try_from(self.t.metadata.llm_author_min_words)
                .unwrap_or(usize::MAX),
        };
        let verdict = oc_ai::gates::schema::gate_response::<metadata::MetadataAnswer>(
            &asked.response,
            &question,
        )
        .and_then(|answer| {
            oc_ai::task::metadata::validate_metadata(&answer, &question.verbatim_text(), &limits)?;
            Ok(oc_ai::task::metadata::plausible(answer, &question, &limits))
        })
        .and_then(|answer| {
            let mut candidate = self.accepted.clone();
            let applied = oc_ai::task::metadata::apply_metadata(&answer, &self.current.metadata);
            let title = applied.title.clone().unwrap_or_default();
            candidate.metadata = Some(applied);
            // Metadata is outside `C`, so gates L and V have nothing to measure; they run anyway,
            // because an edit that is not applied through them is not applied at all.
            self.try_edit(candidate, &Region::Whole)?;
            Ok(title)
        });
        self.settled(choice, asked.trace, verdict);
    }

    /// Ask about each page before the first chapter that the rules left untyped or called a
    /// dedication, and apply the kinds both answers agreed on as one edit.
    fn front_pages(&mut self, session: &mut Session<'_>, twice: bool) {
        use oc_ai::task::front_page::{ask, PageKind};
        use oc_model::doc::{FrontMatterKind, SectionRole};

        let max_pages = usize::try_from(self.t.llm.front_page_max_pages).unwrap_or_default();
        let max_chars = usize::try_from(self.t.llm.front_page_max_chars).unwrap_or(usize::MAX);
        let max_tokens = u32::try_from(self.t.llm.decision_max_tokens).unwrap_or(self.max_tokens);
        let pages: Vec<(u32, FrontMatterKind)> = self
            .current
            .sections
            .iter()
            .filter(|section| section.heading.is_none())
            .filter_map(|section| match section.role {
                SectionRole::FrontMatter(
                    kind @ (FrontMatterKind::Other | FrontMatterKind::Dedication),
                ) => Some((section.source_pages, kind)),
                _ => None,
            })
            .flat_map(|((first, last), kind)| (first..=last).map(move |page| (page, kind)))
            .take(max_pages)
            .collect();

        let mut agreed: Vec<(Choice, LlmTrace, FrontMatterKind)> = Vec::new();
        let mut told: std::collections::BTreeMap<u32, FrontMatterKind> =
            std::collections::BTreeMap::new();
        for (page, deterministic) in pages {
            let text = page_text(self.input, page, max_chars);
            if text.trim().is_empty() {
                continue;
            }
            let choice = Choice {
                stage: oc_core::stages::STRUCTURE.name,
                kind: Purpose::FrontPage.as_str(),
                subject: None,
                deterministic: front_word(deterministic).to_owned(),
                alternatives: Vec::new(),
            };
            match ask(session, &text, twice, max_tokens) {
                Err(why) => self.unasked(choice, why.code()),
                Ok(answer) => {
                    let Some(trace) = answer.traces.first().cloned() else {
                        continue;
                    };
                    let kind = answer.kind.map(|kind| match kind {
                        PageKind::Title => FrontMatterKind::TitlePage,
                        PageKind::HalfTitle => FrontMatterKind::HalfTitle,
                        PageKind::Copyright => FrontMatterKind::Copyright,
                        PageKind::Dedication => FrontMatterKind::Dedication,
                        PageKind::Epigraph => FrontMatterKind::Epigraph,
                        PageKind::Contents => FrontMatterKind::TableOfContents,
                        PageKind::Foreword => FrontMatterKind::Foreword,
                        PageKind::Preface => FrontMatterKind::Preface,
                        PageKind::Introduction => FrontMatterKind::Introduction,
                        // Nothing to say beyond what the rules said.
                        PageKind::Other | PageKind::Body => deterministic,
                    });
                    match kind {
                        Some(kind) => {
                            if kind != deterministic {
                                told.insert(page, kind);
                            }
                            agreed.push((choice, trace, kind));
                        }
                        None => self.settled(choice, trace, Err(GateFailure::Disagreed)),
                    }
                }
            }
        }
        // One edit for every page the answers agreed on, through gates L and V like any other.
        let verdict = if told.is_empty() {
            Ok(())
        } else {
            let mut candidate = self.accepted.clone();
            candidate.front_pages.extend(told);
            self.try_edit(candidate, &Region::Whole)
        };
        for (choice, trace, kind) in agreed {
            let verdict = verdict.clone().map(|()| front_word(kind).to_owned());
            self.settled(choice, trace, verdict);
        }
    }

    fn heading_roles(&mut self, session: &mut Session<'_>, roles: &RolesQuestion) {
        let result = oc_ai::task::heading_roles::run(
            session,
            &roles.facts,
            &roles.input,
            &roles.probes,
            &roles.heading_clusters,
            &roles.limits,
            self.max_tokens,
        );
        let choice = roles_choice();
        match result {
            TaskResult::Refused(refusal) => {
                self.outcome.warnings.extend(refusal.warning());
                self.unasked(choice, refusal.code());
            }
            TaskResult::Unasked(why) => self.unasked(choice, why.code()),
            TaskResult::Rejected { trace, failure } => self.settled(choice, trace, Err(failure)),
            TaskResult::Admitted { trace, answer } => {
                let mut candidate = self.accepted.clone();
                candidate.headings = heading_edits(&answer);
                let verdict = self
                    .try_edit(candidate, &Region::Whole)
                    .map(|()| describe_roles(&answer));
                self.settled(choice, trace, verdict);
            }
        }
    }

    fn verse_quote(&mut self, session: &mut Session<'_>, verse: &VerseQuestion, granted: usize) {
        let batches = oc_ai::task::verse_quote::run(
            session,
            &verse.blocks,
            granted,
            &verse.limits,
            self.max_tokens,
        );
        for batch in batches {
            let blocks: Vec<&oc_ai::task::verse_quote::AmbiguousBlock> = batch
                .blocks
                .iter()
                .filter_map(|id| verse.blocks.iter().find(|block| block.summary.id == *id))
                .collect();
            match batch.result {
                TaskResult::Refused(refusal) => {
                    for block in blocks {
                        self.unasked(verse_choice(block), refusal.code());
                    }
                }
                TaskResult::Unasked(why) => {
                    for block in blocks {
                        self.unasked(verse_choice(block), why.code());
                    }
                }
                TaskResult::Rejected { trace, failure } => {
                    for block in blocks {
                        self.settled(verse_choice(block), trace.clone(), Err(failure.clone()));
                    }
                }
                TaskResult::Admitted { trace, answer } => {
                    let mut candidate = self.accepted.clone();
                    candidate.indented.extend(indented_kinds(&answer));
                    let region = Region::Blocks(batch.blocks.iter().copied().collect());
                    let verdict = self.try_edit(candidate, &region);
                    for block in blocks {
                        let id = block.summary.id;
                        let chosen = answer
                            .kinds
                            .get(&id)
                            .map(|kind| kind.as_str().to_owned())
                            .unwrap_or_default();
                        let decision = match &verdict {
                            Err(failure) => {
                                settle(verse_choice(block), trace.clone(), Err(failure.clone()))
                            }
                            Ok(()) if answer.downgraded.contains(&id) => Decision {
                                fallback: Some(oc_ai::task::verse_quote::COUNTER_EVIDENCE),
                                ..settle(
                                    verse_choice(block),
                                    trace.clone(),
                                    Err(GateFailure::RoleRule("counter-evidence")),
                                )
                            },
                            Ok(()) => settle(verse_choice(block), trace.clone(), Ok(chosen)),
                        };
                        self.outcome.decisions.push(decision);
                    }
                }
            }
        }
    }

    fn book_structure(
        &mut self,
        session: &mut Session<'_>,
        language: &str,
        limits: &oc_ai::task::book_structure::BookStructureLimits,
        granted: usize,
    ) {
        let headings = heading_list(&self.current);
        let result = oc_ai::task::book_structure::run(
            session,
            primary_subtag(language),
            &headings,
            granted,
            limits,
            self.max_tokens,
        );
        let choice = structure_choice();
        match result {
            TaskResult::Refused(refusal) => self.unasked(choice, refusal.code()),
            TaskResult::Unasked(why) => self.unasked(choice, why.code()),
            TaskResult::Rejected { trace, failure } => self.settled(choice, trace, Err(failure)),
            TaskResult::Admitted { trace, answer } => {
                let mut candidate = self.accepted.clone();
                candidate.zones = zone_edits(&answer);
                let zones_before = zone_order_warnings(&self.current);
                let verdict = self
                    .try_edit(candidate.clone(), &Region::Whole)
                    .and_then(|()| {
                        // The stitched zones were in order; applied to the tree with the
                        // headings the answer did not cover, they must still be.
                        if zone_order_warnings(&self.current) > zones_before {
                            Err(GateFailure::NotOrdered(
                                "the zones do not run front, body, back once applied".to_owned(),
                            ))
                        } else {
                            Ok(())
                        }
                    });
                if verdict.is_err() && self.accepted == candidate {
                    // Revert: the zone check failed after the gates had admitted the edit.
                    let mut reverted = candidate;
                    reverted.zones = oc_structure::book::ZoneEdits::default();
                    self.current =
                        oc_structure::stage::structure_with(self.input, self.t, &reverted);
                    self.accepted = reverted;
                }
                self.settled(choice, trace, verdict.map(|()| describe_zones(&answer)));
            }
        }
    }
}

fn zone_order_warnings(output: &StructureOutput) -> usize {
    output
        .warnings
        .iter()
        .filter(|warning| warning.code == oc_structure::book::W_ZONES_OUT_OF_ORDER)
        .count()
}

/// The primary subtag of a BCP-47 tag, as the prompts' `language` field carries it.
fn primary_subtag(language: &str) -> &str {
    language.split(['-', '_']).next().unwrap_or(language)
}

/// A `Document` holding what gates L and V measure: the flow, the notes, the tables, the figures.
fn gate_view(output: &StructureOutput) -> Document {
    Document {
        ir_version: oc_model::IR_VERSION,
        source_sha256: String::new(),
        meta: output.metadata.clone(),
        language: output.metadata.language.clone(),
        sections: output.sections.clone(),
        notes: output.notes.clone(),
        figures: output.figures.clone(),
        tables: output.tables.clone(),
        page_breaks: Vec::new(),
        ledger: oc_model::ledger::Ledger::default(),
        decisions: Vec::new(),
        warnings: Vec::new(),
        classification: oc_model::document::DocClass::default(),
        presets: oc_model::document::PresetName::default(),
        cover: None,
    }
}

// --- Choices: what the deterministic path chose, as each Decision records it.

fn metadata_choice(current: &StructureOutput) -> Choice {
    Choice {
        stage: oc_core::stages::STRUCTURE.name,
        kind: Purpose::Metadata.as_str(),
        subject: None,
        deterministic: current.metadata.title.clone().unwrap_or_default(),
        alternatives: Vec::new(),
    }
}

fn roles_choice() -> Choice {
    Choice {
        stage: oc_core::stages::STRUCTURE.name,
        kind: Purpose::HeadingRoles.as_str(),
        subject: None,
        deterministic: "size_rank".to_owned(),
        alternatives: vec!["llm_mapping".to_owned()],
    }
}

fn structure_choice() -> Choice {
    Choice {
        stage: oc_core::stages::DOCUMENT.name,
        kind: Purpose::BookStructure.as_str(),
        subject: None,
        deterministic: "rules".to_owned(),
        alternatives: vec!["llm_boundaries".to_owned()],
    }
}

fn verse_choice(block: &oc_ai::task::verse_quote::AmbiguousBlock) -> Choice {
    use verse_quote::BlockKind;
    Choice {
        stage: oc_core::stages::STRUCTURE.name,
        kind: Purpose::VerseQuote.as_str(),
        subject: Some(block.summary.id),
        deterministic: block.evidence.default.as_str().to_owned(),
        alternatives: BlockKind::ALL
            .iter()
            .filter(|kind| **kind != block.evidence.default)
            .map(|kind| kind.as_str().to_owned())
            .collect(),
    }
}

fn describe_roles(edit: &oc_ai::task::heading_roles::RoleEdit) -> String {
    let mut parts: Vec<String> = edit
        .levels
        .iter()
        .map(|(cluster, level)| format!("c{cluster}=h{level}"))
        .collect();
    parts.extend(
        edit.demote
            .iter()
            .map(|(cluster, demotion)| format!("c{cluster}={demotion:?}").to_lowercase()),
    );
    parts.join(",")
}

fn describe_zones(edit: &oc_ai::task::book_structure::ZoneEdit) -> String {
    use oc_ai::task::book_structure::Zone;
    let first = |zone: Zone| {
        edit.labels
            .iter()
            .find(|(_, label)| label.zone == zone)
            .map_or_else(|| "-".to_owned(), |(index, _)| index.to_string())
    };
    let parts: Vec<String> = edit
        .labels
        .iter()
        .filter(|(_, label)| label.part)
        .map(|(index, _)| index.to_string())
        .collect();
    format!(
        "body_from={},parts=[{}],back_from={}",
        first(Zone::Body),
        parts.join(","),
        first(Zone::Back)
    )
}

// --- Payloads: what each task is shown, built from what `structure` measured.

/// A page's text as printed, block by block, cut to `max_chars` characters.
fn page_text(input: &StructureInput, page: u32, max_chars: usize) -> String {
    let text = input
        .blocks
        .iter()
        .filter(|block| block.page == page)
        .map(|block| block.text.trim())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    text.chars().take(max_chars).collect()
}

/// A front-matter kind as a decision names it.
fn front_word(kind: oc_model::doc::FrontMatterKind) -> &'static str {
    use oc_model::doc::FrontMatterKind;
    match kind {
        FrontMatterKind::HalfTitle => "halftitle",
        FrontMatterKind::TitlePage => "title",
        FrontMatterKind::Copyright => "copyright",
        FrontMatterKind::Dedication => "dedication",
        FrontMatterKind::Epigraph => "epigraph",
        FrontMatterKind::TableOfContents => "contents",
        FrontMatterKind::Foreword => "foreword",
        FrontMatterKind::Preface => "preface",
        FrontMatterKind::Introduction => "introduction",
        FrontMatterKind::Other => "other",
    }
}

/// The flat heading list, in reading order: what book structure's indices count.
fn heading_list(current: &StructureOutput) -> Vec<book_structure::HeadingEntry> {
    let mut out = Vec::new();
    for root in &current.sections {
        for section in root.walk() {
            if let Some(heading) = &section.heading {
                out.push(book_structure::HeadingEntry {
                    idx: u32::try_from(out.len()).unwrap_or(u32::MAX),
                    text: heading.text(),
                    page: section.source_pages.0.saturating_add(1),
                    c: heading.style_cluster.0,
                });
            }
        }
    }
    out
}

/// The modal left edge of the blocks: the body margin centring is measured against.
fn body_margin(blocks: &[oc_structure::view::BlockView]) -> f32 {
    let mut counts: std::collections::BTreeMap<i64, usize> = std::collections::BTreeMap::new();
    for block in blocks {
        *counts.entry(block.bbox.x0.round() as i64).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by_key(|(edge, count)| (*count, -*edge))
        .map(|(edge, _)| edge as f32)
        .unwrap_or_default()
}

/// Pages 1–3 as the metadata task reads them: one line per printed line, with its size as a
/// category against the body size and a centring flag — never a number (D13.6) — cut at
/// `metadata.llm_max_input_chars` on a line boundary.
fn metadata_input(
    input: &StructureInput,
    current: &StructureOutput,
    t: &Thresholds,
) -> metadata::MetadataInput {
    use metadata::{Size, StyledLine};
    let body = current.inventory.body_size_pt();
    let margin = body_margin(&input.blocks);
    let budget = usize::try_from(t.metadata.llm_max_input_chars).unwrap_or(usize::MAX);
    let mut used = 0usize;
    let mut lines = Vec::new();
    'blocks: for block in input
        .blocks
        .iter()
        .filter(|block| block.page < METADATA_PAGES)
    {
        let centered = block.is_centered(margin, t);
        for line in &block.lines {
            let text = line.text.trim();
            if text.is_empty() {
                continue;
            }
            let chars = text.chars().count();
            if used.saturating_add(chars) > budget {
                break 'blocks;
            }
            used = used.saturating_add(chars);
            let ratio = if body > 0.0 {
                f64::from(line.size_pt() / body)
            } else {
                1.0
            };
            let size = if ratio >= t.metadata.llm_size_large_ratio {
                Some(Size::Large)
            } else if ratio >= t.metadata.llm_size_medium_ratio {
                Some(Size::Medium)
            } else if ratio < t.metadata.llm_size_small_ratio {
                Some(Size::Small)
            } else {
                None
            };
            lines.push(StyledLine {
                size,
                centered,
                text: text.to_owned(),
            });
        }
    }
    metadata::MetadataInput { lines }
}

/// The heading-roles question, and what its answer is checked against.
struct RolesQuestion {
    input: heading_roles::HeadingRolesInput,
    probes: Vec<oc_ai::task::heading_roles::HoldoutProbe>,
    facts: oc_ai::task::InventoryFacts,
    heading_clusters: std::collections::BTreeSet<u32>,
    limits: oc_ai::task::heading_roles::HeadingRolesLimits,
}

impl RolesQuestion {
    fn build(input: &StructureInput, current: &StructureOutput, t: &Thresholds) -> Self {
        use heading_roles::{Align, ClusterSummary, Probe, Weight};
        let inventory = &current.inventory;
        let limits = oc_ai::task::heading_roles::HeadingRolesLimits {
            max_clusters: usize::try_from(t.inventory.max_clusters).unwrap_or_default(),
            min_body_char_share: t.inventory.min_body_char_share as f32,
            min_probes: usize::try_from(t.inventory.holdout_min_probes).unwrap_or_default(),
            holdout_disagree_max: t.inventory.holdout_disagree_max as f32,
            chapter_min_count: u32::try_from(t.inventory.chapter_cluster_min_count)
                .unwrap_or_default(),
            chapter_max_count: u32::try_from(t.inventory.chapter_cluster_max_count)
                .unwrap_or_default(),
        };

        // Every run's cluster, once: the counts and the probe pool both read it.
        let mut runs_of: std::collections::BTreeMap<u32, Vec<String>> =
            std::collections::BTreeMap::new();
        for run in &input.runs {
            let text = run.text.trim();
            if text.is_empty() {
                continue;
            }
            if let Some(cluster) = inventory.cluster_of(run, &input.fonts, t) {
                runs_of.entry(cluster.0).or_default().push(text.to_owned());
            }
        }

        let style = |cluster: &oc_structure::headings::cluster::StyleCluster| {
            (
                if cluster.is_bold(t) {
                    Weight::Bold
                } else {
                    Weight::Regular
                },
                if f64::from(cluster.centered_ratio) >= t.llm.heading_roles_centered_ratio_min {
                    Align::Centered
                } else {
                    Align::Left
                },
            )
        };
        let clusters: Vec<ClusterSummary> = inventory
            .clusters
            .iter()
            .map(|cluster| {
                let (weight, align) = style(cluster);
                ClusterSummary {
                    c: cluster.id.0,
                    size_z: cluster.size_z,
                    weight,
                    italic: cluster.italic,
                    align,
                    count: u32::try_from(runs_of.get(&cluster.id.0).map_or(0, Vec::len))
                        .unwrap_or(u32::MAX),
                    starts_page_ratio: cluster.starts_page_ratio,
                    examples: cluster.examples.clone(),
                }
            })
            .collect();

        // The held-out probe: runs not shown as examples, one cluster at a time in rank order,
        // round and round, until `inventory.holdout_max_probes` or the pool runs out.
        let max_probes = usize::try_from(t.inventory.holdout_max_probes).unwrap_or_default();
        let mut pools: Vec<(u32, std::collections::VecDeque<String>)> = inventory
            .clusters
            .iter()
            .map(|cluster| {
                let mut seen = std::collections::BTreeSet::new();
                let pool = runs_of
                    .get(&cluster.id.0)
                    .into_iter()
                    .flatten()
                    .filter(|text| !cluster.examples.contains(text))
                    .filter(|text| seen.insert((*text).clone()))
                    .cloned()
                    .collect();
                (cluster.id.0, pool)
            })
            .collect();
        let mut holdout = Vec::new();
        let mut probes = Vec::new();
        while holdout.len() < max_probes && pools.iter().any(|(_, pool)| !pool.is_empty()) {
            for (cluster_id, pool) in &mut pools {
                if holdout.len() >= max_probes {
                    break;
                }
                let Some(text) = pool.pop_front() else {
                    continue;
                };
                let Some(cluster) = inventory
                    .clusters
                    .iter()
                    .find(|cluster| cluster.id.0 == *cluster_id)
                else {
                    continue;
                };
                let (weight, align) = style(cluster);
                let i = u32::try_from(holdout.len()).unwrap_or(u32::MAX);
                holdout.push(Probe {
                    i,
                    text,
                    size_z: cluster.size_z,
                    weight,
                    italic: cluster.italic,
                    align,
                });
                probes.push(oc_ai::task::heading_roles::HoldoutProbe {
                    i,
                    cluster: *cluster_id,
                });
            }
        }

        let facts = oc_ai::task::InventoryFacts {
            clusters: inventory.clusters.len(),
            body_char_share: inventory
                .body_cluster()
                .map_or(0.0, |body| body.char_share(inventory.total_chars)),
        };
        let heading_clusters = current
            .headings
            .iter()
            .filter(|heading| {
                heading.source == oc_structure::headings::levels::LevelSource::SizeRank
            })
            .map(|heading| heading.cluster.0)
            .collect();
        Self {
            input: heading_roles::HeadingRolesInput {
                language: primary_subtag(input.lang.as_str()).to_owned(),
                clusters,
                holdout,
            },
            probes,
            facts,
            heading_clusters,
            limits,
        }
    }
}

/// The verse-or-quote question: every escalated block, in reading order.
struct VerseQuestion {
    blocks: Vec<oc_ai::task::verse_quote::AmbiguousBlock>,
    limits: oc_ai::task::verse_quote::VerseQuoteLimits,
}

impl VerseQuestion {
    fn build(
        input: &StructureInput,
        current: &StructureOutput,
        escalations: &Escalations,
        t: &Thresholds,
    ) -> Self {
        use oc_structure::quotes::IndentedKind;
        use verse_quote::{BlockKind, BlockSummary, Indent};
        let limits = oc_ai::task::verse_quote::VerseQuoteLimits {
            blocks_per_call: usize::try_from(t.llm.verse_quote_blocks_per_call).unwrap_or(1),
            max_blocks: usize::try_from(t.llm.max_blocks_per_book).unwrap_or_default(),
            verse_min_lines: u32::try_from(t.verse.min_lines).unwrap_or_default(),
            verse_min_short_line_ratio: t.verse.llm_short_line_ratio_min as f32,
        };
        let body = current.inventory.body_size_pt();
        let deep = body * t.llm.verse_quote_deep_indent_em as f32;
        let max_words = usize::try_from(t.llm.verse_quote_max_words).unwrap_or(usize::MAX);

        let blocks = escalations
            .verse_quote
            .iter()
            .filter_map(|(id, escalation)| {
                let Escalation::Yes(record) = escalation else {
                    return None;
                };
                let view = input.blocks.iter().find(|block| block.id == *id)?;
                let signal = |name: &str| {
                    record
                        .signals
                        .iter()
                        .find(|signal| signal.name == name)
                        .map_or(0.0, |signal| signal.value)
                };
                let default = match current
                    .indented
                    .iter()
                    .find(|entry| entry.block == *id)
                    .map(|entry| entry.resolved)
                {
                    Some(IndentedKind::Verse) => BlockKind::Verse,
                    Some(IndentedKind::Pre) => BlockKind::Preformatted,
                    Some(IndentedKind::Paragraph) => BlockKind::Paragraph,
                    _ => BlockKind::Blockquote,
                };
                // Verbatim, line breaks kept, cut at `llm.verse_quote_max_words` words.
                let mut words = 0usize;
                let mut kept: Vec<String> = Vec::new();
                for line in &view.lines {
                    let line_words: Vec<&str> = line.text.split_whitespace().collect();
                    if line_words.is_empty() {
                        continue;
                    }
                    let room = max_words.saturating_sub(words);
                    if room == 0 {
                        break;
                    }
                    kept.push(line_words[..line_words.len().min(room)].join(" "));
                    words = words.saturating_add(line_words.len().min(room));
                }
                let lines = u32::try_from(view.lines.len()).unwrap_or(u32::MAX);
                let total_words: usize = view
                    .lines
                    .iter()
                    .map(|line| line.text.split_whitespace().count())
                    .sum();
                Some(oc_ai::task::verse_quote::AmbiguousBlock {
                    summary: BlockSummary {
                        id: *id,
                        text: kept.join("\n"),
                        indent: if signal("indent_pt") >= deep {
                            Indent::Deep
                        } else {
                            Indent::Shallow
                        },
                        lines,
                        avg_line_words: u32::try_from(
                            total_words
                                .checked_div(view.lines.len().max(1))
                                .unwrap_or_default(),
                        )
                        .unwrap_or(u32::MAX),
                        centered: signal("centered") > 0.0,
                        monospace: signal("monospace") > 0.0,
                    },
                    evidence: oc_ai::task::verse_quote::BlockEvidence {
                        lines,
                        short_line_ratio: signal("short_line_ratio"),
                        monospace: signal("monospace") > 0.0,
                        default,
                    },
                })
            })
            .collect();
        Self { blocks, limits }
    }
}

/// Fast mode takes less of the model's time than quality mode for the same book, and each stays
/// inside its own floor and ceiling.
#[test]
fn fast_mode_takes_less_of_the_models_time_than_quality() {
    use oc_core::jobspec::AiMode;
    use oc_core::thresholds::T;
    for pages in [1, 50, 300, 2_000] {
        let fast = time_budget_ms(AiMode::Fast, pages, &T);
        let quality = time_budget_ms(AiMode::Quality, pages, &T);
        assert!(
            fast < quality,
            "{pages} pages: {fast} ms is not less than {quality} ms"
        );
        let ms = |secs: i64| u64::try_from(secs).expect("a positive bound") * 1000;
        assert!((ms(T.llm.fast_min_budget_secs)..=ms(T.llm.fast_max_budget_secs)).contains(&fast));
        assert!(
            (ms(T.llm.quality_min_budget_secs)..=ms(T.llm.quality_max_budget_secs))
                .contains(&quality)
        );
    }
}

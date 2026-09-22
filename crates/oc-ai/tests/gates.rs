//! Gate S and gate D (D13.5, ARCHITECTURE §6.2): what a model's answer must be to be considered at
//! all, and what the book records when it is not.
//!
//! Every rejection here is of the *whole* answer. A mapping with one bad entry is not a mapping
//! with one fewer entry: the model has shown it did not follow the question, and the entries it
//! got right are no longer evidence of anything.

mod common;

use oc_ai::gates::fallback::{settle, Choice};
use oc_ai::gates::schema::gate_schema;
use oc_ai::gates::GateFailure;
use oc_ai::prompt::v1::book_structure::BookStructureAnswer;
use oc_ai::prompt::v1::heading_roles::{HeadingRole, HeadingRolesAnswer};
use oc_ai::prompt::v1::metadata::MetadataAnswer;
use oc_ai::prompt::v1::verse_quote::{BlockKind, VerseQuoteAnswer};
use oc_ai::provider::trace;
use oc_model::confidence::Method;
use oc_model::decision::{Decision, LlmTrace};

/// The deterministic answer a heading-roles escalation falls back to (ARCHITECTURE §6.1).
const SIZE_RANK: &str = "size_rank";

fn roles_choice() -> Choice {
    Choice {
        stage: "structure",
        kind: "heading_roles",
        subject: None,
        deterministic: SIZE_RANK.to_owned(),
        alternatives: vec!["llm_mapping".to_owned()],
    }
}

fn roles_trace(output: &str) -> LlmTrace {
    let request = &common::requests()[1];
    trace("qwen3-1.7b-q4_k_m", request, output, false, 412)
}

/// What gate D records for a heading-roles answer that `gate_schema` judged.
fn settled(output: &str) -> Decision {
    let verdict = gate_schema::<HeadingRolesAnswer>(output, &common::heading_roles_input());
    settle(
        roles_choice(),
        roles_trace(output),
        verdict.map(|_| "llm_mapping".to_owned()),
    )
}

/// Test 8.2. The commonest way a small model breaks a JSON contract is to wrap the JSON in a
/// markdown fence, because that is what its chat tuning rewarded. The answer inside is perfect and
/// is still refused: a gate that stripped fences would be a gate that guesses what was meant.
#[test]
fn gate_s_rejects_markdown_fence() {
    let fenced = format!("```json\n{}\n```", common::HEADING_ROLES_ANSWER);
    let verdict = gate_schema::<HeadingRolesAnswer>(&fenced, &common::heading_roles_input());
    assert!(
        matches!(verdict, Err(GateFailure::Unparseable(_))),
        "{verdict:?}"
    );

    let decision = settled(&fenced);
    assert_eq!(decision.method, Method::Deterministic);
    assert_eq!(
        decision.chosen, SIZE_RANK,
        "the deterministic fallback is used"
    );
    assert!(decision.fallback_used());
}

/// Test 8.3. `chapter_headings` is one letter away from a legal role and is not one. An enum is a
/// closed set; near enough is a different role nobody defined.
#[test]
fn gate_s_rejects_out_of_enum_role() {
    let plural = common::HEADING_ROLES_ANSWER.replace("chapter_heading\"", "chapter_headings\"");
    let verdict = gate_schema::<HeadingRolesAnswer>(&plural, &common::heading_roles_input());
    assert_eq!(
        verdict,
        Err(GateFailure::OutOfEnum {
            field: "r",
            value: "chapter_headings".to_owned()
        })
    );
}

/// Test 8.4. A cluster answered twice, or not at all, is a mapping that does not describe the
/// inventory it was asked about — rejected whole, whichever of the two it is.
#[test]
fn gate_s_rejects_id_non_bijection() {
    let input = common::heading_roles_input();

    let duplicated = common::HEADING_ROLES_ANSWER.replace("{\"c\":1,", "{\"c\":0,");
    match gate_schema::<HeadingRolesAnswer>(&duplicated, &input) {
        Err(GateFailure::NotBijective {
            what,
            duplicated,
            missing,
            invented,
        }) => {
            assert_eq!(what, "cluster");
            assert_eq!(duplicated, vec!["0"]);
            assert_eq!(missing, vec!["1"]);
            assert!(invented.is_empty());
        }
        other => panic!("a duplicated cluster id was not refused: {other:?}"),
    }

    let missing = common::HEADING_ROLES_ANSWER.replace("{\"c\":2,\"r\":\"running_head\"}", "");
    let missing = missing.replace(",]", "]");
    match gate_schema::<HeadingRolesAnswer>(&missing, &input) {
        Err(GateFailure::NotBijective { missing, .. }) => assert_eq!(missing, vec!["2"]),
        other => panic!("a missing cluster id was not refused: {other:?}"),
    }

    let invented = common::HEADING_ROLES_ANSWER.replace("{\"c\":2,", "{\"c\":7,");
    match gate_schema::<HeadingRolesAnswer>(&invented, &input) {
        Err(GateFailure::NotBijective { invented, .. }) => assert_eq!(invented, vec!["7"]),
        other => panic!("an invented cluster id was not refused: {other:?}"),
    }
}

/// Test 8.12. Thinking is disabled per request (D10), and that lever is best effort; the check is
/// not. A `<think>` block anywhere fails gate S, so a provider that ignored the lever degrades to
/// the deterministic answer instead of leaking reasoning into the book — even when valid JSON
/// follows the block.
#[test]
fn thinking_output_is_rejected() {
    let input = common::heading_roles_input();
    for output in [
        format!(
            "<think>\nThe centred bold cluster opens chapters.\n</think>\n\n{}",
            common::HEADING_ROLES_ANSWER
        ),
        format!("<think>\n\n</think>\n\n{}", common::HEADING_ROLES_ANSWER),
    ] {
        assert_eq!(
            gate_schema::<HeadingRolesAnswer>(&output, &input),
            Err(GateFailure::ThinkingPresent)
        );
    }
}

/// Test 8.9. Gate D: the deterministic answer stands, and the log says a model was asked, what it
/// was asked (by hash), and which gate refused it — which is the difference between a pipeline
/// that guessed and one that knows it was contradicted (A8.1).
#[test]
fn gate_d_records_fallback_in_decision_log() {
    let out_of_enum = common::HEADING_ROLES_ANSWER.replace("\"body\"", "\"prose\"");
    let decision = settled(&out_of_enum);

    assert_eq!(decision.method, Method::Deterministic);
    assert!(decision.fallback_used());
    assert_eq!(decision.fallback, Some("S.enum"));
    assert_eq!(decision.chosen, SIZE_RANK);
    assert_eq!(decision.kind, "heading_roles");
    assert_eq!(decision.stage, "structure");

    let llm = decision
        .llm
        .expect("the call is recorded although its answer was not used");
    assert_eq!(llm, roles_trace(&out_of_enum));
    assert_eq!(llm.output_sha256.len(), 64);
}

/// The positive control for all of the above: an answer that passes is the model's, recorded as
/// such, with no fallback.
#[test]
fn an_accepted_answer_is_recorded_as_the_models() {
    let decision = settled(common::HEADING_ROLES_ANSWER);
    assert_eq!(decision.method, Method::Llm);
    assert_eq!(decision.chosen, "llm_mapping");
    assert!(!decision.fallback_used());
    assert_eq!(decision.fallback, None);
    assert!(decision.llm.is_some());
}

/// Every worked answer passes gate S and means what it says — a gate that refuses everything
/// would pass every rejection test above.
#[test]
fn gate_s_accepts_every_worked_answer() {
    let metadata =
        gate_schema::<MetadataAnswer>(common::METADATA_ANSWER, &common::metadata_input())
            .expect("the metadata answer passes");
    assert_eq!(metadata.title.as_deref(), Some("Die Verwandlung"));
    assert_eq!(metadata.authors, vec!["Franz Kafka"]);
    assert_eq!(metadata.translator, None);

    let roles = gate_schema::<HeadingRolesAnswer>(
        common::HEADING_ROLES_ANSWER,
        &common::heading_roles_input(),
    )
    .expect("the heading-roles answer passes");
    assert_eq!(roles.clusters.get(&0), Some(&HeadingRole::ChapterHeading));
    assert_eq!(roles.clusters.get(&2), Some(&HeadingRole::RunningHead));
    assert_eq!(roles.probes.get(&1), Some(&HeadingRole::Body));

    let structure = gate_schema::<BookStructureAnswer>(
        common::BOOK_STRUCTURE_ANSWER,
        &common::book_structure_input(),
    )
    .expect("the book-structure answer passes");
    assert_eq!(structure.frontmatter_end_idx, 1);
    assert_eq!(structure.backmatter_start_idx, 4);
    assert!(structure.part_boundaries.is_empty());

    let verse = gate_schema::<VerseQuoteAnswer>(
        &common::verse_quote_answer(),
        &common::verse_quote_input(),
    )
    .expect("the verse answer passes");
    assert_eq!(
        verse.kinds.get(&common::verse_block_id()),
        Some(&BlockKind::Verse)
    );
}

/// A friendly sentence before the JSON, or after it, is not JSON.
#[test]
fn gate_s_rejects_a_preamble_and_a_trailer() {
    let input = common::heading_roles_input();
    let answer = common::HEADING_ROLES_ANSWER;
    for output in [
        format!("Here is the mapping you asked for: {answer}"),
        format!("{answer}\nI hope this helps!"),
        String::new(),
        "{\"m\":[".to_owned(),
    ] {
        assert!(
            matches!(
                gate_schema::<HeadingRolesAnswer>(&output, &input),
                Err(GateFailure::Unparseable(_))
            ),
            "{output:?}"
        );
    }
}

/// Valid JSON of the wrong shape: a key said twice (the second silently winning would be the
/// parser choosing an answer), a field the task does not have, a field it needs and did not get,
/// an empty string where the task says absent is null.
#[test]
fn gate_s_rejects_json_of_the_wrong_shape() {
    let roles = common::heading_roles_input();
    for output in [
        common::HEADING_ROLES_ANSWER.replace("{\"c\":0,", "{\"c\":0,\"c\":1,"),
        common::HEADING_ROLES_ANSWER.replace("{\"m\":", "{\"note\":\"x\",\"m\":"),
        common::HEADING_ROLES_ANSWER.replace(
            ",\"h\":[{\"i\":0,\"r\":\"chapter_heading\"},{\"i\":1,\"r\":\"body\"}]",
            "",
        ),
        common::HEADING_ROLES_ANSWER.replace("{\"c\":0,", "{\"c\":\"0\","),
    ] {
        assert!(
            matches!(
                gate_schema::<HeadingRolesAnswer>(&output, &roles),
                Err(GateFailure::WrongShape(_))
            ),
            "{output:?}"
        );
    }

    let pages = common::metadata_input();
    let without_date = common::METADATA_ANSWER.replace(",\"date\":\"1915\"", "");
    let empty_title = common::METADATA_ANSWER.replace("\"Die Verwandlung\"", "\"\"");
    for output in [without_date, empty_title] {
        assert!(
            matches!(
                gate_schema::<MetadataAnswer>(&output, &pages),
                Err(GateFailure::WrongShape(_))
            ),
            "{output:?}"
        );
    }
}

/// A block id the batch did not contain is refused even when it is shaped exactly like one: the
/// model cannot label a block it was not shown.
#[test]
fn gate_s_rejects_a_block_the_batch_did_not_contain() {
    let foreign = "AAAAAAAAAA";
    let output =
        common::verse_quote_answer().replace(&common::verse_block_id().to_string(), foreign);
    match gate_schema::<VerseQuoteAnswer>(&output, &common::verse_quote_input()) {
        Err(GateFailure::NotBijective {
            what,
            missing,
            invented,
            ..
        }) => {
            assert_eq!(what, "block");
            assert_eq!(invented, vec![foreign]);
            assert_eq!(missing, vec![common::verse_block_id().to_string()]);
        }
        other => panic!("a foreign block id was not refused: {other:?}"),
    }

    let prose = common::verse_quote_answer().replace("\"verse\"", "\"prose\"");
    assert_eq!(
        gate_schema::<VerseQuoteAnswer>(&prose, &common::verse_quote_input()),
        Err(GateFailure::OutOfEnum {
            field: "k",
            value: "prose".to_owned()
        })
    );
}

/// One bad entry among good ones rejects the whole answer, and the failure names it — the good
/// entries are not salvaged.
#[test]
fn one_bad_entry_rejects_the_whole_answer() {
    let one_bad = common::HEADING_ROLES_ANSWER.replace("\"running_head\"", "\"footer\"");
    let verdict = gate_schema::<HeadingRolesAnswer>(&one_bad, &common::heading_roles_input());
    assert_eq!(
        verdict,
        Err(GateFailure::OutOfEnum {
            field: "r",
            value: "footer".to_owned()
        })
    );
}

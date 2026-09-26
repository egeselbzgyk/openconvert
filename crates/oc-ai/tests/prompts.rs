//! The prompt artifacts: one shared prefix, four grammars that parse, and the numbers and enums
//! the artifacts state in more than one place held in agreement (IMPLEMENTATION_PLAN Phase 8,
//! ARCHITECTURE §9.2–§9.3).

mod common;

use std::collections::BTreeSet;

use oc_ai::digest::{hex, sha256};
use oc_ai::gbnf::{Grammar, Node};
use oc_ai::prompt::{self, PROMPT_VERSION};
use oc_ai::provider::Purpose;
use oc_core::thresholds::T;

/// Test 8.1. One byte-identical prefix is what lets a single warm slot serve a whole book: with
/// `-np 1` the server keeps the prefix's KV state between calls, and a prefix that differed by one
/// byte between tasks would be recomputed on every call (D8, ARCHITECTURE §9.3). The four task
/// directories each carry a `system.md`, so this is also the test that holds them identical.
#[test]
fn system_prefix_is_byte_identical_across_purposes() {
    let requests = common::requests();
    let purposes: BTreeSet<Purpose> = requests.iter().map(|request| request.purpose).collect();
    assert_eq!(
        purposes,
        Purpose::ALL.into_iter().collect(),
        "one request per purpose"
    );

    let prefix = requests[0].system_prefix.as_bytes();
    assert!(!prefix.is_empty());
    for request in &requests {
        assert_eq!(
            request.system_prefix.as_bytes(),
            prefix,
            "{:?}'s system prefix differs from {:?}'s",
            request.purpose,
            requests[0].purpose
        );
    }
    for purpose in Purpose::ALL {
        assert_eq!(prompt::artifacts(purpose).system.as_bytes(), prefix);
    }
}

/// Test 8.14. The parser is llama.cpp's dialect, including its one surprising rule: a rule ends at
/// the end of its line unless it is inside parentheses. A grammar that only *looked* right would
/// make every call fail at the server, and the deterministic fallback would hide that completely.
#[test]
fn grammar_files_parse_as_gbnf() {
    for purpose in Purpose::ALL {
        let parsed = Grammar::parse(prompt::artifacts(purpose).grammar);
        assert!(parsed.is_ok(), "{purpose:?}: {parsed:?}");
    }
}

/// A grammar that parses and admits nothing the task means is as useless as one that does not
/// parse. Every worked answer of Appendix A.3 is in its task's grammar.
#[test]
fn every_worked_answer_is_in_its_grammar() {
    for (request, answer) in common::requests().iter().zip(common::answers()) {
        let grammar = Grammar::parse(request.grammar).expect("the grammar parses");
        assert!(
            grammar.accepts(&answer),
            "{:?} rejects its own worked answer {answer}",
            request.purpose
        );
    }
}

/// The failure modes a model actually has, each outside the grammar: the code fence, the
/// friendly preamble, the pretty-printed object, the plural enum value, and an identifier that is
/// not shaped like one of ours.
#[test]
fn the_near_misses_a_model_produces_are_outside_the_grammar() {
    let roles = grammar(Purpose::HeadingRoles);
    let answer = common::HEADING_ROLES_ANSWER;
    for miss in [
        format!("```json\n{answer}\n```"),
        format!("Sure! Here is the mapping: {answer}"),
        answer.replace(":", ": "),
        answer.replace("chapter_heading", "chapter_headings"),
        format!("{answer}\n"),
    ] {
        assert!(!roles.accepts(&miss), "accepted {miss:?}");
    }

    let verse = grammar(Purpose::VerseQuote);
    let answer = common::verse_quote_answer();
    let id = common::verse_block_id().to_string();
    assert!(!verse.accepts(&answer.replace(&id, &id.to_lowercase())));
    assert!(!verse.accepts(&answer.replace(&id, &id[..9])));
    assert!(!verse.accepts(&answer.replace("verse", "poem")));

    let metadata = grammar(Purpose::Metadata);
    assert!(!metadata.accepts(&common::METADATA_ANSWER.replace("\"Erzählung\"", "\"\"")));
}

/// The grammar bounds are the pre-gate's bounds, stated twice. The pre-gate refuses to call above
/// `inventory.max_clusters`, so a mapping grammar admitting fewer entries would reject honest
/// answers and one admitting more would be dead text; task 4's batch is
/// `llm.verse_quote_blocks_per_call` blocks, and its grammar admits exactly that many answers.
#[test]
fn grammar_bounds_agree_with_the_thresholds_that_bound_the_input() {
    let roles = grammar(Purpose::HeadingRoles);
    assert_eq!(
        1 + repetition_max(&roles, "mapping"),
        T.inventory.max_clusters,
        "heading_roles admits one entry plus the repetition's maximum"
    );

    let verse = grammar(Purpose::VerseQuote);
    assert_eq!(
        1 + repetition_max(&verse, "root"),
        T.llm.verse_quote_blocks_per_call
    );
}

/// JSON Schema is sent where a server prefers it to GBNF (ARCHITECTURE §9.2), so the two must
/// name the same values. A role the schema allows and the grammar does not would be accepted
/// from one provider and impossible from another.
#[test]
fn schema_and_grammar_name_the_same_values() {
    let roles = literal_alternatives(&grammar(Purpose::HeadingRoles), "role");
    let roles_schema = schema(Purpose::HeadingRoles);
    assert_eq!(roles, strings(&roles_schema["$defs"]["role"]["enum"]));
    assert_eq!(
        roles_schema["properties"]["m"]["maxItems"],
        serde_json::json!(T.inventory.max_clusters)
    );

    let kinds = literal_alternatives(&grammar(Purpose::VerseQuote), "kind");
    let verse_schema = schema(Purpose::VerseQuote);
    assert_eq!(
        kinds,
        strings(&verse_schema["properties"]["b"]["items"]["properties"]["k"]["enum"])
    );
    assert_eq!(
        verse_schema["properties"]["b"]["maxItems"],
        serde_json::json!(T.llm.verse_quote_blocks_per_call)
    );
}

/// A released prompt is frozen. The cache key carries `PROMPT_VERSION` and the grammar's hash but
/// not the system prefix's (ARCHITECTURE §9.4), so an edit to `system.md` that did not bump the
/// version would serve every cached answer to a question nobody is asking any more. The manifest
/// pins every artifact of the current version by hash; changing one means a new version directory.
#[test]
fn prompt_artifacts_are_pinned_to_their_version() {
    let manifest = include_str!("../prompts/v1.sha256");
    let mut pinned = std::collections::BTreeMap::new();
    for line in manifest.lines().filter(|line| !line.trim().is_empty()) {
        let (hash, path) = line.split_once("  ").expect("`<sha256>  <path>` per line");
        pinned.insert(path.to_owned(), hash.to_owned());
    }

    let mut actual = std::collections::BTreeMap::new();
    for purpose in Purpose::ALL {
        let artifacts = prompt::artifacts(purpose);
        for (file, text) in [
            ("system.md", artifacts.system),
            ("user.tmpl", artifacts.user_template),
            ("grammar.gbnf", artifacts.grammar),
            ("schema.json", artifacts.schema),
        ] {
            let path = format!("{}/v{PROMPT_VERSION}/{file}", purpose.as_str());
            actual.insert(path, hex(&sha256(text.as_bytes())));
        }
    }
    // The free-text task: no grammar and no schema, and two files of its own.
    let front = prompt::artifacts(Purpose::FrontPage);
    for (file, text) in [
        ("system.md", front.system),
        ("user.tmpl", front.user_template),
        ("kinds.txt", oc_ai::prompt::v1::front_page::KINDS),
        (
            "instruction.txt",
            oc_ai::prompt::v1::front_page::INSTRUCTION,
        ),
    ] {
        let path = format!("front_page/v{PROMPT_VERSION}/{file}");
        actual.insert(path, hex(&sha256(text.as_bytes())));
    }
    assert_eq!(
        actual, pinned,
        "a v{PROMPT_VERSION} prompt artifact changed. A released version is frozen: put the \
         change in a new version directory and bump PROMPT_VERSION"
    );
}

/// Every grammar's header names the file it is, and the directory it names is the current
/// version's: the include paths and `PROMPT_VERSION` are two statements of one fact.
#[test]
fn each_grammar_is_the_current_versions_file_for_its_task() {
    for purpose in Purpose::ALL {
        let header = format!(
            "# crates/oc-ai/prompts/{}/v{PROMPT_VERSION}/grammar.gbnf",
            purpose.as_str()
        );
        assert!(
            prompt::artifacts(purpose).grammar.starts_with(&header),
            "{purpose:?}'s grammar does not start with {header:?}"
        );
    }
}

/// The metadata message is the annotated pages and the instruction, in that order, with the
/// annotations as categories and never as numbers (D13.6: geometry pre-digested into words).
#[test]
fn a_metadata_message_is_its_annotated_pages_then_the_instruction() {
    let request =
        oc_ai::prompt::v1::metadata::request(&common::metadata_input(), common::max_tokens())
            .expect("renders");
    assert_eq!(
        request.user,
        "[LARGE][CENTERED] Die Verwandlung\n\
         [MEDIUM][CENTERED] Erzählung\n\
         [SMALL][CENTERED] von Franz Kafka\n\
         [SMALL] Kurt Wolff Verlag · Leipzig · 1915\n\
         Extract title, subtitle, authors, translator, publisher and date. Copy exactly. Null if \
         absent.\n"
    );
    assert_eq!(request.max_tokens, common::max_tokens());
    assert_eq!(request.prompt_version, PROMPT_VERSION);
}

/// Text from the book is inserted once and never read again as template. A title page that
/// happened to say `{{payload}}` is a title page, not an instruction to the renderer.
#[test]
fn text_from_the_book_is_never_expanded_as_a_template() {
    let mut input = common::verse_quote_input();
    input.blocks[0].text = "{{payload}} and {{pages}}".to_owned();
    let request =
        oc_ai::prompt::v1::verse_quote::request(&input, common::max_tokens()).expect("renders");
    assert!(request.user.contains("{{payload}} and {{pages}}"));
    assert_eq!(request.user.matches("\"blocks\"").count(), 1);
}

/// The regression artefact: the four questions exactly as a model receives them. The cache key is
/// a hash of the rendered message, so a rendering change nobody meant — a field reordered, a float
/// printed another way — would silently orphan every cached answer and every cassette. Here it is
/// a reviewed diff instead.
#[test]
fn the_worked_examples_render_to_these_messages() {
    let rendered: String = common::requests()
        .iter()
        .map(|request| format!("=== {}\n{}", request.purpose.as_str(), request.user))
        .collect();
    insta::assert_snapshot!("rendered_worked_examples", rendered);
}

/// A number that is not finite has no JSON spelling; serde would quietly write `null`, and the
/// model would be asked about a cluster whose size is "nothing".
#[test]
fn a_payload_with_a_non_finite_number_is_refused() {
    let mut input = common::heading_roles_input();
    input.clusters[1].size_z = f32::NAN;
    assert!(oc_ai::prompt::v1::heading_roles::request(&input, common::max_tokens()).is_err());
}

// ---------------------------------------------------------------------------

fn grammar(purpose: Purpose) -> Grammar {
    Grammar::parse(prompt::artifacts(purpose).grammar).expect("the grammar parses")
}

fn schema(purpose: Purpose) -> serde_json::Value {
    serde_json::from_str(prompt::artifacts(purpose).schema).expect("the schema is JSON")
}

fn strings(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .expect("an array")
        .iter()
        .map(|item| item.as_str().expect("a string").to_owned())
        .collect()
}

/// The largest repetition bound in a rule's definition.
fn repetition_max(grammar: &Grammar, rule: &str) -> i64 {
    fn walk(node: &Node, best: &mut Option<u32>) {
        match node {
            Node::Repeat { node, max, .. } => {
                if let Some(max) = max {
                    *best = Some(best.map_or(*max, |b| b.max(*max)));
                }
                walk(node, best);
            }
            Node::Group(alternatives) => {
                for node in alternatives.iter().flatten() {
                    walk(node, best);
                }
            }
            _ => {}
        }
    }
    let mut best = None;
    for node in grammar
        .rule(rule)
        .expect("the rule exists")
        .iter()
        .flatten()
    {
        walk(node, &mut best);
    }
    i64::from(best.expect("the rule has a bounded repetition"))
}

/// The string literals a rule chooses between, with their JSON quotes removed — the enum it
/// states. A single parenthesised group is looked through, which is how a long choice is written.
fn literal_alternatives(grammar: &Grammar, rule: &str) -> Vec<String> {
    let mut alternatives = grammar.rule(rule).expect("the rule exists").clone();
    if let [sequence] = alternatives.as_slice() {
        if let [Node::Group(inner)] = sequence.as_slice() {
            alternatives = inner.clone();
        }
    }
    alternatives
        .iter()
        .map(|sequence| match sequence.as_slice() {
            [Node::Literal(chars)] => {
                let text: String = chars.iter().collect();
                text.trim_matches('"').to_owned()
            }
            other => panic!("{rule} has a non-literal alternative {other:?}"),
        })
        .collect()
}

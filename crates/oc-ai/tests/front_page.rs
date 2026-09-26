//! Task `front_page`: a decision per page — the page as the state, the kinds as lettered options,
//! one letter back — asked once, or twice in opposite orders and taken only when both agree.

use oc_ai::gbnf::Grammar;
use oc_ai::prompt::v1::front_page::{kinds, request, LETTERS};
use oc_ai::provider::{trace, LlmRequest, LlmResponse};
use oc_ai::session::{Asked, Asker, Unasked};
use oc_ai::task::front_page::{ask, defined_words, parse, PageKind};

/// A model that answers from a script, one answer per question, and remembers the questions.
struct Scripted {
    answers: Vec<&'static str>,
    asked: Vec<LlmRequest>,
}

impl Asker for Scripted {
    fn ask(&mut self, request: &LlmRequest) -> Result<Asked, Unasked> {
        let Some(answer) = self.answers.get(self.asked.len()).copied() else {
            return Err(Unasked::Unavailable(oc_ai::provider::LlmError::Protocol(
                "no more answers".to_owned(),
            )));
        };
        self.asked.push(request.clone());
        Ok(Asked {
            response: LlmResponse {
                text: answer.to_owned(),
                reasoning: None,
                tokens_in: 0,
                tokens_out: 0,
                cached: false,
                cached_tokens: None,
                finish_reason: Some("stop".to_owned()),
            },
            trace: trace("scripted", request, answer, false, 0),
        })
    }
}

fn scripted(answers: &[&'static str]) -> Scripted {
    Scripted {
        answers: answers.to_vec(),
        asked: Vec::new(),
    }
}

/// The letter, read against the order the options were listed in.
#[test]
fn a_letter_is_read_against_the_order_it_was_asked_in() {
    let forward: Vec<&str> = defined_words();
    let mut backward = forward.clone();
    backward.reverse();
    assert_eq!(parse("D", &forward), Some(PageKind::Dedication));
    assert_eq!(parse(" d", &forward), Some(PageKind::Dedication));
    assert_eq!(parse("\"C\"", &forward), Some(PageKind::Copyright));
    // The same letter is another kind when the options were listed the other way round.
    assert_eq!(parse("A", &backward), Some(PageKind::Body));
    assert_eq!(parse("Z", &forward), None, "no option has that letter");
    assert_eq!(parse("", &forward), None);
}

/// The prompt's kinds file and the enum are one list, and every kind has a letter.
#[test]
fn the_kinds_file_defines_every_kind_in_order() {
    let words: Vec<&str> = PageKind::ALL.iter().map(|kind| kind.word()).collect();
    assert_eq!(defined_words(), words);
    assert_eq!(LETTERS.len(), words.len());
}

/// Fast mode asks once and takes the answer; quality mode asks twice in opposite orders and
/// takes only an answer both give.
#[test]
fn a_kind_is_taken_once_or_when_both_orders_agree() {
    // Once: `D` is the dedication.
    let mut once = scripted(&["D"]);
    let answer = ask(&mut once, "For my mother", false, 4).expect("asked");
    assert_eq!(answer.kind, Some(PageKind::Dedication));
    assert_eq!(answer.traces.len(), 1);

    // Twice: `D` forwards and `H` backwards are both the dedication.
    let mut agree = scripted(&["D", "H"]);
    let answer = ask(&mut agree, "For my mother", true, 4).expect("asked");
    assert_eq!(answer.kind, Some(PageKind::Dedication));
    assert_eq!(answer.traces.len(), 2);
    assert_ne!(agree.asked[0].user, agree.asked[1].user, "the other order");

    // The same letter twice is two different kinds: a model answering by position.
    let mut position = scripted(&["D", "D"]);
    assert_eq!(ask(&mut position, "x", true, 4).expect("asked").kind, None);

    // A session that refuses the first question refuses the task.
    let mut none = scripted(&[]);
    assert!(ask(&mut none, "x", false, 4).is_err());
}

/// The question is a decision: the page as the state, the kinds as lettered options, and a
/// grammar that admits exactly one of the letters.
#[test]
fn the_question_is_a_decision_over_lettered_kinds() {
    let (question, order) = request("A page", false, 4).expect("renders");
    let payload: serde_json::Value = serde_json::from_str(&question.user).expect("JSON");
    assert_eq!(payload["state"], "A page");
    let options = payload["options"].as_array().expect("options");
    assert_eq!(options.len(), kinds().len());
    assert_eq!(options[0]["label"], "A");
    assert_eq!(options[0]["key"], order[0]);

    let grammar = Grammar::parse(question.grammar).expect("the grammar parses");
    for letter in LETTERS.chars() {
        assert!(grammar.accepts(&letter.to_string()), "{letter}");
    }
    assert!(!grammar.accepts("title"));
    assert!(!grammar.accepts("L"));

    let (reversed, back) = request("A page", true, 4).expect("renders");
    let payload: serde_json::Value = serde_json::from_str(&reversed.user).expect("JSON");
    assert_eq!(payload["options"][0]["key"], back[0]);
    assert_eq!(back[0], "body");
}

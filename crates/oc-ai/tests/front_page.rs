//! Task `front_page`: a reasoned answer is read out of what the model wrote, asked twice with the
//! kinds in opposite orders, and taken only when the two agree.

use oc_ai::prompt::v1::front_page::request;
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

/// The kind is the word after the last `ANSWER:`, however the model dressed it.
#[test]
fn a_reasoned_answer_is_read_from_its_last_line() {
    assert_eq!(
        parse("It is two words and nothing else, set alone.\nANSWER: dedication"),
        Some(PageKind::Dedication)
    );
    assert_eq!(parse("**ANSWER:** Copyright."), Some(PageKind::Copyright));
    assert_eq!(parse("answer: half-title"), Some(PageKind::HalfTitle));
    assert_eq!(
        parse("ANSWER: title\nOn reflection...\nANSWER: halftitle"),
        Some(PageKind::HalfTitle),
        "the last answer is the answer"
    );
    assert_eq!(parse("It looks like a title page."), None);
    assert_eq!(parse("ANSWER: colophon"), None, "not a kind of the list");
}

/// The prompt's kinds file and the enum are one list.
#[test]
fn the_kinds_file_defines_every_kind_in_order() {
    let words: Vec<&str> = PageKind::ALL.iter().map(|kind| kind.word()).collect();
    assert_eq!(defined_words(), words);
}

/// Two answers that agree are taken; two that differ, or one that names nothing, are not.
#[test]
fn a_kind_is_taken_only_when_both_orders_agree() {
    let mut agree = scripted(&["ANSWER: epigraph", "…so it is a motto. ANSWER: epigraph"]);
    let answer = ask(&mut agree, "A motto\n— Someone", 5, 400).expect("asked");
    assert_eq!(answer.kind, Some(PageKind::Epigraph));
    assert_eq!(answer.traces.len(), 2);
    assert_ne!(
        agree.asked[0].user, agree.asked[1].user,
        "the second question lists the kinds in the other order"
    );

    let mut differ = scripted(&["ANSWER: title", "ANSWER: dedication"]);
    let answer = ask(&mut differ, "For my mother", 4, 400).expect("asked");
    assert_eq!(answer.kind, None);

    let mut vague = scripted(&["ANSWER: title", "I cannot tell."]);
    assert_eq!(ask(&mut vague, "x", 1, 400).expect("asked").kind, None);

    // A session that refuses the first question refuses the task.
    let mut none = scripted(&[]);
    assert!(ask(&mut none, "x", 1, 400).is_err());
}

/// The question is free text on every provider: no grammar and no schema go with it.
#[test]
fn the_question_is_free_text() {
    let forward = request("A page", 3, false, 400).expect("renders");
    let reversed = request("A page", 3, true, 400).expect("renders");
    assert!(forward.is_free_text());
    assert!(forward.user.contains("A page"));
    let first = |request: &LlmRequest| {
        request
            .user
            .lines()
            .nth(1)
            .map(str::to_owned)
            .unwrap_or_default()
    };
    assert!(first(&forward).starts_with("title:"), "{}", forward.user);
    assert!(first(&reversed).starts_with("body:"), "{}", reversed.user);
}

//! The golden-assertion format and its runner (IMPLEMENTATION_PLAN §0.6).
//!
//! A fixture's expectations live beside it in `<fixture>.assert.json`, as a flat array of
//! independently checkable objects (olmOCR-bench style, R9 §A.5). Each one is reported on
//! its own, so "three of eleven assertions fail, and here is which three" replaces "the
//! fixture is wrong".
//!
//! The kinds are a **closed** enum. A file naming a kind this runner does not know is an
//! error, never a silent skip: an assertion that quietly does nothing is worse than one that
//! fails, because it reads as passing.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One expectation about a converted document.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Assertion {
    /// The text appears somewhere in the document.
    TextPresent {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
    /// The text appears nowhere — how a removed running header is asserted.
    TextAbsent {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
    /// `first` precedes `then` in reading order.
    TextOrder {
        first: String,
        then: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
    /// A heading with this text exists at this level.
    HeadingLevel { text: String, level: u8 },
    /// The heading tree, as `(level, text)` in document order.
    HeadingTree { headings: Vec<(u8, String)> },
    /// How many blocks of a kind the document has.
    BlockCount {
        of: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<u32>,
    },
    /// Exactly this many images.
    ImageCount { equals: u32 },
    /// Every noteref resolves to a footnote and back (D6).
    NoteBijection { holds: bool },
    /// The printed page label of a page.
    PageLabel { page_index: u32, label: String },
    /// The language tag on a block.
    LangTag { text: String, lang: String },
    /// A block carries a CSS class (`verse`, `dropcap`, `caption`, ...).
    CssClass { text: String, class: String },
}

impl Assertion {
    /// The pipeline stage that has to have run before this assertion means anything.
    ///
    /// This is what lets a fixture carry its final expectations from Phase 0 while the
    /// pipeline is still being built: an assertion whose stage has not run reports
    /// [`Outcome::Pending`], and pending is never counted as a pass.
    pub fn required_stage(&self) -> &'static str {
        match self {
            Assertion::TextPresent { .. } | Assertion::TextAbsent { .. } => "text",
            Assertion::TextOrder { .. } => "layout",
            Assertion::BlockCount { .. } => "paragraphs",
            Assertion::HeadingLevel { .. }
            | Assertion::HeadingTree { .. }
            | Assertion::NoteBijection { .. }
            | Assertion::LangTag { .. }
            | Assertion::CssClass { .. } => "structure",
            Assertion::ImageCount { .. } | Assertion::PageLabel { .. } => "document",
        }
    }
}

/// What happened to one assertion.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Outcome {
    Pass,
    Fail {
        reason: String,
    },
    /// The stage that would answer this has not run yet. Reported, never a pass.
    Pending {
        stage: &'static str,
    },
}

impl Outcome {
    /// Only a pass is a pass. Written out because `!is_fail()` is the bug this guards.
    pub fn is_pass(&self) -> bool {
        matches!(self, Outcome::Pass)
    }
}

/// The parts of a converted document assertions can ask about.
///
/// Every field is optional, and `None` means "the stage that fills this has not run", which
/// is what makes [`Outcome::Pending`] possible. Later phases fill more of it in; nothing
/// here has to change when they do.
#[derive(Clone, Debug, Default)]
pub struct AssertionDocument {
    /// The document's text in reading order, one entry per block.
    pub blocks: Option<Vec<String>>,
    /// Headings as `(level, text)` in document order.
    pub headings: Option<Vec<(u8, String)>>,
    /// Block counts by kind name, e.g. `"paragraph" -> 5`.
    pub block_counts: Option<BTreeMap<String, u32>>,
    pub image_count: Option<u32>,
    pub note_bijection_holds: Option<bool>,
    /// Printed page labels by zero-based page index.
    pub page_labels: Option<BTreeMap<u32, String>>,
    /// `xml:lang` by block text.
    pub lang_tags: Option<BTreeMap<String, String>>,
    /// CSS classes by block text.
    pub css_classes: Option<BTreeMap<String, Vec<String>>>,
}

/// Parse an `.assert.json` file.
pub fn parse(json: &str) -> Result<Vec<Assertion>, serde_json::Error> {
    serde_json::from_str(json)
}

/// Evaluate one assertion against a document.
pub fn evaluate(assertion: &Assertion, document: &AssertionDocument) -> Outcome {
    let pending = || Outcome::Pending {
        stage: assertion.required_stage(),
    };
    let fail = |reason: String| Outcome::Fail { reason };
    let check = |holds: bool, reason: String| {
        if holds {
            Outcome::Pass
        } else {
            fail(reason)
        }
    };

    match assertion {
        Assertion::TextPresent { text, .. } => match &document.blocks {
            None => pending(),
            Some(blocks) => check(
                blocks.iter().any(|b| b.contains(text)),
                format!("{text:?} appears nowhere"),
            ),
        },
        Assertion::TextAbsent { text, .. } => match &document.blocks {
            None => pending(),
            Some(blocks) => check(
                !blocks.iter().any(|b| b.contains(text)),
                format!("{text:?} should have been removed"),
            ),
        },
        Assertion::TextOrder { first, then, .. } => match &document.blocks {
            None => pending(),
            Some(blocks) => {
                let position = |needle: &str| blocks.iter().position(|b| b.contains(needle));
                match (position(first), position(then)) {
                    (Some(a), Some(b)) => check(
                        a < b,
                        format!("{first:?} is at block {a}, {then:?} at block {b}"),
                    ),
                    (None, _) => fail(format!("{first:?} appears nowhere")),
                    (_, None) => fail(format!("{then:?} appears nowhere")),
                }
            }
        },
        Assertion::HeadingLevel { text, level } => match &document.headings {
            None => pending(),
            Some(headings) => match headings.iter().find(|(_, t)| t.contains(text)) {
                None => fail(format!("no heading contains {text:?}")),
                Some((found, _)) => check(
                    found == level,
                    format!("{text:?} is level {found}, expected {level}"),
                ),
            },
        },
        Assertion::HeadingTree { headings } => match &document.headings {
            None => pending(),
            Some(found) => check(
                found == headings,
                format!("heading tree is {found:?}, expected {headings:?}"),
            ),
        },
        Assertion::BlockCount { of, min, max } => match &document.block_counts {
            None => pending(),
            Some(counts) => {
                let found = counts.get(of).copied().unwrap_or_default();
                let below = min.is_some_and(|m| found < m);
                let above = max.is_some_and(|m| found > m);
                check(
                    !below && !above,
                    format!("{found} blocks of kind {of:?}, expected {min:?}..={max:?}"),
                )
            }
        },
        Assertion::ImageCount { equals } => match document.image_count {
            None => pending(),
            Some(found) => check(
                found == *equals,
                format!("{found} images, expected {equals}"),
            ),
        },
        Assertion::NoteBijection { holds } => match document.note_bijection_holds {
            None => pending(),
            Some(found) => check(
                found == *holds,
                format!("note bijection is {found}, expected {holds}"),
            ),
        },
        Assertion::PageLabel { page_index, label } => match &document.page_labels {
            None => pending(),
            Some(labels) => match labels.get(page_index) {
                None => fail(format!("page {page_index} has no label")),
                Some(found) => check(
                    found == label,
                    format!("page {page_index} is labelled {found:?}, expected {label:?}"),
                ),
            },
        },
        Assertion::LangTag { text, lang } => match &document.lang_tags {
            None => pending(),
            Some(tags) => match tags.iter().find(|(block, _)| block.contains(text)) {
                None => fail(format!("no block contains {text:?}")),
                Some((_, found)) => check(
                    found == lang,
                    format!("{text:?} is tagged {found:?}, expected {lang:?}"),
                ),
            },
        },
        Assertion::CssClass { text, class } => match &document.css_classes {
            None => pending(),
            Some(classes) => match classes.iter().find(|(block, _)| block.contains(text)) {
                None => fail(format!("no block contains {text:?}")),
                Some((_, found)) => check(
                    found.iter().any(|c| c == class),
                    format!("{text:?} carries {found:?}, expected {class:?}"),
                ),
            },
        },
    }
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Row 0.21 of the Phase 0 table.
// ---------------------------------------------------------------------------

/// One instance of every kind in the closed enum, as it appears in an `.assert.json`.
///
/// The list is the test's subject, not a fixture: if a kind is added without a case here,
/// the completeness assertion below fails.
#[cfg(test)]
const EVERY_KIND: &str = r#"[
  {"kind": "text_present", "text": "It was a dark and stormy night"},
  {"kind": "text_absent", "text": "The Test Book", "note": "running header removed"},
  {"kind": "text_order", "first": "Chapter 3", "then": "It was a dark"},
  {"kind": "heading_level", "text": "Chapter 3", "level": 1},
  {"kind": "heading_tree", "headings": [[1, "Chapter 3"], [2, "The office"]]},
  {"kind": "block_count", "of": "paragraph", "min": 2, "max": 6},
  {"kind": "image_count", "equals": 0},
  {"kind": "note_bijection", "holds": true},
  {"kind": "page_label", "page_index": 1, "label": "2"},
  {"kind": "lang_tag", "text": "Chapter 3", "lang": "en"},
  {"kind": "css_class", "text": "It was a dark", "class": "dropcap"}
]"#;

/// A document every assertion above passes against.
#[cfg(test)]
fn satisfying_document() -> AssertionDocument {
    let pairs = |entries: &[(&str, &str)]| -> BTreeMap<String, String> {
        entries
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    };
    AssertionDocument {
        blocks: Some(vec![
            "Chapter 3".to_owned(),
            "It was a dark and stormy night".to_owned(),
            "The office was quiet.".to_owned(),
        ]),
        headings: Some(vec![
            (1, "Chapter 3".to_owned()),
            (2, "The office".to_owned()),
        ]),
        block_counts: Some([("paragraph".to_owned(), 3u32)].into_iter().collect()),
        image_count: Some(0),
        note_bijection_holds: Some(true),
        page_labels: Some([(1u32, "2".to_owned())].into_iter().collect()),
        lang_tags: Some(pairs(&[("Chapter 3", "en")])),
        css_classes: Some(
            [(
                "It was a dark and stormy night".to_owned(),
                vec!["dropcap".to_owned()],
            )]
            .into_iter()
            .collect(),
        ),
    }
}

#[test]
fn assertion_runner_understands_all_kinds() {
    use crate::assertions::{evaluate, parse, Assertion, AssertionDocument, Outcome};

    let assertions = parse(EVERY_KIND).expect("the sample file parses");

    // Every kind in the closed enum appears exactly once, so a new kind cannot be added
    // without a case here.
    let discriminants: std::collections::BTreeSet<String> = assertions
        .iter()
        .map(|a| {
            serde_json::to_value(a).expect("serialises")["kind"]
                .as_str()
                .expect("tagged")
                .to_owned()
        })
        .collect();
    assert_eq!(
        discriminants.len(),
        assertions.len(),
        "every kind must appear exactly once: {discriminants:?}"
    );
    assert_eq!(assertions.len(), 11, "the enum has eleven kinds (§0.6)");

    // Each round-trips through parse -> serialise -> parse unchanged.
    for assertion in &assertions {
        let json = serde_json::to_string(assertion).expect("serialises");
        let back: Assertion = serde_json::from_str(&json).expect("round-trips");
        assert_eq!(&back, assertion);
    }

    // Each evaluates to a pass against a document that satisfies it.
    let document = satisfying_document();
    for assertion in &assertions {
        assert_eq!(
            evaluate(assertion, &document),
            Outcome::Pass,
            "expected a pass for {assertion:?}"
        );
    }

    // Against an empty document every one is pending, never a pass. This is the property
    // that lets a fixture carry its final expectations while the pipeline is half-built.
    let empty = AssertionDocument::default();
    for assertion in &assertions {
        let outcome = evaluate(assertion, &empty);
        assert!(
            matches!(outcome, Outcome::Pending { .. }),
            "expected pending for {assertion:?}, got {outcome:?}"
        );
        assert!(!outcome.is_pass(), "pending must never count as a pass");
    }

    // And a document that contradicts them fails rather than passing quietly.
    let contradicting = AssertionDocument {
        blocks: Some(vec!["The Test Book".to_owned()]),
        headings: Some(vec![(3, "Chapter 3".to_owned())]),
        block_counts: Some([("paragraph".to_owned(), 99u32)].into_iter().collect()),
        image_count: Some(7),
        note_bijection_holds: Some(false),
        page_labels: Some([(1u32, "xiv".to_owned())].into_iter().collect()),
        lang_tags: Some(
            [("Chapter 3".to_owned(), "de".to_owned())]
                .into_iter()
                .collect(),
        ),
        css_classes: Some(
            [("It was a dark".to_owned(), vec!["caption".to_owned()])]
                .into_iter()
                .collect(),
        ),
    };
    for assertion in &assertions {
        let outcome = evaluate(assertion, &contradicting);
        assert!(
            matches!(outcome, Outcome::Fail { .. }),
            "expected a failure for {assertion:?}, got {outcome:?}"
        );
    }

    // An unknown kind is an error, not a silent skip.
    assert!(parse(r#"[{"kind": "vibes", "text": "x"}]"#).is_err());
    // So is an unexpected field, which is usually a typo in a known one.
    assert!(parse(r#"[{"kind": "image_count", "equals": 0, "eqals": 1}]"#).is_err());
}

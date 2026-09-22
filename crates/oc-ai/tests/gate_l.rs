//! Gate L (D13.5): an LLM edit changes names — labels, levels, roles, CSS classes — and never a
//! character of the book.
//!
//! This is the boundary `docs/DECISIONS_LOG.md` states as one question — *if this is answered
//! wrongly, does the book lose or gain a character?* — made executable over generated books and
//! generated edits, in both directions: every edit that changes `C` is refused, and every edit
//! that only renames is admitted.

mod common;

use common::book::{chapter, document, heading, para, quote, text_slots, verse};
use oc_ai::gates::locality::gate_locality;
use oc_ai::gates::GateFailure;
use oc_model::doc::{Content, Section, SectionRole};
use oc_model::document::Document;
use proptest::prelude::*;
use proptest::sample::Index;

/// Letters, marks and signs from the three target languages, with the characters that make
/// Unicode interesting: a combining diaeresis that composes with the letter before it, OHM SIGN
/// and GREEK CAPITAL OMEGA (canonically equivalent), a ligature, and Turkish's four i's.
const ALPHABET: &[char] = &[
    'a', 'e', 'r', 's', 't', 'A', 'Z', '0', '7', '.', '—', 'ı', 'İ', 'i', 'I', 'ş', 'ğ', 'ü', 'é',
    'ß', '\u{0308}', '\u{2126}', '\u{03A9}', 'ﬁ',
];

/// What an inserted or substituted character is drawn from: plain ASCII, so a substitution can
/// always choose a letter different from the one it replaces.
const REPLACEMENTS: &[char] = &['x', 'y', 'q'];

fn word() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(ALPHABET), 1..6)
        .prop_map(|chars| chars.into_iter().collect())
}

/// Text with at least one character the conservation law counts.
fn text() -> impl Strategy<Value = String> {
    prop::collection::vec(word(), 1..5).prop_map(|words| words.join(" "))
}

fn content() -> impl Strategy<Value = Content> {
    prop_oneof![
        text().prop_map(|text| para(&text)),
        (2u8..=3, text()).prop_map(|(level, text)| heading(level, &text)),
        prop::collection::vec(text(), 1..4).prop_map(|lines| {
            let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
            verse(&lines)
        }),
        text().prop_map(|text| quote(vec![para(&text)])),
    ]
}

fn book() -> impl Strategy<Value = Document> {
    prop::collection::vec(
        (text(), 0u32..40, prop::collection::vec(content(), 1..5)),
        1..4,
    )
    .prop_map(|sections| {
        document(
            sections
                .into_iter()
                .map(|(title, page, content)| chapter(1, &title, page, content))
                .collect(),
        )
    })
}

/// An edit of the book's text. Each one changes `C` by construction: it adds a counted
/// character, removes one, or swaps one for an ASCII letter it is not — or duplicates or drops a
/// whole piece of content, every one of which holds counted text.
#[derive(Clone, Debug)]
enum TextEdit {
    Insert { slot: Index, at: Index, ch: char },
    Delete { slot: Index, at: Index },
    Substitute { slot: Index, at: Index, pick: Index },
    Duplicate { section: Index, item: Index },
    Drop { section: Index, item: Index },
}

fn text_edit() -> impl Strategy<Value = TextEdit> {
    prop_oneof![
        (
            any::<Index>(),
            any::<Index>(),
            prop::sample::select(REPLACEMENTS)
        )
            .prop_map(|(slot, at, ch)| TextEdit::Insert { slot, at, ch }),
        (any::<Index>(), any::<Index>()).prop_map(|(slot, at)| TextEdit::Delete { slot, at }),
        (any::<Index>(), any::<Index>(), any::<Index>())
            .prop_map(|(slot, at, pick)| TextEdit::Substitute { slot, at, pick }),
        (any::<Index>(), any::<Index>())
            .prop_map(|(section, item)| TextEdit::Duplicate { section, item }),
        (any::<Index>(), any::<Index>())
            .prop_map(|(section, item)| TextEdit::Drop { section, item }),
    ]
}

/// The byte offsets of the characters in `text` that `C` counts.
fn counted(text: &str) -> Vec<(usize, char)> {
    text.char_indices()
        .filter(|(_, ch)| !ch.is_whitespace())
        .collect()
}

fn apply(before: &Document, edit: &TextEdit) -> Document {
    let mut after = before.clone();
    match edit {
        TextEdit::Insert { slot, at, ch } => {
            let mut slots = text_slots(&mut after);
            let chosen = slot.index(slots.len());
            let text = &mut slots[chosen];
            let boundaries: Vec<usize> = text
                .char_indices()
                .map(|(offset, _)| offset)
                .chain([text.len()])
                .collect();
            text.insert(boundaries[at.index(boundaries.len())], *ch);
        }
        TextEdit::Delete { slot, at } => {
            let mut slots = text_slots(&mut after);
            let chosen = slot.index(slots.len());
            let text = &mut slots[chosen];
            let chars = counted(text);
            let (offset, _) = chars[at.index(chars.len())];
            text.remove(offset);
        }
        TextEdit::Substitute { slot, at, pick } => {
            let mut slots = text_slots(&mut after);
            let chosen = slot.index(slots.len());
            let text = &mut slots[chosen];
            let chars = counted(text);
            let (offset, old) = chars[at.index(chars.len())];
            let others: Vec<char> = REPLACEMENTS.iter().copied().filter(|c| *c != old).collect();
            text.replace_range(
                offset..offset + old.len_utf8(),
                &others[pick.index(others.len())].to_string(),
            );
        }
        TextEdit::Duplicate { section, item } => {
            let section = &mut after.sections[section.index(before.sections.len())];
            let index = item.index(section.content.len());
            let copy = section.content[index].clone();
            section.content.insert(index, copy);
        }
        TextEdit::Drop { section, item } => {
            let section = &mut after.sections[section.index(before.sections.len())];
            let index = item.index(section.content.len());
            section.content.remove(index);
        }
    }
    after
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(5000))]

    /// Test 8.5. For 5 000 generated edits over generated books, every edit that changes `C` is
    /// refused as a change of characters — whichever character, wherever it is, whether one
    /// character or a whole paragraph, and whatever canonical form the book's text is in.
    #[test]
    fn gate_l_rejects_any_character_change(before in book(), edit in text_edit()) {
        let after = apply(&before, &edit);
        match gate_locality(&before, &after) {
            Err(GateFailure::CharactersChanged { lost, gained }) => {
                prop_assert!(lost + gained > 0, "{edit:?} changed nothing");
            }
            other => prop_assert!(false, "{edit:?} was not refused: {other:?}"),
        }
    }
}

/// A rename of every kind a task can make: a heading's level, a section's role, a paragraph made
/// verse along its own word breaks, a paragraph wrapped in a block quotation.
#[derive(Clone, Debug)]
enum LabelEdit {
    Level { section: Index, level: u8 },
    Role { section: Index },
    ToVerse { section: Index, item: Index },
    ToQuote { section: Index, item: Index },
}

fn label_edit() -> impl Strategy<Value = LabelEdit> {
    prop_oneof![
        (any::<Index>(), 1u8..=6).prop_map(|(section, level)| LabelEdit::Level { section, level }),
        any::<Index>().prop_map(|section| LabelEdit::Role { section }),
        (any::<Index>(), any::<Index>())
            .prop_map(|(section, item)| LabelEdit::ToVerse { section, item }),
        (any::<Index>(), any::<Index>())
            .prop_map(|(section, item)| LabelEdit::ToQuote { section, item }),
    ]
}

fn rename(before: &Document, edit: &LabelEdit) -> Document {
    let mut after = before.clone();
    let count = before.sections.len();
    match edit {
        LabelEdit::Level { section, level } => {
            let section: &mut Section = &mut after.sections[section.index(count)];
            section.level = *level;
            if let Some(heading) = &mut section.heading {
                heading.level = *level;
            }
        }
        LabelEdit::Role { section } => {
            after.sections[section.index(count)].role = SectionRole::Part;
        }
        LabelEdit::ToVerse { section, item } => {
            let content = &mut after.sections[section.index(count)].content;
            let index = item.index(content.len());
            if let Content::Paragraph(paragraph) = &content[index] {
                let lines: Vec<String> = paragraph.text.split(' ').map(str::to_owned).collect();
                content[index] = verse(&lines.iter().map(String::as_str).collect::<Vec<_>>());
            }
        }
        LabelEdit::ToQuote { section, item } => {
            let content = &mut after.sections[section.index(count)].content;
            let index = item.index(content.len());
            let wrapped = quote(vec![content[index].clone()]);
            content[index] = wrapped;
        }
    }
    after
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    /// The converse of 8.5, and what keeps gate L from passing it by refusing everything: every
    /// rename a task can make is admitted.
    #[test]
    fn gate_l_admits_every_rename(before in book(), edit in label_edit()) {
        let after = rename(&before, &edit);
        prop_assert_eq!(gate_locality(&before, &after), Ok(()), "{:?}", edit);
    }
}

/// Test 8.6. A role changed and not one character touched: admitted.
#[test]
fn gate_l_allows_label_only_change() {
    let before = document(vec![chapter(
        1,
        "Erster Teil",
        1,
        vec![
            para("Als Gregor Samsa eines Morgens"),
            heading(2, "Kapitel Eins"),
        ],
    )]);
    let mut after = before.clone();
    after.sections[0].role = SectionRole::Part;
    if let Content::Heading(heading) = &mut after.sections[0].content[1] {
        heading.level = 3;
    }
    assert_ne!(before, after, "the edit changed something");
    assert_eq!(gate_locality(&before, &after), Ok(()));
}

/// Every character still there, and the book no longer reads in the order it did: not a rename.
/// A task names things; it does not rearrange them.
#[test]
fn gate_l_rejects_text_that_moved() {
    let before = document(vec![chapter(
        1,
        "One",
        1,
        vec![para("first paragraph"), para("second paragraph")],
    )]);
    let mut after = before.clone();
    after.sections[0].content.swap(0, 1);
    assert_eq!(
        gate_locality(&before, &after),
        Err(GateFailure::TextReordered)
    );
}

/// A running head a model *labelled* is still in the book: label authority is not deletion
/// authority (D13.5). An edit that acted on the label by deleting the line is a change of
/// characters, however obviously furniture the line was.
#[test]
fn a_running_head_label_does_not_license_its_deletion() {
    let before = document(vec![chapter(
        1,
        "Die Verwandlung",
        1,
        vec![
            para("Die Verwandlung"),
            para("Als Gregor Samsa eines Morgens"),
        ],
    )]);
    let mut after = before.clone();
    after.sections[0].content.remove(0);
    assert_eq!(
        gate_locality(&before, &after),
        Err(GateFailure::CharactersChanged {
            lost: 14,
            gained: 0
        })
    );
}

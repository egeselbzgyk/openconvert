//! Dehyphenation: was this word broken across a line, or does it really have a hyphen in it?
//! (PIPELINE §7 step 6, R2 §B.7, R10 §6.3, D13.6, RT A1.)
//!
//! This is the decision the conservation law cannot protect. Joining `Nord-` and `Süd-Achse`
//! into `NordSüd-Achse` removes one character and records it, and every invariant still
//! balances; the book is simply wrong, silently, in a way no check downstream will notice.
//! RT A1 says so explicitly, and it is why this module has three safeguards that are not
//! consequences of the ledger: the tier order, invariant **I-5**, and the fail-closed default.
//!
//! **The numbers that set the design.** Raw accuracy is a misleading metric here, because
//! about 98 % of line-break hyphens should simply be removed — so a do-nothing-clever baseline
//! scores 98.8 % and learns nothing. The number that matters is **recall on "keep the
//! hyphen"**, and a dictionary-only baseline gets that right **31.7 %** of the time against
//! the tiny classifier's **85.8 %** (balanced accuracy 66.87 % → 92.38 %, over 776,700
//! hyphenated words, R2 §B.7). At 31.7 % keep-recall a 300-page novel with ~2,000 hyphenated
//! line breaks produces hundreds of silently corrupted words while every ledger check reports
//! green.
//!
//! **The tiers**, in order, each one stronger evidence than the one after it:
//!
//! | Tier | Rule |
//! |---|---|
//! | T1 | Soft hyphens (U+00AD) are already gone: `N` stripped them in `text`. |
//! | T2 | Candidacy. A line-final `-` or `‐`, and the pieces on either side of it. |
//! | T3 | **The in-document lexicon.** If the joined form occurs elsewhere in *this book*, join; if the hyphenated form does, keep. |
//! | T4 | The language frequency list, plus a compound acceptor for German. |
//!
//! T3 is first among the evidence tiers on purpose and it is the trick Calibre found: the
//! book's own vocabulary is stronger evidence than any external dictionary, it costs nothing,
//! and it is self-calibrating to the book's proper nouns, neologisms and domain terms. A
//! book about pipelines contains the word `pipeline`.
//!
//! **Fail closed.** When nothing decides, keep the hyphen. A spurious hyphen is visible and
//! the reader can fix it; a wrong join corrupts a word silently *and conserves the character
//! multiset exactly*.

pub mod classifier;
pub mod lexicon;
pub mod tiers;

use oc_core::thresholds::Thresholds;
use oc_model::confidence::{Confidence, Signal};
use oc_model::lang::LangTag;

pub use lexicon::DocLexicon;

/// The hyphens a line break can be made with.
///
/// U+00AD is not here: `N` removed it in `text`, under its own ledger reason, and a soft
/// hyphen that reached this far would be a bug in that stage rather than a case for this one.
/// U+2011 is a *non-breaking* hyphen — a typesetter's instruction never to break here — so a
/// line ending on one is not a line break at all.
pub const LINE_BREAK_HYPHENS: [char; 2] = ['\u{002D}', '\u{2010}'];

/// What to do with a hyphen at a line break.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HyphenAction {
    /// The word was broken: remove the hyphen and join the pieces.
    Join,
    /// The hyphen belongs to the word: keep it.
    Keep,
    /// Nothing decided. The caller keeps the hyphen — see [`Decision::resolved`].
    Undecided,
}

/// Which tier answered.
///
/// Recorded because the report says which, and because they are not equally strong: T3's
/// in-document evidence is materially better than T4's lexicon evidence (PIPELINE §7,
/// confidence signals).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    /// Not a candidate: there is no line-break hyphen here.
    NotACandidate,
    /// T2 refused: the shape of the break rules a join out.
    Candidacy,
    /// T3: the book's own vocabulary.
    InDocument,
    /// T4: the language's frequency list, or the German compound acceptor.
    Lexicon,
    /// The classifier, when the tiers leave it open.
    Classifier,
    /// Nothing decided, so the hyphen stays.
    FailClosed,
}

/// A dehyphenation verdict, with the evidence that produced it.
#[derive(Clone, Debug, PartialEq)]
pub struct Decision {
    pub action: HyphenAction,
    pub tier: Tier,
    pub confidence: Confidence,
}

impl Decision {
    /// The action actually taken. `Undecided` resolves to `Keep`, and that is the whole of
    /// the fail-closed rule (RT C4).
    pub fn resolved(&self) -> HyphenAction {
        match self.action {
            HyphenAction::Join => HyphenAction::Join,
            HyphenAction::Keep | HyphenAction::Undecided => HyphenAction::Keep,
        }
    }

    /// A verdict that carries a number — the classifier's log-odds — rather than only a
    /// predicate. The score is recorded whichever way the verdict went, because a report that
    /// shows why a hyphen stayed is worth as much as one that shows why it went.
    pub(crate) fn with_score(
        action: HyphenAction,
        tier: Tier,
        signals: Vec<Signal>,
        score: f32,
    ) -> Self {
        let mut decision = Self::new(action, tier, signals);
        decision.confidence = decision.confidence.with_score(score);
        decision
    }

    fn new(action: HyphenAction, tier: Tier, signals: Vec<Signal>) -> Self {
        let confidence = match action {
            HyphenAction::Undecided => Confidence::fallback(signals),
            _ => Confidence::deterministic(signals),
        };
        Self {
            action,
            tier,
            confidence,
        }
    }
}

/// Decide what to do with the hyphen between two line-broken pieces.
///
/// `left` is the text of the line that ends with the hyphen, `right` the text of the line
/// that follows it. Both are whole lines, not words: the pieces are taken from their ends
/// here, so that a caller cannot get the tokenisation subtly different from the one the
/// lexicon was built with.
pub fn dehyphenate(
    left: &str,
    right: &str,
    doc: &DocLexicon,
    lang: &LangTag,
    t: &Thresholds,
) -> Decision {
    let Some((head, tail)) = pieces(left, right) else {
        return Decision::new(
            HyphenAction::Keep,
            Tier::NotACandidate,
            vec![Signal::new("candidate", 0.0)],
        );
    };

    // Whether this book capitalises its nouns, read off the book itself as well as off its
    // language tag: German does, and so does any book in a language that does.
    let capitalises_nouns = doc.mid_sentence_capital_rate() >= t.dehyphen.noun_capital_rate_min
        || lang.primary() == "de";
    if !tiers::is_candidate(&head, &tail, capitalises_nouns) {
        return Decision::new(
            HyphenAction::Keep,
            Tier::Candidacy,
            vec![Signal::new("candidate", 0.0)],
        );
    }

    if let Some(decision) = tiers::in_document(&head, &tail, doc) {
        return decision;
    }
    let frequent_rank = usize::try_from(t.dehyphen.function_word_rank.max(0)).unwrap_or(0);
    if let Some(decision) = tiers::suspended(&head, &tail, doc, frequent_rank) {
        return decision;
    }
    if capitalises_nouns {
        if let Some(decision) = tiers::german(&head, &tail, doc) {
            return decision;
        }
    }
    let min_stem = usize::try_from(t.dehyphen.stem_min_chars.max(1)).unwrap_or(usize::MAX);
    let tail_chars = usize::try_from(t.dehyphen.stem_tail_chars.max(1)).unwrap_or(usize::MAX);
    // Only a lower-case continuation: a capital after the break is a name or a noun, and what
    // the book's stems say about those is not what they say about a broken word.
    let lower = tail.chars().next().is_some_and(char::is_lowercase);
    if lower {
        if let Some(decision) = tiers::stem(&head, &tail, doc, min_stem, tail_chars) {
            return decision;
        }
        let min_words = u64::try_from(t.dehyphen.style_min_words.max(0)).unwrap_or(u64::MAX);
        if let Some(decision) = tiers::book_style(
            &head,
            &tail,
            doc,
            t.dehyphen.lower_compound_rate_max,
            min_words,
        ) {
            return decision;
        }
    }
    if let Some(decision) = tiers::lexicon(&head, &tail, lang) {
        return decision;
    }

    // What the tiers left open is the residual the classifier exists for, and it is not a
    // rare case: a book's own vocabulary says nothing about a word it uses once.
    if let Some(decision) = classifier::classify(&head, &tail, lang, t) {
        if decision.action != HyphenAction::Undecided {
            return decision;
        }
    }

    Decision::new(
        HyphenAction::Undecided,
        Tier::FailClosed,
        vec![Signal::new("tiers_decided", 0.0)],
    )
}

/// The word pieces either side of a line-break hyphen, or `None` when there is no break here.
///
/// The hyphen is *not* included in the head: what the tiers reason about is `pipe` and
/// `line`, and re-attaching it at every call site is how a rule ends up testing `pipe-` for
/// membership in a word list and never finding it.
pub fn pieces(left: &str, right: &str) -> Option<(String, String)> {
    let left = left.trim_end();
    let last = left.chars().next_back()?;
    if !LINE_BREAK_HYPHENS.contains(&last) {
        return None;
    }
    let head: String = left[..left.len() - last.len_utf8()]
        .chars()
        .rev()
        .take_while(|ch| is_word_char(*ch))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let tail: String = right
        .trim_start()
        .chars()
        .take_while(|ch| is_word_char(*ch))
        .collect();
    if head.is_empty() || tail.is_empty() {
        return None;
    }
    Some((head, tail))
}

/// What counts as part of a word for the purpose of finding the two pieces.
///
/// Letters, marks and digits, plus the interior hyphen, because `Nord-Süd-Achse` broken after
/// `Nord-Süd-` has a hyphen inside its head and losing it would hide the very evidence that
/// says to keep the break.
pub fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '\u{2019}' || ch == '\'' || LINE_BREAK_HYPHENS.contains(&ch)
}

/// The joined form of two pieces, which is what a `Join` produces.
pub fn joined(head: &str, tail: &str) -> String {
    format!("{head}{tail}")
}

/// The hyphenated form, which is what a `Keep` produces.
pub fn hyphenated(head: &str, tail: &str) -> String {
    format!("{head}-{tail}")
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Rows 3.8, 3.10, 3.11 and 3.17.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use oc_core::thresholds::T;
    use oc_model::confidence::Method;
    use proptest::prelude::*;

    fn lexicon(texts: &[&str], lang: &LangTag) -> DocLexicon {
        DocLexicon::build(texts.iter().copied(), lang)
    }

    /// Row 3.8. The in-document tier: the book says `pipeline` is a word, so `pipe-` / `line`
    /// is a broken one. No external dictionary is consulted and none is needed.
    #[test]
    fn dehyphenate_joins_when_indoc_evidence() {
        let doc = lexicon(&["the pipeline ran the length of the valley"], &LangTag::EN);
        let decision = dehyphenate("a broken pipe-", "line here", &doc, &LangTag::EN, &T);

        assert_eq!(decision.action, HyphenAction::Join);
        assert_eq!(decision.tier, Tier::InDocument);
        assert_eq!(decision.confidence.method, Method::Deterministic);
    }

    /// And the mirror image: the book spells it with a hyphen, so the hyphen stays.
    #[test]
    fn dehyphenate_keeps_when_the_document_spells_it_with_a_hyphen() {
        let doc = lexicon(&["the pipe-line was surveyed twice"], &LangTag::EN);
        let decision = dehyphenate("a broken pipe-", "line here", &doc, &LangTag::EN, &T);

        assert_eq!(decision.action, HyphenAction::Keep);
        assert_eq!(decision.tier, Tier::InDocument);
    }

    /// A book that uses both spellings has said nothing about this break, and the tier says so
    /// rather than counting votes.
    #[test]
    fn a_document_that_spells_it_both_ways_decides_nothing() {
        let doc = lexicon(&["the pipeline and the pipe-line"], &LangTag::EN);
        let decision = dehyphenate("a broken pipe-", "line here", &doc, &LangTag::EN, &T);

        assert_ne!(decision.tier, Tier::InDocument);
    }

    /// Row 3.10, and the rule the whole module is built around. `blan-` / `dish` is in no
    /// lexicon, the halves are not both attested, and the classifier's log-odds are inside
    /// its margin — a coin toss. So nothing decides, and the hyphen stays, recorded as a
    /// fallback rather than as a verdict.
    #[test]
    fn dehyphenate_fails_closed_on_unknown() {
        let doc = lexicon(&["nothing relevant here"], &LangTag::EN);
        let decision = dehyphenate("a bit of blan-", "dish talk", &doc, &LangTag::EN, &T);

        assert_eq!(decision.resolved(), HyphenAction::Keep);
        assert_eq!(decision.confidence.method, Method::Deterministic);
        assert!(
            decision.confidence.fallback_used,
            "a fail-closed keep is not the same claim as a decided keep: {decision:?}"
        );
    }

    /// And the case that is *not* fail-closed: unattested, but the model is sure. The keep is
    /// a verdict, with its score on the record.
    #[test]
    fn an_unattested_pair_the_model_is_sure_about_is_decided() {
        let doc = lexicon(&["nothing relevant here"], &LangTag::EN);
        let decision = dehyphenate("the zarquon-", "blivet follows", &doc, &LangTag::EN, &T);

        assert_eq!(decision.resolved(), HyphenAction::Keep);
        assert_eq!(decision.tier, Tier::Classifier);
        assert!(!decision.confidence.fallback_used);
        assert!(decision.confidence.score.is_some());
    }

    /// Row 3.17. Turkish is agglutinative, so the joined form of a real break is very often a
    /// productively inflected type that no finite list contains. The dictionary signal is
    /// therefore weak, and a weak signal must not produce a confident wrong join.
    #[test]
    fn turkish_agglutinative_join_prefers_keep() {
        let doc = lexicon(&["kitap okudum"], &LangTag::TR);
        let decision = dehyphenate("elimdeki kitap-", "larımızdan biri", &doc, &LangTag::TR, &T);

        assert_eq!(decision.resolved(), HyphenAction::Keep);
        assert_ne!(
            decision.action,
            HyphenAction::Join,
            "an unattested agglutinated form is not evidence to join"
        );
    }

    /// The candidacy tier. An upper-case continuation in English is not a broken word.
    #[test]
    fn an_uppercase_continuation_is_not_a_candidate_in_english() {
        let doc = lexicon(&["nothing"], &LangTag::EN);
        let decision = dehyphenate("the Anglo-", "Saxon world", &doc, &LangTag::EN, &T);
        assert_eq!(decision.tier, Tier::Candidacy);
        assert_eq!(decision.resolved(), HyphenAction::Keep);
    }

    /// In German it is, because every noun is capitalised and `Fahr-` / `Zeug` is an entirely
    /// ordinary broken `Fahrzeug`. The later tiers decide it on evidence.
    #[test]
    fn an_uppercase_continuation_is_still_a_candidate_in_german() {
        let doc = lexicon(&["das Fahrzeug steht"], &LangTag::DE);
        let decision = dehyphenate("ein Fahr-", "Zeug dort", &doc, &LangTag::DE, &T);
        assert_eq!(decision.action, HyphenAction::Join);
        assert_eq!(decision.tier, Tier::InDocument);
    }

    /// A number range is not a broken word in any language.
    #[test]
    fn a_number_range_is_never_joined() {
        let doc = lexicon(&["nothing"], &LangTag::EN);
        let decision = dehyphenate("in 2019-", "2020 the", &doc, &LangTag::EN, &T);
        assert_eq!(decision.resolved(), HyphenAction::Keep);
    }

    /// A line that does not end in a hyphen is not a candidate at all.
    #[test]
    fn a_line_without_a_hyphen_is_not_a_candidate() {
        let doc = lexicon(&["nothing"], &LangTag::EN);
        let decision = dehyphenate("no hyphen here", "and none here", &doc, &LangTag::EN, &T);
        assert_eq!(decision.tier, Tier::NotACandidate);
    }

    /// The non-breaking hyphen is an instruction never to break here, so a line ending on one
    /// is not a line break.
    #[test]
    fn a_non_breaking_hyphen_is_not_a_line_break() {
        let doc = lexicon(&["nothing"], &LangTag::EN);
        let decision = dehyphenate("a non\u{2011}", "breaking one", &doc, &LangTag::EN, &T);
        assert_eq!(decision.tier, Tier::NotACandidate);
    }

    /// The frequency list, on a language that has one: `under` and `stand` are both words and
    /// `understand` is too, so the list alone cannot settle it and the question goes to the
    /// classifier. This is the residual R2 §B.7 measures, and it is not a rare case.
    #[test]
    fn two_attested_halves_that_also_form_a_word_go_to_the_classifier() {
        let doc = lexicon(&["nothing relevant"], &LangTag::EN);
        let decision = dehyphenate("we under-", "stand it", &doc, &LangTag::EN, &T);
        assert_eq!(decision.tier, Tier::Classifier);
        assert_eq!(decision.resolved(), HyphenAction::Keep);
    }

    /// A book long enough to have a style, in which nothing is ever hyphenated: every word
    /// distinct enough that no stem search finds the break's word.
    fn a_book_of(words: usize, extra: &str) -> Vec<String> {
        let mut text: Vec<String> = (0..words)
            .map(|index| format!("w{index}x"))
            .collect::<Vec<_>>()
            .chunks(10)
            .map(|chunk| chunk.join(" "))
            .collect();
        text.push(extra.to_owned());
        text
    }

    /// A book that writes no compound with a hyphen in it does not break one at a line end:
    /// the unattested break is the typesetter's, and it is joined — whatever the language.
    #[test]
    fn a_book_that_hyphenates_nothing_joins_an_unattested_break() {
        let text = a_book_of(3000, "hiçbir şey");
        let doc = DocLexicon::build(text.iter(), &LangTag::TR);
        let decision = dehyphenate("şüphe uyan-", "dırırız dedi", &doc, &LangTag::TR, &T);
        assert_eq!(decision.action, HyphenAction::Join, "{decision:?}");
        assert_eq!(decision.tier, Tier::InDocument);
    }

    /// And a book that writes such compounds all the time says nothing by its style, so an
    /// unattested break goes on to the tiers after it.
    #[test]
    fn a_book_that_hyphenates_freely_says_nothing_by_its_style() {
        let compounds: Vec<String> = (0..200).map(|index| format!("well-known{index}")).collect();
        let text = a_book_of(3000, &compounds.join(" "));
        let doc = DocLexicon::build(text.iter(), &LangTag::EN);
        let decision = dehyphenate("a bit of blan-", "dish talk", &doc, &LangTag::EN, &T);
        assert_ne!(decision.tier, Tier::InDocument, "{decision:?}");
    }

    /// The book's own stems: `uyandırırız` occurs nowhere else, but `uyandırdı` does, so the
    /// break is inside a word the book uses.
    #[test]
    fn an_inflected_form_of_a_word_the_book_uses_is_joined() {
        let doc = lexicon(&["onu uyandırdı ve gitti"], &LangTag::TR);
        let decision = dehyphenate("şüphe uyan-", "dırırız dedi", &doc, &LangTag::TR, &T);
        assert_eq!(decision.action, HyphenAction::Join, "{decision:?}");
        assert_eq!(decision.tier, Tier::InDocument);
    }

    /// A suspended compound — `Haus-` / `und Gartenarbeit` — keeps its hyphen: the book writes
    /// `und` after a hanging hyphen elsewhere, mid-line.
    #[test]
    fn a_suspended_compound_keeps_its_hyphen() {
        let doc = lexicon(&["der Ein- und Ausgang war offen"], &LangTag::DE);
        let decision = dehyphenate("die Haus-", "und Gartenarbeit", &doc, &LangTag::DE, &T);
        assert_eq!(decision.resolved(), HyphenAction::Keep, "{decision:?}");
    }

    proptest! {
        /// Row 3.11, invariant I-5. Whatever the pieces, a join removes **exactly one**
        /// hyphen and nothing else: the joined form is the two pieces concatenated, and the
        /// difference between it and the hyphenated form is that one character.
        ///
        /// The property is stated over the operation rather than over the decision, because
        /// this is the half of I-5 that the ledger cannot check for itself — the ledger sees
        /// one hyphen leave, and what it cannot see is whether the word it left behind is the
        /// word it should be.
        #[test]
        fn dehyphenate_i5_removes_exactly_one_hyphen(
            head in "[a-zßäöü]{1,12}",
            tail in "[a-zßäöü]{1,12}",
            hyphen in proptest::sample::select(LINE_BREAK_HYPHENS.to_vec()),
        ) {
            let left = format!("a line ending {head}{hyphen}");
            let right = format!("{tail} and more");
            let (found_head, found_tail) =
                pieces(&left, &right).ok_or_else(|| TestCaseError::fail("not a candidate"))?;
            prop_assert_eq!(&found_head, &head);
            prop_assert_eq!(&found_tail, &tail);

            let join = joined(&found_head, &found_tail);
            let keep = hyphenated(&found_head, &found_tail);

            // Exactly one hyphen fewer, and every other character in place.
            prop_assert_eq!(
                keep.chars().filter(|ch| LINE_BREAK_HYPHENS.contains(ch)).count(),
                join.chars().filter(|ch| LINE_BREAK_HYPHENS.contains(ch)).count() + 1
            );
            prop_assert_eq!(join.chars().count() + 1, keep.chars().count());
            prop_assert_eq!(join, format!("{head}{tail}"));
        }
    }
}

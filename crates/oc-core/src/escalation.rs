//! The escalation predicates: when the deterministic evidence is too weak to settle a choice
//! (RT C4, ARCHITECTURE §6.1, D13.5's gate D).
//!
//! **Predicates, not scores.** No calibration exists and no gold set exists, and R10 §4.4 calls
//! calibrating these signals for PDF→EPUB "the largest single risk to this architecture". So v1
//! does not need calibration: every trigger is a *structural* predicate that fits no threshold of
//! its own, is testable on day one, and generates the calibration data as a side effect — every
//! escalation is a labelled hard case.
//!
//! **Pure.** Each predicate is a function of the evidence the owning stage measured and of the
//! thresholds, and of nothing else: no clock, no file, no model. That is what lets the table test
//! below state all six at once, and what lets Phase 10 call them from the stage that holds the
//! evidence — `oc-structure` and `oc-text` both depend on this crate, and neither may depend on
//! `oc-ai` (ARCHITECTURE §3.1). Gathering the evidence is the stage's job; deciding whether it is
//! enough is this module's.
//!
//! **What firing means differs, and the table says so.** Four predicates open an LLM task
//! (D13.6): metadata, book structure, heading roles, verse or quote — and only when `ai.enabled`
//! and the budget agree. Dehyphenation has no LLM path in v1: firing *is* the fail-closed answer,
//! "keep the hyphen" (D13.6, R2 §B.7). A run-in heading that fires is a candidate that rides along
//! in the heading-roles call rather than costing a call of its own (PIPELINE §8.2).

use crate::thresholds::Thresholds;

/// What a predicate concluded about one subject, and why, in words a report can carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The deterministic evidence does not settle the choice.
    Fires(&'static str),
    /// It does; nothing is escalated.
    Abstains(&'static str),
}

impl Verdict {
    pub fn fires(self) -> bool {
        matches!(self, Verdict::Fires(_))
    }

    /// The reason, either way.
    pub fn because(self) -> &'static str {
        match self {
            Verdict::Fires(because) | Verdict::Abstains(because) => because,
        }
    }
}

/// Task 1's evidence: what the file declares its title to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MetadataEvidence {
    /// XMP or the Info dictionary names a `dc:title`.
    pub title_present: bool,
    /// The declared title is on PIPELINE §8.8's boilerplate list: `Microsoft Word - …`, a file
    /// name, `untitled`, `Document1`, empty.
    pub title_is_boilerplate: bool,
}

/// Metadata: the declared title is absent, or boilerplate a producer wrote.
pub fn metadata(evidence: &MetadataEvidence) -> Verdict {
    if !evidence.title_present {
        Verdict::Fires("the file declares no title")
    } else if evidence.title_is_boilerplate {
        Verdict::Fires("the declared title is producer boilerplate")
    } else {
        Verdict::Abstains("the file declares a title")
    }
}

/// Task 3's evidence: what the book already says about its own structure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BookStructureEvidence {
    /// Entries in the PDF outline.
    pub outline_entries: u32,
    /// Entries the printed-contents parse found.
    pub toc_entries: u32,
}

/// Book structure: no PDF outline, **and** the printed contents page yielded fewer than
/// `toc.min_entries` entries. An outline is ground truth, and when one exists the call is skipped
/// entirely — the common case in modern born-digital books.
pub fn book_structure(evidence: &BookStructureEvidence, t: &Thresholds) -> Verdict {
    if evidence.outline_entries > 0 {
        Verdict::Abstains("the PDF outline is ground truth")
    } else if i64::from(evidence.toc_entries) >= t.toc.min_entries {
        Verdict::Abstains("the printed contents page parsed")
    } else {
        Verdict::Fires("no outline and no printed contents page")
    }
}

/// Task 2's evidence: how many styles could be headings, and how much numbering orders them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeadingRolesEvidence {
    /// Style clusters that are heading candidates.
    pub candidate_clusters: u32,
    /// Headings found, and how many of them a numbering regex matched.
    pub headings: u32,
    pub numbered_headings: u32,
}

/// Heading roles: more than one candidate style, **and** numbering-regex coverage below 100 %.
/// One style has one rank; full numbering orders every heading by itself.
pub fn heading_roles(evidence: &HeadingRolesEvidence) -> Verdict {
    if evidence.candidate_clusters <= 1 {
        Verdict::Abstains("one heading style: size rank is unambiguous")
    } else if evidence.headings > 0 && evidence.numbered_headings >= evidence.headings {
        Verdict::Abstains("every heading is numbered, and the numbering orders them")
    } else {
        Verdict::Fires("several heading styles, and numbering does not order them all")
    }
}

/// Task 4's evidence about one indented block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VerseQuoteEvidence {
    /// Set in from the body margin by at least `quote.indent_min_em`.
    pub indented: bool,
    /// The share of its lines that stop well short of its measure.
    pub short_line_ratio: f32,
    /// Blocks the book may still send, of `llm.max_blocks_per_book`.
    pub blocks_remaining: u32,
}

/// Where a block's short-line ratio falls against the verse band
/// `[verse.short_line_ratio_min, verse.short_line_ratio_max]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineBand {
    /// Below the band: the lines fill their measure, as a quotation's do.
    Full,
    /// Inside the band, bounds included: the line lengths say neither verse nor quotation.
    Between,
    /// Above the band: the lines stop short, as verse does.
    Short,
}

/// The band a short-line ratio falls in — **the one definition of the band's edges**.
///
/// [`verse_quote`] reads it, and so does `oc-structure`'s classifier of indented blocks, so the
/// block the classifier calls ambiguous and the block this predicate escalates are the same block
/// by construction. Compared in `f32`, the space the ratio is measured in: widening the ratio to
/// the thresholds' `f64` instead would put a block exactly on the closed lower bound outside it —
/// `f64::from(0.35f32)` is 0.3499999… — and a block with 7 of 20 short lines would read as "full
/// lines" (`docs/DECISIONS_LOG.md`, 2026-09-22).
pub fn line_band(short_line_ratio: f32, t: &Thresholds) -> LineBand {
    if short_line_ratio < t.verse.short_line_ratio_min as f32 {
        LineBand::Full
    } else if short_line_ratio > t.verse.short_line_ratio_max as f32 {
        LineBand::Short
    } else {
        LineBand::Between
    }
}

/// Verse or quote: indented, **and** a short-line ratio inside
/// `[verse.short_line_ratio_min, verse.short_line_ratio_max]` — where the line lengths say neither
/// verse nor quotation — **and** the book's block budget has room.
pub fn verse_quote(evidence: &VerseQuoteEvidence, t: &Thresholds) -> Verdict {
    let band = line_band(evidence.short_line_ratio, t);
    if !evidence.indented {
        Verdict::Abstains("not indented: a paragraph")
    } else if band == LineBand::Full {
        Verdict::Abstains("full lines: a block quotation")
    } else if band == LineBand::Short {
        Verdict::Abstains("short lines: verse")
    } else if evidence.blocks_remaining == 0 {
        Verdict::Abstains("the book's block budget is spent")
    } else {
        Verdict::Fires("indented, and the line lengths say neither verse nor quotation")
    }
}

/// The evidence about one line-end hyphen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DehyphenationEvidence {
    /// The joined form occurs elsewhere in the book.
    pub joined_in_document: bool,
    /// The joined form is in the language's lexicon.
    pub joined_in_lexicon: bool,
    /// Both halves occur on their own.
    pub halves_attested: bool,
}

/// Dehyphenation: the joined form is absent from the book **and** from the lexicon, **and** the
/// halves are not both attested. Firing is the fail-closed answer — keep the hyphen — and never a
/// model call: the kilobyte classifier is cheaper, more accurate and more testable (D13.6).
pub fn dehyphenation(evidence: &DehyphenationEvidence) -> Verdict {
    if evidence.joined_in_document {
        Verdict::Abstains("the joined word occurs elsewhere in the book")
    } else if evidence.joined_in_lexicon {
        Verdict::Abstains("the joined word is in the lexicon")
    } else if evidence.halves_attested {
        Verdict::Abstains("both halves occur on their own: a compound, hyphen kept")
    } else {
        Verdict::Fires("no evidence either way: keep the hyphen")
    }
}

/// The evidence about a paragraph's opening.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunInEvidence {
    /// The paragraph opens with a bold or italic run, followed by a run in another style.
    pub styled_opening: bool,
    /// Words in that opening run.
    pub words: u32,
    /// It ends in `.`, `—` or `:`.
    pub terminated: bool,
}

/// Run-in heading: a styled opening run of at most `headings.runin_max_words` words, terminated by
/// `.`, `—` or `:`. Firing makes it a candidate, never a heading: it rides along in the
/// heading-roles call, because a false positive splits a paragraph and moves its first clause into
/// the table of contents.
pub fn run_in_heading(evidence: &RunInEvidence, t: &Thresholds) -> Verdict {
    if !evidence.styled_opening {
        Verdict::Abstains("the paragraph does not open in another style")
    } else if evidence.words == 0 || i64::from(evidence.words) > t.headings.runin_max_words {
        Verdict::Abstains("too long to be a heading")
    } else if !evidence.terminated {
        Verdict::Abstains("no terminator: a styled phrase mid-sentence")
    } else {
        Verdict::Fires("a short styled opening, terminated: a run-in candidate")
    }
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2).
// ---------------------------------------------------------------------------

/// Test 8.16 (RT C4). The six predicates, each on named cases where it fires and where it
/// abstains, in one table — and each one pure: asked twice, it answers the same.
#[test]
fn escalation_predicates_are_pure_and_unit_tested() {
    use crate::thresholds::T;

    let title = |present, boilerplate| MetadataEvidence {
        title_present: present,
        title_is_boilerplate: boilerplate,
    };
    let structure = |outline, toc| BookStructureEvidence {
        outline_entries: outline,
        toc_entries: toc,
    };
    let styles = |clusters, headings, numbered| HeadingRolesEvidence {
        candidate_clusters: clusters,
        headings,
        numbered_headings: numbered,
    };
    let block = |indented, ratio, remaining| VerseQuoteEvidence {
        indented,
        short_line_ratio: ratio,
        blocks_remaining: remaining,
    };
    let hyphen = |document, lexicon, halves| DehyphenationEvidence {
        joined_in_document: document,
        joined_in_lexicon: lexicon,
        halves_attested: halves,
    };
    let opening = |styled, words, terminated| RunInEvidence {
        styled_opening: styled,
        words,
        terminated,
    };

    // (predicate, case, verdict, fires)
    let table: Vec<(&str, &str, Verdict, bool)> = vec![
        (
            "metadata",
            "no title declared",
            metadata(&title(false, false)),
            true,
        ),
        (
            "metadata",
            "\"Microsoft Word - Kapitel.docx\"",
            metadata(&title(true, true)),
            true,
        ),
        (
            "metadata",
            "\"Die Verwandlung\"",
            metadata(&title(true, false)),
            false,
        ),
        (
            "book_structure",
            "no outline, no contents page",
            book_structure(&structure(0, 0), &T),
            true,
        ),
        (
            "book_structure",
            "no outline, two contents lines",
            book_structure(&structure(0, 2), &T),
            true,
        ),
        (
            "book_structure",
            "no outline, a contents page",
            book_structure(&structure(0, 3), &T),
            false,
        ),
        (
            "book_structure",
            "an outline",
            book_structure(&structure(24, 0), &T),
            false,
        ),
        (
            "heading_roles",
            "three styles, half numbered",
            heading_roles(&styles(3, 20, 10)),
            true,
        ),
        (
            "heading_roles",
            "two styles, no headings yet",
            heading_roles(&styles(2, 0, 0)),
            true,
        ),
        (
            "heading_roles",
            "three styles, all numbered",
            heading_roles(&styles(3, 20, 20)),
            false,
        ),
        (
            "heading_roles",
            "one style",
            heading_roles(&styles(1, 20, 0)),
            false,
        ),
        (
            "verse_quote",
            "indented, ratio 0.5",
            verse_quote(&block(true, 0.5, 30), &T),
            true,
        ),
        (
            "verse_quote",
            "indented, at the lower bound",
            verse_quote(&block(true, 0.35, 1), &T),
            true,
        ),
        (
            "verse_quote",
            "indented, at the upper bound",
            verse_quote(&block(true, 0.75, 1), &T),
            true,
        ),
        (
            "verse_quote",
            "not indented",
            verse_quote(&block(false, 0.5, 30), &T),
            false,
        ),
        (
            "verse_quote",
            "full lines",
            verse_quote(&block(true, 0.1, 30), &T),
            false,
        ),
        (
            "verse_quote",
            "short lines",
            verse_quote(&block(true, 0.9, 30), &T),
            false,
        ),
        (
            "verse_quote",
            "budget spent",
            verse_quote(&block(true, 0.5, 0), &T),
            false,
        ),
        (
            "dehyphenation",
            "\"Klassi-fikator\", nothing attested",
            dehyphenation(&hyphen(false, false, false)),
            true,
        ),
        (
            "dehyphenation",
            "joined form elsewhere in the book",
            dehyphenation(&hyphen(true, false, false)),
            false,
        ),
        (
            "dehyphenation",
            "joined form in the lexicon",
            dehyphenation(&hyphen(false, true, false)),
            false,
        ),
        (
            "dehyphenation",
            "\"well-known\", both halves attested",
            dehyphenation(&hyphen(false, false, true)),
            false,
        ),
        (
            "run_in_heading",
            "\"Method.\" in bold",
            run_in_heading(&opening(true, 1, true), &T),
            true,
        ),
        (
            "run_in_heading",
            "eight words and a colon",
            run_in_heading(&opening(true, 8, true), &T),
            true,
        ),
        (
            "run_in_heading",
            "nine words",
            run_in_heading(&opening(true, 9, true), &T),
            false,
        ),
        (
            "run_in_heading",
            "no terminator",
            run_in_heading(&opening(true, 3, false), &T),
            false,
        ),
        (
            "run_in_heading",
            "plain opening",
            run_in_heading(&opening(false, 2, true), &T),
            false,
        ),
    ];

    for (predicate, case, verdict, fires) in &table {
        assert_eq!(
            verdict.fires(),
            *fires,
            "{predicate} on {case}: {verdict:?}"
        );
        assert!(!verdict.because().is_empty());
    }

    // Every predicate is shown firing and abstaining.
    for predicate in [
        "metadata",
        "book_structure",
        "heading_roles",
        "verse_quote",
        "dehyphenation",
        "run_in_heading",
    ] {
        let outcomes: std::collections::BTreeSet<bool> = table
            .iter()
            .filter(|(name, ..)| *name == predicate)
            .map(|(.., fires)| *fires)
            .collect();
        assert_eq!(outcomes.len(), 2, "{predicate} is not shown both ways");
    }

    // Pure: the same evidence, the same verdict, however often it is asked.
    assert_eq!(
        verse_quote(&block(true, 0.5, 30), &T),
        verse_quote(&block(true, 0.5, 30), &T)
    );
    assert_eq!(metadata(&title(true, true)), metadata(&title(true, true)));

    // The bounds the table tests are the thresholds' own.
    assert_eq!(T.verse.short_line_ratio_min, 0.35);
    assert_eq!(T.verse.short_line_ratio_max, 0.75);
    assert_eq!(T.toc.min_entries, 3);
    assert_eq!(T.headings.runin_max_words, 8);
}

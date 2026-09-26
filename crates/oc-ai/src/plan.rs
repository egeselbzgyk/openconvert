//! Which tasks a book asks, and with how many of its calls — decided before any call is made
//! (PHASE 10 details 5–7, D13.6, ratified note N-4).
//!
//! **The language gate.** A task runs under `--ai` only for the languages its evaluation passed
//! (`ai.task.<task>.languages`, keyed on the primary subtag of `dc:language`); for any other it
//! makes no call, and the report says why (`language.gate`). `--ai-all-tasks` sets the gate aside,
//! for an evaluation run or an experiment: that is the flag detail 7 keeps an unproven task behind.
//!
//! **The degradation order.** When the calls the escalated tasks want exceed the calls the book
//! has left, tasks are dropped in a fixed order — `verse_quote` batches first, then
//! `book_structure` chunks beyond the first, then `heading_roles`, then the first `book_structure`
//! chunk — and **never `metadata`**. Deciding this up front is what makes budget exhaustion degrade
//! the same way on every run, instead of by whichever task happened to ask first.

use crate::provider::Purpose;

/// The `Decision.fallback` of an escalation whose task is not enabled for the book's language.
pub const LANGUAGE_GATE: &str = "language.gate";

/// The entry of `ai.task.<task>.languages` that enables a task for every language.
pub const ALL_LANGUAGES: &str = "*";

/// Per task, the languages it is enabled for: `ai.task.<task>.languages`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LanguageGates<'a> {
    pub metadata: &'a [&'a str],
    pub heading_roles: &'a [&'a str],
    pub book_structure: &'a [&'a str],
    pub verse_quote: &'a [&'a str],
    pub front_page: &'a [&'a str],
}

impl LanguageGates<'_> {
    fn of(&self, purpose: Purpose) -> &[&str] {
        match purpose {
            Purpose::Metadata => self.metadata,
            Purpose::HeadingRoles => self.heading_roles,
            Purpose::BookStructure => self.book_structure,
            Purpose::VerseQuote => self.verse_quote,
            Purpose::FrontPage => self.front_page,
        }
    }

    /// Whether `purpose` may run for a book in `language` — a BCP-47 tag, read by its primary
    /// subtag, case-insensitively. `all_tasks` is `--ai-all-tasks`. `*` in a task's list enables
    /// it for every language: a task whose checks read no language at all.
    pub fn allows(&self, purpose: Purpose, language: &str, all_tasks: bool) -> bool {
        if all_tasks {
            return true;
        }
        let primary = language
            .split(['-', '_'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        self.of(purpose)
            .iter()
            .any(|enabled| *enabled == ALL_LANGUAGES || enabled.eq_ignore_ascii_case(&primary))
    }
}

/// What the book's escalations would ask for, after the language gate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Demand {
    pub metadata: bool,
    pub heading_roles: bool,
    /// Chunks of the heading list; 0 when the task is not escalated.
    pub book_structure_chunks: usize,
    /// Batches of ambiguous blocks; 0 when none is escalated.
    pub verse_quote_batches: usize,
}

impl Demand {
    pub fn calls(&self) -> usize {
        usize::from(self.metadata)
            + usize::from(self.heading_roles)
            + self.book_structure_chunks
            + self.verse_quote_batches
    }
}

/// What the book will ask: the demand, cut to the calls it has, in the degradation order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Grant {
    pub metadata: bool,
    pub heading_roles: bool,
    pub book_structure_chunks: usize,
    pub verse_quote_batches: usize,
}

impl Grant {
    pub fn calls(&self) -> usize {
        usize::from(self.metadata)
            + usize::from(self.heading_roles)
            + self.book_structure_chunks
            + self.verse_quote_batches
    }
}

/// Cut `demand` to `calls_left`, dropping in the fixed order and never `metadata`.
pub fn plan(demand: &Demand, calls_left: usize) -> Grant {
    let mut grant = Grant {
        metadata: demand.metadata,
        heading_roles: demand.heading_roles,
        book_structure_chunks: demand.book_structure_chunks,
        verse_quote_batches: demand.verse_quote_batches,
    };
    // 1. verse_quote, one batch at a time from the last.
    while grant.calls() > calls_left && grant.verse_quote_batches > 0 {
        grant.verse_quote_batches -= 1;
    }
    // 2. book_structure chunks beyond the first.
    while grant.calls() > calls_left && grant.book_structure_chunks > 1 {
        grant.book_structure_chunks -= 1;
    }
    // 3. heading_roles.
    if grant.calls() > calls_left {
        grant.heading_roles = false;
    }
    // 4. The first book_structure chunk: what is left before metadata, which is never dropped.
    if grant.calls() > calls_left {
        grant.book_structure_chunks = 0;
    }
    // Metadata is not dropped for another task; with no call at all left it is simply not asked,
    // and the call budget says so.
    if grant.calls() > calls_left {
        grant.metadata = false;
    }
    grant
}

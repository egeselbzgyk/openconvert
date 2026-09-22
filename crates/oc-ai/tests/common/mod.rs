//! IMPLEMENTATION_PLAN Appendix A.3's worked examples, one per task, as the inputs and answers
//! every integration test here renders.
//!
//! The appendix calls them "the seeds for the committed cassettes", and that is what they are
//! used for. Two of them are completed rather than copied: A.3 elides the middle of the
//! `book_structure` heading list with `…`, and writes the `verse_quote` block id as `b0412`,
//! which is not an id oc-model can produce. A seed that no real payload could ever be would
//! test a request the pipeline never sends.

#![allow(dead_code)]

pub mod book;

use oc_ai::prompt::v1::{book_structure, heading_roles, metadata, verse_quote};
use oc_ai::provider::LlmRequest;
use oc_model::geom::Rect;
use oc_model::ids::BlockId;

/// `llm.max_output_tokens_per_call`, the cap every request carries.
pub fn max_tokens() -> u32 {
    u32::try_from(oc_core::thresholds::T.llm.max_output_tokens_per_call)
        .expect("the token cap is a positive u32")
}

// ---------------------------------------------------------------------------
// Task 1 — metadata
// ---------------------------------------------------------------------------

pub fn metadata_input() -> metadata::MetadataInput {
    use metadata::{Size, StyledLine};
    let line = |size, centered, text: &str| StyledLine {
        size,
        centered,
        text: text.to_owned(),
    };
    metadata::MetadataInput {
        lines: vec![
            line(Some(Size::Large), true, "Die Verwandlung"),
            line(Some(Size::Medium), true, "Erzählung"),
            line(Some(Size::Small), true, "von Franz Kafka"),
            line(
                Some(Size::Small),
                false,
                "Kurt Wolff Verlag · Leipzig · 1915",
            ),
        ],
    }
}

pub const METADATA_ANSWER: &str = concat!(
    r#"{"title":"Die Verwandlung","subtitle":"Erzählung","authors":["Franz Kafka"],"#,
    r#""translator":null,"publisher":"Kurt Wolff Verlag","date":"1915"}"#
);

// ---------------------------------------------------------------------------
// Task 2 — heading_roles
// ---------------------------------------------------------------------------

pub fn heading_roles_input() -> heading_roles::HeadingRolesInput {
    use heading_roles::{Align, ClusterSummary, Probe, Weight};
    let examples = |texts: &[&str]| texts.iter().map(|text| (*text).to_owned()).collect();
    heading_roles::HeadingRolesInput {
        language: "de".to_owned(),
        clusters: vec![
            ClusterSummary {
                c: 0,
                size_z: 3.1,
                weight: Weight::Bold,
                italic: false,
                align: Align::Centered,
                count: 24,
                starts_page_ratio: 0.9,
                examples: examples(&["Kapitel Eins", "Kapitel Zwei", "Kapitel Drei"]),
            },
            ClusterSummary {
                c: 1,
                size_z: 0.0,
                weight: Weight::Regular,
                italic: false,
                align: Align::Justified,
                count: 1840,
                starts_page_ratio: 0.02,
                examples: examples(&["Als Gregor Samsa eines Morgens…"]),
            },
            ClusterSummary {
                c: 2,
                size_z: -0.6,
                weight: Weight::Regular,
                italic: true,
                align: Align::Centered,
                count: 312,
                starts_page_ratio: 0.99,
                examples: examples(&["Die Verwandlung"]),
            },
        ],
        holdout: vec![
            Probe {
                i: 0,
                text: "Kapitel Vier".to_owned(),
                size_z: 3.1,
                weight: Weight::Bold,
                italic: false,
                align: Align::Centered,
            },
            Probe {
                i: 1,
                text: "Er lag auf seinem panzerartig harten Rücken".to_owned(),
                size_z: 0.0,
                weight: Weight::Regular,
                italic: false,
                align: Align::Justified,
            },
        ],
    }
}

pub const HEADING_ROLES_ANSWER: &str = concat!(
    r#"{"m":[{"c":0,"r":"chapter_heading"},{"c":1,"r":"body"},{"c":2,"r":"running_head"}],"#,
    r#""h":[{"i":0,"r":"chapter_heading"},{"i":1,"r":"body"}]}"#
);

// ---------------------------------------------------------------------------
// Task 3 — book_structure
// ---------------------------------------------------------------------------

pub fn book_structure_input() -> book_structure::BookStructureInput {
    use book_structure::HeadingEntry;
    let heading = |idx, text: &str, page, c| HeadingEntry {
        idx,
        text: text.to_owned(),
        page,
        c,
    };
    book_structure::BookStructureInput {
        language: "tr".to_owned(),
        headings: vec![
            heading(0, "Önsöz", 3, 1),
            heading(1, "Birinci Bölüm", 7, 0),
            heading(2, "İkinci Bölüm", 41, 0),
            heading(3, "Sonuç", 398, 0),
            heading(4, "Dizin", 410, 1),
        ],
    }
}

pub const BOOK_STRUCTURE_ANSWER: &str =
    r#"{"frontmatter_end_idx":1,"part_boundaries":[],"backmatter_start_idx":4}"#;

// ---------------------------------------------------------------------------
// Task 4 — verse_quote
// ---------------------------------------------------------------------------

/// The block the verse example is about, with an id derived the way oc-model derives every id.
pub fn verse_block_id() -> BlockId {
    BlockId::derive(
        12,
        Rect {
            x0: 96.0,
            y0: 212.0,
            x1: 402.0,
            y1: 288.0,
        },
        "Two roads diverged in a yellow wood,",
    )
}

pub fn verse_quote_input() -> verse_quote::VerseQuoteInput {
    use verse_quote::{BlockSummary, Indent};
    verse_quote::VerseQuoteInput {
        blocks: vec![BlockSummary {
            id: verse_block_id(),
            text: "Two roads diverged in a yellow wood,\nAnd sorry I could not travel both"
                .to_owned(),
            indent: Indent::Deep,
            lines: 4,
            avg_line_words: 7,
            centered: false,
            monospace: false,
        }],
    }
}

pub fn verse_quote_answer() -> String {
    format!(r#"{{"b":[{{"id":"{}","k":"verse"}}]}}"#, verse_block_id())
}

// ---------------------------------------------------------------------------
// All four
// ---------------------------------------------------------------------------

/// One request per task, rendered from the examples above.
pub fn requests() -> Vec<LlmRequest> {
    vec![
        metadata::request(&metadata_input(), max_tokens()).expect("metadata renders"),
        heading_roles::request(&heading_roles_input(), max_tokens())
            .expect("heading_roles renders"),
        book_structure::request(&book_structure_input(), max_tokens())
            .expect("book_structure renders"),
        verse_quote::request(&verse_quote_input(), max_tokens()).expect("verse_quote renders"),
    ]
}

/// Each task's worked answer, in the order [`requests`] returns them.
pub fn answers() -> Vec<String> {
    vec![
        METADATA_ANSWER.to_owned(),
        HEADING_ROLES_ANSWER.to_owned(),
        BOOK_STRUCTURE_ANSWER.to_owned(),
        verse_quote_answer(),
    ]
}

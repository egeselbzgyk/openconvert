//! Phase 4: what `structure` makes of a laid-out book.
//!
//! Row 4.3 is here rather than in `oc-structure` because it is a claim about a *real book*:
//! that the character-weighted mode of `f01`'s styles is its body text and that exactly one
//! other style is a heading candidate. The unit tests in `oc-structure` pin the histogram's
//! behaviour on constructed input; this one pins it on a document Typst set.

use oc_core::ledger_check::ReasonTotals;
use oc_core::thresholds::T;
use oc_model::extract::OutlineEntry;
use oc_model::lang::LangTag;
use oc_model::text::Run;
use oc_pdf::inspect::PdfOpen;
use oc_pdf::pdfium::PdfiumBackend;
use oc_structure::headings::candidate::heading_candidates;
use oc_structure::headings::cluster::{cluster_styles, StyleInventory};
use oc_structure::headings::levels::{assign_levels, HeadingAssignment, LevelSource};
use oc_structure::headings::toc_page::{parse_toc_page, TocPage};
use oc_structure::view::BlockView;
use openconvert::pipeline::{
    body_runs, furniture_stage, layout_stage, text_stage, LayoutStage, TextStage,
};
use openconvert::structure_input::block_views;

/// Everything the `structure` stage reads, for one fixture.
struct Read {
    runs: Vec<Run>,
    text: TextStage,
    layout: LayoutStage,
    outline: Vec<OutlineEntry>,
}

impl Read {
    fn views(&self) -> Vec<BlockView> {
        block_views(&self.text, &self.layout)
    }

    fn inventory(&self) -> StyleInventory {
        cluster_styles(&self.runs, &self.text.fonts, &T)
    }
}

/// Read a fixture and run `text`, `furniture` and `layout` over it.
fn read(relative: &str) -> Read {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "missing fixture {}: {error}; run `cargo run -p xtask -- fixtures`",
            path.display()
        )
    });
    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend.open(&bytes, None).expect("the fixture opens");
    let input = openconvert::input::page_inputs(document.as_ref()).expect("every page extracts");

    let mut totals = ReasonTotals::default();
    let text = text_stage(&input, &mut totals, &T).expect("text conserves");
    let furniture =
        furniture_stage(&text, LangTag::EN, &mut totals, &T).expect("furniture stays in budget");
    let layout = layout_stage(&text, &furniture, &mut totals, &T).expect("layout conserves");
    Read {
        runs: body_runs(&text, &furniture),
        outline: document.outline(),
        text,
        layout,
    }
}

/// The headings a fixture yields, from the sources given.
fn headings_of(
    read: &Read,
    outline: &[OutlineEntry],
    toc: Option<&TocPage>,
) -> (Vec<HeadingAssignment>, oc_model::confidence::Confidence) {
    let views = read.views();
    let inventory = read.inventory();
    let candidates = heading_candidates(&views, &inventory, &read.text.fonts, &T);
    assign_levels(&inventory, &candidates, outline, toc, &LangTag::EN, &T)
}

/// The runs and the font table, for the tests whose subject is the histogram alone.
fn body_of(relative: &str) -> (Vec<Run>, TextStage) {
    let read = read(relative);
    (read.runs, read.text)
}

/// Row 4.3. `f01` is 10 pt Libertinus body with one 1.4 em bold `#heading(level: 1)`.
///
/// Two assertions, and the second is the one that matters: a clustering that found *two*
/// heading candidates on a document with one heading would put a spurious level into every
/// book set with a title page, and one that found none would throw the fast path's anchor
/// away.
#[test]
fn style_clusters_identify_body_mode() {
    let (runs, text) = body_of("../../target/fixtures/f01_prose_single_column.pdf");
    let inventory = cluster_styles(&runs, &text.fonts, &T);

    assert!(
        inventory.valid,
        "an ordinary prose book has a valid inventory: {:?}",
        inventory.warnings
    );
    let body = inventory.body_cluster().expect("f01 has body text");
    assert!(
        f64::from(body.char_share(inventory.total_chars)) >= T.inventory.min_body_char_share,
        "body held {} of {} characters",
        body.char_count,
        inventory.total_chars
    );

    let candidates = inventory.candidates(&T);
    assert_eq!(
        candidates.len(),
        1,
        "f01 sets exactly one style above body: {:?}",
        inventory
            .clusters
            .iter()
            .map(|c| (c.id, c.size_pt, c.weight, c.char_count))
            .collect::<Vec<_>>()
    );

    // And that style is the one "Chapter 3" is set in.
    let heading = inventory
        .clusters
        .iter()
        .find(|cluster| Some(cluster.id) == candidates.first().copied())
        .expect("the candidate is a cluster");
    assert!(
        heading.examples.iter().any(|text| text.contains("Chapter")),
        "the candidate cluster's examples were {:?}",
        heading.examples
    );
    assert!(heading.size_pt > body.size_pt);
}

/// Row 4.1. `f09` carries an outline of seven entries over two levels, and the outline is
/// heading ground truth (PIPELINE §8.1, D13.10).
///
/// "Equal exactly" is the assertion, in both directions: every outline entry binds to a
/// candidate and every heading the stage emits is one the outline named. A detector that
/// found an eighth heading would put a section in the navigation that the producer did not
/// write, and one that found six would lose a chapter.
#[test]
fn outline_is_used_as_heading_ground_truth() {
    let read = read("../../target/fixtures/f09_novel_structure.pdf");
    let (headings, confidence) = headings_of(&read, &read.outline, None);

    let got: Vec<(&str, u8)> = headings
        .iter()
        .map(|heading| (heading.text.as_str(), heading.level))
        .collect();
    assert_eq!(
        got,
        vec![
            ("Preface", 1),
            ("Contents", 1),
            ("Chapter One", 1),
            ("A Section Within", 2),
            ("Chapter Two", 1),
            ("Appendix A", 1),
            ("Index", 1),
        ]
    );
    assert!(headings
        .iter()
        .all(|heading| heading.source == LevelSource::Outline));

    let matched = confidence
        .signals
        .iter()
        .find(|signal| signal.name == "outline_match")
        .map(|signal| signal.value);
    assert_eq!(matched, Some(1.0), "signals were {:?}", confidence.signals);
    assert!(!confidence.fallback_used);
}

/// Row 4.2. The same book with its outline taken away: the printed contents page is parsed
/// and its entries are matched to the same headings.
///
/// The TOC-based baseline is the best-measured approach in Part B — P_ED >= 0.9 median,
/// beating every font-clustering and ML method (R2 §B.5) — and it is the only ground truth
/// available for the 88 % of PDFs that carry no structure tree and the many that carry no
/// outline either.
#[test]
fn toc_page_parsed_when_no_outline() {
    let read = read("../../target/fixtures/f09_novel_structure.pdf");
    let views = read.views();
    let toc = parse_toc_page(&views, &T).expect("f09 prints a contents page");

    assert_eq!(toc.page, 1, "the contents is the second page");
    assert!(
        toc.entries.len() >= usize::try_from(T.toc.min_entries).unwrap_or(3),
        "{:?}",
        toc.entries
    );
    assert_eq!(
        toc.entries
            .iter()
            .map(|entry| (entry.title.as_str(), entry.folio.as_str(), entry.level))
            .collect::<Vec<_>>(),
        vec![
            ("Preface", "i", 1),
            ("Contents", "ii", 1),
            ("Chapter One", "1", 1),
            ("A Section Within", "1", 2),
            ("Chapter Two", "2", 1),
            ("Appendix A", "3", 1),
            ("Index", "3", 1),
        ]
    );

    // And matched: with no outline, the contents page decides the levels.
    let (headings, confidence) = headings_of(&read, &[], Some(&toc));
    assert_eq!(
        headings
            .iter()
            .map(|heading| (heading.text.as_str(), heading.level))
            .collect::<Vec<_>>(),
        vec![
            ("Preface", 1),
            ("Contents", 1),
            ("Chapter One", 1),
            ("A Section Within", 2),
            ("Chapter Two", 1),
            ("Appendix A", 1),
            ("Index", 1),
        ]
    );
    assert!(headings
        .iter()
        .all(|heading| heading.source == LevelSource::TocPage));
    assert!(confidence
        .signals
        .iter()
        .any(|signal| signal.name == "toc_match" && signal.value == 1.0));
}

/// Row 4.4. With neither outline nor contents page, the size rank decides: `f10`'s
/// "Chapter 3" is the largest style and becomes level one, and the smaller bold section
/// headings become level two.
///
/// This is the path the ~25 % error ceiling is quoted against (GROBID 76.43 % F1 on section
/// titles; DocLayNet `Title` human agreement 60-72 %, R2 §B.5, R10 §6.7). It is also the
/// path most real PDFs take, which is why it is tested on its own rather than only as what
/// the other two fall back to.
#[test]
fn heading_level_from_size_rank() {
    let read = read("../../target/fixtures/f10_lists_and_table.pdf");
    let (headings, confidence) = headings_of(&read, &[], None);

    let by_text = |wanted: &str| {
        headings
            .iter()
            .find(|heading| heading.text == wanted)
            .unwrap_or_else(|| {
                panic!(
                    "{wanted} is not a heading; got {:?}",
                    headings
                        .iter()
                        .map(|h| (h.text.as_str(), h.level))
                        .collect::<Vec<_>>()
                )
            })
    };

    assert_eq!(by_text("Chapter 3").level, 1);
    assert_eq!(by_text("Enumerated Procedure").level, 2);
    assert_eq!(by_text("A Ruled Table").level, 2);
    assert_eq!(by_text("A Captioned Figure").level, 2);

    // The numbering regex found the chapter's number, which is the signal the escalation
    // predicate for `heading_roles` reads (PIPELINE §8.8's table).
    assert_eq!(
        by_text("Chapter 3")
            .numbering
            .as_ref()
            .map(|n| n.value.as_str()),
        Some("3")
    );
    assert!(
        confidence.fallback_used,
        "size rank is the fallback, and the report has to be able to say so (D13.5)"
    );
}

/// Row 4.5. No heading tree this pipeline emits ever skips a level.
///
/// A property over every fixture that has headings at all, under every combination of the
/// three sources — because the repair has to hold whichever one decided, and a tree built
/// from a producer's outline is exactly as capable of skipping as one built from size rank.
/// "For all corpus files" is what the plan asks; the corpus arrives in Phase 7, so the scope
/// here is every fixture, the same partial the Phase 2 and Phase 3 rows took.
#[test]
fn heading_tree_has_no_level_skips() {
    for name in [
        "f01_prose_single_column",
        "f02_two_column",
        "f04_german_prose",
        "f06_hyphenation_de",
        "f07_verse_and_quote",
        "f08_footnotes",
        "f09_novel_structure",
        "f10_lists_and_table",
    ] {
        let read = read(&format!("../../target/fixtures/{name}.pdf"));
        let views = read.views();
        let toc = parse_toc_page(&views, &T);
        for (label, outline, toc) in [
            ("outline", read.outline.clone(), None),
            ("toc", Vec::new(), toc.as_ref()),
            ("size-rank", Vec::new(), None),
        ] {
            let (headings, _) = headings_of(&read, &outline, toc);
            let levels: Vec<u8> = headings.iter().map(|heading| heading.level).collect();
            let mut previous = 0u8;
            for level in &levels {
                assert!(
                    *level <= previous + 1,
                    "{name} via {label}: {previous} -> {level} skips a level in {levels:?}"
                );
                previous = *level;
            }
        }
    }
}

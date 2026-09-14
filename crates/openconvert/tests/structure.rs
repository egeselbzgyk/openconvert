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
use oc_structure::figures::{associate_captions, W_CAPTION_AMBIGUOUS};
use oc_structure::headings::candidate::heading_candidates;
use oc_structure::headings::cluster::{cluster_styles, StyleInventory};
use oc_structure::headings::levels::{assign_levels, HeadingAssignment, LevelSource};
use oc_structure::headings::toc_page::{parse_toc_page, TocPage};
use oc_structure::notes::link_notes;
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
    vectors: Vec<oc_model::extract::VectorRegion>,
    images: Vec<oc_model::extract::ImageRef>,
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
    let vectors = (0..document.page_count())
        .filter_map(|page| document.page_vectors(page).ok())
        .flatten()
        .collect();
    let images = openconvert::structure_input::document_images(&text);
    Read {
        runs: body_runs(&text, &furniture),
        outline: document.outline(),
        vectors,
        images,
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

/// Row 4.7. `f08` sets three footnotes over two pages, and every one of them pairs with
/// exactly one marker in the body.
///
/// The bijection is the assertion, in both directions, because both failures are real and
/// named: a marker with no note is EPUBCheck `RSC-007`, and a note with no marker is content
/// a reader never reaches (R2 §B.6, R10 §6.9, R5 §B6).
#[test]
fn footnote_marker_body_bijection() {
    let read = read("../../target/fixtures/f08_footnotes.pdf");
    let views = read.views();
    let body_size = read.inventory().body_size_pt();
    let (notes, refs, stats) = link_notes(&views, &read.vectors, body_size, &T);

    assert_eq!(stats.notes, 3, "f08 sets three notes");
    assert_eq!(stats.markers, 3, "and three markers refer to them");
    assert_eq!(stats.match_rate, 1.0, "{stats:?}");
    assert!(stats.warnings.is_empty(), "{:?}", stats.warnings);

    // Every note has an anchor, and no two notes share one.
    let anchors: Vec<_> = notes.iter().map(|note| note.anchor).collect();
    assert!(anchors.iter().all(Option::is_some), "{anchors:?}");
    let mut distinct = anchors.clone();
    distinct.sort_by_key(|anchor| anchor.map(|id| id.as_str().to_owned()));
    distinct.dedup();
    assert_eq!(distinct.len(), anchors.len(), "two notes share one anchor");

    // And in the other direction: one reference per note, no reference twice.
    assert_eq!(refs.len(), notes.len());
    let mut note_ids: Vec<_> = refs.iter().map(|r| r.note).collect();
    note_ids.sort();
    note_ids.dedup();
    assert_eq!(note_ids.len(), refs.len());

    assert_eq!(
        notes
            .iter()
            .map(|note| note.marker.as_str())
            .collect::<Vec<_>>(),
        vec!["1", "2", "3"]
    );
    assert_eq!(
        stats.separator_rules, 3,
        "each note sits under the short rule Typst draws"
    );
}

/// Row 4.8. `h24` puts a `*` on each of two pages. The symbol cycle resets per page
/// (PIPELINE §8.3), so those are two notes and not one referred to twice.
///
/// A matcher that keys on symbol equality across the book binds both bodies to the first note
/// and orphans the second — which is exactly the broken-reference class the bijection exists
/// to prevent.
#[test]
fn footnote_symbol_cycle_resets_per_page() {
    let read = read("../../corpus/fixtures/handmade/h24_footnote_symbol_cycle.pdf");
    let views = read.views();
    let body_size = read.inventory().body_size_pt();
    let (notes, refs, stats) = link_notes(&views, &read.vectors, body_size, &T);

    assert_eq!(stats.notes, 2, "one note per page");
    assert_eq!(stats.match_rate, 1.0, "{stats:?}");
    assert_eq!(
        notes
            .iter()
            .map(|note| note.marker.as_str())
            .collect::<Vec<_>>(),
        vec!["*", "*"]
    );

    // The two markers are on different pages and resolve to different notes.
    assert_eq!(refs.len(), 2);
    assert_eq!(refs[0].page, 0);
    assert_eq!(refs[1].page, 1);
    assert_ne!(refs[0].note, refs[1].note);
    assert_ne!(
        notes[0].anchor, notes[1].anchor,
        "the two `*` markers must anchor in different blocks"
    );
    assert_eq!(notes[0].page.index, 0);
    assert_eq!(notes[1].page.index, 1);
}

/// Row 4.9. `f10` sets one captioned figure on its last page and an uncaptioned image on the
/// page before it. The caption binds to the figure above it, not to the one on the facing
/// page — which a rule that searched the whole document for the nearest picture would get
/// wrong on any book whose figures cluster.
#[test]
fn caption_associated_to_nearest_figure() {
    let read = read("../../target/fixtures/f10_lists_and_table.pdf");
    let views = read.views();
    let body_size = read.inventory().body_size_pt();
    let (figures, captions, warnings) =
        associate_captions(&read.images, &views, body_size, &LangTag::EN, &T);

    assert_eq!(read.images.len(), 2, "f10 draws two images");
    assert_eq!(
        figures.len(),
        2,
        "every image becomes a figure, captioned or not"
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(
        captions.iter().any(|caption| caption.has_prefix),
        "the captioned figure's caption carries the localized prefix: {captions:?}"
    );

    let captioned: Vec<&oc_model::doc::Figure> = figures
        .iter()
        .filter(|figure| figure.caption.is_some())
        .collect();
    assert_eq!(captioned.len(), 1, "exactly one of the two is captioned");

    // And it is the one on the caption's own page.
    let captioned_image = captioned[0].image;
    let page_of = |id: oc_model::extract::ImageId| {
        read.images
            .iter()
            .find(|image| image.id == id)
            .map(|image| image.page.index)
    };
    assert_eq!(
        page_of(captioned_image),
        Some(2),
        "the caption is on page 2 and so is its figure"
    );

    let text = captioned[0]
        .caption
        .as_ref()
        .map(|spans| oc_model::doc::spans_text(spans))
        .unwrap_or_default();
    // Typst sets a no-break space between the word and the number, and `N` is NFC only —
    // NFKC is banned (D13.4) — so it survives into the text, as it should.
    assert!(
        text.replace('\u{a0}', " ").starts_with("Figure 1"),
        "caption was {text:?}"
    );

    // Alt text is empty rather than invented: an empty `alt` marks an image decorative in
    // EPUB, and 95 % of the alt text in real PDFs is the literal word "Image" (R1 §A.10).
    assert!(figures.iter().all(|figure| figure.alt.is_empty()));
}

/// Row 4.10. `h25` puts two figures side by side with one caption symmetrically beneath the
/// gap between them. The second-best distance equals the best, so no association is made.
///
/// DocLayNet's `Caption` class has inter-annotator agreement of 84-89 (R10 §6.10): this is
/// ambiguous for people too. A caption attached to the wrong picture is worse than no
/// caption, because a reader believes it.
#[test]
fn ambiguous_caption_left_unassociated() {
    let read = read("../../corpus/fixtures/handmade/h25_two_figures_one_caption.pdf");
    let views = read.views();
    let body_size = read.inventory().body_size_pt();
    let (figures, captions, warnings) =
        associate_captions(&read.images, &views, body_size, &LangTag::EN, &T);

    assert_eq!(read.images.len(), 2, "h25 draws two figures");
    assert_eq!(
        captions.len(),
        1,
        "and one caption: {:?}",
        captions.iter().map(|c| c.text.as_str()).collect::<Vec<_>>()
    );
    assert!(
        figures.iter().all(|figure| figure.caption.is_none()),
        "neither figure may take the caption"
    );
    assert!(
        warnings
            .iter()
            .any(|warning| warning.code == W_CAPTION_AMBIGUOUS),
        "the abstention has to be visible: {warnings:?}"
    );
}

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
use oc_structure::images::{drop_ornaments, W_ORNAMENT_DROPPED};
use oc_structure::lists::detect_lists;
use oc_structure::meta::{metadata, InfoDict, MetaSources};
use oc_structure::notes::link_notes;
use oc_structure::quotes::{classify_indented, IndentedKind, TASK_VERSE_QUOTE};
use oc_structure::stage::StructureInput;
use oc_structure::tables::{extract_tables, W_TABLE_AS_IMAGE};
use oc_structure::view::BlockView;
use openconvert::dump_structure::digest;
use openconvert::pipeline::{
    body_runs, furniture_stage, layout_stage, text_stage, LayoutStage, TextStage,
};
use openconvert::pipeline::{structure_stage, StructureStage};
use openconvert::structure_input::block_views;

/// Everything the `structure` stage reads, for one fixture.
struct Read {
    runs: Vec<Run>,
    text: TextStage,
    layout: LayoutStage,
    outline: Vec<OutlineEntry>,
    vectors: Vec<oc_model::extract::VectorRegion>,
    images: Vec<oc_model::extract::ImageRef>,
    xmp: oc_pdf::meta::XmpMeta,
    info: InfoDict,
    /// Per image in `images`, its perceptual hash. Decoding is the backend's, so the hashes
    /// are taken here and handed to `structure` as numbers.
    hashes: Vec<u64>,
    pages: u32,
    labels: Vec<Option<String>>,
    totals: ReasonTotals,
}

impl Read {
    fn views(&self) -> Vec<BlockView> {
        block_views(&self.text, &self.layout)
    }

    fn inventory(&self) -> StyleInventory {
        cluster_styles(&self.runs, &self.text.fonts, &T)
    }

    /// Everything the stage reads, assembled the way a conversion assembles it.
    fn stage_input(&self, filename: &str) -> StructureInput {
        StructureInput {
            blocks: self.views(),
            runs: self.runs.clone(),
            fonts: self.text.fonts.clone(),
            images: self.images.clone(),
            image_hashes: self.hashes.clone(),
            vectors: self.vectors.clone(),
            outline: self.outline.clone(),
            labels: self.labels.clone(),
            drop_caps: self.layout.drop_caps.iter().flatten().cloned().collect(),
            page_count: self.pages,
            meta: MetaSources {
                xmp: self.xmp.clone(),
                info: self.info.clone(),
                filename: filename.to_owned(),
                source_sha256: "0f0f0f".to_owned(),
                language: LangTag::EN,
            },
            lang: LangTag::EN,
        }
    }

    /// Run the whole stage, under the conservation law, as a conversion would.
    fn structure(&self, filename: &str) -> StructureStage {
        let mut totals = self.totals.clone();
        structure_stage(&self.layout, &self.stage_input(filename), &mut totals, &T)
            .expect("structure conserves")
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
    let doc_info = document.doc_info();
    let hashes = images
        .iter()
        .map(|image| {
            // The page-local index is the image's position among those sharing its page,
            // which `document_images` preserves.
            let local = images
                .iter()
                .filter(|other| other.page.index == image.page.index)
                .position(|other| other.id == image.id)
                .unwrap_or_default();
            document
                .image_bytes(
                    image.page.index,
                    oc_model::extract::ImageId(u32::try_from(local).unwrap_or_default()),
                )
                .map(|decoded| oc_pdf::images::perceptual_hash(&decoded))
                .unwrap_or_default()
        })
        .collect();
    Read {
        hashes,
        pages: document.page_count(),
        labels: furniture.labels.clone(),
        totals,
        xmp: document.xmp(),
        info: InfoDict {
            title: doc_info.title.clone(),
            author: doc_info.author.clone(),
        },
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
    let (figures, captions, warnings, _bound) = associate_captions(
        &read.images,
        &views,
        &Default::default(),
        body_size,
        &LangTag::EN,
        &T,
    );

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
    let (figures, captions, warnings, _bound) = associate_captions(
        &read.images,
        &views,
        &Default::default(),
        body_size,
        &LangTag::EN,
        &T,
    );

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

/// Row 4.11. `f10` sets a five-step procedure with two sub-steps under the third: items 1..5,
/// nesting depth two, and nothing outside a list.
///
/// `List-item` is the best-detected structural class in DocLayNet — 86.2 mAP against human
/// agreement of 87-88 (R10 §6.11) — so a failure here is a failure of the implementation
/// rather than of the approach.
#[test]
fn ordered_list_numbering_is_contiguous() {
    let read = read("../../target/fixtures/f10_lists_and_table.pdf");
    let outcome = detect_lists(&read.views(), &[], &std::collections::BTreeMap::new(), &T);
    let (lists, warnings) = (outcome.lists, outcome.warnings);

    assert_eq!(lists.len(), 1, "f10 sets one list");
    assert!(warnings.is_empty(), "{warnings:?}");
    let list = &lists[0];
    assert!(list.ordered);
    assert_eq!(
        list.start, None,
        "it starts at one, so no `start` attribute"
    );
    assert_eq!(list.items.len(), 5, "items 1..5 at the top level");

    // The third item carries the nested list, and it is two deep and no deeper.
    let nested: Vec<usize> = list
        .items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.nested.is_some())
        .map(|(index, _)| index)
        .collect();
    assert_eq!(nested, vec![2], "only the third item nests");
    let inner = list.items[2].nested.as_ref().expect("the third item nests");
    assert_eq!(inner.items.len(), 2);
    assert!(inner.ordered);
    assert!(
        inner.items.iter().all(|item| item.nested.is_none()),
        "nesting stops at two"
    );

    // The marker is not part of the item's text: `<ol>` draws it.
    let text_of = |item: &oc_model::doc::ListItem| match item.content.first() {
        Some(oc_model::doc::Content::Paragraph(para)) => para.text.clone(),
        other => panic!("a list item holds a paragraph, got {other:?}"),
    };
    // The marker stays in the text, and that is the conservation law rather than an
    // oversight: `Reason` is a closed set of fifteen variants and none of them is "a list
    // marker", so `structure` may not drop one. `ListItem.marker` records it for `epub`,
    // which does know that `<ol>` draws its own numbers.
    assert_eq!(
        text_of(&list.items[0]),
        "1. Open the document and read its outline."
    );
    assert_eq!(list.items[0].marker.as_deref(), Some("1."));
    assert_eq!(
        text_of(&inner.items[1]),
        "2. Record which candidates were left unbound."
    );
    assert_eq!(inner.items[1].marker.as_deref(), Some("2."));
}

/// The other half of row 4.12, on a real document: `f01` is prose with no list in it, and a
/// detector that reads any line opening with a number as an item would find several.
#[test]
fn prose_with_no_list_yields_no_list() {
    let read = read("../../target/fixtures/f01_prose_single_column.pdf");
    let outcome = detect_lists(&read.views(), &[], &std::collections::BTreeMap::new(), &T);
    let (lists, warnings) = (outcome.lists, outcome.warnings);
    assert!(lists.is_empty(), "{lists:?}");
    assert!(warnings.is_empty());
}

/// Row 4.13. `f10` draws a 4-column, 3-row lattice: five verticals and four horizontals.
/// It becomes a real `<table>`, and the cell-text multiset equals the source text multiset.
///
/// That equality is the gate, and it is a conservation check in miniature: it catches a grid
/// that dropped a cell, one that duplicated a cell, and — the reason PIPELINE §8.7 names it —
/// a hallucinated cell, if a vision model is ever added.
#[test]
fn ruled_table_becomes_html_table() {
    let read = read("../../target/fixtures/f10_lists_and_table.pdf");
    let views = read.views();
    let outcome = extract_tables(
        &read.vectors,
        &views,
        &Default::default(),
        u32::try_from(read.images.len()).unwrap_or_default(),
        &T,
    );

    assert_eq!(outcome.tables.len(), 1, "f10 draws one lattice");
    assert!(outcome.warnings.is_empty(), "{:?}", outcome.warnings);
    let table = &outcome.tables[0];
    assert!(
        table.fallback_image.is_none(),
        "a clean lattice takes the markup path"
    );
    assert_eq!(table.rows.len(), 3, "three rows");
    assert!(
        table.rows.iter().all(|row| row.len() == 4),
        "four cells in every row: {:?}",
        table.rows.iter().map(Vec::len).collect::<Vec<_>>()
    );

    // Every row has the same cell count after span expansion, which the grid guarantees, and
    // the cell text is the table's text.
    let cells = table.cell_texts();
    assert!(cells.contains(&"Stage".to_owned()), "{cells:?}");
    assert!(cells.contains(&"Conserving".to_owned()), "{cells:?}");
    assert!(cells.contains(&"SoftHyphen".to_owned()), "{cells:?}");

    // The multiset equality the extractor gated on, asserted again from the outside.
    let mut from_cells: Vec<String> = cells.iter().map(|text| text.trim().to_owned()).collect();
    // Runs, not lines: a table row is one baseline, so `text` assembles its four cells into
    // one line and a comparison against lines would compare the row against its cells.
    let mut from_page: Vec<String> = views
        .iter()
        .filter(|block| block.page == 1)
        .flat_map(|block| block.runs())
        .map(|run| run.text.trim().to_owned())
        .filter(|text| !text.is_empty())
        .collect();
    from_cells.retain(|text| !text.is_empty());
    from_cells.sort();
    from_page.retain(|text| from_cells.contains(text));
    from_page.sort();
    assert_eq!(from_cells, from_page);
}

/// Row 4.14. `h26` rules its table above, below and under its header and draws no vertical
/// rules at all — which is how a book actually sets a table.
///
/// Two horizontals bound a region, so the table is found; with no verticals there is no grid
/// to read. PIPELINE §8.7 says what happens then, and accessibility settles the shape rather
/// than engineering taste: an image of a table takes the content away from anyone who cannot
/// see it (DAISY, R10 §6.12), so the fallback still carries every printed line.
#[test]
fn borderless_table_falls_back_to_image_with_details() {
    let read = read("../../corpus/fixtures/handmade/h26_borderless_table.pdf");
    let outcome = extract_tables(&read.vectors, &read.views(), &Default::default(), 0, &T);

    assert_eq!(outcome.tables.len(), 1, "the rules bound one region");
    assert!(
        outcome
            .warnings
            .iter()
            .any(|warning| warning.code == W_TABLE_AS_IMAGE),
        "{:?}",
        outcome.warnings
    );

    let table = &outcome.tables[0];
    assert!(
        table.fallback_image.is_some(),
        "the fallback is an image plus the text, not text alone"
    );
    assert!(table.confidence.fallback_used);

    // And the data is still there: every cell the fixture printed survives into the
    // `<details>` fallback.
    let text = table.cell_texts().join(" ");
    for row in oc_testkit::handmade::BORDERLESS_ROWS {
        for cell in row {
            assert!(text.contains(cell), "{cell:?} is missing from {text:?}");
        }
    }
}

/// Row 4.18. `f07` sets a block quotation, a stanza and a block that is deliberately neither.
///
/// The first two geometry settles. The third it cannot, and the whole point of the row is
/// what happens then: PIPELINE §8.6's deterministic default is taken — blockquote, because
/// the block is indented — **and the block is recorded as an escalation candidate with its
/// signals**. Those records are Phase 10's input and the calibration corpus (RT A7.2), and
/// they are the difference between a pipeline that guessed and one that knows it guessed.
///
/// These four categories are geometrically indistinguishable and no public layout dataset
/// even has the classes: DocLayNet's eleven contain none of `quote`, `verse`, `epigraph`
/// (R10 §6.13). There is nothing to measure against, which is exactly why the abstention is
/// recorded rather than resolved quietly.
#[test]
fn verse_and_quote_ambiguity_recorded_not_guessed() {
    let read = read("../../target/fixtures/f07_verse_and_quote.pdf");
    let views = read.views();
    let body_size = read.inventory().body_size_pt();
    let (classified, escalations) = classify_indented(&views, body_size, &T);

    let kind_of = |needle: &str| {
        let block = views
            .iter()
            .find(|block| block.text.starts_with(needle))
            .unwrap_or_else(|| panic!("no block starts with {needle:?}"));
        classified
            .iter()
            .find(|entry| entry.block == block.id)
            .unwrap_or_else(|| panic!("{needle:?} was not classified"))
    };

    // Geometry settles these two.
    let quotation = kind_of("It is a truth universally");
    assert_eq!(quotation.kind, IndentedKind::BlockQuote);
    assert!(!quotation.confidence.fallback_used);

    let stanza = kind_of("Tyger Tyger");
    assert_eq!(stanza.kind, IndentedKind::Verse);

    // And it cannot settle this one.
    let middle = kind_of("A middle case");
    assert_eq!(middle.kind, IndentedKind::Ambiguous);
    assert_eq!(
        middle.resolved,
        IndentedKind::BlockQuote,
        "the deterministic default is blockquote when the block is indented"
    );
    assert!(
        middle.confidence.fallback_used,
        "a report that cannot tell 'decided' from 'gave up safely' cannot be audited (D13.5)"
    );

    // The record exists, names the task, and carries the signals rather than a conclusion.
    let candidate = escalations
        .iter()
        .find(|candidate| candidate.block == middle.block)
        .expect("the ambiguous block is an escalation candidate");
    assert_eq!(candidate.task, TASK_VERSE_QUOTE);
    assert_eq!(candidate.chosen, IndentedKind::BlockQuote);
    assert!(candidate.alternatives.contains(&IndentedKind::Verse));

    let signal = |name: &str| {
        candidate
            .signals
            .iter()
            .find(|signal| signal.name == name)
            .map(|signal| signal.value)
    };
    let ratio = signal("short_line_ratio").expect("the short-line ratio is recorded");
    assert!(
        f64::from(ratio) >= T.verse.short_line_ratio_min
            && f64::from(ratio) <= T.verse.short_line_ratio_max,
        "the ratio that made it ambiguous was {ratio}"
    );
    assert!(signal("indent_pt").is_some_and(|indent| indent > 0.0));
    assert!(signal("lines").is_some());

    // Nothing is emitted as ambiguous: ambiguity is recorded, not shipped.
    assert!(classified
        .iter()
        .all(|entry| entry.resolved != IndentedKind::Ambiguous));

    // And the three reach the flow as what they were decided to be. Verse is emitted as
    // `Content::Verse` rather than as a paragraph because the one thing a reflowable format
    // must not do to a stanza is re-wrap its lines.
    let stage = read.structure("f07_verse_and_quote.pdf");
    let content: Vec<&oc_model::doc::Content> =
        openconvert::dump_structure::walk(&stage.output.sections)
            .into_iter()
            .flat_map(|section| section.content.iter())
            .collect();
    let verse = content
        .iter()
        .find_map(|item| match item {
            oc_model::doc::Content::Verse(verse) => Some(verse),
            _ => None,
        })
        .expect("the stanza is emitted as verse");
    assert_eq!(verse.stanzas.len(), 1);
    assert_eq!(verse.stanzas[0].len(), 4, "four lines, kept as four");
    assert!(oc_model::doc::spans_text(&verse.stanzas[0][0]).starts_with("Tyger Tyger"));
    assert_eq!(
        content
            .iter()
            .filter(|item| matches!(item, oc_model::doc::Content::BlockQuote(_)))
            .count(),
        2,
        "the quotation and the ambiguous block"
    );
}

/// Row 4.16. `h28` carries `"Microsoft Word - draft.docx"` in its Info dictionary and the
/// book's real title in its XMP packet.
///
/// Boilerplate is **worse than nothing because it looks valid** (PIPELINE §8.8): a converter
/// that trusts `/Title` ships a library whose every second book is called `Microsoft Word -
/// draft`, and nothing downstream can tell that apart from a title somebody meant.
#[test]
fn metadata_prefers_xmp_over_boilerplate_docinfo() {
    let read = read("../../corpus/fixtures/handmade/h28_xmp_over_boilerplate.pdf");
    let views = read.views();
    let sources = MetaSources {
        xmp: read.xmp.clone(),
        info: read.info.clone(),
        filename: "h28_xmp_over_boilerplate.pdf".to_owned(),
        source_sha256: "0f0f0f".to_owned(),
        language: LangTag::EN,
    };
    let (meta, confidence) = metadata(&sources, &views, read.inventory().body_size_pt(), &T);

    // The Info dictionary really does carry the boilerplate — otherwise the test would pass
    // for the wrong reason.
    assert_eq!(
        read.info.title.as_deref(),
        Some(oc_testkit::handmade::BOILERPLATE_TITLE)
    );
    assert_eq!(
        meta.title.as_deref(),
        Some(oc_testkit::handmade::XMP_TITLE),
        "the XMP title wins"
    );
    assert_eq!(meta.source, oc_model::doc::MetaSource::Xmp);
    assert_eq!(meta.authors, vec![oc_testkit::handmade::XMP_AUTHOR]);
    assert!(!confidence.fallback_used);
    assert!(confidence
        .signals
        .iter()
        .any(|signal| signal.name == "info_title_boilerplate" && signal.value == 1.0));
}

/// Row 4.17, end to end: the same bytes converted twice mint the same `dc:identifier`.
///
/// The unit test in `oc-structure` pins the function; this pins the whole path from a file on
/// disk to the identifier, because that is where a stray timestamp or a path would creep in.
#[test]
fn identifier_is_stable_across_reconversions() {
    let read = read("../../corpus/fixtures/handmade/h28_xmp_over_boilerplate.pdf");
    let views = read.views();
    let body = read.inventory().body_size_pt();
    let sha = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
    let sources = |filename: &str, hash: &str| MetaSources {
        xmp: read.xmp.clone(),
        info: read.info.clone(),
        filename: filename.to_owned(),
        source_sha256: hash.to_owned(),
        language: LangTag::EN,
    };

    let once = metadata(&sources("book.pdf", sha), &views, body, &T).0;
    let twice = metadata(&sources("book.pdf", sha), &views, body, &T).0;
    assert_eq!(once.identifier, twice.identifier);
    assert!(once.identifier.starts_with("urn:uuid:"));

    // And it depends on the source bytes and nothing else: a different filename is the same
    // book, a different hash is not.
    let renamed = metadata(&sources("a different name.pdf", sha), &views, body, &T).0;
    assert_eq!(once.identifier, renamed.identifier);
    let other = metadata(&sources("book.pdf", "0000"), &views, body, &T).0;
    assert_ne!(once.identifier, other.identifier);
}

/// Row 4.15. `h27` draws the same small image at the foot of all five of its pages and one
/// figure on one of them. The ornament goes; the figure stays.
///
/// The removal cites no ledger `Reason` and spends no budget, and that is correct rather than
/// an omission: `C` is a multiset of *characters* and an image is not in it (D13.4). What it
/// does instead is warn — a removal nobody can see in the ledger has to be visible somewhere.
#[test]
fn ornament_repeated_on_most_pages_is_dropped() {
    let read = read("../../corpus/fixtures/handmade/h27_repeated_ornament.pdf");
    assert_eq!(read.pages as usize, oc_testkit::handmade::H27_PAGES);
    assert_eq!(
        read.images.len(),
        oc_testkit::handmade::H27_ORNAMENT_PAGES + 1,
        "five ornaments and one figure"
    );

    let policy = drop_ornaments(&read.images, &read.hashes, read.pages, &T);

    assert_eq!(
        policy.dropped.len(),
        oc_testkit::handmade::H27_ORNAMENT_PAGES,
        "every copy of the ornament goes: {policy:?}"
    );
    assert_eq!(
        policy.kept.len(),
        1,
        "and the figure stays — a rule that dropped every small image would pass without this"
    );
    assert!(
        policy
            .warnings
            .iter()
            .any(|warning| warning.code == W_ORNAMENT_DROPPED),
        "{:?}",
        policy.warnings
    );

    // The one that stays is the figure, not an ornament that happened to be missed.
    let kept = read
        .images
        .iter()
        .find(|image| policy.kept.contains(&image.id))
        .expect("one image is kept");
    assert!(
        (kept.bbox.x1 - kept.bbox.x0) > T.images.ornament_max_side_pt as f32,
        "the survivor is the large image"
    );
}

/// Row 4.19. `structure` is Conserving: every operation in it is a label, so I-3 reduces to
/// plain multiset equality across the stage and the ledger is empty.
///
/// The check is what forces the stage's shape. Every block's text lands in exactly one of a
/// section's content, a note's body, a table's cells or a figure's caption; a block consumed
/// by a table is not also a paragraph, a bound caption does not also sit in the flow, and a
/// list marker stays inside its item's text because `Reason` has no variant for it. Anything
/// counted twice or dropped fails here rather than silently in an EPUB somebody reads.
#[test]
fn structure_stage_is_conserving() {
    for name in [
        "../../target/fixtures/f01_prose_single_column.pdf",
        "../../target/fixtures/f02_two_column.pdf",
        "../../target/fixtures/f07_verse_and_quote.pdf",
        "../../target/fixtures/f08_footnotes.pdf",
        "../../target/fixtures/f09_novel_structure.pdf",
        "../../target/fixtures/f10_lists_and_table.pdf",
        "../../corpus/fixtures/handmade/h24_footnote_symbol_cycle.pdf",
        "../../corpus/fixtures/handmade/h26_borderless_table.pdf",
        "../../corpus/fixtures/handmade/h29_drop_cap.pdf",
    ] {
        let read = read(name);
        let stage = read.structure("book.pdf");
        assert!(
            stage.delta.entries().is_empty(),
            "{name}: a Conserving stage has an empty ledger"
        );
        assert_eq!(stage.check.removed_chars, 0, "{name}");
        assert_eq!(stage.check.added_chars, 0, "{name}");
    }
}

/// Row 4.21. `h29` draws its drop cap clear of the text grid, so `layout` gives it a line —
/// and therefore a block — of its own.
///
/// That block is the stray one-character paragraph in waiting: a converter that emits it as a
/// block produces `<p>W</p>` followed by a paragraph beginning "hen the survey", which is a
/// very visible EPUB defect (PIPELINE §6 step 6). The cap belongs to the paragraph it opens,
/// as its first *character* — no space between them.
#[test]
fn drop_cap_is_not_a_one_char_paragraph() {
    let read = read("../../corpus/fixtures/handmade/h29_drop_cap.pdf");
    let cap = oc_testkit::handmade::DROP_CAP;

    // `layout` really does hand it over as its own block, or the test would pass vacuously.
    assert!(
        read.views().iter().any(|block| block.text.trim() == cap),
        "the fixture must present the cap as a block of its own"
    );
    assert_eq!(
        read.layout.drop_caps.iter().flatten().count(),
        1,
        "and `layout` must have detected it"
    );

    let stage = read.structure("h29_drop_cap.pdf");
    let paragraphs: Vec<&oc_model::layout::Para> =
        openconvert::dump_structure::walk(&stage.output.sections)
            .into_iter()
            .flat_map(|section| section.content.iter())
            .filter_map(|content| match content {
                oc_model::doc::Content::Paragraph(para) => Some(para),
                _ => None,
            })
            .collect();

    assert!(
        !paragraphs.iter().any(|para| para.text.trim() == cap),
        "no stray one-character paragraph: {:?}",
        paragraphs
            .iter()
            .map(|p| p.text.as_str())
            .collect::<Vec<_>>()
    );
    let opening = paragraphs
        .iter()
        .find(|para| para.drop_cap)
        .expect("one paragraph carries the drop cap");
    assert!(
        opening.text.starts_with(&format!(
            "{cap}{}",
            oc_testkit::handmade::DROP_CAP_PARAGRAPH
        )),
        "the cap is the paragraph's first character, with no space after it: {:?}",
        opening.text
    );
}

/// Row 4.20. The structural digest of `f09`, snapshotted.
///
/// Counts and shapes, no geometry and nothing derived from it — the lesson of Phase 3's
/// host-dependent glyph boxes (`docs/DECISIONS_LOG.md`, 2026-09-13). IR_SKETCH specifies the
/// contents: counts per `Content` variant and the heading tree as `(level, text[..40])`.
#[test]
fn digest_f09_structure() {
    let read = read("../../target/fixtures/f09_novel_structure.pdf");
    let stage = read.structure("f09_novel_structure.pdf");
    insta::assert_json_snapshot!(digest(&stage.output));
}

/// The same for `f10`, whose subject is the three structures `f09` has none of: a list, a
/// ruled table and a captioned figure.
#[test]
fn digest_f10_structure() {
    let read = read("../../target/fixtures/f10_lists_and_table.pdf");
    let stage = read.structure("f10_lists_and_table.pdf");
    insta::assert_json_snapshot!(digest(&stage.output));
}

/// The book's three zones, on a book that has all of them.
///
/// `f09` numbers its front matter in roman and restarts at arabic one, which is the hard
/// boundary signal detail 5 names — a book that restarts its numbering has told the reader
/// where its body begins. The back matter is found by keyword, and only *after* the body has
/// begun: `Notes on the Text` is an ordinary chapter title when it opens a book.
#[test]
fn book_structure_runs_front_body_back() {
    let read = read("../../target/fixtures/f09_novel_structure.pdf");
    let stage = read.structure("f09_novel_structure.pdf");

    let roles: Vec<(String, Option<String>)> = stage
        .output
        .sections
        .iter()
        .map(|section| {
            (
                format!("{:?}", section.role),
                section.heading.as_ref().map(oc_model::doc::Heading::text),
            )
        })
        .collect();
    assert_eq!(
        roles,
        vec![
            (
                "FrontMatter(Preface)".to_owned(),
                Some("Preface".to_owned())
            ),
            (
                "FrontMatter(TableOfContents)".to_owned(),
                Some("Contents".to_owned())
            ),
            ("Chapter".to_owned(), Some("Chapter One".to_owned())),
            ("Chapter".to_owned(), Some("Chapter Two".to_owned())),
            (
                "BackMatter(Appendix)".to_owned(),
                Some("Appendix A".to_owned())
            ),
            ("BackMatter(Index)".to_owned(), Some("Index".to_owned())),
        ]
    );

    // Front ≺ body ≺ back, and the section inside a chapter is a child rather than a sibling.
    let zones: Vec<oc_model::doc::Zone> = stage
        .output
        .sections
        .iter()
        .map(|section| section.role.zone())
        .collect();
    assert!(zones.windows(2).all(|pair| pair[0] <= pair[1]), "{zones:?}");
    assert_eq!(stage.output.sections[2].children.len(), 1);
    assert_eq!(
        stage.output.sections[2].children[0]
            .heading
            .as_ref()
            .map(oc_model::doc::Heading::text),
        Some("A Section Within".to_owned())
    );

    // And the pages run forwards.
    let starts: Vec<u32> = stage
        .output
        .sections
        .iter()
        .map(|section| section.source_pages.0)
        .collect();
    assert!(
        starts.windows(2).all(|pair| pair[0] <= pair[1]),
        "{starts:?}"
    );
    assert!(stage
        .output
        .warnings
        .iter()
        .all(|warning| warning.code != oc_structure::book::W_ZONES_OUT_OF_ORDER));
}

/// `--dump-stage structure` produces what the CLI spec promises: a header about the book,
/// then one line per top-level section.
///
/// The snapshot is of the *shape* rather than of the whole dump, for the reason Phase 3's
/// digest split was made: a dump carries geometry, and geometry from a substituted face
/// differs between hosts (`docs/DECISIONS_LOG.md`, 2026-09-13).
#[test]
fn dump_stage_structure_header_f09() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/fixtures/f09_novel_structure.pdf");
    let bytes = std::fs::read(&path).expect("f09 is built");
    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend.open(&bytes, None).expect("the fixture opens");

    let (header, sections) = openconvert::dump_structure::dump(
        document.as_ref(),
        "f09_novel_structure.pdf",
        "0f0f0f",
        LangTag::EN,
        &T,
    )
    .expect("the dump runs");

    assert_eq!(header.schema, "openconvert.dump.structure/1");
    assert_eq!(header.stage, "structure");
    assert_eq!(header.check.removed_chars, 0);
    assert_eq!(header.check.added_chars, 0);
    assert!(header.escalation_allowed, "f09's inventory is valid");
    assert_eq!(header.note_match_rate, 1.0, "no notes is a total bijection");

    insta::assert_snapshot!(format!(
        "sections={} headings={} paragraphs={} clusters={} identifier_prefix={} roles={:?}",
        sections.len(),
        header.digest.headings.len(),
        header.digest.content.get("paragraph").copied().unwrap_or(0),
        header.digest.style_clusters,
        &header.metadata.identifier[..9],
        sections
            .iter()
            .map(|section| format!("{:?}", section.role))
            .collect::<Vec<_>>(),
    ));
}

/// `Para` carries both its text and its spans, and the conservation check reads one of them.
/// A span list that said anything other than the paragraph's own text would make I-3 pass on a
/// document that does not exist, so the two have to be the same characters — on every fixture,
/// not on a constructed example.
#[test]
fn a_paragraphs_spans_are_its_text_split() {
    for stem in [
        "f01_prose_single_column",
        "f02_two_column",
        "f07_verse_and_quote",
        "f08_footnotes",
        "f09_novel_structure",
        "f10_lists_and_table",
    ] {
        let read = read(&format!("../../target/fixtures/{stem}.pdf"));
        let stage = read.structure(&format!("{stem}.pdf"));
        for section in stage
            .output
            .sections
            .iter()
            .flat_map(oc_model::doc::Section::walk)
        {
            for content in &section.content {
                if let oc_model::doc::Content::Paragraph(para) = content {
                    assert_eq!(
                        oc_model::doc::spans_text(&para.spans),
                        para.text,
                        "{stem}: a paragraph's spans are not its text"
                    );
                }
            }
        }
    }
}

/// `NoteRef` names the *run* that printed the marker, not the block, and this is why: exactly
/// that run becomes a `Span` carrying the note's id, so `epub` can turn exactly that run into
/// `<a epub:type="noteref">`. Without it the emitted book has footnote bodies and nothing
/// pointing at them — the bijection failure EPUBCheck reports as `RSC-007`.
#[test]
fn a_note_marker_becomes_a_span_that_carries_the_note_it_refers_to() {
    let read = read("../../target/fixtures/f08_footnotes.pdf");
    let stage = read.structure("f08_footnotes.pdf");

    let referenced: Vec<oc_model::ids::NoteId> = stage
        .output
        .sections
        .iter()
        .flat_map(oc_model::doc::Section::walk)
        .flat_map(|section| section.content.clone())
        .filter_map(|content| match content {
            oc_model::doc::Content::Paragraph(para) => Some(para.spans),
            _ => None,
        })
        .flatten()
        .filter_map(|span| span.noteref)
        .collect();

    assert!(
        !referenced.is_empty(),
        "f08 prints footnote markers in its body"
    );
    let linked: Vec<oc_model::ids::NoteId> = stage
        .output
        .note_refs
        .iter()
        .map(|entry| entry.note)
        .collect();
    assert_eq!(
        referenced, linked,
        "every linked marker reaches the spans, in document order"
    );
}

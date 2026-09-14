//! Row 3.14: permuting a document's pages permutes its per-page text and nothing else
//! (R9 §B.4).
//!
//! A metamorphic test, and the reason for it is that `layout` is the first stage with a
//! *document-level* decision in it. Blocks, columns and reading order are page-local, but the
//! column hypothesis is checked across page boundaries and withdrawn document-wide, so a bug
//! there would make a page's segmentation depend on the pages around it. That is exactly the
//! class of defect a single-page fixture cannot show and a permutation can: if page 3 is
//! segmented differently when it is page 1, something is reading across a boundary it should
//! not be.
//!
//! What is asserted is the invariant and not more than it. Reading order *within* a page must
//! be identical under permutation; the continuity rate across the document may legitimately
//! differ, because shuffling the pages of a book genuinely does break the sentences that ran
//! between them.

use oc_core::ledger_check::ReasonTotals;
use oc_core::thresholds::T;
use oc_model::extract::PageRef;
use oc_model::lang::LangTag;
use oc_pdf::inspect::PdfOpen;
use oc_pdf::pdfium::PdfiumBackend;
use openconvert::pipeline::{
    block_text, furniture_stage, layout_stage, text_stage, LayoutStage, PageInput,
};
use proptest::prelude::*;

fn read(relative: &str) -> Vec<PageInput> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("missing fixture {}: {error}", path.display()));
    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend.open(&bytes, None).expect("the fixture opens");
    openconvert::input::page_inputs(document.as_ref()).expect("every page extracts")
}

fn lay_out(input: &[PageInput]) -> LayoutStage {
    let mut totals = ReasonTotals::default();
    let text = text_stage(input, &mut totals, &T).expect("text conserves");
    let furniture =
        furniture_stage(&text, LangTag::EN, &mut totals, &T).expect("furniture stays in budget");
    layout_stage(&text, &furniture, &mut totals, &T).expect("layout conserves")
}

/// A page's blocks, in reading order, as text.
fn page_text(layout: &LayoutStage, index: usize) -> Vec<String> {
    layout.blocks[index]
        .iter()
        .map(|block| block_text(block, &layout.pages[index]))
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]

    /// Row 3.14. Permute the pages; every page's own reading order comes back unchanged.
    #[test]
    fn prop_page_permutation_metamorphic(
        fixture in proptest::sample::select(vec![
            // Five pages whose column hypothesis is withdrawn document-wide, two pages of
            // ordinary prose, and two pages of two columns with a floating title: three
            // different document-level decisions to disturb.
            "../../corpus/fixtures/handmade/h22_false_gutter.pdf",
            "../../target/fixtures/f01_prose_single_column.pdf",
            "../../target/fixtures/f02_two_column.pdf",
        ]),
        rotation in 0usize..4,
        reverse in proptest::bool::ANY,
    ) {
        let original = read(fixture);
        let baseline = lay_out(&original);

        let mut order: Vec<usize> = (0..original.len()).collect();
        if reverse {
            order.reverse();
        }
        let count = order.len();
        order.rotate_left(rotation % count.max(1));

        // The permuted document is renumbered, as a document of those pages in that order
        // would be: the page index is part of a block's identity (D13.3), so permuting
        // without renumbering would be testing something else.
        let permuted: Vec<PageInput> = order
            .iter()
            .enumerate()
            .map(|(position, source)| PageInput {
                page: PageRef::new(u32::try_from(position).unwrap_or(u32::MAX)),
                ..original[*source].clone()
            })
            .collect();
        let shuffled = lay_out(&permuted);

        for (position, source) in order.iter().enumerate() {
            prop_assert_eq!(
                page_text(&shuffled, position),
                page_text(&baseline, *source),
                "page {} read differently as page {}",
                source,
                position
            );
        }
    }
}

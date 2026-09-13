//! Word and line assembly over real PDFs — rows 2.5, 2.8 and 2.9 of the Phase 2 table.
//!
//! These live in the binary crate because they are the first tests that need both ends of
//! the pipeline: `oc-pdf` to extract the glyphs and `oc-text` to assemble them. The two
//! crates may not depend on each other (ARCHITECTURE §3.1), and `openconvert` is where they
//! meet, so it is where the seam is tested.
//!
//! The assertions are about geometry the fixtures actually contain, not about strings chosen
//! to pass: h16 and h18 place a 7 pt superscript 3 pt above a 12 pt body because that is what
//! a layout engine produces, and h17 letter-spaces with `Tc`, the operator a designer's tool
//! writes.

use oc_core::thresholds::T;
use oc_model::extract::PageRef;
use oc_pdf::inspect::PdfOpen;
use oc_pdf::pdfium::PdfiumBackend;
use oc_testkit::handmade::SUPERSCRIPT_ONE;
use oc_text::words::{assemble_runs, RunAssembly};

fn fixture(name: &str) -> Vec<u8> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/fixtures/handmade")
        .join(format!("{name}.pdf"));
    std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "missing fixture {}: {error}; run `cargo run -p xtask -- handmade-fixtures`",
            path.display()
        )
    })
}

/// Extract page zero of a hand-made fixture and assemble it.
fn assemble(name: &str) -> RunAssembly {
    let bytes = fixture(name);
    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend.open(&bytes, None).expect("the fixture opens");
    let page = document.page_glyphs(0).expect("the page extracts");
    assemble_runs(&page.glyphs, PageRef::new(0), &T)
}

#[test]
fn superscript_flag_survives_normalization() {
    let assembly = assemble("h16_superscript_marker");

    // The raised 7 pt `1` is a superscript, read from geometry before `N` ran.
    let marker = assembly
        .runs
        .iter()
        .find(|run| run.text.trim() == "1")
        .expect("the raised marker is its own run");
    assert!(marker.superscript, "{marker:?}");
    assert!(!marker.subscript);
    assert!((marker.size_pt - 7.0).abs() < 0.01, "{}", marker.size_pt);

    // The body text is not superscript, and `N` left its case and characters alone.
    let body = assembly
        .runs
        .iter()
        .find(|run| run.text.starts_with("Text"))
        .expect("the body run");
    assert!(!body.superscript);

    // And U+00B9 — the typographic spelling of the same marker — is still U+00B9. NFKC
    // would have folded it to `1` and taken the footnote signal with it (R2 §B.8).
    let all: String = assembly.runs.iter().map(|run| run.text.as_str()).collect();
    assert!(
        all.contains(SUPERSCRIPT_ONE),
        "U+00B9 did not survive: {all:?}"
    );
}

#[test]
fn words_split_on_bimodal_gap() {
    // `Hallo` set with `Tc 1.2`: five glyphs, four identical gaps, no bimodality to find.
    // A splitter that thresholds on "wider than the narrowest gap" reads `H a l l o`.
    let assembly = assemble("h17_letterspaced");
    let text: String = assembly.runs.iter().map(|run| run.text.as_str()).collect();
    assert_eq!(text, "Hallo");
    assert_eq!(assembly.runs.len(), 1, "{:?}", assembly.runs);
}

#[test]
fn lines_cluster_by_baseline_tolerance() {
    let assembly = assemble("h18_mixed_sizes");
    let lines = oc_text::lines::assemble_lines(&assembly.runs, &T);
    assert_eq!(lines.len(), 2, "{lines:?}");

    let text_of = |line: &oc_model::text::Line| -> String {
        line.runs
            .iter()
            .map(|id| {
                assembly.runs[usize::try_from(id.0).expect("run index")]
                    .text
                    .as_str()
            })
            .collect()
    };

    assert_eq!(text_of(&lines[0]), "Chapter");
    // The 7 pt superscript sits 3 pt above the 12 pt body baseline. It joins the body line:
    // the tolerance is a fraction of size, and the fraction is taken of the larger of the
    // two — 0.3 × 12 = 3.6, not 0.3 × 7 = 2.1 (PIPELINE §4 step 1).
    assert_eq!(text_of(&lines[1]), "Body 2");
}

#[test]
fn a_space_is_inserted_where_the_document_only_left_a_gap() {
    // h16 draws three text objects with no space glyph anywhere. The gaps between them are
    // wide and the gaps inside `Text` are zero, which is the bimodal distribution the
    // 2-means is for.
    let assembly = assemble("h16_superscript_marker");
    let text: String = assembly.runs.iter().map(|run| run.text.as_str()).collect();
    assert_eq!(text, format!("Text 1 n{SUPERSCRIPT_ONE}"));
}

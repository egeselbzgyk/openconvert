//! Rows 13.5–13.8: Tesseract's TSV, read by schema and mapped into normalised page space.
//!
//! The parser is a pure function over bytes, so none of these tests needs a Tesseract.

use oc_core::ocr::tsv::{parse_tsv, pt_to_px, px_to_pt, TsvError, HEADER};
use oc_model::geom::Rect;
use proptest::prelude::*;

/// A real capture: Tesseract 5.3.4, `eng`, `--psm 6`, `--dpi 150`, over a crop of
/// `corpus/fixtures/assets/scan_page_01.png` taken at (90, 90). Truncated after the fourth word of
/// the second line; every row above that point is exactly as Tesseract printed it.
const GOLDEN: &str = include_str!("data/ocr/tesseract_5_3_4_eng_psm6.tsv");

const ORIGIN: Rect = Rect {
    x0: 0.0,
    y0: 0.0,
    x1: 1_000.0,
    y1: 1_000.0,
};

fn row(level: u32, conf: &str, text: &str) -> String {
    format!("{level}\t1\t1\t1\t1\t1\t10\t20\t30\t40\t{conf}\t{text}")
}

/// Row 13.5. A header that is not exactly the twelve columns, in order, is a schema error — never a
/// parse that indexes the columns by position and reads `height` as `conf`.
#[test]
fn tsv_header_mismatch_is_an_error() {
    // Ten columns: `conf` and `text` are gone.
    let ten = HEADER[..10].join("\t");
    let body = format!("{ten}\n5\t1\t1\t1\t1\t1\t10\t20\t30\t40\n");
    let error = parse_tsv(body.as_bytes(), 300, ORIGIN).expect_err("ten columns");
    assert!(
        matches!(error, TsvError::UnexpectedSchema { .. }),
        "{error:?}"
    );

    // Twelve columns, but `left` and `top` swapped: a positional parse would transpose every box.
    let mut swapped: Vec<&str> = HEADER.to_vec();
    swapped.swap(6, 7);
    let body = format!("{}\n{}\n", swapped.join("\t"), row(5, "90", "word"));
    let error = parse_tsv(body.as_bytes(), 300, ORIGIN).expect_err("reordered");
    assert!(
        matches!(error, TsvError::UnexpectedSchema { .. }),
        "{error:?}"
    );

    // A thirteenth column is drift too.
    let body = format!("{}\textra\n", HEADER.join("\t"));
    assert!(matches!(
        parse_tsv(body.as_bytes(), 300, ORIGIN),
        Err(TsvError::UnexpectedSchema { .. })
    ));

    // And nothing at all is not an empty page: Tesseract always prints the header.
    assert!(matches!(
        parse_tsv(b"", 300, ORIGIN),
        Err(TsvError::UnexpectedSchema { .. })
    ));

    // The message names what was found, so a version drift is diagnosable from the report.
    let error = parse_tsv(b"level\tpage\n", 300, ORIGIN).expect_err("drift");
    assert!(error.to_string().contains("level\tpage"), "{error}");
}

/// Row 13.6. The committed capture yields exactly its seven words, in order, with confidences scaled
/// into `0.0..=1.0` and boxes in points.
#[test]
fn tsv_parses_words_and_confidences() {
    // 150 dpi, so one pixel is 72/150 = 0.48 pt; the region starts at (100, 200) pt.
    let region = Rect {
        x0: 100.0,
        y0: 200.0,
        x1: 608.8,
        y1: 281.6,
    };
    let words = parse_tsv(GOLDEN.as_bytes(), 150, region).expect("the capture parses");

    let texts: Vec<&str> = words.iter().map(|word| word.text.as_str()).collect();
    assert_eq!(texts, ["A", "Scanned", "Chapter", "It", "was", "a", "dark"]);

    for word in &words {
        assert!((0.0..=1.0).contains(&word.conf), "{word:?}");
    }
    let first = &words[0];
    assert!((first.conf - 0.956_996_2).abs() < 1e-6, "{first:?}");
    // left 16, top 31, width 22, height 25 px.
    let expect = |value: f32, want: f32| assert!((value - want).abs() < 1e-3, "{value} vs {want}");
    expect(first.bbox.x0, 100.0 + 16.0 * 0.48);
    expect(first.bbox.y0, 200.0 + 31.0 * 0.48);
    expect(first.bbox.x1, 100.0 + 38.0 * 0.48);
    expect(first.bbox.y1, 200.0 + 56.0 * 0.48);

    // Tesseract's segmentation survives: two lines, in two paragraphs of one block.
    assert_eq!(
        words
            .iter()
            .map(|word| (word.block, word.par, word.line))
            .collect::<Vec<_>>(),
        [
            (1, 1, 1),
            (1, 1, 1),
            (1, 1, 1),
            (1, 2, 1),
            (1, 2, 1),
            (1, 2, 1),
            (1, 2, 1)
        ]
    );
}

/// Row 13.7. Page, block, paragraph and line rows are structure, not words; a word Tesseract gave
/// no confidence for is not a word either.
#[test]
fn tsv_drops_non_word_and_negative_conf_rows() {
    let body = [
        HEADER.join("\t"),
        row(1, "-1", ""),
        row(2, "-1", ""),
        row(3, "-1", ""),
        row(4, "-1", "line"),
        row(5, "-1", "unscored"),
        row(5, "91.5", "kept"),
        row(5, "88", "   "),
        row(5, "0", "zero"),
    ]
    .join("\n");

    let words = parse_tsv(body.as_bytes(), 300, ORIGIN).expect("parses");
    let texts: Vec<&str> = words.iter().map(|word| word.text.as_str()).collect();
    assert_eq!(
        texts,
        ["kept", "zero"],
        "level != 5, conf == -1 and blank rows never become runs"
    );
    assert!((words[0].conf - 0.915).abs() < 1e-6);
    assert_eq!(words[1].conf, 0.0, "zero confidence is a confidence");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    /// Row 13.8. For random regions and resolutions, pixel → point → pixel round-trips to within
    /// 0.01 pt, and a pixel box lands inside the region it was rendered from. One wrong factor
    /// here puts every scanned page's text in the wrong reading order and nothing downstream
    /// notices, which is why this is a property and not an example.
    #[test]
    fn pixel_boxes_map_into_normalized_page_space(
        x0 in 0.0f32..1_200.0,
        y0 in 0.0f32..1_800.0,
        width in 1.0f32..1_200.0,
        height in 1.0f32..1_800.0,
        dpi in 50u32..=1_200,
        fx in 0.0f32..=1.0,
        fy in 0.0f32..=1.0,
    ) {
        let region = Rect { x0, y0, x1: x0 + width, y1: y0 + height };
        let scale = dpi as f32 / 72.0;
        let px = (fx * width * scale, fy * height * scale);

        let pt = px_to_pt(px, dpi, region);
        prop_assert!(pt.0 >= region.x0 - 0.01 && pt.0 <= region.x1 + 0.01, "{pt:?} {region:?}");
        prop_assert!(pt.1 >= region.y0 - 0.01 && pt.1 <= region.y1 + 0.01, "{pt:?} {region:?}");

        let back = pt_to_px(pt, dpi, region);
        // Compared in points: the tolerance is stated in the page's units, not the raster's.
        prop_assert!(((back.0 - px.0) / scale).abs() <= 0.01, "{px:?} -> {pt:?} -> {back:?}");
        prop_assert!(((back.1 - px.1) / scale).abs() <= 0.01, "{px:?} -> {pt:?} -> {back:?}");

        // And through the parser: a one-word TSV whose box is the whole raster maps to the region.
        let w = (width * scale).round().max(1.0);
        let h = (height * scale).round().max(1.0);
        let body = format!("{}\n5\t1\t1\t1\t1\t1\t0\t0\t{w}\t{h}\t90\tword\n", HEADER.join("\t"));
        let words = parse_tsv(body.as_bytes(), dpi, region).expect("parses");
        let bbox = words[0].bbox;
        let slack = 0.5 / scale + 0.01;
        prop_assert!((bbox.x0 - region.x0).abs() <= 0.01);
        prop_assert!((bbox.y0 - region.y0).abs() <= 0.01);
        prop_assert!((bbox.x1 - region.x1).abs() <= slack, "{bbox:?} {region:?}");
        prop_assert!((bbox.y1 - region.y1).abs() <= slack, "{bbox:?} {region:?}");
    }
}

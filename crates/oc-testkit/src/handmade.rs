//! Hand-made PDFs, each one built to isolate exactly one extraction behaviour.
//!
//! Typst fixtures are realistic but broad: an `f01` exercises fonts, layout, headers and
//! paragraphs at once, so a failure in one of them is a failure in all of them. These are
//! the opposite — two glyphs, or one invisible layer, or one glyph drawn twice — so that a
//! red test names its own cause.
//!
//! They live here rather than as loose `.rs` files under `corpus/fixtures/handmade/` (which
//! is where the plan's §0.6 table puts them) because a file outside a crate compiles
//! nowhere. `oc-testkit` is the crate Appendix A gives fixture builders to; the *committed
//! PDFs* still land in `corpus/fixtures/handmade/`, written by
//! `cargo run -p xtask -- handmade-fixtures`.
//!
//! Every builder writes the same bytes on every run: no timestamps, no document id, no
//! randomness. That is what lets a fixture be committed and compared.

use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str, TextStr};

/// Helvetica, one of the fourteen fonts every reader has, so no font has to be embedded and
/// the file stays small enough to read in a diff.
const BASE_FONT: &str = "Helvetica";

/// The size every hand-made fixture draws at. Test 1.1 asserts it comes back exactly.
pub const FIXTURE_FONT_SIZE_PT: f32 = 12.0;

/// Where the first glyph's baseline sits, in PDF user space.
pub const FIXTURE_ORIGIN: (f32, f32) = (72.0, 700.0);

/// The page box every hand-made fixture uses unless it is testing the box itself.
const PAGE: [f32; 4] = [0.0, 0.0, 200.0, 800.0];

/// PDF text render mode 3: drawn, but neither filled nor stroked — invisible.
const RENDER_MODE_INVISIBLE: i32 = 3;

/// The grey a scan's paper is, and the size of the stand-in image. Eight pixels is enough:
/// what matters is that an image covers the page, never what is in it.
const GREY_LEVEL: u8 = 235;

/// Eight-bit samples, for both the image and its mask.
const BITS_PER_COMPONENT: i32 = 8;

/// The side h11 claims. Squared it is 1.6 gigapixels, sixteen times the shipped allowance.
const BOMB_IMAGE_SIDE_PX: i32 = 40_000;

/// How much h12's content stream expands to. Small enough to build in a moment, large
/// enough that no reasonable cap lets it through unnoticed.
const BOMB_CONTENT_BYTES: usize = 8 * 1024 * 1024;
const IMAGE_SIDE_PX: usize = 8;

/// The font name Tesseract gives the invisible layer it writes over a scan (R3 §4). A
/// sandwich detector keys on it, so a fixture pretending to be one has to carry it.
const OCR_FONT: &str = "GlyphLessFont";

/// Every hand-made fixture, as `(name, bytes)`.
///
/// One list, so `xtask handmade-fixtures` and the tests cannot disagree about which fixtures
/// exist.
pub fn all() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("h01_two_glyphs", h01_two_glyphs()),
        ("h02_rotate_90", h02_rotate_90()),
        ("h03_cropbox_offset", h03_cropbox_offset()),
        ("h04_ligature_fi", h04_ligature_fi()),
        ("h05_invisible_layer", h05_invisible_layer()),
        ("h06_generated_space", h06_generated_space()),
        ("h07_overdraw_duplicate", h07_overdraw_duplicate()),
        ("h08_overlap_distinct", h08_overlap_distinct()),
        ("h09_image_smask", h09_image_smask()),
        ("h10_inline_image", h10_inline_image()),
        ("h11_pixel_bomb", h11_pixel_bomb()),
        ("h12_decompression_bomb", h12_decompression_bomb()),
        ("h13_outline", h13_outline()),
        ("h14_stencil_mask", h14_stencil_mask()),
        ("h15_indexed_colour", h15_indexed_colour()),
        ("h16_superscript_marker", h16_superscript_marker()),
        ("h17_letterspaced", h17_letterspaced()),
        ("h18_mixed_sizes", h18_mixed_sizes()),
        ("h19_constant_band_number", h19_constant_band_number()),
        ("h20_recto_verso", h20_recto_verso()),
        ("h21_band_is_sole_content", h21_band_is_sole_content()),
        ("h22_false_gutter", h22_false_gutter()),
        ("h23_paragraph_across_pages", h23_paragraph_across_pages()),
        ("h24_footnote_symbol_cycle", h24_footnote_symbol_cycle()),
        ("h25_two_figures_one_caption", h25_two_figures_one_caption()),
        ("h26_borderless_table", h26_borderless_table()),
    ]
}

/// h01 — two glyphs, nothing else. The baseline every glyph signal is asserted against.
pub fn h01_two_glyphs() -> Vec<u8> {
    build(Page::default().text(FIXTURE_ORIGIN, "AB"))
}

/// h02 — h01 with `/Rotate 90`. The extracted characters must not change (test 1.5).
pub fn h02_rotate_90() -> Vec<u8> {
    build(Page::default().text(FIXTURE_ORIGIN, "AB").rotate(90))
}

/// h03 — h01 with the CropBox shifted off the MediaBox origin. Every rect must still land
/// inside the page, and no text may go missing (test 1.6, R1 §D.6 #1).
pub fn h03_cropbox_offset() -> Vec<u8> {
    build(
        Page::default()
            .text(FIXTURE_ORIGIN, "AB")
            .crop([50.0, 50.0, 200.0, 800.0]),
    )
}

/// The byte h04 shows for the `fi` ligature, and the glyph name it is mapped to.
///
/// WinAnsi has no code for U+FB01, so the fixture cannot simply write it: a byte encoding can
/// only say what its table says. The document declares `/Differences [200 /fi]` over WinAnsi,
/// shows byte 200, and carries a `/ToUnicode` CMap saying that byte 200 is U+FB01 — which is
/// how a real typesetter ships a ligature, and the only way this fixture is a ligature fixture
/// at all. The CMap is not optional: with the glyph name alone PDFium resolves `fi` through
/// the Adobe glyph list and hands back two characters, expanding the ligature itself and
/// leaving `N` nothing to do.
///
/// h04 was not a ligature fixture until Phase 2 item 2.7 looked. `to_winansi` mapped every
/// unrepresentable character to `?`, so the page drew `?n`, and every test that carried it
/// through unchanged passed by carrying through a question mark. See `docs/DECISIONS_LOG.md`.
pub const LIGATURE_CODE: u8 = 200;
pub const LIGATURE_GLYPH: &str = "fi";

/// The `/ToUnicode` CMap h04 carries: byte 200 is U+FB01, and nothing else is claimed.
const LIGATURE_TO_UNICODE: &[u8] = br"/CIDInit /ProcSet findresource begin
12 dict begin
begincmap
/CMapName /OpenConvert-Ligature def
/CMapType 2 def
1 begincodespacerange
<00> <FF>
endcodespacerange
1 beginbfchar
<C8> <FB01>
endbfchar
endcmap
CMapName currentdict /CMap defineresource pop
end
end";

/// h04 — the `fi` ligature as a single code point, which normalisation `N` expands in
/// Phase 2. Phase 1 only has to carry it through unchanged.
pub fn h04_ligature_fi() -> Vec<u8> {
    build(
        Page::default()
            .text(FIXTURE_ORIGIN, "\u{FB01}n")
            .ligature_encoding(),
    )
}

/// h05 — an OCR sandwich: a full-page image with an invisible text layer over it, drawn in
/// a font named the way Tesseract names its own (test 1.3).
///
/// The image and the font name are both load-bearing. D13.10 recognises a sandwich from
/// three signals together - invisible text, a glyph-less font, and an image covering the
/// page - so a fixture with only the invisible text would classify as `blank` and test 1.3
/// would be asserting against a rule that never fired.
pub fn h05_invisible_layer() -> Vec<u8> {
    build(
        Page::default()
            .full_page_image()
            .font_name(OCR_FONT)
            // Twenty-six 12 pt characters need ~174 pt; starting at the fixture origin
            // would run them off a 200 pt page and make this a clipping test by accident.
            .text((10.0, FIXTURE_ORIGIN.1), "abcdefghijklmnopqrstuvwxyz")
            .render_mode(RENDER_MODE_INVISIBLE),
    )
}

/// h06 — two words placed with a gap and no space glyph between them, so PDFium synthesises
/// one. The synthesised space must be dropped and ledgered (test 1.2).
pub fn h06_generated_space() -> Vec<u8> {
    build(
        Page::default()
            .text(FIXTURE_ORIGIN, "one")
            .text((FIXTURE_ORIGIN.0 + 60.0, FIXTURE_ORIGIN.1), "two"),
    )
}

/// h07 — the same glyph drawn twice a fifth of a point apart, which is how a producer fakes
/// bold (R1 §D.6 #2). One glyph must survive, with one ledger entry (test 1.4).
pub fn h07_overdraw_duplicate() -> Vec<u8> {
    build(
        Page::default()
            .text(FIXTURE_ORIGIN, "A")
            .text((FIXTURE_ORIGIN.0 + 0.2, FIXTURE_ORIGIN.1), "A"),
    )
}

/// h08 — two *different* glyphs overlapping. Both must survive: PDFium merges duplicates,
/// never distinct characters, and that is what makes its text page safe to build `C_raw` on.
pub fn h08_overlap_distinct() -> Vec<u8> {
    overlap_pair_at(0.2, "A", "B")
}

/// The size a superscript is set at in h16 and h18, and how far its baseline is raised.
///
/// 7 pt over a 12 pt body and a 3 pt rise is what a layout engine actually produces — Typst's
/// `super` shrinks to about 0.6 em and lifts by about 0.25 em. Both Phase 2 rules then fire on
/// a real geometry rather than on one chosen to satisfy them: it is a superscript because
/// 3.0 >= 0.30 x 7 and 7 < 0.80 x 12, and it stays on its parent line because
/// 3.0 <= 0.30 x 12.
pub const SUPERSCRIPT_SIZE_PT: f32 = 7.0;
pub const SUPERSCRIPT_RISE_PT: f32 = 3.0;

/// The typographic superscript h16 also carries: U+00B9, drawn at body size on the baseline.
///
/// It is the character NFKC would fold to `1`, taking the footnote marker with it, which is
/// why NFKC is banned pipeline-wide (R2 §B.8).
pub const SUPERSCRIPT_ONE: char = '\u{00B9}';

/// h16 - a footnote marker in both of its spellings: raised small type, and U+00B9 on the
/// baseline.
///
/// Test 2.5 asks that the geometric flag is captured before `N` and survives it, and the
/// U+00B9 asks that `N` did not quietly become NFKC on the way.
pub fn h16_superscript_marker() -> Vec<u8> {
    build(
        Page::default()
            .text(FIXTURE_ORIGIN, "Text")
            .text_at(
                (
                    FIXTURE_ORIGIN.0 + 30.0,
                    FIXTURE_ORIGIN.1 + SUPERSCRIPT_RISE_PT,
                ),
                "1",
                SUPERSCRIPT_SIZE_PT,
            )
            .text((FIXTURE_ORIGIN.0 + 40.0, FIXTURE_ORIGIN.1), "n\u{00B9}"),
    )
}

/// The letter-spaced word h17 draws, and the extra advance it puts between the letters.
///
/// 1.2 pt is below Helvetica's 3.34 pt space at 12 pt and, crucially, *uniform*: the gap
/// distribution is unimodal, the 2-means separation collapses, and the space threshold falls
/// back to the font metric. A detector that thresholds on "a gap bigger than the smallest
/// gap" reads this as `H a l l o`.
pub const LETTERSPACED_WORD: &str = "Hallo";
pub const LETTERSPACING_PT: f32 = 1.2;

/// h17 - display text set with `Tc 1.2`, which is how a designer letter-spaces a word and
/// how a naive word-splitter turns one word into five (test 2.8, R10 §6.2).
pub fn h17_letterspaced() -> Vec<u8> {
    build(
        Page::default()
            .text(FIXTURE_ORIGIN, LETTERSPACED_WORD)
            .char_spacing(LETTERSPACING_PT),
    )
}

/// The size h18's heading is set at, and how far above the body its baseline sits.
pub const HEADING_SIZE_PT: f32 = 18.0;
pub const HEADING_RISE_PT: f32 = 30.0;

/// h18 - three sizes on two lines: an 18 pt heading, a 12 pt body line, and a 7 pt
/// superscript raised 3 pt inside that body line.
///
/// The clustering has to put the superscript on the body line and the heading on its own,
/// which a fixed baseline tolerance cannot do: 3 pt apart is one line and 30 pt apart is two,
/// and both distances are read from the same page (test 2.9).
pub fn h18_mixed_sizes() -> Vec<u8> {
    build(
        Page::default()
            .text_at(
                (FIXTURE_ORIGIN.0, FIXTURE_ORIGIN.1 + HEADING_RISE_PT),
                "Chapter",
                HEADING_SIZE_PT,
            )
            .text(FIXTURE_ORIGIN, "Body")
            .text_at(
                (
                    FIXTURE_ORIGIN.0 + 30.0,
                    FIXTURE_ORIGIN.1 + SUPERSCRIPT_RISE_PT,
                ),
                "2",
                SUPERSCRIPT_SIZE_PT,
            ),
    )
}

/// Where a running head sits on the 200 x 800 pt fixture page, in PDF user space.
///
/// `layout.furniture.band_ratio` is 0.08, so the bands are the outer 64 pt. A head at PDF
/// y = 760 lands 40 pt from the top and a foot at y = 30 lands 770 pt down, both comfortably
/// inside — comfortably, and not on the boundary, because a fixture that only passes at the
/// exact threshold tests the threshold rather than the rule.
pub const BAND_HEADER_Y: f32 = 760.0;
pub const BAND_FOOTER_Y: f32 = 30.0;
/// Where the fixtures put body text: the middle of the page, far from either band.
pub const BAND_BODY_Y: f32 = 400.0;
/// Running heads and page numbers are set smaller than body text, as they are in books.
pub const FURNITURE_SIZE_PT: f32 = 8.0;

/// The constant number h19 prints in the footer band of every page.
pub const CONSTANT_BAND_NUMBER: &str = "3";

/// h19 - four pages whose footer band carries the same number, `3`, on every one.
///
/// It repeats perfectly, it is entirely numeric, and it is **not** a page number: the values
/// form no arithmetic progression. That test is the cheap, reliable thing that separates a
/// page number from a chapter number (PIPELINE §5 step 6), and without it a chapter number is
/// deleted from every page of the chapter (test 2.12).
pub fn h19_constant_band_number() -> Vec<u8> {
    let pages: Vec<Page> = (0..4)
        .map(|index| {
            Page::default()
                .text_at(
                    (20.0, BAND_FOOTER_Y),
                    CONSTANT_BAND_NUMBER,
                    FURNITURE_SIZE_PT,
                )
                .text((20.0, BAND_BODY_Y), &format!("Body text page {index}"))
        })
        .collect();
    build_pages(pages)
}

/// The two running heads h20 alternates between, verso and recto, plus the one-off.
pub const VERSO_HEAD: &str = "The Book";
pub const RECTO_HEAD: &str = "Chapter One";
pub const ONE_OFF_HEAD: &str = "Errata";

/// h20 - six pages that alternate their running head by parity, plus one band line that
/// appears once.
///
/// Books put the book's title on one side and the chapter's on the other. A parity-blind
/// detector sees two patterns at half strength - 3 pages out of 6, a ratio of 0.5, which is
/// inside the grey zone where the rule abstains - and keeps both. Split by parity each is
/// 3 out of 3 and fires. The one-off has to survive either way (test 2.13).
pub fn h20_recto_verso() -> Vec<u8> {
    let pages: Vec<Page> = (0..6)
        .map(|index| {
            let head = if index % 2 == 0 {
                VERSO_HEAD
            } else {
                RECTO_HEAD
            };
            let page = Page::default()
                .text_at((20.0, BAND_HEADER_Y), head, FURNITURE_SIZE_PT)
                .text((20.0, BAND_BODY_Y), &format!("Body text page {index}"));
            if index == 3 {
                page.text_at((20.0, BAND_FOOTER_Y), ONE_OFF_HEAD, FURNITURE_SIZE_PT)
            } else {
                page
            }
        })
        .collect();
    build_pages(pages)
}

/// h21 - four pages sharing a running head, the last of which has nothing else on it.
///
/// Calibre's blunt rule deletes by position and eats the only line on an atypical page; Marker
/// deletes body text outright in the same class of bug (R1 §C.2 #7, §C.3). The head goes from
/// the three pages that have a body and stays on the page that does not (test 2.14).
pub fn h21_band_is_sole_content() -> Vec<u8> {
    let pages: Vec<Page> = (0..4)
        .map(|index| {
            let page =
                Page::default().text_at((20.0, BAND_HEADER_Y), VERSO_HEAD, FURNITURE_SIZE_PT);
            if index < 3 {
                page.text((20.0, BAND_BODY_Y), &format!("Body text page {index}"))
            } else {
                page
            }
        })
        .collect();
    build_pages(pages)
}

/// The characters test 1.7 draws along one baseline, in the order they read.
pub const LINE_ALPHABET: &str = "ABCDEFGH";

/// How many of them there are, as the permutation strategy needs it.
pub const LINE_GLYPH_COUNT: usize = LINE_ALPHABET.len();

/// The gap between their origins, in points.
///
/// Wider than a 12 pt capital, so no two glyphs touch and the reading-order sort has an
/// unambiguous answer; wide enough that PDFium synthesises a space between them, which is
/// the second thing this fixture is worth — those spaces are dropped at ingestion, so a
/// reorder test that counted them would be testing the wrong invariant.
const LINE_ADVANCE_PT: f32 = 14.0;

/// One line of glyphs, drawn in the order `order` gives.
///
/// `order` is a permutation of `0..LINE_GLYPH_COUNT`: each entry names the slot the next
/// drawing operator fills, so `[7, 6, .., 0]` writes the line back to front. The rendered
/// page is identical whichever permutation is used — position comes from the text matrix,
/// not from the operator's place in the stream — which is what makes this a metamorphic
/// fixture rather than eight different fixtures.
pub fn line_of_glyphs(order: &[usize]) -> Vec<u8> {
    let mut page = Page::default();
    for slot in order {
        let Some(ch) = LINE_ALPHABET.chars().nth(*slot) else {
            continue;
        };
        let x = FIXTURE_ORIGIN.0 + LINE_ADVANCE_PT * slot_offset(*slot);
        page = page.text((x, FIXTURE_ORIGIN.1), ch.encode_utf8(&mut [0u8; 4]));
    }
    build(page)
}

/// A slot index as a distance multiplier. Separate so the cast is in one place.
fn slot_offset(slot: usize) -> f32 {
    u16::try_from(slot).unwrap_or(u16::MAX).into()
}

/// h09 - a full-page image carrying a soft mask.
///
/// The only fixture whose `has_smask` is true, and therefore the only one that proves the
/// flag is read at all: every other image in the corpus is an unmasked XObject, so a detector
/// that always answered "no mask" would pass every test but this one. It is also the seed of
/// the VD-d spike, which has to compare masked compositing against a reference.
pub fn h09_image_smask() -> Vec<u8> {
    build(Page::default().full_page_image().image_smask())
}

/// h10 - an image written inline in the content stream (`BI ... ID ... EI`).
///
/// The counterpart to h09: the only fixture whose `is_inline` is true, and so the only one
/// that proves the flag is read rather than defaulted. Inline images are how producers write
/// the small ones - bullets, rules, logos - and they are invisible to a resource-dictionary
/// walk, because they are in no resource dictionary.
pub fn h10_inline_image() -> Vec<u8> {
    build(Page::default().inline_image())
}

/// h11 - an image that *claims* to be 40 000 x 40 000 pixels.
///
/// 1.6 gigapixels; believing it and allocating four bytes each would ask for 6.4 GB. The
/// stream behind the claim is sixty-four bytes, which is the shape of the attack: the file is
/// small, the promise is not. Test 1.10 requires this to be refused before anything decodes.
pub fn h11_pixel_bomb() -> Vec<u8> {
    build(
        Page::default()
            .full_page_image()
            .declared_image_size(BOMB_IMAGE_SIDE_PX),
    )
}

/// h12 - a page whose content stream decompresses to far more than it costs to store.
///
/// Eight mebibytes of whitespace, which deflate takes down to a few kilobytes: a ratio around
/// a thousand to one. Whitespace rather than random bytes because it is legal content-stream
/// syntax, so the file stays a valid PDF that a reader opens normally - the bomb is in what it
/// costs to read, not in being malformed.
pub fn h12_decompression_bomb() -> Vec<u8> {
    build(
        Page::default()
            .text(FIXTURE_ORIGIN, "AB")
            .content_padding(BOMB_CONTENT_BYTES),
    )
}

/// h14 - an image drawn as a *stencil mask* (`/ImageMask true`).
///
/// The other way a PDF makes part of an image transparent, and the one that predates
/// `/SMask`: one bit per pixel, where — per PDF 32000-1 §8.9.6.2, and this is the opposite of
/// what most people guess — a sample of **0 paints** with the current colour and a sample of
/// **1 leaves the page unchanged**. VD-d has to know whether PDFium composites this form as
/// well as the soft mask, because a scanned book's stamps and logos are usually stencils.
pub fn h14_stencil_mask() -> Vec<u8> {
    build(Page::default().full_page_image().stencil_mask())
}

/// h15 - an image in an Indexed colour space over DeviceRGB.
///
/// A palette rather than direct colour, which is how a PNG with few colours reaches a PDF.
/// The palette here is two entries - one red, one blue - so a decoder that ignores the index
/// and reads the sample as grey produces something obviously wrong rather than something
/// subtly off.
pub fn h15_indexed_colour() -> Vec<u8> {
    build(Page::default().full_page_image().indexed_palette())
}

/// The palette `h15` declares: entry 0 is red, entry 1 is blue.
pub const INDEXED_PALETTE: [[u8; 3]; 2] = [[255, 0, 0], [0, 0, 255]];

/// The page box `h22` needs: wide enough for two columns of twelve-point text.
const WIDE_PAGE: [f32; 4] = [0.0, 0.0, 400.0, 300.0];

/// Where `h22` sets its left and right halves, and the baselines it uses.
const FALSE_GUTTER_LEFT_X: f32 = 20.0;
const FALSE_GUTTER_RIGHT_X: f32 = 220.0;
const FALSE_GUTTER_TOP_Y: f32 = 260.0;
const FALSE_GUTTER_LEADING: f32 = 20.0;

/// How many pages `h22` has, and how many full lines each carries. Five pages give four
/// boundaries, which is enough for a *rate* to mean something; one boundary is a coin.
const FALSE_GUTTER_PAGES: usize = 5;
const FALSE_GUTTER_FULL_LINES: usize = 5;

/// h22 — five pages with a tall empty band down the middle that is not a gutter (test 3.5).
///
/// Every line is written in two halves with 75 pt of nothing between them, so the
/// x-projection has a valley as deep and as tall as a real gutter's, flanked by text on both
/// sides. Nothing about the page says which reading is right, and that is the point: the
/// evidence is not on the page, it is *between* pages.
///
/// Read as two columns, each page emits its left halves and then its right halves, so the
/// page ends on `… to a full stop.` and the next begins with a capital-free line that has
/// nothing to do with it — continuity breaks at every boundary. Read as one column, each page
/// ends on its last left-only line, which stops mid-sentence and runs straight into the next
/// page. The one-column reading is the one that makes the book read like a book, and the
/// re-run is what finds that out (R10 §6.5).
pub fn h22_false_gutter() -> Vec<u8> {
    let pages: Vec<Page> = (0..FALSE_GUTTER_PAGES)
        .map(|index| {
            let mut page = Page::default().media_box(WIDE_PAGE);
            for line in 0..FALSE_GUTTER_FULL_LINES {
                let y = FALSE_GUTTER_TOP_Y - line as f32 * FALSE_GUTTER_LEADING;
                page = page
                    .text(
                        (FALSE_GUTTER_LEFT_X, y),
                        &format!("page {index} line {line} of"),
                    )
                    .text(
                        (FALSE_GUTTER_RIGHT_X, y),
                        &format!("prose to a full stop {line}."),
                    );
            }
            // The last line of the page has no right half, so the two readings end the page
            // on different words. Without it both readings end on the same line and the
            // continuity proxy cannot tell them apart.
            let y = FALSE_GUTTER_TOP_Y - FALSE_GUTTER_FULL_LINES as f32 * FALSE_GUTTER_LEADING;
            page.text((FALSE_GUTTER_LEFT_X, y), "and the sentence goes on")
        })
        .collect();
    build_pages(pages)
}

/// h23 - a paragraph interrupted by a page break, with a word broken across the same break
/// (test 3.7).
///
/// Two failures in one fixture, because they happen together and a repair for either one
/// alone leaves the other visible. The paragraph has to survive the page boundary, and the
/// word `pipe-` / `line` has to be rejoined across it. The evidence for the join is on the
/// page above: the document uses the word `pipeline` in its first sentence, so the
/// in-document lexicon settles it without any dictionary being consulted (PIPELINE §7 tier
/// T3). That is deliberate - a fixture that needed the classifier to answer would be testing
/// the classifier rather than the merge.
///
/// It is also longer than it needs to be to make its point, and that is deliberate too. The
/// `Dehyphenate` budget is a *fraction* of the document, so on a hundred-character fixture a
/// single legitimate hyphen is nine parts in a thousand and breaches a five-in-a-thousand
/// allowance. Two hundred characters is the least a document can be and still have a hyphen
/// measured against a fraction at all.
pub fn h23_paragraph_across_pages() -> Vec<u8> {
    let first = Page::default()
        .media_box(WIDE_PAGE)
        .text((20.0, 260.0), "The pipeline runs north")
        .text((20.0, 240.0), "through the valley and the")
        .text((20.0, 220.0), "survey party followed it")
        .text((20.0, 200.0), "summer.")
        .text((20.0, 180.0), "A second paragraph now")
        .text((20.0, 160.0), "runs on for several lines")
        .text((20.0, 140.0), "and ends the page on a")
        .text((20.0, 120.0), "word broken as pipe-");
    let second = Page::default()
        .media_box(WIDE_PAGE)
        .text((20.0, 260.0), "line that the survey")
        .text((20.0, 240.0), "party had recorded in")
        .text((20.0, 220.0), "its notes that autumn")
        .text((20.0, 200.0), "and again the winter")
        .text((20.0, 180.0), "after.");
    build_pages(vec![first, second])
}

/// The outline `h13` carries, as `(title, level)` in the order it must be read.
///
/// Three levels and two roots, because a tree that is only one level deep cannot tell a
/// depth-first walk from a breadth-first one, and one root cannot tell a walk that returns to
/// the top from one that stops at the first leaf. Public so the test asserts against the same
/// list the builder wrote rather than a copy of it that could drift.
pub const OUTLINE_TREE: [(&str, u16); 6] = [
    ("Part One", 0),
    ("Chapter 1", 1),
    ("Section 1.1", 2),
    ("Section 1.2", 2),
    ("Chapter 2", 1),
    ("Part Two", 0),
];

/// h13 - a document whose only interesting feature is its outline (test 1.15).
///
/// `f01` has an outline too, but a single entry called "Chapter 3": enough to prove one is
/// read, not enough to prove it is read *depth-first with the right levels*, which is the
/// assertion. Both are checked; this is the one that can fail.
pub fn h13_outline() -> Vec<u8> {
    build(
        Page::default()
            .text(FIXTURE_ORIGIN, "AB")
            .outline(&OUTLINE_TREE),
    )
}

/// A document with `count` empty pages, for the `--max-pages` guard.
///
/// Not one of the committed fixtures: three thousand empty page dictionaries are a third of a
/// megabyte of nothing anyone would review, and the only thing the test needs from them is
/// that there are three thousand and one. Built where it is used instead.
pub fn many_pages(count: usize) -> Vec<u8> {
    let catalog = Ref::new(1);
    let tree = Ref::new(2);
    let first_page = 3;

    let page_ids: Vec<Ref> = (0..count)
        .map(|i| Ref::new(first_page + i32::try_from(i).unwrap_or(i32::MAX)))
        .collect();

    let mut pdf = Pdf::new();
    pdf.set_file_id((
        b"openconvert-fixture".to_vec(),
        b"openconvert-fixture".to_vec(),
    ));
    pdf.catalog(catalog).pages(tree);
    pdf.pages(tree)
        .kids(page_ids.iter().copied())
        .count(i32::try_from(count).unwrap_or(i32::MAX));
    for id in &page_ids {
        pdf.page(*id)
            .parent(tree)
            .media_box(Rect::new(PAGE[0], PAGE[1], PAGE[2], PAGE[3]))
            .finish();
    }
    pdf.finish()
}

/// The overdraw fixture at an arbitrary separation.
///
/// Kept public so the PDFium merge threshold recorded in `docs/DECISIONS_LOG.md` can be
/// re-measured after a PDFium bump rather than taken on trust.
pub fn overdraw_at(offset_pt: f32) -> Vec<u8> {
    build(
        Page::default()
            .text(FIXTURE_ORIGIN, "A")
            .text((FIXTURE_ORIGIN.0 + offset_pt, FIXTURE_ORIGIN.1), "A"),
    )
}

/// Two glyphs overlapping, for re-checking that PDFium merges only *identical* ones -
/// merging different characters would be text loss rather than dedup.
pub fn overlap_pair_at(offset_pt: f32, first: &str, second: &str) -> Vec<u8> {
    build(
        Page::default()
            .text(FIXTURE_ORIGIN, first)
            .text((FIXTURE_ORIGIN.0 + offset_pt, FIXTURE_ORIGIN.1), second),
    )
}

/// The page box the Phase-4 fixtures use: wide enough that a column has a width worth
/// measuring a rule against, and the same 800 pt tall as everything else so the furniture
/// bands fall where the earlier fixtures put them.
pub const STRUCTURE_PAGE: [f32; 4] = [0.0, 0.0, 400.0, 800.0];

/// The symbol h24 cycles: the first of `* † ‡ §`, used once on each of its two pages.
pub const CYCLED_MARKER: &str = "*";
/// What h24's two notes say. Different text, because the test is that the same *symbol*
/// resolves to different notes — if the notes said the same thing the assertion would pass
/// for the wrong reason.
pub const FIRST_NOTE: &str = "* First note text.";
pub const SECOND_NOTE: &str = "* Second note text.";
/// The size h24 sets its notes at: 0.75 x body, inside `footnote.font_size_ratio_max`.
pub const NOTE_SIZE_PT: f32 = 9.0;
/// Where h24 puts its separator rule and its notes, in PDF user space.
const NOTE_RULE_Y: f32 = 120.0;
const NOTE_RULE_THICKNESS_PT: f32 = 0.5;
const NOTE_BASELINE_Y: f32 = 100.0;
/// The left margin every Phase-4 fixture sets its text at.
pub const WIDE_MARGIN_PT: f32 = 60.0;

/// h24 - two pages, each carrying a `*` in its body and a `*` note at its foot, separated
/// from the body by a short rule.
///
/// The symbol cycle `* † ‡ §` **resets on every page** (PIPELINE §8.3), so the two `*`
/// markers are not one marker referred to twice: they are two, and a matcher that keys on
/// symbol equality across the book links both bodies to the first note and leaves the second
/// note orphaned. That is exactly the EPUBCheck RSC-007 class the bijection exists to stop
/// (test 4.8).
pub fn h24_footnote_symbol_cycle() -> Vec<u8> {
    let pages: Vec<Page> = [
        (
            "Alpha beta gamma delta.",
            "The first page runs on below.",
            FIRST_NOTE,
        ),
        (
            "Epsilon zeta eta theta.",
            "The second page does the same.",
            SECOND_NOTE,
        ),
    ]
    .into_iter()
    .map(|(body, second, note)| {
        Page::default()
            .media_box(STRUCTURE_PAGE)
            .text((WIDE_MARGIN_PT, 700.0), body)
            // The marker rides on the body line: three points up and set at 7 pt, which is
            // what `superscript_flags` reads as raised-and-small (test 2.9).
            .text_at(
                (WIDE_MARGIN_PT + 132.0, 700.0 + SUPERSCRIPT_RISE_PT),
                CYCLED_MARKER,
                SUPERSCRIPT_SIZE_PT,
            )
            .text((WIDE_MARGIN_PT, 684.0), second)
            .rule([
                WIDE_MARGIN_PT,
                NOTE_RULE_Y,
                WIDE_MARGIN_PT + 60.0,
                NOTE_RULE_Y + NOTE_RULE_THICKNESS_PT,
            ])
            .text_at((WIDE_MARGIN_PT, NOTE_BASELINE_Y), note, NOTE_SIZE_PT)
    })
    .collect();
    build_pages(pages)
}

/// A one-page document that draws the given filled rectangles and no text.
///
/// Not a committed fixture: it is parameterised, and a builder whose output depends on its
/// argument cannot be one file on disk. It exists so that the rule predicate is exercised
/// against boxes a producer actually drew rather than against the arithmetic alone.
pub fn filled_boxes(boxes: &[[f32; 4]]) -> Vec<u8> {
    let page = boxes
        .iter()
        .fold(Page::default().media_box(STRUCTURE_PAGE), |page, box_| {
            page.rule(*box_)
        });
    build_pages(vec![page])
}

/// What h25's one caption says. It carries the localized prefix, so the *caption* is not in
/// doubt — only which figure it belongs to is.
pub const AMBIGUOUS_CAPTION: &str = "Figure 1: a caption between two figures.";

/// h25 - one page, two figures side by side, and one caption placed symmetrically beneath
/// the gap between them.
///
/// Caption association is ambiguous even for humans: DocLayNet's `Caption` class has
/// inter-annotator agreement of 84-89 (R10 §6.10). The rule is to associate only when the
/// second-best distance is at least `caption.distance_ratio_min` times the best, and to
/// abstain otherwise. This fixture makes the ratio exactly one, which is the case the rule
/// exists for: geometry cannot answer, so the answer is not guessed (test 4.10).
pub fn h25_two_figures_one_caption() -> Vec<u8> {
    // Both images the same height and the same vertical distance from the caption, their
    // inner edges the same distance from its centre. Two distinct images, so that a
    // perceptual hash cannot collapse them into one figure and make the question go away.
    build_pages(vec![Page::default()
        .media_box(STRUCTURE_PAGE)
        .text((WIDE_MARGIN_PT, 740.0), "Body text above the figures.")
        .place_image(0, [60.0, 600.0, 160.0, 700.0])
        .place_image(1, [240.0, 600.0, 340.0, 700.0])
        .text_at((120.0, 570.0), AMBIGUOUS_CAPTION, NOTE_SIZE_PT)
        .text((WIDE_MARGIN_PT, 500.0), "Body text below the figures.")])
}

/// The cells h26 prints, row by row. Three columns, three rows, and no vertical rules
/// anywhere — the arrangement a book actually uses for a table, and the one Camelot's lattice
/// parser cannot read (R2 §B.9).
pub const BORDERLESS_ROWS: [[&str; 3]; 3] = [
    ["Stage", "Kind", "Budget"],
    ["text", "Budgeted", "0.005"],
    ["layout", "Conserving", "0.000"],
];

/// h26 - a table ruled above, below and under its header, with no vertical rules at all.
///
/// Two horizontal rules bound a region, so the table *is* found; with no verticals there is
/// no grid to read, and PIPELINE §8.7 says what happens then — the image plus the extracted
/// text in a `<details>` fallback, with `W_TABLE_AS_IMAGE`. Accessibility settles that shape
/// rather than engineering taste: an image of a table takes the content away from anyone who
/// cannot see it, so even the fallback carries the data (DAISY, R10 §6.12). Test 4.14.
pub fn h26_borderless_table() -> Vec<u8> {
    // Three column origins and three row baselines, with the rules between them.
    const COLUMNS: [f32; 3] = [60.0, 160.0, 260.0];
    const ROWS: [f32; 3] = [700.0, 670.0, 640.0];
    const RULE_THICKNESS: f32 = 0.5;
    let mut page = Page::default()
        .media_box(STRUCTURE_PAGE)
        .rule([55.0, 715.0, 345.0, 715.0 + RULE_THICKNESS])
        .rule([55.0, 688.0, 345.0, 688.0 + RULE_THICKNESS])
        .rule([55.0, 628.0, 345.0, 628.0 + RULE_THICKNESS]);
    for (row, cells) in BORDERLESS_ROWS.iter().enumerate() {
        for (column, cell) in cells.iter().enumerate() {
            page = page.text((COLUMNS[column], ROWS[row]), cell);
        }
    }
    build_pages(vec![
        page.text((WIDE_MARGIN_PT, 560.0), "Body text beneath the table.")
    ])
}

/// A page under construction: text runs, plus the two boxes and the rotation.
#[derive(Default)]
struct Page {
    runs: Vec<TextRun>,
    render_mode: Option<i32>,
    crop: Option<[f32; 4]>,
    rotate: Option<i32>,
    font_name: Option<&'static str>,
    full_page_image: bool,
    image_smask: bool,
    inline_image: bool,
    declared_image_size: Option<i32>,
    content_padding: Option<usize>,
    outline: Vec<(&'static str, u16)>,
    stencil_mask: bool,
    indexed_palette: bool,
    /// The `Tc` character-spacing operand, in points. `None` leaves it at the PDF default
    /// of zero.
    char_spacing: Option<f32>,
    /// Declare `/Differences [200 /fi]` over WinAnsi, so U+FB01 has a byte to be shown as.
    ligature_encoding: bool,
    /// A page box of this page's own. `None` is [`PAGE`], the tall narrow box everything
    /// that is not about geometry uses.
    media_box: Option<[f32; 4]>,
    /// Filled rectangles, in PDF user space, drawn before the text. A rule in a real book is
    /// a filled rectangle rather than a stroked line about as often as not, and a filled one
    /// is the honest shape to build: it has a thickness the extractor can measure.
    rules: Vec<[f32; 4]>,
    /// Images placed at a box of their own, as `(which shared image, [x0, y0, x1, y1])`.
    ///
    /// Two shared images rather than one per placement, and that is what makes the ornament
    /// fixture honest: the same XObject drawn on every page is byte-identical by
    /// construction, rather than by the encoder happening to be deterministic.
    placed: Vec<(usize, [f32; 4])>,
}

/// One `BT … ET` block: where it starts, what it says, and at what size.
struct TextRun {
    origin: (f32, f32),
    text: String,
    size_pt: f32,
}

impl Page {
    fn text(self, origin: (f32, f32), text: &str) -> Self {
        self.text_at(origin, text, FIXTURE_FONT_SIZE_PT)
    }

    /// Draw at a size of this run's own, for the fixtures that need a superscript or a
    /// heading next to body text.
    fn text_at(mut self, origin: (f32, f32), text: &str, size_pt: f32) -> Self {
        self.runs.push(TextRun {
            origin,
            text: text.to_owned(),
            size_pt,
        });
        self
    }

    /// Fill a rectangle: `[x0, y0, x1, y1]` in PDF user space, y up.
    fn rule(mut self, box_: [f32; 4]) -> Self {
        self.rules.push(box_);
        self
    }

    /// Place shared image `which` (0 or 1) at `[x0, y0, x1, y1]` in PDF user space.
    fn place_image(mut self, which: usize, box_: [f32; 4]) -> Self {
        self.placed.push((which, box_));
        self
    }

    /// Set `Tc`, the per-character extra advance. The honest way to build letter-spaced
    /// display text: the producer writes one string and the spacing operator, exactly as a
    /// layout engine does, rather than the fixture placing each glyph by hand at a position
    /// it had to know the font metrics to compute.
    fn char_spacing(mut self, points: f32) -> Self {
        self.char_spacing = Some(points);
        self
    }

    /// Give the page a box of its own, for the fixtures whose subject is where things sit
    /// across the width of a page rather than what they say.
    fn media_box(mut self, box_: [f32; 4]) -> Self {
        self.media_box = Some(box_);
        self
    }

    fn ligature_encoding(mut self) -> Self {
        self.ligature_encoding = true;
        self
    }

    fn render_mode(mut self, mode: i32) -> Self {
        self.render_mode = Some(mode);
        self
    }

    fn crop(mut self, box_: [f32; 4]) -> Self {
        self.crop = Some(box_);
        self
    }

    fn rotate(mut self, degrees: i32) -> Self {
        self.rotate = Some(degrees);
        self
    }

    fn font_name(mut self, name: &'static str) -> Self {
        self.font_name = Some(name);
        self
    }

    fn full_page_image(mut self) -> Self {
        self.full_page_image = true;
        self
    }

    /// Draw the page image as a one-bit stencil mask rather than as colour samples.
    fn stencil_mask(mut self) -> Self {
        self.stencil_mask = true;
        self
    }

    /// Put the page image in an Indexed colour space over DeviceRGB.
    fn indexed_palette(mut self) -> Self {
        self.indexed_palette = true;
        self
    }

    /// Give the document an outline, as `(title, level)` in reading order.
    fn outline(mut self, tree: &[(&'static str, u16)]) -> Self {
        self.outline = tree.to_vec();
        self
    }

    /// Declare the page image to be this many pixels on a side, whatever its stream holds.
    fn declared_image_size(mut self, side_px: i32) -> Self {
        self.declared_image_size = Some(side_px);
        self
    }

    /// Pad the content stream with this many bytes of whitespace, then compress it.
    fn content_padding(mut self, bytes: usize) -> Self {
        self.content_padding = Some(bytes);
        self
    }

    fn inline_image(mut self) -> Self {
        self.inline_image = true;
        self
    }

    /// Give the page image a soft mask: an 8-bit greyscale alpha channel the size of the
    /// image, which is what a photograph with a transparent background carries.
    fn image_smask(mut self) -> Self {
        self.image_smask = true;
        self
    }
}

/// The two shared 8 x 8 DeviceGray images [`build_pages`] can place, as grey levels.
///
/// Two distinct levels so that a page with two figures on it has two figures and not one
/// drawn twice, and so that a perceptual hash can tell them apart.
const PLACED_IMAGE_GREYS: [u8; 2] = [96, 200];

/// The resource name of shared image `which`: `/Im1` or `/Im2`.
fn placed_image_name(which: usize) -> &'static [u8] {
    match which {
        0 => b"Im1",
        _ => b"Im2",
    }
}

/// A document of several pages: one shared font, filled rules, two shared images, no outline.
///
/// Separate from [`build`] rather than a generalisation of it because `build` writes one page
/// and six optional features into a fixed object layout, and threading a page count through it
/// would complicate the fourteen fixtures that need exactly one page in order to serve the
/// ones that need several.
fn build_pages(pages: Vec<Page>) -> Vec<u8> {
    let catalog = Ref::new(1);
    let tree = Ref::new(2);
    let font_id = Ref::new(3);
    let image_ids = [Ref::new(4), Ref::new(5)];
    // Then a page object and a content object for each page, interleaved.
    let first_page = 6;

    let ids: Vec<(Ref, Ref)> = (0..pages.len())
        .map(|index| {
            let base = first_page + 2 * i32::try_from(index).unwrap_or(i32::MAX);
            (Ref::new(base), Ref::new(base + 1))
        })
        .collect();

    let mut pdf = Pdf::new();
    pdf.set_file_id((
        b"openconvert-fixture".to_vec(),
        b"openconvert-fixture".to_vec(),
    ));
    pdf.catalog(catalog).pages(tree);
    pdf.pages(tree)
        .kids(ids.iter().map(|(page, _)| *page))
        .count(i32::try_from(pages.len()).unwrap_or(i32::MAX));

    for (page, (page_id, content_id)) in pages.iter().zip(&ids) {
        let mut content = Content::new();
        // Rules and images first, so text drawn over them is text over them, as in a book.
        for box_ in &page.rules {
            content.rect(box_[0], box_[1], box_[2] - box_[0], box_[3] - box_[1]);
            content.fill_nonzero();
        }
        for (which, box_) in &page.placed {
            content.save_state();
            content.transform([
                box_[2] - box_[0],
                0.0,
                0.0,
                box_[3] - box_[1],
                box_[0],
                box_[1],
            ]);
            content.x_object(Name(placed_image_name(*which)));
            content.restore_state();
        }
        for run in &page.runs {
            content.begin_text();
            if let Some(spacing) = page.char_spacing {
                content.set_char_spacing(spacing);
            }
            content.set_font(Name(b"F1"), run.size_pt);
            content.next_line(run.origin.0, run.origin.1);
            content.show(Str(&to_winansi(&run.text)));
            content.end_text();
        }
        {
            let box_ = page.media_box.unwrap_or(PAGE);
            let mut written = pdf.page(*page_id);
            written
                .parent(tree)
                .media_box(Rect::new(box_[0], box_[1], box_[2], box_[3]))
                .contents(*content_id);
            {
                let mut resources = written.resources();
                resources.fonts().pair(Name(b"F1"), font_id);
                if !page.placed.is_empty() {
                    let mut objects = resources.x_objects();
                    for (which, _) in &page.placed {
                        if let Some(id) = image_ids.get(*which) {
                            objects.pair(Name(placed_image_name(*which)), *id);
                        }
                    }
                    objects.finish();
                }
                resources.finish();
            }
            written.finish();
        }
        pdf.stream(*content_id, &content.finish());
    }

    pdf.type1_font(font_id)
        .base_font(Name(BASE_FONT.as_bytes()))
        .encoding_predefined(Name(b"WinAnsiEncoding"));

    // Both shared images are written whether or not any page places one: an unreferenced
    // XObject is legal, costs sixty-four bytes, and keeps the object numbering identical
    // across every fixture this builder writes.
    for (which, id) in image_ids.iter().enumerate() {
        let grey = PLACED_IMAGE_GREYS.get(which).copied().unwrap_or(GREY_LEVEL);
        let samples = vec![grey; IMAGE_SIDE_PX * IMAGE_SIDE_PX];
        let mut image = pdf.image_xobject(*id, &samples);
        image
            .width(IMAGE_SIDE_PX as i32)
            .height(IMAGE_SIDE_PX as i32)
            .bits_per_component(BITS_PER_COMPONENT);
        image.color_space().device_gray();
        image.finish();
    }

    pdf.finish()
}

fn build(page: Page) -> Vec<u8> {
    let catalog = Ref::new(1);
    let tree = Ref::new(2);
    let page_id = Ref::new(3);
    let content_id = Ref::new(4);
    let font_id = Ref::new(5);
    let image_id = Ref::new(6);
    let smask_id = Ref::new(7);
    let to_unicode_id = Ref::new(8);
    // The outline root and its items follow, one object each.
    let outline_root = Ref::new(9);
    let outline_first = 10;

    let mut content = Content::new();
    if page.full_page_image {
        // Drawn first, so the text layer sits over it exactly as a scan's does.
        content.save_state();
        content.transform([PAGE[2], 0.0, 0.0, PAGE[3], 0.0, 0.0]);
        content.x_object(Name(b"Im1"));
        content.restore_state();
    }
    for run in &page.runs {
        content.begin_text();
        if let Some(spacing) = page.char_spacing {
            content.set_char_spacing(spacing);
        }
        if let Some(mode) = page.render_mode {
            content.set_text_rendering_mode(match mode {
                RENDER_MODE_INVISIBLE => pdf_writer::types::TextRenderingMode::Invisible,
                _ => pdf_writer::types::TextRenderingMode::Fill,
            });
        }
        content.set_font(Name(b"F1"), run.size_pt);
        content.next_line(run.origin.0, run.origin.1);
        // WinAnsi is a byte encoding, so a code point outside it cannot be written; the
        // ligature fixture relies on U+FB01 having a WinAnsi byte, which it does not, so it
        // is written through the font's own encoding below.
        content.show(Str(&to_winansi(&run.text)));
        content.end_text();
    }
    // `Content::finish` yields a `Buf`; the inline image is appended to its bytes, because
    // `pdf-writer` has no `BI` operator and the content stream is only ever bytes.
    let mut content: Vec<u8> = content.finish().to_vec();
    if page.inline_image {
        content.extend_from_slice(&inline_image_operator());
    }
    let compressed = page.content_padding.and_then(|bytes| {
        // Whitespace between operators is legal and ignored, so the page renders exactly as
        // it would without it. Compressed, it is what makes the file small and the read big.
        content.extend(std::iter::repeat_n(b' ', bytes));
        let mut stream = lopdf::Stream::new(lopdf::Dictionary::new(), content.clone());
        stream.compress().ok()?;
        Some(stream.content)
    });

    let mut pdf = Pdf::new();
    // A fixed file id: without one, `pdf-writer` leaves the trailer's /ID absent, and with a
    // generated one the bytes would change per run. Committed fixtures need neither.
    pdf.set_file_id((
        b"openconvert-fixture".to_vec(),
        b"openconvert-fixture".to_vec(),
    ));
    {
        let mut written = pdf.catalog(catalog);
        written.pages(tree);
        if !page.outline.is_empty() {
            written.outlines(outline_root);
        }
        written.finish();
    }
    pdf.pages(tree).kids([page_id]).count(1);

    {
        let mut written = pdf.page(page_id);
        written
            .parent(tree)
            .media_box(Rect::new(PAGE[0], PAGE[1], PAGE[2], PAGE[3]))
            .contents(content_id);
        if let Some(crop) = page.crop {
            written.crop_box(Rect::new(crop[0], crop[1], crop[2], crop[3]));
        }
        if let Some(rotate) = page.rotate {
            written.rotate(rotate);
        }
        {
            let mut resources = written.resources();
            resources.fonts().pair(Name(b"F1"), font_id);
            if page.full_page_image {
                resources.x_objects().pair(Name(b"Im1"), image_id);
            }
            resources.finish();
        }
        written.finish();
    }

    {
        let mut font = pdf.type1_font(font_id);
        font.base_font(Name(page.font_name.unwrap_or(BASE_FONT).as_bytes()));
        if page.ligature_encoding {
            font.to_unicode(to_unicode_id);
            font.encoding_predefined(Name(b"WinAnsiEncoding"));
        } else {
            font.encoding_predefined(Name(b"WinAnsiEncoding"));
        }
    }
    if page.ligature_encoding {
        pdf.stream(to_unicode_id, LIGATURE_TO_UNICODE);
    }

    if page.full_page_image {
        // Eight by eight mid-grey pixels, uncompressed. A scan's content does not matter to
        // any of these fixtures; that an image covers the page does.
        // A stencil mask is one bit per pixel and its rows are byte-aligned, so an eight-wide
        // image is one byte per row: the top half 0 (painted), the bottom half 1 (masked out).
        let stencil: Vec<u8> = (0..IMAGE_SIDE_PX)
            .map(|row| if row < IMAGE_SIDE_PX / 2 { 0x00 } else { 0xFF })
            .collect();
        // Indexed samples are palette indices, one byte each: top half entry 0, bottom entry 1.
        let indexed: Vec<u8> = (0..IMAGE_SIDE_PX * IMAGE_SIDE_PX)
            .map(|i| u8::from(i >= IMAGE_SIDE_PX * IMAGE_SIDE_PX / 2))
            .collect();
        let grey = vec![GREY_LEVEL; IMAGE_SIDE_PX * IMAGE_SIDE_PX];

        let samples = if page.stencil_mask {
            &stencil
        } else if page.indexed_palette {
            &indexed
        } else {
            &grey
        };

        let mut image = pdf.image_xobject(image_id, samples);
        // The declared size is what a reader believes and what a limit has to be checked
        // against; the stream behind it stays sixty-four bytes.
        let side = page.declared_image_size.unwrap_or(IMAGE_SIDE_PX as i32);
        image.width(side).height(side);
        if page.stencil_mask {
            image.image_mask(true);
        } else if page.indexed_palette {
            let lookup: Vec<u8> = INDEXED_PALETTE.iter().flatten().copied().collect();
            image.color_space().indexed(
                Name(b"DeviceRGB"),
                i32::try_from(INDEXED_PALETTE.len() - 1).unwrap_or(1),
                &lookup,
            );
        } else {
            image.color_space().device_gray();
        }
        // A stencil mask is one bit per sample by definition; everything else here is eight.
        image.bits_per_component(if page.stencil_mask {
            1
        } else {
            BITS_PER_COMPONENT
        });
        if page.image_smask {
            image.s_mask(smask_id);
        }
        image.finish();

        if page.image_smask {
            // Half opaque, half transparent, so the mask is not a constant a reader could
            // optimise away.
            let samples = IMAGE_SIDE_PX * IMAGE_SIDE_PX;
            let mut alpha = vec![u8::MAX; samples];
            alpha[..samples / 2].fill(0);
            let mut mask = pdf.image_xobject(smask_id, &alpha);
            mask.width(IMAGE_SIDE_PX as i32)
                .height(IMAGE_SIDE_PX as i32)
                .color_space()
                .device_gray();
            mask.bits_per_component(BITS_PER_COMPONENT).finish();
        }
    }

    if !page.outline.is_empty() {
        write_outline(&mut pdf, &page.outline, outline_root, outline_first);
    }

    match &compressed {
        Some(bytes) => {
            let mut stream = pdf.stream(content_id, bytes);
            stream.filter(pdf_writer::Filter::FlateDecode);
            stream.finish();
        }
        None => {
            pdf.stream(content_id, &content).finish();
        }
    }
    pdf.finish()
}

/// Write an outline tree from a flat `(title, level)` list in reading order.
///
/// The flat form is how a reader thinks about a table of contents and how the test asserts;
/// the file wants a doubly-linked tree of `/First`, `/Last`, `/Next`, `/Prev` and `/Parent`.
/// Converting between them here is what makes the fixture's expected order something written
/// down rather than inferred from whatever the builder happened to emit.
fn write_outline(pdf: &mut Pdf, tree: &[(&'static str, u16)], root: Ref, first_id: i32) {
    let id_of = |index: usize| Ref::new(first_id + i32::try_from(index).unwrap_or(i32::MAX));

    // For each entry, its parent (the nearest earlier entry one level up) and its siblings.
    let parents: Vec<Option<usize>> = tree
        .iter()
        .enumerate()
        .map(|(index, (_, level))| {
            tree[..index]
                .iter()
                .rposition(|(_, other)| *other + 1 == *level)
        })
        .collect();
    let children = |parent: Option<usize>| -> Vec<usize> {
        (0..tree.len())
            .filter(|index| parents[*index] == parent)
            .collect()
    };

    let roots = children(None);
    {
        let mut written = pdf.outline(root);
        if let (Some(first), Some(last)) = (roots.first(), roots.last()) {
            written.first(id_of(*first)).last(id_of(*last));
        }
        written.count(i32::try_from(roots.len()).unwrap_or(i32::MAX));
        written.finish();
    }

    for (index, (title, _)) in tree.iter().enumerate() {
        let siblings = children(parents[index]);
        let position = siblings.iter().position(|s| *s == index).unwrap_or(0);
        let mine = children(Some(index));

        let mut item = pdf.outline_item(id_of(index));
        item.title(TextStr(title));
        match parents[index] {
            Some(parent) => item.parent(id_of(parent)),
            None => item.parent(root),
        };
        if let Some(previous) = position.checked_sub(1).and_then(|p| siblings.get(p)) {
            item.prev(id_of(*previous));
        }
        if let Some(next) = siblings.get(position + 1) {
            item.next(id_of(*next));
        }
        if let (Some(first), Some(last)) = (mine.first(), mine.last()) {
            item.first(id_of(*first)).last(id_of(*last));
            item.count(i32::try_from(mine.len()).unwrap_or(i32::MAX));
        }
        item.finish();
    }
}

/// An inline image drawn over the whole page, as raw content-stream bytes.
///
/// `BI` opens the dictionary in abbreviated form - `/W` width, `/H` height, `/CS` colour
/// space, `/BPC` bits per component - `ID` is followed by exactly one space and then the
/// samples, and `EI` closes it. The samples are a constant grey, so no byte sequence in them
/// can be mistaken for the terminator.
fn inline_image_operator() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(
        b"q
",
    );
    out.extend_from_slice(
        format!(
            "{} 0 0 {} 0 0 cm
",
            PAGE[2], PAGE[3]
        )
        .as_bytes(),
    );
    out.extend_from_slice(
        format!("BI /W {IMAGE_SIDE_PX} /H {IMAGE_SIDE_PX} /CS /G /BPC {BITS_PER_COMPONENT} ID ")
            .as_bytes(),
    );
    out.extend(std::iter::repeat_n(
        GREY_LEVEL,
        IMAGE_SIDE_PX * IMAGE_SIDE_PX,
    ));
    out.extend_from_slice(
        b"
EI
Q
",
    );
    out
}

/// Encode text as WinAnsi bytes, which is what the fixtures' font declares.
///
/// A character with no WinAnsi byte is written as its own low byte, which is deliberate: the
/// ligature fixture wants a glyph the font cannot map, and that is exactly what a subset
/// font with no `ToUnicode` produces in the wild.
fn to_winansi(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| match c {
            // The one character with a byte only because the fixture gave it one.
            '\u{FB01}' => LIGATURE_CODE,
            other => u8::try_from(u32::from(other)).unwrap_or(b'?'),
        })
        .collect()
}

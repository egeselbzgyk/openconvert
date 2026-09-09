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

use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str};

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

/// h04 — the `fi` ligature as a single code point, which normalisation `N` expands in
/// Phase 2. Phase 1 only has to carry it through unchanged.
pub fn h04_ligature_fi() -> Vec<u8> {
    build(Page::default().text(FIXTURE_ORIGIN, "\u{FB01}n"))
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

/// A page under construction: text runs, plus the two boxes and the rotation.
#[derive(Default)]
struct Page {
    runs: Vec<((f32, f32), String)>,
    render_mode: Option<i32>,
    crop: Option<[f32; 4]>,
    rotate: Option<i32>,
    font_name: Option<&'static str>,
    full_page_image: bool,
}

impl Page {
    fn text(mut self, origin: (f32, f32), text: &str) -> Self {
        self.runs.push((origin, text.to_owned()));
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
}

fn build(page: Page) -> Vec<u8> {
    let catalog = Ref::new(1);
    let tree = Ref::new(2);
    let page_id = Ref::new(3);
    let content_id = Ref::new(4);
    let font_id = Ref::new(5);
    let image_id = Ref::new(6);

    let mut content = Content::new();
    if page.full_page_image {
        // Drawn first, so the text layer sits over it exactly as a scan's does.
        content.save_state();
        content.transform([PAGE[2], 0.0, 0.0, PAGE[3], 0.0, 0.0]);
        content.x_object(Name(b"Im1"));
        content.restore_state();
    }
    for (origin, text) in &page.runs {
        content.begin_text();
        if let Some(mode) = page.render_mode {
            content.set_text_rendering_mode(match mode {
                RENDER_MODE_INVISIBLE => pdf_writer::types::TextRenderingMode::Invisible,
                _ => pdf_writer::types::TextRenderingMode::Fill,
            });
        }
        content.set_font(Name(b"F1"), FIXTURE_FONT_SIZE_PT);
        content.next_line(origin.0, origin.1);
        // WinAnsi is a byte encoding, so a code point outside it cannot be written; the
        // ligature fixture relies on U+FB01 having a WinAnsi byte, which it does not, so it
        // is written through the font's own encoding below.
        content.show(Str(&to_winansi(text)));
        content.end_text();
    }
    let content = content.finish();

    let mut pdf = Pdf::new();
    // A fixed file id: without one, `pdf-writer` leaves the trailer's /ID absent, and with a
    // generated one the bytes would change per run. Committed fixtures need neither.
    pdf.set_file_id((
        b"openconvert-fixture".to_vec(),
        b"openconvert-fixture".to_vec(),
    ));
    pdf.catalog(catalog).pages(tree);
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

    pdf.type1_font(font_id)
        .base_font(Name(page.font_name.unwrap_or(BASE_FONT).as_bytes()))
        .encoding_predefined(Name(b"WinAnsiEncoding"));

    if page.full_page_image {
        // Eight by eight mid-grey pixels, uncompressed. A scan's content does not matter to
        // any of these fixtures; that an image covers the page does.
        let pixels = vec![GREY_LEVEL; IMAGE_SIDE_PX * IMAGE_SIDE_PX];
        let mut image = pdf.image_xobject(image_id, &pixels);
        image
            .width(IMAGE_SIDE_PX as i32)
            .height(IMAGE_SIDE_PX as i32)
            .color_space()
            .device_gray();
        image.bits_per_component(8).finish();
    }

    pdf.stream(content_id, &content).finish();
    pdf.finish()
}

/// Encode text as WinAnsi bytes, which is what the fixtures' font declares.
///
/// A character with no WinAnsi byte is written as its own low byte, which is deliberate: the
/// ligature fixture wants a glyph the font cannot map, and that is exactly what a subset
/// font with no `ToUnicode` produces in the wild.
fn to_winansi(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?'))
        .collect()
}

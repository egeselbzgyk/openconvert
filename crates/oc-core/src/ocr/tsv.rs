//! Tesseract's TSV output, read by schema (PHASE 13 detail 4).
//!
//! **Pure and schema-checked.** The parser is a function over bytes, so it is testable without a
//! Tesseract, and it refuses any header but the exact twelve columns it was written against. That
//! refusal is the version-drift detector: a naive parser splits on tabs and indexes by position,
//! which is correct until a release adds, drops or reorders a column — and then it reads `height`
//! as `conf` without a word of complaint. Here the same drift is `TsvError::UnexpectedSchema`,
//! loudly, with the header that was found in the message.
//!
//! **Units change at this boundary and nowhere else.** Tesseract speaks pixels of the raster it was
//! given, relative to that raster, and percent. Everything else in the pipeline speaks points in
//! normalised page space (D13.3) and `0.0..=1.0`. [`px_to_pt`] is the one conversion, and property
//! test 13.8 holds it and its inverse to 0.01 pt over two thousand random regions.

use oc_model::geom::Rect;

use super::OcrWord;

/// The columns Tesseract 5's `tsv` config prints, in order. Anything else is drift.
pub const HEADER: [&str; 12] = [
    "level",
    "page_num",
    "block_num",
    "par_num",
    "line_num",
    "word_num",
    "left",
    "top",
    "width",
    "height",
    "conf",
    "text",
];

/// Positions in [`HEADER`], named so a reader never has to count tabs.
const LEVEL: usize = 0;
const BLOCK: usize = 2;
const PAR: usize = 3;
const LINE: usize = 4;
const LEFT: usize = 6;
const TOP: usize = 7;
const WIDTH: usize = 8;
const HEIGHT: usize = 9;
const CONF: usize = 10;
const TEXT: usize = 11;

/// Tesseract's `level` for a word row. Levels 1–4 are page, block, paragraph and line: structure,
/// not text. A property of the format, not a tunable.
const WORD_LEVEL: u32 = 5;

/// Tesseract reports confidence in percent; the pipeline carries `0.0..=1.0`.
const PERCENT: f32 = 100.0;

/// PostScript points per inch — the unit the normalised page space is in (D13.3).
pub const POINTS_PER_INCH: f32 = 72.0;

/// Why a TSV could not be read.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum TsvError {
    /// The header is not [`HEADER`]: Tesseract's column set has drifted, or this is not a TSV.
    #[error(
        "unexpected Tesseract TSV schema: the header was `{found}`, expected the 12 columns of \
         Tesseract 5"
    )]
    UnexpectedSchema { found: String },
    /// A row that does not parse under the header it came with.
    #[error("Tesseract TSV row {row}: {message}")]
    BadRow { row: usize, message: String },
    /// Tesseract's output was not UTF-8.
    #[error("Tesseract TSV is not UTF-8")]
    NotUtf8,
    /// A raster at zero dots per inch has no size in points.
    #[error("a raster at 0 dpi cannot be mapped into page space")]
    ZeroDpi,
}

/// Parse Tesseract's TSV into words in normalised page space.
///
/// `dpi` is the resolution the region was rasterized at and `region_pt` the region's box on the
/// page, so a pixel `(x, y)` of the raster lands at `region_pt.origin + (x, y) × 72 / dpi`. Only
/// `level == 5` rows become words; a row with `conf == -1` (Tesseract's "no score") or with only
/// whitespace for text is dropped.
pub fn parse_tsv(bytes: &[u8], dpi: u32, region_pt: Rect) -> Result<Vec<OcrWord>, TsvError> {
    if dpi == 0 {
        return Err(TsvError::ZeroDpi);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| TsvError::NotUtf8)?;
    let mut lines = text.lines();
    let header = lines.next().unwrap_or_default();
    let columns: Vec<&str> = header.trim_end_matches('\r').split('\t').collect();
    if columns != HEADER {
        return Err(TsvError::UnexpectedSchema {
            found: header.to_owned(),
        });
    }

    let mut words = Vec::new();
    for (index, line) in lines.enumerate() {
        let row = index + 1;
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        // The text is the last column and the only one that could itself contain a tab, so the
        // row is cut into exactly as many fields as the header has and no more.
        let fields: Vec<&str> = line.splitn(HEADER.len(), '\t').collect();
        let field = |position: usize| {
            fields
                .get(position)
                .copied()
                .ok_or_else(|| TsvError::BadRow {
                    row,
                    message: format!("{} columns, the header has {}", fields.len(), HEADER.len()),
                })
        };
        let level: u32 = number(field(LEVEL)?, row, "level")?;
        if level != WORD_LEVEL {
            continue;
        }
        let conf: f32 = number(field(CONF)?, row, "conf")?;
        // `-1` is Tesseract's "not scored", and no real confidence is negative.
        if conf < 0.0 {
            continue;
        }
        let word = field(TEXT)?.trim();
        if word.is_empty() {
            continue;
        }
        let left: f32 = number(field(LEFT)?, row, "left")?;
        let top: f32 = number(field(TOP)?, row, "top")?;
        let width: f32 = number(field(WIDTH)?, row, "width")?;
        let height: f32 = number(field(HEIGHT)?, row, "height")?;
        let (x0, y0) = px_to_pt((left, top), dpi, region_pt);
        let (x1, y1) = px_to_pt((left + width, top + height), dpi, region_pt);
        words.push(OcrWord {
            text: word.to_owned(),
            bbox: Rect { x0, y0, x1, y1 },
            conf: (conf / PERCENT).clamp(0.0, 1.0),
            block: number(field(BLOCK)?, row, "block_num")?,
            par: number(field(PAR)?, row, "par_num")?,
            line: number(field(LINE)?, row, "line_num")?,
        });
    }
    Ok(words)
}

/// A raster pixel of a region rendered at `dpi`, in normalised page space.
pub fn px_to_pt(px: (f32, f32), dpi: u32, region: Rect) -> (f32, f32) {
    let factor = POINTS_PER_INCH / dpi as f32;
    (region.x0 + px.0 * factor, region.y0 + px.1 * factor)
}

/// The inverse of [`px_to_pt`]: where a page point falls in the region's raster.
pub fn pt_to_px(pt: (f32, f32), dpi: u32, region: Rect) -> (f32, f32) {
    let factor = dpi as f32 / POINTS_PER_INCH;
    ((pt.0 - region.x0) * factor, (pt.1 - region.y0) * factor)
}

fn number<N: std::str::FromStr>(field: &str, row: usize, name: &str) -> Result<N, TsvError> {
    field.trim().parse().map_err(|_| TsvError::BadRow {
        row,
        message: format!("{name} is not a number: {field:?}"),
    })
}

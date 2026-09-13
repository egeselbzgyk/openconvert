//! The layout layer: blocks, the unit the `layout` stage mints and everything after it
//! refers to (IR_SKETCH, PIPELINE §6).
//!
//! A **block** is a set of lines the page keeps together: one paragraph, one heading, one
//! cell of running text. It is where block identity is first minted, so every later stage —
//! and every user override, and every LLM decision cache entry — is keyed on something that
//! exists from here on (D13.3).
//!
//! A block carries no semantics. Whether it is a heading, a caption or a list item is
//! Phase 4's question; all `layout` says is which lines belong together, which column they
//! sit in, and in what order a reader meets them.

use serde::Serialize;

use crate::extract::PageRef;
use crate::geom::Rect;
use crate::ids::BlockId;
use crate::text::{FurnitureKind, Line};

/// What the geometry alone suggests a block is.
///
/// Deliberately not a semantic label: these four are exactly the categories reading order
/// needs in order to *pre-mask* (PIPELINE §6 step 3). XY-Cut++ masks the high-dynamic
/// elements before cutting because they are what fragments an otherwise clean sort, and the
/// mask needs to know which blocks those are. Everything finer — heading levels, captions,
/// list items, verse — is `structure`'s, in Phase 4.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockKindHint {
    /// Ordinary text, set in a column.
    Text,
    /// Text that spans more than one column: a floating title, a full-width table caption.
    /// Cutting a page without masking these first splits the title down the gutter.
    FloatingTitle,
    /// An image region.
    Image,
    /// A thin horizontal or vertical vector rule.
    Rule,
}

/// A set of lines the page keeps together.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Block {
    pub id: BlockId,
    pub page: PageRef,
    pub bbox: Rect,
    pub lines: Vec<Line>,
    /// Zero-based column index within the page, contiguous from 0 (PIPELINE §6 validation).
    pub column: u8,
    pub kind_hint: BlockKindHint,
    /// Set only for blocks `furniture` condemned. Body blocks carry `None`; the stage that
    /// removes furniture runs before this one, so in practice this is `None` throughout v1
    /// and exists so a later preset that *keeps* furniture has somewhere to say so.
    pub furniture: Option<FurnitureKind>,
    /// Position in the document's reading order. A permutation of the block set — no index
    /// invented, none dropped, none repeated.
    pub reading_index: u32,
}

impl Block {
    /// The block's text, lines joined by single spaces.
    ///
    /// This is the form the `BlockId` is derived from and the form a confidence signal reads;
    /// it is *not* the paragraph text, which is `paragraphs`' to build because it is the one
    /// that may drop a hyphen.
    pub fn text(&self, line_text: impl Fn(&Line) -> String) -> String {
        self.lines
            .iter()
            .map(line_text)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// How a book marks the start of a paragraph.
///
/// One of these per *book*, not per page: a book is internally consistent about it, and
/// per-document adaptation is the capability R1 §C.4 #3 says nobody in this space fully
/// exploits. A page is not enough evidence — a chapter opening indents nothing, a page of
/// dialogue indents everything.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ParagraphConvention {
    /// The first line of a paragraph is indented; paragraphs follow each other without a gap.
    FirstLineIndent,
    /// Paragraphs are separated by extra leading and no line is indented.
    BlankLine,
}

/// A paragraph: the lines of one, in order, from wherever they were printed.
///
/// One `Para` may span several blocks, several columns and several pages — that is the point
/// of it. What it does not yet carry is meaning: `spans` (with their styles), `align`, `lang`
/// and `confidence` are `structure`'s to add in Phase 4, and adding them here with nothing to
/// fill them would be a field that is always the same value.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Para {
    pub id: BlockId,
    /// The blocks this paragraph was assembled from, in reading order.
    pub blocks: Vec<BlockId>,
    /// The lines, in reading order, with their block-relative indent and right gap.
    pub lines: Vec<Line>,
    /// The paragraph's text: its lines joined, hyphens resolved by `paragraphs` under I-5.
    pub text: String,
    /// Whether the paragraph's own first line was indented.
    pub first_line_indent: bool,
    /// The pages it covers, first and last.
    pub pages: (u32, u32),
}

impl Para {
    /// Whether the paragraph was assembled from more than one block — across a column
    /// boundary, a page boundary, or both.
    pub fn is_merged(&self) -> bool {
        self.blocks.len() > 1
    }
}

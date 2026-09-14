//! Building semantic values out of the block view.
//!
//! One place, because two rules need the same thing — a list item and a note body are both
//! "some of a block's lines, as a paragraph" — and because `BlockId` minting has to be done
//! once, with one collision set, or two paragraphs in one book can share an id.

use oc_model::doc::{Align, Span};
use oc_model::geom::Rect;
use oc_model::ids::BlockId;
use oc_model::layout::Para;

use crate::view::LineView;

/// Mints block ids, refusing to hand out the same one twice.
///
/// `BlockId::derive` is a pure function of page, box and text, so two paragraphs that are
/// genuinely identical — two blank-looking list items, the same line in a table — derive the
/// same id. The collision suffix exists for exactly that (D13.3), and something has to
/// remember which suffixes are spent.
#[derive(Debug, Default)]
pub struct Minter {
    seen: std::collections::BTreeSet<String>,
}

impl Minter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn mint(&mut self, page: u32, bbox: Rect, text: &str) -> BlockId {
        let base = BlockId::derive(page, bbox, text);
        let mut id = base;
        for suffix in 0..32u8 {
            id = base.with_collision_suffix(suffix);
            if self.seen.insert(id.as_str().to_owned()) {
                break;
            }
        }
        id
    }
}

/// Build a paragraph out of some of a block's lines.
///
/// The text is the lines joined by single spaces, which is the same flattening `layout` uses
/// — and it has to stay the same, because the conservation check across `structure` compares
/// the two multisets and a different join would read as a stage that changed the text.
pub fn para_of(
    minter: &mut Minter,
    page: u32,
    source: &[BlockId],
    lines: &[&LineView],
    strip_prefix: Option<&str>,
) -> Para {
    let mut text = lines
        .iter()
        .map(|line| line.text.trim())
        .collect::<Vec<_>>()
        .join(" ");
    // A list marker is not part of its item's text: `<ol>` draws it. It is removed here and
    // nowhere else, and the stage that calls this declares it to the ledger.
    if let Some(prefix) = strip_prefix {
        if let Some(rest) = text.strip_prefix(prefix) {
            text = rest.trim_start().to_owned();
        }
    }

    let bbox = lines
        .iter()
        .map(|line| line.bbox())
        .reduce(|a, b| Rect {
            x0: a.x0.min(b.x0),
            y0: a.y0.min(b.y0),
            x1: a.x1.max(b.x1),
            y1: a.y1.max(b.y1),
        })
        .unwrap_or(Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 0.0,
            y1: 0.0,
        });

    Para {
        id: minter.mint(page, bbox, &text),
        blocks: source.to_vec(),
        lines: lines.iter().map(|line| line.line.clone()).collect(),
        spans: vec![Span::plain(text.clone())],
        text,
        first_line_indent: lines.first().is_some_and(|line| line.indent_pt() > 0.0),
        pages: (page, page),
        drop_cap: false,
        lang: None,
        align: Align::Left,
        confidence: None,
    }
}

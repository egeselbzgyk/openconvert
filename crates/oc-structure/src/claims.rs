//! Which structure took a block's text out of the flow (PHASE 7.5 item 4).
//!
//! `structure` drops a block from the flow when something else has undertaken to emit its
//! text: a note body, a table cell, a list item, a bound caption. Until this module existed
//! that undertaking was a `BTreeSet<BlockId>` — a set of ids with no record of who put each
//! one there and no check that the claimant kept its word. Four independent places could get
//! it wrong and none of them was checked.
//!
//! A `Claim` carries the obligation instead of only the fact. It names the block, the
//! claimant, and the text the claimant is undertaking to emit, which is what makes the
//! obligation checkable: `Σ claimed_text` has to appear in what the book actually contains,
//! and a fifth claimant added later is checked by construction because it cannot enter the
//! set without saying who it is.

use oc_model::doc::{Content, Figure, List, Note, Table};
use oc_model::ids::BlockId;

/// What kind of structure took a block out of the flow.
///
/// The kind alone, because a defect class is `(stage, direction, signature)` and a signature
/// carrying the book's own ids would make every document its own class. The id travels
/// beside it in [`Claimant::id`], for the reader who has to go and look.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ClaimKind {
    /// The block is part of a note's body.
    Note,
    /// The block's text was read into a table's cells.
    Table,
    /// The block became an item of a list.
    List,
    /// The block is a caption bound to a figure.
    Caption,
}

impl ClaimKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ClaimKind::Note => "note",
            ClaimKind::Table => "table",
            ClaimKind::List => "list",
            ClaimKind::Caption => "caption",
        }
    }
}

/// What took a block out of the flow, and which one of them it was.
///
/// `id` is an `Option` and not a sentinel because "a table took it and the stage cannot say
/// which" is a real and useful answer — it is itself a defect, and a made-up id would hide
/// it behind a lookup that fails somewhere else.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Claimant {
    pub kind: ClaimKind,
    pub id: Option<String>,
}

impl Claimant {
    pub fn new(kind: ClaimKind, id: impl Into<String>) -> Self {
        Self {
            kind,
            id: Some(id.into()),
        }
    }

    /// A claimant of this kind that the stage could not name.
    pub fn unnamed(kind: ClaimKind) -> Self {
        Self { kind, id: None }
    }

    /// How the claimant is named in a diagnostic.
    pub fn label(&self) -> String {
        match &self.id {
            Some(id) => format!("{} {id}", self.kind.as_str()),
            None => format!("{} (unnamed)", self.kind.as_str()),
        }
    }

    /// The kind alone, which is what a defect class is keyed on.
    pub fn kind(&self) -> &'static str {
        self.kind.as_str()
    }
}

/// A block whose text some structure has undertaken to emit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Claim {
    pub block: BlockId,
    pub page: u32,
    pub by: Claimant,
    /// The block's text, as the flow would have emitted it.
    pub text: String,
}

/// Every claim made over a document's blocks, in block order.
///
/// A `Vec` and not a set: two claimants wanting the same block is itself a defect, and a set
/// would silently keep whichever arrived first. The check reports it.
#[derive(Clone, Debug, Default)]
pub struct Claims {
    claims: Vec<Claim>,
    /// Which blocks are claimed, for the question the flow loop asks once per block.
    ///
    /// An index and not a convenience: `structure` walks every block and asks whether it is
    /// claimed, so a linear scan here is quadratic in the size of the book — which on a
    /// 368-page InDesign volume is the difference between a stage that returns and one that
    /// does not.
    index: std::collections::BTreeSet<BlockId>,
}

impl Claims {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, claim: Claim) {
        self.index.insert(claim.block);
        self.claims.push(claim);
    }

    pub fn iter(&self) -> impl Iterator<Item = &Claim> {
        self.claims.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.claims.is_empty()
    }

    pub fn len(&self) -> usize {
        self.claims.len()
    }

    /// Whether any claimant has undertaken to emit this block.
    pub fn contains(&self, block: BlockId) -> bool {
        self.index.contains(&block)
    }

    /// The claimant that took this block, if one did.
    pub fn claimant_of(&self, block: BlockId) -> Option<Claimant> {
        self.claims
            .iter()
            .find(|claim| claim.block == block)
            .map(|claim| claim.by.clone())
    }

    /// Blocks claimed by more than one structure.
    ///
    /// Not a hypothetical: a note body opens with `*` exactly as a bulleted list item does,
    /// and a caption under a table is a caption and a table row at once. Two claimants for
    /// one block means one of them will not emit it, and which one is an accident of
    /// iteration order.
    pub fn contested(&self) -> Vec<(BlockId, Vec<Claimant>)> {
        let mut by_block: std::collections::BTreeMap<BlockId, Vec<Claimant>> =
            std::collections::BTreeMap::new();
        for claim in &self.claims {
            by_block
                .entry(claim.block)
                .or_default()
                .push(claim.by.clone());
        }
        by_block
            .into_iter()
            .filter(|(_, claimants)| claimants.len() > 1)
            .collect()
    }
}

/// Where a claimant's text lives, for the check that it kept its word.
///
/// This is deliberately *not* "every note / table / figure the stage produced". A container
/// the stage built and no section references is a container the reader never sees, and
/// counting its text as emitted is how a loss becomes silent. See
/// [`crate::stage::StructureOutput::reachable_text`].
pub fn note_text(note: &Note) -> Vec<String> {
    let mut out = Vec::new();
    collect_note(&note.body, &mut out);
    out
}

fn collect_note(content: &[Content], out: &mut Vec<String>) {
    for item in content {
        match item {
            Content::Paragraph(para) => out.push(para.text.clone()),
            Content::Heading(heading) => out.push(heading.text()),
            Content::BlockQuote(inner) | Content::Epigraph(inner) => collect_note(inner, out),
            Content::Preformatted(pre) => out.extend(pre.lines.iter().cloned()),
            _ => {}
        }
    }
}

/// A table's text: every cell.
pub fn table_text(table: &Table) -> Vec<String> {
    table.cell_texts()
}

/// A figure's text: its caption, when it has one bound.
pub fn figure_text(figure: &Figure) -> Vec<String> {
    figure
        .caption
        .as_ref()
        .map(|caption| vec![oc_model::doc::spans_text(caption)])
        .into_iter()
        .flatten()
        .collect()
}

/// A list's text: every item, and every nested item under it.
pub fn list_text(list: &List) -> Vec<String> {
    let mut out = Vec::new();
    collect_list(list, &mut out);
    out
}

fn collect_list(list: &List, out: &mut Vec<String>) {
    for item in &list.items {
        collect_note(&item.content, out);
        if let Some(nested) = &item.nested {
            collect_list(nested, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oc_model::geom::Rect;

    fn block(text: &str) -> BlockId {
        BlockId::derive(
            0,
            Rect {
                x0: 0.0,
                y0: 0.0,
                x1: 10.0,
                y1: 10.0,
            },
            text,
        )
    }

    #[test]
    fn a_claim_names_the_claimant_that_made_it() {
        let mut claims = Claims::new();
        let id = block("taken");
        claims.push(Claim {
            block: id,
            page: 0,
            by: Claimant::new(ClaimKind::List, block("the list").as_str()),
            text: "taken".to_owned(),
        });

        assert!(claims.contains(id));
        assert_eq!(claims.claimant_of(id).map(|by| by.kind()), Some("list"));
        assert!(claims.contested().is_empty());
    }

    #[test]
    fn two_claimants_for_one_block_are_reported_rather_than_resolved() {
        // A note body opening with `*` is also a bulleted list item. Whichever claimant the
        // iteration reaches second would have been dropped silently by a `BTreeSet`.
        let mut claims = Claims::new();
        let id = block("* a note that looks like a bullet");
        claims.push(Claim {
            block: id,
            page: 0,
            by: Claimant::new(ClaimKind::Note, "n1"),
            text: "* a note".to_owned(),
        });
        claims.push(Claim {
            block: id,
            page: 0,
            by: Claimant::new(ClaimKind::List, block("l").as_str()),
            text: "* a note".to_owned(),
        });

        let contested = claims.contested();

        assert_eq!(contested.len(), 1);
        assert_eq!(contested[0].1.len(), 2);
    }
}

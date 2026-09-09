//! Stable block identifiers (D13.3).
//!
//! `BlockId` is `base32(blake3(page_index ‖ bbox rounded to 1 pt ‖ first 64 NFC chars))[..10]`
//! with a collision suffix. Changing the derivation bumps `ir_version` and invalidates
//! caches and user overrides, which is why test 0.1 pins the derivation to a committed
//! constant: an accidental change to the hash input must fail loudly, not silently
//! renumber every block in every book.

use unicode_normalization::UnicodeNormalization;

use crate::geom::Rect;

/// The RFC 4648 base32 alphabet, in order, as the encoder emits it. The last character
/// of an id is an index into this table (see [`BlockId::with_collision_suffix`]).
const BASE32_ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Base32 without padding: an id is a fixed-width run of alphabet characters.
const BASE32: base32::Alphabet = base32::Alphabet::Rfc4648 { padding: false };

/// Total length of an id in base32 characters (D13.3, `[..10]`).
const ID_LEN: usize = 10;

/// How many of those characters carry hash bits. The last one is the collision counter,
/// so an id is 45 bits of digest plus a 5-bit counter rather than 50 bits of digest.
const HASH_CHARS: usize = ID_LEN - 1;

/// Only this many characters of the block's text enter the hash (D13.3, "first 64 NFC
/// chars"). Characters, not bytes: the count must not depend on the script.
const TEXT_PREFIX_CHARS: usize = 64;

/// A stable, content-derived block identifier (D13.3).
///
/// `base32(blake3(page_index ‖ bbox rounded to 1 pt ‖ first 64 NFC chars))`, truncated to
/// [`HASH_CHARS`] characters, with the collision counter as the final character. The ten
/// bytes are always base32 alphabet characters, which is what makes [`BlockId::as_str`]
/// infallible.
///
/// The derivation is part of the IR contract: changing it changes every id in every book,
/// which invalidates the LLM decision cache and every user override keyed by block id.
/// It therefore requires bumping `IR_VERSION` in the same commit.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockId([u8; ID_LEN]);

impl BlockId {
    /// Derive the id of a block from the three inputs named in D13.3.
    ///
    /// `text` may be of any length; only its first [`TEXT_PREFIX_CHARS`] NFC characters
    /// participate, so a block's id survives an edit to its tail. The bbox is rounded to
    /// whole points before hashing, so sub-point jitter between two extractions of the
    /// same PDF does not renumber the document.
    ///
    /// The result always carries collision counter zero; [`BlockId::with_collision_suffix`]
    /// produces the variants used when two blocks derive the same id.
    pub fn derive(page_index: u32, bbox: Rect, text: &str) -> Self {
        let mut hasher = blake3::Hasher::new();

        // Fixed-width fields first, so no separator is needed before the variable-width text.
        hasher.update(&page_index.to_le_bytes());
        for coordinate in [bbox.x0, bbox.y0, bbox.x1, bbox.y1] {
            hasher.update(&round_to_point(coordinate).to_le_bytes());
        }

        let prefix: String = text.nfc().take(TEXT_PREFIX_CHARS).collect();
        hasher.update(prefix.as_bytes());

        let encoded = base32::encode(BASE32, hasher.finalize().as_bytes());

        let mut bytes = [BASE32_ALPHABET[0]; ID_LEN];
        bytes[..HASH_CHARS].copy_from_slice(&encoded.as_bytes()[..HASH_CHARS]);
        Self(bytes)
    }

    /// The `n`-th variant of this id, used when two blocks derive the same id.
    ///
    /// The counter is the final character, so every variant differs from the base id and
    /// from every other variant by construction rather than by luck — distinctness here is
    /// an invariant, not a probability. `n` is taken modulo the alphabet size, giving 31
    /// alternatives to a base id; `n = 0` is the base id itself.
    pub fn with_collision_suffix(self, n: u8) -> Self {
        let mut bytes = self.0;
        bytes[ID_LEN - 1] = BASE32_ALPHABET[usize::from(n) % BASE32_ALPHABET.len()];
        Self(bytes)
    }

    /// The id as base32 text, for JSON, XHTML ids and overrides keys.
    pub fn as_str(&self) -> &str {
        let decoded = core::str::from_utf8(&self.0);
        debug_assert!(
            decoded.is_ok(),
            "a BlockId only ever holds base32 alphabet bytes"
        );
        decoded.unwrap_or(MALFORMED_ID)
    }
}

/// Returned by [`BlockId::as_str`] on an unreachable path; `unwrap`/`expect` are banned
/// outside tests and `main` (IMPLEMENTATION_PLAN §0.1), and a sentinel that is not a legal
/// id is more useful in a bug report than a panic in the middle of a conversion.
const MALFORMED_ID: &str = "!MALFORMED";

impl core::fmt::Debug for BlockId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "BlockId({})", self.as_str())
    }
}

impl core::fmt::Display for BlockId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Round a coordinate to whole points for hashing (D13.3, "bbox rounded to 1 pt").
///
/// The saturating float-to-int cast keeps this total: a non-finite coordinate cannot reach
/// a well-formed IR, but it must not be able to make id derivation panic either.
fn round_to_point(v: f32) -> i32 {
    v.round() as i32
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Test names are exactly the ones
// listed in the Phase 0 "Tests to write FIRST" table, rows 0.1 and 0.2.
// ---------------------------------------------------------------------------

/// The committed id for the canonical Phase-0 input. Filled in from the first green run of
/// `block_id_is_stable_for_same_inputs` and thereafter treated as a golden value: if this
/// assertion ever fails, the derivation changed and `IR_VERSION` must change with it.
#[cfg(test)]
const GOLDEN_BLOCK_ID_CHAPTER_3: &str = "SDMLH752SA";

#[cfg(test)]
fn golden_rect() -> crate::geom::Rect {
    crate::geom::Rect {
        x0: 72.0,
        y0: 96.5,
        x1: 340.25,
        y1: 118.0,
    }
}

#[test]
fn block_id_is_stable_for_same_inputs() {
    use crate::geom::Rect;
    use crate::ids::BlockId;

    let r = golden_rect();

    // 1. The derivation is a pure function of its three inputs.
    assert_eq!(
        BlockId::derive(3, r, "Chapter 3"),
        BlockId::derive(3, r, "Chapter 3")
    );

    // 2. It equals the committed constant.
    assert_eq!(
        BlockId::derive(3, r, "Chapter 3").as_str(),
        GOLDEN_BLOCK_ID_CHAPTER_3,
        "block id derivation changed; bump IR_VERSION (D13.3) before updating this constant"
    );

    // 3. The id is 10 base32 characters.
    assert_eq!(BlockId::derive(3, r, "Chapter 3").as_str().len(), 10);

    // 4. Changing any one input changes the id.
    let base = BlockId::derive(3, r, "Chapter 3");
    assert_ne!(
        base,
        BlockId::derive(4, r, "Chapter 3"),
        "page index is not in the hash input"
    );
    assert_ne!(
        base,
        BlockId::derive(3, Rect { x0: 73.0, ..r }, "Chapter 3"),
        "bbox is not in the hash input"
    );
    assert_ne!(
        base,
        BlockId::derive(3, r, "Chapter 4"),
        "text is not in the hash input"
    );

    // 5. The bbox enters the hash rounded to 1 pt, so sub-point jitter is absorbed but a
    //    whole-point move is not (D13.3).
    assert_eq!(
        base,
        BlockId::derive(3, Rect { x0: 72.4, ..r }, "Chapter 3"),
        "bbox must be rounded to 1 pt before hashing"
    );

    // 6. Only the first 64 NFC characters of the text participate.
    let long_a = format!("{}{}", "x".repeat(64), "AAAA");
    let long_b = format!("{}{}", "x".repeat(64), "BBBB");
    assert_eq!(
        BlockId::derive(3, r, &long_a),
        BlockId::derive(3, r, &long_b),
        "only the first 64 characters may enter the hash input"
    );
}

#[cfg(test)]
proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(10_000))]

    #[test]
    fn prop_block_id_collision_suffix_is_unique(
        page in 0u32..10_000,
        x0 in -2_000.0f32..2_000.0,
        y0 in -2_000.0f32..2_000.0,
        w in 0.0f32..2_000.0,
        h in 0.0f32..2_000.0,
        text in ".{0,200}",
    ) {
        use std::collections::BTreeSet;

        use crate::geom::Rect;
        use crate::ids::BlockId;

        let r = Rect { x0, y0, x1: x0 + w, y1: y0 + h };
        let base = BlockId::derive(page, r, &text);

        // The base id and every collision-suffixed variant are pairwise distinct: that is the
        // whole point of the suffix, and it is what makes the id usable as an overrides key.
        let family: Vec<BlockId> =
            std::iter::once(base).chain((1u8..=8).map(|n| base.with_collision_suffix(n))).collect();
        let distinct: BTreeSet<BlockId> = family.iter().copied().collect();
        proptest::prop_assert_eq!(distinct.len(), family.len());

        // Every member is still a well-formed 10-character id.
        for id in &family {
            proptest::prop_assert_eq!(id.as_str().len(), 10);
        }
    }
}

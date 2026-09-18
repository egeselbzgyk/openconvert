//! Row 6.16: duplicated paragraphs are detected, at the scale that matters.
//!
//! Two statistics are in play and they answer different questions, so the property is stated
//! against both:
//!
//! - **`quality.dup_para_frac`** is the Gopher/MassiveText repetition statistic, over the
//!   container's text. Its bound, `quality.dup_para_frac = 0.30`, comes from `datatrove`, where it
//!   routes a *source* document to review. Reaching it takes a book that is nearly half repetition.
//! - **`DuplicateStats::frac`** is over the emitted **blocks**, with `validate.dup_block_frac` at
//!   0.02. A block emitted twice is a pipeline bug rather than a property of the book — the case
//!   that motivated it duplicated 21 526 characters of a real 500-page book — so its bound is two
//!   orders of magnitude tighter.
//!
//! The plan's row reads "injecting a duplicated paragraph raises `dup_para_frac` above 0.30",
//! which is true of a short document and arithmetically impossible for a long one: one repeat in
//! `n + 1` paragraphs is `1/(n + 1)`. The property therefore asserts the exact identity —
//! `k / (n + k)` — and that crossing 0.30 is exactly when the Gopher verdict turns `Suspicious`,
//! which is the claim the row is making about the statistic.

use oc_core::thresholds::T;
use oc_text::stats::{quality_stats, Region, Verdict};
use oc_validate::blocks::Block;
use oc_validate::structural::duplicate_stats;

use proptest::prelude::*;

/// `n` distinct paragraphs, then the first `k` of them again.
fn paragraphs(n: usize, k: usize) -> Vec<String> {
    let distinct: Vec<String> = (0..n)
        .map(|index| format!("Paragraph number {index} of this book, long enough to be one."))
        .collect();
    let mut all = distinct.clone();
    all.extend(distinct.into_iter().take(k));
    all
}

fn blocks_of(paragraphs: &[String]) -> Vec<Block> {
    paragraphs
        .iter()
        .map(|text| Block {
            path: "text/c1.xhtml".to_owned(),
            tag: "p".to_owned(),
            heading_level: None,
            text: text.clone(),
        })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Row 6.16. Duplicating `k` of `n` paragraphs puts `dup_para_frac` at exactly `k / (n + k)`,
    /// and the Gopher verdict turns `Suspicious` exactly when that crosses `quality.dup_para_frac`.
    #[test]
    fn prop_duplicate_paragraph_detected(n in 1usize..40, k in 0usize..40) {
        prop_assume!(k <= n);

        let all = paragraphs(n, k);
        let text = all.join("\n\n");
        let stats = quality_stats(&text, oc_model::lang::LangTag::EN, Region::Document, None);

        let expected = k as f32 / (n + k) as f32;
        prop_assert!(
            (stats.dup_para_frac - expected).abs() < 1e-5,
            "n {}, k {}: dup_para_frac {} is not {}",
            n, k, stats.dup_para_frac, expected
        );

        let bound = T.quality.dup_para_frac as f32;
        let over = stats.dup_para_frac > bound;
        // Above the bound the Gopher family calls the text suspicious. Below it, the *n*-gram
        // statistics may still fire on this generator's near-identical sentences, so the
        // one-directional claim is the one that is about duplication.
        if over {
            prop_assert_eq!(
                stats.verdict(&T),
                Verdict::Suspicious,
                "n {}, k {}: dup_para_frac {} is over {} and the verdict is not Suspicious",
                n, k, stats.dup_para_frac, bound
            );
        }

        // The block statistic is the same quantity over the emitted blocks, and it is the one the
        // structural validator flags on, at two orders of magnitude tighter a bound.
        let blocks = duplicate_stats(&blocks_of(&all));
        prop_assert_eq!(blocks.duplicates as usize, k);
        prop_assert!(
            (blocks.frac() - expected).abs() < 1e-5,
            "n {}, k {}: block frac {} is not {}",
            n, k, blocks.frac(), expected
        );
    }
}

/// The plan's row, at the scale it holds: two paragraphs, one of them repeated, is 0.33 and over
/// the Gopher bound. Written out separately because the property above proves the identity and
/// this proves the row.
#[test]
fn one_duplicated_paragraph_in_a_short_document_crosses_the_gopher_bound() {
    let all = paragraphs(2, 1);
    let stats = quality_stats(
        &all.join("\n\n"),
        oc_model::lang::LangTag::EN,
        Region::Document,
        None,
    );

    assert!(
        stats.dup_para_frac > T.quality.dup_para_frac as f32,
        "dup_para_frac {} is not over {}",
        stats.dup_para_frac,
        T.quality.dup_para_frac
    );
    assert_eq!(stats.verdict(&T), Verdict::Suspicious);
}

/// And the case the block statistic exists for: one repeat in a hundred blocks is 1 %, invisible
/// to a 0.30 bound and over the 0.02 one. This is the *AI Engineering* shape — a table detector
/// claiming text that stayed in the flow — at the scale a real book has.
#[test]
fn one_duplicated_block_in_a_long_book_is_invisible_to_gopher_and_not_to_the_block_bound() {
    let all = paragraphs(100, 1);
    let stats = quality_stats(
        &all.join("\n\n"),
        oc_model::lang::LangTag::EN,
        Region::Document,
        None,
    );
    assert!(
        stats.dup_para_frac < T.quality.dup_para_frac as f32,
        "one repeat in a hundred is far under the datatrove bound: {}",
        stats.dup_para_frac
    );

    let blocks = duplicate_stats(&blocks_of(&all));
    assert_eq!(blocks.duplicates, 1);
    assert!(
        f64::from(blocks.frac()) < T.validate.dup_block_frac,
        "0.0099 is just under the 0.02 block bound: {}",
        blocks.frac()
    );

    // Three repeats is over it, which is the resolution the bound buys.
    let three = duplicate_stats(&blocks_of(&paragraphs(100, 3)));
    assert!(
        f64::from(three.frac()) > T.validate.dup_block_frac,
        "{}",
        three.frac()
    );
}

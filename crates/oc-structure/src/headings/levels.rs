//! Which candidates are headings, and at what level (PIPELINE §8.1, §8.2).
//!
//! Three sources, in order of how well they are measured:
//!
//! 1. **The PDF outline.** The one piece of structure a producer wrote on purpose. Each
//!    destination binds to the *nearest heading candidate*, never to the page — Calibre binds
//!    its TOC to page anchors, which is meaningless after reflow and is a long-standing
//!    concrete gap in the field (R1 §C.2 #9).
//! 2. **The printed contents page.** P_ED ≥ 0.9 median, the best of everything HiPS measured
//!    (R2 §B.5).
//! 3. **Size rank.** What is left, and what the ~25 % error ceiling is quoted against
//!    (GROBID 76.43 % F1; DocLayNet `Title` human agreement 60–72 %).
//!
//! Numbering refines whichever source decided: `1.2.3` is a level-three heading whatever it
//! is set in, and `Chapter 3` is level one. Then the tree is repaired so it has no level
//! skips, because an `h1 → h3` transition is invalid navigation in any reading system and
//! the repair is free.

use oc_core::thresholds::Thresholds;
use oc_model::confidence::{Confidence, Signal};
use oc_model::extract::OutlineEntry;
use oc_model::ids::{BlockId, ClusterId};
use oc_model::lang::LangTag;
use oc_text::fold::fold_key;
use oc_text::similarity::normalised_edit_distance;
use serde::Serialize;

use crate::headings::candidate::HeadingCandidate;
use crate::headings::cluster::StyleInventory;
use crate::headings::numbering::{self, Numbering};
use crate::headings::toc_page::TocPage;

/// Which source decided a heading's level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LevelSource {
    Outline,
    TocPage,
    SizeRank,
    /// The level a numbering pattern gave, overriding the source that proposed it.
    Numbering,
    /// A chapter opener: a numbered block found by its place on the page and its sequence
    /// through the book, whatever face it is set in (`headings::openers`).
    Sequence,
}

/// One block, decided to be a heading.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HeadingAssignment {
    pub block: BlockId,
    pub order: u32,
    pub page: u32,
    pub text: String,
    pub level: u8,
    pub cluster: ClusterId,
    pub numbering: Option<Numbering>,
    pub source: LevelSource,
}

/// Assign heading levels from whichever source is available (PIPELINE §8.1).
///
/// The returned `Confidence` is the *document's*, not a heading's: its signals are what
/// PIPELINE §8's confidence list names for this decision — the three-way agreement of
/// outline, TOC and clusters, and the numbering-regex coverage that the `heading_roles`
/// escalation predicate reads.
pub fn assign_levels(
    inventory: &StyleInventory,
    candidates: &[HeadingCandidate],
    outline: &[OutlineEntry],
    toc: Option<&TocPage>,
    lang: &LangTag,
    t: &Thresholds,
) -> (Vec<HeadingAssignment>, Confidence) {
    let admissible: Vec<&HeadingCandidate> = candidates
        .iter()
        .filter(|candidate| candidate.is_admissible())
        .collect();

    let ranks = cluster_ranks(inventory, t);
    let outline_entries = outline_titles(outline);
    let all: Vec<&HeadingCandidate> = candidates.iter().collect();
    let (by_outline, outline_bound) = bind(&admissible, &all, &outline_entries, lang, t);
    let by_toc = toc
        .map(|toc| {
            bind(
                &admissible,
                &all,
                &toc.entries
                    .iter()
                    .map(|entry| (entry.title.clone(), entry.level, None))
                    .collect::<Vec<_>>(),
                lang,
                t,
            )
            .0
        })
        .unwrap_or_default();

    // An invalid inventory forces size rank (detail 3) — unless the outline itself is found
    // on the pages: a technical book sets code, keys and emphasis in so many styles that its
    // inventory is invalid, and its bookmarks, which name nearly every heading it prints, were
    // ignored for a size rank that made the cover's lettering its first headings (2026-09-26).
    let distinct_titles = outline_entries
        .iter()
        .map(|(title, _, _)| fold_key(title, lang.clone()).to_string())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let use_outline = trust_outline(
        inventory.valid,
        OutlineFit {
            entries: outline_entries.len(),
            bound: outline_bound,
            distinct: distinct_titles,
        },
        !by_outline.is_empty(),
        t,
    );
    let use_toc = inventory.valid && !use_outline && !by_toc.is_empty();

    // The admissible candidates, and with a trusted outline also the blocks it bound that the
    // evidence alone did not admit: a chapter title set wide under its label, with no air of
    // its own, is the second half of a heading the outline names.
    let pool: Vec<&HeadingCandidate> = if use_outline {
        candidates
            .iter()
            .filter(|candidate| {
                candidate.is_admissible()
                    || by_outline
                        .iter()
                        .any(|bound| bound.block == candidate.block)
            })
            .collect()
    } else {
        admissible.clone()
    };
    let mut assignments: Vec<HeadingAssignment> = pool
        .iter()
        .filter_map(|candidate| {
            let (level, source) = if use_outline {
                let level = by_outline
                    .iter()
                    .find(|bound| bound.block == candidate.block)?
                    .level;
                (level, LevelSource::Outline)
            } else if use_toc {
                let level = by_toc
                    .iter()
                    .find(|bound| bound.block == candidate.block)?
                    .level;
                (level, LevelSource::TocPage)
            } else {
                let rank = ranks
                    .iter()
                    .find(|(cluster, _)| *cluster == candidate.cluster)
                    .map(|(_, tier)| *tier)?;
                // Size rank tells a chapter from a section and a section from a subsection; past
                // that it is reading type sizes a designer did not mean as levels, and a
                // navigation six deep is one nobody can use.
                let deepest = u8::try_from(t.headings.size_rank_max_level.clamp(1, 6)).unwrap_or(3);
                (
                    u8::try_from(rank + 1).unwrap_or(1).clamp(1, deepest),
                    LevelSource::SizeRank,
                )
            };

            let found = numbering::read(&candidate.text, lang);
            // Numbering refines, and only downwards into detail: a `1.2.3` under a heading
            // the outline called level one is a level-three heading, and a `Chapter 3` the
            // outline called level three is still where the outline put it. The outline knows
            // the book's own hierarchy; the numbering only knows its own depth.
            let (level, source) = match &found {
                Some(n) if source == LevelSource::SizeRank && n.level != level => {
                    (n.level, LevelSource::Numbering)
                }
                _ => (level, source),
            };

            Some(HeadingAssignment {
                block: candidate.block,
                order: candidate.order,
                page: candidate.page,
                text: candidate.text.clone(),
                level,
                cluster: candidate.cluster,
                numbering: found,
                source,
            })
        })
        .collect();

    assignments.sort_by_key(|assignment| assignment.order);
    remove_level_skips(&mut assignments);

    let numbered = assignments
        .iter()
        .filter(|assignment| assignment.numbering.is_some())
        .count();
    let coverage = share(numbered, assignments.len());
    let outline_match = share(by_outline.len(), admissible.len());
    let toc_match = share(by_toc.len(), admissible.len());

    let signals = vec![
        Signal::new("outline_match", outline_match),
        Signal::new("toc_match", toc_match),
        Signal::new("numbering_coverage", coverage),
        Signal::new("cluster_count", inventory.clusters.len() as f32),
        Signal::new("heading_count", assignments.len() as f32),
    ];
    let confidence = if use_outline || use_toc {
        Confidence::deterministic(signals)
    } else {
        // Size rank is the fallback the other two fall back *to*, and a report that cannot
        // tell "the producer said so" from "we ranked the sizes" cannot be audited (D13.5).
        Confidence::fallback(signals)
    };
    (assignments, confidence)
}

/// A binding of one source entry to one candidate block.
struct Bound {
    block: BlockId,
    level: u8,
}

/// The candidate clusters in size-rank order — the h1…h6 ladder.
/// Each candidate cluster's tier: clusters whose sizes differ by less than
/// `headings.level_size_tolerance` of the larger are one level, however many distinct sizes a
/// noisy text layer reported for it. A scan's text layer sets one chapter title at 14.2, the
/// next at 14.6 and the third at 15.1; ranked one by one they were three levels of a tree that
/// has one. Clusters are taken in rank order (largest first, bold before regular at a size), so
/// the tier numbers are the levels, top first.
fn cluster_ranks(inventory: &StyleInventory, t: &Thresholds) -> Vec<(ClusterId, usize)> {
    let tolerance = t.headings.level_size_tolerance as f32;
    let mut tiers: Vec<(ClusterId, usize)> = Vec::new();
    let mut tier = 0usize;
    let mut top_of_tier: Option<(f32, bool)> = None;
    for id in inventory.candidates(t) {
        let Some(cluster) = inventory.clusters.iter().find(|cluster| cluster.id == id) else {
            continue;
        };
        let bold = cluster.is_bold(t);
        if let Some((size, tier_bold)) = top_of_tier {
            let apart = size > 0.0 && (size - cluster.size_pt) / size > tolerance;
            // At one size, bold over regular is a level step; within the tolerance of a size it
            // is the same heading set by a scan that could not tell.
            let weight_step = tier_bold && !bold && (size - cluster.size_pt).abs() < f32::EPSILON;
            if apart || weight_step {
                tier += 1;
                top_of_tier = Some((cluster.size_pt, bold));
            }
        } else {
            top_of_tier = Some((cluster.size_pt, bold));
        }
        tiers.push((id, tier));
    }
    tiers
}

/// Outline entries as `(title, level)`, with the 0-based depth turned into a 1-based level.
fn outline_titles(outline: &[OutlineEntry]) -> Vec<(String, u8, Option<u32>)> {
    outline
        .iter()
        .map(|entry| {
            (
                entry.title.clone(),
                u8::try_from(entry.level.saturating_add(1))
                    .unwrap_or(1)
                    .clamp(1, 6),
                entry.page,
            )
        })
        .collect()
}

/// Bind each source entry to the nearest heading candidate by text (PIPELINE §8.1).
///
/// "Nearest" is by *text*, not by page: the destination's page narrows the search and does
/// not decide it, because a destination points at the top of the page a heading is on and a
/// page may carry two headings. Each candidate is bound at most once, in source order, so a
/// book whose chapters repeat a title does not bind them all to the first occurrence.
/// How far, in pages, a heading may sit from the page its outline entry points at: the entry
/// may point at the top of the page before a heading set at the foot of it.
const OUTLINE_PAGE_SLACK: u32 = 1;

/// How an outline fits the book: its entries, how many bound to headings on the pages, and
/// how many distinct titles it has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutlineFit {
    pub entries: usize,
    pub bound: usize,
    pub distinct: usize,
}

/// Whether the outline's levels are the book's: always when the typography is a valid
/// inventory and the outline bound anywhere; otherwise only when it bound at least
/// `headings.outline_trust_min_bound_share` of its entries to headings on the pages and names
/// its entries with at least `headings.outline_trust_min_distinct_share` distinct titles.
///
/// The second test is for the outline a converter made from the book's bold words: every
/// `Solution` and `Example` of a textbook, at levels that follow nothing, which binds well —
/// the words are on the pages — and is no hierarchy (2026-09-26: a textbook lost 90 of its 125
/// section headings to one).
pub fn trust_outline(
    inventory_valid: bool,
    fit: OutlineFit,
    any_bound: bool,
    t: &Thresholds,
) -> bool {
    if !any_bound || fit.entries == 0 {
        return false;
    }
    let entries = fit.entries as f64;
    inventory_valid
        || (fit.bound as f64 >= t.headings.outline_trust_min_bound_share * entries
            && fit.distinct as f64 >= t.headings.outline_trust_min_distinct_share * entries)
}

/// Bind each entry to the heading candidate that prints it, and say how many entries bound.
///
/// A title may be printed as two blocks that follow each other on one page — `CHAPTER 1` over
/// `Trade-Offs`, for an entry `Chapter 1. Trade-Offs` — and the two are tried together as well
/// as each alone; a pair that matches binds both, at the entry's level, and the heading join
/// makes them one heading again.
fn bind(
    candidates: &[&HeadingCandidate],
    all: &[&HeadingCandidate],
    entries: &[(String, u8, Option<u32>)],
    lang: &LangTag,
    t: &Thresholds,
) -> (Vec<Bound>, usize) {
    let mut ordered: Vec<&HeadingCandidate> = candidates.to_vec();
    ordered.sort_by_key(|candidate| candidate.order);
    // Each single candidate, and each with the block right after it on its page — admissible
    // or not, because a title set wide under its label has no air of its own.
    let mut options: Vec<(Vec<&HeadingCandidate>, String)> = ordered
        .iter()
        .map(|candidate| {
            (
                vec![*candidate],
                fold_key(&candidate.text, lang.clone()).to_string(),
            )
        })
        .collect();
    for first in &ordered {
        let Some(next) = all
            .iter()
            .find(|next| next.page == first.page && next.order == first.order + 1)
        else {
            continue;
        };
        let text = format!("{} {}", first.text, next.text);
        options.push((
            vec![*first, *next],
            fold_key(&text, lang.clone()).to_string(),
        ));
    }

    let mut taken: Vec<BlockId> = Vec::new();
    let mut bound = Vec::new();
    let mut entries_bound = 0;
    for (title, level, page) in entries {
        let key = fold_key(title, lang.clone());
        let best = options
            .iter()
            .filter(|(blocks, _)| blocks.iter().all(|block| !taken.contains(&block.block)))
            // An entry that says where it points is found there: the same title printed on
            // the contents page, or in a running head, is not the heading it names.
            .filter(|(blocks, _)| {
                page.is_none_or(|page| blocks[0].page.abs_diff(page) <= OUTLINE_PAGE_SLACK)
            })
            .map(|(blocks, text)| (blocks, normalised_edit_distance(&key, text)))
            .filter(|(_, distance)| f64::from(*distance) <= t.headings.outline_match_ned_max)
            // Ties go to the earlier candidate in reading order, which is what "the next
            // occurrence of this title" means in a book that repeats one — and then to the
            // single block over the pair.
            .min_by(|(left, left_distance), (right, right_distance)| {
                left_distance
                    .total_cmp(right_distance)
                    .then(left[0].order.cmp(&right[0].order))
                    .then(left.len().cmp(&right.len()))
            });
        if let Some((blocks, _)) = best {
            entries_bound += 1;
            for block in blocks {
                taken.push(block.block);
                bound.push(Bound {
                    block: block.block,
                    level: *level,
                });
            }
        }
    }
    (bound, entries_bound)
}

/// Compress the level tree so it has no skips: an `h1` followed by an `h3` becomes an `h2`.
///
/// Not a cosmetic repair. A skipped level is invalid navigation in every reading system, it
/// is one of the things PIPELINE §8's end-of-stage validation checks for, and the repair is
/// free because the *order* of the levels carries all the information the numbers do.
/// Add the chapter openers the other sources did not already make headings.
///
/// An opener is a chapter: level one, unless the book's own headings already have a level-one
/// tier above it — a book in parts — in which case it sits one below the top. The tree is then
/// repaired as every other source's is.
pub fn with_openers(
    mut assignments: Vec<HeadingAssignment>,
    openers: &[crate::headings::openers::Opener],
    cluster_of: impl Fn(BlockId) -> ClusterId,
) -> Vec<HeadingAssignment> {
    if openers.is_empty() {
        return assignments;
    }
    let known: std::collections::BTreeSet<BlockId> = assignments
        .iter()
        .map(|assignment| assignment.block)
        .collect();
    // A book whose size-ranked headings are fewer than its numbered chapters has used its top
    // tier for something above them only if that tier sits *between* the openers, like parts.
    let first = openers.first().map_or(0, |opener| opener.order);
    let parts = assignments
        .iter()
        .filter(|assignment| assignment.level == 1 && assignment.order > first)
        .count();
    let level = if parts > 0 && parts < openers.len() {
        2
    } else {
        1
    };
    for opener in openers {
        if known.contains(&opener.block) {
            continue;
        }
        assignments.push(HeadingAssignment {
            block: opener.block,
            order: opener.order,
            page: opener.page,
            text: opener.text.clone(),
            level,
            cluster: cluster_of(opener.block),
            numbering: None,
            source: LevelSource::Sequence,
        });
    }
    assignments.sort_by_key(|assignment| assignment.order);
    remove_level_skips(&mut assignments);
    assignments
}

/// Close every level skip in a heading list already in reading order, as every source's list
/// is closed: an `h1 → h3` transition is invalid navigation in any reading system.
pub fn repair_levels(assignments: &mut [HeadingAssignment]) {
    remove_level_skips(assignments);
}

fn remove_level_skips(assignments: &mut [HeadingAssignment]) {
    let mut previous = 0u8;
    for assignment in assignments.iter_mut() {
        if assignment.level > previous.saturating_add(1) {
            assignment.level = previous.saturating_add(1);
        }
        previous = assignment.level;
    }
}

/// What becomes of a heading the heading-roles task said is not one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HeadingDemotion {
    /// An ordinary paragraph.
    Paragraph,
    /// An epigraph: the paragraph, wrapped.
    Epigraph,
}

/// The heading-roles task's edit, in this crate's terms (PHASE 10 detail 3).
///
/// Levels and demotions, keyed by cluster. There is no way to say "remove": label authority is
/// not deletion authority (D13.5), and the type is where that is enforced.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HeadingEdits {
    pub levels: std::collections::BTreeMap<ClusterId, u8>,
    pub demote: std::collections::BTreeMap<ClusterId, HeadingDemotion>,
}

impl HeadingEdits {
    pub fn is_empty(&self) -> bool {
        self.levels.is_empty() && self.demote.is_empty()
    }
}

/// Apply the heading-roles edit to the levels `assign_levels` produced.
///
/// Only a heading whose level came from **size rank** is touched: size rank is the fallback the
/// task stands in for (ARCHITECTURE §6.1). A level the outline or the printed contents page
/// bound, or that a numbering pattern refined, is the book's own statement and stands.
///
/// A demoted heading leaves the heading list — its block then flows as the paragraph it is,
/// through the same path every other paragraph takes — and an epigraph's block is returned so the
/// stage can wrap it. The skip repair runs again, so a model's levels meet the same rule the
/// deterministic ones did.
pub fn apply_heading_edits(
    assignments: Vec<HeadingAssignment>,
    edits: &HeadingEdits,
) -> (Vec<HeadingAssignment>, std::collections::BTreeSet<BlockId>) {
    let mut epigraphs = std::collections::BTreeSet::new();
    if edits.is_empty() {
        return (assignments, epigraphs);
    }
    let mut kept = Vec::with_capacity(assignments.len());
    for mut assignment in assignments {
        if assignment.source == LevelSource::SizeRank {
            match edits.demote.get(&assignment.cluster) {
                Some(HeadingDemotion::Paragraph) => continue,
                Some(HeadingDemotion::Epigraph) => {
                    epigraphs.insert(assignment.block);
                    continue;
                }
                None => {}
            }
            if let Some(level) = edits.levels.get(&assignment.cluster) {
                assignment.level = oc_model::doc::Heading::clamp_level(*level);
            }
        }
        kept.push(assignment);
    }
    remove_level_skips(&mut kept);
    (kept, epigraphs)
}

fn share(numerator: usize, denominator: usize) -> f32 {
    if denominator == 0 {
        return 0.0;
    }
    (numerator as f64 / denominator as f64) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assignment(level: u8, order: u32) -> HeadingAssignment {
        HeadingAssignment {
            block: BlockId::derive(
                0,
                oc_model::geom::Rect {
                    x0: 0.0,
                    y0: f32::from(u8::try_from(order).unwrap_or(0)),
                    x1: 1.0,
                    y1: 1.0,
                },
                "h",
            ),
            order,
            page: 0,
            text: format!("Heading {order}"),
            level,
            cluster: ClusterId(0),
            numbering: None,
            source: LevelSource::SizeRank,
        }
    }

    fn candidate(order: u32, page: u32, text: &str) -> HeadingCandidate {
        HeadingCandidate {
            block: BlockId::derive(
                page,
                oc_model::geom::Rect {
                    x0: 0.0,
                    y0: f32::from(u8::try_from(order).unwrap_or(0)),
                    x1: 1.0,
                    y1: 1.0,
                },
                text,
            ),
            order,
            page,
            text: text.to_owned(),
            cluster: ClusterId(1),
            width_ratio: 0.3,
            short_line: true,
            sentence_continuing: false,
            space_above: true,
            legible: true,
        }
    }

    /// An entry printed as a label over a title binds both blocks at its level; the cover's
    /// lettering, which no entry names, binds nothing.
    #[test]
    fn an_outline_entry_binds_a_label_and_the_title_under_it() {
        let t = &oc_core::thresholds::T;
        let blocks = [
            candidate(0, 0, "2"),
            candidate(1, 0, "n d"),
            candidate(2, 3, "Preface"),
            candidate(3, 9, "CHAPTER 1"),
            candidate(4, 9, "Trade-Offs in Data Systems Architecture"),
            candidate(5, 10, "Operational Versus Analytical Systems"),
        ];
        let mut title = candidate(4, 9, "Trade-Offs in Data Systems Architecture");
        // Set wide under its label, with no air of its own: not admissible alone.
        title.short_line = false;
        title.space_above = false;
        let blocks = [
            blocks[0].clone(),
            blocks[1].clone(),
            blocks[2].clone(),
            blocks[3].clone(),
            title,
            blocks[5].clone(),
            // The contents page, before the chapter, lists it too.
            candidate(6, 1, "Preface"),
        ];
        let admissible: Vec<&HeadingCandidate> = blocks
            .iter()
            .filter(|candidate| candidate.is_admissible())
            .collect();
        let all: Vec<&HeadingCandidate> = blocks.iter().collect();
        let entries = vec![
            ("Preface".to_owned(), 1, Some(3)),
            (
                "Chapter 1. Trade-Offs in Data Systems Architecture".to_owned(),
                1,
                Some(9),
            ),
            (
                "Operational Versus Analytical Systems".to_owned(),
                2,
                Some(10),
            ),
        ];
        let (bound, entries_bound) = bind(&admissible, &all, &entries, &LangTag::EN, t);
        assert_eq!(entries_bound, 3);
        let levels: Vec<(String, u8)> = bound
            .iter()
            .map(|bound| {
                let text = blocks
                    .iter()
                    .find(|candidate| candidate.block == bound.block)
                    .map(|candidate| candidate.text.clone())
                    .unwrap_or_default();
                (text, bound.level)
            })
            .collect();
        assert_eq!(
            levels,
            vec![
                ("Preface".to_owned(), 1),
                ("CHAPTER 1".to_owned(), 1),
                ("Trade-Offs in Data Systems Architecture".to_owned(), 1),
                ("Operational Versus Analytical Systems".to_owned(), 2),
            ]
        );
    }

    /// An outline found on the pages is trusted over typography too varied to rank; one that
    /// binds a few entries by chance is not.
    #[test]
    fn a_well_bound_outline_is_trusted_whatever_the_typography() {
        let t = &oc_core::thresholds::T;
        let fit = |bound, distinct| OutlineFit {
            entries: 100,
            bound,
            distinct,
        };
        assert!(trust_outline(true, fit(1, 100), true, t));
        assert!(trust_outline(false, fit(90, 95), true, t));
        assert!(!trust_outline(false, fit(10, 100), true, t));
        // An outline made of the book's bold words: it binds, and says the same few titles
        // over and over.
        assert!(!trust_outline(false, fit(95, 40), true, t));
        assert!(!trust_outline(
            false,
            OutlineFit {
                entries: 0,
                bound: 0,
                distinct: 0
            },
            false,
            t
        ));
    }

    /// The repair, in isolation: every skip closes, and a level that *descends* by more than
    /// one is left alone, because coming back up two levels is legal navigation.
    #[test]
    fn a_skipped_level_is_closed_and_a_descent_is_not() {
        let mut tree = vec![
            assignment(1, 0),
            assignment(3, 1),
            assignment(4, 2),
            assignment(1, 3),
            assignment(6, 4),
        ];
        remove_level_skips(&mut tree);
        assert_eq!(
            tree.iter().map(|a| a.level).collect::<Vec<_>>(),
            vec![1, 2, 3, 1, 2]
        );
    }

    /// A document whose first heading is an `h3` starts at `h1`: there is nothing above it to
    /// be a child of.
    #[test]
    fn the_first_heading_is_always_level_one() {
        let mut tree = vec![assignment(4, 0), assignment(4, 1)];
        remove_level_skips(&mut tree);
        assert_eq!(tree.iter().map(|a| a.level).collect::<Vec<_>>(), vec![1, 2]);
    }
}

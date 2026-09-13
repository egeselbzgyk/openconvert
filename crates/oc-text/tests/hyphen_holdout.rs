//! Row 3.12: the classifier's keep-hyphen recall on a held-out set it never trained on.
//!
//! The gate is a *golden-decision* test in the sense of TEST_STRATEGY §7: a committed set of
//! labelled items, a committed model, and a number that may not fall. What makes it worth
//! having is that it is the only number in this project measuring the failure the conservation
//! law cannot see. A wrong join removes one character, records it, and leaves every invariant
//! balanced while the word is silently destroyed (RT A1).
//!
//! **Keep-recall, not accuracy.** About 98 % of line-break hyphens should be removed, so a
//! model that always joins scores 98.8 % and destroys every hyphenated word it meets. R2 §B.7
//! measures a dictionary-only baseline at 31.7 % keep-recall against a classifier's 85.8 %;
//! the floor here is the plan's 0.80, which is provisional and derived from that 85.8 %.
//!
//! The recall is measured **through the shipped decision rule**, margin included, rather than
//! on the raw sign of the score — because the margin only ever turns a join into a keep, and
//! what the gate is asked about is what the pipeline actually does.

use std::path::PathBuf;

use oc_core::thresholds::T;
use oc_model::lang::LangTag;
use oc_text::dehyphen::classifier::HyphenClassifier;
use oc_text::dehyphen::HyphenAction;

/// The plan's floor, and the row's own words: "keep-hyphen recall >= 0.80 **[provisional,
/// target from R2 §B.7's 85.8 %]**".
const KEEP_RECALL_FLOOR: f64 = 0.80;

struct Item {
    head: String,
    tail: String,
    lang: LangTag,
    keep: bool,
}

fn holdout() -> Vec<Item> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../eval/data/hyphen_holdout.jsonl");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "missing holdout {}: {error}; rebuild it with \
             `cd eval && PYTHONPATH=src python -m oc_eval.train.hyphen_clf`",
            path.display()
        )
    });
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let row: serde_json::Value =
                serde_json::from_str(line).expect("every holdout line is JSON");
            Item {
                head: row["head"].as_str().unwrap_or_default().to_owned(),
                tail: row["tail"].as_str().unwrap_or_default().to_owned(),
                lang: LangTag::new(row["lang"].as_str().unwrap_or("en")),
                keep: row["keep"].as_bool().unwrap_or(false),
            }
        })
        .collect()
}

#[test]
fn hyphen_classifier_keep_recall_on_holdout() {
    let model = HyphenClassifier::shipped().expect("the model blob is committed and valid");
    let items = holdout();
    assert!(
        items.len() >= 2000,
        "the plan asks for a 2,000-item held-out set; found {}",
        items.len()
    );

    let mut keeps = 0u32;
    let mut kept = 0u32;
    let mut joins = 0u32;
    let mut joined = 0u32;
    for item in &items {
        let decision = model.decide(&item.head, &item.tail, &item.lang, &T);
        let action = decision.resolved();
        if item.keep {
            keeps += 1;
            kept += u32::from(action == HyphenAction::Keep);
        } else {
            joins += 1;
            joined += u32::from(action == HyphenAction::Join);
        }
    }

    assert!(keeps >= 100, "a recall over {keeps} items is noise");
    let keep_recall = f64::from(kept) / f64::from(keeps);
    let join_recall = f64::from(joined) / f64::from(joins);
    assert!(
        keep_recall >= KEEP_RECALL_FLOOR,
        "keep-hyphen recall {keep_recall:.3} over {keeps} items is below the {KEEP_RECALL_FLOOR} \
         floor; join-recall {join_recall:.3} over {joins}"
    );

    // The other side of the trade, asserted so that a future model cannot buy keep-recall by
    // keeping everything: a model that never joins is useless in a different direction.
    assert!(
        join_recall >= 0.5,
        "join-recall {join_recall:.3} has collapsed; the model keeps almost everything"
    );
}

/// The model and the holdout are two halves of one artefact, and a model rebuilt without its
/// holdout — or the other way round — is a gate measuring nothing. The manifest records the
/// numbers the trainer measured; this checks the committed pair still reproduces them.
#[test]
fn the_training_manifest_matches_the_committed_model() {
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/dehyphen/model.sources.json"),
        )
        .expect("the training manifest is committed"),
    )
    .expect("the manifest is JSON");

    let model = HyphenClassifier::shipped().expect("the model is readable");
    assert_eq!(
        manifest["buckets"].as_u64().unwrap_or_default() as usize,
        model.buckets(),
        "the manifest and the blob disagree about the model's shape"
    );
    assert_eq!(
        manifest["counts"]["holdout"].as_u64().unwrap_or_default() as usize,
        holdout().len(),
        "the manifest and the committed holdout disagree about its size"
    );
    for source in manifest["sources"]
        .as_array()
        .expect("the manifest names its sources")
    {
        assert_eq!(
            source["license"].as_str(),
            Some("CC0-1.0"),
            "a training source is not CC0: {source}"
        );
    }
}

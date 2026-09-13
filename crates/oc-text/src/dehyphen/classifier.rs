//! The tiny classifier: what the tiers leave open (R2 §B.7, D13.6, plan Phase 3 detail 5).
//!
//! A kilobyte-scale logistic regression over character features, trained by
//! `eval/src/oc_eval/train/hyphen_clf.py` and committed next to its training manifest. There
//! is no inference library here and no model format to learn: eight thousand weights, a bias,
//! and a dot product.
//!
//! **Why it exists at all.** The deterministic tiers answer most breaks and refuse the rest,
//! and the refusals are not rare — a book's own vocabulary says nothing about a word it uses
//! once, and a frequency list that contains both `pipe` and `line` cannot tell you whether
//! `pipeline` is a word. On that residual a dictionary-only baseline keeps the right hyphens
//! **31.7 %** of the time; this kind of classifier keeps **85.8 %** of them, at no cost in raw
//! accuracy (R2 §B.7, 776,700 hyphenated words). Raw accuracy is the misleading number here:
//! about 98 % of line-break hyphens should be removed, so a model that always joins scores
//! 98.8 % and destroys every genuinely hyphenated word it meets.
//!
//! **The features have to match the trainer exactly**, so they are listed in the same order
//! and the same spellings in both places, and the hash is FNV-1a because it is four lines in
//! any language and can be checked by eye. A feature is a short string; its weight lives at
//! `fnv1a32(feature) % buckets`.
//!
//! **The margin is the fail-closed rule's last line.** A score inside
//! `dehyphen.classifier_margin_min` of zero is a coin toss, and a coin toss keeps the hyphen.

use oc_core::thresholds::Thresholds;
use oc_model::confidence::Signal;
use oc_model::lang::LangTag;

use super::{joined, Decision, HyphenAction, Tier};

/// The weights, compiled in. 32 KB, and the only model this project ships in v1.
const MODEL: &[u8] = include_bytes!("model.bin");

/// The file's magic, which is also its version: a format change changes these bytes, and a
/// model built by a different trainer will not be read by accident.
const MAGIC: &[u8; 8] = b"OCHYPH1\0";

/// Offset of the bucket count, then the bias, then the weights.
const BUCKETS_AT: usize = 8;
const BIAS_AT: usize = 12;
const WEIGHTS_AT: usize = 16;

/// FNV-1a, 32-bit, over UTF-8 bytes. The trainer computes exactly this.
fn fnv1a32(text: &str) -> u32 {
    let mut digest: u32 = 0x811C_9DC5;
    for byte in text.as_bytes() {
        digest ^= u32::from(*byte);
        digest = digest.wrapping_mul(0x0100_0193);
    }
    digest
}

/// The trained model, as read from the blob.
#[derive(Clone, Copy, Debug)]
pub struct HyphenClassifier {
    buckets: usize,
    bias: f32,
}

impl HyphenClassifier {
    /// The shipped model. `None` if the blob is not one this build understands, which is a
    /// packaging fault rather than a document fault — and it degrades to the fail-closed
    /// default rather than to a guess.
    pub fn shipped() -> Option<Self> {
        if MODEL.len() < WEIGHTS_AT || &MODEL[..MAGIC.len()] != MAGIC {
            return None;
        }
        let buckets = u32::from_le_bytes(MODEL[BUCKETS_AT..BIAS_AT].try_into().ok()?) as usize;
        if MODEL.len() < WEIGHTS_AT + buckets * 4 {
            return None;
        }
        let bias = f32::from_le_bytes(MODEL[BIAS_AT..WEIGHTS_AT].try_into().ok()?);
        Some(Self { buckets, bias })
    }

    /// How many weights the model carries.
    pub fn buckets(&self) -> usize {
        self.buckets
    }

    /// The log-odds that this hyphen should be **kept**.
    ///
    /// Positive keeps, negative joins, and the sign convention is the trainer's: the label is
    /// `keep`, so that the number the model is asked for is the one the fail-closed rule is
    /// about.
    pub fn score(&self, head: &str, tail: &str, lang: &LangTag) -> f32 {
        let mut total = self.bias;
        for feature in features(head, tail, lang) {
            total += self.weight(&feature);
        }
        total
    }

    /// The model's verdict, with the margin applied.
    pub fn decide(&self, head: &str, tail: &str, lang: &LangTag, t: &Thresholds) -> Decision {
        let score = self.score(head, tail, lang);
        let margin = t.dehyphen.classifier_margin_min as f32;
        let signals = vec![Signal::new("clf_score", score)];
        let action = if score >= margin {
            HyphenAction::Keep
        } else if score <= -margin {
            HyphenAction::Join
        } else {
            HyphenAction::Undecided
        };
        Decision::with_score(action, Tier::Classifier, signals, score)
    }

    fn weight(&self, feature: &str) -> f32 {
        let bucket = (fnv1a32(feature) as usize) % self.buckets;
        let at = WEIGHTS_AT + bucket * 4;
        MODEL
            .get(at..at + 4)
            .and_then(|bytes| bytes.try_into().ok())
            .map(f32::from_le_bytes)
            .unwrap_or(0.0)
    }
}

/// The feature strings for one break, in the trainer's order and spelling.
///
/// Character shape either side, case shape, bucketed lengths, and the language — the feature
/// set R2 §B.7 describes. Any change here is a change to `hyphen_clf.py` in the same commit,
/// and to the model it produced: a feature the trainer never saw hashes to a weight that was
/// fitted for something else.
fn features(head: &str, tail: &str, lang: &LangTag) -> Vec<String> {
    let head_lower = head.to_lowercase();
    let tail_lower = tail.to_lowercase();
    let mut out = vec![
        "bias".to_owned(),
        format!("lang={}", lang.primary()),
        format!("hl={}", head.chars().count().min(9)),
        format!("tl={}", tail.chars().count().min(9)),
        format!("hup={}", python_bool(starts_upper(head))),
        format!("tup={}", python_bool(starts_upper(tail))),
        format!(
            "hyp={}",
            python_bool(head.contains('-') || tail.contains('-'))
        ),
    ];
    for n in 1..=3usize {
        out.push(format!("h{n}={}", last_chars(&head_lower, n)));
        out.push(format!("t{n}={}", first_chars(&tail_lower, n)));
    }
    out.push(format!(
        "j={}|{}",
        first_chars(&last_chars(&head_lower, 1), 1),
        first_chars(&tail_lower, 1)
    ));
    out
}

/// Python spells its booleans with a capital, and the feature strings are hashed verbatim.
fn python_bool(value: bool) -> &'static str {
    if value {
        "True"
    } else {
        "False"
    }
}

fn starts_upper(word: &str) -> bool {
    word.chars().next().is_some_and(char::is_uppercase)
}

fn last_chars(word: &str, n: usize) -> String {
    let count = word.chars().count();
    word.chars().skip(count.saturating_sub(n)).collect()
}

fn first_chars(word: &str, n: usize) -> String {
    word.chars().take(n).collect()
}

/// The classifier tier, for `dehyphenate` to call when the tiers are done.
pub fn classify(head: &str, tail: &str, lang: &LangTag, t: &Thresholds) -> Option<Decision> {
    let model = HyphenClassifier::shipped()?;
    let decision = model.decide(head, tail, lang, t);
    let _ = joined(head, tail);
    Some(decision)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oc_core::thresholds::T;

    #[test]
    fn the_shipped_model_is_readable() {
        let model = HyphenClassifier::shipped().expect("the model blob is committed and valid");
        assert_eq!(model.buckets(), 8192);
    }

    /// The hash is the contract between this file and the trainer, and it is pinned to values
    /// anyone can recompute: FNV-1a of the empty string is its offset basis, and of "a" is one
    /// round of it.
    #[test]
    fn the_hash_is_the_one_the_trainer_uses() {
        assert_eq!(fnv1a32(""), 0x811C_9DC5);
        assert_eq!(fnv1a32("a"), 0xE40C_292C);
        assert_eq!(fnv1a32("foobar"), 0xBF9C_F968);
    }

    /// The feature list is the other half of that contract: same count, same order, same
    /// spelling, Python's capitalised booleans included.
    #[test]
    fn the_feature_set_matches_the_trainer() {
        let found = features("Pipe", "line", &LangTag::EN);
        assert_eq!(
            found,
            vec![
                "bias",
                "lang=en",
                "hl=4",
                "tl=4",
                "hup=True",
                "tup=False",
                "hyp=False",
                "h1=e",
                "t1=l",
                "h2=pe",
                "t2=li",
                "h3=ipe",
                "t3=lin",
                "j=e|l",
            ]
        );
    }

    /// A break the model is confident about resolves; one it is not stays undecided, and an
    /// undecided one keeps the hyphen.
    #[test]
    fn a_low_margin_is_undecided_and_therefore_keeps() {
        let model = HyphenClassifier::shipped().expect("the model is readable");
        let decision = model.decide("pipe", "line", &LangTag::EN, &T);
        assert!(matches!(
            decision.action,
            HyphenAction::Join | HyphenAction::Keep | HyphenAction::Undecided
        ));
        assert!(decision.confidence.score.is_some(), "the score is recorded");
        if decision.action == HyphenAction::Undecided {
            assert_eq!(decision.resolved(), HyphenAction::Keep);
        }
    }
}

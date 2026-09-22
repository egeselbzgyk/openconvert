//! Gate S: the answer parses, has the task's shape, uses only the task's values, and names exactly
//! the identifiers it was asked about (D13.5, ARCHITECTURE §6.2).
//!
//! Four checks, in the order they are cheapest to explain to a reader of the failure:
//!
//! 1. **No thinking.** Any `<think>` in the answer fails, before anything is parsed: D10 turns
//!    thinking off per request and asserts it absent in the output, and valid JSON after a
//!    thinking block is still an answer whose reasoning nobody asked for.
//! 2. **JSON, of the task's shape** — deserialised straight into the task's wire type, which
//!    denies unknown fields and, unlike a detour through `serde_json::Value`, notices a key said
//!    twice instead of letting the second silently win.
//! 3. **Closed sets.** Every enum value is one of the task's.
//! 4. **Bijection.** The identifiers answered are exactly the identifiers asked about, each once.
//!
//! Any failure rejects the **whole** answer, never a subset: a model that mislabelled one cluster
//! has shown it did not follow the question, and its other labels stop being evidence.
//!
//! What gate S does not check is anything a grammar-constrained decoder already made impossible
//! and a semantic rule that belongs to applying the answer — the verbatim-substring check on
//! metadata, the order of book-structure boundaries, verse's minimum line count. Those are task
//! validations (IMPLEMENTATION_PLAN Phase 10) and run after this gate, not instead of it.

use std::collections::{BTreeMap, BTreeSet};

use serde::de::DeserializeOwned;

use super::GateFailure;

/// A task's answer, as gate S admits it.
pub trait Answer: Sized {
    /// What the question was about: the identifiers the answer must name.
    type Context;
    /// The answer as JSON spells it, before any check of what it means.
    type Wire: DeserializeOwned;

    /// Checks 3 and 4: closed sets and bijection.
    fn check(wire: Self::Wire, context: &Self::Context) -> Result<Self, GateFailure>;
}

/// Gate S, over a model's raw output.
pub fn gate_schema<A: Answer>(raw: &str, context: &A::Context) -> Result<A, GateFailure> {
    if raw.contains("<think") || raw.contains("</think") {
        return Err(GateFailure::ThinkingPresent);
    }
    let wire: A::Wire = serde_json::from_str(raw).map_err(|error| {
        if error.classify() == serde_json::error::Category::Data {
            GateFailure::WrongShape(error.to_string())
        } else {
            GateFailure::Unparseable(error.to_string())
        }
    })?;
    A::check(wire, context)
}

/// Check that `answered` names each identifier of `asked` exactly once and nothing else.
///
/// Every identifier out of place is named, sorted, in the failure — a duplicate, a missing one and
/// an invented one are three different ways a model loses track, and a test or a log line reading
/// the failure should be able to tell which.
pub(crate) fn bijection<K>(
    what: &'static str,
    asked: impl IntoIterator<Item = K>,
    answered: impl IntoIterator<Item = K>,
) -> Result<(), GateFailure>
where
    K: Ord + ToString,
{
    let asked: BTreeSet<K> = asked.into_iter().collect();
    let mut counts: BTreeMap<K, usize> = BTreeMap::new();
    for id in answered {
        *counts.entry(id).or_default() += 1;
    }

    let duplicated: Vec<String> = counts
        .iter()
        .filter(|(_, count)| **count > 1)
        .map(|(id, _)| id.to_string())
        .collect();
    let missing: Vec<String> = asked
        .iter()
        .filter(|id| !counts.contains_key(id))
        .map(ToString::to_string)
        .collect();
    let invented: Vec<String> = counts
        .keys()
        .filter(|id| !asked.contains(id))
        .map(ToString::to_string)
        .collect();

    if duplicated.is_empty() && missing.is_empty() && invented.is_empty() {
        Ok(())
    } else {
        Err(GateFailure::NotBijective {
            what,
            duplicated,
            missing,
            invented,
        })
    }
}

/// A value from a closed set, or the failure that names the value and the field it was in.
pub(crate) fn closed<T>(
    field: &'static str,
    value: &str,
    parse: impl Fn(&str) -> Option<T>,
) -> Result<T, GateFailure> {
    parse(value).ok_or_else(|| GateFailure::OutOfEnum {
        field,
        value: value.to_owned(),
    })
}

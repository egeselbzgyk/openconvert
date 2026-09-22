//! Filling a `user.tmpl`: `{{name}}` slots, one pass, nothing rescanned.
//!
//! Deliberately not a template language. A slot is replaced by its value and the value is never
//! read again, so a book whose text says `{{payload}}` produces a message that says `{{payload}}`
//! — text from the document is data, and that holds for the renderer before it holds for the
//! model (Appendix A.1, rule 4). Every slot the template names must be given and every slot given
//! must be named: a template edit that dropped the payload would otherwise send a model a question
//! with nothing to answer about.

use serde::Serialize;

/// Why a message could not be rendered.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RenderError {
    #[error("the template names slot `{0}`, which was not given")]
    MissingSlot(String),
    #[error("slot `{0}` was given and the template never names it")]
    UnusedSlot(String),
    #[error("the template opens a slot with `{{{{` and never closes it")]
    Unclosed,
    /// JSON has no spelling for NaN or infinity, and serde writes `null` for them without saying
    /// so.
    #[error("{0} is not a finite number")]
    NonFinite(&'static str),
    #[error("the payload could not be written as JSON: {0}")]
    Json(String),
}

const OPEN: &str = "{{";
const CLOSE: &str = "}}";

/// `template` with every `{{name}}` replaced by its value from `slots`.
pub fn fill(template: &str, slots: &[(&str, &str)]) -> Result<String, RenderError> {
    let mut out = String::with_capacity(template.len());
    let mut used = vec![false; slots.len()];
    let mut rest = template;

    while let Some(open) = rest.find(OPEN) {
        out.push_str(&rest[..open]);
        let after = &rest[open + OPEN.len()..];
        let close = after.find(CLOSE).ok_or(RenderError::Unclosed)?;
        let name = after[..close].trim();
        let Some(index) = slots.iter().position(|(slot, _)| *slot == name) else {
            return Err(RenderError::MissingSlot(name.to_owned()));
        };
        out.push_str(slots[index].1);
        used[index] = true;
        rest = &after[close + CLOSE.len()..];
    }
    out.push_str(rest);

    if let Some(index) = used.iter().position(|used| !used) {
        return Err(RenderError::UnusedSlot(slots[index].0.to_owned()));
    }
    Ok(out)
}

/// A payload as compact JSON, fields in declaration order.
///
/// Compact because the payload is hashed into the cache key and read by a model: whitespace would
/// be bytes that mean nothing and tokens that cost something.
pub fn json(payload: &impl Serialize) -> Result<String, RenderError> {
    serde_json::to_string(payload).map_err(|error| RenderError::Json(error.to_string()))
}

/// `Err` naming `field` when `value` is not finite.
pub fn finite(field: &'static str, value: f32) -> Result<(), RenderError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(RenderError::NonFinite(field))
    }
}

#[test]
fn every_slot_is_filled_once_and_every_slot_is_accounted_for() {
    assert_eq!(
        fill("a {{x}} b {{ y }} c", &[("x", "1"), ("y", "{{x}}")]),
        Ok("a 1 b {{x}} c".to_owned())
    );
    assert_eq!(
        fill("{{x}}", &[]),
        Err(RenderError::MissingSlot("x".to_owned()))
    );
    assert_eq!(
        fill("no slots", &[("x", "1")]),
        Err(RenderError::UnusedSlot("x".to_owned()))
    );
    assert_eq!(fill("{{x", &[("x", "1")]), Err(RenderError::Unclosed));
}

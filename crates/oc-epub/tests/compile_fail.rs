//! Phase 5 rows 5.1 and 5.2: the two content-model violations that must not compile.
//!
//! These are the phase's central claim made checkable. A `<figure>` inside a `<p>` and an
//! `<a>` inside an `<a>` are both perfectly well-formed XML, so no parser and no
//! well-formedness check can see them — only a schema engine or EPUBCheck can, and by then the
//! book is written. Making them *unrepresentable* removes the entire error class (D5, RT A6),
//! and a claim like that is worth a test that fails when the types stop enforcing it.
//!
//! `trybuild` compares the compiler's message against a committed `.stderr`. Regenerate with
//! `TRYBUILD=overwrite cargo test -p oc-epub --test compile_fail` after a deliberate API
//! change, and review the diff — a changed message is a changed API.

/// Row 5.1. `El<Phrasing>` has no `figure`, because `Phrasing` does not implement
/// `FlowContext`.
#[test]
fn phrasing_cannot_contain_figure() {
    trybuild::TestCases::new().compile_fail("tests/ui/phrasing_cannot_contain_figure.rs");
}

/// Row 5.2. The inside of an `<a>` is `El<NoAnchor>`, which has neither `link` nor `noteref`.
#[test]
fn anchor_cannot_nest() {
    trybuild::TestCases::new().compile_fail("tests/ui/anchor_cannot_nest.rs");
}

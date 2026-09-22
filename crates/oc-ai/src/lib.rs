#![forbid(unsafe_code)]
//! The `LlmProvider` trait, prompts and GBNF grammars, the decision cache, the four
//! gates and test cassettes (D10, D13.5, D13.6).
//!
//! This crate opens no sockets: transport is a trait implemented by `oc-net` (D13.9).
//!
//! Nothing here decides *whether* to ask a model — that is the escalation predicate, evaluated
//! before any call by the stage that owns the evidence — and nothing here applies an answer to a
//! book. What this crate owns is the question (`prompt`), the shape an answer must have to be
//! considered at all (`gbnf`), and the name every piece of it hashes to (`digest`).

pub mod digest;
pub mod gates;
pub mod gbnf;
pub mod prompt;
pub mod provider;

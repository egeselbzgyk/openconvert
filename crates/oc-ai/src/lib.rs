#![forbid(unsafe_code)]
//! The `LlmProvider` trait, prompts and GBNF grammars, the decision cache, the four
//! gates and test cassettes (D10, D13.5, D13.6).
//!
//! This crate opens no sockets: transport is a trait implemented by `oc-net` (D13.9).

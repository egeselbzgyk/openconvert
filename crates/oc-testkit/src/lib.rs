#![forbid(unsafe_code)]
//! Test-only support: fixture builders, the golden-assertion runner, the structural
//! digest and a stub LLM server. Never a dependency of a shipped crate.

pub mod assertions;
pub mod handmade;

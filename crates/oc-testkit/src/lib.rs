#![forbid(unsafe_code)]
//! Test-only support: fixture builders, the golden-assertion runner, the structural
//! digest, a stub LLM server and a stub model host. Never a dependency of a shipped crate.

pub mod assertions;
pub mod download_stub;
pub mod fake_tesseract;
pub mod fuzz_props;
pub mod handmade;
pub mod hostile;
pub mod mutate;

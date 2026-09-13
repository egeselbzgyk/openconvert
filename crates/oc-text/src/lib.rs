#![forbid(unsafe_code)]
//! Text assembly and normalisation `N`, folding keys, dehyphenation, quality
//! statistics and language detection (D13.4).

pub mod fold;
pub mod lines;
pub mod normalize;
pub mod words;

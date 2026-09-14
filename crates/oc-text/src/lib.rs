#![forbid(unsafe_code)]
//! Text assembly and normalisation `N`, folding keys, dehyphenation, quality
//! statistics and language detection (D13.4).

pub mod compound_de;
pub mod dehyphen;
pub mod fold;
pub mod freq;
pub mod lang;
pub mod lines;
pub mod normalize;
pub mod similarity;
pub mod stats;
pub mod words;

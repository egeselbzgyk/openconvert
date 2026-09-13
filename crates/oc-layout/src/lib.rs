#![forbid(unsafe_code)]
//! Furniture detection, block segmentation, column detection, reading order,
//! paragraph reconstruction and image anchoring.

pub mod blocks;
pub mod columns;
pub mod continuity;
pub mod furniture;
pub mod paragraphs;
pub mod reading_order;

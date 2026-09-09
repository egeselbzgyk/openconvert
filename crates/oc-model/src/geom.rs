//! Geometry in the one normalised page space fixed at extraction: origin top-left,
//! y down, PDF points, after `/Rotate` and after subtracting the CropBox offset (D13.3).
//!
//! There is exactly one geometric space in the IR. Mixing CropBox and image space
//! silently deleted body text in a shipping 2026 tool (R1 §D.6 #1), which is why the
//! invariant is asserted at extraction rather than trusted.

/// An axis-aligned rectangle in normalised page space.
///
/// `x0`/`y0` is the top-left corner and `x1`/`y1` the bottom-right, so `y1 >= y0`
/// holds for a well-formed rectangle: y grows downwards.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct Rect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

//! The one normalised page space, and the only place a PDF user-space rectangle is
//! allowed to become an IR rectangle (D13.3, Phase 0 detail 5, RT D10).
//!
//! PDF user space has its origin at the bottom left with y growing upwards, is offset by
//! the CropBox, and is displayed rotated by `/Rotate`. The IR has exactly one space:
//! origin top-left, y down, points, after `/Rotate` and after the CropBox offset. Mixing
//! the two silently deleted body text in a shipping 2026 tool (R1 §D.6 #1), so the
//! conversion is a single named function with an asserted invariant rather than arithmetic
//! spread across call sites.

use oc_model::geom::Rect;

/// How far outside the page box a normalised rectangle may land before it is a bug.
///
/// One point: enough to absorb `f32` rounding and a producer that draws a hairline flush
/// with the crop edge, small enough that a transposed axis or a forgotten offset — which
/// misplaces content by tens or hundreds of points — cannot hide inside it.
pub const INSIDE_PAGE_TOLERANCE_PT: f32 = 1.0;

/// A rectangle in PDF user space: origin bottom-left, y **up**, points.
///
/// Deliberately not [`Rect`]. The two spaces are not interchangeable, and the type system
/// is the cheapest place to say so.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PdfRect {
    pub llx: f32,
    pub lly: f32,
    pub urx: f32,
    pub ury: f32,
}

/// A page's `/Rotate`, clockwise, as displayed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rotate {
    None,
    Cw90,
    Cw180,
    Cw270,
}

impl Rotate {
    /// Interpret a `/Rotate` value. The PDF specification requires a multiple of 90; it may
    /// be negative or beyond a full turn, and both occur in the wild.
    pub fn from_degrees(degrees: i32) -> Option<Self> {
        match degrees.rem_euclid(FULL_TURN_DEGREES) {
            0 => Some(Rotate::None),
            QUARTER_TURN_DEGREES => Some(Rotate::Cw90),
            HALF_TURN_DEGREES => Some(Rotate::Cw180),
            THREE_QUARTER_TURN_DEGREES => Some(Rotate::Cw270),
            _ => None,
        }
    }
}

const QUARTER_TURN_DEGREES: i32 = 90;
const HALF_TURN_DEGREES: i32 = 180;
const THREE_QUARTER_TURN_DEGREES: i32 = 270;
const FULL_TURN_DEGREES: i32 = 360;

/// The normalised space of one page, fixed once at extraction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageGeometry {
    crop: PdfRect,
    rotate: Rotate,
}

impl PageGeometry {
    pub fn new(crop: PdfRect, rotate: Rotate) -> Self {
        Self { crop, rotate }
    }

    /// Width of the page **as displayed**, so a quarter turn swaps it with the height.
    pub fn width_pt(&self) -> f32 {
        match self.rotate {
            Rotate::None | Rotate::Cw180 => self.crop_width(),
            Rotate::Cw90 | Rotate::Cw270 => self.crop_height(),
        }
    }

    /// The page's `/Rotate`, in degrees, for reports and snapshots.
    pub fn rotate_degrees(&self) -> u16 {
        match self.rotate {
            Rotate::None => 0,
            Rotate::Cw90 => QUARTER_TURN_DEGREES as u16,
            Rotate::Cw180 => HALF_TURN_DEGREES as u16,
            Rotate::Cw270 => THREE_QUARTER_TURN_DEGREES as u16,
        }
    }

    /// Height of the page as displayed.
    pub fn height_pt(&self) -> f32 {
        match self.rotate {
            Rotate::None | Rotate::Cw180 => self.crop_height(),
            Rotate::Cw90 | Rotate::Cw270 => self.crop_width(),
        }
    }

    fn crop_width(&self) -> f32 {
        self.crop.urx - self.crop.llx
    }

    fn crop_height(&self) -> f32 {
        self.crop.ury - self.crop.lly
    }

    /// Map a user-space rectangle into the normalised page space.
    ///
    /// Both corners are mapped and then re-ordered, because a rotation moves the lower-left
    /// corner somewhere else: taking the mapped corners as-is would produce inverted
    /// rectangles for two of the four rotations.
    pub fn normalise(&self, rect: PdfRect) -> Rect {
        let (ax, ay) = self.normalise_point(rect.llx, rect.lly);
        let (bx, by) = self.normalise_point(rect.urx, rect.ury);
        let mapped = Rect {
            x0: ax.min(bx),
            y0: ay.min(by),
            x1: ax.max(bx),
            y1: ay.max(by),
        };
        debug_assert!(
            self.contains(&mapped),
            "normalised rect {mapped:?} escapes the {} x {} page ({:?}, {:?})",
            self.width_pt(),
            self.height_pt(),
            self.crop,
            self.rotate
        );
        mapped
    }

    /// Map one user-space point into the normalised page space.
    pub fn normalise_point(&self, x: f32, y: f32) -> (f32, f32) {
        // Offset by the crop box, giving `across` from the left edge and `up` from the
        // bottom edge of the visible page.
        let across = x - self.crop.llx;
        let up = y - self.crop.lly;
        // Flip to y-down while still unrotated.
        let down = self.crop_height() - up;

        match self.rotate {
            Rotate::None => (across, down),
            // A quarter turn clockwise takes the bottom edge to the left edge.
            Rotate::Cw90 => (self.crop_height() - down, across),
            Rotate::Cw180 => (self.crop_width() - across, self.crop_height() - down),
            Rotate::Cw270 => (down, self.crop_width() - across),
        }
    }

    fn contains(&self, rect: &Rect) -> bool {
        let tolerance = INSIDE_PAGE_TOLERANCE_PT;
        rect.x0 >= -tolerance
            && rect.y0 >= -tolerance
            && rect.x1 <= self.width_pt() + tolerance
            && rect.y1 <= self.height_pt() + tolerance
    }
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Row 0.8 of the Phase 0 table, plus a
// unit test the property cannot replace: a rotation applied in the wrong direction still
// lands inside the page box, so "inside the box" does not pin the direction down.
// ---------------------------------------------------------------------------

#[cfg(test)]
const ROTATIONS: [Rotate; 4] = [Rotate::None, Rotate::Cw90, Rotate::Cw180, Rotate::Cw270];

#[test]
fn normalises_corners_for_each_rotation() {
    use crate::geom::{PageGeometry, PdfRect, Rotate};

    // A 400 x 200 landscape crop box whose origin is not at (0, 0), so an implementation
    // that forgets the offset cannot pass.
    let crop = PdfRect {
        llx: 30.0,
        lly: 50.0,
        urx: 430.0,
        ury: 250.0,
    };
    // The bottom-left corner of the crop box, one point square.
    let corner = PdfRect {
        llx: 30.0,
        lly: 50.0,
        urx: 31.0,
        ury: 51.0,
    };

    // Where the unrotated page's bottom-left corner ends up, per rotation, and the size of
    // the displayed page.
    let cases = [
        // /Rotate 0: bottom-left stays bottom-left; page is 400 x 200.
        (Rotate::None, (0.0, 199.0, 1.0, 200.0), (400.0, 200.0)),
        // 90 CW: the bottom edge becomes the left edge, so bottom-left goes to top-left;
        // the page is now 200 x 400.
        (Rotate::Cw90, (0.0, 0.0, 1.0, 1.0), (200.0, 400.0)),
        // 180: bottom-left goes to top-right.
        (Rotate::Cw180, (399.0, 0.0, 400.0, 1.0), (400.0, 200.0)),
        // 270 CW (= 90 CCW): bottom-left goes to bottom-right.
        (Rotate::Cw270, (199.0, 399.0, 200.0, 400.0), (200.0, 400.0)),
    ];

    for (rotate, expected_rect, expected_size) in cases {
        let page = PageGeometry::new(crop, rotate);
        assert_eq!(
            (page.width_pt(), page.height_pt()),
            expected_size,
            "page size for {rotate:?}"
        );
        let got = page.normalise(corner);
        assert_eq!(
            (got.x0, got.y0, got.x1, got.y1),
            expected_rect,
            "bottom-left corner under {rotate:?}"
        );
    }
}

#[cfg(test)]
proptest::proptest! {
    #[test]
    fn prop_normalised_rects_are_inside_page(
        rotation in 0usize..ROTATIONS.len(),
        llx in -500.0f32..500.0,
        lly in -500.0f32..500.0,
        crop_width in 1.0f32..2_000.0,
        crop_height in 1.0f32..2_000.0,
        left in 0.0f32..1.0,
        bottom in 0.0f32..1.0,
        width in 0.0f32..1.0,
        height in 0.0f32..1.0,
    ) {
        use crate::geom::{PageGeometry, PdfRect, Rotate, INSIDE_PAGE_TOLERANCE_PT};

        let crop = PdfRect {
            llx,
            lly,
            urx: llx + crop_width,
            ury: lly + crop_height,
        };
        let rotate = ROTATIONS[rotation];
        let page = PageGeometry::new(crop, rotate);

        // Any rectangle that lies inside the crop box. Content outside it is clipped off
        // the page and is the `ClippedOffPage` ledger reason, not a geometry question.
        let x0 = llx + left * crop_width;
        let y0 = lly + bottom * crop_height;
        let source = PdfRect {
            llx: x0,
            lly: y0,
            urx: x0 + width * (1.0 - left) * crop_width,
            ury: y0 + height * (1.0 - bottom) * crop_height,
        };

        let mapped = page.normalise(source);
        let (w, h) = (page.width_pt(), page.height_pt());
        let tolerance = INSIDE_PAGE_TOLERANCE_PT;

        proptest::prop_assert!(mapped.x0 >= -tolerance, "x0 {} < 0", mapped.x0);
        proptest::prop_assert!(mapped.y0 >= -tolerance, "y0 {} < 0", mapped.y0);
        proptest::prop_assert!(mapped.x1 <= w + tolerance, "x1 {} > {w}", mapped.x1);
        proptest::prop_assert!(mapped.y1 <= h + tolerance, "y1 {} > {h}", mapped.y1);

        // Normalisation never inverts a rectangle, whatever the rotation did to its corners.
        proptest::prop_assert!(mapped.x0 <= mapped.x1);
        proptest::prop_assert!(mapped.y0 <= mapped.y1);

        // A quarter turn swaps the page box; a half turn does not. The expected sizes are
        // taken from the crop box rather than from the generated `crop_width`/`crop_height`:
        // `(llx + crop_width) - llx` is not exactly `crop_width` in `f32` once the offset is
        // large next to the page, and that is float arithmetic, not a normalisation bug.
        let (cw, ch) = (crop.urx - crop.llx, crop.ury - crop.lly);
        let expected = match rotate {
            Rotate::Cw90 | Rotate::Cw270 => (ch, cw),
            Rotate::None | Rotate::Cw180 => (cw, ch),
        };
        proptest::prop_assert_eq!((w, h), expected);
    }
}

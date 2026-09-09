//! Resource limits: the numbers that make a hostile or merely degenerate PDF fail cleanly
//! instead of taking the machine down with it (D13.2, R8 §A2).
//!
//! Every limit is a *declared* quantity checked **before** the work it bounds. That ordering
//! is the whole point: a check after the allocation is a check that runs once the damage is
//! done. A PDF is free to say its image is forty thousand pixels square; believing it costs
//! six gigabytes, and asking first costs a multiplication.
//!
//! The values come from `thresholds.toml`, so a deployment can move them without a rebuild
//! and so each one carries its provenance (D17).

use crate::thresholds::T;

/// The names limits are reported by. They appear in error messages, in NDJSON events and in
/// the report, so they are written once here rather than spelled out at each call site.
pub const MAX_PAGES: &str = "max_pages";
pub const MAX_IMAGE_PIXELS: &str = "max_image_pixels";
pub const MAX_DECOMPRESSED_STREAM_BYTES: &str = "max_decompressed_stream_bytes";
pub const MAX_XREF_CHAIN: &str = "max_xref_chain";

/// One limit, and what asked to exceed it.
///
/// Carries both numbers because "too big" is not an actionable message: a user who sees
/// `max_image_pixels` exceeded by an image of 1 600 000 000 pixels against an allowance of
/// 100 000 000 knows immediately whether to raise the limit or to distrust the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LimitExceeded {
    pub limit: &'static str,
    pub allowed: u64,
    pub requested: u64,
}

impl std::fmt::Display for LimitExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} exceeded: {} requested, {} allowed",
            self.limit, self.requested, self.allowed
        )
    }
}

impl std::error::Error for LimitExceeded {}

/// What one conversion is allowed to consume.
///
/// `Copy`, and passed by value into every stage, because a limit that can be changed halfway
/// through a document is not a limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub max_pages: u32,
    pub max_memory_bytes: u64,
    pub max_decompressed_stream_bytes: u64,
    pub max_image_pixels: u64,
    pub max_xref_chain: u32,
    pub stage_deadline_secs: u64,
}

impl Default for Limits {
    /// The shipped defaults, from `thresholds.toml`.
    fn default() -> Self {
        let limits = &T.limits;
        Self {
            max_pages: clamp_u32(limits.max_pages),
            max_memory_bytes: clamp_u64(limits.max_memory_bytes),
            max_decompressed_stream_bytes: clamp_u64(limits.max_decompressed_stream_bytes),
            max_image_pixels: clamp_u64(limits.max_image_pixels),
            max_xref_chain: clamp_u32(limits.max_xref_chain),
            stage_deadline_secs: clamp_u64(limits.stage_deadline_secs),
        }
    }
}

impl Limits {
    /// Refuse a document with more pages than we agreed to read.
    ///
    /// Called on the page count the moment a document opens and before any page is touched,
    /// which is what makes `--max-pages` a guard rather than a progress bar: the cost of a
    /// hundred-thousand-page document is paid per page, so the only useful place to decline
    /// is before the first one.
    pub fn check_pages(&self, pages: u32) -> Result<(), LimitExceeded> {
        check(MAX_PAGES, u64::from(self.max_pages), u64::from(pages))
    }

    /// Refuse an image by its *declared* dimensions, before anything decodes it.
    ///
    /// The plan writes this check as `width * height * bpc / 8` against `max_image_pixels`,
    /// which compares bytes to a pixel count; the threshold's own evidence line names Pillow's
    /// `MAX_IMAGE_PIXELS`, which is pixels. Pixels it is. Bits per component vary by an order
    /// of magnitude at most and the allowance has two to spare.
    pub fn check_image_pixels(&self, width: u32, height: u32) -> Result<(), LimitExceeded> {
        let pixels = u64::from(width).saturating_mul(u64::from(height));
        check(MAX_IMAGE_PIXELS, self.max_image_pixels, pixels)
    }

    /// Refuse a stream that has already expanded past its allowance.
    ///
    /// Unlike the two above this one is checked *during* the work, because a compressed
    /// stream does not declare its expanded size honestly — that is the attack. The bound is
    /// therefore on the sink: decompression stops at the cap rather than being predicted.
    pub fn check_decompressed(&self, bytes: u64) -> Result<(), LimitExceeded> {
        check(
            MAX_DECOMPRESSED_STREAM_BYTES,
            self.max_decompressed_stream_bytes,
            bytes,
        )
    }

    /// Refuse an xref or object-stream chain deeper than a well-formed file needs.
    ///
    /// A cyclic chain is the degenerate case: without a bound it does not overflow anything,
    /// it simply never finishes.
    pub fn check_xref_chain(&self, hops: u32) -> Result<(), LimitExceeded> {
        check(
            MAX_XREF_CHAIN,
            u64::from(self.max_xref_chain),
            u64::from(hops),
        )
    }
}

fn check(limit: &'static str, allowed: u64, requested: u64) -> Result<(), LimitExceeded> {
    if requested > allowed {
        return Err(LimitExceeded {
            limit,
            allowed,
            requested,
        });
    }
    Ok(())
}

/// Thresholds arrive as `i64`; a negative or oversized limit is a malformed `thresholds.toml`
/// rather than a runtime condition, and `thresholds-lint` is what catches it. Clamping here
/// keeps the conversion total instead of introducing a panic on a path that cannot recover.
fn clamp_u32(value: i64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn clamp_u64(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2).
// ---------------------------------------------------------------------------

#[test]
fn defaults_come_from_thresholds() {
    let limits = Limits::default();
    assert_eq!(limits.max_pages, 3000);
    assert_eq!(limits.max_image_pixels, 100_000_000);
    assert_eq!(limits.max_decompressed_stream_bytes, 268_435_456);
    assert_eq!(limits.max_xref_chain, 128);
}

/// Every check refuses one past its allowance and accepts exactly its allowance.
///
/// The boundary is the assertion: an off-by-one here is the difference between a limit that
/// rejects a legitimate three-thousand-page book and one that lets a bomb through.
#[test]
fn limits_refuse_one_past_the_allowance() {
    let limits = Limits::default();

    assert!(limits.check_pages(limits.max_pages).is_ok());
    assert_eq!(
        limits.check_pages(limits.max_pages + 1).unwrap_err().limit,
        MAX_PAGES
    );

    // 40 000 x 40 000 is 1.6 gigapixels against an allowance of 100 megapixels.
    let error = limits.check_image_pixels(40_000, 40_000).unwrap_err();
    assert_eq!(error.limit, MAX_IMAGE_PIXELS);
    assert_eq!(error.requested, 1_600_000_000);
    assert_eq!(error.allowed, 100_000_000);
    assert!(limits.check_image_pixels(10_000, 10_000).is_ok());

    assert!(limits
        .check_decompressed(limits.max_decompressed_stream_bytes)
        .is_ok());
    assert_eq!(
        limits
            .check_decompressed(limits.max_decompressed_stream_bytes + 1)
            .unwrap_err()
            .limit,
        MAX_DECOMPRESSED_STREAM_BYTES
    );

    assert!(limits.check_xref_chain(limits.max_xref_chain).is_ok());
    assert!(limits.check_xref_chain(limits.max_xref_chain + 1).is_err());
}

/// A pixel count that overflows `u32 * u32` must still be refused rather than wrap to
/// something small — which is how a bounds check becomes the vulnerability it was added for.
#[test]
fn pixel_count_does_not_wrap() {
    let limits = Limits::default();
    let error = limits.check_image_pixels(u32::MAX, u32::MAX).unwrap_err();
    assert_eq!(error.limit, MAX_IMAGE_PIXELS);
    assert!(error.requested > u64::from(u32::MAX));
}

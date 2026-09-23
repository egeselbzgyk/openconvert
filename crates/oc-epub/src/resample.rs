//! A separable Lanczos-3 downscale whose bytes do not depend on the platform (D13.8).
//!
//! `image::imageops::resize` computes its kernel with `f32::sin`, which is whatever `sinf` the
//! platform's C library ships: glibc, Apple's libm and the Windows CRT round differently in the
//! last place for some arguments, a weight moves by an ulp, a pixel rounds the other way, and a
//! book's EPUB stops being byte-identical across operating systems (CI run 35902627957, `f10`).
//! This is the same filter — the same kernel, the same support, the same order of every
//! floating-point operation (`image` 0.25's `vertical_sample` then `horizontal_sample`) — with
//! the one transcendental call replaced by `libm::sinf`, a pure-Rust port of musl's that is the
//! same code, and so the same bits, on every target. Everything else it does is IEEE-754 basic
//! arithmetic, `floor`, `ceil` and `round`, which are exact everywhere.

/// Lanczos-3: three lobes each side, which is also the filter's support in source pixels at
/// scale one.
const LOBES: f32 = 3.0;
/// A pixel's centre is half a pixel from its edge.
const HALF_PIXEL: f32 = 0.5;
/// RGBA.
const CHANNELS: usize = 4;

/// The Lanczos-3 kernel: `sinc(x) · sinc(x / 3)` inside the window, zero outside.
pub(crate) fn lanczos3(x: f32) -> f32 {
    if x.abs() < LOBES {
        sinc(x) * sinc(x / LOBES)
    } else {
        0.0
    }
}

/// The normalised sinc, `sin(πt) / πt`, with the platform's `sinf` kept out of it.
fn sinc(t: f32) -> f32 {
    let a = t * std::f32::consts::PI;
    if t == 0.0 {
        1.0
    } else {
        libm::sinf(a) / a
    }
}

/// The window of source samples one output sample reads, and their normalised weights.
struct Taps {
    first: usize,
    weights: Vec<f32>,
}

/// For each of `new_len` output samples along an axis of `len` source samples (`len >= 1`):
/// where its window starts and what each source sample in it weighs. Shared by both passes, as
/// in `image`.
fn taps(len: u32, new_len: u32) -> Vec<Taps> {
    let ratio = len as f32 / new_len as f32;
    let sratio = if ratio < 1.0 { 1.0 } else { ratio };
    let support = LOBES * sratio;
    let last = i64::from(len) - 1;

    (0..new_len)
        .map(|out| {
            let centre = (out as f32 + HALF_PIXEL) * ratio;
            let left = ((centre - support).floor() as i64).clamp(0, last);
            let right = ((centre + support).ceil() as i64).clamp(left + 1, i64::from(len));
            // Back to the left edge of the pixel, so that `i` below compares like with like: the
            // kernel treats a pixel's centre as 0.
            let centre = centre - HALF_PIXEL;

            let mut weights = Vec::with_capacity(usize::try_from(right - left).unwrap_or(0));
            let mut sum = 0.0_f32;
            for i in left..right {
                let weight = lanczos3((i as f32 - centre) / sratio);
                weights.push(weight);
                sum += weight;
            }
            for weight in &mut weights {
                *weight /= sum;
            }
            Taps {
                first: usize::try_from(left).unwrap_or(0),
                weights,
            }
        })
        .collect()
}

/// Resize an 8-bit RGBA image with Lanczos-3: columns first into an unrounded `f32` buffer,
/// then rows, clamped to `0..=255` and rounded half away from zero.
///
/// An empty source, or a target of the source's own size, is returned as `image` returns it.
pub fn lanczos3_rgba(
    source: &image::RgbaImage,
    new_width: u32,
    new_height: u32,
) -> image::RgbaImage {
    let (width, height) = source.dimensions();
    if width == 0 || height == 0 || new_width == 0 || new_height == 0 {
        return image::ImageBuffer::new(new_width, new_height);
    }
    if (width, height) == (new_width, new_height) {
        return source.clone();
    }

    let (w, nw, nh) = (width as usize, new_width as usize, new_height as usize);
    let raw = source.as_raw();

    // Vertical pass: `width × new_height`, not rounded.
    let mut columns = vec![0.0_f32; w * nh * CHANNELS];
    for (out_y, taps) in taps(height, new_height).iter().enumerate() {
        for x in 0..w {
            let mut t = [0.0_f32; CHANNELS];
            for (i, weight) in taps.weights.iter().enumerate() {
                let at = ((taps.first + i) * w + x) * CHANNELS;
                for (c, sum) in t.iter_mut().enumerate() {
                    *sum += f32::from(raw[at + c]) * weight;
                }
            }
            let at = (out_y * w + x) * CHANNELS;
            columns[at..at + CHANNELS].copy_from_slice(&t);
        }
    }

    // Horizontal pass: `new_width × new_height`, clamped and rounded.
    let max = f32::from(u8::MAX);
    let mut out = vec![0_u8; nw * nh * CHANNELS];
    for (out_x, taps) in taps(width, new_width).iter().enumerate() {
        for y in 0..nh {
            let mut t = [0.0_f32; CHANNELS];
            for (i, weight) in taps.weights.iter().enumerate() {
                let at = (y * w + taps.first + i) * CHANNELS;
                for (c, sum) in t.iter_mut().enumerate() {
                    *sum += columns[at + c] * weight;
                }
            }
            let at = (y * nw + out_x) * CHANNELS;
            for (c, value) in t.iter().enumerate() {
                out[at + c] = value.clamp(0.0, max).round() as u8;
            }
        }
    }

    image::ImageBuffer::from_raw(new_width, new_height, out)
        .unwrap_or_else(|| image::ImageBuffer::new(new_width, new_height))
}

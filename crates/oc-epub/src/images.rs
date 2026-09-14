//! Encoding the images a book draws (R5 §A7, D13.11).
//!
//! JPEG and PNG only. WebP is legal in EPUB 3.3 and reader support lags, and an image format a
//! reader cannot decode is an image the reader does not have (D5).
//!
//! The codecs are the pure-Rust ones in `image`, with no system libraries behind them, and the
//! JPEG quality is a fixed number from `thresholds.toml` rather than a per-image decision.
//! Both of those are what make `--no-ai` output byte-identical across Linux, macOS and Windows
//! (D13.8) — a system JPEG encoder differs between platforms in the last bit of a DCT
//! coefficient, and that is enough to change a sha256.

use std::collections::BTreeMap;

use oc_model::extract::ImageId;

/// The directory images live in, inside the container's root.
pub const IMAGE_DIR: &str = "images";

/// One image as `ingest` decoded it: RGBA, row-major, top-left origin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceImage {
    pub id: ImageId,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// One image as the container carries it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodedImage {
    pub id: ImageId,
    /// Relative to the package document's directory: `images/i0001.png`.
    pub path: String,
    pub media_type: &'static str,
    pub bytes: Vec<u8>,
}

/// How the encoder is configured.
pub struct ImageOptions {
    /// `images.max_longest_side_px`.
    pub max_longest_side_px: u32,
    /// `images.jpeg_quality`.
    pub jpeg_quality: u8,
}

/// Why an image could not be encoded.
#[derive(Debug, thiserror::Error)]
pub enum ImageError {
    #[error("image {0:?} carries {1} bytes for {2}x{3} pixels, which is not RGBA")]
    NotRgba(ImageId, usize, u32, u32),
    #[error("encoding image {0:?} failed: {1}")]
    Encode(ImageId, String),
}

/// Encode every image, and say which file each one became.
pub fn encode(
    images: &[SourceImage],
    options: &ImageOptions,
) -> Result<(Vec<EncodedImage>, BTreeMap<ImageId, String>), ImageError> {
    let mut encoded = Vec::with_capacity(images.len());
    let mut paths = BTreeMap::new();

    for (index, source) in images.iter().enumerate() {
        let one = encode_one(source, index + 1, options)?;
        paths.insert(one.id, one.path.clone());
        encoded.push(one);
    }

    Ok((encoded, paths))
}

fn encode_one(
    source: &SourceImage,
    number: usize,
    options: &ImageOptions,
) -> Result<EncodedImage, ImageError> {
    let expected = usize::try_from(source.width)
        .unwrap_or(usize::MAX)
        .saturating_mul(usize::try_from(source.height).unwrap_or(usize::MAX))
        .saturating_mul(4);
    if source.rgba.len() != expected || source.width == 0 || source.height == 0 {
        return Err(ImageError::NotRgba(
            source.id,
            source.rgba.len(),
            source.width,
            source.height,
        ));
    }

    let buffer: image::RgbaImage =
        image::ImageBuffer::from_raw(source.width, source.height, source.rgba.clone()).ok_or(
            ImageError::NotRgba(source.id, source.rgba.len(), source.width, source.height),
        )?;
    let buffer = downscale(buffer, options.max_longest_side_px);

    // Transparency decides the format, not the source. A photograph re-encoded as PNG is
    // several times the bytes for no gain; an image with an alpha channel flattened into JPEG
    // loses the mask `ingest` went to the trouble of compositing (VD-d).
    let transparent = buffer.pixels().any(|pixel| pixel.0[3] != u8::MAX);

    let mut bytes = Vec::new();
    let media_type = if transparent {
        image::DynamicImage::ImageRgba8(buffer)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .map_err(|error| ImageError::Encode(source.id, error.to_string()))?;
        "image/png"
    } else {
        let rgb = image::DynamicImage::ImageRgba8(buffer).into_rgb8();
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
            std::io::Cursor::new(&mut bytes),
            options.jpeg_quality,
        );
        image::ImageEncoder::write_image(
            encoder,
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|error| ImageError::Encode(source.id, error.to_string()))?;
        "image/jpeg"
    };

    let extension = if media_type == "image/png" {
        "png"
    } else {
        "jpg"
    };
    Ok(EncodedImage {
        id: source.id,
        path: format!("{IMAGE_DIR}/i{number:04}.{extension}"),
        media_type,
        bytes,
    })
}

/// Scale an image so its longest side is within the bound, and leave it alone otherwise.
///
/// `Lanczos3` because the choice has to be *fixed* for cross-OS byte identity and this is the
/// one that loses least on the downscale a scanned plate actually needs. Never upscales: an
/// image smaller than the bound is already as good as it gets, and enlarging it would add
/// bytes and no detail.
fn downscale(buffer: image::RgbaImage, max_longest_side_px: u32) -> image::RgbaImage {
    let longest = buffer.width().max(buffer.height());
    if max_longest_side_px == 0 || longest <= max_longest_side_px {
        return buffer;
    }
    let scale = f64::from(max_longest_side_px) / f64::from(longest);
    let width = ((f64::from(buffer.width()) * scale).round() as u32).max(1);
    let height = ((f64::from(buffer.height()) * scale).round() as u32).max(1);
    image::imageops::resize(
        &buffer,
        width,
        height,
        image::imageops::FilterType::Lanczos3,
    )
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2)
// ---------------------------------------------------------------------------

#[cfg(test)]
fn solid(id: u32, width: u32, height: u32, alpha: u8) -> SourceImage {
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            rgba.extend_from_slice(&[(x % 256) as u8, (y % 256) as u8, 128, alpha]);
        }
    }
    SourceImage {
        id: ImageId(id),
        width,
        height,
        rgba,
    }
}

#[cfg(test)]
fn options() -> ImageOptions {
    ImageOptions {
        max_longest_side_px: 1600,
        jpeg_quality: 85,
    }
}

/// Transparency decides the format. An alpha channel flattened into JPEG loses the mask
/// `ingest` composited; a photograph re-encoded as PNG is several times the bytes for nothing.
#[test]
fn an_image_with_alpha_becomes_png_and_an_opaque_one_becomes_jpeg() {
    let (encoded, paths) =
        encode(&[solid(0, 8, 8, 255), solid(1, 8, 8, 128)], &options()).expect("both encode");

    assert_eq!(encoded[0].media_type, "image/jpeg");
    assert_eq!(encoded[0].path, "images/i0001.jpg");
    assert_eq!(encoded[1].media_type, "image/png");
    assert_eq!(encoded[1].path, "images/i0002.png");

    assert_eq!(
        paths.get(&ImageId(1)).map(String::as_str),
        Some("images/i0002.png")
    );
    assert!(
        encoded[0].bytes.starts_with(&[0xff, 0xd8]),
        "a JPEG SOI marker"
    );
    assert!(encoded[1].bytes.starts_with(b"\x89PNG"), "a PNG signature");
}

/// The bound is on the longest side and it never enlarges: an image smaller than the bound is
/// already as good as it gets, and upscaling adds bytes and no detail.
#[test]
fn an_image_is_scaled_down_to_the_bound_and_never_up() {
    let options = ImageOptions {
        max_longest_side_px: 16,
        jpeg_quality: 85,
    };
    let wide = downscale(
        image::ImageBuffer::from_raw(64, 32, solid(0, 64, 32, 255).rgba).expect("buffer"),
        options.max_longest_side_px,
    );
    assert_eq!(
        (wide.width(), wide.height()),
        (16, 8),
        "aspect ratio is kept"
    );

    let small = downscale(
        image::ImageBuffer::from_raw(8, 4, solid(0, 8, 4, 255).rgba).expect("buffer"),
        options.max_longest_side_px,
    );
    assert_eq!((small.width(), small.height()), (8, 4));
}

/// Byte identity is the property the cross-OS CI gate rests on, and a fixed quality and a
/// fixed filter are what buy it. Two encodings of one image on one machine agreeing is the
/// weakest form of that claim, and the one a unit test can make.
#[test]
fn encoding_the_same_image_twice_produces_the_same_bytes() {
    let source = [solid(0, 40, 24, 255), solid(1, 40, 24, 200)];
    let (first, _) = encode(&source, &options()).expect("encodes");
    let (second, _) = encode(&source, &options()).expect("encodes");
    assert_eq!(first, second);
}

/// A buffer that is not four bytes per pixel is a bug upstream, and encoding it would produce
/// a picture of noise rather than an error.
#[test]
fn a_buffer_that_is_not_rgba_is_refused() {
    let broken = SourceImage {
        id: ImageId(0),
        width: 4,
        height: 4,
        rgba: vec![0; 3 * 16],
    };
    assert!(matches!(
        encode(&[broken], &options()),
        Err(ImageError::NotRgba(..))
    ));
}

pub mod options;

use tracing::debug;

use crate::Result;
use crate::error::Error;
use crate::tile::Format;
// Import all option types via wildcard; individual items that are only used inside
// `#[cfg(feature = "...")]` blocks would otherwise trigger unused-import warnings.
#[allow(unused_imports)]
use options::*;

// ── Public entry point ────────────────────────────────────────────────────────

/// Encode `pixels` (interleaved u8) into the wire format for `format`,
/// applying the per-format options from `opts`.
///
/// Returns [`Error::Encode`] at runtime if the corresponding Cargo feature
/// (`png`, `jpeg`, `webp`, `avif`, `jxl`) is not compiled in.
pub fn encode_tile(
    pixels: &[u8],
    width: u32,
    height: u32,
    bands: usize,
    format: Format,
    opts: &EncodeOptions,
) -> Result<Vec<u8>> {
    // Validate band count early — only 1–4 bands are supported (1=L8, 2=La8, 3=RGB, 4=RGBA).
    // This ensures `BadBandCount` is returned consistently regardless of which
    // Cargo features are compiled in.
    match bands {
        1..=4 => {}
        n => return Err(Error::BadBandCount(n)),
    }
    debug!(width, height, bands, ?format, "encode_tile");
    match format {
        Format::Png => encode_png(pixels, width, height, bands, &opts.png),
        Format::Jpeg => encode_jpeg(pixels, width, height, bands, &opts.jpeg),
        Format::WebP => encode_webp(pixels, width, height, bands, &opts.webp),
        Format::Avif => encode_avif(pixels, width, height, bands, &opts.avif),
        Format::Jxl => encode_jxl(pixels, width, height, bands, &opts.jxl),
    }
}

// ── Per-format encoder implementations ───────────────────────────────────────

fn encode_png(
    pixels: &[u8],
    width: u32,
    height: u32,
    bands: usize,
    opts: &PngOptions,
) -> Result<Vec<u8>> {
    #[cfg(feature = "png")]
    {
        use image::{ImageEncoder, codecs::png::PngEncoder};

        let compression = match opts.compression {
            PngCompression::Default => image::codecs::png::CompressionType::Default,
            PngCompression::Fast => image::codecs::png::CompressionType::Fast,
            PngCompression::Best => image::codecs::png::CompressionType::Best,
        };
        let filter = match opts.filter {
            PngFilter::Adaptive => image::codecs::png::FilterType::Adaptive,
            PngFilter::NoFilter => image::codecs::png::FilterType::NoFilter,
            PngFilter::Sub => image::codecs::png::FilterType::Sub,
            PngFilter::Up => image::codecs::png::FilterType::Up,
            PngFilter::Avg => image::codecs::png::FilterType::Avg,
            PngFilter::Paeth => image::codecs::png::FilterType::Paeth,
        };

        let color = color_type(bands)?;
        let mut out = Vec::new();
        PngEncoder::new_with_quality(&mut out, compression, filter)
            .write_image(pixels, width, height, color)
            .map_err(|e| Error::Encode(e.to_string()))?;
        Ok(out)
    }
    #[cfg(not(feature = "png"))]
    {
        let _ = (pixels, width, height, bands, opts);
        Err(Error::Encode("PNG feature not compiled in".into()))
    }
}

fn encode_jpeg(
    pixels: &[u8],
    width: u32,
    height: u32,
    bands: usize,
    opts: &JpegOptions,
) -> Result<Vec<u8>> {
    #[cfg(feature = "jpeg")]
    {
        use image::{ImageEncoder, codecs::jpeg::JpegEncoder};

        // JPEG does not support alpha. Strip the alpha channel for 4-band (RGBA) or 2-band (La8) inputs.
        let (encode_pixels, jpeg_bands) = match bands {
            4 => {
                debug!("JPEG: stripping alpha channel (RGBA → RGB)");
                (strip_alpha(pixels, 4), 3usize)
            }
            2 => {
                debug!("JPEG: stripping alpha channel (La8 → L8)");
                (strip_alpha(pixels, 2), 1usize)
            }
            _ => (pixels.to_vec(), bands),
        };

        let color = color_type(jpeg_bands)?;
        let mut out = Vec::new();
        JpegEncoder::new_with_quality(&mut out, opts.quality)
            .write_image(&encode_pixels, width, height, color)
            .map_err(|e| Error::Encode(e.to_string()))?;
        Ok(out)
    }
    #[cfg(not(feature = "jpeg"))]
    {
        let _ = (pixels, width, height, bands, opts);
        Err(Error::Encode("JPEG feature not compiled in".into()))
    }
}

fn encode_webp(
    pixels: &[u8],
    width: u32,
    height: u32,
    bands: usize,
    opts: &WebPOptions,
) -> Result<Vec<u8>> {
    #[cfg(feature = "webp")]
    {
        use image::{ImageEncoder, codecs::webp::WebPEncoder};

        // The `image/webp` codec currently supports lossless WebP only.
        // The `opts.lossless` / `opts.quality` fields are wired in for API completeness;
        // a future update can branch on `opts.lossless` once a lossy path is available.
        let _ = opts; // quality unused until lossy WebP lands in `image`

        // The image/webp codec only accepts Rgb8 and Rgba8.  Expand La8 to RGBA.
        let (effective_pixels, effective_bands) = if bands == 2 {
            (la8_to_rgba(pixels), 4usize)
        } else {
            (pixels.to_vec(), bands)
        };

        let color = color_type(effective_bands)?;
        let mut out = Vec::new();
        WebPEncoder::new_lossless(&mut out)
            .write_image(&effective_pixels, width, height, color)
            .map_err(|e| Error::Encode(e.to_string()))?;
        Ok(out)
    }
    #[cfg(not(feature = "webp"))]
    {
        let _ = (pixels, width, height, bands, opts);
        Err(Error::Encode("WebP feature not compiled in".into()))
    }
}

fn encode_avif(
    pixels: &[u8],
    width: u32,
    height: u32,
    bands: usize,
    opts: &AvifOptions,
) -> Result<Vec<u8>> {
    #[cfg(feature = "avif")]
    {
        use image::{ImageEncoder, codecs::avif::AvifEncoder};

        // The image/avif (ravif) encoder does not support La8.  Expand to RGBA.
        let (effective_pixels, effective_bands) = if bands == 2 {
            (la8_to_rgba(pixels), 4usize)
        } else {
            (pixels.to_vec(), bands)
        };

        let color = color_type(effective_bands)?;
        let mut out = Vec::new();
        AvifEncoder::new_with_speed_quality(&mut out, opts.speed, opts.quality)
            .write_image(&effective_pixels, width, height, color)
            .map_err(|e| Error::Encode(e.to_string()))?;
        Ok(out)
    }
    #[cfg(not(feature = "avif"))]
    {
        let _ = (pixels, width, height, bands, opts);
        Err(Error::Encode("AVIF feature not compiled in".into()))
    }
}

fn encode_jxl(
    pixels: &[u8],
    width: u32,
    height: u32,
    bands: usize,
    opts: &JxlOptions,
) -> Result<Vec<u8>> {
    #[cfg(feature = "jxl")]
    {
        use jxl_encoder::api::{LosslessConfig, LossyConfig, PixelLayout};

        let layout = match bands {
            1 => PixelLayout::Gray8,
            2 => PixelLayout::GrayAlpha8,
            3 => PixelLayout::Rgb8,
            4 => PixelLayout::Rgba8,
            n => return Err(Error::BadBandCount(n)),
        };

        if opts.lossless {
            LosslessConfig::new()
                .with_effort(opts.effort)
                .encode(pixels, width, height, layout)
                .map_err(|e| Error::Encode(e.to_string()))
        } else {
            LossyConfig::new(opts.distance)
                .with_effort(opts.effort)
                .encode(pixels, width, height, layout)
                .map_err(|e| Error::Encode(e.to_string()))
        }
    }
    #[cfg(not(feature = "jxl"))]
    {
        let _ = (pixels, width, height, bands, opts);
        Err(Error::Encode("JXL feature not compiled in".into()))
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Strip the last (alpha) channel from a packed interleaved pixel buffer,
/// returning the remaining `src_bands - 1` channels.
#[cfg(feature = "jpeg")]
fn strip_alpha(pixels: &[u8], src_bands: usize) -> Vec<u8> {
    let dst_bands = src_bands - 1;
    let mut out = Vec::with_capacity(pixels.len() / src_bands * dst_bands);
    for chunk in pixels.chunks_exact(src_bands) {
        out.extend_from_slice(&chunk[..dst_bands]);
    }
    out
}

/// Expand a 2-band La8 (grayscale + alpha) buffer to 4-band RGBA by replicating the
/// grey value into R, G, and B.  Used for encoders that do not natively support La8.
#[cfg(any(feature = "webp", feature = "avif"))]
fn la8_to_rgba(pixels: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixels.len() * 2);
    for px in pixels.chunks_exact(2) {
        out.extend_from_slice(&[px[0], px[0], px[0], px[1]]);
    }
    out
}

/// Map band count to the corresponding `ExtendedColorType`.
#[cfg(any(feature = "png", feature = "jpeg", feature = "webp", feature = "avif"))]
fn color_type(bands: usize) -> Result<image::ExtendedColorType> {
    match bands {
        1 => Ok(image::ExtendedColorType::L8),
        2 => Ok(image::ExtendedColorType::La8),
        3 => Ok(image::ExtendedColorType::Rgb8),
        4 => Ok(image::ExtendedColorType::Rgba8),
        n => Err(Error::BadBandCount(n)),
    }
}

/// For formats that do not support alpha (e.g. JPEG after stripping), map band count.
#[cfg(any(feature = "png", feature = "jpeg", feature = "webp", feature = "avif"))]
#[allow(dead_code)]
fn color_type_no_alpha(bands: usize) -> Result<image::ExtendedColorType> {
    match bands {
        1 => Ok(image::ExtendedColorType::L8),
        3 => Ok(image::ExtendedColorType::Rgb8),
        n => Err(Error::BadBandCount(n)),
    }
}

#[cfg(test)]
mod tests;

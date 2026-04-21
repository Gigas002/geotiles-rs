use tracing::debug;

use crate::Result;
use crate::error::Error;
use crate::tile::Format;

/// Encode `pixels` (interleaved, u8) into the wire format for `format`.
///
/// Returns [`Error::Encode`] if the required Cargo feature is not compiled in.
pub fn encode_tile(
    pixels: &[u8],
    width: u32,
    height: u32,
    bands: usize,
    format: Format,
) -> Result<Vec<u8>> {
    debug!(width, height, bands, ?format, "encode_tile");
    match format {
        Format::Png => encode_png(pixels, width, height, bands),
        Format::Jpeg => encode_jpeg(pixels, width, height, bands),
        Format::WebP => encode_webp(pixels, width, height, bands),
        Format::Avif | Format::Jxl => Err(Error::Encode(format!(
            "{:?} encoding is not yet implemented",
            format
        ))),
    }
}

fn encode_png(pixels: &[u8], width: u32, height: u32, bands: usize) -> Result<Vec<u8>> {
    #[cfg(feature = "png")]
    {
        use image::{ImageEncoder, codecs::png::PngEncoder};

        let color = color_type(bands)?;
        let mut out = Vec::new();
        PngEncoder::new(&mut out)
            .write_image(pixels, width, height, color)
            .map_err(|e| Error::Encode(e.to_string()))?;
        Ok(out)
    }
    #[cfg(not(feature = "png"))]
    {
        let _ = (pixels, width, height, bands);
        Err(Error::Encode("PNG feature not compiled in".into()))
    }
}

fn encode_jpeg(pixels: &[u8], width: u32, height: u32, bands: usize) -> Result<Vec<u8>> {
    #[cfg(feature = "jpeg")]
    {
        use image::{ImageEncoder, codecs::jpeg::JpegEncoder};

        let color = color_type(bands)?;
        let mut out = Vec::new();
        JpegEncoder::new(&mut out)
            .write_image(pixels, width, height, color)
            .map_err(|e| Error::Encode(e.to_string()))?;
        Ok(out)
    }
    #[cfg(not(feature = "jpeg"))]
    {
        let _ = (pixels, width, height, bands);
        Err(Error::Encode("JPEG feature not compiled in".into()))
    }
}

fn encode_webp(pixels: &[u8], width: u32, height: u32, bands: usize) -> Result<Vec<u8>> {
    #[cfg(feature = "webp")]
    {
        use image::{ImageEncoder, codecs::webp::WebPEncoder};

        let color = color_type(bands)?;
        let mut out = Vec::new();
        WebPEncoder::new_lossless(&mut out)
            .write_image(pixels, width, height, color)
            .map_err(|e| Error::Encode(e.to_string()))?;
        Ok(out)
    }
    #[cfg(not(feature = "webp"))]
    {
        let _ = (pixels, width, height, bands);
        Err(Error::Encode("WebP feature not compiled in".into()))
    }
}

#[cfg(test)]
mod tests;

#[cfg(any(feature = "png", feature = "jpeg", feature = "webp"))]
fn color_type(bands: usize) -> Result<image::ExtendedColorType> {
    match bands {
        1 => Ok(image::ExtendedColorType::L8),
        3 => Ok(image::ExtendedColorType::Rgb8),
        4 => Ok(image::ExtendedColorType::Rgba8),
        n => Err(Error::BadBandCount(n)),
    }
}

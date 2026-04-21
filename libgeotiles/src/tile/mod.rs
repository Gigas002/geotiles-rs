use fast_image_resize::{
    ResizeOptions, Resizer,
    images::{Image, ImageRef},
    pixels::PixelType,
};
use tracing::debug;

use crate::Result;
use crate::error::Error;

// ── Public types ──────────────────────────────────────────────────────────────

/// Output format for encoded tiles.
///
/// All variants are always available; encoding returns [`Error::Encode`] at runtime
/// if the corresponding Cargo feature (`png`, `jpeg`, `webp`, `avif`, `jxl`) is off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    #[default]
    Png,
    Jpeg,
    WebP,
    Avif,
    Jxl,
}

impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::WebP => "webp",
            Self::Avif => "avif",
            Self::Jxl => "jxl",
        }
    }
}

/// Which backend is used to resample tile pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResampleBackend {
    #[default]
    Cpu,
    #[cfg(feature = "gpu")]
    Gpu,
}

// ── Internal pipeline types ───────────────────────────────────────────────────

/// A rectangular region in source-pixel space (column-major, top-left origin).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelWindow {
    pub col: usize,
    pub row: usize,
    pub width: usize,
    pub height: usize,
}

/// In-RAM buffer for a horizontal slice of source rows, all bands, planar layout.
///
/// `band_data[b]` holds `ds_width * row_count` u8 values in row-major order for band `b+1`.
pub struct ChunkBuffer {
    pub band_data: Vec<Vec<u8>>,
    pub ds_width: usize,
    pub row_start: usize,
    pub row_count: usize,
}

impl ChunkBuffer {
    pub fn band_count(&self) -> usize {
        self.band_data.len()
    }

    pub fn contains_row(&self, row: usize) -> bool {
        row >= self.row_start && row < self.row_start + self.row_count
    }
}

// ── Per-tile crop + resize ────────────────────────────────────────────────────

/// Select the `fast_image_resize` pixel format for `bands` bands.
fn pixel_type(bands: usize) -> Result<PixelType> {
    match bands {
        1 => Ok(PixelType::U8),
        3 => Ok(PixelType::U8x3),
        4 => Ok(PixelType::U8x4),
        n => Err(Error::BadBandCount(n)),
    }
}

/// Extract the pixel window for a tile from `chunk`, resize to `tile_size × tile_size`,
/// and return the raw interleaved pixel bytes (same band layout as the source).
///
/// `window.row` is expressed in dataset-absolute pixel coordinates; it must fall
/// within `[chunk.row_start, chunk.row_start + chunk.row_count)`.
pub fn crop_tile(chunk: &ChunkBuffer, window: PixelWindow, tile_size: u32) -> Result<Vec<u8>> {
    let bands = chunk.band_count();
    let pt = pixel_type(bands)?;

    debug!(
        col = window.col,
        row = window.row,
        src_w = window.width,
        src_h = window.height,
        bands,
        tile_size,
        "crop_tile"
    );

    let row_off = window.row.saturating_sub(chunk.row_start);
    let src_w = window.width;
    let src_h = window.height;

    // Build interleaved pixel buffer from planar chunk data.
    let mut interleaved = vec![0u8; src_w * src_h * bands];
    for row in 0..src_h {
        let chunk_row = row_off + row;
        for col in 0..src_w {
            let chunk_idx = chunk_row * chunk.ds_width + window.col + col;
            let out_base = (row * src_w + col) * bands;
            for band in 0..bands {
                interleaved[out_base + band] = chunk.band_data[band][chunk_idx];
            }
        }
    }

    // Resize to tile_size × tile_size.
    let src_img = ImageRef::new(src_w as u32, src_h as u32, &interleaved, pt)?;
    let mut dst_img = Image::new(tile_size, tile_size, pt);
    Resizer::new().resize(&src_img, &mut dst_img, &ResizeOptions::default())?;

    Ok(dst_img.into_vec())
}

#[cfg(test)]
mod tests;

use fast_image_resize::{
    ResizeOptions, Resizer,
    images::{CroppedImageMut, Image, ImageRef},
    pixels::PixelType,
};
use tracing::debug;

use crate::Result;
use crate::error::Error;
use crate::tile::{ChunkBuffer, DstRect, PixelWindow};

/// Select the `fast_image_resize` pixel format for the given band count.
fn pixel_type(bands: usize) -> Result<PixelType> {
    match bands {
        1 => Ok(PixelType::U8),
        2 => Ok(PixelType::U8x2),
        3 => Ok(PixelType::U8x3),
        4 => Ok(PixelType::U8x4),
        n => Err(Error::BadBandCount(n)),
    }
}

/// Extract the pixel window for a tile from `chunk`, resize it, and place it at `dst`
/// within a `tile_size × tile_size` canvas using SIMD-accelerated bilinear resampling
/// (`fast_image_resize`).
///
/// Returns raw interleaved pixel bytes in the same band layout as the source
/// (1 = L8, 2 = La8, 3 = RGB, 4 = RGBA). Canvas area outside `dst` is left at its
/// zero-initialised value (fully transparent for bands with alpha) — this is the part of
/// the tile the source dataset does not cover.
///
/// `window.row` is in dataset-absolute pixel coordinates; it must fall within
/// `[chunk.row_start, chunk.row_start + chunk.row_count)`.
pub fn crop_tile(
    chunk: &ChunkBuffer,
    window: PixelWindow,
    tile_size: u32,
    dst: DstRect,
) -> Result<Vec<u8>> {
    let bands = chunk.band_count();
    let pt = pixel_type(bands)?;

    debug!(
        col = window.col,
        row = window.row,
        src_w = window.width,
        src_h = window.height,
        bands,
        tile_size,
        ?dst,
        "cpu::crop_tile"
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

    let src_img = ImageRef::new(src_w as u32, src_h as u32, &interleaved, pt)?;
    // Zero-initialised: transparent for RGBA/LA, black for RGB/L — the correct fill for
    // canvas area the source dataset does not reach.
    let mut dst_img = Image::new(tile_size, tile_size, pt);
    let mut cropped_dst = CroppedImageMut::new(&mut dst_img, dst.x, dst.y, dst.width, dst.height)?;
    Resizer::new().resize(&src_img, &mut cropped_dst, &ResizeOptions::default())?;

    Ok(dst_img.into_vec())
}

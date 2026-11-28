//! CPU resampling with `fast_image_resize` (SIMD Lanczos3 convolution).

use fast_image_resize::images::{CroppedImage, CroppedImageMut, Image, ImageRef};
use fast_image_resize::pixels::PixelType;
use fast_image_resize::{FilterType, ResizeAlg, ResizeOptions, Resizer};

use super::{Resampler, TilePixels};
use crate::Result;
use crate::error::Error;
use crate::plan::TileJob;
use crate::raster::RowBlock;

/// Lanczos3 radius.
const SUPPORT: f64 = 3.0;

/// Lanczos3 resampler over the row block in place: no per-tile copy of the source. The
/// source rectangle is fractional, so tiles line up exactly at any zoom.
#[derive(Debug, Default, Clone, Copy)]
pub struct CpuResampler;

fn pixel_type(bands: usize) -> Result<PixelType> {
    match bands {
        1 => Ok(PixelType::U8),
        2 => Ok(PixelType::U8x2),
        3 => Ok(PixelType::U8x3),
        4 => Ok(PixelType::U8x4),
        n => Err(Error::BadBandCount(n)),
    }
}

impl Resampler for CpuResampler {
    fn support(&self) -> f64 {
        SUPPORT
    }

    fn resample(&self, block: &RowBlock, job: &TileJob, tile_size: u32) -> Result<TilePixels> {
        let pt = pixel_type(block.bands)?;
        let (r0, r1) = job.rows_needed(SUPPORT, block.row_end());
        let r0 = r0.max(block.row_start);
        let (c0, c1) = job.cols_needed(SUPPORT, block.width);

        let src = ImageRef::new(block.width as u32, block.rows as u32, &block.data, pt)?;
        // Restrict the resizer to the kernel footprint: with alpha it premultiplies its
        // whole input, which must not be the whole (possibly raster-wide) block.
        let view = CroppedImage::new(
            &src,
            c0 as u32,
            (r0 - block.row_start) as u32,
            (c1 - c0) as u32,
            (r1 - r0) as u32,
        )?;
        let opts = ResizeOptions::new()
            .resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3))
            .crop(
                job.src.x - c0 as f64,
                job.src.y - r0 as f64,
                job.src.width,
                job.src.height,
            );

        let mut canvas = Image::new(tile_size, tile_size, pt);
        let d = job.dst;
        let mut dst = CroppedImageMut::new(&mut canvas, d.x, d.y, d.width, d.height)?;
        Resizer::new().resize(&view, &mut dst, &opts)?;

        Ok(TilePixels {
            data: canvas.into_vec(),
            bands: block.bands,
        })
    }
}

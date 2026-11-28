//! Resampling: turning a tile's source rectangle into `tile_size × tile_size` pixels.

pub mod cpu;
#[cfg(feature = "gpu")]
pub mod gpu;

use crate::Result;
use crate::plan::TileJob;
use crate::raster::RowBlock;

/// Interleaved tile pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TilePixels {
    pub data: Vec<u8>,
    /// 1 = L, 2 = LA, 3 = RGB, 4 = RGBA.
    pub bands: usize,
}

/// A resampling backend. Shared across worker threads.
pub trait Resampler: Sync {
    /// Kernel radius in source pixels at 1:1 scale. The tiler reads this many extra rows
    /// (times the downscale factor) around every tile so kernels see real neighbours
    /// instead of a clamped edge, which keeps tiles seamless.
    fn support(&self) -> f64;

    /// Render `job` from `block`, which holds at least
    /// [`TileJob::rows_needed`]`(self.support(), ..)`. Canvas pixels outside `job.dst` are
    /// zero (transparent where there is alpha).
    fn resample(&self, block: &RowBlock, job: &TileJob, tile_size: u32) -> Result<TilePixels>;
}

#[cfg(test)]
mod tests;

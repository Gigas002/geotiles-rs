//! Raster input: what the tiler consumes, independent of where the pixels come from.
//!
//! The tiler only ever sees a [`RasterSource`]: an 8-bit, pixel-interleaved, north-up
//! raster with 1–4 bands (L, LA, RGB, RGBA) and a [`GeoTransform`] in the tile grid's
//! CRS. Anything else (reprojection, bit-depth scaling, nodata→alpha, palette expansion)
//! is the caller's job, done before tiling with whatever tool they like.

#[cfg(feature = "geotiff")]
pub mod geotiff;

use crate::Result;
use crate::error::Error;
use crate::geo::{Bounds, GeoTransform};

/// Static description of a raster.
#[derive(Debug, Clone, PartialEq)]
pub struct RasterInfo {
    pub width: usize,
    pub height: usize,
    /// Samples per pixel: 1 = L, 2 = LA, 3 = RGB, 4 = RGBA.
    pub bands: usize,
    pub transform: GeoTransform,
    /// EPSG code of the raster's CRS when the source declares one.
    pub epsg: Option<u32>,
}

impl RasterInfo {
    pub fn bounds(&self) -> Bounds {
        self.transform.bounds(self.width, self.height)
    }

    /// Check the invariants every [`RasterSource`] must uphold.
    pub fn validate(&self) -> Result<()> {
        if self.width == 0 || self.height == 0 {
            return Err(Error::Unsupported(format!(
                "empty raster ({}x{})",
                self.width, self.height
            )));
        }
        if !(1..=4).contains(&self.bands) {
            return Err(Error::BadBandCount(self.bands));
        }
        Ok(())
    }

    /// Bytes per full-width row.
    pub fn row_bytes(&self) -> usize {
        self.width * self.bands
    }
}

/// A horizontal band of full-width rows, pixel-interleaved.
#[derive(Debug, Clone, PartialEq)]
pub struct RowBlock {
    /// `rows * width * bands` bytes, row-major, interleaved.
    pub data: Vec<u8>,
    pub width: usize,
    pub bands: usize,
    /// First raster row held.
    pub row_start: usize,
    pub rows: usize,
}

impl RowBlock {
    pub fn row_end(&self) -> usize {
        self.row_start + self.rows
    }
}

/// Anything that can hand out rows of a georeferenced 8-bit raster.
pub trait RasterSource {
    fn info(&self) -> &RasterInfo;

    /// Read rows `[row_start, row_start + rows)`, clamped to the raster height.
    fn read_rows(&mut self, row_start: usize, rows: usize) -> Result<RowBlock>;
}

/// A raster held entirely in memory. Mostly useful for tests and for callers that already
/// decoded their input some other way.
#[derive(Debug, Clone)]
pub struct MemoryRaster {
    info: RasterInfo,
    data: Vec<u8>,
}

impl MemoryRaster {
    /// `data` must be `width * height * bands` interleaved bytes.
    pub fn new(info: RasterInfo, data: Vec<u8>) -> Result<Self> {
        info.validate()?;
        let expected = info.row_bytes() * info.height;
        if data.len() != expected {
            return Err(Error::Unsupported(format!(
                "pixel buffer is {} bytes, expected {expected}",
                data.len()
            )));
        }
        Ok(Self { info, data })
    }
}

impl RasterSource for MemoryRaster {
    fn info(&self) -> &RasterInfo {
        &self.info
    }

    fn read_rows(&mut self, row_start: usize, rows: usize) -> Result<RowBlock> {
        let start = row_start.min(self.info.height);
        let end = row_start.saturating_add(rows).min(self.info.height);
        let stride = self.info.row_bytes();
        Ok(RowBlock {
            data: self.data[start * stride..end * stride].to_vec(),
            width: self.info.width,
            bands: self.info.bands,
            row_start: start,
            rows: end - start,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(w: usize, h: usize, bands: usize) -> RasterInfo {
        RasterInfo {
            width: w,
            height: h,
            bands,
            transform: GeoTransform::new(0.0, 0.0, 1.0, -1.0).unwrap(),
            epsg: None,
        }
    }

    #[test]
    fn memory_raster_reads_clamped_rows() {
        let mut r = MemoryRaster::new(info(2, 3, 1), (0..6).collect()).unwrap();
        let b = r.read_rows(1, 10).unwrap();
        assert_eq!((b.row_start, b.rows, b.data), (1, 2, vec![2, 3, 4, 5]));
        assert_eq!(r.read_rows(5, 1).unwrap().rows, 0);
    }

    #[test]
    fn memory_raster_validates() {
        assert!(MemoryRaster::new(info(2, 2, 1), vec![0; 3]).is_err());
        assert!(MemoryRaster::new(info(2, 2, 5), vec![0; 20]).is_err());
        assert!(MemoryRaster::new(info(0, 2, 1), vec![]).is_err());
    }
}

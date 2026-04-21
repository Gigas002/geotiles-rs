use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

use tracing::{info, info_span};

use crate::Result;
#[cfg(any(feature = "geographic", feature = "mercator"))]
use crate::coords::Tile;
use crate::coords::{Bounds, DEFAULT_TILE_SIZE};
use crate::encode::options::{
    AvifOptions, EncodeOptions, JpegOptions, JxlOptions, PngOptions, WebPOptions,
};
use crate::tile::{Format, ResampleBackend};

#[cfg(not(any(feature = "geographic", feature = "mercator")))]
use crate::error::Error;

/// Builder and entry point for the tile generation pipeline.
///
/// ```rust,no_run
/// # use libgeotiles::{GeoTiff, Format};
/// GeoTiff::open("input.tif").unwrap()
///     .zoom(4..=10)
///     .chunk_size(512)
///     .format(Format::Png)
///     .output("tiles/")
///     .crop().unwrap();
/// ```
pub struct GeoTiff {
    path: PathBuf,
    zoom: RangeInclusive<u8>,
    /// Maximum source-pixel rows read into RAM per chunk iteration.
    chunk_size: usize,
    format: Format,
    output: PathBuf,
    backend: ResampleBackend,
    tile_size: u32,
    /// Per-format encoder options.  Each format uses its own sub-struct with sane defaults.
    encode_options: EncodeOptions,
}

impl GeoTiff {
    /// Open `path` for reading and return a builder with sensible defaults.
    ///
    /// Validates that the dataset is readable; the full pipeline runs only on [`Self::crop`].
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let _ = crate::gdal_io::open_dataset(&path)?;
        Ok(Self {
            path,
            zoom: 0..=12,
            chunk_size: 512,
            format: Format::Png,
            output: PathBuf::from("tiles"),
            backend: ResampleBackend::Cpu,
            tile_size: DEFAULT_TILE_SIZE,
            encode_options: EncodeOptions::default(),
        })
    }

    pub fn zoom(mut self, range: RangeInclusive<u8>) -> Self {
        self.zoom = range;
        self
    }

    /// Maximum source-pixel rows read into RAM at once.
    ///
    /// Smaller values reduce peak memory; larger values reduce GDAL I/O calls.
    /// Default: 512. A value of 1 forces one GDAL read per tile row (useful for testing
    /// that output is identical regardless of chunk boundaries).
    pub fn chunk_size(mut self, rows: usize) -> Self {
        self.chunk_size = rows;
        self
    }

    pub fn format(mut self, fmt: Format) -> Self {
        self.format = fmt;
        self
    }

    pub fn output(mut self, dir: impl AsRef<Path>) -> Self {
        self.output = dir.as_ref().to_path_buf();
        self
    }

    pub fn backend(mut self, b: ResampleBackend) -> Self {
        self.backend = b;
        self
    }

    pub fn tile_size(mut self, size: u32) -> Self {
        self.tile_size = size;
        self
    }

    /// Set PNG-specific encoding options (compression level, filter).
    pub fn png_options(mut self, opts: PngOptions) -> Self {
        self.encode_options.png = opts;
        self
    }

    /// Set JPEG-specific encoding options (quality).
    ///
    /// Note: alpha channels are automatically stripped for JPEG output (RGBA → RGB).
    pub fn jpeg_options(mut self, opts: JpegOptions) -> Self {
        self.encode_options.jpeg = opts;
        self
    }

    /// Set WebP-specific encoding options (lossless flag, quality for future lossy path).
    pub fn webp_options(mut self, opts: WebPOptions) -> Self {
        self.encode_options.webp = opts;
        self
    }

    /// Set AVIF-specific encoding options (quality, encoder speed).
    ///
    /// Requires the `avif` Cargo feature.
    pub fn avif_options(mut self, opts: AvifOptions) -> Self {
        self.encode_options.avif = opts;
        self
    }

    /// Set JPEG XL–specific encoding options (Butteraugli distance, effort, lossless flag).
    ///
    /// Requires the `jxl` Cargo feature.
    pub fn jxl_options(mut self, opts: JxlOptions) -> Self {
        self.encode_options.jxl = opts;
        self
    }

    /// Run the full tiling pipeline, writing `{output}/{z}/{x}/{y}.{ext}` for every
    /// tile that overlaps the source raster extent.
    ///
    /// The outer loop iterates sequential source-pixel chunks (bounded by `chunk_size`);
    /// tiles within each chunk are processed in parallel via rayon.
    pub fn crop(self) -> Result<()> {
        let _span = info_span!("GeoTiff::crop", path = %self.path.display()).entered();

        let (src_ds, _) = crate::gdal_io::open_dataset(&self.path)?;
        // Warp to EPSG:4326 via a lazy in-memory VRT; no full reproject for large rasters.
        let warped_opt = crate::gdal_io::warp_to_epsg(&src_ds, 4326)?;
        let ds = warped_opt.as_ref().unwrap_or(&src_ds);

        let (ds_width, ds_height) = ds.raster_size();
        let gt = ds.geo_transform()?;
        let band_count = ds.raster_count();

        let ds_bounds = Bounds {
            min_x: gt[0],
            min_y: gt[3] + gt[5] * ds_height as f64,
            max_x: gt[0] + gt[1] * ds_width as f64,
            max_y: gt[3],
        };

        info!(
            ds_width,
            ds_height,
            band_count,
            min_lon = ds_bounds.min_x,
            min_lat = ds_bounds.min_y,
            max_lon = ds_bounds.max_x,
            max_lat = ds_bounds.max_y,
            "dataset ready"
        );

        self.run(
            ds,
            ds_bounds,
            &gt,
            ds_width,
            ds_height,
            band_count,
            &self.encode_options,
        )
    }

    #[cfg(feature = "geographic")]
    #[allow(clippy::too_many_arguments)]
    fn run(
        &self,
        ds: &gdal::Dataset,
        ds_bounds: Bounds,
        gt: &[f64; 6],
        ds_width: usize,
        ds_height: usize,
        band_count: usize,
        encode_options: &EncodeOptions,
    ) -> Result<()> {
        use crate::coords::geographic::Geographic;

        struct GeoGrid(Geographic);

        impl crate::pipeline::TileGrid for GeoGrid {
            fn tile_range(&self, area: Bounds, z: u8) -> (Tile, Tile) {
                self.0.tile_range(area, z)
            }
            fn tile_bounds(&self, tile: Tile) -> Bounds {
                self.0.bounds(tile)
            }
        }

        let grid = GeoGrid(Geographic::new(self.tile_size));
        crate::pipeline::run_pipeline(
            ds,
            &grid,
            ds_bounds,
            gt,
            ds_width,
            ds_height,
            band_count,
            self.zoom.clone(),
            self.chunk_size,
            self.tile_size,
            self.format,
            encode_options,
            &self.output,
        )
    }

    #[cfg(all(not(feature = "geographic"), feature = "mercator"))]
    #[allow(clippy::too_many_arguments)]
    fn run(
        &self,
        ds: &gdal::Dataset,
        ds_bounds: Bounds,
        gt: &[f64; 6],
        ds_width: usize,
        ds_height: usize,
        band_count: usize,
        encode_options: &EncodeOptions,
    ) -> Result<()> {
        use crate::coords::mercator::WebMercator;

        struct MercGrid(WebMercator);

        impl crate::pipeline::TileGrid for MercGrid {
            fn tile_range(&self, area: Bounds, z: u8) -> (Tile, Tile) {
                self.0.tile_range(area, z)
            }
            fn tile_bounds(&self, tile: Tile) -> Bounds {
                self.0.bounds(tile)
            }
        }

        let grid = MercGrid(WebMercator::new(self.tile_size));
        crate::pipeline::run_pipeline(
            ds,
            &grid,
            ds_bounds,
            gt,
            ds_width,
            ds_height,
            band_count,
            self.zoom.clone(),
            self.chunk_size,
            self.tile_size,
            self.format,
            encode_options,
            &self.output,
        )
    }

    #[cfg(not(any(feature = "geographic", feature = "mercator")))]
    #[allow(clippy::too_many_arguments)]
    fn run(
        &self,
        _ds: &gdal::Dataset,
        _ds_bounds: Bounds,
        _gt: &[f64; 6],
        _ds_width: usize,
        _ds_height: usize,
        _band_count: usize,
        _encode_options: &EncodeOptions,
    ) -> Result<()> {
        Err(Error::Encode(
            "no CRS tile math compiled in; enable the 'geographic' or 'mercator' feature".into(),
        ))
    }
}

#[cfg(test)]
mod tests;

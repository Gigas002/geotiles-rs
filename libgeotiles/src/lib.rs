//! Cut web-map tiles (XYZ/TMS, EPSG:4326 or EPSG:3857) from georeferenced 8-bit rasters.
//!
//! The library takes standard input and does nothing else to it: an 8-bit, north-up,
//! pixel-interleaved raster with 1–4 bands, already in the tile grid's CRS. Reprojection,
//! bit-depth scaling, nodata→alpha and similar preparation belong in a separate step with
//! whatever tool fits.
//!
//! ```no_run
//! # #[cfg(feature = "geotiff")]
//! # fn main() -> libgeotiles::Result<()> {
//! use libgeotiles::{CpuResampler, DirSink, GeoTiff, Geographic, TilerOptions, generate};
//!
//! let mut source = GeoTiff::open("input.tif".as_ref())?;
//! let opts = TilerOptions { zooms: 0..=8, ..Default::default() };
//! generate(&mut source, &Geographic::DEFAULT, &CpuResampler, &DirSink::new("tiles", false), &opts)?;
//! # Ok(())
//! # }
//! # #[cfg(not(feature = "geotiff"))]
//! # fn main() {}
//! ```
//!
//! Pieces are swappable: any [`RasterSource`], [`TileGrid`], [`Resampler`] or [`TileSink`].

/// Tile addressing, the [`TileGrid`] trait and the built-in grids.
pub mod coords;
/// Tile encoders: PNG, JPEG, WebP, AVIF, JPEG XL.
pub mod encode;
/// Public error type.
pub mod error;
/// Bounds, geotransform, CRS.
pub mod geo;
/// Band-layout conversion.
pub mod pixels;
/// Tile planning and row batching.
pub mod plan;
/// Raster input: [`RasterSource`], [`MemoryRaster`], [`GeoTiff`].
pub mod raster;
/// Resampling backends.
pub mod resample;
/// Tile output.
pub mod sink;
/// The driver tying it together.
pub mod tiler;

pub use coords::geographic::Geographic;
pub use coords::mercator::WebMercator;
pub use coords::{MAX_ZOOM, Tile, TileGrid, flip_y};
pub use encode::options::{
    AvifOptions, EncodeOptions, JpegOptions, JxlOptions, PngCompression, PngFilter, PngOptions,
    WebPOptions,
};
pub use encode::{Format, encode_tile};
pub use error::Error;
pub use geo::{Bounds, Crs, GeoTransform};
#[cfg(feature = "geotiff")]
pub use raster::geotiff::GeoTiff;
pub use raster::{MemoryRaster, RasterInfo, RasterSource, RowBlock};
pub use resample::cpu::CpuResampler;
#[cfg(feature = "gpu")]
pub use resample::gpu::GpuResampler;
pub use resample::{Resampler, TilePixels};
pub use sink::{DirSink, TileSink};
pub use tiler::{Stats, TilerOptions, generate};

pub type Result<T> = std::result::Result<T, Error>;

/// Tile coordinate types and grid math for EPSG:4326 and EPSG:3857.
pub mod coords;
#[doc(hidden)]
pub mod crs;
// `encode` is always compiled so that option types and `encode_tile` are unconditionally
// available (for benchmarks and the `GeoTiff` builder API).  The actual encoding functions
// inside it are individually gated on their respective Cargo features (`png`, `jpeg`, etc.).
pub(crate) mod encode;
pub mod error;
#[doc(hidden)]
pub mod gdal_io;
mod geotiff;
#[cfg(any(feature = "geographic", feature = "mercator"))]
mod pipeline;
#[doc(hidden)]
pub mod tile;

pub use coords::{Bounds, Tile, flip_y};
/// Semi-public re-export of the raw tile encoder for use in benchmarks.
///
/// This is **not** part of the stable public API; it may change without notice.
#[doc(hidden)]
pub use encode::encode_tile;
pub use encode::options::{
    AvifOptions, EncodeOptions, JpegOptions, JxlOptions, PngCompression, PngFilter, PngOptions,
    WebPOptions,
};
pub use error::Error;
pub use geotiff::GeoTiff;
pub use tile::{Format, ResampleBackend};
pub type Result<T> = std::result::Result<T, Error>;

/// Tile coordinate types and grid math for EPSG:4326 and EPSG:3857.
pub mod coords;
#[doc(hidden)]
pub mod crs;
#[cfg(any(feature = "geographic", feature = "mercator"))]
mod encode;
pub mod error;
#[doc(hidden)]
pub mod gdal_io;
mod geotiff;
#[cfg(any(feature = "geographic", feature = "mercator"))]
mod pipeline;
#[doc(hidden)]
pub mod tile;

pub use coords::{Bounds, Tile, flip_y};
pub use error::Error;
pub use geotiff::GeoTiff;
pub use tile::{Format, ResampleBackend};
pub type Result<T> = std::result::Result<T, Error>;

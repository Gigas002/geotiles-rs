/// Resample backends: CPU (`fast_image_resize`) and optional GPU (`wgpu`).
pub mod backend;
/// Tile coordinate types and grid math for EPSG:4326 and EPSG:3857.
pub mod coords;
/// Tile encoder: PNG, JPEG, WebP, AVIF, JXL dispatch.
pub mod encode;
/// Public error types.
pub mod error;
/// GDAL dataset operations: open, warp, windowed read, mask alpha, CRS detection.
pub mod gdal_io;
/// Tile enumeration utilities: `TileGrid` trait, `group_tiles_by_chunk`.
pub mod pipeline;
/// Core data types: `Format`, `PixelWindow`, `ChunkBuffer`.
pub mod tile;

pub use backend::ResampleBackend;
pub use coords::{Bounds, Tile, flip_y};
pub use encode::options::{
    AvifOptions, EncodeOptions, JpegOptions, JxlOptions, PngCompression, PngFilter, PngOptions,
    WebPOptions,
};
pub use error::Error;
pub use tile::Format;

/// Semi-public re-export of the raw tile encoder for use in benchmarks.
///
/// Not part of the stable public API; may change without notice.
#[doc(hidden)]
pub use encode::encode_tile;

pub type Result<T> = std::result::Result<T, Error>;

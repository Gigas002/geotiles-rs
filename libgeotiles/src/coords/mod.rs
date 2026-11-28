//! Tile addressing and the two built-in tile grids (EPSG:4326 and EPSG:3857).

pub mod geographic;
pub mod mercator;

use crate::geo::{Bounds, Crs};

/// Default tile width/height in pixels; used by both CRS grids.
pub const DEFAULT_TILE_SIZE: u32 = 256;

/// Deepest supported zoom level: the geodetic grid has `2^(z+1)` columns, which must fit
/// in a `u32` tile index.
pub const MAX_ZOOM: u8 = 30;

/// XYZ (slippy-map) tile address. `y = 0` is north; use [`flip_y`] for TMS ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Tile {
    pub x: u32,
    pub y: u32,
    pub z: u8,
}

impl Tile {
    pub fn new(x: u32, y: u32, z: u8) -> Self {
        Self { x, y, z }
    }
}

/// Flip Y between XYZ (y = 0 north) and TMS (y = 0 south) conventions.
/// The transform is its own inverse.
pub fn flip_y(y: u32, z: u8) -> u32 {
    (1u32 << z).saturating_sub(1).saturating_sub(y)
}

/// A tile grid in some CRS. All bounds are in that CRS's native units.
///
/// Implement this to tile into a custom grid; the built-ins are
/// [`geographic::Geographic`] and [`mercator::WebMercator`].
pub trait TileGrid: Sync {
    /// The CRS the grid (and therefore the input raster) is expressed in.
    fn crs(&self) -> Crs;
    /// Tile width and height in pixels.
    fn tile_size(&self) -> u32;
    /// Inclusive `(top_left, bottom_right)` tile range covering `area` at zoom `z`.
    fn tile_range(&self, area: Bounds, z: u8) -> (Tile, Tile);
    /// Native-unit bounds of `tile`.
    fn tile_bounds(&self, tile: Tile) -> Bounds;
}

#[cfg(test)]
mod tests;

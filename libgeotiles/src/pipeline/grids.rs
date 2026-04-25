//! [`TileGrid`] implementations for the built-in coordinate grids.

#[cfg(feature = "geographic")]
use crate::coords::geographic::Geographic;
#[cfg(feature = "mercator")]
use crate::coords::mercator::WebMercator;
#[cfg(any(feature = "geographic", feature = "mercator"))]
use crate::coords::{Bounds, Tile};

#[cfg(any(feature = "geographic", feature = "mercator"))]
use super::TileGrid;

#[cfg(feature = "geographic")]
impl TileGrid for Geographic {
    fn tile_range(&self, area: Bounds, z: u8) -> (Tile, Tile) {
        self.tile_range(area, z)
    }

    /// Returns bounds in EPSG:4326 degrees — matches the working dataset CRS when
    /// the source was warped (or is natively) in geographic coordinates.
    fn tile_bounds(&self, tile: Tile) -> Bounds {
        self.bounds(tile)
    }
}

#[cfg(feature = "mercator")]
impl TileGrid for WebMercator {
    /// `area` must be in Web Mercator metres (the working dataset CRS after warping to
    /// EPSG:3857).
    fn tile_range(&self, area: Bounds, z: u8) -> (Tile, Tile) {
        self.tile_range_from_merc(area, z)
    }

    /// Returns bounds in Web Mercator metres — matches the working dataset CRS when
    /// the source was warped to EPSG:3857.
    fn tile_bounds(&self, tile: Tile) -> Bounds {
        self.merc_bounds(tile)
    }
}

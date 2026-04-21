//! EPSG:4326 (WGS-84 geographic) tile grid — the default CRS for this library.
//!
//! Grid layout follows the WMTS/TMS geodetic profile:
//! - Zoom `z` has `2^(z+1)` columns × `2^z` rows.
//! - Resolution: `180 / (tile_size × 2^z)` degrees per pixel.
//! - `y = 0` is north (XYZ); use [`super::flip_y`] for TMS south-origin ordering.

use tracing::debug;

use super::{Bounds, DEFAULT_TILE_SIZE, Tile};

/// Tile grid for EPSG:4326.
///
/// Construct once; all methods borrow `self` so the `tile_size` is never repeated
/// at call sites. Use [`Geographic::DEFAULT`] for the standard 256-pixel grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geographic {
    pub tile_size: u32,
}

impl Geographic {
    pub const DEFAULT: Self = Self {
        tile_size: DEFAULT_TILE_SIZE,
    };

    pub fn new(tile_size: u32) -> Self {
        Self { tile_size }
    }

    /// Ground resolution in degrees per pixel at zoom `z`.
    pub fn resolution(&self, z: u8) -> f64 {
        180.0 / (self.tile_size as f64 * (1u64 << z) as f64)
    }

    /// Number of tile columns at zoom `z` (= `2^(z+1)`).
    pub fn x_count(&self, z: u8) -> u32 {
        1u32 << (z + 1)
    }

    /// Number of tile rows at zoom `z` (= `2^z`).
    pub fn y_count(&self, z: u8) -> u32 {
        1u32 << z
    }

    /// Tile containing the point `(lon, lat)` at zoom `z`.
    pub fn tile(&self, lon: f64, lat: f64, z: u8) -> Tile {
        let step = self.resolution(z) * self.tile_size as f64;
        let x = ((lon + 180.0) / step).floor().max(0.0) as u32;
        let y = ((90.0 - lat) / step).floor().max(0.0) as u32;
        let t = Tile::new(
            x.min(self.x_count(z).saturating_sub(1)),
            y.min(self.y_count(z).saturating_sub(1)),
            z,
        );
        debug!(lon, lat, z, x = t.x, y = t.y, "tile (4326)");
        t
    }

    /// Geographic bounding box (degrees) of tile `t`.
    pub fn bounds(&self, t: Tile) -> Bounds {
        let step = self.resolution(t.z) * self.tile_size as f64;
        Bounds {
            min_x: -180.0 + t.x as f64 * step,
            max_x: -180.0 + (t.x + 1) as f64 * step,
            max_y: 90.0 - t.y as f64 * step,
            min_y: 90.0 - (t.y + 1) as f64 * step,
        }
    }

    /// Inclusive tile range `(top_left, bottom_right)` covering `area` at zoom `z`.
    pub fn tile_range(&self, area: Bounds, z: u8) -> (Tile, Tile) {
        let min = self.tile(area.min_x, area.max_y, z);
        let max = self.tile(area.max_x, area.min_y, z);
        debug!(z, ?area, ?min, ?max, "tile_range (4326)");
        (min, max)
    }
}

impl Default for Geographic {
    fn default() -> Self {
        Self::DEFAULT
    }
}

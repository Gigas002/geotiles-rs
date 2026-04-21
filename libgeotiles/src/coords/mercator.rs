//! EPSG:3857 (Web Mercator) tile grid and projection helpers.
//!
//! Grid layout is the standard slippy-map square grid:
//! - Zoom `z` has `2^z` columns × `2^z` rows.
//! - Resolution: `2π R / (tile_size × 2^z)` metres per pixel.
//! - `y = 0` is north (XYZ); use [`super::flip_y`] for TMS south-origin ordering.

use std::f64::consts::PI;

use tracing::debug;

use super::{Bounds, DEFAULT_TILE_SIZE, Tile};

/// WGS-84 semi-major axis in metres.
pub const EARTH_RADIUS: f64 = 6_378_137.0;

/// Half the equatorial circumference — the coordinate extent of EPSG:3857 on each axis.
pub const ORIGIN_SHIFT: f64 = PI * EARTH_RADIUS;

/// Latitude clamp for Web Mercator (the poles are at ±∞ in this projection).
pub const MAX_LAT: f64 = 85.051_128_779_806_6;

// ── Projection helpers (do not depend on tile size) ───────────────────────────

/// Project WGS-84 degrees to Web Mercator metres.
pub fn to_merc(lon: f64, lat: f64) -> (f64, f64) {
    let lat = lat.clamp(-MAX_LAT, MAX_LAT);
    let mx = lon * ORIGIN_SHIFT / 180.0;
    let my = (lat.to_radians() / 2.0 + PI / 4.0).tan().ln() * EARTH_RADIUS;
    (mx, my)
}

/// Unproject Web Mercator metres back to WGS-84 degrees.
pub fn from_merc(mx: f64, my: f64) -> (f64, f64) {
    let lon = mx * 180.0 / ORIGIN_SHIFT;
    let lat = (2.0 * (my / EARTH_RADIUS).exp().atan() - PI / 2.0).to_degrees();
    (lon, lat)
}

// ── Tile grid ─────────────────────────────────────────────────────────────────

/// Tile grid for EPSG:3857.
///
/// Construct once; all methods borrow `self`. Use [`WebMercator::DEFAULT`] for
/// the standard 256-pixel grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebMercator {
    pub tile_size: u32,
}

impl WebMercator {
    pub const DEFAULT: Self = Self {
        tile_size: DEFAULT_TILE_SIZE,
    };

    pub fn new(tile_size: u32) -> Self {
        Self { tile_size }
    }

    /// Ground resolution in metres per pixel at zoom `z`.
    pub fn resolution(&self, z: u8) -> f64 {
        2.0 * ORIGIN_SHIFT / (self.tile_size as f64 * (1u64 << z) as f64)
    }

    /// Tile count on each axis at zoom `z` (= `2^z`).
    pub fn count(&self, z: u8) -> u32 {
        1u32 << z
    }

    /// Tile containing the point `(lon, lat)` at zoom `z`.
    pub fn tile(&self, lon: f64, lat: f64, z: u8) -> Tile {
        let (mx, my) = to_merc(lon, lat);
        self.tile_from_merc(mx, my, z)
    }

    /// Tile containing the Web Mercator point `(mx, my)` at zoom `z`.
    pub fn tile_from_merc(&self, mx: f64, my: f64, z: u8) -> Tile {
        let res = self.resolution(z);
        let ts = self.tile_size as f64;
        let max = self.count(z).saturating_sub(1);
        let x = ((mx + ORIGIN_SHIFT) / res / ts).floor() as u32;
        let y = ((ORIGIN_SHIFT - my) / res / ts).floor() as u32;
        let t = Tile::new(x.min(max), y.min(max), z);
        debug!(mx, my, z, x = t.x, y = t.y, "tile_from_merc (3857)");
        t
    }

    /// Geographic bounding box (WGS-84 degrees) of tile `t`.
    pub fn bounds(&self, t: Tile) -> Bounds {
        let mb = self.merc_bounds(t);
        let (min_x, min_y) = from_merc(mb.min_x, mb.min_y);
        let (max_x, max_y) = from_merc(mb.max_x, mb.max_y);
        Bounds {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }

    /// Web Mercator bounding box (metres) of tile `t`.
    pub fn merc_bounds(&self, t: Tile) -> Bounds {
        let res = self.resolution(t.z);
        let ts = self.tile_size as f64;
        Bounds {
            min_x: t.x as f64 * ts * res - ORIGIN_SHIFT,
            max_x: (t.x + 1) as f64 * ts * res - ORIGIN_SHIFT,
            max_y: ORIGIN_SHIFT - t.y as f64 * ts * res,
            min_y: ORIGIN_SHIFT - (t.y + 1) as f64 * ts * res,
        }
    }

    /// Inclusive tile range `(top_left, bottom_right)` covering geographic `area` at zoom `z`.
    pub fn tile_range(&self, area: Bounds, z: u8) -> (Tile, Tile) {
        let min = self.tile(area.min_x, area.max_y, z);
        let max = self.tile(area.max_x, area.min_y, z);
        debug!(z, ?area, ?min, ?max, "tile_range (3857)");
        (min, max)
    }

    /// Inclusive tile range covering a Web Mercator `area` (metres) at zoom `z`.
    pub fn tile_range_from_merc(&self, area: Bounds, z: u8) -> (Tile, Tile) {
        let min = self.tile_from_merc(area.min_x, area.max_y, z);
        let max = self.tile_from_merc(area.max_x, area.min_y, z);
        (min, max)
    }
}

impl Default for WebMercator {
    fn default() -> Self {
        Self::DEFAULT
    }
}

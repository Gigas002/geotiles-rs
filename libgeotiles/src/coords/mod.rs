#[cfg(feature = "geographic")]
pub mod geographic;
#[cfg(feature = "mercator")]
pub mod mercator;

/// Default tile width/height in pixels; used by both CRS grids.
pub const DEFAULT_TILE_SIZE: u32 = 256;

/// XYZ (slippy-map) tile address. `y = 0` is north; use [`flip_y`] for TMS ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

/// Axis-aligned bounding box. Units depend on context: degrees for geographic,
/// metres for Web Mercator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

/// Flip Y between XYZ (y = 0 north) and TMS (y = 0 south) conventions.
/// The transform is its own inverse.
pub fn flip_y(y: u32, z: u8) -> u32 {
    (1u32 << z).saturating_sub(1).saturating_sub(y)
}

#[cfg(test)]
mod tests;

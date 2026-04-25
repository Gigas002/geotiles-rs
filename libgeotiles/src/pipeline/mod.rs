use crate::coords::{Bounds, Tile};

pub mod chunks;
pub use chunks::TileJob;

/// Abstraction over a tile coordinate grid (geographic or Web Mercator).
///
/// Implement this trait to use [`chunks::group_tiles_by_chunk`] with a custom grid.
pub trait TileGrid {
    fn tile_range(&self, area: Bounds, z: u8) -> (Tile, Tile);
    fn tile_bounds(&self, tile: Tile) -> Bounds;
}

#[cfg(test)]
mod tests;

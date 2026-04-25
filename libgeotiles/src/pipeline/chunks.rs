use std::collections::BTreeMap;

use tracing::debug;

use crate::coords::{Bounds, Tile};
use crate::gdal_io::source_window;
use crate::tile::PixelWindow;

use super::TileGrid;

pub struct TileJob {
    pub tile: Tile,
    pub window: PixelWindow,
}

/// Enumerate all tiles for zoom `z` that overlap the dataset and group them by
/// chunk id, where chunk `j` covers source rows `[j * chunk_size, (j+1) * chunk_size)`.
///
/// Returns a `BTreeMap<chunk_id, Vec<TileJob>>` in ascending row order so the caller
/// can iterate chunks sequentially and process tiles within each chunk in parallel.
pub fn group_tiles_by_chunk(
    grid: &dyn TileGrid,
    ds_bounds: Bounds,
    gt: &[f64; 6],
    ds_width: usize,
    ds_height: usize,
    z: u8,
    chunk_size: usize,
) -> BTreeMap<usize, Vec<TileJob>> {
    let (tile_min, tile_max) = grid.tile_range(ds_bounds, z);
    let mut map: BTreeMap<usize, Vec<TileJob>> = BTreeMap::new();

    for ty in tile_min.y..=tile_max.y {
        for tx in tile_min.x..=tile_max.x {
            let tile = Tile::new(tx, ty, z);
            let tile_bounds = grid.tile_bounds(tile);
            if let Some(win) = source_window(&tile_bounds, gt, ds_width, ds_height) {
                let chunk_id = win.row / chunk_size;
                debug!(tx, ty, z, chunk_id, ?win, "tile assigned to chunk");
                map.entry(chunk_id)
                    .or_default()
                    .push(TileJob { tile, window: win });
            }
        }
    }

    map
}

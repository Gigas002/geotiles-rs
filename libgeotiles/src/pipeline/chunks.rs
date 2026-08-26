use std::collections::BTreeMap;

use tracing::debug;

use crate::coords::{Bounds, Tile};
use crate::tiff_io::source_window;
use crate::tile::{DstRect, PixelWindow};

use super::TileGrid;

pub struct TileJob {
    pub tile: Tile,
    pub window: PixelWindow,
    /// Where in the `tile_size × tile_size` output canvas `window`'s resampled content
    /// belongs. Equal to the full canvas unless the tile straddles the dataset edge.
    pub dst: DstRect,
}

/// Enumerate all tiles for zoom `z` that overlap the dataset and group them by
/// chunk id, where chunk `j` covers source rows `[j * chunk_size, (j+1) * chunk_size)`.
///
/// Returns a `BTreeMap<chunk_id, Vec<TileJob>>` in ascending row order so the caller
/// can iterate chunks sequentially and process tiles within each chunk in parallel.
#[allow(clippy::too_many_arguments)]
pub fn group_tiles_by_chunk(
    grid: &dyn TileGrid,
    ds_bounds: Bounds,
    gt: &[f64; 6],
    ds_width: usize,
    ds_height: usize,
    z: u8,
    chunk_size: usize,
    tile_size: u32,
) -> BTreeMap<usize, Vec<TileJob>> {
    let (tile_min, tile_max) = grid.tile_range(ds_bounds, z);
    let mut map: BTreeMap<usize, Vec<TileJob>> = BTreeMap::new();

    for ty in tile_min.y..=tile_max.y {
        for tx in tile_min.x..=tile_max.x {
            let tile = Tile::new(tx, ty, z);
            let tile_bounds = grid.tile_bounds(tile);
            if let Some(win) = source_window(&tile_bounds, gt, ds_width, ds_height) {
                let chunk_id = win.row / chunk_size;
                let dst = dst_rect(tile_bounds, ds_bounds, tile_size);
                debug!(tx, ty, z, chunk_id, ?win, ?dst, "tile assigned to chunk");
                map.entry(chunk_id).or_default().push(TileJob {
                    tile,
                    window: win,
                    dst,
                });
            }
        }
    }

    map
}

/// Compute the sub-rectangle of a `tile_size × tile_size` canvas that corresponds to the
/// intersection of `tile_bounds` and `ds_bounds`, in geographic proportion.
///
/// Returns the full canvas when the dataset covers `tile_bounds` entirely.
fn dst_rect(tile_bounds: Bounds, ds_bounds: Bounds, tile_size: u32) -> DstRect {
    let clip_min_x = tile_bounds.min_x.max(ds_bounds.min_x);
    let clip_max_x = tile_bounds.max_x.min(ds_bounds.max_x);
    let clip_min_y = tile_bounds.min_y.max(ds_bounds.min_y);
    let clip_max_y = tile_bounds.max_y.min(ds_bounds.max_y);

    let tile_w = tile_bounds.max_x - tile_bounds.min_x;
    let tile_h = tile_bounds.max_y - tile_bounds.min_y;
    let size = tile_size as f64;

    // left/right come from x; top/bottom are y flipped (tile row 0 = max_y).
    let left = ((clip_min_x - tile_bounds.min_x) / tile_w * size).round();
    let right = ((clip_max_x - tile_bounds.min_x) / tile_w * size).round();
    let top = ((tile_bounds.max_y - clip_max_y) / tile_h * size).round();
    let bottom = ((tile_bounds.max_y - clip_min_y) / tile_h * size).round();

    let x = left.clamp(0.0, size) as u32;
    let x_end = right.clamp(0.0, size) as u32;
    let y = top.clamp(0.0, size) as u32;
    let y_end = bottom.clamp(0.0, size) as u32;

    DstRect {
        x,
        y,
        // Guard against rounding collapsing a real (non-empty) overlap to zero width/height.
        width: x_end.saturating_sub(x).max(1).min(tile_size - x),
        height: y_end.saturating_sub(y).max(1).min(tile_size - y),
    }
}

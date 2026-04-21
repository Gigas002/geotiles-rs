use crate::coords::{Bounds, Tile};

use super::TileGrid;
use super::chunks::group_tiles_by_chunk;

// ── Minimal TileGrid implementation for testing ───────────────────────────────

/// Axis-aligned grid of uniform tiles.
///
/// Tile (x, y) has bounds:
///   lon ∈ [origin_x + x*tile_w, origin_x + (x+1)*tile_w]
///   lat ∈ [origin_y - (y+1)*tile_h, origin_y - y*tile_h]
struct FixedGrid {
    min_tile: Tile,
    max_tile: Tile,
    tile_w: f64,
    tile_h: f64,
    origin_x: f64,
    origin_y: f64,
}

impl TileGrid for FixedGrid {
    fn tile_range(&self, _area: Bounds, _z: u8) -> (Tile, Tile) {
        (self.min_tile, self.max_tile)
    }

    fn tile_bounds(&self, tile: Tile) -> Bounds {
        Bounds {
            min_x: self.origin_x + tile.x as f64 * self.tile_w,
            max_x: self.origin_x + (tile.x + 1) as f64 * self.tile_w,
            min_y: self.origin_y - (tile.y + 1) as f64 * self.tile_h,
            max_y: self.origin_y - tile.y as f64 * self.tile_h,
        }
    }
}

// ── Shared test fixtures ──────────────────────────────────────────────────────

/// GeoTransform for a 100×100 dataset covering lon [0,10], lat [0,10].
///
/// origin (0, 10), pixel size 0.1°.  Pixel (col, row) maps to
/// lon = col*0.1, lat = 10 - row*0.1.
fn gt_100x100() -> [f64; 6] {
    [0.0, 0.1, 0.0, 10.0, 0.0, -0.1]
}

/// 2×2 grid: four tiles each covering 5×5 degrees of the 100×100 dataset.
///
/// Tile (0,0): lon [0,5] lat [5,10]  → source rows [0,50)
/// Tile (1,0): lon [5,10] lat [5,10] → source rows [0,50)
/// Tile (0,1): lon [0,5] lat [0,5]   → source rows [50,100)
/// Tile (1,1): lon [5,10] lat [0,5]  → source rows [50,100)
fn grid_2x2() -> FixedGrid {
    FixedGrid {
        min_tile: Tile::new(0, 0, 0),
        max_tile: Tile::new(1, 1, 0),
        tile_w: 5.0,
        tile_h: 5.0,
        origin_x: 0.0,
        origin_y: 10.0,
    }
}

fn world_bounds() -> Bounds {
    Bounds { min_x: 0.0, min_y: 0.0, max_x: 10.0, max_y: 10.0 }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn tiles_grouped_by_chunk_id() {
    // chunk_size=40: rows [0,40) → chunk 0, rows [40,80) → chunk 1.
    // Tiles (0,0) and (1,0) start at row 0  → chunk 0.
    // Tiles (0,1) and (1,1) start at row 50 → chunk 1.
    let groups = group_tiles_by_chunk(
        &grid_2x2(),
        world_bounds(),
        &gt_100x100(),
        100,
        100,
        0,
        40,
    );

    assert_eq!(groups.len(), 2, "expected two distinct chunk groups");
    assert_eq!(groups[&0].len(), 2, "chunk 0 should contain the two top-row tiles");
    assert_eq!(groups[&1].len(), 2, "chunk 1 should contain the two bottom-row tiles");
}

#[test]
fn chunk_size_larger_than_dataset_gives_one_chunk() {
    let groups = group_tiles_by_chunk(
        &grid_2x2(),
        world_bounds(),
        &gt_100x100(),
        100,
        100,
        0,
        1000,
    );

    assert_eq!(groups.len(), 1, "all tiles should land in chunk 0 when chunk_size >= ds_height");
    assert_eq!(groups[&0].len(), 4, "all four tiles in chunk 0");
}

#[test]
fn non_overlapping_tile_is_excluded() {
    // Tile (2,0) covers lon [10,15], which is outside the dataset (lon [0,10]).
    let grid = FixedGrid {
        min_tile: Tile::new(0, 0, 0),
        max_tile: Tile::new(2, 0, 0),
        tile_w: 5.0,
        tile_h: 10.0,
        origin_x: 0.0,
        origin_y: 10.0,
    };

    let groups = group_tiles_by_chunk(&grid, world_bounds(), &gt_100x100(), 100, 100, 0, 200);

    let total: usize = groups.values().map(|v| v.len()).sum();
    assert_eq!(total, 2, "only tiles x=0 and x=1 overlap the dataset extent");
}

#[test]
fn chunk_size_1_groups_tiles_by_first_row() {
    // Both top-row tiles start at source row 0; chunk_size=1 → chunk_id = 0/1 = 0.
    // Both bottom-row tiles start at row 50 → chunk_id = 50.
    let groups = group_tiles_by_chunk(
        &grid_2x2(),
        world_bounds(),
        &gt_100x100(),
        100,
        100,
        0,
        1,
    );

    assert_eq!(groups.len(), 2, "two distinct starting rows → two chunks");
    assert_eq!(groups[&0].len(), 2, "both tiles starting at row 0 in chunk 0");
    assert_eq!(groups[&50].len(), 2, "both tiles starting at row 50 in chunk 50");
}

#[test]
fn chunk_id_matches_row_div_chunk_size() {
    // Verify that chunk_id == window.row / chunk_size for every assigned tile.
    let chunk_size = 30;
    let groups = group_tiles_by_chunk(
        &grid_2x2(),
        world_bounds(),
        &gt_100x100(),
        100,
        100,
        0,
        chunk_size,
    );

    for (chunk_id, jobs) in &groups {
        for job in jobs {
            assert_eq!(
                job.window.row / chunk_size,
                *chunk_id,
                "tile {:?} window row {} not in chunk {}",
                job.tile,
                job.window.row,
                chunk_id
            );
        }
    }
}

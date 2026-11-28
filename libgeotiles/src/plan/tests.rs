use super::*;
use crate::coords::geographic::Geographic;
use crate::geo::{Bounds, Crs, GeoTransform};

/// Uniform grid of `tile_w × tile_h` unit tiles anchored at (0, 10), 256 px tiles.
struct FixedGrid {
    tile_w: f64,
    tile_h: f64,
    max: Tile,
}

impl TileGrid for FixedGrid {
    fn crs(&self) -> Crs {
        Crs::Wgs84
    }
    fn tile_size(&self) -> u32 {
        256
    }
    fn tile_range(&self, _area: Bounds, z: u8) -> (Tile, Tile) {
        (Tile::new(0, 0, z), self.max)
    }
    fn tile_bounds(&self, t: Tile) -> Bounds {
        Bounds {
            min_x: t.x as f64 * self.tile_w,
            max_x: (t.x + 1) as f64 * self.tile_w,
            min_y: 10.0 - (t.y + 1) as f64 * self.tile_h,
            max_y: 10.0 - t.y as f64 * self.tile_h,
        }
    }
}

/// 100×100 px raster covering x [0, 10], y [0, 10] at 0.1 units/px.
fn info_100() -> RasterInfo {
    RasterInfo {
        width: 100,
        height: 100,
        bands: 3,
        transform: GeoTransform::new(0.0, 10.0, 0.1, -0.1).unwrap(),
        epsg: None,
    }
}

#[test]
fn interior_tiles_get_exact_source_and_full_canvas() {
    let grid = FixedGrid {
        tile_w: 5.0,
        tile_h: 5.0,
        max: Tile::new(1, 1, 0),
    };
    let jobs = plan_zoom(&grid, &info_100(), 0);
    assert_eq!(jobs.len(), 4);
    let j = jobs.iter().find(|j| j.tile == Tile::new(1, 1, 0)).unwrap();
    assert_eq!(
        j.src,
        SrcRect {
            x: 50.0,
            y: 50.0,
            width: 50.0,
            height: 50.0
        }
    );
    assert_eq!(j.dst, DstRect::full(256));
}

#[test]
fn edge_tile_gets_partial_canvas_and_matching_source() {
    // Tile 1 covers x [8, 16]; only [8, 10] has data → left quarter of the canvas.
    let grid = FixedGrid {
        tile_w: 8.0,
        tile_h: 10.0,
        max: Tile::new(2, 0, 0),
    };
    let jobs = plan_zoom(&grid, &info_100(), 0);
    assert_eq!(jobs.len(), 2, "tile 2 (x [16, 24]) does not overlap");
    let j = jobs[1];
    assert_eq!(j.dst.x, 0);
    assert_eq!(j.dst.width, 64);
    assert!((j.src.x - 80.0).abs() < 1e-9 && (j.src.width - 20.0).abs() < 1e-9);
}

#[test]
fn deep_zoom_keeps_sub_pixel_source() {
    // Tiles 0.025 units wide = a quarter of a raster pixel each.
    let grid = FixedGrid {
        tile_w: 0.025,
        tile_h: 0.025,
        max: Tile::new(5, 0, 0),
    };
    let jobs = plan_zoom(&grid, &info_100(), 0);
    let j = jobs.iter().find(|j| j.tile.x == 5).unwrap();
    assert!((j.src.x - 1.25).abs() < 1e-9, "{:?}", j.src);
    assert!((j.src.width - 0.25).abs() < 1e-9, "{:?}", j.src);
    assert_eq!(j.dst, DstRect::full(256));
}

#[test]
fn sliver_overlap_keeps_one_pixel() {
    assert_eq!(snap(10.2, 10.3, 256), (10, 11));
    assert_eq!(snap(255.8, 256.0, 256), (255, 256));
    assert_eq!(snap(-3.0, 300.0, 256), (0, 256));
}

#[test]
fn geographic_world_raster_z0() {
    let info = RasterInfo {
        width: 720,
        height: 360,
        bands: 1,
        transform: GeoTransform::new(-180.0, 90.0, 0.5, -0.5).unwrap(),
        epsg: Some(4326),
    };
    let jobs = plan_zoom(&Geographic::DEFAULT, &info, 0);
    assert_eq!(jobs.len(), 2);
    for j in &jobs {
        assert_eq!(j.dst, DstRect::full(256));
        assert!((j.src.width - 360.0).abs() < 1e-9 && (j.src.height - 360.0).abs() < 1e-9);
    }
}

#[test]
fn rows_needed_widens_with_downscale_and_clamps() {
    let job = TileJob {
        tile: Tile::new(0, 0, 0),
        src: SrcRect {
            x: 0.0,
            y: 10.5,
            width: 512.0,
            height: 512.0,
        },
        dst: DstRect::full(256),
    };
    // radius = 3 * 2 + 1 = 7
    assert_eq!(job.rows_needed(3.0, 10_000), (3, 530));
    assert_eq!(job.rows_needed(3.0, 100), (3, 100));
    assert_eq!(job.cols_needed(3.0, 10_000), (0, 519));
}

#[test]
fn batches_group_by_start_row_and_cover_footprints() {
    let grid = FixedGrid {
        tile_w: 5.0,
        tile_h: 5.0,
        max: Tile::new(1, 1, 0),
    };
    let jobs = plan_zoom(&grid, &info_100(), 0);
    let batches = batch_jobs(jobs, 40, 3.0, 100);
    assert_eq!(batches.len(), 2);
    assert_eq!(batches[0].jobs.len(), 2);
    // rows [0, 50) + radius 3*1+1 = 4 → [0, 54); second batch [46, 100)
    assert_eq!((batches[0].row_start, batches[0].row_end), (0, 54));
    assert_eq!((batches[1].row_start, batches[1].row_end), (46, 100));
}

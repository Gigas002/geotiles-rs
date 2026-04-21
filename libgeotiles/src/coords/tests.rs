use super::flip_y;

#[cfg(any(feature = "geographic", feature = "mercator"))]
use super::{Bounds, Tile};

#[cfg(any(feature = "geographic", feature = "mercator"))]
const EPS: f64 = 1e-6;

// ── flip_y ────────────────────────────────────────────────────────────────────

#[test]
fn flip_y_z0_is_identity() {
    assert_eq!(flip_y(0, 0), 0);
}

#[test]
fn flip_y_z1_swaps() {
    assert_eq!(flip_y(0, 1), 1);
    assert_eq!(flip_y(1, 1), 0);
}

#[test]
fn flip_y_inverts_at_z2() {
    for y in 0..4u32 {
        assert_eq!(flip_y(y, 2), 3 - y);
    }
}

#[test]
fn flip_y_twice_is_identity() {
    for z in 0u8..=5 {
        for y in 0..(1u32 << z) {
            assert_eq!(flip_y(flip_y(y, z), z), y, "z={z} y={y}");
        }
    }
}

// ── EPSG:4326 ─────────────────────────────────────────────────────────────────

#[cfg(feature = "geographic")]
mod geo {
    use super::*;
    use crate::coords::geographic::Geographic;

    #[test]
    fn resolution_formula() {
        let g = Geographic::DEFAULT;
        assert!((g.resolution(0) - 180.0 / 256.0).abs() < EPS);
        assert!((g.resolution(0) / g.resolution(1) - 2.0).abs() < EPS);
        assert!((g.resolution(0) / g.resolution(5) - 32.0).abs() < EPS);
    }

    #[test]
    fn grid_dimensions() {
        let g = Geographic::DEFAULT;
        assert_eq!((g.x_count(0), g.y_count(0)), (2, 1));
        assert_eq!((g.x_count(1), g.y_count(1)), (4, 2));
        assert_eq!((g.x_count(2), g.y_count(2)), (8, 4));
    }

    #[test]
    fn bounds_z0_two_tiles() {
        let g = Geographic::DEFAULT;
        // Western hemisphere
        let w = g.bounds(Tile::new(0, 0, 0));
        assert!((w.min_x + 180.0).abs() < EPS);
        assert!((w.max_x).abs() < EPS);
        assert!((w.min_y + 90.0).abs() < EPS);
        assert!((w.max_y - 90.0).abs() < EPS);
        // Eastern hemisphere
        let e = g.bounds(Tile::new(1, 0, 0));
        assert!((e.min_x).abs() < EPS);
        assert!((e.max_x - 180.0).abs() < EPS);
    }

    #[test]
    fn bounds_z1_nw_and_se() {
        let g = Geographic::DEFAULT;
        // NW = (0,0,1): lon [-180,-90], lat [0,90]
        let nw = g.bounds(Tile::new(0, 0, 1));
        assert!((nw.min_x + 180.0).abs() < EPS);
        assert!((nw.max_x + 90.0).abs() < EPS);
        assert!((nw.min_y).abs() < EPS);
        assert!((nw.max_y - 90.0).abs() < EPS);
        // SE = (3,1,1): lon [90,180], lat [-90,0]
        let se = g.bounds(Tile::new(3, 1, 1));
        assert!((se.min_x - 90.0).abs() < EPS);
        assert!((se.max_x - 180.0).abs() < EPS);
        assert!((se.min_y + 90.0).abs() < EPS);
        assert!((se.max_y).abs() < EPS);
    }

    #[test]
    fn tile_known_points_z1() {
        let g = Geographic::DEFAULT;
        // (-90, 45) → step=90°, tx=floor(90/90)=1, ty=floor(45/90)=0
        assert_eq!(g.tile(-90.0, 45.0, 1), Tile::new(1, 0, 1));
        // (90, -45) → tx=floor(270/90)=3, ty=floor(135/90)=1
        assert_eq!(g.tile(90.0, -45.0, 1), Tile::new(3, 1, 1));
    }

    #[test]
    fn tile_contains_source_point() {
        let g = Geographic::DEFAULT;
        let cases = [
            (2.35, 48.85, 10u8),
            (139.69, 35.69, 12),
            (-73.98, 40.75, 8),
            (-43.17, -22.9, 9),
        ];
        for (lon, lat, z) in cases {
            let t = g.tile(lon, lat, z);
            let b = g.bounds(t);
            assert!(
                b.min_x <= lon && lon <= b.max_x,
                "lon {lon} outside tile at z={z}"
            );
            assert!(
                b.min_y <= lat && lat <= b.max_y,
                "lat {lat} outside tile at z={z}"
            );
        }
    }

    #[test]
    fn tile_range_world_z2() {
        let g = Geographic::DEFAULT;
        let world = Bounds {
            min_x: -180.0,
            min_y: -90.0,
            max_x: 180.0,
            max_y: 90.0,
        };
        let (min, max) = g.tile_range(world, 2);
        assert_eq!(min, Tile::new(0, 0, 2));
        assert_eq!(max, Tile::new(7, 3, 2));
    }

    #[test]
    fn default_and_new_are_equivalent() {
        assert_eq!(Geographic::DEFAULT, Geographic::new(256));
        assert_eq!(Geographic::default(), Geographic::new(256));
    }
}

// ── EPSG:3857 ─────────────────────────────────────────────────────────────────

#[cfg(feature = "mercator")]
mod merc {
    use super::*;
    use crate::coords::mercator::{self, ORIGIN_SHIFT, WebMercator};

    #[test]
    fn projection_roundtrip() {
        let cases = [(0.0, 0.0), (10.0, 50.0), (-73.98, 40.75), (2.35, 48.85)];
        for (lon, lat) in cases {
            let (mx, my) = mercator::to_merc(lon, lat);
            let (lon2, lat2) = mercator::from_merc(mx, my);
            assert!((lon - lon2).abs() < EPS, "lon roundtrip {lon} → {lon2}");
            assert!((lat - lat2).abs() < EPS, "lat roundtrip {lat} → {lat2}");
        }
    }

    #[test]
    fn equator_projects_to_origin() {
        let (mx, my) = mercator::to_merc(0.0, 0.0);
        assert!(mx.abs() < EPS);
        assert!(my.abs() < EPS);
    }

    #[test]
    fn resolution_formula() {
        let m = WebMercator::DEFAULT;
        assert!((m.resolution(0) / m.resolution(1) - 2.0).abs() < EPS);
        assert!((m.resolution(0) / m.resolution(5) - 32.0).abs() < EPS);
    }

    #[test]
    fn merc_bounds_z0_is_full_extent() {
        let m = WebMercator::DEFAULT;
        let b = m.merc_bounds(Tile::new(0, 0, 0));
        assert!((b.min_x + ORIGIN_SHIFT).abs() < EPS);
        assert!((b.max_x - ORIGIN_SHIFT).abs() < EPS);
        assert!((b.min_y + ORIGIN_SHIFT).abs() < EPS);
        assert!((b.max_y - ORIGIN_SHIFT).abs() < EPS);
    }

    #[test]
    fn merc_bounds_z1_quadrants() {
        let m = WebMercator::DEFAULT;
        // NW = (0,0,1): x ∈ [-OS,0], y ∈ [0,OS]
        let nw = m.merc_bounds(Tile::new(0, 0, 1));
        assert!((nw.min_x + ORIGIN_SHIFT).abs() < EPS);
        assert!((nw.max_x).abs() < EPS);
        assert!((nw.min_y).abs() < EPS);
        assert!((nw.max_y - ORIGIN_SHIFT).abs() < EPS);
        // SE = (1,1,1): x ∈ [0,OS], y ∈ [-OS,0]
        let se = m.merc_bounds(Tile::new(1, 1, 1));
        assert!((se.min_x).abs() < EPS);
        assert!((se.max_x - ORIGIN_SHIFT).abs() < EPS);
        assert!((se.min_y + ORIGIN_SHIFT).abs() < EPS);
        assert!((se.max_y).abs() < EPS);
    }

    #[test]
    fn tile_known_points_z1() {
        let m = WebMercator::DEFAULT;
        // (-90,45) → NW quadrant → (0,0,1)
        assert_eq!(m.tile(-90.0, 45.0, 1), Tile::new(0, 0, 1));
        // (90,-45) → SE quadrant → (1,1,1)
        assert_eq!(m.tile(90.0, -45.0, 1), Tile::new(1, 1, 1));
    }

    #[test]
    fn tile_contains_source_point() {
        let m = WebMercator::DEFAULT;
        let cases = [
            (2.35, 48.85, 10u8),
            (139.69, 35.69, 12),
            (-73.98, 40.75, 8),
            (-43.17, -22.9, 9),
        ];
        for (lon, lat, z) in cases {
            let t = m.tile(lon, lat, z);
            let b = m.bounds(t);
            assert!(
                b.min_x <= lon && lon <= b.max_x,
                "lon {lon} outside tile at z={z}"
            );
            assert!(
                b.min_y <= lat && lat <= b.max_y,
                "lat {lat} outside tile at z={z}"
            );
        }
    }

    #[test]
    fn tile_range_merc_world_z2() {
        let m = WebMercator::DEFAULT;
        let world = Bounds {
            min_x: -ORIGIN_SHIFT,
            min_y: -ORIGIN_SHIFT,
            max_x: ORIGIN_SHIFT,
            max_y: ORIGIN_SHIFT,
        };
        let (min, max) = m.tile_range_from_merc(world, 2);
        assert_eq!(min, Tile::new(0, 0, 2));
        assert_eq!(max, Tile::new(3, 3, 2));
    }

    #[test]
    fn default_and_new_are_equivalent() {
        assert_eq!(WebMercator::DEFAULT, WebMercator::new(256));
        assert_eq!(WebMercator::default(), WebMercator::new(256));
    }
}

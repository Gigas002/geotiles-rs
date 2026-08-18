use super::*;
#[cfg(any(feature = "geographic", feature = "mercator"))]
use libgeotiles::tile::DstRect;

#[cfg(any(feature = "geographic", feature = "mercator"))]
#[test]
fn apply_bands_pads_alpha_opaque_inside_dst_and_transparent_outside() {
    // 2×2 tile, RGB → RGBA, real data only in the top-left pixel (dst = 1×1 at origin).
    let pixels = vec![
        10, 20, 30, 0, 0, 0, // row 0: real pixel, padding pixel
        0, 0, 0, 0, 0, 0, // row 1: padding, padding
    ];
    let dst = DstRect {
        x: 0,
        y: 0,
        width: 1,
        height: 1,
    };
    let out = apply_bands(pixels, 3, 4, 2, dst);
    assert_eq!(
        out,
        vec![10, 20, 30, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    );
}

#[cfg(any(feature = "geographic", feature = "mercator"))]
#[test]
fn apply_bands_noop_when_band_counts_match() {
    let pixels = vec![1, 2, 3, 4];
    let out = apply_bands(pixels.clone(), 4, 4, 1, DstRect::full(1));
    assert_eq!(out, pixels);
}

#[test]
fn dataset_bounds_north_up() {
    let gt = [-180.0, 0.5, 0.0, 90.0, 0.0, -0.5];
    let bounds = dataset_bounds(&gt, 720, 360);
    assert!((bounds.min_x - (-180.0)).abs() < 1e-9);
    assert!((bounds.max_x - 180.0).abs() < 1e-9);
    assert!((bounds.max_y - 90.0).abs() < 1e-9);
    assert!((bounds.min_y - (-90.0)).abs() < 1e-9);
}

#[test]
fn dataset_bounds_partial() {
    let gt = [10.0, 1.0, 0.0, 50.0, 0.0, -1.0];
    let bounds = dataset_bounds(&gt, 5, 5);
    assert!((bounds.min_x - 10.0).abs() < 1e-9);
    assert!((bounds.max_x - 15.0).abs() < 1e-9);
    assert!((bounds.max_y - 50.0).abs() < 1e-9);
    assert!((bounds.min_y - 45.0).abs() < 1e-9);
}

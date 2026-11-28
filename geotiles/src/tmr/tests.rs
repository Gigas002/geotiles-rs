use libgeotiles::Crs;

use super::*;

#[test]
fn units_per_pixel_geographic_z0() {
    let result = units_per_pixel(Crs::Wgs84, 0, 256);
    let expected = 180.0_f64 / 256.0;
    assert!(
        (result - expected).abs() < 1e-9,
        "expected {expected}, got {result}"
    );
}

#[test]
fn units_per_pixel_mercator_z0() {
    let result = units_per_pixel(Crs::WebMercator, 0, 256);
    let expected = std::f64::consts::PI * 6_378_137.0 * 2.0 / 256.0;
    assert!(
        (result - expected).abs() < 1e-3,
        "expected {expected}, got {result}"
    );
}

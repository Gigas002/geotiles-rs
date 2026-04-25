use libgeotiles::Format;

use super::*;

#[test]
fn units_per_pixel_geographic_z0() {
    let result = units_per_pixel(Crs::Geographic, 0, 256);
    let expected = 180.0_f64 / 256.0;
    assert!(
        (result - expected).abs() < 1e-9,
        "expected {expected}, got {result}"
    );
}

#[test]
fn units_per_pixel_mercator_z0() {
    let result = units_per_pixel(Crs::Mercator, 0, 256);
    let expected = std::f64::consts::PI * 6_378_137.0 * 2.0 / 256.0;
    assert!(
        (result - expected).abs() < 1e-3,
        "expected {expected}, got {result}"
    );
}

#[test]
fn mime_type_all_formats() {
    assert_eq!(mime_type(Format::Png), "image/png");
    assert_eq!(mime_type(Format::Jpeg), "image/jpeg");
    assert_eq!(mime_type(Format::WebP), "image/webp");
    assert_eq!(mime_type(Format::Avif), "image/avif");
    assert_eq!(mime_type(Format::Jxl), "image/jxl");
}

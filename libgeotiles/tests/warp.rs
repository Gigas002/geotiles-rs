use std::path::PathBuf;

use gdal::DriverManager;
use gdal::spatial_ref::SpatialRef;

fn synthetic_4326_gtiff(tag: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("geotiles_test_warp_{tag}.tif"));
    let driver = DriverManager::get_driver_by_name("GTiff").unwrap();
    let mut ds = driver
        .create_with_band_type::<u8, _>(&path, 64, 64, 3)
        .unwrap();
    // Small tile in Western Europe: lon 0–1°, lat 49–50°
    ds.set_geo_transform(&[0.0, 1.0 / 64.0, 0.0, 50.0, 0.0, -1.0 / 64.0])
        .unwrap();
    ds.set_spatial_ref(&SpatialRef::from_epsg(4326).unwrap())
        .unwrap();
    path
}

#[test]
fn warp_4326_to_3857_changes_geotransform() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let path = synthetic_4326_gtiff("change");
    let (src, src_info) = libgeotiles::gdal_io::open_dataset(&path).unwrap();

    // Source is EPSG:4326: pixel size should be in degrees (~0.015°/px for 64px over 1°)
    let src_pixel_x = src_info.geo_transform[1];
    assert!(
        src_pixel_x < 1.0,
        "source pixel size should be in degrees, got {src_pixel_x}"
    );

    let warped = libgeotiles::gdal_io::warp_to_epsg(&src, 3857)
        .unwrap()
        .expect("4326→3857 should produce a warped VRT");

    let (w, h) = warped.raster_size();
    assert!(w > 0 && h > 0, "warped dataset has zero dimensions");

    let gt = warped.geo_transform().unwrap();
    let pixel_x = gt[1];

    // EPSG:3857 pixel size is in metres; 1° longitude near 50°N ≈ 71 km
    // For a 64-pixel dataset covering ~1° it should be thousands of metres per pixel
    assert!(
        pixel_x > 100.0,
        "warped pixel size should be in metres (>100), got {pixel_x}"
    );

    // Origin should now be in Web Mercator metres, not degrees
    let origin_x = gt[0];
    assert!(
        origin_x.abs() < 20_100_000.0,
        "warped origin_x out of Web Mercator range: {origin_x}"
    );
}

#[test]
fn warp_same_epsg_returns_none() {
    let path = synthetic_4326_gtiff("same");
    let (src, _) = libgeotiles::gdal_io::open_dataset(&path).unwrap();
    let result = libgeotiles::gdal_io::warp_to_epsg(&src, 4326).unwrap();
    assert!(result.is_none(), "warping to same CRS should return None");
}

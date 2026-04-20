use std::path::PathBuf;

use gdal::DriverManager;
use gdal::spatial_ref::SpatialRef;

fn create_synthetic_gtiff() -> PathBuf {
    let path = std::env::temp_dir().join("geotiles_test_synthetic.tif");

    let driver = DriverManager::get_driver_by_name("GTiff").unwrap();
    let mut ds = driver
        .create_with_band_type::<u8, _>(&path, 4, 4, 1)
        .unwrap();

    let geo_transform = [10.0, 0.5, 0.0, 50.0, 0.0, -0.5];
    ds.set_geo_transform(&geo_transform).unwrap();

    let srs = SpatialRef::from_epsg(4326).unwrap();
    ds.set_projection(&srs.to_wkt().unwrap()).unwrap();

    path
}

#[test]
fn open_dataset_returns_correct_metadata() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let path = create_synthetic_gtiff();
    let (_ds, info) = libgeotiles::gdal_io::open_dataset(&path).unwrap();

    assert_eq!(info.width, 4);
    assert_eq!(info.height, 4);
    assert_eq!(info.band_count, 1);
    assert!((info.geo_transform[0] - 10.0).abs() < 1e-9); // x origin
    assert!((info.geo_transform[1] - 0.5).abs() < 1e-9); // x pixel size
    assert!((info.geo_transform[3] - 50.0).abs() < 1e-9); // y origin
    assert!(!info.projection.is_empty());
}

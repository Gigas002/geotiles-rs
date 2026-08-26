use std::path::PathBuf;

use tiff::encoder::{TiffEncoder, colortype};
use tiff::tags::Tag;

/// GeoTIFF tag numbers not modeled by `tiff::tags::Tag` itself.
const MODEL_PIXEL_SCALE_TAG: u16 = 33550;
const MODEL_TIEPOINT_TAG: u16 = 33922;

/// Write a tiny single-band 4x4 GeoTIFF with a simple north-up geotransform:
/// origin (10.0, 50.0), 0.5 units/pixel in x, -0.5 units/pixel in y.
fn create_synthetic_gtiff(tag: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("geotiles_test_tiff_io_{tag}.tif"));
    let file = std::fs::File::create(&path).unwrap();
    let mut tiff = TiffEncoder::new(file).unwrap();

    let mut image = tiff.new_image::<colortype::Gray8>(4, 4).unwrap();
    image
        .encoder()
        .write_tag(
            Tag::from_u16_exhaustive(MODEL_PIXEL_SCALE_TAG),
            &[0.5_f64, 0.5_f64, 0.0_f64][..],
        )
        .unwrap();
    image
        .encoder()
        .write_tag(
            Tag::from_u16_exhaustive(MODEL_TIEPOINT_TAG),
            &[0.0_f64, 0.0_f64, 0.0_f64, 10.0_f64, 50.0_f64, 0.0_f64][..],
        )
        .unwrap();

    let data: Vec<u8> = (0..16u8).collect();
    image.write_data(&data).unwrap();

    path
}

#[test]
fn open_dataset_returns_correct_metadata() {
    let path = create_synthetic_gtiff("metadata");
    let (_ds, info) = libgeotiles::tiff_io::open_dataset(&path).unwrap();

    assert_eq!(info.width, 4);
    assert_eq!(info.height, 4);
    assert_eq!(info.band_count, 1);
    assert!((info.geo_transform[0] - 10.0).abs() < 1e-9); // x origin
    assert!((info.geo_transform[1] - 0.5).abs() < 1e-9); // x pixel size
    assert!((info.geo_transform[3] - 50.0).abs() < 1e-9); // y origin
    assert!((info.geo_transform[5] - (-0.5)).abs() < 1e-9); // y pixel size (negative, top-down)
}

#[test]
fn open_dataset_without_georeferencing_errors() {
    let path = std::env::temp_dir().join("geotiles_test_tiff_io_no_georef.tif");
    let file = std::fs::File::create(&path).unwrap();
    let mut tiff = TiffEncoder::new(file).unwrap();
    let image = tiff.new_image::<colortype::Gray8>(2, 2).unwrap();
    image.write_data(&[0u8, 1, 2, 3]).unwrap();

    let result = libgeotiles::tiff_io::open_dataset(&path);
    assert!(
        result.is_err(),
        "expected error for a TIFF with no georeferencing tags"
    );
}

#[test]
fn read_chunk_returns_expected_pixels() {
    let path = create_synthetic_gtiff("read_chunk");
    let (mut ds, _info) = libgeotiles::tiff_io::open_dataset(&path).unwrap();

    let chunk = libgeotiles::tiff_io::read_chunk(&mut ds, 1, 2).unwrap();
    assert_eq!(chunk.band_count(), 1);
    assert_eq!(chunk.row_start, 1);
    assert_eq!(chunk.row_count, 2);
    // 4x4 image with values 0..16 row-major; rows 1..3 are [4..8) and [8..12).
    assert_eq!(chunk.band_data[0], vec![4, 5, 6, 7, 8, 9, 10, 11]);
}

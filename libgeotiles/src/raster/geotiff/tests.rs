use std::io::Cursor;

use tiff::encoder::{TiffEncoder, colortype};
use tiff::tags::Tag;

use super::GeoTiff;
use crate::error::Error;
use crate::raster::RasterSource;

/// Encode a 4×4 GeoTIFF of `C` with one row per strip and the given GeoKey directory.
fn encode<C: colortype::ColorType<Inner = T>, T: tiff::encoder::TiffValue + Copy>(
    data: &[T],
    geo: bool,
    keys: Option<&[u16]>,
) -> Cursor<Vec<u8>>
where
    [T]: tiff::encoder::TiffValue,
{
    let mut buf = Cursor::new(Vec::new());
    {
        let mut tiff = TiffEncoder::new(&mut buf).unwrap();
        let mut image = tiff.new_image::<C>(4, 4).unwrap();
        image.rows_per_strip(1).unwrap();
        if geo {
            let enc = image.encoder();
            enc.write_tag(Tag::ModelPixelScaleTag, &[0.5_f64, 0.5, 0.0][..])
                .unwrap();
            enc.write_tag(
                Tag::ModelTiepointTag,
                &[0.0_f64, 0.0, 0.0, 10.0, 50.0, 0.0][..],
            )
            .unwrap();
        }
        if let Some(keys) = keys {
            image
                .encoder()
                .write_tag(Tag::GeoKeyDirectoryTag, keys)
                .unwrap();
        }
        image.write_data(data).unwrap();
    }
    buf.set_position(0);
    buf
}

fn gray() -> Vec<u8> {
    (0..16).collect()
}

#[test]
fn reads_metadata_and_transform() {
    let tif = GeoTiff::from_reader(encode::<colortype::Gray8, u8>(&gray(), true, None)).unwrap();
    let info = tif.info();
    assert_eq!(
        (info.width, info.height, info.bands, info.epsg),
        (4, 4, 1, None)
    );
    assert_eq!(info.transform.origin(), (10.0, 50.0));
    assert_eq!(info.transform.pixel_size(), (0.5, -0.5));
}

#[test]
fn reads_rows_across_strips() {
    let mut tif =
        GeoTiff::from_reader(encode::<colortype::Gray8, u8>(&gray(), true, None)).unwrap();
    let block = tif.read_rows(1, 2).unwrap();
    assert_eq!((block.row_start, block.rows), (1, 2));
    assert_eq!(block.data, (4..12).collect::<Vec<u8>>());
    assert_eq!(tif.read_rows(3, 10).unwrap().rows, 1);
}

#[test]
fn rgba_rows_stay_interleaved() {
    let data: Vec<u8> = (0..64).collect();
    let mut tif = GeoTiff::from_reader(encode::<colortype::RGBA8, u8>(&data, true, None)).unwrap();
    assert_eq!(tif.info().bands, 4);
    assert_eq!(
        tif.read_rows(2, 1).unwrap().data,
        (32..48).collect::<Vec<u8>>()
    );
}

#[test]
fn epsg_from_geokeys() {
    // version 1.1.0, 2 keys: GTModelType = geographic, GeographicType = 4326
    let keys = [1, 1, 0, 2, 1024, 0, 1, 2, 2048, 0, 1, 4326];
    let tif =
        GeoTiff::from_reader(encode::<colortype::Gray8, u8>(&gray(), true, Some(&keys))).unwrap();
    assert_eq!(tif.info().epsg, Some(4326));

    let keys = [1, 1, 0, 2, 1024, 0, 1, 1, 3072, 0, 1, 3857];
    let tif =
        GeoTiff::from_reader(encode::<colortype::Gray8, u8>(&gray(), true, Some(&keys))).unwrap();
    assert_eq!(tif.info().epsg, Some(3857));

    let keys = [1, 1, 0, 1, 3072, 0, 1, 32767];
    let tif =
        GeoTiff::from_reader(encode::<colortype::Gray8, u8>(&gray(), true, Some(&keys))).unwrap();
    assert_eq!(tif.info().epsg, None, "user-defined CRS has no EPSG code");
}

#[test]
fn pixel_is_point_shifts_origin_half_a_pixel() {
    let keys = [1, 1, 0, 1, 1025, 0, 1, 2];
    let tif =
        GeoTiff::from_reader(encode::<colortype::Gray8, u8>(&gray(), true, Some(&keys))).unwrap();
    assert_eq!(tif.info().transform.origin(), (9.75, 50.25));
}

#[test]
fn rejects_missing_georeferencing() {
    let err = GeoTiff::from_reader(encode::<colortype::Gray8, u8>(&gray(), false, None))
        .err()
        .unwrap();
    assert!(matches!(err, Error::MissingGeoreferencing), "{err}");
}

#[test]
fn rejects_16_bit() {
    let data: Vec<u16> = (0..16).collect();
    let err = GeoTiff::from_reader(encode::<colortype::Gray16, u16>(&data, true, None))
        .err()
        .unwrap();
    assert!(matches!(err, Error::Unsupported(_)), "{err}");
}

#[test]
fn open_nonexistent_is_io_error() {
    let err = GeoTiff::open(std::path::Path::new("/definitely/not/here.tif"))
        .err()
        .unwrap();
    assert!(matches!(err, Error::Io(_)));
}

//! GeoTIFF on disk → PNG tiles on disk, through the public API only.

#![cfg(all(feature = "geotiff", feature = "png"))]

use std::path::PathBuf;

use libgeotiles::{CpuResampler, DirSink, GeoTiff, Geographic, TilerOptions, generate};
use tiff::encoder::{TiffEncoder, colortype};
use tiff::tags::Tag;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("libgeotiles-e2e-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// 360×180 RGB raster covering lon [0, 90], lat [0, 45], EPSG:4326.
fn write_geotiff(path: &std::path::Path) {
    let (w, h) = (360u32, 180u32);
    let mut tiff = TiffEncoder::new(std::fs::File::create(path).unwrap()).unwrap();
    let mut image = tiff.new_image::<colortype::RGB8>(w, h).unwrap();
    image.rows_per_strip(16).unwrap();
    let enc = image.encoder();
    enc.write_tag(Tag::ModelPixelScaleTag, &[0.25_f64, 0.25, 0.0][..])
        .unwrap();
    enc.write_tag(
        Tag::ModelTiepointTag,
        &[0.0_f64, 0.0, 0.0, 0.0, 45.0, 0.0][..],
    )
    .unwrap();
    enc.write_tag(
        Tag::GeoKeyDirectoryTag,
        &[1u16, 1, 0, 2, 1024, 0, 1, 2, 2048, 0, 1, 4326][..],
    )
    .unwrap();
    let data: Vec<u8> = (0..h)
        .flat_map(|r| (0..w).flat_map(move |c| [r as u8, c as u8, 128]))
        .collect();
    image.write_data(&data).unwrap();
}

#[test]
fn geotiff_to_png_tiles() {
    let dir = scratch("png");
    let input = dir.join("in.tif");
    write_geotiff(&input);
    let out = dir.join("tiles");

    let mut source = GeoTiff::open(&input).unwrap();
    assert_eq!(source_epsg(&source), Some(4326));
    let opts = TilerOptions {
        zooms: 0..=3,
        bands: Some(4),
        chunk_rows: 32,
        ..Default::default()
    };
    let stats = generate(
        &mut source,
        &Geographic::DEFAULT,
        &CpuResampler,
        &DirSink::new(&out, false),
        &opts,
    )
    .unwrap();

    // lon [0, 90] × lat [0, 45]: z0 1, z1 1, z2 2×1 (45° tiles), z3 4×2 (22.5°).
    assert_eq!(stats.tiles, 1 + 1 + 2 + 8);
    for path in [
        "0/1/0.png",
        "1/2/0.png",
        "2/4/1.png",
        "2/5/1.png",
        "3/8/2.png",
        "3/11/3.png",
    ] {
        let png = std::fs::read(out.join(path)).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert_eq!(&png[1..4], b"PNG", "{path} is not a PNG");
    }

    std::fs::remove_dir_all(&dir).unwrap();
}

fn source_epsg(source: &GeoTiff) -> Option<u32> {
    use libgeotiles::RasterSource;
    source.info().epsg
}

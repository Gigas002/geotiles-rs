/// Integration tests for the Phase-4 tile pipeline (source_window → read_chunk → crop → encode).
///
/// Uses a tiny synthetic GeoTIFF created in a temp directory — no network, no LFS.
#[cfg(all(feature = "png", feature = "geographic"))]
use std::path::PathBuf;

#[cfg(all(feature = "png", feature = "geographic"))]
use gdal::DriverManager;
#[cfg(all(feature = "png", feature = "geographic"))]
use gdal::spatial_ref::SpatialRef;

/// Create a 64×64 3-band EPSG:4326 GeoTIFF covering lon [0°,1°], lat [49°,50°].
/// Pixels are filled with a simple gradient so we can detect corruption.
#[cfg(all(feature = "png", feature = "geographic"))]
fn synthetic_4326_gtiff(tag: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("geotiles_test_tile_{tag}.tif"));
    let driver = DriverManager::get_driver_by_name("GTiff").unwrap();
    let mut ds = driver
        .create_with_band_type::<u8, _>(&path, 64, 64, 3)
        .unwrap();
    ds.set_geo_transform(&[0.0, 1.0 / 64.0, 0.0, 50.0, 0.0, -1.0 / 64.0])
        .unwrap();
    ds.set_spatial_ref(&SpatialRef::from_epsg(4326).unwrap())
        .unwrap();

    for band_idx in 1..=3u8 {
        let mut band = ds.rasterband(band_idx as usize).unwrap();
        let data: Vec<u8> = (0u16..64 * 64)
            .map(|i| ((i as u32 * band_idx as u32) % 256) as u8)
            .collect();
        let mut buf = gdal::raster::Buffer::new((64, 64), data);
        band.write((0, 0), (64, 64), &mut buf).unwrap();
    }
    path
}

/// Helper: collect all tile files under a directory tree (recursive).
#[cfg(all(feature = "png", feature = "geographic"))]
fn collect_tiles(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_tiles_inner(dir, &mut out);
    out.sort();
    out
}

#[cfg(all(feature = "png", feature = "geographic"))]
fn collect_tiles_inner(dir: &std::path::Path, acc: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_tiles_inner(&path, acc);
        } else {
            acc.push(path);
        }
    }
}

#[cfg(feature = "png")]
#[cfg(feature = "geographic")]
#[test]
fn crop_produces_tiles_at_zoom4() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let src = synthetic_4326_gtiff("zoom4");
    let out = std::env::temp_dir().join("geotiles_test_tile_zoom4_out");
    let _ = std::fs::remove_dir_all(&out);

    libgeotiles::GeoTiff::open(&src)
        .unwrap()
        .zoom(4..=4)
        .chunk_size(512)
        .format(libgeotiles::Format::Png)
        .output(&out)
        .crop()
        .unwrap();

    let tiles = collect_tiles(&out);
    assert!(!tiles.is_empty(), "expected at least one tile at zoom 4");

    // Every tile file must be a valid PNG (starts with PNG magic bytes).
    for tile in &tiles {
        let bytes = std::fs::read(tile).unwrap();
        assert!(
            bytes.starts_with(b"\x89PNG"),
            "tile {tile:?} is not a valid PNG"
        );
    }
}

/// Verify that chunk_size=1 (one GDAL read per row) produces byte-identical tiles
/// to the default chunk_size. This catches any off-by-one in chunk boundary math.
#[cfg(feature = "png")]
#[cfg(feature = "geographic")]
#[test]
fn crop_chunk_size_1_matches_default() {
    let src = synthetic_4326_gtiff("chunk_cmp");

    let out_default = std::env::temp_dir().join("geotiles_test_tile_chunk_default");
    let out_one = std::env::temp_dir().join("geotiles_test_tile_chunk_1");
    let _ = std::fs::remove_dir_all(&out_default);
    let _ = std::fs::remove_dir_all(&out_one);

    for (out, cs) in [(&out_default, 512usize), (&out_one, 1usize)] {
        libgeotiles::GeoTiff::open(&src)
            .unwrap()
            .zoom(4..=4)
            .chunk_size(cs)
            .format(libgeotiles::Format::Png)
            .output(out)
            .crop()
            .unwrap();
    }

    let tiles_default = collect_tiles(&out_default);
    let tiles_one = collect_tiles(&out_one);

    assert_eq!(
        tiles_default.len(),
        tiles_one.len(),
        "tile counts differ between chunk sizes"
    );

    for (a, b) in tiles_default.iter().zip(tiles_one.iter()) {
        let data_a = std::fs::read(a).unwrap();
        let data_b = std::fs::read(b).unwrap();
        assert_eq!(
            data_a, data_b,
            "tile {a:?} differs between chunk_size=512 and chunk_size=1"
        );
    }
}

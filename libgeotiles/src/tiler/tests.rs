use std::collections::BTreeMap;
use std::sync::Mutex;

use super::*;
use crate::coords::Tile;
use crate::coords::geographic::Geographic;
use crate::coords::mercator::WebMercator;
use crate::geo::GeoTransform;
use crate::raster::{MemoryRaster, RasterInfo};
use crate::resample::cpu::CpuResampler;

#[derive(Default)]
struct MemSink(Mutex<BTreeMap<Tile, Vec<u8>>>);

impl TileSink for MemSink {
    fn write_tile(&self, tile: Tile, _format: Format, data: &[u8]) -> Result<()> {
        assert!(
            self.0.lock().unwrap().insert(tile, data.to_vec()).is_none(),
            "tile {tile:?} written twice"
        );
        Ok(())
    }
}

/// 720×360 RGB world raster with a smooth pattern, EPSG:4326 declared.
fn world(epsg: Option<u32>) -> MemoryRaster {
    let (w, h) = (720usize, 360usize);
    let data = (0..h)
        .flat_map(|r| {
            (0..w).flat_map(move |c| [(r % 256) as u8, (c % 256) as u8, ((r + c) / 5 % 256) as u8])
        })
        .collect();
    MemoryRaster::new(
        RasterInfo {
            width: w,
            height: h,
            bands: 3,
            transform: GeoTransform::new(-180.0, 90.0, 0.5, -0.5).unwrap(),
            epsg,
        },
        data,
    )
    .unwrap()
}

fn run(
    src: &mut MemoryRaster,
    grid: &dyn TileGrid,
    opts: &TilerOptions,
) -> Result<BTreeMap<Tile, Vec<u8>>> {
    let sink = MemSink::default();
    generate(src, grid, &CpuResampler, &sink, opts)?;
    Ok(sink.0.into_inner().unwrap())
}

#[test]
#[cfg(feature = "png")]
fn world_raster_produces_every_tile_once() {
    let opts = TilerOptions {
        zooms: 0..=2,
        ..Default::default()
    };
    let tiles = run(&mut world(Some(4326)), &Geographic::DEFAULT, &opts).unwrap();
    assert_eq!(tiles.len(), 2 + 8 + 32);
}

#[test]
#[cfg(feature = "png")]
fn output_does_not_depend_on_chunk_rows() {
    let base = TilerOptions {
        zooms: 0..=3,
        bands: Some(4),
        ..Default::default()
    };
    let big = run(
        &mut world(Some(4326)),
        &Geographic::DEFAULT,
        &TilerOptions {
            chunk_rows: 100_000,
            ..base.clone()
        },
    )
    .unwrap();
    let small = run(
        &mut world(Some(4326)),
        &Geographic::DEFAULT,
        &TilerOptions {
            chunk_rows: 7,
            ..base
        },
    )
    .unwrap();
    assert_eq!(big, small);
}

#[test]
fn crs_mismatch_is_rejected() {
    let err = run(
        &mut world(Some(4326)),
        &WebMercator::DEFAULT,
        &TilerOptions::default(),
    )
    .unwrap_err();
    assert!(
        matches!(err, Error::CrsMismatch { raster: 4326, .. }),
        "{err}"
    );
    let err = run(
        &mut world(Some(3395)),
        &Geographic::DEFAULT,
        &TilerOptions::default(),
    )
    .unwrap_err();
    assert!(
        matches!(err, Error::CrsMismatch { raster: 3395, .. }),
        "{err}"
    );
}

#[test]
#[cfg(feature = "png")]
fn undeclared_crs_is_trusted() {
    assert_eq!(
        run(
            &mut world(None),
            &Geographic::DEFAULT,
            &TilerOptions::default()
        )
        .unwrap()
        .len(),
        2
    );
}

#[test]
#[cfg(not(feature = "jxl"))]
fn unavailable_format_is_rejected_before_any_work() {
    let opts = TilerOptions {
        format: Format::Jxl,
        ..Default::default()
    };
    let err = run(&mut world(Some(4326)), &Geographic::DEFAULT, &opts).unwrap_err();
    assert!(matches!(err, Error::InvalidOptions(_)), "{err}");
}

#[test]
fn invalid_options_are_rejected() {
    let mut src = world(Some(4326));
    let grid = Geographic::DEFAULT;
    for opts in [
        TilerOptions {
            bands: Some(5),
            ..Default::default()
        },
        TilerOptions {
            chunk_rows: 0,
            ..Default::default()
        },
        TilerOptions {
            zooms: 0..=31,
            ..Default::default()
        },
    ] {
        assert!(run(&mut src, &grid, &opts).is_err(), "{opts:?}");
    }
    assert!(run(&mut src, &Geographic::new(0), &TilerOptions::default()).is_err());
}

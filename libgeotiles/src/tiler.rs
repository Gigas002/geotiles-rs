//! The tiling driver: source → plan → resample → encode → sink.

use std::ops::RangeInclusive;

use rayon::prelude::*;
use tracing::{debug, info, info_span, warn};

use crate::Result;
use crate::coords::{MAX_ZOOM, TileGrid};
use crate::encode::options::EncodeOptions;
use crate::encode::{Format, encode_tile};
use crate::error::Error;
use crate::geo::Crs;
use crate::pixels::convert_bands;
use crate::plan::{batch_jobs, plan_zoom};
use crate::raster::RasterSource;
use crate::resample::Resampler;
use crate::sink::TileSink;

/// What to produce.
#[derive(Debug, Clone)]
pub struct TilerOptions {
    pub zooms: RangeInclusive<u8>,
    pub format: Format,
    pub encode: EncodeOptions,
    /// Output bands (1–4); `None` keeps the source's.
    pub bands: Option<usize>,
    /// Raster rows per read batch. Bounds memory to about
    /// `chunk_rows × width × bands` bytes, except at zooms where a single tile needs more.
    pub chunk_rows: usize,
}

impl Default for TilerOptions {
    fn default() -> Self {
        Self {
            zooms: 0..=0,
            format: Format::default(),
            encode: EncodeOptions::default(),
            bands: None,
            chunk_rows: 512,
        }
    }
}

/// Summary of a run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    pub tiles: usize,
}

/// Cut `source` into tiles of `grid` for every zoom in `opts.zooms`.
///
/// The source must already be in `grid`'s CRS: a declared EPSG code that disagrees is an
/// error ([`Error::CrsMismatch`]); a source without one is trusted.
pub fn generate<S: RasterSource + ?Sized>(
    source: &mut S,
    grid: &dyn TileGrid,
    resampler: &dyn Resampler,
    sink: &dyn TileSink,
    opts: &TilerOptions,
) -> Result<Stats> {
    let info = source.info().clone();
    info.validate()?;
    validate(&info.epsg, grid, opts)?;
    if !opts.format.is_available() {
        return Err(Error::InvalidOptions(format!(
            "{:?} output needs the `{}` feature",
            opts.format,
            opts.format.extension().replace("jpg", "jpeg")
        )));
    }

    let tile_size = grid.tile_size();
    let support = resampler.support();
    let out_bands = opts.bands.unwrap_or(info.bands);
    let mut stats = Stats::default();

    for z in opts.zooms.clone() {
        let _span = info_span!("zoom", z).entered();
        let jobs = plan_zoom(grid, &info, z);
        let tiles = jobs.len();
        let batches = batch_jobs(jobs, opts.chunk_rows, support, info.height);
        info!(z, tiles, batches = batches.len(), "zoom planned");

        for batch in batches {
            debug!(
                row_start = batch.row_start,
                row_end = batch.row_end,
                tiles = batch.jobs.len(),
                "batch"
            );
            let block = source.read_rows(batch.row_start, batch.row_end - batch.row_start)?;
            batch.jobs.par_iter().try_for_each(|job| {
                let px = resampler.resample(&block, job, tile_size)?;
                let px = convert_bands(px, out_bands, tile_size, job.dst);
                let encoded = encode_tile(
                    &px,
                    tile_size,
                    tile_size,
                    out_bands,
                    opts.format,
                    &opts.encode,
                )?;
                sink.write_tile(job.tile, opts.format, &encoded)
            })?;
        }
        stats.tiles += tiles;
    }
    Ok(stats)
}

fn validate(epsg: &Option<u32>, grid: &dyn TileGrid, opts: &TilerOptions) -> Result<()> {
    let invalid = |msg: String| Err(Error::InvalidOptions(msg));
    if grid.tile_size() == 0 {
        return invalid("tile size must be positive".into());
    }
    if opts.zooms.is_empty() {
        return invalid(format!("empty zoom range {:?}", opts.zooms));
    }
    if *opts.zooms.end() > MAX_ZOOM {
        return invalid(format!("max zoom {} exceeds {MAX_ZOOM}", opts.zooms.end()));
    }
    if let Some(b) = opts.bands
        && !(1..=4).contains(&b)
    {
        return Err(Error::BadBandCount(b));
    }
    if opts.chunk_rows == 0 {
        return invalid("chunk rows must be positive".into());
    }

    match epsg {
        Some(code) if Crs::from_epsg(*code) != Some(grid.crs()) => Err(Error::CrsMismatch {
            raster: *code,
            grid: grid.crs(),
        }),
        Some(_) => Ok(()),
        None => {
            warn!(
                grid = %grid.crs(),
                "raster declares no EPSG code; assuming it is already in the grid's CRS"
            );
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests;

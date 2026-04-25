//! Pipeline orchestration — calls into `libgeotiles` instruments to produce tiles.

#[cfg(any(feature = "geographic", feature = "mercator"))]
use std::path::Path;

use gdal::Dataset;
#[cfg(any(feature = "geographic", feature = "mercator"))]
use libgeotiles::Format;
#[cfg(any(feature = "geographic", feature = "mercator"))]
use libgeotiles::backend::cpu::crop_tile;
use libgeotiles::coords::Bounds;
#[cfg(any(feature = "geographic", feature = "mercator"))]
use libgeotiles::coords::{Tile, flip_y};
#[cfg(any(feature = "geographic", feature = "mercator"))]
use libgeotiles::encode::encode_tile;
#[cfg(any(feature = "geographic", feature = "mercator"))]
use libgeotiles::gdal_io::{append_mask_alpha, read_chunk};
use libgeotiles::gdal_io::{open_dataset, warp_to_epsg};
#[cfg(any(feature = "geographic", feature = "mercator"))]
use libgeotiles::pipeline::TileGrid;
#[cfg(any(feature = "geographic", feature = "mercator"))]
use libgeotiles::pipeline::chunks::group_tiles_by_chunk;
#[cfg(any(feature = "geographic", feature = "mercator"))]
use rayon::prelude::*;
#[cfg(any(feature = "geographic", feature = "mercator"))]
use tracing::debug;
use tracing::{info, info_span};

use crate::settings::{Crs, Settings};

// -- Main entry point ---------------------------------------------------------

/// Execute the full tiling pipeline for the given `settings`.
pub fn run(settings: &Settings) -> anyhow::Result<()> {
    let _span = info_span!("run", input = %settings.input.display()).entered();

    let (src_ds, _src_info) = open_dataset(&settings.input)?;

    let target_epsg = match settings.crs {
        Crs::Geographic => 4326u32,
        Crs::Mercator => 3857u32,
    };

    // Warp to target CRS if needed; the warped VRT stays alive until end of scope.
    let warped_opt = warp_to_epsg(&src_ds, target_epsg)?;
    let work_ds = warped_opt.as_ref().unwrap_or(&src_ds);

    let (ds_w, ds_h) = work_ds.raster_size();
    let gt = work_ds.geo_transform()?;

    let ds_bounds = dataset_bounds(&gt, ds_w, ds_h);

    info!(
        target_epsg,
        ds_w,
        ds_h,
        min_x = ds_bounds.min_x,
        min_y = ds_bounds.min_y,
        max_x = ds_bounds.max_x,
        max_y = ds_bounds.max_y,
        "working dataset ready"
    );

    std::fs::create_dir_all(&settings.output)?;

    dispatch_crs(settings, work_ds, &gt, ds_bounds, ds_w, ds_h)?;

    if settings.tmr {
        crate::tmr::write(&settings.output, settings, ds_bounds)?;
    }

    Ok(())
}

// -- CRS dispatch -------------------------------------------------------------

fn dispatch_crs(
    settings: &Settings,
    work_ds: &Dataset,
    gt: &[f64; 6],
    ds_bounds: Bounds,
    ds_w: usize,
    ds_h: usize,
) -> anyhow::Result<()> {
    match settings.crs {
        Crs::Geographic => dispatch_geographic(settings, work_ds, gt, ds_bounds, ds_w, ds_h),
        Crs::Mercator => dispatch_mercator(settings, work_ds, gt, ds_bounds, ds_w, ds_h),
    }
}

fn dispatch_geographic(
    settings: &Settings,
    work_ds: &Dataset,
    gt: &[f64; 6],
    ds_bounds: Bounds,
    ds_w: usize,
    ds_h: usize,
) -> anyhow::Result<()> {
    #[cfg(feature = "geographic")]
    {
        use libgeotiles::Geographic;
        let grid = Geographic::new(settings.tile_size);
        run_zooms(settings, work_ds, &grid, gt, ds_bounds, ds_w, ds_h)
    }
    #[cfg(not(feature = "geographic"))]
    {
        let _ = (settings, work_ds, gt, ds_bounds, ds_w, ds_h);
        anyhow::bail!("geographic CRS requested but the geographic feature is not compiled in")
    }
}

fn dispatch_mercator(
    settings: &Settings,
    work_ds: &Dataset,
    gt: &[f64; 6],
    ds_bounds: Bounds,
    ds_w: usize,
    ds_h: usize,
) -> anyhow::Result<()> {
    #[cfg(feature = "mercator")]
    {
        use libgeotiles::WebMercator;
        let grid = WebMercator::new(settings.tile_size);
        run_zooms(settings, work_ds, &grid, gt, ds_bounds, ds_w, ds_h)
    }
    #[cfg(not(feature = "mercator"))]
    {
        let _ = (settings, work_ds, gt, ds_bounds, ds_w, ds_h);
        anyhow::bail!("mercator CRS requested but the mercator feature is not compiled in")
    }
}

// -- Zoom loop + inner tile loop ----------------------------------------------

#[cfg(any(feature = "geographic", feature = "mercator"))]
fn run_zooms(
    settings: &Settings,
    work_ds: &Dataset,
    grid: &dyn TileGrid,
    gt: &[f64; 6],
    ds_bounds: Bounds,
    ds_w: usize,
    ds_h: usize,
) -> anyhow::Result<()> {
    for z in settings.min_zoom..=settings.max_zoom {
        let _span = info_span!("zoom", z).entered();
        info!(z, "processing zoom level");

        let chunk_map =
            group_tiles_by_chunk(grid, ds_bounds, gt, ds_w, ds_h, z, settings.chunk_size);

        let total_tiles: usize = chunk_map.values().map(|v| v.len()).sum();
        info!(
            z,
            chunks = chunk_map.len(),
            total_tiles,
            "tile groups ready"
        );

        for (&chunk_id, jobs) in &chunk_map {
            let _cspan = info_span!("chunk", chunk_id, tiles = jobs.len()).entered();

            let row_start = chunk_id * settings.chunk_size;
            // Expand the read window to cover the deepest source row required by any
            // tile assigned to this chunk.  Without this, tiles whose source window
            // straddles a chunk boundary (common at low zoom levels where a single tile
            // can span the full raster height) would index past the end of the chunk
            // buffer in `crop_tile` and panic.
            let natural_row_end = (row_start + settings.chunk_size).min(ds_h);
            let required_row_end = jobs
                .iter()
                .map(|job| job.window.row + job.window.height)
                .max()
                .unwrap_or(natural_row_end)
                .min(ds_h);
            let row_count = required_row_end.saturating_sub(row_start).max(1);

            if required_row_end > natural_row_end {
                debug!(
                    row_start,
                    natural_row_end,
                    required_row_end,
                    row_count,
                    "chunk expanded to cover full tile source windows"
                );
            }
            debug!(row_start, row_count, "reading chunk");
            let mut chunk = read_chunk(work_ds, row_start, row_count)?;
            append_mask_alpha(work_ds, &mut chunk, row_start, row_count)?;

            let tile_size = settings.tile_size;
            let format = settings.format;
            let tms = settings.tms;
            let output_dir = settings.output.as_path();
            let encode_opts = &settings.encode_opts;
            let bands_override = settings.bands;
            let src_bands = chunk.band_count();

            let results: Vec<libgeotiles::Result<()>> = jobs
                .par_iter()
                .map(|job| {
                    let pixels = crop_tile(&chunk, job.window, tile_size)?;
                    let out_bands = bands_override.unwrap_or(src_bands);
                    let out_pixels = apply_bands(pixels, src_bands, out_bands);
                    let encoded = encode_tile(
                        &out_pixels,
                        tile_size,
                        tile_size,
                        out_bands,
                        format,
                        encode_opts,
                    )?;
                    write_tile(output_dir, job.tile, z, tms, format, &encoded)?;
                    Ok(())
                })
                .collect();

            for r in results {
                r?;
            }
        }
    }

    Ok(())
}

// -- Tile output --------------------------------------------------------------

#[cfg(any(feature = "geographic", feature = "mercator"))]
fn write_tile(
    output: &Path,
    tile: Tile,
    z: u8,
    tms: bool,
    format: Format,
    data: &[u8],
) -> libgeotiles::Result<()> {
    let y_final = if tms { flip_y(tile.y, z) } else { tile.y };
    let dir = output.join(z.to_string()).join(tile.x.to_string());
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.{}", y_final, format.extension()));
    std::fs::write(path, data)?;
    debug!(z, x = tile.x, y = tile.y, y_final, "tile written");
    Ok(())
}

// -- Band selection -----------------------------------------------------------

/// Repack interleaved pixels from `from_bands` to `to_bands` channels.
///
/// Extra channels are padded with 255; excess source channels beyond `to_bands`
/// are dropped.
#[cfg(any(feature = "geographic", feature = "mercator"))]
fn apply_bands(pixels: Vec<u8>, from_bands: usize, to_bands: usize) -> Vec<u8> {
    if from_bands == to_bands || from_bands == 0 || to_bands == 0 {
        return pixels;
    }
    let npx = pixels.len() / from_bands;
    let mut out = Vec::with_capacity(npx * to_bands);
    for px in pixels.chunks_exact(from_bands) {
        let copy = to_bands.min(from_bands);
        out.extend_from_slice(&px[..copy]);
        out.extend(std::iter::repeat_n(255u8, to_bands.saturating_sub(copy)));
    }
    out
}

// -- Helpers ------------------------------------------------------------------

/// Compute dataset bounds in the working CRS from a north-up geo-transform.
pub(crate) fn dataset_bounds(gt: &[f64; 6], ds_w: usize, ds_h: usize) -> Bounds {
    Bounds {
        min_x: gt[0],
        max_x: gt[0] + ds_w as f64 * gt[1],
        max_y: gt[3],
        min_y: gt[3] + ds_h as f64 * gt[5],
    }
}

#[cfg(test)]
mod tests;

//! Pipeline orchestration — calls into `libgeotiles` instruments to produce tiles.

#[cfg(any(feature = "geographic", feature = "mercator"))]
use std::path::Path;

#[cfg(any(feature = "geographic", feature = "mercator"))]
use libgeotiles::Format;
#[cfg(any(feature = "geographic", feature = "mercator"))]
use libgeotiles::ResampleBackend;
#[cfg(any(feature = "geographic", feature = "mercator"))]
use libgeotiles::backend::cpu::crop_tile;
#[cfg(all(feature = "gpu", any(feature = "geographic", feature = "mercator")))]
use libgeotiles::backend::gpu::GpuContext;
use libgeotiles::coords::Bounds;
#[cfg(any(feature = "geographic", feature = "mercator"))]
use libgeotiles::coords::{Tile, flip_y};
#[cfg(any(feature = "geographic", feature = "mercator"))]
use libgeotiles::encode::encode_tile;
#[cfg(any(feature = "geographic", feature = "mercator"))]
use libgeotiles::pipeline::TileGrid;
#[cfg(any(feature = "geographic", feature = "mercator"))]
use libgeotiles::pipeline::chunks::group_tiles_by_chunk;
#[cfg(any(feature = "geographic", feature = "mercator"))]
use libgeotiles::tiff_io::read_chunk;
use libgeotiles::tiff_io::{RasterDataset, open_dataset};
#[cfg(any(feature = "geographic", feature = "mercator"))]
use rayon::prelude::*;
#[cfg(any(feature = "geographic", feature = "mercator"))]
use tracing::debug;
use tracing::{info, info_span};

use crate::settings::{Crs, Settings};

// -- Main entry point ---------------------------------------------------------

/// Execute the full tiling pipeline for the given `settings`.
///
/// No reprojection or nodata/mask handling is performed: the input is read exactly as it is
/// on disk, so it must already be an 8-bit, chunky-interleaved GeoTIFF in the CRS selected by
/// `settings.crs` (`--crs`). Pre-process the input with any GeoTIFF-capable tool first if
/// either is needed.
pub fn run(settings: &Settings) -> anyhow::Result<()> {
    let _span = info_span!("run", input = %settings.input.display()).entered();

    let (mut work_ds, info) = open_dataset(&settings.input)?;

    let ds_w = info.width;
    let ds_h = info.height;
    let gt = info.geo_transform;

    let ds_bounds = dataset_bounds(&gt, ds_w, ds_h);

    info!(
        ds_w,
        ds_h,
        band_count = info.band_count,
        min_x = ds_bounds.min_x,
        min_y = ds_bounds.min_y,
        max_x = ds_bounds.max_x,
        max_y = ds_bounds.max_y,
        "dataset ready"
    );

    std::fs::create_dir_all(&settings.output)?;

    dispatch_crs(settings, &mut work_ds, &gt, ds_bounds, ds_w, ds_h)?;

    if settings.tmr {
        crate::tmr::write(&settings.output, settings, ds_bounds)?;
    }

    Ok(())
}

// -- CRS dispatch -------------------------------------------------------------

fn dispatch_crs(
    settings: &Settings,
    work_ds: &mut RasterDataset,
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
    work_ds: &mut RasterDataset,
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
    work_ds: &mut RasterDataset,
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

// -- Resample backend selection ------------------------------------------------

/// Which resample implementation `run_zooms` dispatches to, resolved once per run.
///
/// The GPU variant owns a [`GpuContext`] (device/queue/pipeline), which is expensive to
/// initialise, so it is created exactly once and shared (read-only) across every tile.
#[cfg(any(feature = "geographic", feature = "mercator"))]
enum Backend {
    Cpu,
    #[cfg(feature = "gpu")]
    Gpu(GpuContext),
}

#[cfg(any(feature = "geographic", feature = "mercator"))]
fn init_backend(backend: ResampleBackend) -> anyhow::Result<Backend> {
    match backend {
        ResampleBackend::Cpu => Ok(Backend::Cpu),
        #[cfg(feature = "gpu")]
        ResampleBackend::Gpu => {
            info!("initialising GPU backend");
            Ok(Backend::Gpu(GpuContext::new()?))
        }
    }
}

// -- Zoom loop + inner tile loop ----------------------------------------------

#[cfg(any(feature = "geographic", feature = "mercator"))]
fn run_zooms(
    settings: &Settings,
    work_ds: &mut RasterDataset,
    grid: &dyn TileGrid,
    gt: &[f64; 6],
    ds_bounds: Bounds,
    ds_w: usize,
    ds_h: usize,
) -> anyhow::Result<()> {
    let backend = init_backend(settings.backend)?;

    for z in settings.min_zoom..=settings.max_zoom {
        let _span = info_span!("zoom", z).entered();
        info!(z, "processing zoom level");

        let chunk_map = group_tiles_by_chunk(
            grid,
            ds_bounds,
            gt,
            ds_w,
            ds_h,
            z,
            settings.chunk_size,
            settings.tile_size,
        );

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
            let chunk = read_chunk(work_ds, row_start, row_count)?;

            let tile_size = settings.tile_size;
            let format = settings.format;
            let tms = settings.tms;
            let output_dir = settings.output.as_path();
            let encode_opts = &settings.encode_opts;
            let bands_override = settings.bands;
            let src_bands = chunk.band_count();

            let out_bands = bands_override.unwrap_or(src_bands);

            let results: Vec<libgeotiles::Result<()>> = jobs
                .par_iter()
                .map(|job| {
                    // The GPU path always produces 4-band RGBA (see `GpuContext::crop_tile`
                    // doc comment) regardless of the source dataset's band count, so
                    // `apply_bands` must be told the *actual* pixel layout it received, not
                    // `src_bands`.
                    let (pixels, actual_bands) = match &backend {
                        Backend::Cpu => (
                            crop_tile(&chunk, job.window, tile_size, job.dst)?,
                            src_bands,
                        ),
                        #[cfg(feature = "gpu")]
                        Backend::Gpu(ctx) => (
                            ctx.crop_tile(&chunk, job.window, tile_size, job.dst)?,
                            4usize,
                        ),
                    };
                    let out_pixels =
                        apply_bands(pixels, actual_bands, out_bands, tile_size, job.dst);
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
/// Extra channels are padded `255` (opaque) for pixels inside `dst` — real resampled
/// data — and `0` for pixels outside it, so a synthesized alpha channel stays
/// transparent over the padding `crop_tile` leaves around partial-overlap tiles instead
/// of turning it into opaque black. Excess source channels beyond `to_bands` are dropped.
#[cfg(any(feature = "geographic", feature = "mercator"))]
fn apply_bands(
    pixels: Vec<u8>,
    from_bands: usize,
    to_bands: usize,
    tile_size: u32,
    dst: libgeotiles::tile::DstRect,
) -> Vec<u8> {
    if from_bands == to_bands || from_bands == 0 || to_bands == 0 {
        return pixels;
    }
    let tile_size = tile_size as usize;
    let (dx0, dx1) = (dst.x as usize, (dst.x + dst.width) as usize);
    let (dy0, dy1) = (dst.y as usize, (dst.y + dst.height) as usize);
    let copy = to_bands.min(from_bands);
    let pad = to_bands.saturating_sub(copy);

    let mut out = Vec::with_capacity((pixels.len() / from_bands) * to_bands);
    for (i, px) in pixels.chunks_exact(from_bands).enumerate() {
        let (col, row) = (i % tile_size, i / tile_size);
        let inside_dst = (dx0..dx1).contains(&col) && (dy0..dy1).contains(&row);
        out.extend_from_slice(&px[..copy]);
        out.extend(std::iter::repeat_n(
            if inside_dst { 255u8 } else { 0u8 },
            pad,
        ));
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

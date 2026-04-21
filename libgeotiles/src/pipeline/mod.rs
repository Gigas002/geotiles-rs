use std::path::Path;
use std::time::Instant;

use rayon::prelude::*;
use tracing::{debug, info, info_span};

use crate::Result;
use crate::coords::{Bounds, Tile};
use crate::encode::encode_tile;
use crate::encode::options::EncodeOptions;
use crate::gdal_io::read_chunk;
use crate::tile::{Format, crop_tile};

pub(crate) mod chunks;

/// Abstraction over a tile coordinate grid (geographic or Web Mercator).
///
/// Implemented by the grid structs in `crate::coords`; used as a trait object
/// by [`run_pipeline`] so that the chunk loop is grid-agnostic.
pub(crate) trait TileGrid {
    fn tile_range(&self, area: Bounds, z: u8) -> (Tile, Tile);
    fn tile_bounds(&self, tile: Tile) -> Bounds;
}

/// Run the full tiling pipeline for all zoom levels.
///
/// Memory model:
/// - **Outer loop (sequential):** iterates over source-pixel chunk bands of `chunk_size` rows.
///   The chunk buffer is read once, all overlapping tiles are processed, then the buffer is
///   dropped before the next chunk is read. Peak RAM is bounded by one chunk at a time.
/// - **Inner loop (parallel via rayon):** tiles within each chunk are processed concurrently.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_pipeline(
    ds: &gdal::Dataset,
    grid: &dyn TileGrid,
    ds_bounds: Bounds,
    gt: &[f64; 6],
    ds_width: usize,
    ds_height: usize,
    band_count: usize,
    zoom: std::ops::RangeInclusive<u8>,
    chunk_size: usize,
    tile_size: u32,
    format: Format,
    encode_options: &EncodeOptions,
    output: &Path,
) -> Result<()> {
    for z in zoom {
        let _span = info_span!("zoom", z).entered();

        let chunk_groups =
            chunks::group_tiles_by_chunk(grid, ds_bounds, gt, ds_width, ds_height, z, chunk_size);

        let total_tiles: usize = chunk_groups.values().map(|v| v.len()).sum();
        info!(
            z,
            band_count,
            total_tiles,
            num_chunks = chunk_groups.len(),
            "enumerating tiles for zoom level"
        );

        for (chunk_id, jobs) in &chunk_groups {
            let chunk_start = chunk_id * chunk_size;
            // Extend read to cover all rows required by every tile in this chunk.
            let chunk_end = jobs
                .iter()
                .map(|j| j.window.row + j.window.height)
                .max()
                .unwrap_or(chunk_start + 1)
                .min(ds_height);
            let row_count = chunk_end.saturating_sub(chunk_start).max(1);

            let t0 = Instant::now();
            debug!(
                chunk_id,
                chunk_start,
                row_count,
                tile_count = jobs.len(),
                "reading chunk"
            );

            let mut chunk = read_chunk(ds, chunk_start, row_count)?;

            // Append a synthetic alpha band from the GDAL mask if the dataset has nodata or
            // an explicit alpha channel.  After this call, chunk.band_count() is the ground
            // truth for the effective number of bands (may be dataset band_count + 1).
            crate::gdal_io::append_mask_alpha(ds, &mut chunk, chunk_start, row_count)?;
            let effective_bands = chunk.band_count();

            jobs.par_iter().try_for_each(|job| -> Result<()> {
                let pixels = crop_tile(&chunk, job.window, tile_size)?;
                let encoded = encode_tile(
                    &pixels,
                    tile_size,
                    tile_size,
                    effective_bands,
                    format,
                    encode_options,
                )?;

                let tile_path = output
                    .join(z.to_string())
                    .join(job.tile.x.to_string())
                    .join(format!("{}.{}", job.tile.y, format.extension()));
                std::fs::create_dir_all(tile_path.parent().unwrap())?;
                std::fs::write(&tile_path, &encoded)?;

                debug!(
                    path = %tile_path.display(),
                    bytes = encoded.len(),
                    "tile written"
                );
                Ok(())
            })?;

            info!(
                chunk_id,
                chunk_start,
                row_count,
                tile_count = jobs.len(),
                elapsed_ms = t0.elapsed().as_millis(),
                "chunk complete"
            );
        }
    }

    Ok(())
}

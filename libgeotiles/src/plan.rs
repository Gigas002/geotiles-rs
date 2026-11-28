//! Tile planning: which tiles a zoom level needs, where their pixels come from in the
//! raster (with sub-pixel precision), and how to group them into row batches so only a
//! horizontal slice of the raster is held in memory at a time.

use std::collections::BTreeMap;

use crate::coords::{Tile, TileGrid};
use crate::raster::RasterInfo;

/// Source rectangle in fractional raster pixels (absolute, top-left origin).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SrcRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// The part of the `tile_size × tile_size` canvas that `src` is resampled into. Smaller
/// than the canvas only for tiles on the raster's edge; the rest stays transparent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DstRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl DstRect {
    pub fn full(tile_size: u32) -> Self {
        Self {
            x: 0,
            y: 0,
            width: tile_size,
            height: tile_size,
        }
    }

    pub fn contains(&self, col: u32, row: u32) -> bool {
        (self.x..self.x + self.width).contains(&col)
            && (self.y..self.y + self.height).contains(&row)
    }
}

/// One tile to render.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TileJob {
    pub tile: Tile,
    pub src: SrcRect,
    pub dst: DstRect,
}

impl TileJob {
    /// Raster rows `[lo, hi)` a resampler whose kernel reaches `support` source pixels
    /// (at 1:1 scale) needs to render this tile.
    pub fn rows_needed(&self, support: f64, height: usize) -> (usize, usize) {
        needed(
            self.src.y,
            self.src.height,
            self.dst.height,
            support,
            height,
        )
    }

    /// Raster columns `[lo, hi)`, see [`rows_needed`](Self::rows_needed).
    pub fn cols_needed(&self, support: f64, width: usize) -> (usize, usize) {
        needed(self.src.x, self.src.width, self.dst.width, support, width)
    }
}

/// Kernel footprint around `[start, start + len)` when mapping it onto `out` pixels: the
/// kernel widens with the downscale factor.
fn needed(start: f64, len: f64, out: u32, support: f64, limit: usize) -> (usize, usize) {
    let radius = support * (len / out as f64).max(1.0) + 1.0;
    let lo = ((start - radius).floor().max(0.0) as usize).min(limit.saturating_sub(1));
    let hi = ((start + len + radius).ceil().max(0.0) as usize).clamp(lo + 1, limit.max(1));
    (lo, hi)
}

/// Every tile of zoom `z` that overlaps the raster, with its exact source rectangle.
///
/// The destination rectangle is snapped to whole tile pixels first and the source
/// rectangle is derived from the snapped edges, so the source→canvas mapping stays exact
/// (no stretching) even for edge tiles.
pub fn plan_zoom(grid: &dyn TileGrid, info: &RasterInfo, z: u8) -> Vec<TileJob> {
    let ds = info.bounds();
    let size = grid.tile_size();
    let size_f = size as f64;
    let (min, max) = grid.tile_range(ds, z);
    let (w, h) = (info.width as f64, info.height as f64);
    let mut jobs = Vec::new();

    for y in min.y..=max.y {
        for x in min.x..=max.x {
            let tile = Tile::new(x, y, z);
            let tb = grid.tile_bounds(tile);
            let Some(clip) = tb.intersection(&ds) else {
                continue;
            };

            let (x0, x1) = snap(
                (clip.min_x - tb.min_x) / tb.width() * size_f,
                (clip.max_x - tb.min_x) / tb.width() * size_f,
                size,
            );
            let (y0, y1) = snap(
                (tb.max_y - clip.max_y) / tb.height() * size_f,
                (tb.max_y - clip.min_y) / tb.height() * size_f,
                size,
            );

            let gx = |px: u32| tb.min_x + px as f64 / size_f * tb.width();
            let gy = |py: u32| tb.max_y - py as f64 / size_f * tb.height();
            let (c0, r0) = info.transform.to_pixel(gx(x0), gy(y0));
            let (c1, r1) = info.transform.to_pixel(gx(x1), gy(y1));
            let (c0, c1) = (c0.clamp(0.0, w), c1.clamp(0.0, w));
            let (r0, r1) = (r0.clamp(0.0, h), r1.clamp(0.0, h));
            if c1 <= c0 || r1 <= r0 {
                continue;
            }

            jobs.push(TileJob {
                tile,
                src: SrcRect {
                    x: c0,
                    y: r0,
                    width: c1 - c0,
                    height: r1 - r0,
                },
                dst: DstRect {
                    x: x0,
                    y: y0,
                    width: x1 - x0,
                    height: y1 - y0,
                },
            });
        }
    }
    jobs
}

/// Round a fractional canvas span to whole pixels, keeping at least one pixel.
fn snap(lo: f64, hi: f64, size: u32) -> (u32, u32) {
    let size_f = size as f64;
    let lo = lo.round().clamp(0.0, size_f) as u32;
    let hi = hi.round().clamp(0.0, size_f) as u32;
    match hi.cmp(&lo) {
        std::cmp::Ordering::Greater => (lo, hi),
        _ if lo < size => (lo, lo + 1),
        _ => (size - 1, size),
    }
}

/// Tiles sharing one read of raster rows `[row_start, row_end)`.
#[derive(Debug)]
pub struct Batch {
    pub row_start: usize,
    pub row_end: usize,
    pub jobs: Vec<TileJob>,
}

/// Group jobs by the `chunk_rows`-high raster slice their source starts in, and widen each
/// batch's read to cover every job's full kernel footprint.
///
/// At low zooms one tile can span far more than `chunk_rows` rows; its batch then reads
/// whatever that tile needs.
pub fn batch_jobs(
    jobs: Vec<TileJob>,
    chunk_rows: usize,
    support: f64,
    raster_height: usize,
) -> Vec<Batch> {
    let chunk_rows = chunk_rows.max(1);
    let mut groups: BTreeMap<usize, Vec<TileJob>> = BTreeMap::new();
    for job in jobs {
        groups
            .entry(job.src.y as usize / chunk_rows)
            .or_default()
            .push(job);
    }
    groups
        .into_values()
        .map(|jobs| {
            let (row_start, row_end) = jobs
                .iter()
                .map(|j| j.rows_needed(support, raster_height))
                .fold((usize::MAX, 0), |(lo, hi), (a, b)| (lo.min(a), hi.max(b)));
            Batch {
                row_start,
                row_end,
                jobs,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;

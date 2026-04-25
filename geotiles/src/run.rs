//! Pipeline orchestration — calls into `libgeotiles` instruments to produce tiles.

#[cfg(any(feature = "geographic", feature = "mercator"))]
use std::path::Path;
use std::path::PathBuf;

use gdal::Dataset;
use libgeotiles::EncodeOptions;
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

use crate::config::Config;

/// Target coordinate reference system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Crs {
    Geographic,
    Mercator,
}

/// All resolved run parameters (config merged with CLI overrides).
#[derive(Debug)]
pub struct Params {
    pub input: PathBuf,
    pub output: PathBuf,
    pub min_zoom: u8,
    pub max_zoom: u8,
    pub format: Format,
    pub tms: bool,
    pub crs: Crs,
    // Read only by the CRS-gated pipeline functions; suppress dead-code lint when
    // neither geographic nor mercator feature is compiled in.
    #[cfg_attr(
        not(any(feature = "geographic", feature = "mercator")),
        allow(dead_code)
    )]
    pub bands: Option<usize>,
    pub tile_size: u32,
    pub tmr: bool,
    pub chunk_size: usize,
    #[cfg_attr(
        not(any(feature = "geographic", feature = "mercator")),
        allow(dead_code)
    )]
    pub encode_opts: EncodeOptions,
}

impl Params {
    /// Build resolved `Params` by merging `config` defaults with explicit CLI overrides.
    ///
    /// `cli_*` arguments are `None` when the flag was absent; the config value (or a
    /// hard-coded default) is used in that case.
    #[allow(clippy::too_many_arguments)]
    pub fn resolve(
        input: PathBuf,
        output: PathBuf,
        min_zoom: u8,
        max_zoom: u8,
        cli_extension: Option<String>,
        cli_tms: Option<bool>,
        cli_crs: Option<String>,
        cli_bands: Option<usize>,
        cli_tilesize: Option<u32>,
        cli_tmr: Option<bool>,
        cli_chunk_size: Option<usize>,
        config: &Config,
    ) -> anyhow::Result<Self> {
        let format = parse_format(
            cli_extension
                .as_deref()
                .or(config.extension.as_deref())
                .unwrap_or("png"),
        )?;

        let tms = cli_tms.or(config.tms).unwrap_or(false);

        let crs = parse_crs(
            cli_crs
                .as_deref()
                .or(config.crs.as_deref())
                .unwrap_or("geographic"),
        )?;

        let bands = cli_bands.or(config.bands);
        let tile_size = cli_tilesize.or(config.tilesize).unwrap_or(256);
        let tmr = cli_tmr.or(config.tmr).unwrap_or(false);
        let chunk_size = cli_chunk_size.or(config.chunk_size).unwrap_or(512);
        let encode_opts = build_encode_opts(config);

        Ok(Self {
            input,
            output,
            min_zoom,
            max_zoom,
            format,
            tms,
            crs,
            bands,
            tile_size,
            tmr,
            chunk_size,
            encode_opts,
        })
    }
}

// ── Main entry point ──────────────────────────────────────────────────────────

/// Execute the full tiling pipeline for the given `params`.
pub fn run(params: &Params) -> anyhow::Result<()> {
    let _span = info_span!("run", input = %params.input.display()).entered();

    let (src_ds, _src_info) = open_dataset(&params.input)?;

    let target_epsg = match params.crs {
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

    std::fs::create_dir_all(&params.output)?;

    dispatch_crs(params, work_ds, &gt, ds_bounds, ds_w, ds_h)?;

    if params.tmr {
        crate::tmr::write(&params.output, params, ds_bounds)?;
    }

    Ok(())
}

// ── CRS dispatch ──────────────────────────────────────────────────────────────

fn dispatch_crs(
    params: &Params,
    work_ds: &Dataset,
    gt: &[f64; 6],
    ds_bounds: Bounds,
    ds_w: usize,
    ds_h: usize,
) -> anyhow::Result<()> {
    match params.crs {
        Crs::Geographic => dispatch_geographic(params, work_ds, gt, ds_bounds, ds_w, ds_h),
        Crs::Mercator => dispatch_mercator(params, work_ds, gt, ds_bounds, ds_w, ds_h),
    }
}

fn dispatch_geographic(
    params: &Params,
    work_ds: &Dataset,
    gt: &[f64; 6],
    ds_bounds: Bounds,
    ds_w: usize,
    ds_h: usize,
) -> anyhow::Result<()> {
    #[cfg(feature = "geographic")]
    {
        use libgeotiles::Geographic;
        let grid = Geographic::new(params.tile_size);
        run_zooms(params, work_ds, &grid, gt, ds_bounds, ds_w, ds_h)
    }
    #[cfg(not(feature = "geographic"))]
    {
        let _ = (params, work_ds, gt, ds_bounds, ds_w, ds_h);
        anyhow::bail!("geographic CRS requested but the 'geographic' feature is not compiled in")
    }
}

fn dispatch_mercator(
    params: &Params,
    work_ds: &Dataset,
    gt: &[f64; 6],
    ds_bounds: Bounds,
    ds_w: usize,
    ds_h: usize,
) -> anyhow::Result<()> {
    #[cfg(feature = "mercator")]
    {
        use libgeotiles::WebMercator;
        let grid = WebMercator::new(params.tile_size);
        run_zooms(params, work_ds, &grid, gt, ds_bounds, ds_w, ds_h)
    }
    #[cfg(not(feature = "mercator"))]
    {
        let _ = (params, work_ds, gt, ds_bounds, ds_w, ds_h);
        anyhow::bail!("mercator CRS requested but the 'mercator' feature is not compiled in")
    }
}

// ── Zoom loop + inner tile loop ───────────────────────────────────────────────

// These functions are only reachable when at least one CRS feature is compiled in.
#[cfg(any(feature = "geographic", feature = "mercator"))]
fn run_zooms(
    params: &Params,
    work_ds: &Dataset,
    grid: &dyn TileGrid,
    gt: &[f64; 6],
    ds_bounds: Bounds,
    ds_w: usize,
    ds_h: usize,
) -> anyhow::Result<()> {
    for z in params.min_zoom..=params.max_zoom {
        let _span = info_span!("zoom", z).entered();
        info!(z, "processing zoom level");

        let chunk_map = group_tiles_by_chunk(grid, ds_bounds, gt, ds_w, ds_h, z, params.chunk_size);

        let total_tiles: usize = chunk_map.values().map(|v| v.len()).sum();
        info!(
            z,
            chunks = chunk_map.len(),
            total_tiles,
            "tile groups ready"
        );

        for (&chunk_id, jobs) in &chunk_map {
            let _cspan = info_span!("chunk", chunk_id, tiles = jobs.len()).entered();

            let row_start = chunk_id * params.chunk_size;
            let row_count = (params.chunk_size).min(ds_h.saturating_sub(row_start));

            debug!(row_start, row_count, "reading chunk");
            let mut chunk = read_chunk(work_ds, row_start, row_count)?;
            append_mask_alpha(work_ds, &mut chunk, row_start, row_count)?;

            let tile_size = params.tile_size;
            let format = params.format;
            let tms = params.tms;
            let output_dir = params.output.as_path();
            let encode_opts = &params.encode_opts;
            let bands_override = params.bands;
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

// ── Tile output ───────────────────────────────────────────────────────────────

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

// ── Band selection ────────────────────────────────────────────────────────────

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

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Compute dataset bounds in the working CRS from a north-up geo-transform.
pub fn dataset_bounds(gt: &[f64; 6], ds_w: usize, ds_h: usize) -> Bounds {
    Bounds {
        min_x: gt[0],
        max_x: gt[0] + ds_w as f64 * gt[1],
        max_y: gt[3],
        min_y: gt[3] + ds_h as f64 * gt[5],
    }
}

fn parse_format(s: &str) -> anyhow::Result<Format> {
    match s.trim().to_lowercase().trim_start_matches('.') {
        "png" => Ok(Format::Png),
        "jpg" | "jpeg" => Ok(Format::Jpeg),
        "webp" => Ok(Format::WebP),
        "avif" => Ok(Format::Avif),
        "jxl" => Ok(Format::Jxl),
        other => anyhow::bail!(
            "unknown format '{}'; supported: png, jpg, webp, avif, jxl",
            other
        ),
    }
}

pub fn parse_crs(s: &str) -> anyhow::Result<Crs> {
    match s.trim().to_lowercase().as_str() {
        "geographic" | "geodetic" | "4326" | "epsg:4326" => Ok(Crs::Geographic),
        "mercator" | "webmercator" | "3857" | "epsg:3857" => Ok(Crs::Mercator),
        other => anyhow::bail!(
            "unknown CRS '{}'; supported: geographic (EPSG:4326), mercator (EPSG:3857)",
            other
        ),
    }
}

fn build_encode_opts(cfg: &Config) -> EncodeOptions {
    use libgeotiles::{
        AvifOptions, JpegOptions, JxlOptions, PngCompression, PngFilter, PngOptions, WebPOptions,
    };

    let png = PngOptions {
        compression: cfg
            .png
            .compression
            .as_deref()
            .map(|s| match s {
                "fast" => PngCompression::Fast,
                "best" => PngCompression::Best,
                _ => PngCompression::Default,
            })
            .unwrap_or_default(),
        filter: cfg
            .png
            .filter
            .as_deref()
            .map(|s| match s {
                "none" | "nofilter" => PngFilter::NoFilter,
                "sub" => PngFilter::Sub,
                "up" => PngFilter::Up,
                "avg" | "average" => PngFilter::Avg,
                "paeth" => PngFilter::Paeth,
                _ => PngFilter::Adaptive,
            })
            .unwrap_or_default(),
    };

    let jpeg = JpegOptions {
        quality: cfg.jpeg.quality.unwrap_or(85),
    };

    let webp = WebPOptions {
        lossless: cfg.webp.lossless.unwrap_or(true),
        quality: cfg.webp.quality.unwrap_or(85),
    };

    let avif = AvifOptions {
        quality: cfg.avif.quality.unwrap_or(60),
        speed: cfg.avif.speed.unwrap_or(4),
    };

    let jxl = JxlOptions {
        distance: cfg.jxl.distance.unwrap_or(1.0),
        effort: cfg.jxl.effort.unwrap_or(7),
        lossless: cfg.jxl.lossless.unwrap_or(false),
    };

    EncodeOptions {
        png,
        jpeg,
        webp,
        avif,
        jxl,
    }
}

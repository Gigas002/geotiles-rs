//! Resolved application settings — single source of truth after CLI + config are merged.
//!
//! # Resolution order
//!
//! 1. Built-in hard-coded defaults (defined here)
//! 2. Config file values ([`crate::config::Config`])
//! 3. CLI flags ([`crate::cli::Cli`]) — always win
//!
//! Once [`Settings::resolve`] returns, all references to [`crate::cli::Cli`] and
//! [`crate::config::Config`] must be dropped.  Only [`Settings`] is passed downstream.

use std::path::PathBuf;

use libgeotiles::{
    AvifOptions, Crs, EncodeOptions, Format, JpegOptions, JxlOptions, PngCompression, PngFilter,
    PngOptions, WebPOptions,
};

use crate::cli::{Cli, ZoomRange};
use crate::config::Config;

// ── Public types ──────────────────────────────────────────────────────────────

/// Tile resample backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Backend {
    #[default]
    Cpu,
    #[cfg(feature = "gpu")]
    Gpu,
}

/// All resolved run parameters after merging CLI flags and config file values.
///
/// Constructed **once** in `main` via [`Settings::resolve`].  All downstream code
/// receives only `&Settings` — never `&Cli` or `&Config`.
#[derive(Debug)]
pub struct Settings {
    /// Path to the source GeoTIFF (8-bit, north-up, already in the tile CRS).
    pub input: PathBuf,
    /// Path to the output tile directory.  Created if it does not exist.
    pub output: PathBuf,
    /// Minimum zoom level (inclusive).
    pub min_zoom: u8,
    /// Maximum zoom level (inclusive).
    pub max_zoom: u8,
    /// Output tile image format.
    pub format: Format,
    /// Use TMS (south-origin) Y-axis ordering instead of XYZ.
    pub tms: bool,
    /// Tile grid CRS; `None` = take it from the raster's GeoKeys.
    pub crs: Option<Crs>,
    /// Optional output band count override (1–4).  `None` = inherit source band count.
    pub bands: Option<usize>,
    /// Tile width and height in pixels.
    pub tile_size: u32,
    /// Write `tilemapresource.xml` into the output directory.
    pub tmr: bool,
    /// Maximum source-pixel rows held in RAM at once (chunked I/O budget).
    pub chunk_size: usize,
    /// Tile resample backend (CPU or GPU).
    pub backend: Backend,
    /// Per-format encoder options derived from the config file.
    pub encode_opts: EncodeOptions,
}

impl Settings {
    /// Build resolved [`Settings`] by merging `config` defaults with `cli` overrides.
    ///
    /// CLI flags always win; config values are used when a flag is absent; built-in
    /// defaults apply when neither the CLI nor the config file specifies a value.
    pub fn resolve(cli: &Cli, config: &Config) -> anyhow::Result<Self> {
        let zoom = ZoomRange::parse(&cli.zoom)?;

        let format = parse_format(
            cli.extension
                .as_deref()
                .or(config.extension.as_deref())
                .unwrap_or("png"),
        )?;

        let tms = cli.tms.or(config.tms).unwrap_or(false);

        let crs = cli
            .crs
            .as_deref()
            .or(config.crs.as_deref())
            .map(parse_crs)
            .transpose()?;

        let bands = cli.bands.or(config.bands);
        let tile_size = cli.tilesize.or(config.tilesize).unwrap_or(256);
        let tmr = cli.tmr.or(config.tmr).unwrap_or(false);
        let chunk_size = cli.chunk_size.or(config.chunk_size).unwrap_or(512);
        let backend = cli
            .backend
            .as_deref()
            .or(config.backend.as_deref())
            .map(parse_backend)
            .transpose()?
            .unwrap_or_default();
        let encode_opts = build_encode_opts(config);

        Ok(Self {
            input: cli.input.clone(),
            output: cli.output.clone(),
            min_zoom: zoom.min,
            max_zoom: zoom.max,
            format,
            tms,
            crs,
            bands,
            tile_size,
            tmr,
            chunk_size,
            backend,
            encode_opts,
        })
    }
}

/// Parse a `--backend` string.
///
/// Returns an error for `"gpu"` when the crate was not built with the `gpu` feature, so the
/// failure is reported at startup rather than as a confusing later error.
fn parse_backend(s: &str) -> anyhow::Result<Backend> {
    match s.trim().to_lowercase().as_str() {
        "cpu" => Ok(Backend::Cpu),
        #[cfg(feature = "gpu")]
        "gpu" => Ok(Backend::Gpu),
        #[cfg(not(feature = "gpu"))]
        "gpu" => anyhow::bail!(
            "backend 'gpu' requested but this binary was not built with the 'gpu' feature"
        ),
        other => anyhow::bail!("unknown backend '{}'; supported: cpu, gpu", other),
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Parse a format extension string (case-insensitive, leading `.` stripped) into
/// a [`Format`] variant.
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

/// Parse a CRS string into a [`Crs`] variant.
///
/// Accepted aliases:
/// - `geographic`, `geodetic`, `4326`, `epsg:4326` → [`Crs::Wgs84`]
/// - `mercator`, `webmercator`, `3857`, `epsg:3857` → [`Crs::WebMercator`]
fn parse_crs(s: &str) -> anyhow::Result<Crs> {
    match s.trim().to_lowercase().as_str() {
        "geographic" | "geodetic" | "4326" | "epsg:4326" => Ok(Crs::Wgs84),
        "mercator" | "webmercator" | "3857" | "epsg:3857" => Ok(Crs::WebMercator),
        other => anyhow::bail!(
            "unknown CRS '{}'; supported: geographic (EPSG:4326), mercator (EPSG:3857)",
            other
        ),
    }
}

/// Build the complete [`EncodeOptions`] struct from per-format config sections.
///
/// Each section falls back to sensible defaults when the config key is absent.
fn build_encode_opts(cfg: &Config) -> EncodeOptions {
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

#[cfg(test)]
mod tests;

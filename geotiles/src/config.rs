//! Application config file format (TOML).
//!
//! Discovery order:
//! 1. `$XDG_CONFIG_HOME/geotiles/config.toml` (or `~/.config/geotiles/config.toml`)
//! 2. `--config <path>` CLI override takes precedence over the default location.
//!
//! CLI flags always override config values.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Top-level config file structure.
///
/// All fields are optional; unset fields fall back to CLI defaults.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    /// Output tile format extension: `png`, `jpg`, `webp`, `avif`, `jxl`.
    pub extension: Option<String>,
    /// Use TMS (south-origin) y-axis ordering.
    pub tms: Option<bool>,
    /// Target coordinate system: `geographic` (EPSG:4326) or `mercator` (EPSG:3857).
    pub crs: Option<String>,
    /// Output band count per tile (1–4).
    pub bands: Option<usize>,
    /// Tile width/height in pixels.
    pub tilesize: Option<u32>,
    /// Generate `tilemapresource.xml` in the output directory.
    pub tmr: Option<bool>,
    /// Source-pixel rows to hold in RAM at once (chunked I/O budget).
    pub chunk_size: Option<usize>,

    /// PNG encoder options.
    #[serde(default)]
    pub png: PngConfig,
    /// JPEG encoder options.
    #[serde(default)]
    pub jpeg: JpegConfig,
    /// WebP encoder options.
    #[serde(default)]
    pub webp: WebPConfig,
    /// AVIF encoder options.
    #[serde(default)]
    pub avif: AvifConfig,
    /// JPEG XL encoder options.
    #[serde(default)]
    pub jxl: JxlConfig,
}

// ── Per-format config sections ─────────────────────────────────────────────────

/// PNG encoder settings.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PngConfig {
    /// Compression preset: `default`, `fast`, `best`.
    pub compression: Option<String>,
    /// Filter heuristic: `adaptive`, `none`, `sub`, `up`, `avg`, `paeth`.
    pub filter: Option<String>,
}

/// JPEG encoder settings.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct JpegConfig {
    /// Quality 1–100 (default 85).
    pub quality: Option<u8>,
}

/// WebP encoder settings.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct WebPConfig {
    /// Use lossless encoding (default true; only lossless is supported currently).
    pub lossless: Option<bool>,
    /// Quality for a future lossy path (0–100, default 85).
    pub quality: Option<u8>,
}

/// AVIF encoder settings.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct AvifConfig {
    /// Encoder quality 1–100 (default 60).
    pub quality: Option<u8>,
    /// Encoder speed 1–10 (default 4).
    pub speed: Option<u8>,
}

/// JPEG XL encoder settings.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct JxlConfig {
    /// Butteraugli perceptual distance (default 1.0; 0.0 = mathematically lossless).
    pub distance: Option<f32>,
    /// Encoder effort 1–10 (default 7).
    pub effort: Option<u8>,
    /// True lossless encoding (default false).
    pub lossless: Option<bool>,
}

// ── Loading ────────────────────────────────────────────────────────────────────

/// Return the default config path: `$XDG_CONFIG_HOME/geotiles/config.toml`
/// falling back to `~/.config/geotiles/config.toml`.
pub fn default_config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs_next().map(|h| h.join(".config")))?;
    Some(base.join("geotiles").join("config.toml"))
}

/// Load and parse a config file.  Returns `Config::default()` when the file does
/// not exist; propagates I/O and parse errors otherwise.
pub fn load(path: &Path) -> anyhow::Result<Config> {
    if !path.exists() {
        return Ok(Config::default());
    }
    let raw = std::fs::read_to_string(path)?;
    let cfg: Config = toml::from_str(&raw)?;
    Ok(cfg)
}

/// Minimal shim for `dirs::home_dir` without pulling in the `dirs` crate.
fn dirs_next() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

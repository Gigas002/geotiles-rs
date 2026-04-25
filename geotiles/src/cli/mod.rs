//! Command-line interface definition.

use std::path::PathBuf;

use clap::Parser;

/// GeoTIFF → web-map tiles (XYZ / TMS layout).
#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Path to the input GeoTIFF (or any GDAL-supported raster).
    #[arg(short, long, value_name = "PATH")]
    pub input: PathBuf,

    /// Path to the output directory.  Created if it does not exist.
    #[arg(short, long, value_name = "PATH")]
    pub output: PathBuf,

    /// Zoom range (inclusive): `min..max`, e.g. `0..10` or `5..5` for a single level.
    #[arg(long, value_name = "RANGE")]
    pub zoom: String,

    /// Output tile format: `png`, `jpg`, `webp`, `avif`, `jxl`.
    #[arg(short, long, value_name = "EXT")]
    pub extension: Option<String>,

    /// Use TMS (south-origin) y-axis ordering instead of XYZ.
    #[arg(long)]
    pub tms: Option<bool>,

    /// Target coordinate system: `geographic` (EPSG:4326) or `mercator` (EPSG:3857).
    #[arg(long, value_name = "CRS")]
    pub crs: Option<String>,

    /// Output band count per tile (1–4).  Defaults to the source raster band count.
    #[arg(short, long, value_name = "N")]
    pub bands: Option<usize>,

    /// Tile width/height in pixels (must be a power of two; default 256).
    #[arg(long, value_name = "N")]
    pub tilesize: Option<u32>,

    /// Generate `tilemapresource.xml` in the output directory.
    #[arg(long)]
    pub tmr: Option<bool>,

    /// Source-pixel rows to hold in RAM per chunk (default 512).
    #[arg(long, value_name = "N")]
    pub chunk_size: Option<usize>,

    /// Path to a TOML config file.  Defaults to
    /// `$XDG_CONFIG_HOME/geotiles/config.toml`.
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,
}

/// Parsed zoom range with inclusive min and max.
#[derive(Debug, Clone, Copy)]
pub struct ZoomRange {
    pub min: u8,
    pub max: u8,
}

impl ZoomRange {
    /// Parse `"min..max"` or `"z"` (single zoom level) into an inclusive range.
    pub fn parse(s: &str) -> anyhow::Result<Self> {
        if let Some((a, b)) = s.split_once("..") {
            let min = a
                .trim()
                .parse::<u8>()
                .map_err(|e| anyhow::anyhow!("invalid zoom min '{}': {}", a, e))?;
            let max = b
                .trim()
                .parse::<u8>()
                .map_err(|e| anyhow::anyhow!("invalid zoom max '{}': {}", b, e))?;
            if min > max {
                anyhow::bail!("zoom min ({}) must be <= zoom max ({})", min, max);
            }
            Ok(Self { min, max })
        } else {
            let z = s
                .trim()
                .parse::<u8>()
                .map_err(|e| anyhow::anyhow!("invalid zoom '{}': {}", s, e))?;
            Ok(Self { min: z, max: z })
        }
    }
}

#[cfg(test)]
mod tests;

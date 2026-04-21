use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

use tracing::{info, info_span};

use crate::Result;
use crate::coords::{Bounds, DEFAULT_TILE_SIZE};
use crate::tile::{Format, ResampleBackend};

#[cfg(not(feature = "geographic"))]
use crate::error::Error;

/// Builder and entry point for the tile generation pipeline.
///
/// ```rust,no_run
/// # use libgeotiles::{GeoTiff, Format};
/// GeoTiff::open("input.tif").unwrap()
///     .zoom(4..=10)
///     .chunk_size(512)
///     .format(Format::Png)
///     .output("tiles/")
///     .crop().unwrap();
/// ```
pub struct GeoTiff {
    path: PathBuf,
    zoom: RangeInclusive<u8>,
    /// Maximum source-pixel rows loaded into RAM per iteration.
    chunk_size: usize,
    format: Format,
    output: PathBuf,
    backend: ResampleBackend,
    tile_size: u32,
}

impl GeoTiff {
    /// Open `path` for reading and return a builder with sensible defaults.
    ///
    /// Validates that the dataset is readable; the full pipeline runs only on [`Self::crop`].
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        // Validate the path opens as a GDAL dataset.
        let _ = crate::gdal_io::open_dataset(&path)?;
        Ok(Self {
            path,
            zoom: 0..=12,
            chunk_size: 512,
            format: Format::Png,
            output: PathBuf::from("tiles"),
            backend: ResampleBackend::Cpu,
            tile_size: DEFAULT_TILE_SIZE,
        })
    }

    pub fn zoom(mut self, range: RangeInclusive<u8>) -> Self {
        self.zoom = range;
        self
    }

    /// Maximum source-pixel rows read into RAM at once.
    ///
    /// Smaller values reduce peak memory; larger values reduce GDAL I/O calls.
    /// Default: 512. A value of 1 forces one GDAL read per tile row (useful for testing
    /// that output is identical regardless of chunk boundaries).
    pub fn chunk_size(mut self, rows: usize) -> Self {
        self.chunk_size = rows;
        self
    }

    pub fn format(mut self, fmt: Format) -> Self {
        self.format = fmt;
        self
    }

    pub fn output(mut self, dir: impl AsRef<Path>) -> Self {
        self.output = dir.as_ref().to_path_buf();
        self
    }

    pub fn backend(mut self, b: ResampleBackend) -> Self {
        self.backend = b;
        self
    }

    pub fn tile_size(mut self, size: u32) -> Self {
        self.tile_size = size;
        self
    }

    /// Run the full tiling pipeline, writing `{output}/{z}/{x}/{y}.{ext}` for every
    /// tile that overlaps the source raster extent.
    ///
    /// Phase 4 implements EPSG:4326 (geographic) tiles. The source is warped to 4326
    /// on-demand via a lazy VRT if needed. Requires the `geographic` Cargo feature.
    pub fn crop(self) -> Result<()> {
        let _span = info_span!("GeoTiff::crop", path = %self.path.display()).entered();

        let (src_ds, _) = crate::gdal_io::open_dataset(&self.path)?;
        // Warp to EPSG:4326 if the source is in another CRS; lazy VRT, no full reproject.
        let warped_opt = crate::gdal_io::warp_to_epsg(&src_ds, 4326)?;
        // working_ds borrows either the warp VRT or the original; both outlive this scope.
        let ds = warped_opt.as_ref().unwrap_or(&src_ds);

        let (ds_width, ds_height) = ds.raster_size();
        let gt = ds.geo_transform()?;
        let band_count = ds.raster_count();

        let ds_bounds = Bounds {
            min_x: gt[0],
            min_y: gt[3] + gt[5] * ds_height as f64,
            max_x: gt[0] + gt[1] * ds_width as f64,
            max_y: gt[3],
        };

        info!(
            ds_width,
            ds_height,
            band_count,
            min_lon = ds_bounds.min_x,
            min_lat = ds_bounds.min_y,
            max_lon = ds_bounds.max_x,
            max_lat = ds_bounds.max_y,
            "dataset ready"
        );

        self.crop_geographic(ds, ds_bounds, &gt, ds_width, ds_height, band_count)
    }

    #[cfg(feature = "geographic")]
    fn crop_geographic(
        &self,
        ds: &gdal::Dataset,
        ds_bounds: Bounds,
        gt: &[f64; 6],
        ds_width: usize,
        ds_height: usize,
        band_count: usize,
    ) -> Result<()> {
        use crate::coords::geographic::Geographic;
        use tracing::debug;

        let geo = Geographic::new(self.tile_size);

        for z in self.zoom.clone() {
            let _span = info_span!("zoom", z).entered();
            let (tile_min, tile_max) = geo.tile_range(ds_bounds, z);
            info!(
                z,
                x_min = tile_min.x,
                x_max = tile_max.x,
                y_min = tile_min.y,
                y_max = tile_max.y,
                "zoom level"
            );

            for ty in tile_min.y..=tile_max.y {
                for tx in tile_min.x..=tile_max.x {
                    let tile = crate::coords::Tile::new(tx, ty, z);
                    let tile_bounds = geo.bounds(tile);

                    let Some(win) =
                        crate::gdal_io::source_window(&tile_bounds, gt, ds_width, ds_height)
                    else {
                        debug!(tx, ty, z, "tile does not overlap dataset, skipping");
                        continue;
                    };

                    debug!(tx, ty, z, ?win, "processing tile");

                    // For Phase 4: one chunk read per tile (covers exactly this tile's rows).
                    // Phase 5 will batch multiple tiles per chunk read.
                    let chunk = crate::gdal_io::read_chunk(ds, win.row, win.height)?;
                    let pixels = crate::tile::crop_tile(&chunk, win, self.tile_size)?;
                    let encoded = crate::encode::encode_tile(
                        &pixels,
                        self.tile_size,
                        self.tile_size,
                        band_count,
                        self.format,
                    )?;

                    let tile_path = self
                        .output
                        .join(z.to_string())
                        .join(tx.to_string())
                        .join(format!("{}.{}", ty, self.format.extension()));
                    std::fs::create_dir_all(tile_path.parent().unwrap())?;
                    std::fs::write(&tile_path, &encoded)?;

                    debug!(path = %tile_path.display(), bytes = encoded.len(), "tile written");
                }
            }
        }

        Ok(())
    }

    #[cfg(not(feature = "geographic"))]
    fn crop_geographic(
        &self,
        _ds: &gdal::Dataset,
        _ds_bounds: Bounds,
        _gt: &[f64; 6],
        _ds_width: usize,
        _ds_height: usize,
        _band_count: usize,
    ) -> Result<()> {
        Err(Error::Encode(
            "no CRS tile math compiled in; enable the 'geographic' or 'mercator' feature".into(),
        ))
    }
}

#[cfg(test)]
mod tests;

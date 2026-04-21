use std::ffi::CString;
use std::path::Path;

use gdal::Dataset;
use gdal::spatial_ref::SpatialRef;
use tracing::{debug, info};

use crate::coords::Bounds;
use crate::tile::{ChunkBuffer, PixelWindow};

use crate::Result;

/// Metadata read from a GDAL dataset on open.
pub struct DatasetInfo {
    pub width: usize,
    pub height: usize,
    /// GDAL affine geo-transform: [x_origin, x_pixel, x_rot, y_origin, y_rot, y_pixel]
    pub geo_transform: [f64; 6],
    pub projection: String,
    pub band_count: usize,
}

/// Open a raster dataset and read its basic metadata.
pub fn open_dataset(path: &Path) -> Result<(Dataset, DatasetInfo)> {
    let _span = tracing::info_span!("open_dataset", path = %path.display()).entered();

    info!("opening dataset");
    let ds = Dataset::open(path)?;

    let (width, height) = ds.raster_size();
    let geo_transform = ds.geo_transform()?;
    let projection = ds.projection();
    let band_count = ds.raster_count();

    debug!(
        width,
        height,
        band_count,
        projection = %projection,
        geo_transform = ?geo_transform,
        "dataset metadata",
    );

    Ok((
        ds,
        DatasetInfo {
            width,
            height,
            geo_transform,
            projection,
            band_count,
        },
    ))
}

/// Warp `src` into `target_epsg` using a lazy in-memory VRT (`GDALAutoCreateWarpedVRT`).
///
/// Returns `None` when the source is already in the target CRS — no copy is made.
/// Returns `Some(vrt)` otherwise; the VRT reprojects source data on demand, keeping
/// RAM usage proportional to what is actually read rather than the full raster.
///
/// # Lifetimes
/// The returned VRT dataset holds an internal GDAL reference to `src`. Ensure `src`
/// outlives the returned dataset.
pub fn warp_to_epsg(src: &Dataset, target_epsg: u32) -> Result<Option<Dataset>> {
    // Compare by EPSG authority code — more reliable than WKT equivalence in GDAL 3+.
    if matches!(crate::crs::epsg_of(src), Ok(Some(src_epsg)) if src_epsg == target_epsg) {
        debug!(target_epsg, "source already in target CRS, skipping warp");
        return Ok(None);
    }

    let dst_srs = SpatialRef::from_epsg(target_epsg)?;
    let dst_wkt = CString::new(dst_srs.to_wkt()?)?;

    // Safety: src.c_dataset() is a valid open handle; dst_wkt is a valid C string;
    // null src WKT means GDAL reads the projection from the source handle.
    let warped_h = unsafe {
        gdal_sys::GDALAutoCreateWarpedVRT(
            src.c_dataset(),
            std::ptr::null(),
            dst_wkt.as_ptr(),
            gdal_sys::GDALResampleAlg::GRA_Bilinear,
            0.125,
            std::ptr::null(),
        )
    };

    if warped_h.is_null() {
        return Err(gdal::errors::GdalError::NullPointer {
            method_name: "GDALAutoCreateWarpedVRT",
            msg: format!("failed to warp dataset to EPSG:{target_epsg}"),
        }
        .into());
    }

    // Safety: warped_h is a valid non-null GDAL dataset handle.
    let warped = unsafe { Dataset::from_c_dataset(warped_h) };
    let (width, height) = warped.raster_size();
    let gt = warped.geo_transform()?;

    info!(
        target_epsg,
        width,
        height,
        origin_x = gt[0],
        origin_y = gt[3],
        pixel_x = gt[1],
        pixel_y = gt[5],
        "warped VRT created"
    );

    Ok(Some(warped))
}

/// Compute the source-pixel window corresponding to `tile_geo` (in dataset CRS units).
///
/// Assumes a north-up raster (`gt[2] == 0`, `gt[4] == 0`). Returns `None` when the
/// tile does not overlap the dataset extent.
pub fn source_window(
    tile_geo: &Bounds,
    gt: &[f64; 6],
    ds_width: usize,
    ds_height: usize,
) -> Option<PixelWindow> {
    // gt[1] > 0 (pixel width), gt[5] < 0 (pixel height, top-down)
    let col_f = (tile_geo.min_x - gt[0]) / gt[1];
    let col_t = (tile_geo.max_x - gt[0]) / gt[1];
    // max_y = top of tile → smallest row; min_y = bottom → largest row
    let row_f = (tile_geo.max_y - gt[3]) / gt[5];
    let row_t = (tile_geo.min_y - gt[3]) / gt[5];

    let col_start = col_f.floor().max(0.0) as usize;
    let col_end = (col_t.ceil().max(0.0) as usize).min(ds_width);
    let row_start = row_f.floor().max(0.0) as usize;
    let row_end = (row_t.ceil().max(0.0) as usize).min(ds_height);

    if col_start >= col_end || row_start >= row_end {
        debug!(
            col_start,
            col_end, row_start, row_end, "source_window: tile outside dataset extent"
        );
        return None;
    }

    let w = PixelWindow {
        col: col_start,
        row: row_start,
        width: col_end - col_start,
        height: row_end - row_start,
    };
    debug!(?w, "source_window");
    Some(w)
}

/// Read `row_count` source rows starting at absolute dataset row `row_start` into RAM.
///
/// All bands are read at full dataset width (so that [`crate::tile::crop_tile`] can
/// index any column within the row using `window.col`). Data type is converted to `u8`
/// by GDAL RasterIO; values from non-byte rasters are scaled/clamped automatically.
pub fn read_chunk(ds: &Dataset, row_start: usize, row_count: usize) -> crate::Result<ChunkBuffer> {
    let (ds_width, _ds_height) = ds.raster_size();
    let band_count = ds.raster_count();

    debug!(row_start, row_count, ds_width, band_count, "read_chunk");

    let mut band_data = Vec::with_capacity(band_count);
    for band_idx in 1..=band_count {
        let band = ds.rasterband(band_idx)?;
        let buf = band.read_as::<u8>(
            (0, row_start as isize),
            (ds_width, row_count),
            (ds_width, row_count),
            None,
        )?;
        band_data.push(buf.data().to_vec());
    }

    Ok(ChunkBuffer {
        band_data,
        ds_width,
        row_start,
        row_count,
    })
}

/// Append a synthetic alpha channel to `chunk` by reading GDAL's mask band for the given
/// chunk region.
///
/// GDAL's mask band (from `GDALGetMaskBand`) encodes validity at native pixel precision,
/// so this works correctly for Float32, Int16, etc. — unlike comparing u8-converted pixel
/// values against a stored f64 nodata value.
///
/// **Mask value semantics:** 255 = valid (fully opaque), 0 = nodata / masked (transparent).
///
/// **Behaviour by source band count:**
/// - `band_count == 1` or `3` — if masking is non-trivial, a new alpha band is appended,
///   making the effective band count 2 or 4 respectively.
/// - `band_count == 4` with `GMF_ALPHA` set — the dataset carries a real alpha band that
///   GDAL already read as the 4th data band; no synthetic band is added.
/// - `band_count == 4` with other masking — silently skipped (adding a 5th band is
///   not supported by the encoder pipeline).
/// - All-valid mask (`GMF_ALL_VALID`) — early return, no allocation.
///
/// Returns `true` when an alpha band was appended (effective band count increased by 1).
pub fn append_mask_alpha(
    ds: &Dataset,
    chunk: &mut crate::tile::ChunkBuffer,
    row_start: usize,
    row_count: usize,
) -> crate::Result<bool> {
    use tracing::debug;

    let ds_width = chunk.ds_width;

    // Inspect band 1's mask flags (typically shared per-dataset for nodata).
    let band1 = ds.rasterband(1)?;
    let flags = band1.mask_flags()?;

    // Fast path: nothing to mask.
    if flags.is_all_valid() {
        return Ok(false);
    }

    // The dataset already has an explicit alpha band (GDAL reads it as the last data band).
    // Our 4-band RGBA path handles it natively — no synthetic band needed.
    if flags.is_alpha() {
        return Ok(false);
    }

    // A 4-band dataset with nodata masking (no alpha) would become 5-band after adding alpha,
    // which the encoder pipeline does not support. Skip rather than produce a hard error.
    if chunk.band_count() == 4 {
        debug!(
            "4-band dataset with nodata masking: skipping alpha synthesis (band 4 is not an alpha band)"
        );
        return Ok(false);
    }

    // Read the primary mask band.  For GMF_PER_DATASET (nodata masking, most common), all
    // bands share a single mask so one read is sufficient.
    let mask_band = band1.open_mask_band()?;
    let mask_buf = mask_band.read_as::<u8>(
        (0, row_start as isize),
        (ds_width, row_count),
        (ds_width, row_count),
        None,
    )?;
    let mut alpha = mask_buf.data().to_vec();

    // For non-per-dataset masks each band has its own mask; AND-combine them so that a
    // pixel is transparent if *any* band considers it invalid.
    if !flags.is_per_dataset() {
        let band_count = ds.raster_count();
        for band_idx in 2..=band_count {
            let band = ds.rasterband(band_idx)?;
            if band.mask_flags()?.is_all_valid() {
                continue;
            }
            let mb = band.open_mask_band()?;
            let mb_buf = mb.read_as::<u8>(
                (0, row_start as isize),
                (ds_width, row_count),
                (ds_width, row_count),
                None,
            )?;
            for (a, m) in alpha.iter_mut().zip(mb_buf.data().iter()) {
                if *m == 0 {
                    *a = 0;
                }
            }
        }
    }

    debug!(
        row_start,
        row_count,
        prev_band_count = chunk.band_count(),
        "appended synthetic alpha band from GDAL mask"
    );

    chunk.band_data.push(alpha);
    Ok(true)
}

#[cfg(test)]
mod tests;

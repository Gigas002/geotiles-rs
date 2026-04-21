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

#[cfg(test)]
mod tests;

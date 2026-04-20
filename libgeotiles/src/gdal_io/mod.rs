use std::ffi::CString;
use std::path::Path;

use gdal::Dataset;
use gdal::spatial_ref::SpatialRef;
use tracing::{debug, info};

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

#[cfg(test)]
mod tests;

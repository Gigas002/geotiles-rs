use std::path::Path;

use gdal::Dataset;
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

use gdal::Dataset;
use tracing::debug;

use crate::Result;

/// Attempt to identify the EPSG code of a dataset's spatial reference.
///
/// Returns `None` for an unrecognized or missing CRS rather than an error,
/// since callers often need to decide what to do when the CRS is unknown.
pub fn epsg_of(ds: &Dataset) -> Result<Option<u32>> {
    let mut srs = ds.spatial_ref()?;
    let _ = srs.auto_identify_epsg(); // best-effort; ignore failure
    match srs.authority() {
        Ok(auth) => {
            let code = auth.split(':').nth(1).and_then(|s| s.parse::<u32>().ok());
            debug!(authority = %auth, epsg = ?code, "detected CRS");
            Ok(code)
        }
        Err(_) => Ok(None),
    }
}

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[cfg(feature = "geotiff")]
    #[error("TIFF error: {0}")]
    Tiff(#[from] tiff::TiffError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("image buffer error: {0}")]
    ImageBuffer(#[from] fast_image_resize::ImageBufferError),

    #[error("resize error: {0}")]
    Resize(#[from] fast_image_resize::ResizeError),

    #[error("crop box error: {0}")]
    CropBox(#[from] fast_image_resize::CropBoxError),

    /// Encoding a tile to the requested format failed.
    #[error("encode error: {0}")]
    Encode(String),

    /// Band count outside 1–4 (L, LA, RGB, RGBA).
    #[error("unsupported band count: {0}")]
    BadBandCount(usize),

    /// GPU context initialisation or per-tile operation failed.
    #[error("GPU error: {0}")]
    Gpu(String),

    /// No `ModelPixelScaleTag`/`ModelTiepointTag`/`ModelTransformationTag` found — the file
    /// carries no georeferencing.
    #[error("no georeferencing found in TIFF (missing GeoTIFF model tags)")]
    MissingGeoreferencing,

    /// Input this library does not take as-is (layout, sample type, georeferencing).
    /// Pre-process it with a GeoTIFF-capable tool first.
    #[error("unsupported input: {0}")]
    Unsupported(String),

    /// The raster declares a CRS other than the tile grid's.
    #[error("raster is in EPSG:{raster} but the tile grid is {grid}; reproject the input first")]
    CrsMismatch { raster: u32, grid: crate::geo::Crs },

    /// Tiler options out of range.
    #[error("invalid options: {0}")]
    InvalidOptions(String),
}

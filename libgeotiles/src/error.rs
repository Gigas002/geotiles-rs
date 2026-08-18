use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
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

    /// The requested tile does not overlap the dataset extent.
    #[error("tile ({x}, {y}, z={z}) does not overlap the dataset")]
    OutOfBounds { x: u32, y: u32, z: u8 },

    /// Band count not supported by the current pipeline (1, 3, or 4 bands expected).
    #[error("unsupported band count: {0}")]
    BadBandCount(usize),

    /// GPU context initialisation or per-tile operation failed.
    #[error("GPU error: {0}")]
    Gpu(String),

    /// No `ModelPixelScaleTag`/`ModelTiepointTag`/`ModelTransformationTag` found — the file
    /// carries no georeferencing.
    #[error("no georeferencing found in TIFF (missing GeoTIFF model tags)")]
    MissingGeoreferencing,

    /// A TIFF layout or sample format this pipeline does not (yet) support.
    #[error("unsupported TIFF layout: {0}")]
    Unsupported(String),
}

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("GDAL error: {0}")]
    Gdal(#[from] gdal::errors::GdalError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("C string contains interior null byte: {0}")]
    NulByte(#[from] std::ffi::NulError),

    #[error("image buffer error: {0}")]
    ImageBuffer(#[from] fast_image_resize::ImageBufferError),

    #[error("resize error: {0}")]
    Resize(#[from] fast_image_resize::ResizeError),

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
}

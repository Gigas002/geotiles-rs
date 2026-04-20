use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("GDAL error: {0}")]
    Gdal(#[from] gdal::errors::GdalError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("C string contains interior null byte: {0}")]
    NulByte(#[from] std::ffi::NulError),
}

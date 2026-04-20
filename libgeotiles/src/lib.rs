pub mod error;
#[doc(hidden)]
pub mod gdal_io;

pub use error::Error;
pub type Result<T> = std::result::Result<T, Error>;

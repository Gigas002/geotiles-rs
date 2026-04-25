/// Output format for encoded tiles.
///
/// All variants are always available; encoding returns [`crate::error::Error::Encode`]
/// at runtime if the corresponding Cargo feature (`png`, `jpeg`, `webp`, `avif`, `jxl`)
/// is not compiled in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    #[default]
    Png,
    Jpeg,
    WebP,
    Avif,
    Jxl,
}

impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::WebP => "webp",
            Self::Avif => "avif",
            Self::Jxl => "jxl",
        }
    }
}

/// A rectangular region in source-pixel space (column-major, top-left origin).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelWindow {
    pub col: usize,
    pub row: usize,
    pub width: usize,
    pub height: usize,
}

/// In-RAM buffer for a horizontal slice of source rows, all bands, planar layout.
///
/// `band_data[b]` holds `ds_width * row_count` u8 values in row-major order for band `b+1`.
pub struct ChunkBuffer {
    pub band_data: Vec<Vec<u8>>,
    pub ds_width: usize,
    pub row_start: usize,
    pub row_count: usize,
}

impl ChunkBuffer {
    pub fn band_count(&self) -> usize {
        self.band_data.len()
    }

    pub fn contains_row(&self, row: usize) -> bool {
        row >= self.row_start && row < self.row_start + self.row_count
    }
}

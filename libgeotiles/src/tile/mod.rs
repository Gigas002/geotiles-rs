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

/// The sub-rectangle of a `tile_size × tile_size` destination canvas that a resampled
/// [`PixelWindow`] should be written into.
///
/// Only equal to the full canvas (`x = y = 0`, `width = height = tile_size`) when the
/// tile's geographic bounds are fully covered by the source dataset on every side. For
/// tiles that straddle the dataset edge (or a dataset smaller than one tile), this is a
/// smaller rectangle placed at the position proportional to where the overlapping source
/// data actually falls within the tile — the rest of the canvas is left at its
/// zero-initialised (transparent, for RGBA) value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DstRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl DstRect {
    /// The full `tile_size × tile_size` canvas, e.g. for tiles entirely interior to the
    /// dataset.
    pub fn full(tile_size: u32) -> Self {
        Self {
            x: 0,
            y: 0,
            width: tile_size,
            height: tile_size,
        }
    }
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

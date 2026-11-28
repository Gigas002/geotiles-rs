//! Tile output.

use std::path::{Path, PathBuf};

use crate::Result;
use crate::coords::{Tile, flip_y};
use crate::encode::Format;

/// Destination for encoded tiles. Called concurrently from worker threads.
pub trait TileSink: Sync {
    fn write_tile(&self, tile: Tile, format: Format, data: &[u8]) -> Result<()>;
}

/// Writes `<root>/<z>/<x>/<y>.<ext>`, with `y` in XYZ (north = 0) or TMS (south = 0)
/// order.
#[derive(Debug, Clone)]
pub struct DirSink {
    root: PathBuf,
    tms: bool,
}

impl DirSink {
    pub fn new(root: impl Into<PathBuf>, tms: bool) -> Self {
        Self {
            root: root.into(),
            tms,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Path a tile is written to.
    pub fn tile_path(&self, tile: Tile, format: Format) -> PathBuf {
        let y = if self.tms {
            flip_y(tile.y, tile.z)
        } else {
            tile.y
        };
        self.root
            .join(tile.z.to_string())
            .join(tile.x.to_string())
            .join(format!("{y}.{}", format.extension()))
    }
}

impl TileSink for DirSink {
    fn write_tile(&self, tile: Tile, format: Format, data: &[u8]) -> Result<()> {
        let path = self.tile_path(tile, format);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, data)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xyz_and_tms_paths() {
        let t = Tile::new(3, 1, 2);
        assert_eq!(
            DirSink::new("/o", false).tile_path(t, Format::Png),
            PathBuf::from("/o/2/3/1.png")
        );
        assert_eq!(
            DirSink::new("/o", true).tile_path(t, Format::Jpeg),
            PathBuf::from("/o/2/3/2.jpg")
        );
    }
}

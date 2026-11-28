# Changelog

## Unreleased

### Changed

- The tiling pipeline lives in `libgeotiles` (`generate`) behind `RasterSource`, `TileGrid`,
  `Resampler` and `TileSink` traits; the CLI is a thin wrapper.
- Source windows are fractional: tiles are placed with sub-pixel accuracy at every zoom
  (previously up to one source pixel off, visible at deep zooms).
- Row batches read a kernel-sized margin, so output no longer depends on `chunk_size`.
- CPU resampling works in place on the row block (no per-tile copies).
- GPU backend box-filters when downscaling instead of point-sampling (no aliasing at low
  zooms) and uses the same sub-pixel mapping.
- `--crs` defaults to the raster's GeoKeys; a contradicting `--crs` is an error.
- Gray sources converted to RGB(A) via `--bands` are replicated instead of padded with 255.
- Removed the `geographic`/`mercator` features (both grids are always built); added a
  default `geotiff` feature.
- No GDAL anywhere: CI no longer installs it, docs describe input prep as a separate step.

### Added

- Input validation with actionable errors: CRS (EPSG from GeoKeys, incl. 3857 aliases),
  rotation, south-up, GCPs, sample format, WhiteIsZero, palette/CMYK/YCbCr; `PixelIsPoint`
  is honoured; a nodata tag triggers a warning.
- Option validation (bands, zoom ≤ 30, tile size, chunk size, compiled-in format) before
  any work starts.
- `backend` config key.

## [0.1.0] - 2026-09-04

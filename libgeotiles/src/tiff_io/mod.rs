use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use tiff::decoder::{ChunkType, Decoder, DecodingResult};
use tiff::tags::Tag;
use tracing::{debug, info};

use crate::Result;
use crate::coords::Bounds;
use crate::error::Error;
use crate::tile::{ChunkBuffer, PixelWindow};

/// Metadata read from a TIFF dataset on open.
pub struct DatasetInfo {
    pub width: usize,
    pub height: usize,
    /// GDAL-style affine geo-transform: `[x_origin, x_pixel, x_rot, y_origin, y_rot, y_pixel]`.
    ///
    /// Derived directly from the GeoTIFF's own `ModelPixelScaleTag`/`ModelTiepointTag` (or
    /// `ModelTransformationTag`) — there is no reprojection step. The caller is responsible
    /// for supplying a raster already in the desired CRS.
    pub geo_transform: [f64; 6],
    pub band_count: usize,
}

/// An opened TIFF dataset kept alive across chunked reads.
pub struct RasterDataset {
    decoder: Decoder<BufReader<File>>,
    width: usize,
    height: usize,
    band_count: usize,
    chunk_type: ChunkType,
    /// Native chunk width/height: `(width, rows_per_strip)` for strips, `(tile_w, tile_h)` for tiles.
    chunk_w: usize,
    chunk_h: usize,
    /// Only meaningful for `ChunkType::Tile`: number of tile columns across the image.
    tiles_across: usize,
}

/// Open a raster dataset and read its basic metadata (dimensions, band count, geotransform).
///
/// Requires 8-bit-per-sample, chunky (pixel-interleaved) TIFF data. Reprojection and
/// nodata/mask handling are intentionally not performed here — pre-process the input with
/// any GeoTIFF-capable tool first if either is needed.
pub fn open_dataset(path: &Path) -> Result<(RasterDataset, DatasetInfo)> {
    let _span = tracing::info_span!("open_dataset", path = %path.display()).entered();

    info!("opening dataset");
    let file = File::open(path)?;
    let mut decoder = Decoder::new(BufReader::new(file))?;

    let (width_u32, height_u32) = decoder.dimensions()?;
    let (width, height) = (width_u32 as usize, height_u32 as usize);

    let band_count = match decoder.find_tag(Tag::SamplesPerPixel)? {
        None => 1,
        Some(value) => value.into_u16()? as usize,
    };

    // Reject non-interleaved (planar) storage: the `tiff` crate's chunk-read API only
    // decodes the first sample plane for `PlanarConfiguration::Planar`, which would silently
    // produce wrong (not erroring) output for multi-band planar TIFFs.
    if let Some(value) = decoder.find_tag(Tag::PlanarConfiguration)? {
        let planar_config = value.into_u16()?;
        if planar_config != 1 {
            return Err(Error::Unsupported(
                "planar (non-interleaved) TIFF storage is not supported; re-encode as chunky/interleaved"
                    .into(),
            ));
        }
    }

    let geo_transform = geo_transform_of(&mut decoder)?;

    let chunk_type = decoder.get_chunk_type();
    let (chunk_w_u32, chunk_h_u32) = decoder.chunk_dimensions();
    let (chunk_w, chunk_h) = (chunk_w_u32 as usize, chunk_h_u32 as usize);
    let tiles_across = if chunk_type == ChunkType::Tile {
        width.div_ceil(chunk_w.max(1))
    } else {
        0
    };

    debug!(
        width,
        height,
        band_count,
        ?chunk_type,
        chunk_w,
        chunk_h,
        ?geo_transform,
        "dataset metadata",
    );

    Ok((
        RasterDataset {
            decoder,
            width,
            height,
            band_count,
            chunk_type,
            chunk_w,
            chunk_h,
            tiles_across,
        },
        DatasetInfo {
            width,
            height,
            geo_transform,
            band_count,
        },
    ))
}

/// Derive a GDAL-style 6-element affine geo-transform from the GeoTIFF's own georeferencing
/// tags (`ModelTransformationTag`, or `ModelTiepointTag` + `ModelPixelScaleTag`).
///
/// Ref: <https://docs.ogc.org/is/19-008r4/19-008r4.html#_raster_to_model_coordinate_transformation_requirements>
fn geo_transform_of<R: std::io::Read + std::io::Seek>(
    decoder: &mut Decoder<R>,
) -> Result<[f64; 6]> {
    let transformation_matrix = decoder
        .find_tag(Tag::ModelTransformationTag)?
        .map(|value| value.into_f64_vec())
        .transpose()?;

    if let Some(matrix) = transformation_matrix {
        let matrix: [f64; 16] = matrix
            .try_into()
            .map_err(|_| Error::Unsupported("ModelTransformationTag must have 16 values".into()))?;
        // `matrix` maps (raster_x, raster_y) -> (model_x, model_y):
        //   model_x = raster_x * m[0] + raster_y * m[1] + m[3]
        //   model_y = raster_x * m[4] + raster_y * m[5] + m[7]
        // Re-index into the GDAL convention [x0, px, rx, y0, ry, py].
        return Ok([
            matrix[3], matrix[0], matrix[1], matrix[7], matrix[4], matrix[5],
        ]);
    }

    let pixel_scale = decoder
        .find_tag(Tag::ModelPixelScaleTag)?
        .map(|value| value.into_f64_vec())
        .transpose()?;
    let tie_points = decoder
        .find_tag(Tag::ModelTiepointTag)?
        .map(|value| value.into_f64_vec())
        .transpose()?;

    let (Some(pixel_scale), Some(tie_points)) = (pixel_scale, tie_points) else {
        return Err(Error::MissingGeoreferencing);
    };
    if pixel_scale.len() != 3 || tie_points.len() < 6 {
        return Err(Error::Unsupported(
            "ModelPixelScaleTag must have 3 values and ModelTiepointTag at least 6".into(),
        ));
    }

    // Single tie point + pixel scale: the common case for north-up GeoTIFFs.
    let (raster_x, raster_y) = (tie_points[0], tie_points[1]);
    let (model_x, model_y) = (tie_points[3], tie_points[4]);
    let px = pixel_scale[0];
    let py = -pixel_scale[1];
    Ok([
        model_x - raster_x * px,
        px,
        0.0,
        model_y - raster_y * py,
        0.0,
        py,
    ])
}

/// Compute the source-pixel window corresponding to `tile_geo` (in dataset CRS units).
///
/// Assumes a north-up raster (`gt[2] == 0`, `gt[4] == 0`). Returns `None` when the
/// tile does not overlap the dataset extent.
pub fn source_window(
    tile_geo: &Bounds,
    gt: &[f64; 6],
    ds_width: usize,
    ds_height: usize,
) -> Option<PixelWindow> {
    // gt[1] > 0 (pixel width), gt[5] < 0 (pixel height, top-down)
    let col_f = (tile_geo.min_x - gt[0]) / gt[1];
    let col_t = (tile_geo.max_x - gt[0]) / gt[1];
    // max_y = top of tile → smallest row; min_y = bottom → largest row
    let row_f = (tile_geo.max_y - gt[3]) / gt[5];
    let row_t = (tile_geo.min_y - gt[3]) / gt[5];

    let col_start = col_f.floor().max(0.0) as usize;
    let col_end = (col_t.ceil().max(0.0) as usize).min(ds_width);
    let row_start = row_f.floor().max(0.0) as usize;
    let row_end = (row_t.ceil().max(0.0) as usize).min(ds_height);

    if col_start >= col_end || row_start >= row_end {
        debug!(
            col_start,
            col_end, row_start, row_end, "source_window: tile outside dataset extent"
        );
        return None;
    }

    let w = PixelWindow {
        col: col_start,
        row: row_start,
        width: col_end - col_start,
        height: row_end - row_start,
    };
    debug!(?w, "source_window");
    Some(w)
}

/// Read `row_count` source rows starting at absolute dataset row `row_start` into RAM.
///
/// All bands are read at full dataset width (so that [`crate::backend::cpu::crop_tile`]
/// can index any column within the row using `window.col`). Decodes only the TIFF
/// strips/tiles that overlap the requested row range, keeping RAM proportional to
/// `row_count` rather than the whole raster. Samples must already be 8-bit unsigned.
pub fn read_chunk(
    ds: &mut RasterDataset,
    row_start: usize,
    row_count: usize,
) -> Result<ChunkBuffer> {
    debug!(
        row_start,
        row_count,
        ds_width = ds.width,
        band_count = ds.band_count,
        "read_chunk"
    );

    let row_end = (row_start + row_count).min(ds.height);
    let band_count = ds.band_count;
    let width = ds.width;

    let mut band_data: Vec<Vec<u8>> = (0..band_count)
        .map(|_| vec![0u8; width * row_count])
        .collect();

    match ds.chunk_type {
        ChunkType::Strip => {
            let rows_per_strip = ds.chunk_h.max(1);
            let first = row_start / rows_per_strip;
            let last = row_end.saturating_sub(1) / rows_per_strip;

            for strip_idx in first..=last {
                let (strip_w, strip_h) = ds.decoder.chunk_data_dimensions(strip_idx as u32);
                let (strip_w, strip_h) = (strip_w as usize, strip_h as usize);
                let buf = read_u8_chunk(&mut ds.decoder, strip_idx as u32)?;

                let strip_row_start = strip_idx * rows_per_strip;
                copy_interleaved_rows(
                    &buf,
                    strip_w,
                    strip_h,
                    band_count,
                    strip_row_start,
                    0,
                    row_start,
                    row_end,
                    width,
                    &mut band_data,
                );
            }
        }
        ChunkType::Tile => {
            let tile_w = ds.chunk_w.max(1);
            let tile_h = ds.chunk_h.max(1);
            let tiles_across = ds.tiles_across.max(1);

            let tile_row_first = row_start / tile_h;
            let tile_row_last = row_end.saturating_sub(1) / tile_h;

            for tile_row in tile_row_first..=tile_row_last {
                for tile_col in 0..tiles_across {
                    let idx = (tile_row * tiles_across + tile_col) as u32;
                    let (tile_data_w, tile_data_h) = ds.decoder.chunk_data_dimensions(idx);
                    let (tile_data_w, tile_data_h) = (tile_data_w as usize, tile_data_h as usize);
                    let buf = read_u8_chunk(&mut ds.decoder, idx)?;

                    let tile_abs_row_start = tile_row * tile_h;
                    let tile_abs_col_start = tile_col * tile_w;
                    copy_interleaved_rows(
                        &buf,
                        tile_data_w,
                        tile_data_h,
                        band_count,
                        tile_abs_row_start,
                        tile_abs_col_start,
                        row_start,
                        row_end,
                        width,
                        &mut band_data,
                    );
                }
            }
        }
    }

    Ok(ChunkBuffer {
        band_data,
        ds_width: width,
        row_start,
        row_count,
    })
}

/// Read one chunk (strip or tile) and require it to already be 8-bit unsigned samples.
fn read_u8_chunk(decoder: &mut Decoder<BufReader<File>>, chunk_index: u32) -> Result<Vec<u8>> {
    match decoder.read_chunk(chunk_index)? {
        DecodingResult::U8(buf) => Ok(buf),
        other => Err(Error::Unsupported(format!(
            "expected 8-bit unsigned samples, found {other:?}; convert the input to Byte first"
        ))),
    }
}

/// Copy the rows of a decoded interleaved chunk buffer that fall within `[row_start, row_end)`
/// into the planar `band_data` output, offsetting columns by `col_offset` within the full
/// dataset width.
#[allow(clippy::too_many_arguments)]
fn copy_interleaved_rows(
    buf: &[u8],
    buf_w: usize,
    buf_h: usize,
    band_count: usize,
    chunk_abs_row_start: usize,
    col_offset: usize,
    row_start: usize,
    row_end: usize,
    ds_width: usize,
    band_data: &mut [Vec<u8>],
) {
    for r_local in 0..buf_h {
        let abs_row = chunk_abs_row_start + r_local;
        if abs_row < row_start || abs_row >= row_end {
            continue;
        }
        let out_row = abs_row - row_start;
        for c_local in 0..buf_w {
            let abs_col = col_offset + c_local;
            if abs_col >= ds_width {
                continue;
            }
            let src_base = (r_local * buf_w + c_local) * band_count;
            let out_idx = out_row * ds_width + abs_col;
            for (b, band) in band_data.iter_mut().enumerate().take(band_count) {
                band[out_idx] = buf[src_base + b];
            }
        }
    }
}

#[cfg(test)]
mod tests;

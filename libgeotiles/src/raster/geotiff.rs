//! [`RasterSource`] for GeoTIFF files, via the pure-Rust `tiff` crate.
//!
//! Accepted input is exactly what the tiler consumes: 8-bit unsigned, chunky
//! (pixel-interleaved), gray/gray+alpha/RGB/RGBA, north-up. Anything else is rejected
//! with an explanation instead of being converted.

use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::Path;

use tiff::ColorType;
use tiff::decoder::{ChunkType, Decoder, DecodingResult};
use tiff::tags::{PhotometricInterpretation, PlanarConfiguration, SampleFormat, Tag};
use tracing::{debug, warn};

use super::{RasterInfo, RasterSource, RowBlock};
use crate::Result;
use crate::error::Error;
use crate::geo::GeoTransform;

// GeoKey ids (OGC GeoTIFF 1.1, §7).
const GT_MODEL_TYPE: u16 = 1024;
const GT_RASTER_TYPE: u16 = 1025;
const GEOGRAPHIC_TYPE: u16 = 2048;
const PROJECTED_CS_TYPE: u16 = 3072;
const RASTER_PIXEL_IS_POINT: u16 = 2;
const MODEL_TYPE_PROJECTED: u16 = 1;
const MODEL_TYPE_GEOGRAPHIC: u16 = 2;
const USER_DEFINED: u16 = 32767;

/// An opened GeoTIFF, decoding strips/tiles on demand.
pub struct GeoTiff<R: Read + Seek = BufReader<File>> {
    decoder: Decoder<R>,
    info: RasterInfo,
    chunk_type: ChunkType,
    chunk_w: usize,
    chunk_h: usize,
}

impl GeoTiff {
    pub fn open(path: &Path) -> Result<Self> {
        let _span = tracing::info_span!("open_geotiff", path = %path.display()).entered();
        Self::from_reader(BufReader::new(File::open(path)?))
    }
}

impl<R: Read + Seek> GeoTiff<R> {
    pub fn from_reader(reader: R) -> Result<Self> {
        let mut decoder = Decoder::new(reader)?;
        let (width, height) = decoder.dimensions()?;
        let (width, height) = (width as usize, height as usize);

        let planar = decoder
            .find_tag(Tag::PlanarConfiguration)?
            .map(|v| v.into_u16())
            .transpose()?;
        if planar.is_some_and(|p| p != PlanarConfiguration::Chunky.to_u16()) {
            return Err(Error::Unsupported(
                "planar (band-separate) storage; re-encode as pixel-interleaved".into(),
            ));
        }

        if let Some(formats) = decoder.find_tag(Tag::SampleFormat)? {
            let uint = SampleFormat::Uint.to_u16();
            if formats.into_u16_vec()?.iter().any(|&f| f != uint) {
                return Err(Error::Unsupported(
                    "signed or floating-point samples; scale the input to 8-bit unsigned first"
                        .into(),
                ));
            }
        }

        let photometric = decoder
            .find_tag(Tag::PhotometricInterpretation)?
            .map(|v| v.into_u16())
            .transpose()?;
        let bands = match (decoder.colortype()?, photometric) {
            (ColorType::Gray(8), Some(p))
                if p == PhotometricInterpretation::WhiteIsZero.to_u16() =>
            {
                return Err(Error::Unsupported(
                    "WhiteIsZero grayscale; invert it to BlackIsZero first".into(),
                ));
            }
            (ColorType::Gray(8), _) => 1,
            (ColorType::GrayA(8), _) => 2,
            // The decoder reports gray + one extra sample as multiband.
            (
                ColorType::Multiband {
                    bit_depth: 8,
                    num_samples: 2,
                },
                Some(p),
            ) if p == PhotometricInterpretation::BlackIsZero.to_u16() => 2,
            (ColorType::RGB(8), _) => 3,
            (ColorType::RGBA(8), _) => 4,
            (other, _) => {
                return Err(Error::Unsupported(format!(
                    "{other:?} pixels; convert the input to 8-bit gray, gray+alpha, RGB or RGBA first"
                )));
            }
        };

        let keys = geo_keys(&mut decoder)?;
        let pixel_is_point = key(&keys, GT_RASTER_TYPE) == Some(RASTER_PIXEL_IS_POINT);
        let transform = geo_transform(&mut decoder, pixel_is_point)?;
        let epsg = epsg_of(&keys);

        if decoder.find_tag(Tag::GdalNodata)?.is_some() {
            warn!(
                "input declares a nodata value; it is not turned into transparency — \
                 add an alpha band before tiling if that is wanted"
            );
        }

        let chunk_type = decoder.get_chunk_type();
        let (chunk_w, chunk_h) = decoder.chunk_dimensions();
        let info = RasterInfo {
            width,
            height,
            bands,
            transform,
            epsg,
        };
        info.validate()?;
        debug!(?info, ?chunk_type, chunk_w, chunk_h, "geotiff opened");

        Ok(Self {
            decoder,
            info,
            chunk_type,
            chunk_w: chunk_w.max(1) as usize,
            chunk_h: chunk_h.max(1) as usize,
        })
    }

    fn read_u8_chunk(&mut self, index: usize) -> Result<(Vec<u8>, usize, usize)> {
        let (w, h) = self.decoder.chunk_data_dimensions(index as u32);
        match self.decoder.read_chunk(index as u32)? {
            DecodingResult::U8(buf) => Ok((buf, w as usize, h as usize)),
            other => Err(Error::Unsupported(format!(
                "expected 8-bit samples, decoder produced {}",
                decoding_kind(&other)
            ))),
        }
    }
}

impl<R: Read + Seek> RasterSource for GeoTiff<R> {
    fn info(&self) -> &RasterInfo {
        &self.info
    }

    fn read_rows(&mut self, row_start: usize, rows: usize) -> Result<RowBlock> {
        let height = self.info.height;
        let start = row_start.min(height);
        let end = row_start.saturating_add(rows).min(height);
        let bands = self.info.bands;
        let stride = self.info.row_bytes();
        let mut data = vec![0u8; (end - start) * stride];
        debug!(start, end, "read_rows");

        if start < end {
            let first = start / self.chunk_h;
            let last = (end - 1) / self.chunk_h;
            let across = match self.chunk_type {
                ChunkType::Strip => 1,
                ChunkType::Tile => self.info.width.div_ceil(self.chunk_w),
            };
            for chunk_row in first..=last {
                for chunk_col in 0..across {
                    let (buf, w, h) = self.read_u8_chunk(chunk_row * across + chunk_col)?;
                    let chunk_y = chunk_row * self.chunk_h;
                    let col0 = chunk_col * self.chunk_w;
                    let w = w.min(self.info.width - col0);
                    let src_stride = w * bands;
                    for r in start.max(chunk_y)..end.min(chunk_y + h) {
                        let src = (r - chunk_y) * src_stride;
                        let dst = (r - start) * stride + col0 * bands;
                        data[dst..dst + src_stride].copy_from_slice(&buf[src..src + src_stride]);
                    }
                }
            }
        }

        Ok(RowBlock {
            data,
            width: self.info.width,
            bands,
            row_start: start,
            rows: end - start,
        })
    }
}

fn decoding_kind(r: &DecodingResult) -> &'static str {
    match r {
        DecodingResult::U8(_) => "u8",
        DecodingResult::U16(_) => "u16",
        DecodingResult::U32(_) => "u32",
        DecodingResult::U64(_) => "u64",
        DecodingResult::I8(_) => "i8",
        DecodingResult::I16(_) => "i16",
        DecodingResult::I32(_) => "i32",
        DecodingResult::I64(_) => "i64",
        DecodingResult::F16(_) => "f16",
        DecodingResult::F32(_) => "f32",
        DecodingResult::F64(_) => "f64",
    }
}

/// `(key id, value)` pairs from `GeoKeyDirectoryTag` whose value is stored inline.
/// Keys stored in other tags (doubles, ASCII citations) are not needed here.
fn geo_keys<R: Read + Seek>(decoder: &mut Decoder<R>) -> Result<Vec<(u16, u16)>> {
    let Some(dir) = decoder.find_tag(Tag::GeoKeyDirectoryTag)? else {
        return Ok(Vec::new());
    };
    let dir = dir.into_u16_vec()?;
    let count = dir.get(3).copied().unwrap_or(0) as usize;
    Ok(dir
        .get(4..)
        .unwrap_or_default()
        .as_chunks::<4>()
        .0
        .iter()
        .take(count)
        .filter(|entry| entry[1] == 0)
        .map(|entry| (entry[0], entry[3]))
        .collect())
}

fn key(keys: &[(u16, u16)], id: u16) -> Option<u16> {
    keys.iter().find(|(k, _)| *k == id).map(|(_, v)| *v)
}

fn epsg_of(keys: &[(u16, u16)]) -> Option<u32> {
    let code = match key(keys, GT_MODEL_TYPE) {
        Some(MODEL_TYPE_PROJECTED) => key(keys, PROJECTED_CS_TYPE),
        Some(MODEL_TYPE_GEOGRAPHIC) => key(keys, GEOGRAPHIC_TYPE),
        _ => key(keys, PROJECTED_CS_TYPE).or_else(|| key(keys, GEOGRAPHIC_TYPE)),
    }?;
    (code != USER_DEFINED && code != 0).then_some(code as u32)
}

/// Raster→model transform from `ModelTransformationTag`, or a single tie point plus
/// `ModelPixelScaleTag`. With `PixelIsPoint` the tie point names a pixel centre, so the
/// corner origin moves half a pixel up-left (as GDAL does).
///
/// Ref: <https://docs.ogc.org/is/19-008r4/19-008r4.html#_raster_to_model_coordinate_transformation_requirements>
fn geo_transform<R: Read + Seek>(
    decoder: &mut Decoder<R>,
    pixel_is_point: bool,
) -> Result<GeoTransform> {
    let affine = if let Some(matrix) = decoder.find_tag(Tag::ModelTransformationTag)? {
        let m: [f64; 16] = matrix
            .into_f64_vec()?
            .try_into()
            .map_err(|_| Error::Unsupported("ModelTransformationTag must have 16 values".into()))?;
        // model_x = col * m[0] + row * m[1] + m[3]; model_y = col * m[4] + row * m[5] + m[7]
        [m[3], m[0], m[1], m[7], m[4], m[5]]
    } else {
        let scale = decoder
            .find_tag(Tag::ModelPixelScaleTag)?
            .map(|v| v.into_f64_vec())
            .transpose()?;
        let ties = decoder
            .find_tag(Tag::ModelTiepointTag)?
            .map(|v| v.into_f64_vec())
            .transpose()?;
        let (Some(scale), Some(ties)) = (scale, ties) else {
            return Err(Error::MissingGeoreferencing);
        };
        if scale.len() < 2 || ties.len() < 6 {
            return Err(Error::Unsupported(
                "ModelPixelScaleTag needs 3 values and ModelTiepointTag at least 6".into(),
            ));
        }
        if ties.len() > 6 {
            return Err(Error::Unsupported(
                "multiple tie points (GCP georeferencing); warp the input to a regular grid first"
                    .into(),
            ));
        }
        let (px, py) = (scale[0], -scale[1]);
        [
            ties[3] - ties[0] * px,
            px,
            0.0,
            ties[4] - ties[1] * py,
            0.0,
            py,
        ]
    };

    let mut gt = affine;
    if pixel_is_point {
        gt[0] -= 0.5 * gt[1];
        gt[3] -= 0.5 * gt[5];
    }
    GeoTransform::from_affine(gt)
}

#[cfg(test)]
mod tests;

//! Georeferencing primitives: bounding boxes, the raster→model affine transform, and the
//! two coordinate reference systems tiles can be cut in.

use crate::Result;
use crate::error::Error;

/// Axis-aligned bounding box. Units depend on the CRS: degrees for EPSG:4326, metres for
/// EPSG:3857.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl Bounds {
    /// The overlapping part of `self` and `other`, or `None` when they only touch or are
    /// disjoint.
    pub fn intersection(&self, other: &Bounds) -> Option<Bounds> {
        let b = Bounds {
            min_x: self.min_x.max(other.min_x),
            min_y: self.min_y.max(other.min_y),
            max_x: self.max_x.min(other.max_x),
            max_y: self.max_y.min(other.max_y),
        };
        (b.min_x < b.max_x && b.min_y < b.max_y).then_some(b)
    }

    pub fn width(&self) -> f64 {
        self.max_x - self.min_x
    }

    pub fn height(&self) -> f64 {
        self.max_y - self.min_y
    }
}

/// A north-up raster→model transform: pixel corner `(col, row)` maps to
/// `(origin_x + col * pixel_width, origin_y + row * pixel_height)`.
///
/// Rotated/sheared and south-up rasters are rejected at construction: tiling only takes
/// rasters that are already axis-aligned in the target CRS.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeoTransform {
    origin_x: f64,
    origin_y: f64,
    pixel_width: f64,
    pixel_height: f64,
}

impl GeoTransform {
    /// `pixel_height` is negative for a north-up raster (rows go south).
    pub fn new(origin_x: f64, origin_y: f64, pixel_width: f64, pixel_height: f64) -> Result<Self> {
        let all_finite = [origin_x, origin_y, pixel_width, pixel_height]
            .iter()
            .all(|v| v.is_finite());
        if !all_finite || pixel_width <= 0.0 || pixel_height == 0.0 {
            return Err(Error::Unsupported(format!(
                "degenerate geotransform (origin {origin_x}, {origin_y}; pixel {pixel_width} x {pixel_height})"
            )));
        }
        if pixel_height > 0.0 {
            return Err(Error::Unsupported(
                "south-up raster (positive pixel height); flip it to north-up first".into(),
            ));
        }
        Ok(Self {
            origin_x,
            origin_y,
            pixel_width,
            pixel_height,
        })
    }

    /// Build from the GDAL-ordered 6-term affine `[x0, px_w, rot_x, y0, rot_y, px_h]`.
    pub fn from_affine(gt: [f64; 6]) -> Result<Self> {
        if gt[2] != 0.0 || gt[4] != 0.0 {
            return Err(Error::Unsupported(
                "rotated/sheared raster; warp it to a north-up grid first".into(),
            ));
        }
        Self::new(gt[0], gt[3], gt[1], gt[5])
    }

    pub fn origin(&self) -> (f64, f64) {
        (self.origin_x, self.origin_y)
    }

    /// `(pixel_width, pixel_height)`; height is negative.
    pub fn pixel_size(&self) -> (f64, f64) {
        (self.pixel_width, self.pixel_height)
    }

    /// Model-space extent of a `width × height` raster.
    pub fn bounds(&self, width: usize, height: usize) -> Bounds {
        Bounds {
            min_x: self.origin_x,
            max_x: self.origin_x + width as f64 * self.pixel_width,
            max_y: self.origin_y,
            min_y: self.origin_y + height as f64 * self.pixel_height,
        }
    }

    /// Fractional pixel coordinates `(col, row)` of a model-space point.
    pub fn to_pixel(&self, x: f64, y: f64) -> (f64, f64) {
        (
            (x - self.origin_x) / self.pixel_width,
            (y - self.origin_y) / self.pixel_height,
        )
    }
}

/// Coordinate reference system of a tile grid (and the raster it is cut from).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Crs {
    /// EPSG:4326 — WGS 84 longitude/latitude, degrees.
    Wgs84,
    /// EPSG:3857 — Web Mercator, metres.
    WebMercator,
}

impl Crs {
    pub fn epsg(self) -> u32 {
        match self {
            Self::Wgs84 => 4326,
            Self::WebMercator => 3857,
        }
    }

    /// Map an EPSG code to a supported CRS, including the deprecated aliases of 3857
    /// (3785, 900913, 102100, 102113) that older GeoTIFFs still carry.
    pub fn from_epsg(code: u32) -> Option<Self> {
        match code {
            4326 => Some(Self::Wgs84),
            3857 | 3785 | 900913 | 102100 | 102113 => Some(Self::WebMercator),
            _ => None,
        }
    }
}

impl std::fmt::Display for Crs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EPSG:{}", self.epsg())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn affine_rejects_rotation_and_south_up() {
        assert!(GeoTransform::from_affine([0.0, 1.0, 0.1, 0.0, 0.0, -1.0]).is_err());
        assert!(GeoTransform::from_affine([0.0, 1.0, 0.0, 0.0, 0.0, 1.0]).is_err());
        assert!(GeoTransform::from_affine([0.0, 0.0, 0.0, 0.0, 0.0, -1.0]).is_err());
        assert!(GeoTransform::from_affine([f64::NAN, 1.0, 0.0, 0.0, 0.0, -1.0]).is_err());
    }

    #[test]
    fn bounds_and_pixel_roundtrip() {
        let gt = GeoTransform::new(10.0, 50.0, 0.5, -0.25).unwrap();
        let b = gt.bounds(4, 8);
        assert_eq!(
            b,
            Bounds {
                min_x: 10.0,
                min_y: 48.0,
                max_x: 12.0,
                max_y: 50.0
            }
        );
        assert_eq!(gt.to_pixel(11.0, 49.0), (2.0, 4.0));
    }

    #[test]
    fn intersection_requires_positive_area() {
        let a = Bounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        };
        let touching = Bounds {
            min_x: 1.0,
            min_y: 0.0,
            max_x: 2.0,
            max_y: 1.0,
        };
        assert!(a.intersection(&touching).is_none());
        let inner = Bounds {
            min_x: 0.5,
            min_y: -1.0,
            max_x: 3.0,
            max_y: 0.5,
        };
        let i = a.intersection(&inner).unwrap();
        assert_eq!((i.min_x, i.min_y, i.max_x, i.max_y), (0.5, 0.0, 1.0, 0.5));
    }

    #[test]
    fn epsg_aliases() {
        assert_eq!(Crs::from_epsg(4326), Some(Crs::Wgs84));
        assert_eq!(Crs::from_epsg(3785), Some(Crs::WebMercator));
        assert_eq!(Crs::from_epsg(3395), None);
    }
}

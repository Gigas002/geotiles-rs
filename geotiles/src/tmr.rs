//! TileMapResource XML writer.
//!
//! Generates a `tilemapresource.xml` compatible with the OGC TMS 1.0.0 spec
//! (as used by MapTiler, gdal2tiles, and similar tools).

use std::path::Path;

use libgeotiles::coords::Bounds;

use crate::run::{Crs, Params};

/// Write `tilemapresource.xml` into `output_dir`.
pub fn write(output_dir: &Path, params: &Params, ds_bounds: Bounds) -> anyhow::Result<()> {
    let xml = render(params, ds_bounds);
    let path = output_dir.join("tilemapresource.xml");
    std::fs::write(&path, xml)?;
    tracing::info!(path = %path.display(), "tilemapresource.xml written");
    Ok(())
}

/// Render the XML document as a `String`.
fn render(params: &Params, bounds: Bounds) -> String {
    let (srs, profile, origin_x, origin_y) = match params.crs {
        Crs::Geographic => ("EPSG:4326", "global-geodetic", -180.0_f64, -90.0_f64),
        Crs::Mercator => (
            "EPSG:3857",
            "global-mercator",
            -20_037_508.342_789_244_f64,
            -20_037_508.342_789_244_f64,
        ),
    };

    let mime = mime_type(params.format);
    let ext = params.format.extension();
    let ts = params.tile_size;

    let mut tile_sets = String::new();
    for z in params.min_zoom..=params.max_zoom {
        let units_per_pixel = units_per_pixel(params.crs, z, params.tile_size);
        tile_sets.push_str(&format!(
            "    <TileSet href=\"{z}\" units-per-pixel=\"{units_per_pixel:.10}\" order=\"{z}\"/>\n"
        ));
    }

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<TileMap version="1.0.0" tilemapservice="http://tms.osgeo.org/1.0.0">
  <Title>Tiles</Title>
  <SRS>{srs}</SRS>
  <BoundingBox minx="{minx}" miny="{miny}" maxx="{maxx}" maxy="{maxy}"/>
  <Origin x="{origin_x}" y="{origin_y}"/>
  <TileFormat width="{ts}" height="{ts}" mime-type="{mime}" extension="{ext}"/>
  <TileSets profile="{profile}">
{tile_sets}  </TileSets>
</TileMap>
"#,
        minx = bounds.min_x,
        miny = bounds.min_y,
        maxx = bounds.max_x,
        maxy = bounds.max_y,
    )
}

/// Half the Web Mercator equatorial extent in metres (π × 6 378 137).
///
/// Defined locally so `tmr` does not depend on the `mercator` Cargo feature.
const MERC_ORIGIN_SHIFT: f64 = std::f64::consts::PI * 6_378_137.0;

/// Ground resolution in the native CRS units per pixel at zoom `z`.
fn units_per_pixel(crs: Crs, z: u8, tile_size: u32) -> f64 {
    let full_extent = match crs {
        // 180° / (tile_size × 2^z) for the geodetic profile (2× wider grid).
        Crs::Geographic => 180.0_f64,
        // 2 × half-circumference for Web Mercator.
        Crs::Mercator => MERC_ORIGIN_SHIFT * 2.0,
    };
    full_extent / (tile_size as f64 * (1u64 << z) as f64)
}

fn mime_type(format: libgeotiles::Format) -> &'static str {
    use libgeotiles::Format;
    match format {
        Format::Png => "image/png",
        Format::Jpeg => "image/jpeg",
        Format::WebP => "image/webp",
        Format::Avif => "image/avif",
        Format::Jxl => "image/jxl",
    }
}

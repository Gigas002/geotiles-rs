//! Wires CLI settings to `libgeotiles`: GeoTIFF in, tile directory out.

use libgeotiles::{
    CpuResampler, Crs, DirSink, GeoTiff, Geographic, RasterSource, Resampler, TileGrid,
    TilerOptions, WebMercator, generate,
};
use tracing::{info, info_span};

use crate::settings::{Backend, Settings};

/// Run the tiler for `settings`. The input is used exactly as it is on disk; see
/// [`libgeotiles`] for what it must look like.
pub fn run(settings: &Settings) -> anyhow::Result<()> {
    let _span = info_span!("run", input = %settings.input.display()).entered();

    let mut source = GeoTiff::open(&settings.input)?;
    let info = source.info().clone();
    let crs = resolve_crs(settings.crs, info.epsg)?;
    let bounds = info.bounds();
    info!(
        width = info.width,
        height = info.height,
        bands = info.bands,
        epsg = info.epsg,
        %crs,
        ?bounds,
        "dataset ready"
    );

    let grid: Box<dyn TileGrid> = match crs {
        Crs::Wgs84 => Box::new(Geographic::new(settings.tile_size)),
        Crs::WebMercator => Box::new(WebMercator::new(settings.tile_size)),
    };
    let resampler = resampler(settings.backend)?;
    std::fs::create_dir_all(&settings.output)?;
    let sink = DirSink::new(&settings.output, settings.tms);
    let opts = TilerOptions {
        zooms: settings.min_zoom..=settings.max_zoom,
        format: settings.format,
        encode: settings.encode_opts.clone(),
        bands: settings.bands,
        chunk_rows: settings.chunk_size,
    };

    let stats = generate(&mut source, grid.as_ref(), resampler.as_ref(), &sink, &opts)?;
    info!(tiles = stats.tiles, "tiles written");

    if settings.tmr {
        crate::tmr::write(&settings.output, settings, crs, bounds)?;
    }
    Ok(())
}

/// The requested CRS, else the raster's own. A request that contradicts the raster is
/// caught by the tiler.
pub(crate) fn resolve_crs(requested: Option<Crs>, epsg: Option<u32>) -> anyhow::Result<Crs> {
    if let Some(crs) = requested {
        return Ok(crs);
    }
    match epsg {
        Some(code) => Crs::from_epsg(code).ok_or_else(|| {
            anyhow::anyhow!(
                "raster is in EPSG:{code}; tiles need EPSG:4326 or EPSG:3857, reproject it first"
            )
        }),
        None => anyhow::bail!("raster declares no CRS; pass --crs geographic|mercator"),
    }
}

fn resampler(backend: Backend) -> anyhow::Result<Box<dyn Resampler>> {
    match backend {
        Backend::Cpu => Ok(Box::new(CpuResampler)),
        #[cfg(feature = "gpu")]
        Backend::Gpu => {
            info!("initialising GPU backend");
            Ok(Box::new(libgeotiles::GpuResampler::new()?))
        }
    }
}

#[cfg(test)]
mod tests;

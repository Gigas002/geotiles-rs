mod cli;
mod config;
mod run;
mod tmr;

use clap::Parser;
use tracing::info;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = cli::Cli::parse();

    // Load config: explicit --config path, or the XDG default.
    let cfg_path = cli.config.clone().or_else(config::default_config_path);
    let config = match cfg_path {
        Some(ref p) => {
            info!(path = %p.display(), "loading config");
            config::load(p)?
        }
        None => {
            info!("no config file found; using defaults");
            config::Config::default()
        }
    };

    let zoom = cli::ZoomRange::parse(&cli.zoom)?;

    let params = run::Params::resolve(
        cli.input,
        cli.output,
        zoom.min,
        zoom.max,
        cli.extension,
        cli.tms,
        cli.crs,
        cli.bands,
        cli.tilesize,
        cli.tmr,
        cli.chunk_size,
        &config,
    )?;

    info!(
        input = %params.input.display(),
        output = %params.output.display(),
        min_zoom = params.min_zoom,
        max_zoom = params.max_zoom,
        format = ?params.format,
        tms = params.tms,
        crs = ?params.crs,
        tile_size = params.tile_size,
        tmr = params.tmr,
        chunk_size = params.chunk_size,
        "starting"
    );

    run::run(&params)?;

    info!("done");
    Ok(())
}

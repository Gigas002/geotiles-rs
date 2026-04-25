mod cli;
mod config;
mod logger;
mod run;
mod settings;
mod tmr;

use clap::Parser;
use tracing::info;

fn main() -> anyhow::Result<()> {
    logger::init();

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

    // Resolve all settings from CLI + config.  Past this point `cli` and `config`
    // must not be referenced again — only `settings` is passed downstream.
    let settings = settings::Settings::resolve(&cli, &config)?;
    drop(cli);
    drop(config);

    info!(
        input = %settings.input.display(),
        output = %settings.output.display(),
        min_zoom = settings.min_zoom,
        max_zoom = settings.max_zoom,
        format = ?settings.format,
        tms = settings.tms,
        crs = ?settings.crs,
        tile_size = settings.tile_size,
        tmr = settings.tmr,
        chunk_size = settings.chunk_size,
        "starting"
    );

    run::run(&settings)?;

    info!("done");
    Ok(())
}

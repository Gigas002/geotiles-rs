use std::path::PathBuf;

use libgeotiles::Format;

use super::*;

fn minimal_cli() -> crate::cli::Cli {
    crate::cli::Cli {
        input: PathBuf::from("input.tif"),
        output: PathBuf::from("output"),
        zoom: String::from("0..5"),
        extension: None,
        tms: None,
        crs: None,
        bands: None,
        tilesize: None,
        tmr: None,
        chunk_size: None,
        config: None,
    }
}

#[test]
fn defaults_apply_when_nothing_set() {
    let cli = minimal_cli();
    let config = crate::config::Config::default();
    let s = Settings::resolve(&cli, &config).unwrap();
    assert_eq!(s.min_zoom, 0);
    assert_eq!(s.max_zoom, 5);
    assert_eq!(s.format, Format::Png);
    assert!(!s.tms);
    assert_eq!(s.crs, Crs::Geographic);
    assert_eq!(s.tile_size, 256);
    assert!(!s.tmr);
    assert_eq!(s.chunk_size, 512);
}

#[test]
fn cli_overrides_config() {
    let mut cli = minimal_cli();
    cli.extension = Some(String::from("jpg"));
    cli.tms = Some(true);
    cli.tilesize = Some(512);
    let config = crate::config::Config {
        extension: Some(String::from("png")),
        tms: Some(false),
        ..Default::default()
    };
    let s = Settings::resolve(&cli, &config).unwrap();
    assert_eq!(s.format, Format::Jpeg);
    assert!(s.tms);
    assert_eq!(s.tile_size, 512);
}

#[test]
fn config_fills_absent_cli() {
    let cli = minimal_cli();
    let config = crate::config::Config {
        extension: Some(String::from("webp")),
        chunk_size: Some(1024),
        ..Default::default()
    };
    let s = Settings::resolve(&cli, &config).unwrap();
    assert_eq!(s.format, Format::WebP);
    assert_eq!(s.chunk_size, 1024);
}

#[test]
fn parse_crs_all_aliases() {
    assert_eq!(super::parse_crs("geographic").unwrap(), Crs::Geographic);
    assert_eq!(super::parse_crs("EPSG:4326").unwrap(), Crs::Geographic);
    assert_eq!(super::parse_crs("4326").unwrap(), Crs::Geographic);
    assert_eq!(super::parse_crs("mercator").unwrap(), Crs::Mercator);
    assert_eq!(super::parse_crs("EPSG:3857").unwrap(), Crs::Mercator);
    assert_eq!(super::parse_crs("3857").unwrap(), Crs::Mercator);
    assert!(super::parse_crs("unknown").is_err());
}

#[test]
fn parse_format_all_variants() {
    assert_eq!(super::parse_format("png").unwrap(), Format::Png);
    assert_eq!(super::parse_format(".jpg").unwrap(), Format::Jpeg);
    assert_eq!(super::parse_format("JPEG").unwrap(), Format::Jpeg);
    assert_eq!(super::parse_format("webp").unwrap(), Format::WebP);
    assert_eq!(super::parse_format("avif").unwrap(), Format::Avif);
    assert_eq!(super::parse_format("jxl").unwrap(), Format::Jxl);
    assert!(super::parse_format("tif").is_err());
}

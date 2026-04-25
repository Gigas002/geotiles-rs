use std::path::Path;

use super::*;

#[test]
fn default_is_all_none() {
    let cfg = Config::default();
    assert!(cfg.extension.is_none());
    assert!(cfg.tms.is_none());
    assert!(cfg.crs.is_none());
    assert!(cfg.bands.is_none());
    assert!(cfg.tilesize.is_none());
    assert!(cfg.tmr.is_none());
    assert!(cfg.chunk_size.is_none());
}

#[test]
fn load_nonexistent_returns_default() {
    let cfg = load(Path::new("/tmp/geotiles_test_nonexistent_xyz987.toml")).unwrap();
    assert!(cfg.extension.is_none());
    assert!(cfg.tms.is_none());
}

#[test]
fn load_valid_toml() {
    let tmp_path = std::env::temp_dir().join("geotiles_test_config_abc123.toml");
    let toml_str = "extension = \"jpg\"\ntms = true\ntilesize = 512\n";
    std::fs::write(&tmp_path, toml_str).unwrap();
    let cfg = load(&tmp_path).unwrap();
    assert_eq!(cfg.extension.as_deref(), Some("jpg"));
    assert_eq!(cfg.tms, Some(true));
    assert_eq!(cfg.tilesize, Some(512));
    std::fs::remove_file(&tmp_path).ok();
}

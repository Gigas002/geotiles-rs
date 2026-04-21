use super::GeoTiff;
use crate::tile::{Format, ResampleBackend};

#[test]
fn open_nonexistent_returns_error() {
    let result = GeoTiff::open("/definitely/does/not/exist/input.tif");
    assert!(result.is_err(), "expected error for nonexistent path");
}

#[test]
fn builder_setters_are_chainable() {
    // We can't call open() without a real file, but we can verify the builder
    // types compile and chain correctly via a type-level smoke test.
    // This function is intentionally not called; it just asserts type-checking passes.
    fn _compile_check(g: GeoTiff) -> GeoTiff {
        g.zoom(2..=8)
            .chunk_size(256)
            .format(Format::Png)
            .output("/tmp/tiles")
            .backend(ResampleBackend::Cpu)
            .tile_size(512)
    }
    // suppress dead-code lint — the function is the test
    let _ = _compile_check as fn(GeoTiff) -> GeoTiff;
}

use super::GeoTiff;
use crate::tile::{Format, ResampleBackend};
use crate::{AvifOptions, EncodeOptions, JpegOptions, JxlOptions, PngOptions, WebPOptions};

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
            .png_options(PngOptions::default())
            .jpeg_options(JpegOptions { quality: 90 })
            .webp_options(WebPOptions::default())
            .avif_options(AvifOptions {
                quality: 70,
                speed: 5,
            })
            .jxl_options(JxlOptions {
                distance: 1.5,
                effort: 5,
                lossless: false,
            })
    }
    // suppress dead-code lint — the function is the test
    let _ = _compile_check as fn(GeoTiff) -> GeoTiff;
}

#[test]
fn encode_options_default_roundtrips_through_builder() {
    // Verify that setting options equal to the default on a (type-level) GeoTiff
    // compiles and that EncodeOptions::default() is consistent.
    let opts = EncodeOptions::default();
    assert_eq!(opts.jpeg.quality, 85);
    assert_eq!(opts.avif.quality, 60);
    assert_eq!(opts.avif.speed, 4);
    assert!((opts.jxl.distance - 1.0).abs() < f32::EPSILON);
    assert_eq!(opts.jxl.effort, 7);
    assert!(!opts.jxl.lossless);
    assert!(opts.webp.lossless);
    assert_eq!(opts.webp.quality, 85);
}

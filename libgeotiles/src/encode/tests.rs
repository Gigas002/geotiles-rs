use super::{encode_tile, options::EncodeOptions};
use crate::tile::Format;

// ── PNG ───────────────────────────────────────────────────────────────────────

#[cfg(feature = "png")]
#[test]
fn png_grayscale_has_valid_header() {
    let pixels = vec![128u8; 4 * 4];
    let out = encode_tile(&pixels, 4, 4, 1, Format::Png, &EncodeOptions::default()).unwrap();
    assert!(out.starts_with(b"\x89PNG"), "expected PNG magic bytes");
    assert!(!out.is_empty());
}

#[cfg(feature = "png")]
#[test]
fn png_rgb_has_valid_header() {
    let pixels = vec![255u8; 4 * 4 * 3];
    let out = encode_tile(&pixels, 4, 4, 3, Format::Png, &EncodeOptions::default()).unwrap();
    assert!(out.starts_with(b"\x89PNG"), "expected PNG magic bytes");
}

#[cfg(feature = "png")]
#[test]
fn png_rgba_has_valid_header() {
    let pixels = vec![200u8; 8 * 8 * 4];
    let out = encode_tile(&pixels, 8, 8, 4, Format::Png, &EncodeOptions::default()).unwrap();
    assert!(out.starts_with(b"\x89PNG"));
}

#[cfg(feature = "png")]
#[test]
fn png_fast_compression_produces_valid_output() {
    use super::options::{PngCompression, PngOptions};
    let pixels = vec![42u8; 4 * 4 * 3];
    let opts = EncodeOptions {
        png: PngOptions {
            compression: PngCompression::Fast,
            ..Default::default()
        },
        ..Default::default()
    };
    let out = encode_tile(&pixels, 4, 4, 3, Format::Png, &opts).unwrap();
    assert!(out.starts_with(b"\x89PNG"));
}

#[cfg(feature = "png")]
#[test]
fn png_best_compression_produces_valid_output() {
    use super::options::{PngCompression, PngOptions};
    let pixels = vec![0u8; 16 * 16 * 3];
    let opts = EncodeOptions {
        png: PngOptions {
            compression: PngCompression::Best,
            ..Default::default()
        },
        ..Default::default()
    };
    let out = encode_tile(&pixels, 16, 16, 3, Format::Png, &opts).unwrap();
    assert!(out.starts_with(b"\x89PNG"));
}

// ── JPEG ──────────────────────────────────────────────────────────────────────

#[cfg(feature = "jpeg")]
#[test]
fn jpeg_rgb_has_valid_header() {
    let pixels = vec![128u8; 8 * 8 * 3];
    let out = encode_tile(&pixels, 8, 8, 3, Format::Jpeg, &EncodeOptions::default()).unwrap();
    // JPEG magic bytes: FF D8 FF
    assert!(
        out.starts_with(&[0xFF, 0xD8, 0xFF]),
        "expected JPEG SOI marker"
    );
}

#[cfg(feature = "jpeg")]
#[test]
fn jpeg_rgba_alpha_is_stripped() {
    // 4-band RGBA input must produce a valid JPEG (alpha silently dropped).
    let pixels = vec![200u8; 8 * 8 * 4];
    let out = encode_tile(&pixels, 8, 8, 4, Format::Jpeg, &EncodeOptions::default()).unwrap();
    assert!(
        out.starts_with(&[0xFF, 0xD8, 0xFF]),
        "expected JPEG SOI marker after alpha strip"
    );
}

#[cfg(feature = "jpeg")]
#[test]
fn jpeg_quality_setting_affects_output_size() {
    use super::options::JpegOptions;
    let pixels: Vec<u8> = (0..256u32 * 256 * 3).map(|i| (i % 251) as u8).collect();

    let opts_low = EncodeOptions {
        jpeg: JpegOptions { quality: 10 },
        ..Default::default()
    };
    let opts_high = EncodeOptions {
        jpeg: JpegOptions { quality: 95 },
        ..Default::default()
    };

    let low = encode_tile(&pixels, 256, 256, 3, Format::Jpeg, &opts_low).unwrap();
    let high = encode_tile(&pixels, 256, 256, 3, Format::Jpeg, &opts_high).unwrap();

    // Higher quality should yield a larger file for non-trivial content.
    assert!(
        high.len() > low.len(),
        "expected quality=95 output ({} bytes) to be larger than quality=10 ({} bytes)",
        high.len(),
        low.len()
    );
}

// ── WebP ──────────────────────────────────────────────────────────────────────

#[cfg(feature = "webp")]
#[test]
fn webp_rgb_has_valid_header() {
    let pixels = vec![100u8; 8 * 8 * 3];
    let out = encode_tile(&pixels, 8, 8, 3, Format::WebP, &EncodeOptions::default()).unwrap();
    // WebP files start with "RIFF????WEBP"
    assert_eq!(&out[0..4], b"RIFF", "expected RIFF header");
    assert_eq!(&out[8..12], b"WEBP", "expected WEBP marker");
}

#[cfg(feature = "webp")]
#[test]
fn webp_rgba_has_valid_header() {
    let pixels = vec![50u8; 8 * 8 * 4];
    let out = encode_tile(&pixels, 8, 8, 4, Format::WebP, &EncodeOptions::default()).unwrap();
    assert_eq!(&out[0..4], b"RIFF");
    assert_eq!(&out[8..12], b"WEBP");
}

// ── AVIF ──────────────────────────────────────────────────────────────────────

#[cfg(feature = "avif")]
#[test]
fn avif_rgb_produces_non_empty_output() {
    let pixels = vec![128u8; 16 * 16 * 3];
    let out = encode_tile(&pixels, 16, 16, 3, Format::Avif, &EncodeOptions::default()).unwrap();
    assert!(!out.is_empty(), "expected non-empty AVIF output");
    // AVIF files are ISOBMFF containers; the ftyp box starts at byte 4 with "ftyp".
    assert_eq!(&out[4..8], b"ftyp", "expected ISO BMFF ftyp box");
}

#[cfg(feature = "avif")]
#[test]
fn avif_rgba_produces_non_empty_output() {
    let pixels = vec![200u8; 8 * 8 * 4];
    let out = encode_tile(&pixels, 8, 8, 4, Format::Avif, &EncodeOptions::default()).unwrap();
    assert!(!out.is_empty());
}

#[cfg(feature = "avif")]
#[test]
fn avif_quality_option_accepted() {
    use super::options::AvifOptions;
    let pixels = vec![64u8; 16 * 16 * 3];
    let opts = EncodeOptions {
        avif: AvifOptions {
            quality: 40,
            speed: 6,
        },
        ..Default::default()
    };
    let out = encode_tile(&pixels, 16, 16, 3, Format::Avif, &opts).unwrap();
    assert!(!out.is_empty());
}

// ── JPEG XL ───────────────────────────────────────────────────────────────────

#[cfg(feature = "jxl")]
#[test]
fn jxl_rgb_produces_non_empty_output() {
    let pixels = vec![100u8; 16 * 16 * 3];
    let out = encode_tile(&pixels, 16, 16, 3, Format::Jxl, &EncodeOptions::default()).unwrap();
    assert!(!out.is_empty(), "expected non-empty JXL output");
}

#[cfg(feature = "jxl")]
#[test]
fn jxl_rgba_with_alpha_produces_non_empty_output() {
    let pixels = vec![150u8; 8 * 8 * 4];
    let out = encode_tile(&pixels, 8, 8, 4, Format::Jxl, &EncodeOptions::default()).unwrap();
    assert!(!out.is_empty());
}

#[cfg(feature = "jxl")]
#[test]
fn jxl_grayscale_produces_non_empty_output() {
    let pixels = vec![200u8; 16 * 16];
    let out = encode_tile(&pixels, 16, 16, 1, Format::Jxl, &EncodeOptions::default()).unwrap();
    assert!(!out.is_empty());
}

#[cfg(feature = "jxl")]
#[test]
fn jxl_lossless_option_accepted() {
    use super::options::JxlOptions;
    let pixels = vec![42u8; 8 * 8 * 3];
    let opts = EncodeOptions {
        jxl: JxlOptions {
            lossless: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let out = encode_tile(&pixels, 8, 8, 3, Format::Jxl, &opts).unwrap();
    assert!(!out.is_empty());
}

#[cfg(feature = "jxl")]
#[test]
fn jxl_custom_distance_and_effort_accepted() {
    use super::options::JxlOptions;
    let pixels = vec![99u8; 8 * 8 * 3];
    let opts = EncodeOptions {
        jxl: JxlOptions {
            distance: 2.5,
            effort: 3,
            lossless: false,
        },
        ..Default::default()
    };
    let out = encode_tile(&pixels, 8, 8, 3, Format::Jxl, &opts).unwrap();
    assert!(!out.is_empty());
}

// ── Feature-not-compiled errors ───────────────────────────────────────────────

#[test]
fn bad_band_count_returns_error() {
    // 5-band images are not supported by any encoder.
    let pixels = vec![0u8; 4 * 4 * 5];
    let err = encode_tile(&pixels, 4, 4, 5, Format::Png, &EncodeOptions::default()).unwrap_err();
    assert!(
        matches!(err, crate::Error::BadBandCount(5)),
        "unexpected error: {err}"
    );
}

// ── La8 (grayscale + alpha, produced by nodata synthesis on 1-band datasets) ──

#[cfg(feature = "png")]
#[test]
fn png_la8_has_valid_header() {
    // 4×4 grayscale+alpha tile: 2 bytes per pixel
    let pixels: Vec<u8> = (0..4 * 4 * 2).map(|i| (i % 256) as u8).collect();
    let out = encode_tile(&pixels, 4, 4, 2, Format::Png, &EncodeOptions::default()).unwrap();
    assert!(out.starts_with(b"\x89PNG"), "expected PNG magic bytes");
}

#[cfg(feature = "jpeg")]
#[test]
fn jpeg_la8_strips_to_grayscale() {
    // La8 → JPEG should strip alpha and produce a valid grayscale JPEG.
    let pixels: Vec<u8> = (0..8 * 8 * 2).map(|i| (i % 256) as u8).collect();
    let out = encode_tile(&pixels, 8, 8, 2, Format::Jpeg, &EncodeOptions::default()).unwrap();
    assert!(
        out.starts_with(&[0xFF, 0xD8, 0xFF]),
        "expected JPEG SOI marker"
    );
}

#[cfg(feature = "webp")]
#[test]
fn webp_la8_expands_to_rgba() {
    // La8 → WebP: expand to RGBA internally, output should be valid WebP.
    let pixels: Vec<u8> = (0..8 * 8 * 2).map(|i| (i % 256) as u8).collect();
    let out = encode_tile(&pixels, 8, 8, 2, Format::WebP, &EncodeOptions::default()).unwrap();
    assert_eq!(&out[0..4], b"RIFF");
    assert_eq!(&out[8..12], b"WEBP");
}

#[cfg(feature = "avif")]
#[test]
fn avif_la8_expands_to_rgba() {
    let pixels: Vec<u8> = (0..16 * 16 * 2).map(|i| (i % 256) as u8).collect();
    let out = encode_tile(&pixels, 16, 16, 2, Format::Avif, &EncodeOptions::default()).unwrap();
    assert!(!out.is_empty());
    assert_eq!(&out[4..8], b"ftyp");
}

#[cfg(feature = "jxl")]
#[test]
fn jxl_la8_produces_non_empty_output() {
    let pixels: Vec<u8> = (0..8 * 8 * 2).map(|i| (i % 256) as u8).collect();
    let out = encode_tile(&pixels, 8, 8, 2, Format::Jxl, &EncodeOptions::default()).unwrap();
    assert!(!out.is_empty());
}

#[cfg(not(feature = "avif"))]
#[test]
fn avif_feature_off_returns_encode_error() {
    let pixels = vec![0u8; 4 * 4 * 3];
    let err = encode_tile(&pixels, 4, 4, 3, Format::Avif, &EncodeOptions::default()).unwrap_err();
    assert!(
        matches!(err, crate::Error::Encode(_)),
        "unexpected error: {err}"
    );
}

#[cfg(not(feature = "jxl"))]
#[test]
fn jxl_feature_off_returns_encode_error() {
    let pixels = vec![0u8; 4 * 4 * 3];
    let err = encode_tile(&pixels, 4, 4, 3, Format::Jxl, &EncodeOptions::default()).unwrap_err();
    assert!(
        matches!(err, crate::Error::Encode(_)),
        "unexpected error: {err}"
    );
}

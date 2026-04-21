use super::encode_tile;
use crate::tile::Format;

#[cfg(feature = "png")]
#[test]
fn png_grayscale_has_valid_header() {
    let pixels = vec![128u8; 4 * 4];
    let out = encode_tile(&pixels, 4, 4, 1, Format::Png).unwrap();
    assert!(out.starts_with(b"\x89PNG"), "expected PNG magic bytes");
    assert!(!out.is_empty());
}

#[cfg(feature = "png")]
#[test]
fn png_rgb_has_valid_header() {
    let pixels = vec![255u8; 4 * 4 * 3];
    let out = encode_tile(&pixels, 4, 4, 3, Format::Png).unwrap();
    assert!(out.starts_with(b"\x89PNG"), "expected PNG magic bytes");
}

#[test]
fn bad_band_count_returns_error() {
    // 2-band images are not supported.
    let pixels = vec![0u8; 4 * 4 * 2];
    let err = encode_tile(&pixels, 4, 4, 2, Format::Png).unwrap_err();
    assert!(
        matches!(err, crate::Error::BadBandCount(2)),
        "unexpected error: {err}"
    );
}

#[test]
fn unimplemented_format_returns_encode_error() {
    let pixels = vec![0u8; 4 * 4];
    let err = encode_tile(&pixels, 4, 4, 1, Format::Avif).unwrap_err();
    assert!(
        matches!(err, crate::Error::Encode(_)),
        "unexpected error: {err}"
    );
}

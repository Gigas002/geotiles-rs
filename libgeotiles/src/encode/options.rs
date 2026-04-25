//! Per-format encoder options passed to [`crate::encode::encode_tile`].
//!
//! Each option struct has a [`Default`] implementation with sensible values so callers
//! that do not customise encoding still get reasonable output.
//!
//! # Future CLI config
//! These types are intentionally flat so they map cleanly to per-section TOML config, e.g.:
//! ```toml
//! [png]
//! compression = "best"
//! filter = "adaptive"
//!
//! [jpeg]
//! quality = 85
//!
//! [jxl]
//! distance = 1.0
//! effort = 7
//! ```

// ── PNG ───────────────────────────────────────────────────────────────────────

/// PNG compression preset.
///
/// Maps to `image::codecs::png::CompressionType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PngCompression {
    /// Default libpng compression (roughly level 6).  Good balance of file size and speed.
    #[default]
    Default,
    /// Fast compression (roughly level 1).  Larger files, quicker to encode.
    Fast,
    /// Best (maximum) compression (roughly level 9).  Smallest files, slowest to encode.
    Best,
}

/// PNG pre-compression filter heuristic applied per scanline.
///
/// Maps to `image::codecs::png::FilterType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PngFilter {
    /// Let the encoder pick the best filter per scanline (default; usually smallest output).
    #[default]
    Adaptive,
    /// No filter.
    NoFilter,
    /// Sub filter.
    Sub,
    /// Up filter.
    Up,
    /// Average filter.
    Avg,
    /// Paeth filter.
    Paeth,
}

/// Options for PNG encoding.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PngOptions {
    /// Compression preset.  Default: [`PngCompression::Default`].
    pub compression: PngCompression,
    /// Filter heuristic.  Default: [`PngFilter::Adaptive`].
    pub filter: PngFilter,
}

// ── JPEG ──────────────────────────────────────────────────────────────────────

/// Options for JPEG encoding.
///
/// JPEG does not support an alpha channel.  When the source tile has 4 bands (RGBA) the
/// alpha channel is **stripped** (the RGB values are kept as-is, no compositing against a
/// background colour is performed) before encoding.  This is documented behaviour and
/// matches the intent of §1.5 in the migration plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JpegOptions {
    /// Quality factor (1–100, higher → better quality and larger file).
    ///
    /// Default: `85`.
    pub quality: u8,
}

impl Default for JpegOptions {
    fn default() -> Self {
        Self { quality: 85 }
    }
}

// ── WebP ──────────────────────────────────────────────────────────────────────

/// Options for WebP encoding.
///
/// The `image/webp` codec currently supports **lossless** WebP only.  The `quality` field
/// and `lossless = false` path are present for API stability; they will take effect once a
/// lossy codec is wired in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebPOptions {
    /// Use lossless encoding.
    ///
    /// Default: `true` (the only mode currently supported by the `image/webp` codec).
    pub lossless: bool,
    /// Quality for a future lossy path (0–100).  Currently unused.
    ///
    /// Default: `85`.
    pub quality: u8,
}

impl Default for WebPOptions {
    fn default() -> Self {
        Self {
            lossless: true,
            quality: 85,
        }
    }
}

// ── AVIF ──────────────────────────────────────────────────────────────────────

/// Options for AVIF encoding via `image/avif` (backed by `ravif` / `rav1e`, pure Rust).
///
/// No system libraries are required for this encoder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvifOptions {
    /// Encoder quality (1–100, higher → better quality and larger file).
    ///
    /// Default: `60`.
    pub quality: u8,
    /// Encoder speed (1–10, lower → slower encoding with better compression).
    ///
    /// Default: `4`.
    pub speed: u8,
}

impl Default for AvifOptions {
    fn default() -> Self {
        Self {
            quality: 60,
            speed: 4,
        }
    }
}

// ── JPEG XL ───────────────────────────────────────────────────────────────────

/// Options for JPEG XL encoding via `jpegxl-rs` (wraps `libjxl`).
///
/// # System dependency
/// Requires the `libjxl` C library (`libjxl-dev` on Debian/Ubuntu).
/// Alternatively, enable the `vendored` sub-feature of `jpegxl-rs` to compile it from source.
///
/// # Licence note
/// `jpegxl-rs` and `jpegxl-sys` are licensed **GPL-3.0-or-later**, which is compatible with
/// this crate's AGPL-3.0-only licence.
#[derive(Debug, Clone, PartialEq)]
pub struct JxlOptions {
    /// Butteraugli perceptual distance target (lossy quality):
    ///
    /// * `0.0` – mathematically lossless (use `lossless = true` for true bit-exact lossless).
    /// * `1.0` – visually lossless (default).
    /// * Recommended range: `0.5`–`3.0`.
    /// * Maximum: `15.0`.
    ///
    /// Ignored when `lossless = true`.
    pub distance: f32,
    /// Encoder effort (1–9, higher → smaller output file at the cost of encoding speed).
    ///
    /// Corresponds to `libjxl`'s `effort` parameter:
    /// 1 = Lightning, 7 = Squirrel (default), 9 = Tortoise.
    pub effort: u8,
    /// Use true lossless encoding.
    ///
    /// When `true`, overrides `distance` (sets it to 0.0 internally).
    /// Default: `false`.
    pub lossless: bool,
}

impl Default for JxlOptions {
    fn default() -> Self {
        Self {
            distance: 1.0,
            effort: 7,
            lossless: false,
        }
    }
}

// ── Bundle ────────────────────────────────────────────────────────────────────

/// All per-format encoder options bundled together.
///
/// Construct with `Default::default()` and override only what you need:
/// ```rust
/// use libgeotiles::{EncodeOptions, JpegOptions};
/// let opts = EncodeOptions { jpeg: JpegOptions { quality: 90 }, ..Default::default() };
/// ```
/// Unset options fall back to their `Default` implementation.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EncodeOptions {
    /// PNG encoding options.
    pub png: PngOptions,
    /// JPEG encoding options.
    pub jpeg: JpegOptions,
    /// WebP encoding options.
    pub webp: WebPOptions,
    /// AVIF encoding options.
    pub avif: AvifOptions,
    /// JPEG XL encoding options.
    pub jxl: JxlOptions,
}

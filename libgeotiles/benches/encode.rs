#![allow(unused_imports, dead_code, unused_variables)]
//! Benchmarks for tile encoder throughput.
//!
//! Each benchmark encodes a fixed synthetic pixel buffer (matching a standard 256×256 tile)
//! to measure raw encoder performance.  Results are recorded in `docs/benchmarks/` when a
//! path is first stabilised — see §6.6 of the migration plan.
//!
//! Run with default features (PNG):
//!   cargo bench -p libgeotiles --bench encode
//!
//! Run with all format features:
//!   cargo bench -p libgeotiles --bench encode --features png,jpeg,webp,avif,jxl

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
#[cfg(feature = "jxl")]
use libgeotiles::JxlOptions;
use libgeotiles::tile::Format;
use libgeotiles::{
    AvifOptions, EncodeOptions, JpegOptions, PngCompression, PngOptions, encode_tile,
};
use std::hint::black_box;

// ── Helpers ───────────────────────────────────────────────────────────────────

#[allow(dead_code)]
const TILE: u32 = 256;

/// Build a synthetic RGBA pixel buffer of the given dimensions.
#[allow(dead_code)]
fn synthetic_rgba(width: u32, height: u32) -> Vec<u8> {
    let len = (width * height * 4) as usize;
    (0..len).map(|i| (i % 251) as u8).collect()
}

/// Build a synthetic RGB pixel buffer of the given dimensions.
#[allow(dead_code)]
fn synthetic_rgb(width: u32, height: u32) -> Vec<u8> {
    let len = (width * height * 3) as usize;
    (0..len).map(|i| (i % 251) as u8).collect()
}

// ── PNG ───────────────────────────────────────────────────────────────────────

#[cfg(feature = "png")]
fn bench_png(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode_png");

    let compressions = [
        ("default", PngCompression::Default),
        ("fast", PngCompression::Fast),
        ("best", PngCompression::Best),
    ];

    for (name, compression) in compressions {
        let pixels = synthetic_rgba(TILE, TILE);
        let opts = EncodeOptions {
            png: PngOptions {
                compression,
                ..Default::default()
            },
            ..Default::default()
        };

        group.bench_with_input(
            BenchmarkId::new("rgba256", name),
            &(pixels, opts),
            |b, (px, o)| {
                b.iter_batched(
                    || px.clone(),
                    |p| encode_tile(black_box(&p), TILE, TILE, 4, Format::Png, o).unwrap(),
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

// ── JPEG ──────────────────────────────────────────────────────────────────────

#[cfg(feature = "jpeg")]
fn bench_jpeg(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode_jpeg");

    for quality in [50u8, 75, 85, 95] {
        let pixels = synthetic_rgb(TILE, TILE);
        let opts = EncodeOptions {
            jpeg: JpegOptions { quality },
            ..Default::default()
        };

        group.bench_with_input(
            BenchmarkId::new("rgb256", quality),
            &(pixels, opts),
            |b, (px, o)| {
                b.iter_batched(
                    || px.clone(),
                    |p| encode_tile(black_box(&p), TILE, TILE, 3, Format::Jpeg, o).unwrap(),
                    BatchSize::SmallInput,
                );
            },
        );
    }

    // Also benchmark RGBA input (exercises the alpha-stripping path).
    {
        let pixels = synthetic_rgba(TILE, TILE);
        let opts = EncodeOptions::default(); // quality = 85

        group.bench_function("rgba256_alpha_strip", |b| {
            b.iter_batched(
                || pixels.clone(),
                |p| encode_tile(black_box(&p), TILE, TILE, 4, Format::Jpeg, &opts).unwrap(),
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

// ── WebP ──────────────────────────────────────────────────────────────────────

#[cfg(feature = "webp")]
fn bench_webp(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode_webp");

    // The `image/webp` codec is currently lossless-only; one benchmark suffices.
    let pixels_rgb = synthetic_rgb(TILE, TILE);
    let pixels_rgba = synthetic_rgba(TILE, TILE);
    let opts = EncodeOptions::default();

    group.bench_function("rgb256_lossless", |b| {
        b.iter_batched(
            || pixels_rgb.clone(),
            |p| encode_tile(black_box(&p), TILE, TILE, 3, Format::WebP, &opts).unwrap(),
            BatchSize::SmallInput,
        );
    });

    group.bench_function("rgba256_lossless", |b| {
        b.iter_batched(
            || pixels_rgba.clone(),
            |p| encode_tile(black_box(&p), TILE, TILE, 4, Format::WebP, &opts).unwrap(),
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

// ── AVIF ──────────────────────────────────────────────────────────────────────

#[cfg(feature = "avif")]
fn bench_avif(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode_avif");
    // AVIF is slow; reduce the sample count to keep bench runs manageable.
    group.sample_size(20);

    for speed in [4u8, 6, 8] {
        let pixels = synthetic_rgb(TILE, TILE);
        let opts = EncodeOptions {
            avif: AvifOptions { quality: 60, speed },
            ..Default::default()
        };

        group.bench_with_input(
            BenchmarkId::new("rgb256_q60", speed),
            &(pixels, opts),
            |b, (px, o)| {
                b.iter_batched(
                    || px.clone(),
                    |p| encode_tile(black_box(&p), TILE, TILE, 3, Format::Avif, o).unwrap(),
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

// ── JPEG XL ───────────────────────────────────────────────────────────────────

#[cfg(feature = "jxl")]
fn bench_jxl(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode_jxl");
    group.sample_size(20);

    let cases = [
        ("d1_e7", 1.0f32, 7u8, false),
        ("d2_e5", 2.0, 5, false),
        ("lossless_e7", 0.0, 7, true),
    ];

    for (label, distance, effort, lossless) in cases {
        let pixels = synthetic_rgb(TILE, TILE);
        let opts = EncodeOptions {
            jxl: JxlOptions {
                distance,
                effort,
                lossless,
            },
            ..Default::default()
        };

        group.bench_with_input(
            BenchmarkId::new("rgb256", label),
            &(pixels, opts),
            |b, (px, o)| {
                b.iter_batched(
                    || px.clone(),
                    |p| encode_tile(black_box(&p), TILE, TILE, 3, Format::Jxl, o).unwrap(),
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

// ── Criterion wiring ──────────────────────────────────────────────────────────

/// Single dispatcher that calls each format benchmark when its feature is on.
/// Using one function avoids `criterion_group!` limitations with `#[cfg(...)]`.
#[allow(unused_variables)]
fn all_benchmarks(c: &mut Criterion) {
    #[cfg(feature = "png")]
    bench_png(c);
    #[cfg(feature = "jpeg")]
    bench_jpeg(c);
    #[cfg(feature = "webp")]
    bench_webp(c);
    #[cfg(feature = "avif")]
    bench_avif(c);
    #[cfg(feature = "jxl")]
    bench_jxl(c);
}

criterion_group!(benches, all_benchmarks);
criterion_main!(benches);

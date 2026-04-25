//! Benchmarks for the tile resample (crop + resize) hot path — CPU vs GPU.
//!
//! These benchmarks measure the per-tile crop + resize step in isolation,
//! independent of GDAL I/O or encoding.  The input is a synthetic
//! `ChunkBuffer` of known dimensions; the output is a `tile_size × tile_size`
//! pixel buffer ready for encoding.
//!
//! Run CPU-only (default features):
//!   cargo bench -p libgeotiles --bench pipeline
//!
//! Run with GPU backend (requires GPU hardware or software renderer like lavapipe):
//!   cargo bench -p libgeotiles --bench pipeline --features gpu
//!
//! Compare GPU vs CPU results: Criterion stores HTML reports in `target/criterion/`.
//! Snapshot baselines in `docs/benchmarks/` when a path is first stabilised (see §6.6).

use criterion::{BatchSize, BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use libgeotiles::backend::cpu::crop_tile;
use libgeotiles::tile::{ChunkBuffer, PixelWindow};

// ── Helpers ───────────────────────────────────────────────────────────────────

const TILE: u32 = 256;

fn synthetic_chunk(bands: usize, src_w: usize, src_h: usize) -> ChunkBuffer {
    let band_data: Vec<Vec<u8>> = (0..bands)
        .map(|b| {
            (0..src_w * src_h)
                .map(|i| ((i + b * 37) % 251) as u8)
                .collect()
        })
        .collect();
    ChunkBuffer {
        band_data,
        ds_width: src_w,
        row_start: 0,
        row_count: src_h,
    }
}

fn window(w: usize, h: usize) -> PixelWindow {
    PixelWindow {
        col: 0,
        row: 0,
        width: w,
        height: h,
    }
}

// ── CPU resample ──────────────────────────────────────────────────────────────

fn bench_tile_resample_cpu(c: &mut Criterion) {
    let mut group = c.benchmark_group("tile_resample_cpu");

    for (label, bands, src_w, src_h) in [
        ("rgba_256x256_to_256", 4usize, 256usize, 256usize),
        ("rgba_512x512_to_256", 4, 512, 512),
        ("rgb_512x512_to_256", 3, 512, 512),
    ] {
        let chunk = synthetic_chunk(bands, src_w, src_h);
        let win = window(src_w, src_h);

        group.bench_with_input(
            BenchmarkId::new(label, TILE),
            &(chunk, win),
            |b, (ch, w)| {
                b.iter_batched(
                    || (),
                    |_| crop_tile(black_box(ch), black_box(*w), TILE).unwrap(),
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

// ── GPU resample ──────────────────────────────────────────────────────────────

#[cfg(feature = "gpu")]
fn bench_tile_resample_gpu(c: &mut Criterion) {
    use libgeotiles::backend::gpu::GpuContext;

    // Attempt GPU init once; skip all GPU benchmarks gracefully if unavailable.
    let ctx = match GpuContext::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("GPU benchmark skipped: {e}");
            return;
        }
    };

    let mut group = c.benchmark_group("tile_resample_gpu");

    for (label, bands, src_w, src_h) in [
        ("rgba_256x256_to_256", 4usize, 256usize, 256usize),
        ("rgba_512x512_to_256", 4, 512, 512),
        ("rgb_512x512_to_256", 3, 512, 512),
    ] {
        let chunk = synthetic_chunk(bands, src_w, src_h);
        let win = window(src_w, src_h);

        group.bench_with_input(
            BenchmarkId::new(label, TILE),
            &(chunk, win),
            |b, (ch, w)| {
                b.iter_batched(
                    || (),
                    |_| ctx.crop_tile(black_box(ch), black_box(*w), TILE).unwrap(),
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

// ── Criterion wiring ──────────────────────────────────────────────────────────

#[allow(unused_variables)]
fn all_benchmarks(c: &mut Criterion) {
    bench_tile_resample_cpu(c);
    #[cfg(feature = "gpu")]
    bench_tile_resample_gpu(c);
}

criterion_group!(benches, all_benchmarks);
criterion_main!(benches);

//! Benchmarks for the per-tile resample step in isolation (no I/O, no encoding).
//!
//!   cargo bench -p libgeotiles --bench pipeline
//!   cargo bench -p libgeotiles --bench pipeline --features gpu   # adds GPU rows
//!
//! Criterion writes HTML reports to `target/criterion/`.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use libgeotiles::plan::{DstRect, SrcRect, TileJob};
use libgeotiles::{CpuResampler, Resampler, RowBlock, Tile};
use std::hint::black_box;

const TILE: u32 = 256;

/// Cases: (label, bands, source window side). The block is 4× wider than the window so
/// the resampler works inside a larger row block, as in the real pipeline.
const CASES: [(&str, usize, usize); 4] = [
    ("rgb_256_to_256", 3, 256),
    ("rgb_512_to_256", 3, 512),
    ("rgba_512_to_256", 4, 512),
    ("rgb_4096_to_256", 3, 4096),
];

fn block(bands: usize, side: usize) -> RowBlock {
    let width = side * 4;
    let rows = side + 16;
    RowBlock {
        data: (0..width * rows * bands).map(|i| (i % 251) as u8).collect(),
        width,
        bands,
        row_start: 0,
        rows,
    }
}

fn job(side: usize) -> TileJob {
    TileJob {
        tile: Tile::new(0, 0, 0),
        src: SrcRect {
            x: side as f64 + 0.37,
            y: 8.21,
            width: side as f64,
            height: side as f64,
        },
        dst: DstRect::full(TILE),
    }
}

fn bench_with(c: &mut Criterion, group: &str, r: &dyn Resampler) {
    let mut g = c.benchmark_group(group);
    for (label, bands, side) in CASES {
        let (blk, j) = (block(bands, side), job(side));
        g.bench_with_input(BenchmarkId::new(label, TILE), &(blk, j), |b, (blk, j)| {
            b.iter(|| r.resample(black_box(blk), black_box(j), TILE).unwrap())
        });
    }
    g.finish();
}

fn all_benchmarks(c: &mut Criterion) {
    bench_with(c, "tile_resample_cpu", &CpuResampler);
    #[cfg(feature = "gpu")]
    match libgeotiles::GpuResampler::new() {
        Ok(gpu) => bench_with(c, "tile_resample_gpu", &gpu),
        Err(e) => eprintln!("GPU benchmark skipped: {e}"),
    }
}

criterion_group!(benches, all_benchmarks);
criterion_main!(benches);

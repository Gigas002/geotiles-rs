use super::cpu::CpuResampler;
use super::{Resampler, TilePixels};
use crate::coords::Tile;
use crate::plan::{DstRect, SrcRect, TileJob};
use crate::raster::RowBlock;

fn block(
    width: usize,
    rows: usize,
    bands: usize,
    row_start: usize,
    f: impl Fn(usize, usize, usize) -> u8,
) -> RowBlock {
    let mut data = Vec::with_capacity(width * rows * bands);
    for r in 0..rows {
        for c in 0..width {
            for b in 0..bands {
                data.push(f(row_start + r, c, b));
            }
        }
    }
    RowBlock {
        data,
        width,
        bands,
        row_start,
        rows,
    }
}

fn job(x: f64, y: f64, w: f64, h: f64, dst: DstRect) -> TileJob {
    TileJob {
        tile: Tile::new(0, 0, 0),
        src: SrcRect {
            x,
            y,
            width: w,
            height: h,
        },
        dst,
    }
}

/// Smooth gradient: resamplers agree on it up to kernel differences.
fn gradient(r: usize, c: usize, b: usize) -> u8 {
    ((r * 3 + c * 2 + b * 20) % 256) as u8
}

#[test]
fn cpu_identity_at_one_to_one() {
    let blk = block(4, 4, 1, 0, |r, c, _| (r * 4 + c) as u8);
    let out = CpuResampler
        .resample(&blk, &job(0.0, 0.0, 4.0, 4.0, DstRect::full(4)), 4)
        .unwrap();
    assert_eq!(
        out,
        TilePixels {
            data: (0..16).collect(),
            bands: 1
        }
    );
}

#[test]
fn cpu_reads_relative_to_block_row_start() {
    // Rows 10..20 of a raster, constant 42: any window renders as 42.
    let blk = block(8, 10, 3, 10, |_, _, _| 42);
    let out = CpuResampler
        .resample(&blk, &job(1.5, 12.25, 5.0, 4.0, DstRect::full(16)), 16)
        .unwrap();
    assert_eq!(out.bands, 3);
    assert!(out.data.iter().all(|&p| p == 42));
}

#[test]
fn cpu_leaves_canvas_outside_dst_empty() {
    let blk = block(8, 8, 4, 0, |_, _, _| 200);
    let dst = DstRect {
        x: 4,
        y: 0,
        width: 4,
        height: 8,
    };
    let out = CpuResampler
        .resample(&blk, &job(0.0, 0.0, 8.0, 8.0, dst), 8)
        .unwrap();
    for (i, px) in out.data.as_chunks::<4>().0.iter().enumerate() {
        let inside = i % 8 >= 4;
        assert_eq!(*px, if inside { [200; 4] } else { [0; 4] }, "pixel {i}");
    }
}

#[test]
fn cpu_result_independent_of_how_many_rows_the_block_holds() {
    // A tile in the middle of a 64-row raster, rendered from the whole raster and from
    // just the rows it needs: must be identical, or batching would show seams.
    let j = job(10.3, 20.7, 17.0, 13.0, DstRect::full(8));
    let full = block(48, 64, 3, 0, gradient);
    let (lo, hi) = j.rows_needed(CpuResampler.support(), 64);
    let slice = block(48, hi - lo, 3, lo, gradient);
    assert_eq!(
        CpuResampler.resample(&full, &j, 8).unwrap(),
        CpuResampler.resample(&slice, &j, 8).unwrap()
    );
}

#[test]
fn cpu_upscale_honours_sub_pixel_offset() {
    // Horizontal ramp: pixel column c (centre c + 0.5) has value 10c, which Lanczos
    // reproduces exactly away from the edges. A quarter-pixel window at x = 3.25 must
    // land on values 10 * (u - 0.5) for u in (3.25, 3.5): about 27.6 → 29.9.
    let blk = block(16, 16, 1, 0, |_, c, _| (c * 10) as u8);
    let out = CpuResampler
        .resample(&blk, &job(3.25, 6.0, 0.25, 0.25, DstRect::full(16)), 16)
        .unwrap();
    let row = &out.data[..16];
    assert!(row.iter().all(|&p| (27..=30).contains(&p)), "{row:?}");
    assert!(row[0] < row[15], "{row:?}");
}

#[cfg(feature = "gpu")]
mod gpu {
    //! Need a GPU adapter (or a software one such as lavapipe):
    //! `cargo test -p libgeotiles --features gpu -- --ignored resample::`
    use super::*;
    use crate::resample::gpu::GpuResampler;

    #[test]
    #[ignore = "requires GPU adapter"]
    fn rgba_output_and_opaque_alpha_for_gray() {
        let ctx = GpuResampler::new().expect("GPU init");
        let blk = block(32, 32, 1, 0, gradient);
        let out = ctx
            .resample(&blk, &job(0.0, 0.0, 32.0, 32.0, DstRect::full(64)), 64)
            .unwrap();
        assert_eq!((out.bands, out.data.len()), (4, 64 * 64 * 4));
        assert!(out.data.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
    }

    #[test]
    #[ignore = "requires GPU adapter"]
    fn close_to_cpu_on_smooth_input() {
        let ctx = GpuResampler::new().expect("GPU init");
        let blk = block(64, 64, 4, 0, |r, c, b| {
            ((r + c) * 2 + b * 10).min(255) as u8
        });
        let j = job(4.5, 3.25, 50.0, 50.0, DstRect::full(32));
        let gpu = ctx.resample(&blk, &j, 32).unwrap();
        let cpu = CpuResampler.resample(&blk, &j, 32).unwrap();
        let diff: u64 = gpu
            .data
            .iter()
            .zip(&cpu.data)
            .map(|(a, b)| a.abs_diff(*b) as u64)
            .sum();
        let mean = diff as f64 / gpu.data.len() as f64;
        assert!(mean < 2.0, "mean diff {mean:.2}");
    }

    #[test]
    #[ignore = "requires GPU adapter"]
    fn downscale_averages_instead_of_aliasing() {
        // 1-px checkerboard shrunk 8×: every output pixel should be mid-gray.
        let ctx = GpuResampler::new().expect("GPU init");
        let blk = block(
            256,
            256,
            1,
            0,
            |r, c, _| if (r + c) % 2 == 0 { 255 } else { 0 },
        );
        let out = ctx
            .resample(&blk, &job(0.0, 0.0, 256.0, 256.0, DstRect::full(32)), 32)
            .unwrap();
        for px in out.data.as_chunks::<4>().0 {
            assert!((120..=135).contains(&px[0]), "aliased value {}", px[0]);
        }
    }
}

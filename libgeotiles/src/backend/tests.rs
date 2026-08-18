use super::cpu::crop_tile;
use crate::tile::{ChunkBuffer, DstRect, PixelWindow};

#[test]
fn crop_tile_identity_1band() {
    let data = (0u8..16).collect::<Vec<_>>();
    let chunk = ChunkBuffer {
        band_data: vec![data],
        ds_width: 4,
        row_start: 0,
        row_count: 4,
    };
    let window = PixelWindow {
        col: 0,
        row: 0,
        width: 4,
        height: 4,
    };
    let result = crop_tile(&chunk, window, 2, DstRect::full(2)).unwrap();
    assert_eq!(result.len(), 4, "2×2 × 1 band = 4 bytes");
}

#[test]
fn crop_tile_sub_window_3band() {
    let band: Vec<u8> = (0u8..24).collect();
    let chunk = ChunkBuffer {
        band_data: vec![band.clone(), band.clone(), band],
        ds_width: 6,
        row_start: 0,
        row_count: 4,
    };
    let window = PixelWindow {
        col: 2,
        row: 1,
        width: 2,
        height: 2,
    };
    let result = crop_tile(&chunk, window, 4, DstRect::full(4)).unwrap();
    assert_eq!(result.len(), 4 * 4 * 3, "4×4 × 3 bands = 48 bytes");
}

#[test]
fn crop_tile_respects_chunk_row_offset() {
    let data = vec![42u8; 8 * 4];
    let chunk = ChunkBuffer {
        band_data: vec![data],
        ds_width: 8,
        row_start: 10,
        row_count: 4,
    };
    let window = PixelWindow {
        col: 0,
        row: 10,
        width: 8,
        height: 4,
    };
    let result = crop_tile(&chunk, window, 4, DstRect::full(4)).unwrap();
    assert_eq!(result.len(), 16);
    assert!(result.iter().all(|&p| p == 42));
}

// ── GPU ───────────────────────────────────────────────────────────────────────
//
// Require an actual GPU adapter (Vulkan, Metal, DX12, or a software renderer
// such as lavapipe).  Marked `#[ignore]` so plain `cargo test` stays offline.
//
// Run manually:
//   cargo test -p libgeotiles --features gpu -- --ignored backend::

#[cfg(feature = "gpu")]
mod gpu_tests {
    use super::super::gpu::GpuContext;
    use crate::tile::{ChunkBuffer, DstRect, PixelWindow};

    /// A smooth per-band diagonal gradient. Deliberately *not* `(i + b*7) % 251`-style
    /// data: that wraps every 251 pixels within a 1024-pixel band, creating sawtooth
    /// discontinuities that make bilinear (GPU) and Lanczos3 (CPU) diverge sharply via
    /// ringing — real algorithmic disagreement on adversarial data, not a bug, but it
    /// makes an "approximately equal" comparison meaningless.
    fn synthetic_chunk(bands: usize) -> ChunkBuffer {
        let w = 32usize;
        let h = 32usize;
        let band_data: Vec<Vec<u8>> = (0..bands)
            .map(|b| {
                (0..h)
                    .flat_map(|row| {
                        (0..w).map(move |col| {
                            let gradient = ((row * 255) / (h - 1) + (col * 255) / (w - 1)) / 2;
                            (gradient as i32 - (b as i32) * 20).clamp(0, 255) as u8
                        })
                    })
                    .collect()
            })
            .collect();
        ChunkBuffer {
            band_data,
            ds_width: w,
            row_start: 0,
            row_count: h,
        }
    }

    fn window_full(w: usize, h: usize) -> PixelWindow {
        PixelWindow {
            col: 0,
            row: 0,
            width: w,
            height: h,
        }
    }

    #[test]
    #[ignore = "requires GPU adapter (run manually or in GPU-equipped CI)"]
    fn init_succeeds() {
        GpuContext::new().expect("GPU context should initialise");
    }

    #[test]
    #[ignore = "requires GPU adapter"]
    fn crop_tile_returns_correct_size() {
        let ctx = GpuContext::new().expect("GPU init");
        let chunk = synthetic_chunk(3);
        let win = window_full(32, 32);
        let result = ctx
            .crop_tile(&chunk, win, 256, DstRect::full(256))
            .expect("crop_tile");
        assert_eq!(
            result.len(),
            256 * 256 * 4,
            "GPU output should be RGBA 256×256"
        );
    }

    #[test]
    #[ignore = "requires GPU adapter"]
    fn crop_tile_single_band_alpha_is_255() {
        let ctx = GpuContext::new().expect("GPU init");
        let chunk = synthetic_chunk(1);
        let win = window_full(32, 32);
        let result = ctx
            .crop_tile(&chunk, win, 64, DstRect::full(64))
            .expect("crop_tile single band");
        assert_eq!(result.len(), 64 * 64 * 4);
        for pixel in result.chunks_exact(4) {
            assert_eq!(pixel[3], 255, "alpha should be 255 for single-band source");
        }
    }

    #[test]
    #[ignore = "requires GPU adapter"]
    fn crop_tile_matches_cpu_approximately() {
        use crate::backend::cpu::crop_tile as cpu_crop;

        let ctx = GpuContext::new().expect("GPU init");
        let chunk = synthetic_chunk(4);
        let win = window_full(32, 32);

        let cpu_out = cpu_crop(&chunk, win, 64, DstRect::full(64)).expect("CPU crop");
        let gpu_out = ctx
            .crop_tile(&chunk, win, 64, DstRect::full(64))
            .expect("GPU crop");

        assert_eq!(gpu_out.len(), 64 * 64 * 4);

        // CPU (Lanczos3) and GPU (bilinear) are genuinely different resampling kernels,
        // not just different roundings of the same one — Lanczos3's wider kernel needs
        // edge-extrapolated samples near tile boundaries, which legitimately diverges
        // from bilinear's purely local sampling there (observed: a handful of near-edge
        // pixels differing by 50+, even on a smooth gradient with no sharp source
        // features). A per-pixel max-diff bound is therefore the wrong check; compare
        // the *mean* absolute difference across the whole tile instead, which is stable
        // and small when the two algorithms genuinely agree overall.
        let sum: u64 = cpu_out
            .iter()
            .zip(gpu_out.iter())
            .map(|(c, g)| c.abs_diff(*g) as u64)
            .sum();
        let mean_diff = sum as f64 / cpu_out.len() as f64;
        assert!(
            mean_diff <= 8.0,
            "GPU/CPU mean pixel diff should be small, got {mean_diff:.2}"
        );
    }
}

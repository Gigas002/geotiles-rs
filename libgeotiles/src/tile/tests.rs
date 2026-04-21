use super::{ChunkBuffer, PixelWindow, crop_tile};

#[test]
fn crop_tile_identity_1band() {
    // 4×4 grayscale chunk, tile window = entire chunk → resize to 2×2
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
    let result = crop_tile(&chunk, window, 2).unwrap();
    assert_eq!(result.len(), 4, "2×2 × 1 band = 4 bytes");
}

#[test]
fn crop_tile_sub_window_3band() {
    // 6×4 RGB chunk; extract a 2×2 sub-window and scale to 4×4
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
    let result = crop_tile(&chunk, window, 4).unwrap();
    assert_eq!(result.len(), 4 * 4 * 3, "4×4 × 3 bands = 48 bytes");
}

#[test]
fn crop_tile_respects_chunk_row_offset() {
    // Chunk starts at row 10; tile window is at absolute row 10
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
    let result = crop_tile(&chunk, window, 4).unwrap();
    assert_eq!(result.len(), 16);
    // All source pixels are 42, so resized result should stay 42
    assert!(result.iter().all(|&p| p == 42));
}

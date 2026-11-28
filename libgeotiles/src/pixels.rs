//! Band-layout conversion of rendered tiles (L / LA / RGB / RGBA).

use crate::plan::DstRect;
use crate::resample::TilePixels;

/// Convert `px` to `to` bands (1–4).
///
/// Gray expands to RGB by replication, colour reduces to gray by Rec. 601 luma, alpha is
/// kept when both sides have it and dropped otherwise. A synthesised alpha is opaque
/// inside `dst` (real data) and transparent outside it, so the uncovered part of an edge
/// tile stays see-through.
pub fn convert_bands(px: TilePixels, to: usize, tile_size: u32, dst: DstRect) -> Vec<u8> {
    let from = px.bands;
    if from == to {
        return px.data;
    }
    let from_alpha = from == 2 || from == 4;
    let to_alpha = to == 2 || to == 4;
    let to_color = to >= 3;
    let mut out = Vec::with_capacity(px.data.len() / from * to);

    for (i, p) in px.data.chunks_exact(from).enumerate() {
        let (r, g, b) = if from >= 3 {
            (p[0], p[1], p[2])
        } else {
            (p[0], p[0], p[0])
        };
        if to_color {
            out.extend_from_slice(&[r, g, b]);
        } else if from >= 3 {
            let luma = (299 * r as u32 + 587 * g as u32 + 114 * b as u32 + 500) / 1000;
            out.push(luma as u8);
        } else {
            out.push(r);
        }
        if to_alpha {
            let alpha = if from_alpha {
                p[from - 1]
            } else {
                let (col, row) = (i as u32 % tile_size, i as u32 / tile_size);
                if dst.contains(col, row) { 255 } else { 0 }
            };
            out.push(alpha);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(data: Vec<u8>, bands: usize) -> TilePixels {
        TilePixels { data, bands }
    }

    #[test]
    fn synthesised_alpha_follows_dst() {
        // 2×2 RGB tile, real data only in the top-left pixel.
        let data = vec![10, 20, 30, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let dst = DstRect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        };
        assert_eq!(
            convert_bands(px(data, 3), 4, 2, dst),
            vec![10, 20, 30, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        );
    }

    #[test]
    fn gray_replicates_and_color_reduces_to_luma() {
        let full = DstRect::full(1);
        assert_eq!(convert_bands(px(vec![7], 1), 3, 1, full), vec![7, 7, 7]);
        assert_eq!(
            convert_bands(px(vec![7, 9], 2), 4, 1, full),
            vec![7, 7, 7, 9]
        );
        assert_eq!(
            convert_bands(px(vec![255, 0, 0, 40], 4), 2, 1, full),
            vec![76, 40]
        );
        assert_eq!(
            convert_bands(px(vec![1, 2, 3, 4], 4), 3, 1, full),
            vec![1, 2, 3]
        );
        assert_eq!(
            convert_bands(px(vec![9, 8, 7], 3), 3, 1, full),
            vec![9, 8, 7]
        );
    }
}

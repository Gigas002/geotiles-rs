// Bilinear-resize compute shader.
//
// Reads from a 2-D RGBA texture (source window, arbitrary size) and writes
// bilinearly-sampled output to a storage buffer as packed RGBA u32 values
// (R in bits 0-7, G in 8-15, B in 16-23, A in 24-31).
//
// Using a storage buffer rather than a storage texture avoids rgba8unorm
// write-access support requirements that not all backends/drivers guarantee.
//
// Bindings:
//   0 — source texture (texture_2d<f32>)
//   1 — bilinear sampler
//   2 — output buffer  (array<u32>, one u32 per pixel, row-major)
//   3 — uniform dims   (vec4<u32>: [dst_width, dst_height, 0, 0])

@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var src_sampler: sampler;
@group(0) @binding(2) var<storage, read_write> dst_buf: array<u32>;
@group(0) @binding(3) var<uniform> dims: vec4<u32>;

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dst_w = dims.x;
    let dst_h = dims.y;
    if gid.x >= dst_w || gid.y >= dst_h {
        return;
    }

    // Centre-of-pixel UV mapping for correct bilinear interpolation at edges.
    let uv = (vec2<f32>(gid.xy) + vec2<f32>(0.5)) / vec2<f32>(f32(dst_w), f32(dst_h));
    let c = textureSampleLevel(src_tex, src_sampler, uv, 0.0);

    let r = u32(clamp(c.r, 0.0, 1.0) * 255.0 + 0.5);
    let g = u32(clamp(c.g, 0.0, 1.0) * 255.0 + 0.5);
    let b = u32(clamp(c.b, 0.0, 1.0) * 255.0 + 0.5);
    let a = u32(clamp(c.a, 0.0, 1.0) * 255.0 + 0.5);

    dst_buf[gid.y * dst_w + gid.x] = r | (g << 8u) | (b << 16u) | (a << 24u);
}

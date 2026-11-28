// Tile resample compute shader.
//
// Maps each output pixel back onto a fractional source rectangle (`crop`, in texel units
// of the uploaded source view) and filters it:
// - upscaling (scale <= 1): one bilinear tap at the pixel centre;
// - downscaling: a box filter over the pixel's footprint, approximated by an n×n grid of
//   bilinear taps (n = ceil(scale), at most 32), so low zooms average instead of alias.
// Colour is premultiplied by alpha while averaging.
//
// Output goes to a storage buffer as packed RGBA u32 (R in bits 0-7 ... A in 24-31):
// storage textures with rgba8unorm write access are not guaranteed everywhere.
//
// Bindings:
//   0 — source view (texture_2d<f32>, rgba8unorm)
//   1 — bilinear sampler (clamp to edge)
//   2 — output buffer (array<u32>, one per destination pixel, row-major)
//   3 — params: dst = [width, height, 0, 0], crop = [left, top, width, height]

struct Params {
    dst: vec4<u32>,
    crop: vec4<f32>,
}

@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var src_sampler: sampler;
@group(0) @binding(2) var<storage, read_write> dst_buf: array<u32>;
@group(0) @binding(3) var<uniform> params: Params;

const MAX_TAPS: f32 = 32.0;

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dst_size = params.dst.xy;
    if gid.x >= dst_size.x || gid.y >= dst_size.y {
        return;
    }

    let tex_size = vec2<f32>(textureDimensions(src_tex));
    let scale = params.crop.zw / vec2<f32>(dst_size);
    let centre = params.crop.xy + (vec2<f32>(gid.xy) + vec2<f32>(0.5)) * scale;
    let taps = clamp(ceil(scale), vec2<f32>(1.0), vec2<f32>(MAX_TAPS));
    let n = vec2<u32>(taps);

    var acc = vec4<f32>(0.0);
    for (var j = 0u; j < n.y; j++) {
        for (var i = 0u; i < n.x; i++) {
            // Tap offsets in [-0.5, 0.5) of the footprint; a single tap sits at the centre.
            let off = (vec2<f32>(f32(i), f32(j)) + vec2<f32>(0.5)) / taps - vec2<f32>(0.5);
            let pos = centre + off * scale;
            let c = textureSampleLevel(src_tex, src_sampler, pos / tex_size, 0.0);
            acc += vec4<f32>(c.rgb * c.a, c.a);
        }
    }
    acc /= taps.x * taps.y;

    var rgb = vec3<f32>(0.0);
    if acc.a > 0.0 {
        rgb = acc.rgb / acc.a;
    }
    let r = u32(clamp(rgb.r, 0.0, 1.0) * 255.0 + 0.5);
    let g = u32(clamp(rgb.g, 0.0, 1.0) * 255.0 + 0.5);
    let b = u32(clamp(rgb.b, 0.0, 1.0) * 255.0 + 0.5);
    let a = u32(clamp(acc.a, 0.0, 1.0) * 255.0 + 0.5);

    dst_buf[gid.y * dst_size.x + gid.x] = r | (g << 8u) | (b << 16u) | (a << 24u);
}

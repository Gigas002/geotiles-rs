// GPU-accelerated tile crop + bilinear resize via wgpu.
//
// Only compiled when the `gpu` Cargo feature is enabled.
//
// Design summary
// ──────────────
// `GpuContext` is initialised once per pipeline run and reused for every tile.
// Initialisation is async; `new()` blocks with `pollster::block_on`.
//
// Per-tile flow
// ─────────────
// 1. Extract source window from `ChunkBuffer` into an interleaved RGBA u8 vec
//    (1-band → grey replicated to RGB, A=255; 2-band La8 → grey+alpha;
//     3-band RGB → pad A=255; 4-band RGBA → as-is).
// 2. Upload as `Rgba8Unorm` source texture.
// 3. Dispatch the compute shader (shaders/resize.wgsl) to a storage buffer of
//    `tile_size * tile_size` packed RGBA u32 values.
// 4. Copy storage buffer → staging buffer; map + read back.
// 5. Unpack u32 → [u8; 4] RGBA and return.
//
// The GPU path always returns 4-band RGBA.  Exact pixel values may differ
// slightly from the CPU path due to floating-point rounding in the shader.

use pollster::FutureExt as _;
use tracing::{debug, info};

use crate::error::Error;
use crate::tile::{ChunkBuffer, PixelWindow};

const SHADER_SRC: &str = include_str!("shaders/resize.wgsl");
const WORKGROUP: u32 = 8;

/// One-time GPU device context reused across all tile operations in a pipeline run.
pub struct GpuContext {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

impl GpuContext {
    /// Initialise GPU adapter, device, queue, and compile the resize compute pipeline.
    ///
    /// Returns `Err(Error::Gpu)` when no suitable adapter is available (e.g. no Vulkan/Metal/DX12/GL).
    /// Callers should fall back to the CPU backend on error.
    pub fn new() -> crate::Result<Self> {
        async {
            let instance = wgpu::Instance::default();
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    compatible_surface: None,
                    force_fallback_adapter: false,
                })
                .await
                .map_err(|e| Error::Gpu(e.to_string()))?;

            let info = adapter.get_info();
            info!(backend = ?info.backend, name = %info.name, "GPU adapter selected");

            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: Some("geotiles-gpu"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    ..Default::default()
                })
                .await
                .map_err(|e| Error::Gpu(e.to_string()))?;

            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("resize"),
                source: wgpu::ShaderSource::Wgsl(SHADER_SRC.into()),
            });

            let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("resize_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

            let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("resize_pl"),
                bind_group_layouts: &[Some(&bgl)],
                immediate_size: 0,
            });

            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("resize_pipeline"),
                layout: Some(&pl),
                module: &shader,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });

            let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("bilinear"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Nearest,
                ..Default::default()
            });

            Ok(Self {
                device,
                queue,
                pipeline,
                bgl,
                sampler,
            })
        }
        .block_on()
    }

    /// Crop the pixel window from `chunk` and resize to `tile_size × tile_size` on the GPU.
    ///
    /// Always returns 4-band RGBA bytes (`tile_size * tile_size * 4` elements).
    /// Source bands are expanded to RGBA before upload regardless of source band count.
    pub fn crop_tile(
        &self,
        chunk: &ChunkBuffer,
        window: PixelWindow,
        tile_size: u32,
    ) -> crate::Result<Vec<u8>> {
        let bands = chunk.band_count();
        let src_w = window.width as u32;
        let src_h = window.height as u32;
        let row_off = window.row.saturating_sub(chunk.row_start);

        debug!(
            col = window.col,
            row = window.row,
            src_w,
            src_h,
            bands,
            tile_size,
            "gpu::crop_tile"
        );

        let rgba = build_rgba(chunk, window, row_off, bands)?;

        let src_tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("src"),
            size: wgpu::Extent3d {
                width: src_w,
                height: src_h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        self.queue.write_texture(
            src_tex.as_image_copy(),
            &rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(src_w * 4),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: src_w,
                height: src_h,
                depth_or_array_layers: 1,
            },
        );

        let out_pixels = tile_size * tile_size;
        let out_buf_size = (out_pixels * 4) as u64;
        let out_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("out_buf"),
            size: out_buf_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let dims_data: [u32; 4] = [tile_size, tile_size, 0, 0];
        let dims_bytes: Vec<u8> = dims_data.iter().flat_map(|v| v.to_le_bytes()).collect();
        let dims_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dims_uniform"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&dims_buf, 0, &dims_bytes);

        let src_view = src_tex.create_view(&Default::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("resize_bg"),
            layout: &self.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&src_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: out_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: dims_buf.as_entire_binding(),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("resize"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("resize"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            let wg = tile_size.div_ceil(WORKGROUP);
            pass.dispatch_workgroups(wg, wg, 1);
        }

        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging"),
            size: out_buf_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_buffer_to_buffer(&out_buf, 0, &staging, 0, out_buf_size);
        self.queue.submit([encoder.finish()]);

        let (tx, rx) = std::sync::mpsc::channel();
        staging.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| Error::Gpu(e.to_string()))?;
        rx.recv()
            .expect("map_async channel dropped")
            .map_err(|e| Error::Gpu(e.to_string()))?;

        let mapped = staging.slice(..).get_mapped_range();
        let mut out = Vec::with_capacity((out_pixels * 4) as usize);
        for chunk4 in mapped.chunks_exact(4) {
            let packed = u32::from_le_bytes([chunk4[0], chunk4[1], chunk4[2], chunk4[3]]);
            out.push((packed & 0xFF) as u8);
            out.push(((packed >> 8) & 0xFF) as u8);
            out.push(((packed >> 16) & 0xFF) as u8);
            out.push(((packed >> 24) & 0xFF) as u8);
        }
        drop(mapped);
        staging.unmap();

        Ok(out)
    }
}

fn build_rgba(
    chunk: &ChunkBuffer,
    window: PixelWindow,
    row_off: usize,
    bands: usize,
) -> crate::Result<Vec<u8>> {
    let n = window.width * window.height;
    let mut rgba = vec![0u8; n * 4];

    for row in 0..window.height {
        let chunk_row = row_off + row;
        for col in 0..window.width {
            let src = chunk_row * chunk.ds_width + window.col + col;
            let dst = (row * window.width + col) * 4;
            match bands {
                1 => {
                    let v = chunk.band_data[0][src];
                    rgba[dst] = v;
                    rgba[dst + 1] = v;
                    rgba[dst + 2] = v;
                    rgba[dst + 3] = 255;
                }
                2 => {
                    let l = chunk.band_data[0][src];
                    rgba[dst] = l;
                    rgba[dst + 1] = l;
                    rgba[dst + 2] = l;
                    rgba[dst + 3] = chunk.band_data[1][src];
                }
                3 => {
                    rgba[dst] = chunk.band_data[0][src];
                    rgba[dst + 1] = chunk.band_data[1][src];
                    rgba[dst + 2] = chunk.band_data[2][src];
                    rgba[dst + 3] = 255;
                }
                4 => {
                    rgba[dst] = chunk.band_data[0][src];
                    rgba[dst + 1] = chunk.band_data[1][src];
                    rgba[dst + 2] = chunk.band_data[2][src];
                    rgba[dst + 3] = chunk.band_data[3][src];
                }
                n => return Err(Error::BadBandCount(n)),
            }
        }
    }

    Ok(rgba)
}

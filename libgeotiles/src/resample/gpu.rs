//! GPU resampling via `wgpu` (feature `gpu`).
//!
//! The context (adapter, device, pipeline) is created once and shared by all worker
//! threads. Per tile, the kernel footprint of the source is expanded to RGBA, uploaded as
//! a texture, resampled by `shaders/resize.wgsl` (bilinear when upscaling, box-filtered
//! taps when downscaling) and read back. Output is always RGBA.
//!
//! A tile whose footprint exceeds the device's maximum texture size (only at very low
//! zooms of very large rasters) is handed to [`CpuResampler`] instead.
//!
//! Measured: crop/resize is a small share of a tile's cost next to encoding, so this
//! backend mostly moves a little CPU work to the GPU; it exists for completeness and
//! for machines with weak CPUs, not as the fast path.

use pollster::FutureExt as _;
use tracing::{debug, info};

use super::cpu::CpuResampler;
use super::{Resampler, TilePixels};
use crate::error::Error;
use crate::plan::TileJob;
use crate::raster::RowBlock;

const SHADER_SRC: &str = include_str!("shaders/resize.wgsl");
const WORKGROUP: u32 = 8;
/// The box filter's footprint reaches half the downscale factor past the source rect,
/// plus one texel for the bilinear taps; `support * scale + 1` covers that.
const SUPPORT: f64 = 1.0;

/// GPU resampler: one device context shared across all tiles of a run.
pub struct GpuResampler {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    max_texture_dim: u32,
}

impl GpuResampler {
    /// Initialise GPU adapter, device, queue, and compile the resize compute pipeline.
    ///
    /// Returns `Err(Error::Gpu)` when no suitable adapter is available (e.g. no
    /// Vulkan/Metal/DX12/GL).
    pub fn new() -> crate::Result<Self> {
        async {
            let instance = wgpu::Instance::default();
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    compatible_surface: None,
                    force_fallback_adapter: false,
                    // Native app, not exposing wgpu to untrusted web content — no need to
                    // bucket reported limits for fingerprinting mitigation.
                    apply_limit_buckets: false,
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

            let max_texture_dim = device.limits().max_texture_dimension_2d;
            info!(max_texture_dim, "GPU limits");

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
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                ..Default::default()
            });

            Ok(Self {
                device,
                queue,
                pipeline,
                bgl,
                sampler,
                max_texture_dim,
            })
        }
        .block_on()
    }

    /// The device's maximum 2-D texture dimension; larger footprints go to the CPU.
    pub fn max_texture_dim(&self) -> u32 {
        self.max_texture_dim
    }

    /// Resample on the GPU. `view` is the RGBA footprint `[c0, c1) × [r0, r1)`.
    fn run(
        &self,
        view: &[u8],
        view_w: u32,
        view_h: u32,
        crop: [f32; 4],
        job: &TileJob,
        tile_size: u32,
    ) -> crate::Result<Vec<u8>> {
        let size = wgpu::Extent3d {
            width: view_w,
            height: view_h,
            depth_or_array_layers: 1,
        };
        let src_tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("src"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            src_tex.as_image_copy(),
            view,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(view_w * 4),
                rows_per_image: None,
            },
            size,
        );

        let d = job.dst;
        let out_size = (d.width * d.height * 4) as u64;
        let out_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("out_buf"),
            size: out_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let mut params = Vec::with_capacity(32);
        for v in [d.width, d.height, 0, 0] {
            params.extend_from_slice(&v.to_le_bytes());
        }
        for v in crop {
            params.extend_from_slice(&v.to_le_bytes());
        }
        let params_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("params"),
            size: params.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&params_buf, 0, &params);

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
                    resource: params_buf.as_entire_binding(),
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
            pass.dispatch_workgroups(d.width.div_ceil(WORKGROUP), d.height.div_ceil(WORKGROUP), 1);
        }
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging"),
            size: out_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_buffer_to_buffer(&out_buf, 0, &staging, 0, out_size);
        self.queue.submit([encoder.finish()]);

        let (tx, rx) = std::sync::mpsc::channel();
        staging.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| Error::Gpu(e.to_string()))?;
        rx.recv()
            .map_err(|e| Error::Gpu(e.to_string()))?
            .map_err(|e| Error::Gpu(e.to_string()))?;

        let mapped = staging
            .slice(..)
            .get_mapped_range()
            .map_err(|e| Error::Gpu(e.to_string()))?;
        // Packed u32 little-endian is byte order R, G, B, A: rows copy straight in.
        let mut canvas = vec![0u8; (tile_size * tile_size * 4) as usize];
        let row_bytes = (d.width * 4) as usize;
        for row in 0..d.height as usize {
            let src = row * row_bytes;
            let dst = ((d.y as usize + row) * tile_size as usize + d.x as usize) * 4;
            canvas[dst..dst + row_bytes].copy_from_slice(&mapped[src..src + row_bytes]);
        }
        drop(mapped);
        staging.unmap();
        Ok(canvas)
    }
}

impl Resampler for GpuResampler {
    fn support(&self) -> f64 {
        // The CPU fallback needs the wider Lanczos footprint.
        CpuResampler.support().max(SUPPORT)
    }

    fn resample(
        &self,
        block: &RowBlock,
        job: &TileJob,
        tile_size: u32,
    ) -> crate::Result<TilePixels> {
        let (r0, r1) = job.rows_needed(SUPPORT, block.row_end());
        let r0 = r0.max(block.row_start);
        let (c0, c1) = job.cols_needed(SUPPORT, block.width);
        let (view_w, view_h) = ((c1 - c0) as u32, (r1 - r0) as u32);

        if view_w > self.max_texture_dim || view_h > self.max_texture_dim {
            debug!(view_w, view_h, tile = ?job.tile, "footprint exceeds GPU texture limit; using CPU");
            return CpuResampler.resample(block, job, tile_size);
        }

        let view = rgba_view(block, c0, c1, r0, r1)?;
        let crop = [
            (job.src.x - c0 as f64) as f32,
            (job.src.y - r0 as f64) as f32,
            job.src.width as f32,
            job.src.height as f32,
        ];
        Ok(TilePixels {
            data: self.run(&view, view_w, view_h, crop, job, tile_size)?,
            bands: 4,
        })
    }
}

/// Expand columns `[c0, c1)` of block rows `[r0, r1)` (absolute) to RGBA.
fn rgba_view(
    block: &RowBlock,
    c0: usize,
    c1: usize,
    r0: usize,
    r1: usize,
) -> crate::Result<Vec<u8>> {
    let bands = block.bands;
    let mut out = Vec::with_capacity((c1 - c0) * (r1 - r0) * 4);
    for row in r0..r1 {
        let start = ((row - block.row_start) * block.width + c0) * bands;
        let px = &block.data[start..start + (c1 - c0) * bands];
        match bands {
            1 => px
                .iter()
                .for_each(|&l| out.extend_from_slice(&[l, l, l, 255])),
            2 => px
                .as_chunks::<2>()
                .0
                .iter()
                .for_each(|p| out.extend_from_slice(&[p[0], p[0], p[0], p[1]])),
            3 => px
                .as_chunks::<3>()
                .0
                .iter()
                .for_each(|p| out.extend_from_slice(&[p[0], p[1], p[2], 255])),
            4 => out.extend_from_slice(px),
            n => return Err(Error::BadBandCount(n)),
        }
    }
    Ok(out)
}

//! A frame with backdrops is drawn into an offscreen texture. At each backdrop the
//! pass ends, what was drawn so far is downsampled to half size and blurred
//! horizontally then vertically, and a new pass draws the blurred image inside the
//! backdrop's rounded rectangle with its tint before carrying on. Finally the frame is
//! copied to the target. Frames without backdrops are drawn straight to the target.

use std::mem::size_of;

use bytemuck::{Pod, Zeroable};
use ui_scene::{Color, Rect};

const SHADER: &str = concat!(
    include_str!("shaders/common.wgsl"),
    include_str!("shaders/backdrop.wgsl")
);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Backdrop {
    pub rect: Rect,
    pub radius: f32,
    pub blur: f32,
    pub tint: Color,
}

/// Mirrors `Params` in `backdrop.wgsl`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Params {
    rect: [f32; 4],
    tint: [f32; 4],
    direction: [f32; 2],
    radius: f32,
    sigma: f32,
}

/// Uniform blocks per backdrop: horizontal blur, vertical blur, composite.
const BLOCKS: usize = 3;

struct Targets {
    width: u32,
    height: u32,
    frame: wgpu::TextureView,
    half: [wgpu::TextureView; 2],
    /// Bind groups sampling `frame`, `half[0]` and `half[1]`.
    sample_frame: wgpu::BindGroup,
    sample_half: [wgpu::BindGroup; 2],
}

pub(crate) struct BackdropPass {
    format: wgpu::TextureFormat,
    copy: wgpu::RenderPipeline,
    blur: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    params_layout: wgpu::BindGroupLayout,
    params: Option<(wgpu::Buffer, wgpu::BindGroup, usize)>,
    stride: u64,
    sampler: wgpu::Sampler,
    targets: Option<Targets>,
    /// Device pixels per logical pixel for the prepared frame.
    scale: f32,
    pub items: Vec<Backdrop>,
}

impl BackdropPass {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        globals_layout: &wgpu::BindGroupLayout,
        image_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("backdrop"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let params_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("backdrop params layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(size_of::<Params>() as u64),
                },
                count: None,
            }],
        });
        let plain = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("backdrop copy layout"),
            bind_group_layouts: &[Some(globals_layout), Some(image_layout)],
            ..Default::default()
        });
        let with_params = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("backdrop blur layout"),
            bind_group_layouts: &[
                Some(globals_layout),
                Some(image_layout),
                Some(&params_layout),
            ],
            ..Default::default()
        });
        let pipeline = |label: &str,
                        layout: &wgpu::PipelineLayout,
                        vertex: &str,
                        fragment: &str,
                        blend: Option<wgpu::BlendState>,
                        strip: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some(vertex),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(fragment),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: if strip {
                        wgpu::PrimitiveTopology::TriangleStrip
                    } else {
                        wgpu::PrimitiveTopology::TriangleList
                    },
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                cache: None,
                multiview_mask: None,
            })
        };
        let copy = pipeline("backdrop copy", &plain, "vs_full", "fs_copy", None, false);
        let blur = pipeline(
            "backdrop blur",
            &with_params,
            "vs_full",
            "fs_blur",
            None,
            false,
        );
        let composite = pipeline(
            "backdrop composite",
            &with_params,
            "vs_rect",
            "fs_composite",
            Some(wgpu::BlendState::ALPHA_BLENDING),
            true,
        );
        let align = u64::from(device.limits().min_uniform_buffer_offset_alignment);
        let stride = (size_of::<Params>() as u64).div_ceil(align) * align;
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("backdrop sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            format,
            copy,
            blur,
            composite,
            params_layout,
            params: None,
            stride,
            sampler,
            targets: None,
            scale: 1.0,
            items: Vec::new(),
        }
    }

    pub fn is_active(&self) -> bool {
        !self.items.is_empty()
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image_layout: &wgpu::BindGroupLayout,
        width: u32,
        height: u32,
        scale: f32,
    ) {
        if self.items.is_empty() {
            return;
        }
        self.ensure_targets(device, image_layout, width, height);
        self.scale = scale;
        let needed = self.items.len() * BLOCKS;
        if self.params.as_ref().is_none_or(|(_, _, cap)| *cap < needed) {
            let capacity = needed.next_power_of_two();
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("backdrop params"),
                size: self.stride * capacity as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("backdrop params"),
                layout: &self.params_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(size_of::<Params>() as u64),
                    }),
                }],
            });
            self.params = Some((buffer, bind_group, capacity));
        }
        let (half_w, half_h) = (width.div_ceil(2).max(1), height.div_ceil(2).max(1));
        let mut data = vec![0u8; self.stride as usize * needed];
        for (i, item) in self.items.iter().enumerate() {
            let sigma = sigma(item.blur, scale);
            let base = Params {
                rect: [item.rect.x, item.rect.y, item.rect.width, item.rect.height],
                tint: item.tint.to_array(),
                direction: [0.0, 0.0],
                radius: item.radius,
                sigma,
            };
            let blocks = [
                Params {
                    direction: [1.0 / half_w as f32, 0.0],
                    ..base
                },
                Params {
                    direction: [0.0, 1.0 / half_h as f32],
                    ..base
                },
                base,
            ];
            for (b, block) in blocks.iter().enumerate() {
                let at = (i * BLOCKS + b) * self.stride as usize;
                data[at..at + size_of::<Params>()].copy_from_slice(bytemuck::bytes_of(block));
            }
        }
        let (buffer, _, _) = self.params.as_ref().expect("created above");
        queue.write_buffer(buffer, 0, &data);
    }

    fn ensure_targets(
        &mut self,
        device: &wgpu::Device,
        image_layout: &wgpu::BindGroupLayout,
        width: u32,
        height: u32,
    ) {
        if self
            .targets
            .as_ref()
            .is_some_and(|t| t.width == width && t.height == height)
        {
            return;
        }
        let texture = |label: &str, w: u32, h: u32| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: w.max(1),
                        height: h.max(1),
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: self.format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let (half_w, half_h) = (width.div_ceil(2), height.div_ceil(2));
        let frame = texture("backdrop frame", width, height);
        let half = [
            texture("backdrop half a", half_w, half_h),
            texture("backdrop half b", half_w, half_h),
        ];
        let sample = |view: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("backdrop sample"),
                layout: image_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            })
        };
        let sample_frame = sample(&frame);
        let sample_half = [sample(&half[0]), sample(&half[1])];
        self.targets = Some(Targets {
            width,
            height,
            frame,
            half,
            sample_frame,
            sample_half,
        });
    }

    pub fn frame(&self) -> &wgpu::TextureView {
        &self.targets.as_ref().expect("prepared").frame
    }

    /// Blur the frame drawn so far and draw backdrop `index` over it. The caller has
    /// ended its pass on the frame and starts a new one (loading) afterwards.
    pub fn apply(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        globals: &wgpu::BindGroup,
        index: usize,
        scissor: (u32, u32, u32, u32),
    ) {
        let targets = self.targets.as_ref().expect("prepared");
        let Some(region) = self.blur_region(targets, index, scissor) else {
            return;
        };
        let (_, params, _) = self.params.as_ref().expect("prepared");
        let offset = |block: usize| ((index * BLOCKS + block) as u64 * self.stride) as u32;
        let pass = begin;
        let clear = wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT);
        {
            let mut p = pass(encoder, &targets.half[0], clear);
            p.set_scissor_rect(region.0, region.1, region.2, region.3);
            p.set_pipeline(&self.copy);
            p.set_bind_group(0, globals, &[]);
            p.set_bind_group(1, &targets.sample_frame, &[]);
            p.draw(0..3, 0..1);
        }
        for (block, (from, to)) in [(0usize, (0usize, 1usize)), (1, (1, 0))] {
            let mut p = pass(encoder, &targets.half[to], clear);
            p.set_scissor_rect(region.0, region.1, region.2, region.3);
            p.set_pipeline(&self.blur);
            p.set_bind_group(0, globals, &[]);
            p.set_bind_group(1, &targets.sample_half[from], &[]);
            p.set_bind_group(2, params, &[offset(block)]);
            p.draw(0..3, 0..1);
        }
        let mut p = pass(encoder, &targets.frame, wgpu::LoadOp::Load);
        p.set_scissor_rect(scissor.0, scissor.1, scissor.2, scissor.3);
        p.set_pipeline(&self.composite);
        p.set_bind_group(0, globals, &[]);
        p.set_bind_group(1, &targets.sample_half[0], &[]);
        p.set_bind_group(2, params, &[offset(2)]);
        p.draw(0..4, 0..1);
    }

    /// The part of the half-size targets that the blur of backdrop `index` must cover
    /// so its composite inside `scissor` (device px) reads correct values: the visible
    /// backdrop plus two blur reaches (one per axis) and a texel for bilinear
    /// sampling. `None` when nothing of the backdrop is visible.
    fn blur_region(
        &self,
        targets: &Targets,
        index: usize,
        scissor: (u32, u32, u32, u32),
    ) -> Option<(u32, u32, u32, u32)> {
        let item = &self.items[index];
        let s = self.scale;
        let (x0, y0) = (
            (item.rect.x * s).max(scissor.0 as f32),
            (item.rect.y * s).max(scissor.1 as f32),
        );
        let (x1, y1) = (
            ((item.rect.x + item.rect.width) * s).min((scissor.0 + scissor.2) as f32),
            ((item.rect.y + item.rect.height) * s).min((scissor.1 + scissor.3) as f32),
        );
        if x0 >= x1 || y0 >= y1 {
            return None;
        }
        let margin = 2.0 * reach(sigma(item.blur, s)) + 2.0;
        let (half_w, half_h) = (targets.width.div_ceil(2), targets.height.div_ceil(2));
        let left = (x0 / 2.0 - margin).floor().max(0.0) as u32;
        let top = (y0 / 2.0 - margin).floor().max(0.0) as u32;
        let right = ((x1 / 2.0 + margin).ceil() as u32).min(half_w);
        let bottom = ((y1 / 2.0 + margin).ceil() as u32).min(half_h);
        (left < right && top < bottom).then(|| (left, top, right - left, bottom - top))
    }

    pub fn blit(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        globals: &wgpu::BindGroup,
        view: &wgpu::TextureView,
    ) {
        let targets = self.targets.as_ref().expect("prepared");
        let mut p = begin(encoder, view, wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT));
        p.set_pipeline(&self.copy);
        p.set_bind_group(0, globals, &[]);
        p.set_bind_group(1, &targets.sample_frame, &[]);
        p.draw(0..3, 0..1);
    }
}

/// Gaussian sigma in half-size texels for a blur of `blur` logical px: a blur
/// "radius" covers about two standard deviations.
fn sigma(blur: f32, scale: f32) -> f32 {
    (blur * scale / 2.0 / 2.0).clamp(0.5, 12.0)
}

/// Taps on each side of a blur with `sigma`; mirrors `fs_blur` in `backdrop.wgsl`.
fn reach(sigma: f32) -> f32 {
    (sigma.max(0.5) * 2.5).ceil().min(32.0)
}

fn begin<'e>(
    encoder: &'e mut wgpu::CommandEncoder,
    target: &wgpu::TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
) -> wgpu::RenderPass<'e> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("backdrop"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

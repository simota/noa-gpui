use std::fmt;
use std::mem::size_of;
use std::ops::Range;

use glyphon::{
    Cache, ColorMode, Resolution, SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer,
};
use noa_gpui_scene::{
    BatchKind, Batcher, Color, Direction, DisplayList, DrawCommand, ImageId, Rect, Shape, Size,
};
use noa_gpui_text::TextStore;

use crate::backdrop::{Backdrop, BackdropPass};
use crate::buffers::{Globals, IconInstance, ImageInstance, InstanceBuffer, QuadInstance};
use crate::icons::IconAtlas;
use crate::images::ImageStore;
use crate::pipelines::Pipelines;

/// The render target's size in physical pixels and its scale factor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
    /// Physical pixels per logical pixel.
    pub scale_factor: f32,
}

impl Viewport {
    pub fn logical_size(&self) -> Size {
        Size::new(
            self.width as f32 / self.scale_factor,
            self.height as f32 / self.scale_factor,
        )
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameStats {
    /// The display list, text layouts and viewport were unchanged, so the previous
    /// frame's batches and GPU data were reused as they were.
    pub cached: bool,
    /// GPU buffers allocated by this frame (0 in steady state).
    pub buffer_allocations: usize,
    pub commands: usize,
    pub culled: usize,
    pub batches: usize,
    pub quads: usize,
    pub images: usize,
    pub icons: usize,
    pub texts: usize,
}

#[derive(Debug)]
pub enum RenderError {
    TextPrepare(glyphon::PrepareError),
    TextRender(glyphon::RenderError),
    /// Reading a frame back from the GPU failed (offscreen rendering).
    Readback(String),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenderError::TextPrepare(e) => write!(f, "text prepare failed: {e}"),
            RenderError::TextRender(e) => write!(f, "text render failed: {e}"),
            RenderError::Readback(e) => write!(f, "reading the frame back failed: {e}"),
        }
    }
}

impl std::error::Error for RenderError {}

/// Scissor rectangle in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Scissor {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone)]
enum DrawOp {
    Quads(Range<u32>),
    Images(ImageId, Range<u32>),
    Icons(Range<u32>),
    Text(usize),
    /// Index into the backdrop list (drawn by [`Renderer::encode`] only).
    Backdrop(usize),
}

struct TextState {
    atlas: TextAtlas,
    viewport: glyphon::Viewport,
    swash: SwashCache,
    /// One renderer per text batch, so text interleaves correctly with other batches.
    renderers: Vec<TextRenderer>,
}

pub struct Renderer {
    format: wgpu::TextureFormat,
    srgb_output: bool,
    pipelines: Pipelines,
    globals: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
    images: ImageStore,
    icons: IconAtlas,
    text: TextState,

    batcher: Batcher,
    quad_data: Vec<QuadInstance>,
    image_data: Vec<ImageInstance>,
    icon_data: Vec<IconInstance>,
    quad_buffer: InstanceBuffer,
    image_buffer: InstanceBuffer,
    icon_buffer: InstanceBuffer,
    ops: Vec<(Scissor, DrawOp)>,
    backdrop: BackdropPass,
    stats: FrameStats,
    /// What the current `ops` and GPU data were prepared from.
    prepared: Option<PreparedKey>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PreparedKey {
    list: u64,
    text: u64,
    viewport: Viewport,
}

impl Renderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let srgb_output = format.is_srgb();
        let pipelines = Pipelines::new(device, format);

        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals bind group"),
            layout: &pipelines.globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });

        let cache = Cache::new(device);
        let color_mode = if srgb_output {
            ColorMode::Accurate
        } else {
            ColorMode::Web
        };
        let text = TextState {
            atlas: TextAtlas::with_color_mode(device, queue, &cache, format, color_mode),
            viewport: glyphon::Viewport::new(device, &cache),
            swash: SwashCache::new(),
            renderers: Vec::new(),
        };

        let backdrop = BackdropPass::new(
            device,
            format,
            &pipelines.globals_layout,
            &pipelines.image_layout,
        );
        Self {
            format,
            srgb_output,
            images: ImageStore::new(device, srgb_output),
            icons: IconAtlas::new(device, &pipelines.image_layout),
            backdrop,
            pipelines,
            globals,
            globals_bind_group,
            text,
            batcher: Batcher::new(),
            quad_data: Vec::new(),
            image_data: Vec::new(),
            icon_data: Vec::new(),
            quad_buffer: InstanceBuffer::new("quad instances"),
            image_buffer: InstanceBuffer::new("image instances"),
            icon_buffer: InstanceBuffer::new("icon instances"),
            ops: Vec::new(),
            stats: FrameStats::default(),
            prepared: None,
        }
    }

    /// Upload a tightly packed, sRGB-encoded RGBA8 image.
    pub fn create_image(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> ImageId {
        self.images.create(
            device,
            queue,
            &self.pipelines.image_layout,
            width,
            height,
            rgba,
        )
    }

    pub fn image_size(&self, id: ImageId) -> (u32, u32) {
        let image = self.images.get(id);
        (image.width, image.height)
    }

    pub fn stats(&self) -> FrameStats {
        self.stats
    }

    /// Must run before [`Renderer::render`] for each frame.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        list: &DisplayList,
        text_store: &mut TextStore,
        viewport: Viewport,
    ) -> Result<(), RenderError> {
        let key = PreparedKey {
            list: list.revision(),
            text: text_store.layouts().revision(),
            viewport,
        };
        if self.prepared == Some(key) {
            self.stats.cached = true;
            self.stats.buffer_allocations = 0;
            return Ok(());
        }
        // A failure below leaves partial state; never reuse it.
        self.prepared = None;
        if !self.prepare_frame(device, queue, list, text_store, viewport)? {
            // The icon atlas filled up: start it over with this frame's icons only.
            self.icons.clear();
            if !self.prepare_frame(device, queue, list, text_store, viewport)? {
                log::warn!("the icons of one frame do not fit the icon atlas");
            }
        }
        self.prepared = Some(key);
        Ok(())
    }

    /// Returns `false` if an icon did not fit the atlas (it is then not drawn).
    fn prepare_frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        list: &DisplayList,
        text_store: &mut TextStore,
        viewport: Viewport,
    ) -> Result<bool, RenderError> {
        let logical = viewport.logical_size();
        queue.write_buffer(
            &self.globals,
            0,
            bytemuck::bytes_of(&Globals {
                viewport: [logical.width, logical.height],
                scale: viewport.scale_factor,
                srgb_output: self.srgb_output as u32,
            }),
        );

        let Self {
            batcher,
            quad_data,
            image_data,
            icon_data,
            icons,
            ops,
            text,
            stats,
            backdrop,
            ..
        } = self;
        quad_data.clear();
        image_data.clear();
        icon_data.clear();
        ops.clear();
        backdrop.items.clear();
        *stats = FrameStats {
            commands: list.len(),
            ..FrameStats::default()
        };

        text.viewport.update(
            queue,
            Resolution {
                width: viewport.width,
                height: viewport.height,
            },
        );
        let (font_system, layouts) = text_store.split_mut();
        let commands = list.commands();
        let full = Rect::new(0.0, 0.0, logical.width, logical.height);
        let mut text_batches = 0;
        let mut icons_fit = true;

        for batch in batcher.build(list, full) {
            let Some(scissor) = to_scissor(batch.clip.unwrap_or(full), viewport) else {
                continue;
            };
            let items = batch.commands().iter().map(|&i| &commands[i as usize]);
            let op = match batch.kind {
                BatchKind::Quad => {
                    let start = quad_data.len() as u32;
                    for command in items {
                        quad_data.push(match command {
                            DrawCommand::Quad {
                                rect,
                                radius,
                                color,
                                border,
                            } => QuadInstance {
                                rect: rect_array(rect),
                                radii: *radius,
                                color: color.to_array(),
                                border_color: border.color.to_array(),
                                border_width: border.width,
                                shape: 0.0,
                                shape_param: 0.0,
                                _pad: 0.0,
                            },
                            DrawCommand::Shape { shape, color } => shape_instance(shape, *color),
                            _ => unreachable!("quad batch holds only quads and shapes"),
                        });
                    }
                    DrawOp::Quads(start..quad_data.len() as u32)
                }
                BatchKind::Image(id) => {
                    let start = image_data.len() as u32;
                    for command in items {
                        let DrawCommand::Image { rect, .. } = command else {
                            unreachable!("image batch holds only images")
                        };
                        image_data.push(ImageInstance {
                            rect: rect_array(rect),
                            uv: [0.0, 0.0, 1.0, 1.0],
                        });
                    }
                    DrawOp::Images(id, start..image_data.len() as u32)
                }
                BatchKind::Icon => {
                    let start = icon_data.len() as u32;
                    let s = viewport.scale_factor;
                    for command in items {
                        let DrawCommand::Icon { rect, icon, color } = command else {
                            unreachable!("icon batch holds only icons")
                        };
                        // Whole physical pixels, so the mask maps one texel per pixel.
                        let x0 = (rect.x * s).round();
                        let y0 = (rect.y * s).round();
                        let w = (rect.width * s).round().max(1.0);
                        let h = (rect.height * s).round().max(1.0);
                        let Some(uv) = icons.mask(queue, icon, w as u32, h as u32) else {
                            icons_fit = false;
                            continue;
                        };
                        icon_data.push(IconInstance {
                            rect: [x0 / s, y0 / s, w / s, h / s],
                            uv,
                            color: color.to_array(),
                        });
                    }
                    DrawOp::Icons(start..icon_data.len() as u32)
                }
                BatchKind::Text => {
                    let bounds = TextBounds {
                        left: scissor.x as i32,
                        top: scissor.y as i32,
                        right: (scissor.x + scissor.width) as i32,
                        bottom: (scissor.y + scissor.height) as i32,
                    };
                    let scale = viewport.scale_factor;
                    let areas = items.map(|command| {
                        let DrawCommand::Text {
                            rect,
                            layout,
                            color,
                        } = command
                        else {
                            unreachable!("text batch holds only text")
                        };
                        stats.texts += 1;
                        TextArea {
                            buffer: layouts.buffer(*layout),
                            left: rect.x * scale,
                            top: rect.y * scale,
                            scale,
                            bounds,
                            default_color: glyphon_color(*color),
                            custom_glyphs: &[],
                        }
                    });
                    if text.renderers.len() == text_batches {
                        stats.buffer_allocations += 1;
                        text.renderers.push(TextRenderer::new(
                            &mut text.atlas,
                            device,
                            wgpu::MultisampleState::default(),
                            None,
                        ));
                    }
                    text.renderers[text_batches]
                        .prepare(
                            device,
                            queue,
                            font_system,
                            &mut text.atlas,
                            &text.viewport,
                            areas,
                            &mut text.swash,
                        )
                        .map_err(RenderError::TextPrepare)?;
                    text_batches += 1;
                    DrawOp::Text(text_batches - 1)
                }
                BatchKind::Backdrop => {
                    let Some(DrawCommand::Backdrop {
                        rect,
                        radius,
                        blur,
                        tint,
                    }) = items.into_iter().next()
                    else {
                        unreachable!("backdrop batch holds one backdrop")
                    };
                    backdrop.items.push(Backdrop {
                        rect: *rect,
                        radius: *radius,
                        blur: *blur,
                        tint: *tint,
                    });
                    DrawOp::Backdrop(backdrop.items.len() - 1)
                }
            };
            ops.push((scissor, op));
        }

        stats.culled = batcher.culled();
        stats.batches = ops.len();
        stats.quads = quad_data.len();
        stats.images = image_data.len();
        stats.icons = icon_data.len();

        let allocations = [
            self.quad_buffer
                .upload(device, queue, bytemuck::cast_slice(&self.quad_data)),
            self.image_buffer
                .upload(device, queue, bytemuck::cast_slice(&self.image_data)),
            self.icon_buffer
                .upload(device, queue, bytemuck::cast_slice(&self.icon_data)),
        ];
        self.stats.buffer_allocations += allocations.into_iter().filter(|&a| a).count();
        self.backdrop.prepare(
            device,
            queue,
            &self.pipelines.image_layout,
            viewport.width,
            viewport.height,
            viewport.scale_factor,
        );
        Ok(icons_fit)
    }

    /// Release glyph atlas memory (for example, on a low-memory warning). The atlas
    /// restarts at its smallest size and refills with the glyphs the next frames use;
    /// icon masks are rasterized again as they are drawn.
    pub fn trim_caches(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let color_mode = if self.srgb_output {
            ColorMode::Accurate
        } else {
            ColorMode::Web
        };
        let cache = Cache::new(device);
        self.text = TextState {
            atlas: TextAtlas::with_color_mode(device, queue, &cache, self.format, color_mode),
            viewport: glyphon::Viewport::new(device, &cache),
            swash: SwashCache::new(),
            renderers: Vec::new(),
        };
        self.icons.clear();
        self.prepared = None;
    }

    /// Record the prepared frame into `pass`. Backdrops need passes of their own and
    /// are skipped here; [`Renderer::encode`] draws them.
    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>) -> Result<(), RenderError> {
        self.render_ops(pass, &self.ops)
    }

    fn render_ops(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        ops: &[(Scissor, DrawOp)],
    ) -> Result<(), RenderError> {
        for (scissor, op) in ops {
            pass.set_scissor_rect(scissor.x, scissor.y, scissor.width, scissor.height);
            match op {
                DrawOp::Quads(range) => {
                    pass.set_pipeline(&self.pipelines.quad);
                    pass.set_bind_group(0, &self.globals_bind_group, &[]);
                    pass.set_vertex_buffer(
                        0,
                        self.quad_buffer
                            .slice(range.clone(), size_of::<QuadInstance>() as u64),
                    );
                    pass.draw(0..4, 0..range.len() as u32);
                }
                DrawOp::Images(id, range) => {
                    pass.set_pipeline(&self.pipelines.image);
                    pass.set_bind_group(0, &self.globals_bind_group, &[]);
                    pass.set_bind_group(1, &self.images.get(*id).bind_group, &[]);
                    pass.set_vertex_buffer(
                        0,
                        self.image_buffer
                            .slice(range.clone(), size_of::<ImageInstance>() as u64),
                    );
                    pass.draw(0..4, 0..range.len() as u32);
                }
                DrawOp::Icons(range) => {
                    pass.set_pipeline(&self.pipelines.icon);
                    pass.set_bind_group(0, &self.globals_bind_group, &[]);
                    pass.set_bind_group(1, &self.icons.bind_group, &[]);
                    pass.set_vertex_buffer(
                        0,
                        self.icon_buffer
                            .slice(range.clone(), size_of::<IconInstance>() as u64),
                    );
                    pass.draw(0..4, 0..range.len() as u32);
                }
                DrawOp::Text(index) => self.text.renderers[*index]
                    .render(&self.text.atlas, &self.text.viewport, pass)
                    .map_err(RenderError::TextRender)?,
                DrawOp::Backdrop(_) => {}
            }
        }
        Ok(())
    }

    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        clear: Color,
    ) -> Result<(), RenderError> {
        if !self.backdrop.is_active() {
            let mut pass =
                self.begin_pass(encoder, view, wgpu::LoadOp::Clear(self.clear_color(clear)));
            return self.render(&mut pass);
        }
        let frame = self.backdrop.frame();
        let mut load = wgpu::LoadOp::Clear(self.clear_color(clear));
        let mut rest: &[(Scissor, DrawOp)] = &self.ops;
        loop {
            let split = rest
                .iter()
                .position(|(_, op)| matches!(op, DrawOp::Backdrop(_)))
                .unwrap_or(rest.len());
            {
                let mut pass = self.begin_pass(encoder, frame, load);
                self.render_ops(&mut pass, &rest[..split])?;
            }
            load = wgpu::LoadOp::Load;
            let Some((scissor, DrawOp::Backdrop(index))) = rest.get(split) else {
                break;
            };
            let s = (scissor.x, scissor.y, scissor.width, scissor.height);
            self.backdrop
                .apply(encoder, &self.globals_bind_group, *index, s);
            rest = &rest[split + 1..];
        }
        self.backdrop.blit(encoder, &self.globals_bind_group, view);
        Ok(())
    }

    fn begin_pass<'e>(
        &self,
        encoder: &'e mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        load: wgpu::LoadOp<wgpu::Color>,
    ) -> wgpu::RenderPass<'e> {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ui pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
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

    /// Call after submitting.
    pub fn end_frame(&mut self) {
        self.text.atlas.trim();
    }

    fn clear_color(&self, color: Color) -> wgpu::Color {
        let channel = |c: f32| {
            let c = c as f64;
            if !self.srgb_output {
                c
            } else if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        wgpu::Color {
            r: channel(color.r),
            g: channel(color.g),
            b: channel(color.b),
            a: color.a as f64,
        }
    }
}

fn rect_array(rect: &Rect) -> [f32; 4] {
    [rect.x, rect.y, rect.width, rect.height]
}

fn glyphon_color(color: Color) -> glyphon::Color {
    let [r, g, b, a] = color.to_rgba8();
    glyphon::Color::rgba(r, g, b, a)
}

/// Convert a logical clip rectangle to whole physical pixels inside the target.
fn to_scissor(rect: Rect, viewport: Viewport) -> Option<Scissor> {
    let s = viewport.scale_factor;
    let x0 = (rect.x * s).floor().max(0.0) as u32;
    let y0 = (rect.y * s).floor().max(0.0) as u32;
    let x1 = ((rect.right() * s).ceil().max(0.0) as u32).min(viewport.width);
    let y1 = ((rect.bottom() * s).ceil().max(0.0) as u32).min(viewport.height);
    (x1 > x0 && y1 > y0).then(|| Scissor {
        x: x0,
        y: y0,
        width: x1 - x0,
        height: y1 - y0,
    })
}

/// A [`Shape`] as a quad instance: the quad covers the shape's bounds and the fragment
/// shader evaluates the shape's distance field.
fn shape_instance(shape: &Shape, color: noa_gpui_scene::Color) -> QuadInstance {
    let bounds = shape.bounds();
    let mut end = [0.0; 4];
    let (kind, radii, half_width, param) = match *shape {
        Shape::Line { from, to, width } => (1.0, [from.x, from.y, to.x, to.y], width / 2.0, 0.0),
        Shape::Star { outline, .. } => (4.0, [0.0; 4], outline / 2.0, 0.0),
        Shape::Arc {
            radius,
            width,
            sweep,
            ..
        } => (5.0, [radius, 0.0, 0.0, 0.0], width / 2.0, sweep),
        Shape::Gradient {
            radius,
            end: to,
            vertical,
            ..
        } => {
            end = to.to_array();
            (3.0, [radius; 4], 0.0, if vertical { 1.0 } else { 0.0 })
        }
        Shape::Triangle { direction, .. } => {
            let direction = match direction {
                Direction::Up => 0.0,
                Direction::Down => 1.0,
                Direction::Left => 2.0,
                Direction::Right => 3.0,
            };
            (2.0, [0.0; 4], 0.0, direction)
        }
    };
    QuadInstance {
        rect: rect_array(&bounds),
        radii,
        color: color.to_array(),
        border_color: end,
        border_width: half_width,
        shape: kind,
        shape_param: param,
        _pad: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEWPORT: Viewport = Viewport {
        width: 200,
        height: 100,
        scale_factor: 2.0,
    };

    #[test]
    fn scissor_scales_and_rounds_outward() {
        let s = to_scissor(Rect::new(10.25, 5.5, 20.0, 10.0), VIEWPORT).unwrap();
        assert_eq!(
            s,
            Scissor {
                x: 20,
                y: 11,
                width: 41,
                height: 20
            }
        );
    }

    #[test]
    fn scissor_is_clamped_to_target() {
        let s = to_scissor(Rect::new(-10.0, -10.0, 1000.0, 1000.0), VIEWPORT).unwrap();
        assert_eq!(
            s,
            Scissor {
                x: 0,
                y: 0,
                width: 200,
                height: 100
            }
        );
        assert_eq!(
            to_scissor(Rect::new(150.0, 0.0, 10.0, 10.0), VIEWPORT),
            None
        );
    }
}

use noa_gpui_scene::{Color, DisplayList};
use noa_gpui_text::TextStore;

use crate::gpu::{InitError, required_limits};
use crate::renderer::{RenderError, Renderer, Viewport};

pub const OFFSCREEN_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// Pixels read back from the GPU: RGBA8 in sRGB, rows top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl RgbaImage {
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        self.pixels[i..i + 4].try_into().expect("four channels")
    }

    /// Uncompressed.
    pub fn to_png(&self) -> Vec<u8> {
        crate::png::encode(self.width, self.height, &self.pixels)
    }

    pub fn save_png(&self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        std::fs::write(path, self.to_png())
    }
}

pub struct OffscreenRenderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub renderer: Renderer,
}

impl OffscreenRenderer {
    /// Fails with [`InitError::Adapter`] where there is no GPU (a CI runner or sandbox
    /// without one).
    pub async fn new() -> Result<Self, InitError> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle().with_env());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .map_err(InitError::Adapter)?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("noa-gpui offscreen device"),
                required_limits: required_limits(&adapter),
                ..Default::default()
            })
            .await
            .map_err(InitError::Device)?;
        let renderer = Renderer::new(&device, &queue, OFFSCREEN_FORMAT);
        Ok(Self {
            device,
            queue,
            renderer,
        })
    }

    /// Draws at the physical size and scale of `viewport` and waits for the pixels.
    pub fn render(
        &mut self,
        list: &DisplayList,
        text: &mut TextStore,
        viewport: Viewport,
        background: Color,
    ) -> Result<RgbaImage, RenderError> {
        let Self {
            device,
            queue,
            renderer,
        } = self;
        let (width, height) = (viewport.width.max(1), viewport.height.max(1));
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen frame"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: OFFSCREEN_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        renderer.prepare(device, queue, list, text, viewport)?;

        // Copies need rows padded to 256 bytes.
        let row = width * 4;
        let padded =
            row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("offscreen readback"),
            size: u64::from(padded) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        renderer.encode(&mut encoder, &view, background)?;
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(height),
                },
            },
            size,
        );
        queue.submit([encoder.finish()]);
        renderer.end_frame();

        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| RenderError::Readback(e.to_string()))?;
        let mapped = slice
            .get_mapped_range()
            .map_err(|e| RenderError::Readback(e.to_string()))?;
        let mut pixels = Vec::with_capacity((row * height) as usize);
        for line in mapped.chunks_exact(padded as usize) {
            pixels.extend_from_slice(&line[..row as usize]);
        }
        Ok(RgbaImage {
            width,
            height,
            pixels,
        })
    }
}

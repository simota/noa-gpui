use bytemuck::{Pod, Zeroable};

/// Mirrors `Globals` in `common.wgsl`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct Globals {
    pub viewport: [f32; 2],
    pub scale: f32,
    pub srgb_output: u32,
}

/// Mirrors `Instance` in `quad.wgsl`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct QuadInstance {
    pub rect: [f32; 4],
    pub radii: [f32; 4],
    pub color: [f32; 4],
    pub border_color: [f32; 4],
    pub border_width: f32,
    /// 0: rounded box, 1: line (`radii` = from.xy, to.xy; `border_width` = half width),
    /// 2: triangle (`shape_param` = direction 0 up, 1 down, 2 left, 3 right),
    /// 3: gradient box from `color` to `border_color` (`shape_param` 1 = vertical),
    /// 4: star (`border_width` = half the outline width, 0 = filled),
    /// 5: arc (`radii.x` = radius, `border_width` = half width, `shape_param` = sweep).
    pub shape: f32,
    pub shape_param: f32,
    pub _pad: f32,
}

/// Mirrors `Instance` in `image.wgsl`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct ImageInstance {
    pub rect: [f32; 4],
    pub uv: [f32; 4],
}

/// Mirrors `Instance` in `icon.wgsl`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct IconInstance {
    pub rect: [f32; 4],
    pub uv: [f32; 4],
    pub color: [f32; 4],
}

/// A vertex buffer that grows to the next power of two when a frame needs more space
/// and is otherwise rewritten in place, so steady-state frames allocate nothing.
pub(crate) struct InstanceBuffer {
    label: &'static str,
    buffer: Option<wgpu::Buffer>,
}

impl InstanceBuffer {
    pub fn new(label: &'static str) -> Self {
        Self {
            label,
            buffer: None,
        }
    }

    /// Returns whether a new GPU buffer was allocated.
    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, bytes: &[u8]) -> bool {
        if bytes.is_empty() {
            return false;
        }
        let needed = bytes.len() as u64;
        let grow = self.buffer.as_ref().is_none_or(|b| b.size() < needed);
        if grow {
            self.buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(self.label),
                size: needed.next_power_of_two().max(4096),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        queue.write_buffer(self.buffer.as_ref().expect("allocated above"), 0, bytes);
        grow
    }

    /// `range` is in instances of `stride` bytes.
    /// Slices by byte offset because WebGL2 lacks `first_instance`.
    pub fn slice(&self, range: std::ops::Range<u32>, stride: u64) -> wgpu::BufferSlice<'_> {
        let buffer = self.buffer.as_ref().expect("slice of an empty buffer");
        buffer.slice(range.start as u64 * stride..range.end as u64 * stride)
    }
}

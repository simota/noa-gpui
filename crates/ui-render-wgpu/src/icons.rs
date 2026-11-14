//! Icon masks are rasterized once per pixel size and packed into one texture so that
//! every icon draws in the same batch.

use std::collections::HashMap;

use ui_scene::VectorIcon;

/// Capped at the device's texture limit.
const ATLAS_SIZE: u32 = 1024;
/// Empty texels around each mask, so filtering never reads a neighbor.
const PADDING: u32 = 1;

/// Texture coordinates (u0, v0, u1, v1).
pub(crate) type Uv = [f32; 4];

/// (icon id, width, height) in pixels.
type Key = (u64, u32, u32);

pub(crate) struct IconAtlas {
    texture: wgpu::Texture,
    size: u32,
    pub bind_group: wgpu::BindGroup,
    masks: HashMap<Key, Uv>,
    shelf_y: u32,
    shelf_height: u32,
    cursor_x: u32,
}

impl IconAtlas {
    pub fn new(device: &wgpu::Device, layout: &wgpu::BindGroupLayout) -> Self {
        let size = ATLAS_SIZE.min(device.limits().max_texture_dimension_2d);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("icon atlas"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("icon sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("icon atlas bind group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        Self {
            texture,
            size,
            bind_group,
            masks: HashMap::new(),
            shelf_y: 0,
            shelf_height: 0,
            cursor_x: 0,
        }
    }

    /// Leaves the texels in place; masks added later overwrite them.
    pub fn clear(&mut self) {
        self.masks.clear();
        self.shelf_y = 0;
        self.shelf_height = 0;
        self.cursor_x = 0;
    }

    /// Rasterizes and uploads the mask on first use. `None` when the atlas is full.
    pub fn mask(
        &mut self,
        queue: &wgpu::Queue,
        icon: &VectorIcon,
        width: u32,
        height: u32,
    ) -> Option<Uv> {
        // Larger icons are drawn from a mask half the atlas wide, scaled up.
        let max = self.size / 2;
        let (width, height) = (width.clamp(1, max), height.clamp(1, max));
        let key = (icon.id(), width, height);
        if let Some(uv) = self.masks.get(&key) {
            return Some(*uv);
        }
        let (block_w, block_h) = (width + 2 * PADDING, height + 2 * PADDING);
        let (bx, by) = self.allocate(block_w, block_h)?;
        // Upload the padding too, as zeros: after a clear the texels there may hold
        // part of an older mask.
        let mask = icon.rasterize(width, height);
        let mut block = vec![0u8; (block_w * block_h) as usize];
        for row in 0..height as usize {
            let start = (row + PADDING as usize) * block_w as usize + PADDING as usize;
            block[start..start + width as usize]
                .copy_from_slice(&mask[row * width as usize..(row + 1) * width as usize]);
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: bx, y: by, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &block,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(block_w),
                rows_per_image: Some(block_h),
            },
            wgpu::Extent3d {
                width: block_w,
                height: block_h,
                depth_or_array_layers: 1,
            },
        );
        let (x, y) = (bx + PADDING, by + PADDING);
        let size = self.size as f32;
        let uv = [
            x as f32 / size,
            y as f32 / size,
            (x + width) as f32 / size,
            (y + height) as f32 / size,
        ];
        self.masks.insert(key, uv);
        Some(uv)
    }

    /// Top-left corner of a free `width` × `height` block (padding included).
    fn allocate(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        if self.cursor_x + width > self.size {
            self.shelf_y += self.shelf_height;
            self.shelf_height = 0;
            self.cursor_x = 0;
        }
        if self.shelf_y + height > self.size {
            return None;
        }
        let at = (self.cursor_x, self.shelf_y);
        self.cursor_x += width;
        self.shelf_height = self.shelf_height.max(height);
        Some(at)
    }
}

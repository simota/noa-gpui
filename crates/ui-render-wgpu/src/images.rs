use ui_scene::ImageId;

pub(crate) struct GpuImage {
    pub bind_group: wgpu::BindGroup,
    pub width: u32,
    pub height: u32,
}

pub(crate) struct ImageStore {
    images: Vec<GpuImage>,
    sampler: wgpu::Sampler,
    /// Texture format: sRGB when the target is sRGB, so sampled values are linear.
    format: wgpu::TextureFormat,
}

impl ImageStore {
    pub fn new(device: &wgpu::Device, srgb_output: bool) -> Self {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("image sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            images: Vec::new(),
            sampler,
            format: if srgb_output {
                wgpu::TextureFormat::Rgba8UnormSrgb
            } else {
                wgpu::TextureFormat::Rgba8Unorm
            },
        }
    }

    pub fn create(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> ImageId {
        assert_eq!(
            rgba.len(),
            width as usize * height as usize * 4,
            "image data must be tightly packed RGBA8"
        );
        let max = device.limits().max_texture_dimension_2d;
        assert!(
            width <= max && height <= max,
            "image {width}x{height} exceeds the device limit of {max}"
        );

        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("image"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            size,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("image bind group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        self.images.push(GpuImage {
            bind_group,
            width,
            height,
        });
        ImageId(self.images.len() as u32 - 1)
    }

    /// # Panics
    /// If `id` was not created by this store.
    pub fn get(&self, id: ImageId) -> &GpuImage {
        &self.images[id.0 as usize]
    }
}

//! The device is requested with WebGL2-level limits and no optional features, so the
//! same renderer runs on Direct3D 12, Metal, Vulkan, GLES, WebGPU and WebGL2.

use std::fmt;

#[derive(Debug)]
pub enum InitError {
    /// No adapter can present to the surface (for example, WebGPU is exposed but unusable).
    Adapter(wgpu::RequestAdapterError),
    Device(wgpu::RequestDeviceError),
    NoSurfaceFormat,
}

impl fmt::Display for InitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InitError::Adapter(e) => write!(f, "no suitable GPU adapter: {e}"),
            InitError::Device(e) => write!(f, "failed to create GPU device: {e}"),
            InitError::NoSurfaceFormat => write!(f, "surface has no supported format"),
        }
    }
}

impl std::error::Error for InitError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcquireError {
    /// The surface was reconfigured or is temporarily unavailable; try again next frame.
    Retry,
    /// The surface is gone and must be recreated from the window.
    SurfaceLost,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GpuOptions {
    /// Largest texture side to allow. Bounds the glyph atlases (which grow from 256 px
    /// up to this size) and the largest image. `None` uses what the adapter supports.
    pub max_texture_size: Option<u32>,
    /// The window shows what is behind it where the frame is not opaque (the clear
    /// color's alpha and what is drawn over it). Off: the surface is opaque.
    pub transparent: bool,
}

/// How the compositor blends a surface with what is behind the window. The frame holds
/// premultiplied colors (see [`crate::Renderer`]): a transparent surface takes
/// `PostMultiplied` where it is offered, which is how Metal names a non-opaque layer
/// (Core Animation composites it premultiplied), and `PreMultiplied` otherwise.
pub fn alpha_mode(
    modes: &[wgpu::CompositeAlphaMode],
    transparent: bool,
) -> wgpu::CompositeAlphaMode {
    use wgpu::CompositeAlphaMode as Mode;
    let wanted: &[Mode] = if transparent {
        &[Mode::PostMultiplied, Mode::PreMultiplied]
    } else {
        &[]
    };
    wanted
        .iter()
        .copied()
        .find(|mode| modes.contains(mode))
        .or_else(|| modes.first().copied())
        .unwrap_or(Mode::Auto)
}

/// Limits every backend must satisfy: the WebGL2 baseline, with resolution limits
/// raised to what the adapter supports.
pub fn required_limits(adapter: &wgpu::Adapter) -> wgpu::Limits {
    limits_with(adapter, GpuOptions::default())
}

pub fn limits_with(adapter: &wgpu::Adapter, options: GpuOptions) -> wgpu::Limits {
    let mut limits = wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits());
    if let Some(max) = options.max_texture_size {
        limits.max_texture_dimension_2d = limits.max_texture_dimension_2d.min(max);
    }
    limits
}

pub struct GpuContext {
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
}

impl GpuContext {
    pub async fn new(
        instance: &wgpu::Instance,
        surface: wgpu::Surface<'static>,
        width: u32,
        height: u32,
    ) -> Result<Self, InitError> {
        Self::with_options(instance, surface, width, height, GpuOptions::default()).await
    }

    pub async fn with_options(
        instance: &wgpu::Instance,
        surface: wgpu::Surface<'static>,
        width: u32,
        height: u32,
        options: GpuOptions,
    ) -> Result<Self, InitError> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(InitError::Adapter)?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("noa-gpui device"),
                required_features: wgpu::Features::empty(),
                required_limits: limits_with(&adapter, options),
                ..Default::default()
            })
            .await
            .map_err(InitError::Device)?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| caps.formats.first().copied())
            .ok_or(InitError::NoSurfaceFormat)?;

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: alpha_mode(&caps.alpha_modes, options.transparent),
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &config);

        let info = adapter.get_info();
        log::info!(
            "GPU: {} ({:?}), surface format {:?}, alpha {:?}",
            info.name,
            info.backend,
            format,
            config.alpha_mode
        );

        Ok(Self {
            adapter,
            device,
            queue,
            surface,
            config,
        })
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
    }

    /// Keeps the device and all GPU resources.
    pub fn replace_surface(&mut self, surface: wgpu::Surface<'static>) {
        self.surface = surface;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn acquire(&mut self) -> Result<wgpu::SurfaceTexture, AcquireError> {
        acquire(&self.surface, &self.device, &self.config)
    }

    /// Uses the main surface's format when the window supports it, so one
    /// [`crate::Renderer`] configuration fits both.
    pub fn create_window_surface(
        &self,
        surface: wgpu::Surface<'static>,
        width: u32,
        height: u32,
    ) -> Result<WindowSurface, InitError> {
        let caps = surface.get_capabilities(&self.adapter);
        let format = if caps.formats.contains(&self.config.format) {
            self.config.format
        } else {
            caps.formats
                .iter()
                .copied()
                .find(wgpu::TextureFormat::is_srgb)
                .or_else(|| caps.formats.first().copied())
                .ok_or(InitError::NoSurfaceFormat)?
        };
        let config = wgpu::SurfaceConfiguration {
            format,
            width: width.max(1),
            height: height.max(1),
            alpha_mode: caps.alpha_modes[0],
            ..self.config.clone()
        };
        surface.configure(&self.device, &config);
        Ok(WindowSurface { surface, config })
    }
}

pub struct WindowSurface {
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
}

impl WindowSurface {
    pub fn format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(device, &self.config);
    }

    pub fn replace_surface(&mut self, device: &wgpu::Device, surface: wgpu::Surface<'static>) {
        self.surface = surface;
        self.surface.configure(device, &self.config);
    }

    pub fn acquire(&mut self, device: &wgpu::Device) -> Result<wgpu::SurfaceTexture, AcquireError> {
        acquire(&self.surface, device, &self.config)
    }
}

fn acquire(
    surface: &wgpu::Surface<'static>,
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
) -> Result<wgpu::SurfaceTexture, AcquireError> {
    match surface.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(frame) => Ok(frame),
        wgpu::CurrentSurfaceTexture::Suboptimal(_) | wgpu::CurrentSurfaceTexture::Outdated => {
            surface.configure(device, config);
            Err(AcquireError::Retry)
        }
        wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
            Err(AcquireError::Retry)
        }
        wgpu::CurrentSurfaceTexture::Lost => Err(AcquireError::SurfaceLost),
        wgpu::CurrentSurfaceTexture::Validation => {
            panic!("surface validation error while acquiring a frame")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::CompositeAlphaMode as Mode;

    #[test]
    fn a_transparent_surface_takes_a_blending_alpha_mode() {
        // Metal offers these two.
        let metal = [Mode::Opaque, Mode::PostMultiplied];
        assert_eq!(alpha_mode(&metal, true), Mode::PostMultiplied);
        assert_eq!(alpha_mode(&metal, false), Mode::Opaque);
        let vulkan = [Mode::Opaque, Mode::PreMultiplied, Mode::Inherit];
        assert_eq!(alpha_mode(&vulkan, true), Mode::PreMultiplied);
        // Where nothing blends, the window stays opaque.
        assert_eq!(alpha_mode(&[Mode::Opaque], true), Mode::Opaque);
        assert_eq!(alpha_mode(&[], false), Mode::Auto);
    }
}

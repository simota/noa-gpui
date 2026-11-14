//! Everything stays within WebGL2 limits: no compute shaders, no storage buffers, and
//! no `first_instance`.
//!
//! Enable exactly the backends an app needs with the `native`, `webgpu` or `webgl2`
//! features.

mod backdrop;
mod buffers;
pub mod gpu;
mod icons;
mod images;
#[cfg(not(target_arch = "wasm32"))]
mod offscreen;
mod pipelines;
#[cfg(not(target_arch = "wasm32"))]
mod png;
mod renderer;
#[cfg(not(target_arch = "wasm32"))]
mod software;

pub use glyphon;
pub use gpu::{
    AcquireError, GpuContext, GpuOptions, InitError, WindowSurface, limits_with, required_limits,
};
#[cfg(not(target_arch = "wasm32"))]
pub use offscreen::{OFFSCREEN_FORMAT, OffscreenRenderer, RgbaImage};
pub use renderer::{FrameStats, RenderError, Renderer, Viewport};
#[cfg(not(target_arch = "wasm32"))]
pub use software::SoftwareRenderer;
pub use wgpu;

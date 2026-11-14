#[cfg(test)]
mod a11y_audit;
mod app;
mod assets;
mod bench;
mod catalog;
mod mini_os;
mod scene;
mod timing;
mod ui_demo;

pub use app::Config;
#[cfg(not(target_arch = "wasm32"))]
pub use app::app;
#[cfg(target_arch = "wasm32")]
pub use app::app_with_gpu;
#[cfg(feature = "native")]
pub use app::snapshot;
pub use scene::SceneKind;
pub use ui_platform::{GpuContext, InitError, wgpu, winit};

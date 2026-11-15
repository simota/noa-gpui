//! Implement [`Screen`] and hand it to [`App`]. Build with exactly one of the `native`,
//! `webgpu` or `webgl2` features. The crates an application builds its screens from are
//! re-exported, so depending on this one is enough.

#[cfg(feature = "native")]
mod accessibility;
mod app;
#[cfg(feature = "native")]
mod clipboard;
#[cfg(feature = "native")]
mod headless;
mod input;
#[cfg(all(feature = "native", any(target_os = "macos", target_os = "linux")))]
mod pty;
#[cfg(all(feature = "native", target_os = "macos"))]
mod reopen;
mod screen;
#[cfg(feature = "native")]
mod stylesheet;
#[cfg(all(feature = "native", any(target_os = "macos", target_os = "windows")))]
mod system_menu;
#[cfg(all(feature = "native", target_os = "linux"))]
#[path = "linux_tray.rs"]
mod system_menu;
#[cfg(feature = "native")]
mod view_state;
mod wake;
#[cfg(feature = "native")]
mod windows;

pub use app::{App, AppOptions, FrameReport, Setup};
#[cfg(feature = "native")]
pub use headless::Headless;
pub use input::command_is_meta;
pub use noa_gpui_render_wgpu::{GpuContext, InitError, wgpu};
#[cfg(not(target_arch = "wasm32"))]
pub use noa_gpui_render_wgpu::{RenderError, RgbaImage};
#[cfg(all(feature = "native", any(target_os = "macos", target_os = "linux")))]
pub use pty::Pty;
pub use screen::Screen;
#[cfg(feature = "native")]
pub use screen::{Detached, FileDialogRequest, FileDialogResult, WindowRequest};
#[cfg(feature = "native")]
pub use stylesheet::{StyleSheetLoad, StyleSheetLoadError, StyleSheetWatcher};
#[cfg(feature = "native")]
pub use view_state::{MAIN_WINDOW_KEY, MainWindowState, ViewStateFile};
pub use wake::{Wake, Waker};
pub use {noa_gpui_core, noa_gpui_render_wgpu, noa_gpui_scene, noa_gpui_text, winit};

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
#[cfg(all(feature = "native", target_os = "macos"))]
mod ime_macos;
mod input;
mod inspector;
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
#[cfg(not(target_arch = "wasm32"))]
mod titlebar;
#[cfg(feature = "native")]
mod view_state;
mod wake;
#[cfg(target_arch = "wasm32")]
mod web_a11y;
#[cfg(any(target_arch = "wasm32", test))]
mod web_aria;
#[cfg(target_arch = "wasm32")]
mod web_ime;
#[cfg(any(target_arch = "wasm32", test))]
mod web_text;
#[cfg(feature = "native")]
mod windows;

#[cfg(not(target_arch = "wasm32"))]
pub use app::WindowBackground;
pub use app::{App, AppOptions, FrameReport, Setup};
#[cfg(feature = "native")]
pub use headless::Headless;
pub use input::command_is_meta;
pub use noa_gpui_render_wgpu::{GpuContext, InitError, wgpu};
#[cfg(not(target_arch = "wasm32"))]
pub use noa_gpui_render_wgpu::{RenderError, RgbaImage};
#[cfg(all(feature = "native", any(target_os = "macos", target_os = "linux")))]
pub use pty::Pty;
#[cfg(feature = "native")]
pub use screen::{
    AlertLevel, AlertRequest, AlertResult, Detached, FileDialogRequest, FileDialogResult,
    WindowDrag, WindowRequest,
};
pub use screen::{Screen, advance, replay};
#[cfg(feature = "native")]
pub use stylesheet::{StyleSheetLoad, StyleSheetLoadError, StyleSheetWatcher};
#[cfg(not(target_arch = "wasm32"))]
pub use titlebar::Titlebar;
#[cfg(feature = "native")]
pub use view_state::{MAIN_WINDOW_KEY, MainWindowState, ViewStateFile};
pub use wake::{Wake, Waker};
pub use {noa_gpui_core, noa_gpui_render_wgpu, noa_gpui_scene, noa_gpui_text, winit};

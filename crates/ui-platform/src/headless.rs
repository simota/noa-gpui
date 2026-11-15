use noa_gpui_core::UiTree;
use noa_gpui_render_wgpu::{
    InitError, OffscreenRenderer, RenderError, RgbaImage, SoftwareRenderer, Viewport,
};
use noa_gpui_scene::{Color, DisplayList, ImageId};
use noa_gpui_text::TextStore;

use crate::app::Setup;
use crate::screen::Screen;
use crate::wake::Waker;

/// Renders widget trees and screens to images without a window (desktop).
///
/// [`Headless::new`] draws on the GPU; [`Headless::software`] draws on the CPU, so it
/// works in a sandbox or on a CI runner without a GPU adapter, and
/// [`Headless::new_or_software`] takes the GPU when there is one. The CPU follows the
/// GPU's drawing closely but not to the pixel (see [`SoftwareRenderer`]).
///
/// ```no_run
/// # use noa_gpui_platform::Headless;
/// # use noa_gpui_platform::noa_gpui_core::UiTree;
/// # use noa_gpui_platform::noa_gpui_text::{FontOptions, TextStore};
/// # fn build() -> UiTree { UiTree::new() }
/// let mut headless = Headless::new().expect("a GPU adapter");
/// let mut text = TextStore::new(FontOptions::default());
/// let mut tree = build();
/// let image = headless
///     .render_tree(&mut tree, &mut text, (480, 320), 2.0)
///     .expect("render");
/// image.save_png("snapshot.png").expect("write");
/// ```
pub struct Headless {
    backend: Backend,
    waker: Waker,
}

enum Backend {
    Gpu(Box<OffscreenRenderer>),
    Software(Box<SoftwareRenderer>),
}

impl Headless {
    /// Open the default GPU adapter. Where there is none (a CI runner or sandbox
    /// without a GPU), this fails with [`InitError::Adapter`].
    pub fn new() -> Result<Self, InitError> {
        Ok(Self {
            backend: Backend::Gpu(Box::new(pollster::block_on(OffscreenRenderer::new())?)),
            waker: Waker::default(),
        })
    }

    /// Draw on the CPU ([`SoftwareRenderer`]): no GPU needed. There is no
    /// [`Setup`] then (see [`Headless::try_setup`]); images for a tree are made
    /// with [`Headless::create_image`].
    pub fn software() -> Self {
        Self {
            backend: Backend::Software(Box::default()),
            waker: Waker::default(),
        }
    }

    /// The GPU if there is an adapter, else the CPU ([`Headless::software`]).
    pub fn new_or_software() -> Self {
        Self::new().unwrap_or_else(|e| {
            log::info!("headless: {e}; drawing on the CPU");
            Self::software()
        })
    }

    pub fn is_software(&self) -> bool {
        matches!(self.backend, Backend::Software(_))
    }

    /// What building a screen needs (to upload its images), as the app passes it.
    ///
    /// # Panics
    ///
    /// When drawing on the CPU, which has no GPU device to give: use
    /// [`Headless::try_setup`] where that can be.
    pub fn setup(&mut self) -> Setup<'_> {
        self.try_setup()
            .expect("Headless::setup needs the GPU; this one draws on the CPU")
    }

    /// What building a screen needs, or `None` when drawing on the CPU.
    pub fn try_setup(&mut self) -> Option<Setup<'_>> {
        let Backend::Gpu(offscreen) = &mut self.backend else {
            return None;
        };
        let OffscreenRenderer {
            device,
            queue,
            renderer,
        } = &mut **offscreen;
        Some(Setup {
            renderer,
            device,
            queue,
            waker: self.waker.clone(),
        })
    }

    /// Make an image (RGBA8 in sRGB, straight alpha, rows top to bottom) to show
    /// with [`noa_gpui_core::widgets::Image`], on the GPU or the CPU.
    pub fn create_image(&mut self, width: u32, height: u32, rgba: &[u8]) -> ImageId {
        match &mut self.backend {
            Backend::Gpu(o) => o
                .renderer
                .create_image(&o.device, &o.queue, width, height, rgba),
            Backend::Software(s) => s.create_image(width, height, rgba),
        }
    }

    fn render(
        &mut self,
        list: &DisplayList,
        text: &mut TextStore,
        viewport: Viewport,
        background: Color,
    ) -> Result<RgbaImage, RenderError> {
        match &mut self.backend {
            Backend::Gpu(o) => o.render(list, text, viewport, background),
            Backend::Software(s) => Ok(s.render(list, text, viewport, background)),
        }
    }

    /// Lay out and draw `tree` at `size` logical points times `scale`, on its theme's
    /// window background.
    pub fn render_tree(
        &mut self,
        tree: &mut UiTree,
        text: &mut TextStore,
        size: (u32, u32),
        scale: f32,
    ) -> Result<RgbaImage, RenderError> {
        let viewport = viewport(size, scale);
        tree.prepare(viewport.logical_size(), text);
        let background = tree.theme().background;
        self.render(tree.display_list(), text, viewport, background)
    }

    /// Lay out and draw `screen` as the app would show it in a window of `size`
    /// logical points times `scale`.
    pub fn render_screen(
        &mut self,
        screen: &mut dyn Screen,
        text: &mut TextStore,
        size: (u32, u32),
        scale: f32,
    ) -> Result<RgbaImage, RenderError> {
        let viewport = viewport(size, scale);
        screen.before_frame(text);
        screen.prepare(viewport.logical_size(), text);
        let background = screen.background();
        self.render(screen.display_list(), text, viewport, background)
    }
}

fn viewport(size: (u32, u32), scale: f32) -> Viewport {
    Viewport {
        width: (size.0 as f32 * scale).round() as u32,
        height: (size.1 as f32 * scale).round() as u32,
        scale_factor: scale,
    }
}

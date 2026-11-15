//! Each window has its own surface, renderer and widget tree; the GPU device and the
//! text store are shared with the main window.

use std::sync::Arc;

use noa_gpui_render_wgpu::{AcquireError, GpuContext, Renderer, Viewport, WindowSurface, wgpu};
use noa_gpui_text::TextStore;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

use crate::accessibility::Accessibility;
use crate::input::InputState;
use crate::screen::{Screen, WindowRequest};

pub struct SecondaryWindow {
    pub window: Arc<Window>,
    accessibility: Accessibility,
    surface: WindowSurface,
    renderer: Renderer,
    screen: Box<dyn Screen>,
    input: InputState,
    needs_present: bool,
    ime_area: Option<noa_gpui_scene::Rect>,
    /// The pointer shape last given to the window.
    cursor: noa_gpui_core::Cursor,
    titlebar: crate::Titlebar,
}

impl SecondaryWindow {
    pub fn open(
        event_loop: &ActiveEventLoop,
        instance: &wgpu::Instance,
        gpu: &GpuContext,
        request: WindowRequest,
    ) -> Option<Self> {
        let attributes = crate::titlebar::apply(
            Window::default_attributes()
                .with_title(request.title)
                .with_inner_size(winit::dpi::LogicalSize::new(request.size.0, request.size.1))
                .with_visible(false),
            &request.titlebar,
            request.min_size,
        );
        let attributes = if request.floating {
            attributes
                .with_window_level(winit::window::WindowLevel::AlwaysOnTop)
                .with_resizable(false)
        } else {
            attributes
        };
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .inspect_err(|e| log::error!("could not open a window: {e}"))
                .ok()?,
        );
        crate::titlebar::follow(&window, &request.titlebar, None);
        let accessibility = Accessibility::attach(event_loop, &window);
        let surface = instance
            .create_surface(window.clone())
            .inspect_err(|e| log::error!("could not create a surface: {e}"))
            .ok()?;
        let size = window.inner_size();
        let surface = gpu
            .create_window_surface(surface, size.width, size.height)
            .inspect_err(|e| log::error!("could not configure a surface: {e}"))
            .ok()?;
        let renderer = Renderer::new(&gpu.device, &gpu.queue, surface.format());
        let mut screen = request.screen;
        screen
            .tree_mut()
            .set_clipboard(Box::new(crate::clipboard::SystemClipboard::new()));
        if let Some(theme) = window.theme() {
            screen.set_system_appearance(appearance(theme));
        }
        window.request_redraw();
        Some(Self {
            window,
            accessibility,
            surface,
            renderer,
            screen,
            input: InputState::default(),
            needs_present: true,
            ime_area: None,
            cursor: noa_gpui_core::Cursor::Default,
            titlebar: request.titlebar,
        })
    }

    /// When this window's tree next needs a frame for a timer (see
    /// [`noa_gpui_core::UiTree::next_timer`]).
    pub fn next_timer(&self) -> Option<f64> {
        self.screen.tree().next_timer()
    }

    pub fn take_detached(&mut self) -> Option<crate::screen::Detached> {
        self.screen.take_detached()
    }

    /// Close the window, releasing its widgets' text layouts from the shared store.
    pub fn close(mut self, text: &mut TextStore) {
        let tree = self.screen.tree_mut();
        let roots: Vec<_> = tree.root().into_iter().chain(tree.overlays()).collect();
        for id in roots {
            tree.remove(id, text);
        }
    }

    pub fn id(&self) -> winit::window::WindowId {
        self.window.id()
    }

    /// Handle one event for this window. Returns `false` when the window should close.
    pub fn window_event(
        &mut self,
        event: WindowEvent,
        instance: &wgpu::Instance,
        gpu: &GpuContext,
        text: &mut TextStore,
        now: f64,
    ) -> bool {
        self.accessibility.process_event(&self.window, &event);
        crate::titlebar::follow(&self.window, &self.titlebar, Some(&event));
        match event {
            WindowEvent::CloseRequested => {
                self.screen.close_requested();
                return false;
            }
            WindowEvent::Resized(size) => {
                self.surface.resize(&gpu.device, size.width, size.height);
                self.needs_present = true;
                self.window.request_redraw();
            }
            WindowEvent::RedrawRequested => self.redraw(instance, gpu, text, now),
            WindowEvent::ThemeChanged(theme) => {
                self.screen.set_system_appearance(appearance(theme));
                self.window.request_redraw();
            }
            event => {
                self.screen.tree_mut().set_now(now);
                crate::input::sync_multi_click(self.screen.tree_mut());
                for event in self.input.translate(&event, self.window.scale_factor()) {
                    self.screen.dispatch(event, text);
                }
                crate::app::start_window_drags(&self.window, self.screen.as_mut());
                crate::app::sync_cursor(
                    &self.window,
                    &mut self.cursor,
                    self.screen.tree().cursor(),
                );
                if self.screen.needs_update(self.viewport().logical_size()) {
                    self.window.request_redraw();
                }
            }
        }
        !self.screen.wants_close()
    }

    /// Hand the files the OS dropped on this window to its tree as one drop.
    pub fn deliver_drop(&mut self, text: &mut TextStore, now: f64) {
        if let Some(drop) = self.input.take_drop() {
            self.screen.tree_mut().set_now(now);
            self.screen.dispatch(drop, text);
            self.window.request_redraw();
        }
    }

    fn viewport(&self) -> Viewport {
        let (width, height) = self.surface.size();
        Viewport {
            width,
            height,
            scale_factor: self.window.scale_factor() as f32,
        }
    }

    fn redraw(
        &mut self,
        instance: &wgpu::Instance,
        gpu: &GpuContext,
        text: &mut TextStore,
        now: f64,
    ) {
        let viewport = self.viewport();
        self.screen.tree_mut().set_now(now);
        if crate::app::wants_frame(self.screen.tree(), now) {
            self.screen.animate(now, text);
        }
        self.accessibility.run_actions(self.screen.as_mut(), text);
        let changed = self.screen.prepare(viewport.logical_size(), text);
        if changed {
            self.needs_present = true;
        }
        self.accessibility
            .update(self.screen.as_ref(), changed, viewport.scale_factor as f64);
        crate::app::sync_ime(
            &self.window,
            &mut self.ime_area,
            self.screen.text_input_area(),
        );
        crate::app::sync_cursor(&self.window, &mut self.cursor, self.screen.tree().cursor());
        let animating = self.screen.tree().wants_animation_frame();
        if !self.needs_present {
            if animating {
                self.window.request_redraw();
            }
            return;
        }
        if let Err(e) = self.renderer.prepare(
            &gpu.device,
            &gpu.queue,
            self.screen.display_list(),
            text,
            viewport,
        ) {
            log::error!("{e}");
            return;
        }
        let frame = match self.surface.acquire(&gpu.device) {
            Ok(frame) => frame,
            Err(AcquireError::Retry) => {
                self.window.request_redraw();
                return;
            }
            Err(AcquireError::SurfaceLost) => {
                if let Ok(surface) = instance.create_surface(self.window.clone()) {
                    self.surface.replace_surface(&gpu.device, surface);
                }
                self.window.request_redraw();
                return;
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        if let Err(e) = self
            .renderer
            .encode(&mut encoder, &view, self.screen.background())
        {
            log::error!("{e}");
            return;
        }
        gpu.queue.submit([encoder.finish()]);
        self.window.pre_present_notify();
        gpu.queue.present(frame);
        self.renderer.end_frame();
        self.needs_present = false;
        if animating {
            self.window.request_redraw();
        }
    }

    pub fn set_system_appearance(&mut self, theme: winit::window::Theme) {
        self.screen.set_system_appearance(appearance(theme));
        self.window.request_redraw();
    }
}

pub fn appearance(theme: winit::window::Theme) -> noa_gpui_core::Appearance {
    match theme {
        winit::window::Theme::Dark => noa_gpui_core::Appearance::Dark,
        winit::window::Theme::Light => noa_gpui_core::Appearance::Light,
    }
}

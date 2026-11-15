use std::sync::Arc;
use std::time::Duration;

use noa_gpui_render_wgpu::{AcquireError, FrameStats, GpuContext, Renderer, Viewport, wgpu};
use noa_gpui_text::{FontOptions, TextStore};
use web_time::Instant;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use crate::input::InputState;
use crate::screen::Screen;
use crate::wake::{Wake, Waker};

#[derive(Debug, Clone, PartialEq)]
pub struct AppOptions {
    /// The application's name: the application menu and About panel on macOS, and the
    /// tooltip of the menu bar extra or tray icon.
    pub name: String,
    /// Shown in the About panel on macOS.
    pub version: Option<String>,
    pub title: String,
    /// The main window's logical size (desktop; on the Web the canvas decides).
    pub size: (f64, f64),
    /// Desktop: closing the main window hides it and the application keeps running (a
    /// menu bar extra or tray icon can show it again, see
    /// [`Screen::take_main_window_visibility`]; on macOS so does a click on the Dock
    /// icon). It ends when the screen
    /// [wants to close](Screen::wants_close). Off by default: closing ends it.
    pub keep_running: bool,
}

impl AppOptions {
    /// A 1280×800 window titled `name`.
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            title: name.clone(),
            name,
            version: None,
            size: (1280.0, 800.0),
            keep_running: false,
        }
    }
}

/// What a screen can use while it is built: the GPU is ready, so images can be uploaded
/// with [`Renderer::create_image`].
pub struct Setup<'a> {
    pub renderer: &'a mut Renderer,
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub waker: Waker,
}

#[derive(Debug, Clone, Copy)]
pub struct FrameReport<'a> {
    /// The graphics backend in use, such as `Metal` or `BrowserWebGpu`.
    pub backend: &'a str,
    pub viewport: Viewport,
    pub stats: FrameStats,
    /// CPU time spent on the frame, without the wait for the surface texture (vsync).
    pub cpu: Duration,
}

type Build = Box<dyn FnOnce(&mut Setup<'_>) -> Box<dyn Screen>>;

/// Everything that exists only while the GPU is ready.
struct Graphics {
    ctx: GpuContext,
    renderer: Renderer,
    screen: Box<dyn Screen>,
    backend: String,
}

impl Graphics {
    fn new(ctx: GpuContext, build: Build, waker: &Waker) -> Self {
        let mut renderer = Renderer::new(&ctx.device, &ctx.queue, ctx.format());
        let backend = format!("{:?}", ctx.adapter.get_info().backend);
        #[cfg_attr(not(feature = "native"), expect(unused_mut))]
        let mut screen = build(&mut Setup {
            renderer: &mut renderer,
            device: &ctx.device,
            queue: &ctx.queue,
            waker: waker.clone(),
        });
        #[cfg(feature = "native")]
        screen
            .tree_mut()
            .set_clipboard(Box::new(crate::clipboard::SystemClipboard::new()));
        Self {
            ctx,
            renderer,
            screen,
            backend,
        }
    }
}

pub struct App {
    options: AppOptions,
    #[cfg(not(target_arch = "wasm32"))]
    build: Option<Build>,
    instance: Option<wgpu::Instance>,
    window: Option<Arc<Window>>,
    gfx: Option<Graphics>,
    text: TextStore,
    presented: bool,
    input: InputState,
    /// The surface holds no up-to-date frame (first frame, surface reconfigured, or a
    /// prepared frame failed to present), so the next redraw must render even if the
    /// widget tree reports no change.
    needs_present: bool,
    /// The caret area last reported to the IME; `None` while IME input is disabled.
    ime_area: Option<noa_gpui_scene::Rect>,
    cursor: noa_gpui_core::Cursor,
    clock: Instant,
    waker: Waker,
    /// The main window was hidden (closed with [`AppOptions::keep_running`], or by the
    /// screen).
    main_hidden: bool,
    #[cfg(feature = "native")]
    windows: Vec<crate::windows::SecondaryWindow>,
    #[cfg(feature = "native")]
    accessibility: Option<crate::accessibility::Accessibility>,
    #[cfg(all(
        feature = "native",
        any(target_os = "macos", target_os = "windows", target_os = "linux")
    ))]
    system_menus: crate::system_menu::SystemMenus,
    #[cfg(target_arch = "wasm32")]
    canvas: web_sys::HtmlCanvasElement,
    #[cfg(feature = "native")]
    stylesheet: Option<crate::stylesheet::StyleSheetWatcher>,
    #[cfg(feature = "native")]
    view_state: Option<(crate::ViewStateFile, noa_gpui_core::ViewState)>,
}

impl App {
    fn with_parts(
        options: AppOptions,
        #[cfg(not(target_arch = "wasm32"))] build: Option<Build>,
        instance: Option<wgpu::Instance>,
        gfx: Option<Graphics>,
        waker: Waker,
        #[cfg(target_arch = "wasm32")] canvas: web_sys::HtmlCanvasElement,
    ) -> Self {
        Self {
            #[cfg(all(feature = "native", any(target_os = "macos", target_os = "windows")))]
            system_menus: crate::system_menu::SystemMenus::new(&options),
            #[cfg(all(feature = "native", target_os = "linux"))]
            system_menus: crate::system_menu::SystemMenus::new(&options, &waker),
            options,
            #[cfg(not(target_arch = "wasm32"))]
            build,
            instance,
            window: None,
            gfx,
            text: TextStore::new(FontOptions::default()),
            presented: false,
            input: InputState::default(),
            needs_present: true,
            ime_area: None,
            cursor: noa_gpui_core::Cursor::Default,
            clock: Instant::now(),
            waker,
            main_hidden: false,
            #[cfg(feature = "native")]
            windows: Vec::new(),
            #[cfg(feature = "native")]
            accessibility: None,
            #[cfg(target_arch = "wasm32")]
            canvas,
            #[cfg(feature = "native")]
            stylesheet: None,
            #[cfg(feature = "native")]
            view_state: None,
        }
    }

    /// Desktop: the window and GPU are created on the first `resumed`, then `build`
    /// makes the main screen.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(
        options: AppOptions,
        build: impl FnOnce(&mut Setup<'_>) -> Box<dyn Screen> + 'static,
    ) -> Self {
        Self::with_parts(options, Some(Box::new(build)), None, None, Waker::default())
    }

    /// Web: the GPU was initialized from `canvas` before the event loop started, so the
    /// loader could fall back to another backend if initialization failed.
    #[cfg(target_arch = "wasm32")]
    pub fn with_gpu(
        options: AppOptions,
        instance: wgpu::Instance,
        ctx: GpuContext,
        canvas: web_sys::HtmlCanvasElement,
        build: impl FnOnce(&mut Setup<'_>) -> Box<dyn Screen> + 'static,
    ) -> Self {
        let waker = Waker::default();
        let gfx = Graphics::new(ctx, Box::new(build), &waker);
        Self::with_parts(options, Some(instance), Some(gfx), waker, canvas)
    }

    /// Desktop: restyle the main screen from the Noa Style sheet at `path` whenever the
    /// file is saved. A file that cannot be read or parsed is reported in the log and
    /// leaves the sheet in use as it is.
    #[cfg(feature = "native")]
    pub fn watch_stylesheet(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.stylesheet = Some(crate::stylesheet::StyleSheetWatcher::new(
            path,
            self.waker.clone(),
        ));
        self
    }

    /// Desktop: keep the states of the widgets named with
    /// [`noa_gpui_core::UiTree::set_autosave_name`] and the main window's size, position and
    /// maximized state between runs, in the file at `path`.
    /// The file is read now and written when the application ends. A saved position
    /// that is on no monitor any more is not used. A file that cannot be read or written
    /// is reported in the log; the application runs as without it.
    #[cfg(feature = "native")]
    pub fn autosave_view_state(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        let file = crate::ViewStateFile::new(path);
        let state = file.load().unwrap_or_else(|e| {
            log::warn!("view state not read from {}: {e}", file.path().display());
            noa_gpui_core::ViewState::new()
        });
        self.view_state = Some((file, state));
        self
    }

    /// The main window's saved geometry, if [`App::autosave_view_state`] read one.
    #[cfg(feature = "native")]
    fn saved_main_window(&self) -> Option<crate::MainWindowState> {
        let (_, state) = self.view_state.as_ref()?;
        crate::MainWindowState::parse(state.get(crate::MAIN_WINDOW_KEY)?)
    }

    #[cfg(feature = "native")]
    fn save_view_state(&self) {
        let (Some((file, _)), Some(gfx)) = (&self.view_state, &self.gfx) else {
            return;
        };
        let mut state = gfx.screen.tree().view_state();
        if let Some(window) = &self.window {
            let size = window.inner_size().to_logical::<f64>(window.scale_factor());
            let maximized = window.is_maximized();
            // A maximized window keeps the geometry it had before, to go back to.
            let earlier = maximized
                .then(|| state.get(crate::MAIN_WINDOW_KEY))
                .flatten()
                .and_then(crate::MainWindowState::parse);
            let geometry = match earlier {
                Some(earlier) => crate::MainWindowState {
                    maximized: true,
                    ..earlier
                },
                None => crate::MainWindowState {
                    size: (size.width, size.height),
                    position: window.outer_position().ok().map(|p| (p.x, p.y)),
                    maximized,
                },
            };
            if geometry.size.0 >= crate::MainWindowState::MIN_SIZE.0
                && geometry.size.1 >= crate::MainWindowState::MIN_SIZE.1
            {
                state.set(crate::MAIN_WINDOW_KEY, geometry.to_value());
            }
        }
        match file.save(&state) {
            Ok(()) => log::info!("view state saved to {}", file.path().display()),
            Err(e) => log::warn!("view state not saved to {}: {e}", file.path().display()),
        }
    }

    #[cfg(feature = "native")]
    fn apply_stylesheet(&mut self) {
        let (Some(watcher), Some(gfx)) = (&self.stylesheet, &mut self.gfx) else {
            return;
        };
        match watcher.take() {
            Some(Ok(sheet)) => {
                log::info!("stylesheet loaded from {}", watcher.path().display());
                gfx.screen.tree_mut().set_stylesheet(sheet);
            }
            Some(Err(e)) => log::warn!("stylesheet not applied: {e}"),
            None => {}
        }
    }

    /// A [`Waker`] for this application's event loop (the one [`Setup::waker`] gives).
    pub fn waker(&self) -> Waker {
        self.waker.clone()
    }

    /// Desktop: run the application until its main window closes.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn run(mut self) -> Result<(), winit::error::EventLoopError> {
        let event_loop = winit::event_loop::EventLoop::<Wake>::with_user_event().build()?;
        self.waker.connect(event_loop.create_proxy());
        #[cfg(all(feature = "native", target_os = "macos"))]
        crate::reopen::install(&self.waker);
        event_loop.run_app(&mut self)
    }

    /// Web: start the application's event loop; it runs after this returns.
    #[cfg(target_arch = "wasm32")]
    pub fn spawn(self) -> Result<(), winit::error::EventLoopError> {
        use winit::platform::web::EventLoopExtWebSys;
        let event_loop = winit::event_loop::EventLoop::<Wake>::with_user_event().build()?;
        self.waker.connect(event_loop.create_proxy());
        event_loop.spawn_app(self);
        Ok(())
    }

    fn window_attributes(
        &self,
        #[cfg_attr(not(feature = "native"), expect(unused_variables))] event_loop: &ActiveEventLoop,
    ) -> winit::window::WindowAttributes {
        let attributes = Window::default_attributes().with_title(&self.options.title);
        #[cfg(target_arch = "wasm32")]
        let attributes = {
            use winit::platform::web::WindowAttributesExtWebSys;
            attributes.with_canvas(Some(self.canvas.clone()))
        };
        #[cfg(not(target_arch = "wasm32"))]
        let attributes = attributes.with_inner_size(winit::dpi::LogicalSize::new(
            self.options.size.0,
            self.options.size.1,
        ));
        #[cfg(feature = "native")]
        let attributes = match self.saved_main_window() {
            Some(saved) => {
                let attributes = attributes
                    .with_inner_size(winit::dpi::LogicalSize::new(saved.size.0, saved.size.1))
                    .with_maximized(saved.maximized);
                match saved.position.filter(|&p| on_a_monitor(event_loop, p)) {
                    Some((x, y)) => {
                        attributes.with_position(winit::dpi::PhysicalPosition::new(x, y))
                    }
                    None => attributes,
                }
            }
            None => attributes,
        };
        // The accessibility adapter must be attached before the window is first shown.
        #[cfg(feature = "native")]
        let attributes = attributes.with_visible(false);
        attributes
    }

    fn create_surface(&self) -> wgpu::Surface<'static> {
        let instance = self
            .instance
            .as_ref()
            .expect("instance exists once resumed");
        #[cfg(target_arch = "wasm32")]
        let target = wgpu::SurfaceTarget::Canvas(self.canvas.clone());
        #[cfg(not(target_arch = "wasm32"))]
        let target = self.window.clone().expect("window exists once resumed");
        instance.create_surface(target).expect("create surface")
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn init_gpu(&mut self, event_loop: &ActiveEventLoop) {
        let window = self.window.clone().expect("window created before GPU");
        let descriptor = wgpu::InstanceDescriptor::new_with_display_handle(Box::new(
            event_loop.owned_display_handle(),
        ));
        self.instance = Some(wgpu::Instance::new(descriptor.with_env()));
        let surface = self.create_surface();
        let size = window.inner_size();
        let ctx = pollster::block_on(GpuContext::new(
            self.instance.as_ref().expect("set above"),
            surface,
            size.width,
            size.height,
        ))
        .unwrap_or_else(|e| panic!("GPU initialization failed: {e}"));
        let build = self.build.take().expect("the screen is built once");
        self.gfx = Some(Graphics::new(ctx, build, &self.waker));
        #[cfg(feature = "native")]
        self.apply_stylesheet();
        #[cfg(feature = "native")]
        if let (Some((_, state)), Some(gfx)) = (&self.view_state, &mut self.gfx) {
            gfx.screen.tree_mut().restore_view_state(state.clone());
        }
    }

    fn redraw(&mut self) {
        let (Some(window), Some(gfx)) = (self.window.clone(), self.gfx.as_mut()) else {
            return;
        };
        let started = Instant::now();
        let (width, height) = gfx.ctx.size();
        let viewport = Viewport {
            width,
            height,
            scale_factor: window.scale_factor() as f32,
        };

        let Graphics {
            ctx,
            renderer,
            screen,
            ..
        } = gfx;
        screen
            .tree_mut()
            .set_now(self.clock.elapsed().as_secs_f64());
        screen.before_frame(&mut self.text);
        let now = self.clock.elapsed().as_secs_f64();
        if wants_frame(screen.tree(), now) {
            screen.animate(now, &mut self.text);
        }
        #[cfg(feature = "native")]
        if let Some(accessibility) = &mut self.accessibility {
            accessibility.run_actions(screen.as_mut(), &mut self.text);
        }
        let changed = screen.prepare(viewport.logical_size(), &mut self.text);
        if changed {
            self.needs_present = true;
        }
        #[cfg(feature = "native")]
        if let Some(accessibility) = &mut self.accessibility {
            accessibility.update(screen.as_ref(), changed, viewport.scale_factor as f64);
        }
        sync_ime(&window, &mut self.ime_area, screen.text_input_area());
        sync_cursor(&window, &mut self.cursor, screen.tree().cursor());
        if !self.needs_present && !screen.continuous() {
            // Nothing to draw this frame, but an animation may still be running.
            if screen.tree().wants_animation_frame() {
                window.request_redraw();
            }
            return;
        }
        if let Err(e) = renderer.prepare(
            &ctx.device,
            &ctx.queue,
            screen.display_list(),
            &mut self.text,
            viewport,
        ) {
            log::error!("{e}");
            return;
        }
        let background = screen.background();

        // Waiting for the next surface texture is vsync, not CPU work: exclude it.
        let acquire_started = Instant::now();
        let frame = match gfx.ctx.acquire() {
            Ok(frame) => frame,
            Err(AcquireError::Retry) => {
                window.request_redraw();
                return;
            }
            Err(AcquireError::SurfaceLost) => {
                let surface = self.create_surface();
                let gfx = self.gfx.as_mut().expect("checked above");
                gfx.ctx.replace_surface(surface);
                self.needs_present = true;
                window.request_redraw();
                return;
            }
        };
        let acquired = Instant::now();
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gfx
            .ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        if let Err(e) = gfx.renderer.encode(&mut encoder, &view, background) {
            log::error!("{e}");
            return;
        }
        gfx.ctx.queue.submit([encoder.finish()]);
        window.pre_present_notify();
        gfx.ctx.queue.present(frame);
        self.needs_present = false;
        if !self.presented {
            self.presented = true;
            log::info!("first frame presented ({width}x{height})");
        }
        gfx.renderer.end_frame();

        gfx.screen.frame_presented(&FrameReport {
            backend: &gfx.backend,
            viewport,
            stats: gfx.renderer.stats(),
            cpu: (acquire_started - started) + acquired.elapsed(),
        });
        if gfx.screen.continuous() || gfx.screen.tree().wants_animation_frame() {
            window.request_redraw();
        }
    }

    #[cfg(all(
        feature = "native",
        any(target_os = "macos", target_os = "windows", target_os = "linux")
    ))]
    fn sync_system_menus(&mut self) {
        let Some(gfx) = &mut self.gfx else {
            return;
        };
        let screen = gfx.screen.as_mut();
        screen
            .tree_mut()
            .set_now(self.clock.elapsed().as_secs_f64());
        let chosen = self.system_menus.poll(screen, &mut self.text);
        self.system_menus.sync(screen);
        if chosen && let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    #[cfg(feature = "native")]
    fn open_requested_windows(&mut self, event_loop: &ActiveEventLoop) {
        let (Some(gfx), Some(instance)) = (&mut self.gfx, &self.instance) else {
            return;
        };
        for request in gfx.screen.take_window_requests() {
            if let Some(window) =
                crate::windows::SecondaryWindow::open(event_loop, instance, &gfx.ctx, request)
            {
                self.windows.push(window);
            }
        }
    }

    fn schedule_timers(&mut self, event_loop: &ActiveEventLoop) {
        let now = self.clock.elapsed().as_secs_f64();
        let mut next: Option<f64> = None;
        let mut due = |timer: Option<f64>, window: &Window| {
            let Some(t) = timer else { return };
            if t <= now {
                window.request_redraw();
            } else {
                next = Some(next.map_or(t, |n: f64| n.min(t)));
            }
        };
        if let (Some(window), Some(gfx)) = (&self.window, &self.gfx) {
            due(gfx.screen.tree().next_timer(), window);
        }
        #[cfg(feature = "native")]
        for window in &self.windows {
            due(window.next_timer(), &window.window);
        }
        event_loop.set_control_flow(match next {
            Some(t) => winit::event_loop::ControlFlow::WaitUntil(
                self.clock + web_time::Duration::from_secs_f64(t),
            ),
            None => winit::event_loop::ControlFlow::Wait,
        });
    }

    /// Show the open and save panels the screen asked for (the OS's own), one after
    /// the other, and hand back what the user chose.
    #[cfg(all(
        feature = "native",
        any(target_os = "macos", target_os = "windows", target_os = "linux")
    ))]
    fn run_file_dialogs(&mut self) {
        use crate::screen::FileDialogResult;
        let (Some(window), Some(gfx)) = (&self.window, &mut self.gfx) else {
            return;
        };
        for request in gfx.screen.take_file_dialogs() {
            let mut dialog = rfd::FileDialog::new()
                .set_title(&request.title)
                .set_parent(window.as_ref());
            for (name, extensions) in &request.filters {
                dialog = dialog.add_filter(name, extensions);
            }
            let result = if request.save {
                dialog
                    .set_file_name(&request.file_name)
                    .save_file()
                    .map_or(FileDialogResult::Cancelled, FileDialogResult::Saved)
            } else {
                dialog
                    .pick_files()
                    .map_or(FileDialogResult::Cancelled, FileDialogResult::Opened)
            };
            gfx.screen.file_dialog_finished(result, &mut self.text);
            window.request_redraw();
        }
    }

    fn set_main_visible(&mut self, visible: bool) {
        let (Some(window), Some(gfx)) = (&self.window, &mut self.gfx) else {
            return;
        };
        if visibility_change(self.main_hidden, Some(visible)).is_none() {
            return;
        }
        self.main_hidden = !visible;
        window.set_visible(visible);
        if visible {
            window.focus_window();
            window.request_redraw();
        }
        gfx.screen
            .main_window_visibility_changed(visible, &mut self.text);
    }

    fn apply_main_window_request(&mut self) {
        let request = self
            .gfx
            .as_mut()
            .and_then(|gfx| gfx.screen.take_main_window_visibility());
        if let Some(visible) = visibility_change(self.main_hidden, request) {
            self.set_main_visible(visible);
        }
    }

    fn apply_system_theme(&mut self, theme: Option<winit::window::Theme>) {
        let (Some(window), Some(gfx)) = (&self.window, &mut self.gfx) else {
            return;
        };
        let appearance = match theme {
            Some(winit::window::Theme::Dark) => noa_gpui_core::Appearance::Dark,
            _ => noa_gpui_core::Appearance::Light,
        };
        gfx.screen.set_system_appearance(appearance);
        window.request_redraw();
    }

    fn input(&mut self, event: &WindowEvent) {
        let (Some(window), Some(gfx)) = (&self.window, &mut self.gfx) else {
            return;
        };
        let scale = window.scale_factor();
        gfx.screen
            .tree_mut()
            .set_now(self.clock.elapsed().as_secs_f64());
        for event in self.input.translate(event, scale) {
            gfx.screen.dispatch(event, &mut self.text);
        }
        sync_cursor(window, &mut self.cursor, gfx.screen.tree().cursor());
        let (width, height) = gfx.ctx.size();
        let logical = Viewport {
            width,
            height,
            scale_factor: scale as f32,
        }
        .logical_size();
        if gfx.screen.needs_update(logical) {
            window.request_redraw();
        }
    }

    /// Hand the files the OS dropped on a window (one event per file) to its tree as
    /// one drop.
    fn deliver_drops(&mut self) {
        let now = self.clock.elapsed().as_secs_f64();
        if let Some(drop) = self.input.take_drop()
            && let (Some(window), Some(gfx)) = (&self.window, &mut self.gfx)
        {
            gfx.screen.tree_mut().set_now(now);
            gfx.screen.dispatch(drop, &mut self.text);
            window.request_redraw();
        }
        #[cfg(feature = "native")]
        for window in &mut self.windows {
            window.deliver_drop(&mut self.text, now);
        }
    }
}

/// Enable the IME only while a text field is focused, and keep its candidate window
/// next to the caret.
pub(crate) fn sync_ime(
    window: &Window,
    current: &mut Option<noa_gpui_scene::Rect>,
    area: Option<noa_gpui_scene::Rect>,
) {
    if area == *current {
        return;
    }
    match area {
        Some(rect) => {
            if current.is_none() {
                window.set_ime_allowed(true);
            }
            window.set_ime_cursor_area(
                winit::dpi::LogicalPosition::new(rect.x as f64, rect.y as f64),
                winit::dpi::LogicalSize::new(rect.width as f64, rect.height as f64),
            );
        }
        None => window.set_ime_allowed(false),
    }
    *current = area;
}

pub(crate) fn sync_cursor(
    window: &Window,
    current: &mut noa_gpui_core::Cursor,
    cursor: noa_gpui_core::Cursor,
) {
    if cursor != *current {
        window.set_cursor(cursor_icon(cursor));
        *current = cursor;
    }
}

fn cursor_icon(cursor: noa_gpui_core::Cursor) -> winit::window::CursorIcon {
    use noa_gpui_core::Cursor;
    use winit::window::CursorIcon;
    match cursor {
        Cursor::Default => CursorIcon::Default,
        Cursor::Pointer => CursorIcon::Pointer,
        Cursor::Text => CursorIcon::Text,
        Cursor::Crosshair => CursorIcon::Crosshair,
        Cursor::Move => CursorIcon::Move,
        Cursor::Grab => CursorIcon::Grab,
        Cursor::Grabbing => CursorIcon::Grabbing,
        Cursor::NotAllowed => CursorIcon::NotAllowed,
        Cursor::EwResize => CursorIcon::EwResize,
        Cursor::NsResize => CursorIcon::NsResize,
        Cursor::NwseResize => CursorIcon::NwseResize,
        Cursor::NeswResize => CursorIcon::NeswResize,
        Cursor::ColResize => CursorIcon::ColResize,
        Cursor::RowResize => CursorIcon::RowResize,
    }
}

/// The visibility to switch the main window to, if `request` changes it.
fn visibility_change(hidden: bool, request: Option<bool>) -> Option<bool> {
    request.filter(|&visible| visible == hidden)
}

/// Whether a frame must deliver animation frames or a due timer to `tree`.
pub(crate) fn wants_frame(tree: &noa_gpui_core::UiTree, now: f64) -> bool {
    tree.wants_animation_frame() || tree.next_timer().is_some_and(|t| t <= now)
}

impl ApplicationHandler<Wake> for App {
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: Wake) {
        #[cfg(all(feature = "native", target_os = "macos"))]
        if crate::reopen::take() {
            self.set_main_visible(true);
            if let Some(gfx) = &mut self.gfx {
                gfx.screen.reopened(&mut self.text);
            }
            self.apply_main_window_request();
        }
        if !self.waker.take() {
            return;
        }
        #[cfg(feature = "native")]
        self.apply_stylesheet();
        let (Some(window), Some(gfx)) = (&self.window, &mut self.gfx) else {
            return;
        };
        gfx.screen
            .tree_mut()
            .set_now(self.clock.elapsed().as_secs_f64());
        gfx.screen.woken(&mut self.text);
        window.request_redraw();
        self.apply_main_window_request();
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let window = Arc::new(
                event_loop
                    .create_window(self.window_attributes(event_loop))
                    .expect("create window"),
            );
            #[cfg(feature = "native")]
            {
                self.accessibility = Some(crate::accessibility::Accessibility::attach(
                    event_loop, &window,
                ));
            }
            self.window = Some(window);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.gfx.is_none() {
            self.init_gpu(event_loop);
        }
        let theme = self.window.as_ref().and_then(|w| w.theme());
        self.apply_system_theme(theme);
        #[cfg(all(
            feature = "native",
            any(target_os = "macos", target_os = "windows", target_os = "linux")
        ))]
        self.sync_system_menus();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        #[cfg(feature = "native")]
        if let Some(index) = self.windows.iter().position(|w| w.id() == id) {
            let (Some(instance), Some(gfx)) = (&self.instance, &self.gfx) else {
                return;
            };
            let now = self.clock.elapsed().as_secs_f64();
            let keep =
                self.windows[index].window_event(event, instance, &gfx.ctx, &mut self.text, now);
            let detached = self.windows[index].take_detached();
            if !keep {
                self.windows.remove(index).close(&mut self.text);
            }
            // An inner window's content coming back from its own OS window.
            if let (Some(detached), Some(gfx), Some(window)) =
                (detached, &mut self.gfx, &self.window)
            {
                gfx.screen.adopt(detached, &mut self.text);
                window.request_redraw();
            }
            return;
        }
        let _ = id;
        #[cfg(feature = "native")]
        if let (Some(accessibility), Some(window)) = (&mut self.accessibility, &self.window) {
            accessibility.process_event(window, &event);
        }
        match event {
            WindowEvent::CloseRequested if self.options.keep_running => {
                self.set_main_visible(false);
            }
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gfx) = &mut self.gfx {
                    gfx.ctx.resize(size.width, size.height);
                    self.needs_present = true;
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::ThemeChanged(theme) => {
                self.apply_system_theme(Some(theme));
                #[cfg(feature = "native")]
                for window in &mut self.windows {
                    window.set_system_appearance(theme);
                }
            }
            event => self.input(&event),
        }
        #[cfg(all(
            feature = "native",
            any(target_os = "macos", target_os = "windows", target_os = "linux")
        ))]
        self.sync_system_menus();
        #[cfg(feature = "native")]
        self.open_requested_windows(event_loop);
        self.apply_main_window_request();
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        #[cfg(feature = "native")]
        self.save_view_state();
    }

    /// Items chosen from the system menus arrive between window events; timers are
    /// scheduled once the queue is empty.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.deliver_drops();
        #[cfg(all(
            feature = "native",
            any(target_os = "macos", target_os = "windows", target_os = "linux")
        ))]
        self.sync_system_menus();
        #[cfg(feature = "native")]
        self.open_requested_windows(event_loop);
        #[cfg(all(
            feature = "native",
            any(target_os = "macos", target_os = "windows", target_os = "linux")
        ))]
        self.run_file_dialogs();
        self.apply_main_window_request();
        // Async wakes and tray/menu actions can request exit while the main window
        // is hidden, when there may never be another window event or redraw.
        if self
            .gfx
            .as_ref()
            .is_some_and(|gfx| gfx.screen.wants_close())
        {
            event_loop.exit();
            return;
        }
        self.schedule_timers(event_loop);
    }
}

/// Whether a window placed with its outer top-left corner at `position` (physical)
/// would show its title bar on one of the monitors connected now.
#[cfg(feature = "native")]
fn on_a_monitor(event_loop: &ActiveEventLoop, (x, y): (i32, i32)) -> bool {
    let (px, py) = (x.saturating_add(48), y.saturating_add(8));
    event_loop.available_monitors().any(|m| {
        let (origin, size) = (m.position(), m.size());
        px >= origin.x
            && py >= origin.y
            && i64::from(px) < i64::from(origin.x) + i64::from(size.width)
            && i64::from(py) < i64::from(origin.y) + i64::from(size.height)
    })
}

#[cfg(test)]
mod tests {
    use super::visibility_change;

    #[test]
    fn only_requests_that_change_the_main_window_apply() {
        assert_eq!(visibility_change(false, Some(false)), Some(false));
        assert_eq!(visibility_change(false, Some(true)), None);
        assert_eq!(visibility_change(true, Some(true)), Some(true));
        assert_eq!(visibility_change(true, Some(false)), None);
        assert_eq!(visibility_change(true, None), None);
    }
}

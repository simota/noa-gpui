//! wry's web views (WKWebView on macOS, WebView2 on Windows, WebKitGTK on Linux) as
//! children of a winit window, over what the window draws.
//!
//! A child view cannot be clipped, so it covers only the part of its widget that shows:
//! a widget partly scrolled out of its scroll view has its page laid out in that part.
//! Linux supports X11 only (wry's child views), and GTK's main loop is run from the
//! event loop while views exist.

use std::sync::Arc;

use noa_gpui_core::WidgetId;
use noa_gpui_core::widgets::{WebViewContent, WebViewEvent, WebViewFrame, WebViewRequest};
use noa_gpui_scene::Rect;
use winit::window::Window;
use wry::dpi::{PhysicalPosition, PhysicalSize};
use wry::{PageLoadEvent, WebViewBuilder};

use crate::wake::Waker;
use crate::webview::{Backend, NativeViews, Reports, WebViews, to_pixels};

pub(crate) struct Wry {
    window: Arc<Window>,
}

impl WebViews {
    /// Views for the widgets of the tree `window` shows. `None` where they cannot be
    /// made (Linux without GTK or outside X11).
    pub fn new(window: &Arc<Window>, waker: &Waker) -> Option<Self> {
        #[cfg(target_os = "linux")]
        if !linux::init() {
            return None;
        }
        Some(NativeViews::with_backend(
            Wry {
                window: window.clone(),
            },
            waker.clone(),
        ))
    }
}

/// `visible` (logical points) in the window's physical pixels.
fn bounds(visible: Rect, scale: f64) -> wry::Rect {
    let (x, y, width, height) = to_pixels(visible, scale);
    wry::Rect {
        position: PhysicalPosition::new(x, y).into(),
        size: PhysicalSize::new(width, height).into(),
    }
}

impl Backend for Wry {
    type View = wry::WebView;

    fn create(
        &mut self,
        frame: &WebViewFrame,
        content: &WebViewContent,
        scale: f64,
        reports: &Reports,
    ) -> Option<wry::WebView> {
        let visible = frame.visible?;
        let id = frame.id;
        let (loads, titles) = (reports.clone(), reports.clone());
        let builder = WebViewBuilder::new()
            .with_bounds(bounds(visible, scale))
            .with_on_page_load_handler(move |event, url| {
                loads.send(
                    id,
                    match event {
                        PageLoadEvent::Started => WebViewEvent::Navigated(url),
                        PageLoadEvent::Finished => WebViewEvent::Loaded(url),
                    },
                );
            })
            .with_document_title_changed_handler(move |title| {
                titles.send(id, WebViewEvent::TitleChanged(title));
            });
        let builder = match content {
            WebViewContent::Url(url) => builder.with_url(url),
            WebViewContent::Html(html) => builder.with_html(html),
        };
        builder
            .build_as_child(self.window.as_ref())
            .inspect_err(|e| log::warn!("no native view for web view {id:?}: {e}"))
            .ok()
    }

    fn place(&mut self, view: &mut wry::WebView, frame: &WebViewFrame, scale: f64) {
        let placed = match frame.visible {
            Some(visible) => view
                .set_bounds(bounds(visible, scale))
                .and_then(|()| view.set_visible(true)),
            None => view.set_visible(false),
        };
        if let Err(e) = placed {
            log::warn!("web view {:?} not placed: {e}", frame.id);
        }
    }

    fn request(
        &mut self,
        view: &mut wry::WebView,
        id: WidgetId,
        request: &WebViewRequest,
        _reports: &Reports,
    ) {
        let done = match request {
            WebViewRequest::LoadUrl(url) => view.load_url(url),
            WebViewRequest::LoadHtml(html) => view.load_html(html),
            WebViewRequest::Reload => view.reload(),
            WebViewRequest::Back => view.evaluate_script("history.back()"),
            WebViewRequest::Forward => view.evaluate_script("history.forward()"),
        };
        if let Err(e) = done {
            log::warn!("web view {id:?}: {request:?} failed: {e}");
        }
    }

    #[cfg(target_os = "linux")]
    fn poll(&mut self, has_views: bool) -> bool {
        linux::run_pending();
        has_views
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use gtk::prelude::DisplayExtManual;

    /// Initialize GTK on this (the main) thread, once. Whether wry's views can be used:
    /// they are X11 windows.
    pub fn init() -> bool {
        if !gtk::is_initialized_main_thread()
            && let Err(e) = gtk::init()
        {
            log::warn!("no native web views: GTK did not start: {e}");
            return false;
        }
        let x11 = gtk::gdk::Display::default().is_some_and(|d| d.backend().is_x11());
        if !x11 {
            log::warn!("no native web views: they need X11");
        }
        x11
    }

    /// Let GTK handle what is waiting (the views draw and take input through it).
    pub fn run_pending() {
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
    }
}

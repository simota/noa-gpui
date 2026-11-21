//! The `<iframe>`s of [`noa_gpui_core::widgets::WebView`] widgets on the Web, placed
//! over the canvas. Each sits in a box that clips it to the part of its widget that
//! shows.
//!
//! A page from another origin keeps its URL, title and history to itself: its frame
//! reports the URL it was told to load, no title, and ignores back and forward.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use noa_gpui_core::WidgetId;
use noa_gpui_core::widgets::{WebViewContent, WebViewEvent, WebViewFrame, WebViewRequest};
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{Document, HtmlCanvasElement, HtmlElement, HtmlIFrameElement};
use winit::window::Window;

use crate::wake::Waker;
use crate::webview::{Backend, NativeViews, Reports, WebViews};

/// The URL a frame showing HTML text has.
const SRCDOC_URL: &str = "about:srcdoc";

pub(crate) struct Frames {
    document: Document,
    canvas: HtmlCanvasElement,
}

pub(crate) struct Frame {
    /// Clips the iframe.
    clip: HtmlElement,
    iframe: HtmlIFrameElement,
    /// What the iframe was last told to load.
    requested: Rc<RefCell<String>>,
    on_load: Closure<dyn FnMut(web_sys::Event)>,
}

impl Drop for Frame {
    fn drop(&mut self) {
        let _ = self
            .iframe
            .remove_event_listener_with_callback("load", self.on_load.as_ref().unchecked_ref());
        self.clip.remove();
    }
}

impl WebViews {
    /// Frames over the canvas of `window`. `None` without a canvas or document.
    pub fn new(window: &Arc<Window>, waker: &Waker) -> Option<Self> {
        use winit::platform::web::WindowExtWebSys;
        let canvas = window.canvas()?;
        let document = web_sys::window()?.document()?;
        Some(NativeViews::with_backend(
            Frames { document, canvas },
            waker.clone(),
        ))
    }
}

impl Frames {
    /// The canvas's top-left corner in document coordinates (CSS pixels, which are the
    /// tree's logical points).
    fn origin(&self) -> (f64, f64) {
        let rect = self.canvas.get_bounding_client_rect();
        let scroll = web_sys::window().map_or((0.0, 0.0), |w| {
            (w.scroll_x().unwrap_or(0.0), w.scroll_y().unwrap_or(0.0))
        });
        (rect.left() + scroll.0, rect.top() + scroll.1)
    }

    fn frame(&self, id: WidgetId, reports: &Reports) -> Option<Frame> {
        let clip: HtmlElement = self.document.create_element("div").ok()?.dyn_into().ok()?;
        let iframe: HtmlIFrameElement = self
            .document
            .create_element("iframe")
            .ok()?
            .dyn_into()
            .ok()?;
        set_styles(
            &clip,
            &[
                ("position", "absolute"),
                ("overflow", "hidden"),
                ("margin", "0"),
                ("padding", "0"),
                ("border", "0"),
            ],
        );
        set_styles(
            &iframe,
            &[
                ("position", "absolute"),
                ("display", "block"),
                ("margin", "0"),
                ("border", "0"),
            ],
        );
        let _ = iframe.set_attribute("title", "Web content");
        let requested = Rc::new(RefCell::new(String::new()));
        let on_load = {
            let (iframe, requested, reports) = (iframe.clone(), requested.clone(), reports.clone());
            Closure::<dyn FnMut(web_sys::Event)>::new(move |_: web_sys::Event| {
                // Another origin's location cannot be read.
                let url = iframe
                    .content_window()
                    .and_then(|w| w.location().href().ok())
                    .unwrap_or_else(|| requested.borrow().clone());
                reports.send(id, WebViewEvent::Loaded(url));
                if let Some(title) = iframe.content_document().map(|d| d.title())
                    && !title.is_empty()
                {
                    reports.send(id, WebViewEvent::TitleChanged(title));
                }
            })
        };
        iframe
            .add_event_listener_with_callback("load", on_load.as_ref().unchecked_ref())
            .ok()?;
        clip.append_child(&iframe).ok()?;
        self.document.body()?.append_child(&clip).ok()?;
        Some(Frame {
            clip,
            iframe,
            requested,
            on_load,
        })
    }
}

fn set_styles(element: &HtmlElement, styles: &[(&str, &str)]) {
    let style = element.style();
    for (name, value) in styles {
        let _ = style.set_property(name, value);
    }
}

fn px(value: f64) -> String {
    format!("{value}px")
}

fn load(frame: &Frame, id: WidgetId, content: &WebViewContent, reports: &Reports) {
    let url = match content {
        WebViewContent::Url(url) => {
            // `srcdoc` wins over `src`.
            let _ = frame.iframe.remove_attribute("srcdoc");
            frame.iframe.set_src(url);
            url.as_str()
        }
        WebViewContent::Html(html) => {
            frame.iframe.set_srcdoc(html);
            SRCDOC_URL
        }
    };
    *frame.requested.borrow_mut() = url.to_owned();
    reports.send(id, WebViewEvent::Navigated(url.to_owned()));
}

impl Backend for Frames {
    type View = Frame;

    fn create(
        &mut self,
        frame: &WebViewFrame,
        content: &WebViewContent,
        scale: f64,
        reports: &Reports,
    ) -> Option<Frame> {
        let mut view = self.frame(frame.id, reports)?;
        self.place(&mut view, frame, scale);
        load(&view, frame.id, content, reports);
        Some(view)
    }

    fn place(&mut self, view: &mut Frame, frame: &WebViewFrame, _scale: f64) {
        let Some(visible) = frame.visible else {
            set_styles(&view.clip, &[("display", "none")]);
            return;
        };
        let (x, y) = self.origin();
        let (left, top) = (px(x + f64::from(visible.x)), px(y + f64::from(visible.y)));
        let (width, height) = (px(visible.width.into()), px(visible.height.into()));
        set_styles(
            &view.clip,
            &[
                ("display", "block"),
                ("left", &left),
                ("top", &top),
                ("width", &width),
                ("height", &height),
            ],
        );
        let rect = frame.rect;
        let (left, top) = (
            px(f64::from(rect.x - visible.x)),
            px(f64::from(rect.y - visible.y)),
        );
        let (width, height) = (px(rect.width.into()), px(rect.height.into()));
        set_styles(
            &view.iframe,
            &[
                ("left", &left),
                ("top", &top),
                ("width", &width),
                ("height", &height),
            ],
        );
    }

    fn request(
        &mut self,
        view: &mut Frame,
        id: WidgetId,
        request: &WebViewRequest,
        reports: &Reports,
    ) {
        match request {
            WebViewRequest::LoadUrl(url) => {
                load(view, id, &WebViewContent::Url(url.clone()), reports)
            }
            WebViewRequest::LoadHtml(html) => {
                load(view, id, &WebViewContent::Html(html.clone()), reports)
            }
            WebViewRequest::Reload => {
                let content = match view.iframe.get_attribute("srcdoc") {
                    Some(html) => WebViewContent::Html(html),
                    None => WebViewContent::Url(view.iframe.src()),
                };
                load(view, id, &content, reports);
            }
            WebViewRequest::Back | WebViewRequest::Forward => {
                let Some(history) = view.iframe.content_window().and_then(|w| w.history().ok())
                else {
                    log::debug!("web view {id:?}: no history to go through (another origin)");
                    return;
                };
                let went = if *request == WebViewRequest::Back {
                    history.back()
                } else {
                    history.forward()
                };
                if went.is_err() {
                    log::debug!("web view {id:?}: {request:?} refused (another origin)");
                }
            }
        }
    }
}

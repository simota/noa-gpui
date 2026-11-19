use noa_gpui_layout::Style;
use noa_gpui_layout::style::{Dimension, Size as LayoutSize, percent};
use noa_gpui_scene::{Border, Rect};
use noa_gpui_text::{TextStore, TextStyle};

use super::badge::centered;
use super::{CachedText, view_height};
use crate::event::Event;
use crate::theme::Role;
use crate::tree::{UiTree, WidgetId};
use crate::widget::{EventCx, PaintCx, UpdateCx, Widget};

const PAD: f32 = 12.0;
const MIN_HEIGHT: f32 = 120.0;

/// What a [`WebView`] shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebViewContent {
    Url(String),
    /// A page given as HTML text.
    Html(String),
}

/// What the app asked a [`WebView`] to do, for the platform layer to carry out on the
/// native view (see [`WebView::take_requests`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebViewRequest {
    LoadUrl(String),
    LoadHtml(String),
    Reload,
    Back,
    Forward,
}

/// What the native view reported. The platform layer hands it to the widget with
/// [`WebView::report`], and the widget emits it as a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebViewEvent {
    /// The view started loading `url`: the app's request, a link, a redirect.
    Navigated(String),
    /// The page at `url` finished loading.
    Loaded(String),
    TitleChanged(String),
}

/// A web page shown by the OS's web view (WKWebView, WebView2, WebKitGTK) or, on the
/// Web, an `<iframe>`. The page is not drawn by the GPU: the platform layer places a
/// native view over the window where this widget is laid out, so the widget itself only
/// takes the space and paints a placeholder (the URL), which shows where there is no
/// native view (Linux without the `webview-linux` feature, drawing without a window).
///
/// The native view sits on top of everything the window draws: menus, popovers and
/// sheets that overlap the widget show below it.
///
/// It fills the height its parent leaves unless [`WebView::height`] fixes it. The
/// platform layer reports navigation and title changes, which are emitted as
/// [`WebViewEvent`] messages.
#[derive(Debug)]
pub struct WebView {
    content: WebViewContent,
    height: Option<f32>,
    requests: Vec<WebViewRequest>,
    /// Reported by the native view and not yet emitted.
    reported: Vec<WebViewEvent>,
    url: Option<String>,
    title: String,
    loading: bool,
    caption: CachedText,
}

impl WebView {
    pub fn new(url: impl Into<String>) -> Self {
        Self::with_content(WebViewContent::Url(url.into()))
    }

    /// A page given as HTML text.
    pub fn html(html: impl Into<String>) -> Self {
        Self::with_content(WebViewContent::Html(html.into()))
    }

    fn with_content(content: WebViewContent) -> Self {
        Self {
            content,
            height: None,
            requests: Vec::new(),
            reported: Vec::new(),
            url: None,
            title: String::new(),
            loading: false,
            caption: CachedText::default(),
        }
    }

    /// A fixed height instead of filling the parent.
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height.max(0.0));
        self
    }

    /// What the app asked for last, which a native view created now loads.
    pub fn content(&self) -> &WebViewContent {
        &self.content
    }

    /// The URL the view last reported, or else the one the app asked for (empty for
    /// HTML content).
    pub fn url(&self) -> &str {
        match (&self.url, &self.content) {
            (Some(url), _) => url,
            (None, WebViewContent::Url(url)) => url,
            (None, WebViewContent::Html(_)) => "",
        }
    }

    /// The page's title as the view last reported it.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Between [`WebViewEvent::Navigated`] and [`WebViewEvent::Loaded`].
    pub fn is_loading(&self) -> bool {
        self.loading
    }

    pub fn load_url(&mut self, cx: &mut UpdateCx, url: impl Into<String>) {
        let url = url.into();
        self.content = WebViewContent::Url(url.clone());
        self.url = None;
        self.request(cx, WebViewRequest::LoadUrl(url));
    }

    pub fn load_html(&mut self, cx: &mut UpdateCx, html: impl Into<String>) {
        let html = html.into();
        self.content = WebViewContent::Html(html.clone());
        self.url = None;
        self.request(cx, WebViewRequest::LoadHtml(html));
    }

    pub fn reload(&mut self, cx: &mut UpdateCx) {
        self.request(cx, WebViewRequest::Reload);
    }

    /// The page before this one in the view's history.
    pub fn go_back(&mut self, cx: &mut UpdateCx) {
        self.request(cx, WebViewRequest::Back);
    }

    pub fn go_forward(&mut self, cx: &mut UpdateCx) {
        self.request(cx, WebViewRequest::Forward);
    }

    fn request(&mut self, cx: &mut UpdateCx, request: WebViewRequest) {
        self.requests.push(request);
        // The placeholder shows the URL, and a changed frame is what makes the
        // platform layer look at the requests.
        cx.request_paint();
    }

    /// Requests not yet taken, oldest first.
    pub fn requests(&self) -> &[WebViewRequest] {
        &self.requests
    }

    /// For the platform layer: the requests to carry out on the native view, oldest
    /// first. When it creates the view, it loads [`WebView::content`] instead, which
    /// already covers the loads asked for before.
    pub fn take_requests(&mut self) -> Vec<WebViewRequest> {
        std::mem::take(&mut self.requests)
    }

    /// For the platform layer: what the native view reported. The widget's state
    /// changes at once; the message is emitted on the next frame (through a timer due
    /// now, so it reaches the app as other widgets' messages do).
    pub fn report(&mut self, cx: &mut UpdateCx, event: WebViewEvent) {
        match &event {
            WebViewEvent::Navigated(url) => {
                self.url = Some(url.clone());
                self.loading = true;
            }
            WebViewEvent::Loaded(url) => {
                self.url = Some(url.clone());
                self.loading = false;
            }
            WebViewEvent::TitleChanged(title) => self.title = title.clone(),
        }
        self.reported.push(event);
        cx.request_timer(0.0);
        cx.request_paint();
    }

    fn caption(&self) -> &str {
        match &self.content {
            WebViewContent::Html(_) if self.url.is_none() => "HTML content",
            _ => self.url(),
        }
    }
}

impl Widget for WebView {
    fn style(&self) -> Style {
        let mut style = Style {
            size: LayoutSize {
                width: percent(1.0),
                height: Dimension::auto(),
            },
            ..Default::default()
        };
        view_height(&mut style, self.height, MIN_HEIGHT);
        style
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let background = cx.color(Role::Surface);
        let outline = cx.color(Role::Outline);
        cx.rounded_rect(
            bounds,
            6.0,
            background,
            Border {
                width: 1.0,
                color: outline,
            },
        );
        let wrap = (bounds.width - 2.0 * PAD).max(1.0);
        let caption = self.caption().to_owned();
        let size = self
            .caption
            .shape(cx.text_store(), &caption, TextStyle::new(13.0), Some(wrap));
        if let Some(layout) = self.caption.layout() {
            let area = Rect::new(PAD, PAD, wrap, (bounds.height - 2.0 * PAD).max(0.0));
            let color = cx.color(Role::Muted);
            cx.push_clip(area);
            cx.text(centered(area, size), layout, color);
            cx.pop_clip();
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        if let Event::Timer { .. } = event {
            for reported in self.reported.drain(..) {
                cx.emit(reported);
            }
        }
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::WebView)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        let name = if self.title.is_empty() {
            self.caption()
        } else {
            &self.title
        };
        node.set_label(name.to_owned());
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.caption.release(text);
    }
}

/// Where a [`WebView`] is, as [`web_views`] finds it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WebViewFrame {
    pub id: WidgetId,
    /// The whole widget, in window coordinates (logical points).
    pub rect: Rect,
    /// The part of it that shows: clipped by scroll views and other clipping ancestors
    /// and by the window. `None` while it is hidden or out of sight.
    pub visible: Option<Rect>,
}

/// Every [`WebView`] in `tree`, including those inside overlays, hidden ones too, in
/// tree order. Reflects the last [`UiTree::prepare`].
pub fn web_views(tree: &UiTree) -> Vec<WebViewFrame> {
    let mut found = Vec::new();
    let mut stack: Vec<WidgetId> = tree.root().into_iter().chain(tree.overlays()).collect();
    stack.reverse();
    while let Some(id) = stack.pop() {
        stack.extend(tree.children(id).iter().rev());
        if tree.get::<WebView>(id).is_none() {
            continue;
        }
        let Some(rect) = tree.window_rect(id) else {
            continue;
        };
        let visible = if tree.is_hidden(id) {
            None
        } else {
            tree.visible_rect(id)
        };
        found.push(WebViewFrame { id, rect, visible });
    }
    found
}

//! Native web views for the `WebView` widgets of a window's tree: each frame that
//! changed something, every widget gets a view placed over the part of it that shows,
//! hidden while it is hidden or out of sight, and destroyed with the widget. A view is
//! created the first time its widget shows, loading `WebView::content`; after that the
//! widget's requests (load, reload, back, forward) are carried out on it. What the view
//! reports (navigation, title) waits in `Reports` until the next frame hands it to
//! the widget, which emits it as a message.
//!
//! Desktop views are wry's (`webview_native`, macOS and Windows with `native`, Linux
//! with `webview-linux`); the Web's are `<iframe>`s (`webview_web`). Elsewhere
//! `WebViews` never exists and the widgets show their placeholder.

#[cfg(any(
    all(feature = "native", any(target_os = "macos", target_os = "windows")),
    all(feature = "webview-linux", target_os = "linux"),
    target_arch = "wasm32",
    test
))]
pub(crate) use views::*;

#[cfg(any(
    all(feature = "native", any(target_os = "macos", target_os = "windows")),
    all(feature = "webview-linux", target_os = "linux"),
))]
pub(crate) type WebViews = NativeViews<crate::webview_native::Wry>;

#[cfg(target_arch = "wasm32")]
pub(crate) type WebViews = NativeViews<crate::webview_web::Frames>;

/// No native web views on this platform: `WebViews::new` gives `None`.
#[cfg(not(any(
    all(feature = "native", any(target_os = "macos", target_os = "windows")),
    all(feature = "webview-linux", target_os = "linux"),
    target_arch = "wasm32",
)))]
pub(crate) enum WebViews {}

#[cfg(not(any(
    all(feature = "native", any(target_os = "macos", target_os = "windows")),
    all(feature = "webview-linux", target_os = "linux"),
    target_arch = "wasm32",
)))]
impl WebViews {
    pub fn new(
        _window: &std::sync::Arc<winit::window::Window>,
        _waker: &crate::Waker,
    ) -> Option<Self> {
        None
    }

    pub fn has_reports(&self) -> bool {
        match *self {}
    }

    pub fn deliver_reports(&self, _tree: &mut noa_gpui_core::UiTree) {
        match *self {}
    }

    pub fn sync(&mut self, _tree: &mut noa_gpui_core::UiTree, _scale: f64, _changed: bool) {
        match *self {}
    }

    pub fn poll(&mut self) -> bool {
        match *self {}
    }
}

#[cfg(any(
    all(feature = "native", any(target_os = "macos", target_os = "windows")),
    all(feature = "webview-linux", target_os = "linux"),
    target_arch = "wasm32",
    test
))]
mod views {
    use std::cell::RefCell;
    use std::rc::Rc;

    use noa_gpui_core::widgets::{
        WebView, WebViewContent, WebViewEvent, WebViewFrame, WebViewRequest, web_views,
    };
    use noa_gpui_core::{UiTree, WidgetId};

    use crate::wake::Waker;

    /// What native views reported, waiting for their widgets. Sending one wakes the
    /// event loop, which then redraws the window.
    #[derive(Clone)]
    pub(crate) struct Reports {
        queue: Rc<RefCell<Vec<(WidgetId, WebViewEvent)>>>,
        waker: Waker,
    }

    impl Reports {
        pub fn new(waker: Waker) -> Self {
            Self {
                queue: Rc::default(),
                waker,
            }
        }

        pub fn send(&self, id: WidgetId, event: WebViewEvent) {
            self.queue.borrow_mut().push((id, event));
            self.waker.nudge();
        }
    }

    /// Makes and moves the native views.
    pub(crate) trait Backend {
        type View;

        /// A view for the widget at `frame`, which shows (`frame.visible` is `Some`),
        /// loading `content`. `None` if it cannot be made; it is not asked again for
        /// that widget.
        fn create(
            &mut self,
            frame: &WebViewFrame,
            content: &WebViewContent,
            scale: f64,
            reports: &Reports,
        ) -> Option<Self::View>;

        /// Move `view` to `frame`, or hide it when `frame.visible` is `None`.
        fn place(&mut self, view: &mut Self::View, frame: &WebViewFrame, scale: f64);

        fn request(
            &mut self,
            view: &mut Self::View,
            id: WidgetId,
            request: &WebViewRequest,
            reports: &Reports,
        );

        /// Called once per pass of the event loop while the app waits. Returns whether
        /// the loop must not sleep until the next event (the views need it to run).
        fn poll(&mut self, _has_views: bool) -> bool {
            false
        }
    }

    struct Native<V> {
        id: WidgetId,
        view: V,
        /// Where the view was last put, with the scale factor then.
        placed: (WebViewFrame, f64),
    }

    pub(crate) struct NativeViews<B: Backend> {
        /// Declared before `backend`, which may hold the last reference to the window
        /// the views are children of: fields drop in order, so the views go first.
        views: Vec<Native<B::View>>,
        backend: B,
        /// Widgets whose view could not be made.
        failed: Vec<WidgetId>,
        reports: Reports,
        /// The scale factor of the last sync, `None` before the first.
        scale: Option<f64>,
    }

    impl<B: Backend> NativeViews<B> {
        pub fn with_backend(backend: B, waker: Waker) -> Self {
            Self {
                backend,
                views: Vec::new(),
                failed: Vec::new(),
                reports: Reports::new(waker),
                scale: None,
            }
        }

        pub fn has_reports(&self) -> bool {
            !self.reports.queue.borrow().is_empty()
        }

        /// Hand what the views reported to their widgets. Call it before animations
        /// run, so the messages come out in the same frame.
        pub fn deliver_reports(&self, tree: &mut UiTree) {
            let reported = std::mem::take(&mut *self.reports.queue.borrow_mut());
            for (id, event) in reported {
                tree.update::<WebView, _>(id, |w, cx| w.report(cx, event));
            }
        }

        /// Make the views follow the widgets of `tree`, laid out at `scale` physical
        /// pixels per point. Call it after layout; `changed` is whether that frame
        /// changed anything (only then can a widget have moved, shown, hidden, gone
        /// or asked for something).
        pub fn sync(&mut self, tree: &mut UiTree, scale: f64, changed: bool) {
            if !changed && self.scale == Some(scale) {
                return;
            }
            self.scale = Some(scale);
            let frames = web_views(tree);
            let alive = |id: &WidgetId| frames.iter().any(|f| f.id == *id);
            // Dropping a view destroys it.
            self.views.retain(|v| alive(&v.id));
            self.failed.retain(alive);
            for frame in frames {
                let requests = tree
                    .update::<WebView, _>(frame.id, |w, _| w.take_requests())
                    .unwrap_or_default();
                if let Some(native) = self.views.iter_mut().find(|v| v.id == frame.id) {
                    for request in &requests {
                        self.backend
                            .request(&mut native.view, frame.id, request, &self.reports);
                    }
                    if native.placed != (frame, scale) {
                        self.backend.place(&mut native.view, &frame, scale);
                        native.placed = (frame, scale);
                    }
                    continue;
                }
                // Made when it first shows, loading what was asked for last, which
                // covers the requests taken above.
                if frame.visible.is_none() || self.failed.contains(&frame.id) {
                    continue;
                }
                let Some(content) = tree.get::<WebView>(frame.id).map(|w| w.content().clone())
                else {
                    continue;
                };
                match self.backend.create(&frame, &content, scale, &self.reports) {
                    Some(view) => self.views.push(Native {
                        id: frame.id,
                        view,
                        placed: (frame, scale),
                    }),
                    None => self.failed.push(frame.id),
                }
            }
        }

        /// Called once per pass of the event loop while the app waits. Returns whether
        /// the loop must wake again soon instead of sleeping until the next event.
        pub fn poll(&mut self) -> bool {
            self.backend.poll(!self.views.is_empty())
        }

        #[cfg(test)]
        pub fn backend(&self) -> &B {
            &self.backend
        }
    }
}

/// `rect` (logical points) in physical pixels at `scale`, each edge on the nearest
/// pixel, so views side by side neither overlap nor leave a gap: `(x, y, width,
/// height)`.
#[cfg(any(
    all(feature = "native", any(target_os = "macos", target_os = "windows")),
    all(feature = "webview-linux", target_os = "linux"),
    test
))]
pub(crate) fn to_pixels(rect: noa_gpui_scene::Rect, scale: f64) -> (i32, i32, u32, u32) {
    let edge = |v: f32| (f64::from(v) * scale).round() as i32;
    let (left, top) = (edge(rect.x), edge(rect.y));
    let (right, bottom) = (edge(rect.right()), edge(rect.bottom()));
    (
        left,
        top,
        right.saturating_sub(left).max(0) as u32,
        bottom.saturating_sub(top).max(0) as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use noa_gpui_core::style::{FlexDirection, Size as LayoutSize, length, percent};
    use noa_gpui_core::widgets::{
        Container, ScrollView, WebView, WebViewContent, WebViewEvent, WebViewFrame, WebViewRequest,
    };
    use noa_gpui_core::{Style, UiTree, WidgetId};
    use noa_gpui_scene::{Rect, Size};
    use noa_gpui_text::{FontOptions, TextStore};

    /// What the views were asked to do, in order.
    #[derive(Debug, Clone, PartialEq)]
    enum Call {
        Create(WidgetId, WebViewContent, Option<Rect>),
        Place(WidgetId, Option<Rect>, f64),
        Request(WidgetId, WebViewRequest),
        Destroy(WidgetId),
        /// The window the views are children of went away.
        CloseWindow,
    }

    /// The window a backend holds and puts its views in, as `Wry` holds an
    /// `Arc<Window>`.
    struct ParentWindow(std::rc::Rc<std::cell::RefCell<Vec<Call>>>);

    impl Drop for ParentWindow {
        fn drop(&mut self) {
            self.0.borrow_mut().push(Call::CloseWindow);
        }
    }

    #[derive(Default)]
    struct Fake {
        calls: std::rc::Rc<std::cell::RefCell<Vec<Call>>>,
        refuse: bool,
        /// Its views need the event loop to keep running, as GTK's do.
        polled: bool,
        window: Option<ParentWindow>,
    }

    struct FakeView {
        id: WidgetId,
        calls: std::rc::Rc<std::cell::RefCell<Vec<Call>>>,
    }

    impl Drop for FakeView {
        fn drop(&mut self) {
            self.calls.borrow_mut().push(Call::Destroy(self.id));
        }
    }

    impl Backend for Fake {
        type View = FakeView;

        fn create(
            &mut self,
            frame: &WebViewFrame,
            content: &WebViewContent,
            _scale: f64,
            reports: &Reports,
        ) -> Option<FakeView> {
            self.calls
                .borrow_mut()
                .push(Call::Create(frame.id, content.clone(), frame.visible));
            if self.refuse {
                return None;
            }
            // As a real view reports its first load.
            if let WebViewContent::Url(url) = content {
                reports.send(frame.id, WebViewEvent::Navigated(url.clone()));
            }
            Some(FakeView {
                id: frame.id,
                calls: self.calls.clone(),
            })
        }

        fn place(&mut self, view: &mut FakeView, frame: &WebViewFrame, scale: f64) {
            self.calls
                .borrow_mut()
                .push(Call::Place(view.id, frame.visible, scale));
        }

        fn request(
            &mut self,
            _view: &mut FakeView,
            id: WidgetId,
            request: &WebViewRequest,
            _reports: &Reports,
        ) {
            self.calls
                .borrow_mut()
                .push(Call::Request(id, request.clone()));
        }

        fn poll(&mut self, has_views: bool) -> bool {
            self.polled && has_views
        }
    }

    struct Window {
        tree: UiTree,
        text: TextStore,
        scroll: WidgetId,
        web: WidgetId,
        views: NativeViews<Fake>,
    }

    impl Window {
        /// A 300×200 scroll view holding 100 points of space, a 150-point web view and
        /// 400 more points.
        fn new(fake: Fake) -> Self {
            let mut tree = UiTree::new();
            let root = tree.insert(None, Container::new(Default::default()));
            let scroll = tree.insert(
                Some(root),
                ScrollView::new(Style {
                    size: LayoutSize {
                        width: length(300.0),
                        height: length(200.0),
                    },
                    ..Default::default()
                }),
            );
            let content = tree.insert(
                Some(scroll),
                Container::new(Style {
                    flex_direction: FlexDirection::Column,
                    size: LayoutSize {
                        width: percent(1.0),
                        height: noa_gpui_core::style::auto(),
                    },
                    ..Default::default()
                }),
            );
            let block = |height| {
                Container::new(Style {
                    size: LayoutSize {
                        width: percent(1.0),
                        height: length(height),
                    },
                    flex_shrink: 0.0,
                    ..Default::default()
                })
            };
            tree.insert(Some(content), block(100.0));
            let web = tree.insert(
                Some(content),
                WebView::new("https://example.com").height(150.0),
            );
            tree.insert(Some(content), block(400.0));
            let mut window = Self {
                tree,
                text: TextStore::new(FontOptions {
                    locale: "en-US".into(),
                    load_system_fonts: false,
                }),
                scroll,
                web,
                views: NativeViews::with_backend(fake, crate::Waker::default()),
            };
            window.frame(1.0);
            window
        }

        /// One frame as the app draws it.
        fn frame(&mut self, scale: f64) {
            self.views.deliver_reports(&mut self.tree);
            self.tree.animate(self.tree.now(), &mut self.text);
            let changed = self
                .tree
                .prepare(Size::new(400.0, 300.0), &mut self.text)
                .is_some();
            self.views.sync(&mut self.tree, scale, changed);
        }

        fn scroll_to(&mut self, offset: f32) {
            self.tree
                .update::<ScrollView, _>(self.scroll, |s, cx| s.set_offset(cx, offset));
            self.frame(1.0);
        }

        fn take_calls(&self) -> Vec<Call> {
            std::mem::take(&mut *self.views.backend().calls.borrow_mut())
        }
    }

    #[test]
    fn a_view_follows_its_widget_as_it_scrolls_and_hides_once_out_of_sight() {
        let mut w = Window::new(Fake::default());
        let url = WebViewContent::Url("https://example.com".into());
        assert_eq!(
            w.take_calls(),
            [Call::Create(
                w.web,
                url,
                Some(Rect::new(0.0, 100.0, 300.0, 100.0))
            )]
        );
        w.frame(1.0);
        assert_eq!(w.take_calls(), [], "nothing changed, nothing to do");

        w.scroll_to(60.0);
        assert_eq!(
            w.take_calls(),
            [Call::Place(
                w.web,
                Some(Rect::new(0.0, 40.0, 300.0, 150.0)),
                1.0
            )]
        );
        w.scroll_to(400.0);
        assert_eq!(w.take_calls(), [Call::Place(w.web, None, 1.0)]);
        w.scroll_to(0.0);
        assert_eq!(
            w.take_calls(),
            [Call::Place(
                w.web,
                Some(Rect::new(0.0, 100.0, 300.0, 100.0)),
                1.0
            )],
            "the same view shows again"
        );
    }

    #[test]
    fn a_new_scale_factor_moves_the_view_even_when_the_layout_stays() {
        let mut w = Window::new(Fake::default());
        w.take_calls();
        w.frame(2.0);
        assert_eq!(
            w.take_calls(),
            [Call::Place(
                w.web,
                Some(Rect::new(0.0, 100.0, 300.0, 100.0)),
                2.0
            )]
        );
    }

    #[test]
    fn a_view_is_made_only_once_its_widget_shows_and_goes_with_it() {
        let mut w = Window::new(Fake::default());
        // At the top of the scroll view's content, hidden.
        let hidden = w.tree.insert(
            w.tree.parent(w.web),
            WebView::html("<p>Hi</p>").height(50.0),
        );
        w.tree.move_child(hidden, 0);
        w.tree.set_hidden(hidden, true);
        w.tree
            .update::<WebView, _>(hidden, |v, cx| v.load_html(cx, "<p>Later</p>"));
        w.take_calls();
        w.frame(1.0);
        assert_eq!(w.take_calls(), [], "hidden: no view yet");
        assert!(
            w.tree.get::<WebView>(hidden).unwrap().requests().is_empty(),
            "requests made before the view exists are covered by its content"
        );

        w.tree.set_hidden(hidden, false);
        w.frame(1.0);
        // It loads the last content asked for; the other view moves down below it.
        assert_eq!(
            w.take_calls(),
            [
                Call::Create(
                    hidden,
                    WebViewContent::Html("<p>Later</p>".into()),
                    Some(Rect::new(0.0, 0.0, 300.0, 50.0))
                ),
                Call::Place(w.web, Some(Rect::new(0.0, 150.0, 300.0, 50.0)), 1.0),
            ]
        );

        w.tree.remove(w.web, &mut w.text);
        w.frame(1.0);
        assert_eq!(w.take_calls(), [Call::Destroy(w.web)]);
    }

    #[test]
    fn requests_reach_the_view_and_reports_come_back_as_messages() {
        let mut w = Window::new(Fake::default());
        w.take_calls();
        // The view reported its first load while it was made.
        assert!(w.views.has_reports());
        w.frame(1.0);
        assert!(!w.views.has_reports());
        let reported: Vec<WebViewEvent> = w
            .tree
            .take_messages()
            .into_iter()
            .filter(|m| m.source == w.web)
            .filter_map(|m| m.downcast_ref::<WebViewEvent>().cloned())
            .collect();
        assert_eq!(
            reported,
            [WebViewEvent::Navigated("https://example.com".into())]
        );
        assert!(w.tree.get::<WebView>(w.web).unwrap().is_loading());

        w.tree.update::<WebView, _>(w.web, |v, cx| {
            v.load_url(cx, "https://example.org");
            v.go_back(cx);
        });
        w.frame(1.0);
        assert_eq!(
            w.take_calls(),
            [
                Call::Request(w.web, WebViewRequest::LoadUrl("https://example.org".into())),
                Call::Request(w.web, WebViewRequest::Back),
            ]
        );
    }

    #[test]
    fn a_view_that_cannot_be_made_is_not_tried_again() {
        let mut w = Window::new(Fake {
            refuse: true,
            ..Default::default()
        });
        assert_eq!(w.take_calls().len(), 1);
        w.scroll_to(10.0);
        assert_eq!(w.take_calls(), []);
    }

    #[test]
    fn views_go_before_the_window_they_are_children_of() {
        let calls = std::rc::Rc::default();
        let w = Window::new(Fake {
            calls: std::rc::Rc::clone(&calls),
            window: Some(ParentWindow(std::rc::Rc::clone(&calls))),
            ..Default::default()
        });
        let web = w.web;
        calls.borrow_mut().clear();
        drop(w);
        assert_eq!(*calls.borrow(), [Call::Destroy(web), Call::CloseWindow]);
    }

    #[test]
    fn the_loop_keeps_running_only_while_views_that_need_it_exist() {
        let mut w = Window::new(Fake {
            polled: true,
            ..Default::default()
        });
        assert!(w.views.poll());
        w.tree.remove(w.web, &mut w.text);
        w.frame(1.0);
        assert!(!w.views.poll(), "no views left");
        assert!(!Window::new(Fake::default()).views.poll());
    }

    #[test]
    fn rects_snap_to_whole_physical_pixels_without_gaps() {
        assert_eq!(
            to_pixels(Rect::new(10.0, 20.0, 300.0, 150.0), 2.0),
            (20, 40, 600, 300)
        );
        // Two views side by side at 150 %: one's right edge is the other's left.
        let left = to_pixels(Rect::new(0.0, 0.0, 100.3, 10.0), 1.5);
        let right = to_pixels(Rect::new(100.3, 0.0, 50.0, 10.0), 1.5);
        assert_eq!(left.0 + left.2 as i32, right.0);
        assert_eq!(left, (0, 0, 150, 15));
        assert_eq!(right, (150, 0, 75, 15));
        assert_eq!(to_pixels(Rect::new(5.0, 5.0, 0.0, 0.0), 1.25).2, 0);
    }
}

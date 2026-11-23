use noa_gpui_core::style::{FlexDirection, Size as LayoutSize, auto, length, percent};
use noa_gpui_core::widgets::{
    Container, ScrollView, WebView, WebViewContent, WebViewEvent, WebViewFrame, WebViewRequest,
    web_views,
};
use noa_gpui_core::{Style, UiTree, WidgetId, access_id};
use noa_gpui_scene::{Rect, Size};
use noa_gpui_text::{FontOptions, TextStore};

struct Harness {
    tree: UiTree,
    text: TextStore,
    root: WidgetId,
    viewport: Size,
}

impl Harness {
    fn new() -> Self {
        let mut tree = UiTree::new();
        let root = tree.insert(None, column(percent(1.0), percent(1.0)));
        Self {
            tree,
            text: TextStore::new(FontOptions {
                locale: "en-US".into(),
                load_system_fonts: false,
            }),
            root,
            viewport: Size::new(640.0, 400.0),
        }
    }

    fn prepare(&mut self) {
        self.tree.prepare(self.viewport, &mut self.text);
    }

    fn block(&mut self, parent: WidgetId, height: f32) -> WidgetId {
        self.tree.insert(
            Some(parent),
            Container::new(Style {
                size: LayoutSize {
                    width: percent(1.0),
                    height: length(height),
                },
                flex_shrink: 0.0,
                ..Default::default()
            }),
        )
    }

    fn frame(&self, id: WidgetId) -> Option<WebViewFrame> {
        web_views(&self.tree).into_iter().find(|f| f.id == id)
    }

    fn view(&self, id: WidgetId) -> &WebView {
        self.tree.get::<WebView>(id).unwrap()
    }
}

fn column(
    width: noa_gpui_core::style::Dimension,
    height: noa_gpui_core::style::Dimension,
) -> Container {
    Container::new(Style {
        flex_direction: FlexDirection::Column,
        size: LayoutSize { width, height },
        ..Default::default()
    })
}

#[test]
fn a_web_view_fills_what_its_parent_leaves_unless_its_height_is_fixed() {
    let mut h = Harness::new();
    h.block(h.root, 40.0);
    let fill = h
        .tree
        .insert(Some(h.root), WebView::new("https://example.com"));
    h.prepare();
    let frame = h.frame(fill).unwrap();
    assert_eq!(frame.rect, Rect::new(0.0, 40.0, 640.0, 360.0));
    assert_eq!(frame.visible, Some(frame.rect));

    let fixed = h
        .tree
        .insert(Some(h.root), WebView::html("<p>Hi</p>").height(100.0));
    h.prepare();
    assert_eq!(
        h.frame(fixed).unwrap().rect,
        Rect::new(0.0, 300.0, 640.0, 100.0)
    );
    assert_eq!(h.frame(fill).unwrap().rect.height, 260.0);
    let found: Vec<WidgetId> = web_views(&h.tree).iter().map(|f| f.id).collect();
    assert_eq!(found, [fill, fixed], "in tree order");
}

#[test]
fn scrolling_clips_the_part_that_shows_and_scrolling_it_away_leaves_none() {
    let mut h = Harness::new();
    let scroll = h.tree.insert(
        Some(h.root),
        ScrollView::new(Style {
            size: LayoutSize {
                width: length(300.0),
                height: length(200.0),
            },
            flex_shrink: 0.0,
            ..Default::default()
        }),
    );
    let content = h.tree.insert(Some(scroll), column(percent(1.0), auto()));
    h.block(content, 100.0);
    let web = h.tree.insert(
        Some(content),
        WebView::new("https://example.com").height(150.0),
    );
    h.block(content, 400.0);
    h.prepare();
    let frame = h.frame(web).unwrap();
    assert_eq!(frame.rect, Rect::new(0.0, 100.0, 300.0, 150.0));
    assert_eq!(frame.visible, Some(Rect::new(0.0, 100.0, 300.0, 100.0)));

    let scroll_to = |h: &mut Harness, offset: f32| {
        h.tree
            .update::<ScrollView, _>(scroll, |s, cx| s.set_offset(cx, offset));
        h.prepare();
        h.frame(web).unwrap()
    };
    let frame = scroll_to(&mut h, 60.0);
    assert_eq!(frame.rect, Rect::new(0.0, 40.0, 300.0, 150.0));
    assert_eq!(frame.visible, Some(frame.rect), "all of it shows");
    let frame = scroll_to(&mut h, 200.0);
    assert_eq!(frame.rect.y, -100.0);
    assert_eq!(frame.visible, Some(Rect::new(0.0, 0.0, 300.0, 50.0)));
    let frame = scroll_to(&mut h, 300.0);
    assert_eq!(frame.visible, None, "scrolled out of the view");
}

#[test]
fn a_hidden_web_view_is_still_found_but_shows_nothing_and_a_removed_one_is_gone() {
    let mut h = Harness::new();
    let parent = h
        .tree
        .insert(Some(h.root), column(percent(1.0), length(200.0)));
    let web = h
        .tree
        .insert(Some(parent), WebView::new("https://example.com"));
    h.prepare();
    assert!(h.frame(web).unwrap().visible.is_some());

    h.tree.set_hidden(parent, true);
    h.prepare();
    assert_eq!(h.frame(web).unwrap().visible, None);
    h.tree.set_hidden(parent, false);
    h.prepare();
    assert_eq!(
        h.frame(web).unwrap().visible,
        Some(Rect::new(0.0, 0.0, 640.0, 200.0))
    );

    h.tree.remove(web, &mut h.text);
    h.prepare();
    assert!(web_views(&h.tree).is_empty());
}

#[test]
fn navigation_requests_wait_in_order_until_the_platform_takes_them() {
    let mut h = Harness::new();
    let web = h
        .tree
        .insert(Some(h.root), WebView::new("https://example.com"));
    h.prepare();
    assert_eq!(
        h.view(web).content(),
        &WebViewContent::Url("https://example.com".into())
    );
    assert!(h.view(web).requests().is_empty());

    h.tree.update::<WebView, _>(web, |w, cx| {
        w.load_url(cx, "https://example.org");
        w.reload(cx);
        w.go_back(cx);
        w.go_forward(cx);
    });
    // A request changes the frame: the platform layer looks at the requests then.
    assert!(h.tree.needs_update(h.viewport));
    h.prepare();
    assert_eq!(h.view(web).url(), "https://example.org");
    assert_eq!(
        h.view(web).content(),
        &WebViewContent::Url("https://example.org".into()),
        "a view created now loads the last URL asked for"
    );
    let taken = h
        .tree
        .update::<WebView, _>(web, |w, _| w.take_requests())
        .unwrap();
    assert_eq!(
        taken,
        [
            WebViewRequest::LoadUrl("https://example.org".into()),
            WebViewRequest::Reload,
            WebViewRequest::Back,
            WebViewRequest::Forward,
        ]
    );
    assert!(h.view(web).requests().is_empty());

    h.tree
        .update::<WebView, _>(web, |w, cx| w.load_html(cx, "<h1>Hi</h1>"));
    assert_eq!(
        h.view(web).content(),
        &WebViewContent::Html("<h1>Hi</h1>".into())
    );
    assert_eq!(
        h.view(web).url(),
        "",
        "HTML has no URL until the view reports one"
    );
    assert_eq!(
        h.view(web).requests(),
        [WebViewRequest::LoadHtml("<h1>Hi</h1>".into())]
    );
}

#[test]
fn reports_change_the_state_at_once_and_arrive_as_messages_on_the_next_frame() {
    let mut h = Harness::new();
    let web = h
        .tree
        .insert(Some(h.root), WebView::new("https://example.com"));
    h.prepare();
    h.tree.set_now(1.0);
    let report = |h: &mut Harness, event: WebViewEvent| {
        h.tree
            .update::<WebView, _>(web, |w, cx| w.report(cx, event));
    };
    let messages = |h: &mut Harness| -> Vec<WebViewEvent> {
        h.tree.animate(1.0, &mut h.text);
        h.tree
            .take_messages()
            .into_iter()
            .inspect(|m| assert_eq!(m.source, web))
            .filter_map(|m| m.downcast_ref::<WebViewEvent>().cloned())
            .collect()
    };

    report(
        &mut h,
        WebViewEvent::Navigated("https://example.com/".into()),
    );
    assert!(h.view(web).is_loading());
    assert_eq!(h.view(web).url(), "https://example.com/");
    assert!(h.tree.take_messages().is_empty(), "not during the update");
    assert_eq!(h.tree.next_timer(), Some(1.0));
    assert_eq!(
        messages(&mut h),
        [WebViewEvent::Navigated("https://example.com/".into())]
    );

    report(&mut h, WebViewEvent::TitleChanged("Example Domain".into()));
    report(&mut h, WebViewEvent::Loaded("https://example.com/".into()));
    assert!(!h.view(web).is_loading());
    assert_eq!(h.view(web).title(), "Example Domain");
    assert_eq!(
        messages(&mut h),
        [
            WebViewEvent::TitleChanged("Example Domain".into()),
            WebViewEvent::Loaded("https://example.com/".into()),
        ]
    );
    assert!(messages(&mut h).is_empty(), "each is emitted once");
}

#[test]
fn assistive_technology_sees_a_web_view_named_by_its_title() {
    let mut h = Harness::new();
    let web = h
        .tree
        .insert(Some(h.root), WebView::new("https://example.com"));
    h.prepare();
    let node = |h: &Harness| {
        let update = h.tree.accessibility_update(1.0);
        let (_, node) = update
            .nodes
            .into_iter()
            .find(|(id, _)| *id == access_id(web))
            .unwrap();
        node
    };
    let first = node(&h);
    assert_eq!(first.role(), noa_gpui_core::accesskit::Role::WebView);
    assert_eq!(first.label(), Some("https://example.com"));
    h.tree.update::<WebView, _>(web, |w, cx| {
        w.report(cx, WebViewEvent::TitleChanged("Example Domain".into()));
    });
    h.prepare();
    assert_eq!(node(&h).label(), Some("Example Domain"));
}

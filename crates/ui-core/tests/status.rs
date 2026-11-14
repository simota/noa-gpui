use ui_core::style::{Display, FlexDirection, Size as LayoutSize, length, percent};
use ui_core::widgets::{
    BAR_PERIOD, Container, Image, ImageFrame, ImageScaling, LEVEL_CRITICAL, LEVEL_NORMAL,
    LEVEL_WARNING, LevelIndicator, LevelStyle, ProgressBar, SPINNER_PERIOD, ScrollView, Spinner,
    ValueChanged, image_rect,
};
use ui_core::{
    Event, Key, Message, Modifiers, PointerButton, Style, UiTree, WidgetId, access_id, accesskit,
};
use ui_scene::{ImageId, Point, Rect, Size};
use ui_text::{FontOptions, TextStore};

const VIEWPORT: Size = Size::new(400.0, 300.0);

struct Harness {
    tree: UiTree,
    text: TextStore,
    root: WidgetId,
}

impl Harness {
    fn new() -> Self {
        let mut tree = UiTree::new();
        let root = tree.insert(
            None,
            Container::new(Style {
                flex_direction: FlexDirection::Column,
                align_items: ui_core::style::AlignItems::START,
                size: LayoutSize {
                    width: percent(1.0),
                    height: percent(1.0),
                },
                ..Default::default()
            }),
        );
        Self {
            tree,
            text: TextStore::new(FontOptions {
                locale: "en-US".into(),
                load_system_fonts: false,
            }),
            root,
        }
    }

    fn add_to(&mut self, parent: WidgetId, widget: impl ui_core::Widget) -> WidgetId {
        let id = self.tree.insert(Some(parent), widget);
        self.prepare();
        id
    }

    fn add(&mut self, widget: impl ui_core::Widget) -> WidgetId {
        self.add_to(self.root, widget)
    }

    fn spacer(&mut self, parent: WidgetId, height: f32) {
        self.add_to(
            parent,
            Container::new(Style {
                size: LayoutSize {
                    width: length(10.0),
                    height: length(height),
                },
                flex_shrink: 0.0,
                ..Default::default()
            }),
        );
    }

    fn prepare(&mut self) {
        self.tree.prepare(VIEWPORT, &mut self.text);
    }

    fn frame(&mut self, now: f64) {
        self.tree.animate(now, &mut self.text);
        self.prepare();
    }

    fn at(&self, id: WidgetId, x: f32, y: f32) -> Point {
        let r = self.tree.window_rect(id).unwrap();
        Point::new(r.x + x, r.y + y)
    }

    fn click(&mut self, pos: Point) {
        let button = PointerButton::Primary;
        self.tree
            .dispatch(Event::PointerDown { pos, button }, &mut self.text);
        self.tree
            .dispatch(Event::PointerUp { pos, button }, &mut self.text);
        self.prepare();
    }

    fn key(&mut self, key: Key) {
        self.tree.dispatch(
            Event::KeyDown {
                key,
                modifiers: Modifiers::default(),
            },
            &mut self.text,
        );
        self.prepare();
    }

    fn messages(&mut self) -> Vec<Message> {
        self.tree.take_messages()
    }
}

fn values(messages: &[Message]) -> Vec<f64> {
    messages
        .iter()
        .filter_map(|m| m.downcast_ref::<ValueChanged>().map(|v| v.0))
        .collect()
}

#[test]
fn determinate_bars_show_the_fraction_and_never_animate() {
    let mut h = Harness::new();
    let bar = h.add(ProgressBar::new(200.0).value(30.0, 120.0));
    assert_eq!(
        h.tree.get::<ProgressBar>(bar).unwrap().fraction(),
        Some(0.25)
    );
    h.tree
        .update::<ProgressBar, _>(bar, |b, cx| b.set_value(cx, 500.0, 120.0));
    assert_eq!(
        h.tree.get::<ProgressBar>(bar).unwrap().fraction(),
        Some(1.0)
    );
    assert!(!h.tree.wants_animation_frame());
    let update = h.tree.accessibility_update(1.0);
    let node = &update
        .nodes
        .iter()
        .find(|(n, _)| *n == access_id(bar))
        .unwrap()
        .1;
    assert_eq!(node.role(), accesskit::Role::ProgressIndicator);
    assert_eq!(node.numeric_value(), Some(500.0));
}

#[test]
fn indeterminate_bars_animate_only_between_start_and_stop() {
    let mut h = Harness::new();
    let bar = h.add(ProgressBar::new(200.0).indeterminate());
    assert!(!h.tree.wants_animation_frame());
    assert_eq!(h.tree.get::<ProgressBar>(bar).unwrap().highlight(), None);

    h.tree.set_now(10.0);
    h.tree
        .update::<ProgressBar, _>(bar, |b, cx| b.start_animation(cx));
    assert!(h.tree.wants_animation_frame());
    let highlight = |h: &Harness| {
        h.tree
            .get::<ProgressBar>(bar)
            .unwrap()
            .highlight()
            .unwrap()
            .0
    };
    h.frame(10.0);
    let start = highlight(&h);
    h.frame(10.0 + BAR_PERIOD / 2.0);
    let middle = highlight(&h);
    assert!(middle > start, "{start} -> {middle}");
    // One period later it is back where it started.
    h.frame(10.0 + BAR_PERIOD);
    assert!((highlight(&h) - start).abs() < 1e-4);
    assert!(h.tree.wants_animation_frame());

    h.tree
        .update::<ProgressBar, _>(bar, |b, cx| b.stop_animation(cx));
    h.frame(12.0);
    assert!(!h.tree.wants_animation_frame(), "stopped: back to 0 FPS");
    assert_eq!(h.tree.get::<ProgressBar>(bar).unwrap().highlight(), None);
}

#[test]
fn spinners_turn_with_time_and_show_a_determinate_ring() {
    let mut h = Harness::new();
    let spinner = h.add(Spinner::new());
    assert_eq!(h.tree.get::<Spinner>(spinner).unwrap().lead_spoke(), None);
    h.tree.set_now(3.0);
    h.tree
        .update::<Spinner, _>(spinner, |s, cx| s.start_animation(cx));
    h.frame(3.0);
    assert_eq!(
        h.tree.get::<Spinner>(spinner).unwrap().lead_spoke(),
        Some(0)
    );
    h.frame(3.0 + SPINNER_PERIOD / 4.0);
    assert_eq!(
        h.tree.get::<Spinner>(spinner).unwrap().lead_spoke(),
        Some(3)
    );
    h.frame(3.0 + SPINNER_PERIOD * 0.75);
    assert_eq!(
        h.tree.get::<Spinner>(spinner).unwrap().lead_spoke(),
        Some(9)
    );

    let ring = h.add(Spinner::new().determinate(1.0, 4.0));
    assert_eq!(h.tree.get::<Spinner>(ring).unwrap().fraction(), Some(0.25));
    assert_eq!(h.tree.get::<Spinner>(ring).unwrap().lead_spoke(), None);
}

#[test]
fn busy_indicators_out_of_view_draw_no_frames_until_scrolled_back() {
    let mut h = Harness::new();
    let scroll = h.add(ScrollView::new(Style {
        flex_direction: FlexDirection::Column,
        size: LayoutSize {
            width: length(200.0),
            height: length(100.0),
        },
        flex_shrink: 0.0,
        ..Default::default()
    }));
    // Inside the window (300 px tall) but below the scroll view's 100 px: hidden only
    // by the scroll view's clipping.
    h.spacer(scroll, 150.0);
    let spinner = h.add_to(scroll, Spinner::new());
    h.tree.set_now(1.0);
    h.tree
        .update::<Spinner, _>(spinner, |s, cx| s.start_animation(cx));
    h.prepare();
    let r = h.tree.window_rect(spinner).unwrap();
    assert!(r.y > 100.0 && r.bottom() < 300.0, "{r:?}");
    assert!(!h.tree.is_visible(spinner));
    assert!(!h.tree.wants_animation_frame());
    h.frame(1.25);
    assert_eq!(
        h.tree.get::<Spinner>(spinner).unwrap().lead_spoke(),
        Some(0)
    );
    assert!(!h.tree.wants_animation_frame());

    h.tree
        .update::<ScrollView, _>(scroll, |s, cx| s.set_offset(cx, 130.0));
    h.prepare();
    assert!(h.tree.is_visible(spinner));
    assert!(h.tree.wants_animation_frame());
    h.frame(1.5);
    assert_eq!(
        h.tree.get::<Spinner>(spinner).unwrap().lead_spoke(),
        Some(6)
    );
    assert!(h.tree.wants_animation_frame());
}

#[test]
fn hidden_or_off_window_indicators_draw_no_frames() {
    let mut h = Harness::new();
    let hidden = h.add(Container::new(Style {
        display: Display::None,
        ..Default::default()
    }));
    let bar = h.add_to(hidden, ProgressBar::new(100.0).indeterminate());
    h.spacer(h.root, 400.0);
    let spinner = h.add(Spinner::new());
    h.tree
        .update::<ProgressBar, _>(bar, |b, cx| b.start_animation(cx));
    h.tree
        .update::<Spinner, _>(spinner, |s, cx| s.start_animation(cx));
    h.prepare();
    assert!(!h.tree.is_visible(bar));
    assert!(!h.tree.is_visible(spinner));
    assert!(!h.tree.wants_animation_frame());

    h.tree
        .update::<Container, _>(hidden, |c, cx| c.set_style(cx, Style::default()));
    h.prepare();
    assert!(h.tree.wants_animation_frame());
}

#[test]
fn capacity_colors_follow_the_warning_and_critical_levels() {
    let level = |value: f64| {
        LevelIndicator::new(LevelStyle::ContinuousCapacity, 0.0, 100.0, value)
            .warning(70.0)
            .critical(90.0)
            .color()
    };
    assert_eq!(level(50.0), LEVEL_NORMAL);
    assert_eq!(level(70.0), LEVEL_WARNING);
    assert_eq!(level(95.0), LEVEL_CRITICAL);
    // Critical below warning: lower is worse (a battery).
    let battery = |value: f64| {
        LevelIndicator::new(LevelStyle::DiscreteCapacity, 0.0, 10.0, value)
            .warning(3.0)
            .critical(1.0)
            .color()
    };
    assert_eq!(battery(8.0), LEVEL_NORMAL);
    assert_eq!(battery(3.0), LEVEL_WARNING);
    assert_eq!(battery(1.0), LEVEL_CRITICAL);
}

#[test]
fn an_editable_rating_takes_whole_stars_by_click_and_keys() {
    let mut h = Harness::new();
    let rating = h.add(LevelIndicator::new(LevelStyle::Rating, 0.0, 5.0, 2.0).editable(true));
    let r = h.tree.window_rect(rating).unwrap();
    assert_eq!(r.width, 5.0 * 16.0 + 4.0 * 3.0);
    // The fourth star (each 16 px plus a 3 px gap).
    h.click(h.at(rating, 3.0 * 19.0 + 8.0, 9.0));
    h.click(h.at(rating, 1.0, 9.0));
    h.tree.set_focus(Some(rating));
    h.key(Key::ArrowRight);
    h.key(Key::End);
    h.key(Key::ArrowRight);
    assert_eq!(values(&h.messages()), [4.0, 0.0, 1.0, 5.0]);
}

#[test]
fn discrete_levels_take_whole_units_and_read_only_ones_ignore_input() {
    let mut h = Harness::new();
    let discrete = h.add(
        LevelIndicator::new(LevelStyle::DiscreteCapacity, 0.0, 10.0, 0.0)
            .width(200.0)
            .editable(true),
    );
    // 41% of the width is inside the fifth segment.
    h.click(h.at(discrete, 82.0, 9.0));
    assert_eq!(values(&h.messages()), [5.0]);

    let meter = h.add(LevelIndicator::new(LevelStyle::Relevancy, 0.0, 1.0, 0.5).width(200.0));
    h.click(h.at(meter, 190.0, 9.0));
    h.tree.set_focus(Some(meter));
    h.key(Key::ArrowRight);
    assert!(values(&h.messages()).is_empty());
    assert_eq!(h.tree.get::<LevelIndicator>(meter).unwrap().value(), 0.5);
    let update = h.tree.accessibility_update(1.0);
    let role = |id| {
        update
            .nodes
            .iter()
            .find(|(n, _)| *n == access_id(id))
            .unwrap()
            .1
            .role()
    };
    assert_eq!(role(meter), accesskit::Role::Meter);
    assert_eq!(role(discrete), accesskit::Role::Slider);
}

#[test]
fn images_fit_fill_or_keep_their_size() {
    // A 200 x 100 picture in a 100 x 100 box.
    let natural = Size::new(200.0, 100.0);
    let area = Rect::new(0.0, 0.0, 100.0, 100.0);
    assert_eq!(image_rect(ImageScaling::Stretch, natural, area), area);
    assert_eq!(
        image_rect(ImageScaling::Fit, natural, area),
        Rect::new(0.0, 25.0, 100.0, 50.0)
    );
    assert_eq!(
        image_rect(ImageScaling::Fill, natural, area),
        Rect::new(-50.0, 0.0, 200.0, 100.0)
    );
    assert_eq!(
        image_rect(ImageScaling::None, natural, area),
        Rect::new(-50.0, 0.0, 200.0, 100.0)
    );
    let small = Size::new(20.0, 10.0);
    assert_eq!(
        image_rect(ImageScaling::Fit, small, area),
        Rect::new(0.0, 25.0, 100.0, 50.0)
    );
    assert_eq!(
        image_rect(ImageScaling::FitDown, small, area),
        Rect::new(40.0, 45.0, 20.0, 10.0)
    );
    let framed = Image::new(ImageId(0), Size::new(100.0, 100.0))
        .natural_size(Size::new(10.0, 10.0))
        .scaling(ImageScaling::Fit)
        .frame(ImageFrame::Bezel);
    assert_eq!(framed.picture_rect(), Rect::new(4.0, 4.0, 92.0, 92.0));
}

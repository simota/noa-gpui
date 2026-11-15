use noa_gpui_core::style::{FlexDirection, Size as LayoutSize, percent};
use noa_gpui_core::widgets::{
    ColorChanged, ColorPicker, ColorWell, Container, DateChanged, DateElements, DateFormat,
    DateOrder, DatePicker, DatePickerStyle, Menu, PathControl, PathItemClicked, PathStyle,
    REPEAT_DELAY, REPEAT_INTERVAL, Slider, Stepper, ValueChanged, from_hsv, hex_string, to_hsv,
};
use noa_gpui_core::{
    DateTime, Event, Key, Message, Modifiers, PointerButton, Style, UiTree, WidgetId, access_id,
    accesskit,
};
use noa_gpui_scene::{Color, Point, Size};
use noa_gpui_text::{FontOptions, TextStore};

const VIEWPORT: Size = Size::new(800.0, 600.0);

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
                align_items: noa_gpui_core::style::AlignItems::START,
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

    fn add(&mut self, widget: impl noa_gpui_core::Widget) -> WidgetId {
        let id = self.tree.insert(Some(self.root), widget);
        self.prepare();
        id
    }

    fn prepare(&mut self) {
        self.tree.prepare(VIEWPORT, &mut self.text);
    }

    fn at(&self, id: WidgetId, x: f32, y: f32) -> Point {
        let r = self.tree.window_rect(id).unwrap();
        Point::new(r.x + x, r.y + y)
    }

    fn down(&mut self, pos: Point) {
        let button = PointerButton::Primary;
        self.tree
            .dispatch(Event::PointerDown { pos, button }, &mut self.text);
        self.prepare();
    }

    fn up(&mut self, pos: Point) {
        let button = PointerButton::Primary;
        self.tree
            .dispatch(Event::PointerUp { pos, button }, &mut self.text);
        self.prepare();
    }

    fn drag_to(&mut self, pos: Point) {
        self.tree
            .dispatch(Event::PointerMove { pos }, &mut self.text);
        self.prepare();
    }

    fn click(&mut self, pos: Point) {
        self.down(pos);
        self.up(pos);
    }

    fn key(&mut self, key: Key) {
        self.key_with(key, Modifiers::default());
    }

    fn key_with(&mut self, key: Key, modifiers: Modifiers) {
        self.tree
            .dispatch(Event::KeyDown { key, modifiers }, &mut self.text);
        self.prepare();
    }

    fn type_text(&mut self, text: &str) {
        self.tree.dispatch(Event::Text(text.into()), &mut self.text);
        self.prepare();
    }

    fn access(&mut self, id: WidgetId, action: accesskit::Action) {
        let request = accesskit::ActionRequest {
            action,
            target_tree: accesskit::TreeId::ROOT,
            target_node: access_id(id),
            data: None,
        };
        self.tree.accessibility_action(&request, &mut self.text);
        self.prepare();
    }

    fn messages(&mut self) -> Vec<Message> {
        self.tree.take_messages()
    }

    fn overlay(&self) -> Option<WidgetId> {
        self.tree.overlays().last()
    }
}

fn values<T: 'static + Clone>(messages: &[Message]) -> Vec<T> {
    messages
        .iter()
        .filter_map(|m| m.downcast_ref::<T>().cloned())
        .collect()
}

fn date(year: i32, month: u32, day: u32) -> DateTime {
    DateTime::date(year, month, day).unwrap()
}

/// Knob diameter and track length of a regular slider with default length.
const KNOB: f32 = 18.0;
const LENGTH: f32 = 160.0;

fn slider_x(t: f32) -> f32 {
    KNOB / 2.0 + t * (LENGTH - KNOB)
}

#[test]
fn slider_jumps_to_a_press_and_follows_the_drag_within_its_range() {
    let mut h = Harness::new();
    let slider = h.add(Slider::new(0.0, 100.0, 50.0));
    h.down(h.at(slider, slider_x(0.25), 9.0));
    h.drag_to(h.at(slider, slider_x(0.75), 30.0));
    h.drag_to(h.at(slider, LENGTH + 50.0, 9.0));
    h.up(h.at(slider, LENGTH + 50.0, 9.0));
    let changes = values::<ValueChanged>(&h.messages());
    let got: Vec<f64> = changes.iter().map(|v| v.0.round()).collect();
    assert_eq!(got, [25.0, 75.0, 100.0]);
    assert_eq!(h.tree.get::<Slider>(slider).unwrap().value(), 100.0);
}

#[test]
fn slider_keys_step_page_and_jump_to_the_ends() {
    let mut h = Harness::new();
    let slider = h.add(Slider::new(0.0, 10.0, 5.0).step(1.0));
    h.tree.set_focus(Some(slider));
    let value = |h: &Harness| h.tree.get::<Slider>(slider).unwrap().value();
    h.key(Key::ArrowRight);
    assert_eq!(value(&h), 6.0);
    h.key(Key::ArrowDown);
    h.key(Key::ArrowDown);
    assert_eq!(value(&h), 4.0);
    h.key(Key::PageUp);
    assert_eq!(value(&h), 5.0);
    h.key(Key::End);
    assert_eq!(value(&h), 10.0);
    h.messages();
    h.key(Key::ArrowUp);
    assert_eq!(value(&h), 10.0);
    assert!(values::<ValueChanged>(&h.messages()).is_empty());
    h.key(Key::Home);
    assert_eq!(values::<ValueChanged>(&h.messages()), [ValueChanged(0.0)]);
}

#[test]
fn setting_a_range_sliders_lower_value_cannot_cross_the_upper_knob() {
    let mut h = Harness::new();
    let slider = h.add(Slider::range(0.0, 100.0, 20.0, 40.0));
    h.tree
        .update::<Slider, _>(slider, |s, cx| s.set_value(cx, 80.0));
    h.prepare();
    assert_eq!(
        h.tree.get::<Slider>(slider).unwrap().range_values(),
        Some((40.0, 40.0))
    );
    h.tree.set_focus(Some(slider));
    h.key(Key::ArrowLeft);
    assert_eq!(
        h.tree.get::<Slider>(slider).unwrap().range_values(),
        Some((35.0, 40.0))
    );
}

#[test]
fn tick_configuration_snaps_both_range_knobs_in_either_builder_order() {
    let mut h = Harness::new();
    let sliders = [
        Slider::range(0.0, 100.0, 14.0, 86.0)
            .tick_marks(5)
            .snap_to_ticks(true),
        Slider::range(0.0, 100.0, 14.0, 86.0)
            .snap_to_ticks(true)
            .tick_marks(5),
    ];
    for slider in sliders {
        let id = h.add(slider);
        assert_eq!(
            h.tree.get::<Slider>(id).unwrap().range_values(),
            Some((25.0, 75.0))
        );
        h.tree.set_focus(Some(id));
        h.key(Key::ArrowRight);
        assert_eq!(
            h.tree.get::<Slider>(id).unwrap().range_values(),
            Some((50.0, 75.0))
        );
    }
}

#[test]
fn a_slider_restricted_to_ticks_only_takes_tick_values() {
    let mut h = Harness::new();
    // Ticks at 0, 25, 50, 75, 100.
    let slider = h.add(
        Slider::new(0.0, 100.0, 40.0)
            .tick_marks(5)
            .snap_to_ticks(true),
    );
    assert_eq!(h.tree.get::<Slider>(slider).unwrap().value(), 50.0);
    h.click(h.at(slider, slider_x(0.3), 9.0));
    assert_eq!(values::<ValueChanged>(&h.messages()), [ValueChanged(25.0)]);
    h.tree.set_focus(Some(slider));
    h.key(Key::ArrowRight);
    assert_eq!(values::<ValueChanged>(&h.messages()), [ValueChanged(50.0)]);
}

#[test]
fn vertical_sliders_grow_upward_and_circular_ones_clockwise_from_the_top() {
    let mut h = Harness::new();
    let vertical = h.add(Slider::new(0.0, 1.0, 0.0).vertical(true));
    h.click(h.at(vertical, 9.0, KNOB / 2.0));
    assert_eq!(values::<ValueChanged>(&h.messages()), [ValueChanged(1.0)]);

    let dial = h.add(Slider::new(0.0, 360.0, 0.0).circular());
    let r = h.tree.window_rect(dial).unwrap();
    assert_eq!((r.width, r.height), (32.0, 32.0));
    // Right of the center: a quarter turn.
    h.click(h.at(dial, 30.0, 16.0));
    let got = values::<ValueChanged>(&h.messages());
    assert_eq!(got.len(), 1);
    assert!((got[0].0 - 90.0).abs() < 0.5, "{got:?}");
}

#[test]
fn assistive_technology_increments_and_decrements_sliders_and_steppers() {
    let mut h = Harness::new();
    let slider = h.add(Slider::new(0.0, 10.0, 5.0).step(2.0));
    let stepper = h.add(Stepper::new(0.0, 10.0, 5.0));
    h.access(slider, accesskit::Action::Increment);
    h.access(stepper, accesskit::Action::Decrement);
    assert_eq!(h.tree.get::<Slider>(slider).unwrap().value(), 7.0);
    assert_eq!(h.tree.get::<Stepper>(stepper).unwrap().value(), 4.0);

    let update = h.tree.accessibility_update(1.0);
    let node = |id| {
        update
            .nodes
            .iter()
            .find(|(n, _)| *n == access_id(id))
            .map(|(_, node)| node.clone())
            .unwrap()
    };
    let s = node(slider);
    assert_eq!(s.role(), accesskit::Role::Slider);
    assert_eq!(
        (
            s.numeric_value(),
            s.min_numeric_value(),
            s.max_numeric_value()
        ),
        (Some(7.0), Some(0.0), Some(10.0))
    );
    assert!(s.supports_action(accesskit::Action::Increment));
    assert_eq!(node(stepper).role(), accesskit::Role::SpinButton);
}

#[test]
fn stepper_arrows_step_by_the_increment_and_stop_at_the_ends() {
    let mut h = Harness::new();
    let stepper = h.add(Stepper::new(0.0, 1.0, 0.8).increment(0.1));
    let (top, bottom) = (h.at(stepper, 8.0, 4.0), h.at(stepper, 8.0, 20.0));
    h.click(top);
    h.click(top);
    h.click(top);
    h.click(bottom);
    let got: Vec<f64> = values::<ValueChanged>(&h.messages())
        .iter()
        .map(|v| v.0)
        .collect();
    assert_eq!(got, [0.9, 1.0, 0.9]);
    h.tree.set_focus(Some(stepper));
    h.key(Key::ArrowDown);
    assert_eq!(h.tree.get::<Stepper>(stepper).unwrap().value(), 0.8);
}

#[test]
fn a_wrapping_stepper_continues_from_the_other_end() {
    let mut h = Harness::new();
    let stepper = h.add(Stepper::new(0.0, 59.0, 59.0).wraps(true));
    h.tree.set_focus(Some(stepper));
    h.key(Key::ArrowUp);
    assert_eq!(values::<ValueChanged>(&h.messages()), [ValueChanged(0.0)]);
    h.key(Key::ArrowDown);
    assert_eq!(values::<ValueChanged>(&h.messages()), [ValueChanged(59.0)]);
}

#[test]
fn holding_a_stepper_arrow_repeats_after_a_delay_while_the_pointer_is_over_it() {
    let mut h = Harness::new();
    let stepper = h.add(Stepper::new(0.0, 100.0, 0.0));
    let top = h.at(stepper, 8.0, 4.0);
    h.tree.set_now(10.0);
    h.down(top);
    assert_eq!(h.tree.get::<Stepper>(stepper).unwrap().value(), 1.0);
    let frame = |h: &mut Harness, t: f64| h.tree.animate(10.0 + t, &mut h.text);
    frame(&mut h, REPEAT_DELAY - 0.01);
    assert_eq!(h.tree.get::<Stepper>(stepper).unwrap().value(), 1.0);
    frame(&mut h, REPEAT_DELAY);
    frame(&mut h, REPEAT_DELAY + REPEAT_INTERVAL);
    assert_eq!(h.tree.get::<Stepper>(stepper).unwrap().value(), 3.0);
    h.drag_to(h.at(stepper, 8.0, 20.0));
    frame(&mut h, REPEAT_DELAY + 2.0 * REPEAT_INTERVAL);
    h.up(h.at(stepper, 8.0, 20.0));
    frame(&mut h, 5.0);
    assert_eq!(h.tree.get::<Stepper>(stepper).unwrap().value(), 3.0);
    assert!(!h.tree.wants_animation_frame());
}

#[test]
fn hsv_conversion_round_trips() {
    for rgb in [0xff3b30, 0x34c759, 0x007aff, 0x808080, 0x000000, 0xffffff] {
        let color = Color::hex(rgb);
        let (h, s, v) = to_hsv(color);
        assert_eq!(from_hsv(h, s, v).to_rgba8(), color.to_rgba8(), "{rgb:06x}");
    }
    assert_eq!(hex_string(Color::hex(0x007aff)), "#007AFF");
}

#[test]
fn a_color_well_opens_a_picker_whose_changes_apply_to_the_well() {
    let mut h = Harness::new();
    let well = h.add(ColorWell::new(Color::hex(0x007aff)));
    h.click(h.at(well, 20.0, 12.0));
    let picker = h.overlay().expect("the picker opened");
    assert!(h.tree.get::<ColorPicker>(picker).is_some());
    assert!(h.tree.get::<ColorWell>(well).unwrap().is_open());
    assert_eq!(h.tree.focused(), Some(picker));

    // The first swatch (red) in the picker.
    let swatch = h.at(picker, 12.0 + 5.0, 12.0 + 140.0 + 10.0 + 14.0 + 10.0 + 5.0);
    h.click(swatch);
    let changes = h.messages();
    assert_eq!(
        values::<ColorChanged>(&changes)
            .iter()
            .map(|c| c.0.to_rgba8())
            .collect::<Vec<_>>(),
        [Color::hex(0xff3b30).to_rgba8()]
    );
    assert!(changes.iter().all(|m| m.source == well));
    assert_eq!(
        h.tree.get::<ColorWell>(well).unwrap().color().to_rgba8(),
        Color::hex(0xff3b30).to_rgba8()
    );

    h.key(Key::ArrowDown);
    let darker = values::<ColorChanged>(&h.messages());
    assert_eq!(darker.len(), 1);
    assert!((to_hsv(darker[0].0).2 - 0.95).abs() < 1e-4);

    h.key(Key::Escape);
    assert!(h.overlay().is_none());
    assert_eq!(h.tree.focused(), Some(well));
    assert!(!h.tree.get::<ColorWell>(well).unwrap().is_open());
}

#[test]
fn pressing_the_well_again_only_closes_the_picker() {
    let mut h = Harness::new();
    let well = h.add(ColorWell::new(Color::WHITE));
    let pos = h.at(well, 20.0, 12.0);
    h.click(pos);
    assert!(h.overlay().is_some());
    h.click(pos);
    assert!(h.overlay().is_none());
    assert!(!h.tree.get::<ColorWell>(well).unwrap().is_open());
    h.key(Key::Space);
    assert!(h.overlay().is_some());
    h.key(Key::Enter);
    assert!(h.overlay().is_none());
}

#[test]
fn dragging_in_the_picker_field_sets_saturation_and_brightness() {
    let mut h = Harness::new();
    let picker = h.add(ColorPicker::new(Color::hex(0xff0000)));
    // Top-left of the field: white; bottom: black. Standalone pickers emit themselves.
    h.down(h.at(picker, 12.0, 12.0));
    h.drag_to(h.at(picker, 100.0, 400.0));
    h.up(h.at(picker, 100.0, 400.0));
    let got: Vec<[u8; 4]> = values::<ColorChanged>(&h.messages())
        .iter()
        .map(|c| c.0.to_rgba8())
        .collect();
    assert_eq!(got, [[255, 255, 255, 255], [0, 0, 0, 255]]);
}

#[test]
fn date_fields_wrap_within_their_range_and_clamp_the_day() {
    let mut h = Harness::new();
    let picker = h.add(DatePicker::new(date(2024, 12, 31)));
    h.tree.set_focus(Some(picker));
    let value = |h: &Harness| h.tree.get::<DatePicker>(picker).unwrap().value();
    // Month: December wraps to January without changing the year.
    h.key(Key::ArrowUp);
    assert_eq!(value(&h), date(2024, 1, 31));
    // February clamps the day to 29 (2024 is a leap year).
    h.key(Key::ArrowUp);
    assert_eq!(value(&h), date(2024, 2, 29));
    h.key(Key::ArrowRight);
    h.key(Key::ArrowUp);
    assert_eq!(value(&h), date(2024, 2, 1));
    h.key(Key::ArrowRight);
    h.key(Key::ArrowDown);
    assert_eq!(value(&h), date(2023, 2, 1));
    let changes = values::<DateChanged>(&h.messages());
    assert_eq!(changes.len(), 4);
    assert_eq!(changes[3], DateChanged(date(2023, 2, 1)));
}

#[test]
fn typing_digits_fills_each_field_and_moves_on() {
    let mut h = Harness::new();
    let picker = h.add(
        DatePicker::new(DateTime::new(2026, 11, 12, 9, 30).unwrap())
            .elements(DateElements::DateAndTime),
    );
    h.tree.set_focus(Some(picker));
    // Month "0" waits for a second digit; "3" completes it. Day "4" cannot take a
    // second digit (40 > 30) so it completes at once.
    h.type_text("03");
    h.type_text("4");
    h.type_text("2025");
    h.type_text("1");
    h.type_text("7");
    h.type_text("05");
    assert_eq!(
        h.tree.get::<DatePicker>(picker).unwrap().value(),
        DateTime::new(2025, 3, 4, 17, 5).unwrap()
    );
}

#[test]
fn date_formats_order_the_fields_and_typing_follows_that_order() {
    let mut h = Harness::new();
    let value = DateTime::new(2026, 11, 13, 21, 5).unwrap();
    let shown = |format: DateFormat| {
        DatePicker::new(value)
            .elements(DateElements::DateAndTime)
            .format(format)
            .formatted()
    };
    assert_eq!(shown(DateFormat::default()), "11/13/2026, 21:05");
    assert_eq!(shown(DateFormat::ISO), "2026-11-13, 21:05");
    assert_eq!(shown(DateFormat::for_locale("ja-JP")), "2026/11/13, 21:05");
    assert_eq!(
        shown(DateFormat::for_locale("de_DE.UTF-8")),
        "13.11.2026, 21:05"
    );
    assert_eq!(shown(DateFormat::for_locale("en-GB")), "13/11/2026, 21:05");
    assert_eq!(
        shown(DateFormat::for_locale("en-US")),
        "11/13/2026, 9:05 PM"
    );
    assert_eq!(
        shown(DateFormat::for_locale("zh-Hant-TW")),
        "2026/11/13, 21:05"
    );

    // Typing fills the fields in the order they are shown: year, month, day.
    let picker =
        h.add(DatePicker::new(value).format(DateFormat::default().order(DateOrder::YearMonthDay)));
    h.tree.set_focus(Some(picker));
    h.type_text("2025");
    h.type_text("03");
    h.type_text("4");
    assert_eq!(
        h.tree.get::<DatePicker>(picker).unwrap().value(),
        DateTime::new(2025, 3, 4, 21, 5).unwrap()
    );
}

#[test]
fn twelve_hour_clocks_keep_am_or_pm_and_toggle_it_with_a_p_and_the_arrows() {
    let mut h = Harness::new();
    let picker = h.add(
        DatePicker::new(DateTime::new(2026, 11, 13, 9, 30).unwrap())
            .elements(DateElements::Time)
            .format(DateFormat::default().twelve_hour(true)),
    );
    let get = |h: &Harness| {
        let p = h.tree.get::<DatePicker>(picker).unwrap();
        (p.value().hour, p.formatted())
    };
    assert_eq!(get(&h), (9, "9:30 AM".to_owned()));
    h.tree.set_focus(Some(picker));
    // Hours are typed 1–12; "1" waits for a possible "12", "2" completes it.
    h.type_text("1");
    h.type_text("2");
    assert_eq!(get(&h), (0, "12:30 AM".to_owned()), "12 AM is midnight");
    h.type_text("45");
    h.type_text("p");
    assert_eq!(get(&h), (12, "12:45 PM".to_owned()));
    h.type_text("a");
    assert_eq!(get(&h).0, 0);
    h.key(Key::ArrowUp);
    assert_eq!(get(&h).0, 12);
    h.key(Key::ArrowLeft);
    h.key(Key::ArrowLeft);
    h.type_text("3");
    assert_eq!(get(&h), (15, "3:45 PM".to_owned()));
}

#[test]
fn the_value_stays_inside_the_range() {
    let mut h = Harness::new();
    let picker = h.add(
        DatePicker::new(date(2026, 1, 1)).range(Some(date(2026, 3, 10)), Some(date(2026, 3, 20))),
    );
    let value = |h: &Harness| h.tree.get::<DatePicker>(picker).unwrap().value();
    assert_eq!(value(&h), date(2026, 3, 10));
    h.tree.set_focus(Some(picker));
    h.key(Key::ArrowRight); // day
    h.key(Key::ArrowDown);
    assert_eq!(value(&h), date(2026, 3, 10));
    assert!(values::<DateChanged>(&h.messages()).is_empty());
    for _ in 0..15 {
        h.key(Key::ArrowUp);
    }
    assert_eq!(value(&h), date(2026, 3, 20));
}

#[test]
fn the_stepper_style_steps_the_selected_field() {
    let mut h = Harness::new();
    let picker =
        h.add(DatePicker::new(date(2026, 11, 12)).style(DatePickerStyle::TextFieldAndStepper));
    let r = h.tree.window_rect(picker).unwrap();
    let up = Point::new(r.right() - 8.0, r.y + 4.0);
    let down = Point::new(r.right() - 8.0, r.bottom() - 4.0);
    h.click(up);
    assert_eq!(
        values::<DateChanged>(&h.messages()),
        [DateChanged(date(2026, 12, 12))]
    );
    h.click(Point::new(r.right() - 30.0, r.y + 12.0));
    h.click(down);
    assert_eq!(
        values::<DateChanged>(&h.messages()),
        [DateChanged(date(2025, 12, 12))]
    );
}

/// Window position of `day` in a graphical picker showing November 2026 (which starts
/// on a Sunday).
fn november_day(h: &Harness, picker: WidgetId, day: u32) -> Point {
    let index = day - 1;
    let (row, col) = ((index / 7) as f32, (index % 7) as f32);
    h.at(
        picker,
        8.0 + col * 28.0 + 14.0,
        8.0 + 28.0 + 20.0 + row * 24.0 + 12.0,
    )
}

#[test]
fn the_calendar_selects_days_by_click_and_keys_and_pages_months() {
    let mut h = Harness::new();
    let picker = h.add(
        DatePicker::new(DateTime::new(2026, 11, 12, 8, 0).unwrap())
            .style(DatePickerStyle::Graphical)
            .range(None, Some(DateTime::new(2026, 12, 31, 23, 59).unwrap())),
    );
    h.click(november_day(&h, picker, 25));
    let value = |h: &Harness| h.tree.get::<DatePicker>(picker).unwrap().value();
    assert_eq!(value(&h), DateTime::new(2026, 11, 25, 8, 0).unwrap());
    h.key(Key::ArrowRight);
    h.key(Key::ArrowDown);
    assert_eq!(value(&h).day, 26 + 7 - 30);
    assert_eq!(value(&h).month, 12);
    assert_eq!(
        h.tree
            .get::<DatePicker>(picker)
            .unwrap()
            .shown_month()
            .month,
        12
    );
    h.key(Key::PageDown);
    assert_eq!(value(&h), DateTime::new(2026, 12, 31, 23, 59).unwrap());
    let previous = h.at(picker, 8.0 + 7.0 * 28.0 - 48.0 + 10.0, 8.0 + 14.0);
    h.click(previous);
    assert_eq!(
        h.tree
            .get::<DatePicker>(picker)
            .unwrap()
            .shown_month()
            .month,
        11
    );
    assert_eq!(value(&h).month, 12);
}

#[test]
fn the_clock_takes_the_hands_and_tab_moves_from_calendar_to_clock() {
    let mut h = Harness::new();
    let picker = h.add(
        DatePicker::new(DateTime::new(2026, 11, 12, 15, 0).unwrap())
            .style(DatePickerStyle::Graphical)
            .elements(DateElements::DateAndTime),
    );
    let value = |h: &Harness| h.tree.get::<DatePicker>(picker).unwrap().value();
    // Clock center: right of the calendar.
    let (cx, cy) = (8.0 + 7.0 * 28.0 + 16.0 + 75.0, 8.0 + 75.0);
    // Far out at 3 o'clock: the minute hand -> 15 minutes.
    h.click(h.at(picker, cx + 70.0, cy));
    assert_eq!((value(&h).hour, value(&h).minute), (15, 15));
    // Near the center at 6 o'clock: the hour hand, staying in the afternoon.
    h.click(h.at(picker, cx, cy + 30.0));
    assert_eq!((value(&h).hour, value(&h).minute), (18, 15));

    h.tree.set_focus(Some(picker));
    h.key(Key::Tab);
    assert_eq!(h.tree.focused(), Some(picker));
    h.key(Key::ArrowUp);
    h.key(Key::PageDown);
    assert_eq!((value(&h).hour, value(&h).minute), (17, 16));
    assert_eq!(value(&h).day, 12);
}

#[test]
fn the_calendar_popover_edits_the_date_of_a_text_picker() {
    let mut h = Harness::new();
    let picker = h.add(
        DatePicker::new(DateTime::new(2026, 11, 12, 9, 45).unwrap())
            .elements(DateElements::DateAndTime)
            .calendar_popover(true),
    );
    h.click(h.at(picker, 10.0, 12.0));
    let calendar = h.overlay().expect("the calendar opened");
    assert_eq!(h.tree.focused(), Some(calendar));
    h.click(november_day(&h, calendar, 3));
    assert!(h.overlay().is_none());
    assert_eq!(h.tree.focused(), Some(picker));
    let changes = h.messages();
    assert!(changes.iter().all(|m| m.source == picker));
    assert_eq!(
        values::<DateChanged>(&changes),
        [DateChanged(DateTime::new(2026, 11, 3, 9, 45).unwrap())]
    );
}

#[test]
fn path_components_are_chosen_by_click_or_keyboard() {
    let mut h = Harness::new();
    let path = h.add(PathControl::new(["Macintosh HD", "Users", "noa"]));
    h.click(h.at(path, 8.0, 12.0));
    assert_eq!(
        values::<PathItemClicked>(&h.messages()),
        [PathItemClicked(0)]
    );
    h.tree.set_focus(Some(path));
    h.key(Key::ArrowLeft);
    h.key(Key::ArrowRight);
    h.key(Key::Space);
    assert_eq!(
        values::<PathItemClicked>(&h.messages()),
        [PathItemClicked(1)]
    );

    h.tree
        .update::<PathControl, _>(path, |p, cx| p.set_items(cx, ["Macintosh HD"]));
    h.prepare();
    h.key(Key::Space);
    assert_eq!(
        values::<PathItemClicked>(&h.messages()),
        [PathItemClicked(0)]
    );
}

#[test]
fn the_pop_up_path_lists_every_component_in_a_menu() {
    let mut h = Harness::new();
    let path = h.add(PathControl::new(["Macintosh HD", "Users", "noa"]).style(PathStyle::PopUp));
    h.click(h.at(path, 8.0, 12.0));
    let menu = h.overlay().expect("the menu opened");
    let labels: Vec<String> = h
        .tree
        .get::<Menu>(menu)
        .unwrap()
        .items()
        .iter()
        .map(|i| i.label().to_owned())
        .collect();
    assert_eq!(labels, ["Macintosh HD", "Users", "noa"]);
    h.key(Key::ArrowUp);
    h.key(Key::Enter);
    assert_eq!(
        values::<PathItemClicked>(&h.messages()),
        [PathItemClicked(1)]
    );
    assert_eq!(h.tree.focused(), Some(path));
}

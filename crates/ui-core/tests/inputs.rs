use noa_gpui_core::style::{FlexDirection, Size as LayoutSize, percent};
use noa_gpui_core::widgets::{
    CollectionItem, CollectionView, ComboBox, Container, DatePicker, DatePickerStyle,
    DateRangeChanged, FindAction, FindBar, FindOptions, FormField, InlineEditableLabel,
    ItemRenamed, LabelEdited, NumberInput, OutlineItem, OutlineView, RangeChanged,
    SelectionChanged, ShortcutChanged, ShortcutRecorder, Slider, TextArea, TextChanged, TextField,
    TextView, TokenField, TokensChanged, ValueChanged, drive_find,
};
use noa_gpui_core::{
    DateTime, Event, Key, Message, Modifiers, PointerButton, Shortcut, ShortcutPressed, Style,
    UiTree, WidgetId,
};
use noa_gpui_scene::{Point, Size};
use noa_gpui_text::{FontOptions, TextStore};

const VIEWPORT: Size = Size::new(800.0, 600.0);

struct Harness {
    tree: UiTree,
    text: TextStore,
    root: WidgetId,
    now: f64,
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
        let mut h = Self {
            tree,
            text: TextStore::new(FontOptions {
                locale: "en-US".into(),
                load_system_fonts: false,
            }),
            root,
            now: 10.0,
        };
        h.tree.set_now(h.now);
        h
    }

    fn add(&mut self, widget: impl noa_gpui_core::Widget) -> WidgetId {
        let id = self.tree.insert(Some(self.root), widget);
        self.prepare();
        id
    }

    fn prepare(&mut self) {
        self.tree.prepare(VIEWPORT, &mut self.text);
    }

    /// Let time pass (so presses do not count as double clicks).
    fn wait(&mut self, seconds: f64) {
        self.now += seconds;
        self.tree.set_now(self.now);
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

    fn focus(&mut self, id: Option<WidgetId>) {
        self.tree.set_focus(id);
        self.prepare();
    }

    fn get<W: noa_gpui_core::Widget>(&self, id: WidgetId) -> &W {
        self.tree.get::<W>(id).unwrap()
    }

    fn messages(&mut self) -> Vec<Message> {
        self.tree.take_messages()
    }
}

fn values<T: 'static + Clone>(messages: &[Message]) -> Vec<T> {
    messages
        .iter()
        .filter_map(|m| m.downcast_ref::<T>().cloned())
        .collect()
}

fn command() -> Modifiers {
    Modifiers {
        command: true,
        meta: true,
        ..Default::default()
    }
}

fn shift() -> Modifiers {
    Modifiers {
        shift: true,
        ..Default::default()
    }
}

#[test]
fn focus_changes_reach_the_widgets_after_the_event_that_moved_the_focus() {
    let mut h = Harness::new();
    let a = h.add(NumberInput::new(0.0, 10.0, 1.0));
    let b = h.add(TextField::new("", 100.0));
    h.focus(Some(a));
    assert!(h.get::<NumberInput>(a).is_editing());
    h.type_text("7");
    h.click(h.at(b, 10.0, 10.0));
    assert_eq!(h.get::<NumberInput>(a).value(), 7.0);
    assert!(!h.get::<NumberInput>(a).is_editing());
    assert_eq!(values::<ValueChanged>(&h.messages()), [ValueChanged(7.0)]);
}

#[test]
fn a_prefix_and_suffix_frame_the_input_without_being_part_of_it() {
    let mut h = Harness::new();
    let field = h.add(TextField::new("", 240.0).prefix("https://").suffix(".com"));
    h.focus(Some(field));
    h.type_text("example");
    let f = h.tree.get::<TextField>(field).unwrap();
    assert_eq!(f.text(), "example");
    assert_eq!((f.prefix_text(), f.suffix_text()), ("https://", ".com"));
    h.wait(1.0);
    h.click(h.at(field, 14.0, 12.0));
    assert_eq!(h.tree.get::<TextField>(field).unwrap().caret(), 0);
    let plain = h.add(TextField::new("", 240.0));
    h.focus(Some(plain));
    h.type_text("example");
    h.wait(1.0);
    h.click(h.at(plain, 14.0, 12.0));
    assert!(h.tree.get::<TextField>(plain).unwrap().caret() > 0);
}

#[test]
fn a_number_is_committed_clamped_on_return_and_bad_text_goes_back() {
    let mut h = Harness::new();
    let id = h.add(NumberInput::new(0.0, 100.0, 50.0).unit("px"));
    h.focus(Some(id));
    h.type_text("250");
    assert_eq!(h.get::<NumberInput>(id).text(), "250");
    h.key(Key::Enter);
    assert_eq!(h.get::<NumberInput>(id).value(), 100.0);
    assert_eq!(h.get::<NumberInput>(id).text(), "100");
    assert_eq!(values::<ValueChanged>(&h.messages()), [ValueChanged(100.0)]);
    h.type_text("12 px");
    h.key(Key::Enter);
    assert_eq!(h.get::<NumberInput>(id).value(), 12.0);
    h.messages();
    h.type_text("abc");
    h.key(Key::Enter);
    assert_eq!(h.get::<NumberInput>(id).value(), 12.0);
    assert_eq!(h.get::<NumberInput>(id).text(), "12");
    assert!(values::<ValueChanged>(&h.messages()).is_empty());
}

#[test]
fn rounding_never_leaves_the_range() {
    let mut h = Harness::new();
    let id = h.add(NumberInput::new(0.0, 0.75, 0.5).decimals(1));
    h.focus(Some(id));
    h.type_text("0.75");
    h.key(Key::Enter);
    let input = h.get::<NumberInput>(id);
    assert_eq!((input.value(), input.text()), (0.7, "0.7"));
}

#[test]
fn arrows_step_shift_steps_ten_and_decimals_round() {
    let mut h = Harness::new();
    let id = h.add(NumberInput::new(-5.0, 5.0, 0.0).step(0.1).decimals(1));
    h.focus(Some(id));
    h.key(Key::ArrowUp);
    h.key(Key::ArrowUp);
    h.key(Key::ArrowUp);
    assert_eq!(h.get::<NumberInput>(id).value(), 0.3);
    assert_eq!(h.get::<NumberInput>(id).text(), "0.3");
    h.key_with(Key::ArrowDown, shift());
    assert_eq!(h.get::<NumberInput>(id).value(), -0.7);
    h.type_text("1.26");
    h.key(Key::Enter);
    assert_eq!(h.get::<NumberInput>(id).text(), "1.3");
}

#[test]
fn dragging_across_an_unedited_field_scrubs_and_a_click_edits() {
    let mut h = Harness::new();
    let id = h.add(NumberInput::new(0.0, 100.0, 10.0));
    let start = h.at(id, 20.0, 12.0);
    h.down(start);
    h.drag_to(Point::new(start.x + 2.0, start.y));
    assert_eq!(h.get::<NumberInput>(id).value(), 10.0);
    h.drag_to(Point::new(start.x + 21.0, start.y));
    assert!(h.get::<NumberInput>(id).is_scrubbing());
    assert_eq!(h.get::<NumberInput>(id).value(), 15.0);
    h.drag_to(Point::new(start.x - 9.0, start.y));
    assert_eq!(h.get::<NumberInput>(id).value(), 8.0);
    h.up(Point::new(start.x - 9.0, start.y));
    assert!(!h.get::<NumberInput>(id).is_editing());
    assert_eq!(
        values::<ValueChanged>(&h.messages()),
        [ValueChanged(15.0), ValueChanged(8.0)]
    );
    h.wait(1.0);
    h.click(start);
    assert!(h.get::<NumberInput>(id).is_editing());
    assert_eq!(h.tree.focused(), Some(id));
}

#[test]
fn the_stepper_inside_the_field_steps_up_and_down() {
    let mut h = Harness::new();
    let id = h.add(NumberInput::new(0.0, 3.0, 2.0));
    let height = h.tree.window_rect(id).unwrap().height;
    h.click(h.at(id, 85.0, 6.0));
    assert_eq!(h.get::<NumberInput>(id).value(), 3.0);
    h.click(h.at(id, 85.0, 6.0));
    assert_eq!(h.get::<NumberInput>(id).value(), 3.0);
    h.click(h.at(id, 85.0, height - 6.0));
    assert_eq!(h.get::<NumberInput>(id).value(), 2.0);
}

#[test]
fn a_recorder_takes_a_modified_key_and_refuses_bare_ones() {
    let mut h = Harness::new();
    let id = h.add(ShortcutRecorder::new(None));
    h.tree.add_shortcut(Shortcut::command('k'), h.root);
    h.click(h.at(id, 40.0, 10.0));
    assert!(h.get::<ShortcutRecorder>(id).is_recording());
    h.key(Key::Char('k'));
    h.type_text("k");
    assert!(h.get::<ShortcutRecorder>(id).is_recording());
    h.key_with(Key::Char('k'), command());
    assert!(!h.get::<ShortcutRecorder>(id).is_recording());
    let shortcut = Shortcut::command('k');
    assert_eq!(h.get::<ShortcutRecorder>(id).shortcut(), Some(shortcut));
    let messages = h.messages();
    assert_eq!(
        values::<ShortcutChanged>(&messages),
        [ShortcutChanged(Some(shortcut))]
    );
    assert!(values::<ShortcutPressed>(&messages).is_empty());
    h.key(Key::Enter);
    let alt_shift = Modifiers {
        alt: true,
        shift: true,
        ..Default::default()
    };
    h.key_with(Key::Char('p'), alt_shift);
    let p = Shortcut {
        key: Key::Char('p'),
        command: false,
        shift: true,
        alt: true,
    };
    assert_eq!(h.get::<ShortcutRecorder>(id).shortcut(), Some(p));
    h.key(Key::Space);
    h.key(Key::Escape);
    assert!(!h.get::<ShortcutRecorder>(id).is_recording());
    assert_eq!(h.get::<ShortcutRecorder>(id).shortcut(), Some(p));
    h.messages();
    h.key(Key::Backspace);
    assert_eq!(h.get::<ShortcutRecorder>(id).shortcut(), None);
    assert_eq!(
        values::<ShortcutChanged>(&h.messages()),
        [ShortcutChanged(None)]
    );
}

#[test]
fn leaving_a_recorder_stops_recording() {
    let mut h = Harness::new();
    let id = h.add(ShortcutRecorder::new(Some(Shortcut::command('s'))));
    h.focus(Some(id));
    h.key(Key::Enter);
    assert!(h.tree.get::<ShortcutRecorder>(id).unwrap().is_recording());
    h.focus(None);
    let rec = h.tree.get::<ShortcutRecorder>(id).unwrap();
    assert!(!rec.is_recording());
    assert_eq!(rec.shortcut(), Some(Shortcut::command('s')));
}

#[test]
fn a_form_field_puts_its_label_above_and_the_error_in_place_of_the_help() {
    let mut h = Harness::new();
    let form = h.add(
        FormField::new("Email")
            .required(true)
            .help("We never share it."),
    );
    let field = h.tree.insert(Some(form), TextField::new("", 200.0));
    h.prepare();
    let outer = h.tree.window_rect(form).unwrap();
    let inner = h.tree.window_rect(field).unwrap();
    assert!(inner.y - outer.y >= 14.0);
    assert!(outer.bottom() - inner.bottom() >= 14.0);
    let f = h.tree.get::<FormField>(form).unwrap();
    assert!(f.is_required());
    assert_eq!(f.message(), Some("We never share it."));
    h.tree.update::<FormField, _>(form, |f, cx| {
        f.set_error(cx, Some("Enter an email address.".into()))
    });
    h.prepare();
    let f = h.tree.get::<FormField>(form).unwrap();
    assert_eq!(f.message(), Some("Enter an email address."));
    assert_eq!(h.tree.window_rect(form).unwrap().height, outer.height);
    let bare = h.add(FormField::new("Name"));
    let control = h.tree.insert(Some(bare), TextField::new("", 200.0));
    h.prepare();
    let (outer, inner) = (
        h.tree.window_rect(bare).unwrap(),
        h.tree.window_rect(control).unwrap(),
    );
    assert_eq!(outer.bottom(), inner.bottom());
}

#[test]
fn a_leading_label_puts_the_control_beside_it() {
    let mut h = Harness::new();
    let form = h.add(
        FormField::new("Name")
            .label_position(noa_gpui_core::widgets::LabelPosition::Leading(100.0)),
    );
    let field = h.tree.insert(Some(form), TextField::new("", 200.0));
    h.prepare();
    let (outer, inner) = (
        h.tree.window_rect(form).unwrap(),
        h.tree.window_rect(field).unwrap(),
    );
    assert_eq!(inner.y, outer.y);
    assert!(inner.x - outer.x >= 100.0);
}

#[test]
fn a_double_click_edits_a_label_and_return_commits_it() {
    let mut h = Harness::new();
    let id = h.add(InlineEditableLabel::new("Untitled"));
    let p = h.at(id, 10.0, 8.0);
    h.click(p);
    assert!(!h.get::<InlineEditableLabel>(id).is_editing());
    h.wait(0.1);
    h.click(p);
    assert!(h.get::<InlineEditableLabel>(id).is_editing());
    h.type_text("Notes");
    assert_eq!(
        h.get::<InlineEditableLabel>(id).edited_text(),
        Some("Notes")
    );
    h.key(Key::Enter);
    assert!(!h.get::<InlineEditableLabel>(id).is_editing());
    assert_eq!(h.get::<InlineEditableLabel>(id).text(), "Notes");
    assert_eq!(
        values::<LabelEdited>(&h.messages()),
        [LabelEdited("Notes".into())]
    );
}

#[test]
fn escape_cancels_leaving_commits_and_empty_names_are_refused() {
    let mut h = Harness::new();
    let id = h.add(InlineEditableLabel::new("Report"));
    h.focus(Some(id));
    h.key(Key::Enter);
    h.type_text("Draft");
    h.key(Key::Escape);
    assert_eq!(h.get::<InlineEditableLabel>(id).text(), "Report");
    h.key(Key::Enter);
    h.type_text("Final");
    h.focus(None);
    assert!(!h.get::<InlineEditableLabel>(id).is_editing());
    assert_eq!(h.get::<InlineEditableLabel>(id).text(), "Final");
    h.focus(Some(id));
    h.key(Key::Enter);
    h.key(Key::Backspace);
    h.key(Key::Enter);
    assert_eq!(h.get::<InlineEditableLabel>(id).text(), "Final");
    assert_eq!(
        values::<LabelEdited>(&h.messages()),
        [LabelEdited("Final".into())]
    );
}

fn outline_items() -> Vec<OutlineItem> {
    vec![
        OutlineItem::new("Documents")
            .expanded(true)
            .children([OutlineItem::new("Notes.txt"), OutlineItem::new("Plan.md")]),
        OutlineItem::new("Pictures"),
    ]
}

#[test]
fn return_renames_an_outline_item_in_place() {
    let mut h = Harness::new();
    let id = h.add(OutlineView::new(outline_items()).renamable(true));
    // The second row: Notes.txt (item 1).
    h.click(h.at(id, 120.0, 1.0 + 22.0 + 11.0));
    assert_eq!(h.get::<OutlineView>(id).selected_items(), [1]);
    h.key(Key::Enter);
    assert_eq!(h.get::<OutlineView>(id).renaming(), Some(1));
    h.type_text("Ideas.txt");
    h.key(Key::ArrowLeft);
    assert_eq!(h.get::<OutlineView>(id).selected_items(), [1]);
    h.key(Key::Enter);
    assert_eq!(h.get::<OutlineView>(id).renaming(), None);
    assert_eq!(h.get::<OutlineView>(id).title(1), "Ideas.txt");
    let renamed = values::<ItemRenamed>(&h.messages());
    assert_eq!(
        renamed,
        [ItemRenamed {
            item: 1,
            name: "Ideas.txt".into()
        }]
    );
    h.key(Key::Enter);
    h.type_text("Scratch");
    h.key(Key::Escape);
    assert_eq!(h.get::<OutlineView>(id).title(1), "Ideas.txt");
    h.key(Key::Enter);
    h.type_text("Scratch.txt");
    h.wait(1.0);
    h.click(h.at(id, 120.0, 1.0 + 3.0 * 22.0 + 11.0));
    assert_eq!(h.get::<OutlineView>(id).title(1), "Scratch.txt");
    assert_eq!(h.get::<OutlineView>(id).selected_items(), [3]);
}

#[test]
fn a_press_where_a_scrolled_away_rename_was_commits_it_and_selects_the_row() {
    let mut h = Harness::new();
    let items = (0..40).map(|i| OutlineItem::new(format!("Item {i}")));
    let id = h.add(
        OutlineView::new(items.collect())
            .renamable(true)
            .height(200.0),
    );
    // The second row, where its title starts.
    let title = h.at(id, 40.0, 1.0 + 22.0 + 11.0);
    h.click(title);
    h.key(Key::Enter);
    h.type_text("Renamed");
    // Ten rows up: row 1 and its editor are out of view, row 11 is in its place.
    let event = Event::Wheel {
        pos: title,
        delta: Point::new(0.0, -220.0),
    };
    h.tree.dispatch(event, &mut h.text);
    h.prepare();
    assert_eq!(h.get::<OutlineView>(id).visible_rows().start, 10);
    h.wait(1.0);
    h.click(title);
    assert_eq!(h.get::<OutlineView>(id).renaming(), None);
    assert_eq!(h.get::<OutlineView>(id).title(1), "Renamed");
    assert_eq!(h.get::<OutlineView>(id).selected_items(), [11]);
}

#[test]
fn replacing_the_items_cancels_a_rename() {
    let mut h = Harness::new();
    let items = ["One", "Two", "Three"].map(CollectionItem::new).to_vec();
    let id = h.add(CollectionView::new(items).renamable(true));
    h.tree
        .update::<CollectionView, _>(id, |v, cx| v.select_items(cx, [2]));
    h.focus(Some(id));
    h.key(Key::Enter);
    h.type_text("Third");
    h.tree.update::<CollectionView, _>(id, |v, cx| {
        v.set_items(cx, vec![CollectionItem::new("Only")])
    });
    h.prepare();
    assert_eq!(h.get::<CollectionView>(id).renaming(), None);
    h.key(Key::Enter);
    h.focus(None);
    assert_eq!(h.get::<CollectionView>(id).items()[0].title(), "Only");
    assert!(values::<ItemRenamed>(&h.messages()).is_empty());
}

#[test]
fn return_renames_a_collection_item_and_activates_without_renaming() {
    let mut h = Harness::new();
    let items = ["One", "Two", "Three"].map(CollectionItem::new).to_vec();
    let id = h.add(CollectionView::new(items).renamable(true));
    h.tree
        .update::<CollectionView, _>(id, |v, cx| v.select_items(cx, [2]));
    h.focus(Some(id));
    h.key(Key::Enter);
    assert_eq!(h.get::<CollectionView>(id).renaming(), Some(2));
    h.type_text("Third");
    h.key(Key::Enter);
    assert_eq!(h.get::<CollectionView>(id).items()[2].title(), "Third");
    assert_eq!(
        values::<ItemRenamed>(&h.messages()),
        [ItemRenamed {
            item: 2,
            name: "Third".into()
        }]
    );
    let plain = h.add(CollectionView::new(vec![CollectionItem::new("A")]));
    h.tree
        .update::<CollectionView, _>(plain, |v, cx| v.select_items(cx, [0]));
    h.focus(Some(plain));
    h.key(Key::Enter);
    assert_eq!(
        h.tree.get::<CollectionView>(plain).unwrap().renaming(),
        None
    );
}

fn find_setup(text: &str) -> (Harness, WidgetId, WidgetId) {
    let mut h = Harness::new();
    let bar = h.add(FindBar::new().replacing(true));
    let view = h.add(TextView::new(200.0).width(400.0));
    h.tree
        .update::<TextView, _>(view, |v, cx| v.set_text(cx, text));
    h.prepare();
    h.focus(Some(bar));
    (h, bar, view)
}

fn pump(h: &mut Harness, bar: WidgetId, view: WidgetId) -> Vec<FindAction> {
    let messages = h.messages();
    for m in &messages {
        drive_find(&mut h.tree, bar, view, m);
    }
    h.prepare();
    values::<FindAction>(&messages)
}

#[test]
fn typing_in_the_find_bar_highlights_matches_and_return_walks_them() {
    let (mut h, bar, view) = find_setup("cat dog cat bird cat");
    h.type_text("cat");
    let actions = pump(&mut h, bar, view);
    assert_eq!(
        actions.last(),
        Some(&FindAction::Search {
            query: "cat".into(),
            options: FindOptions::default()
        })
    );
    let v = h.tree.get::<TextView>(view).unwrap();
    assert_eq!(v.found_ranges(), [0..3, 8..11, 17..20]);
    let status = h.tree.get::<FindBar>(bar).unwrap().status().clone();
    assert_eq!((status.total, status.current), (3, Some(0)));
    h.key(Key::Enter);
    pump(&mut h, bar, view);
    assert_eq!(h.tree.get::<TextView>(view).unwrap().selection(), 8..11);
    h.key_with(Key::Char('g'), command());
    pump(&mut h, bar, view);
    assert_eq!(h.tree.get::<TextView>(view).unwrap().selection(), 17..20);
    h.key(Key::Enter);
    pump(&mut h, bar, view);
    assert_eq!(h.tree.get::<TextView>(view).unwrap().selection(), 0..3);
    h.key_with(Key::Enter, shift());
    pump(&mut h, bar, view);
    assert_eq!(h.tree.get::<TextView>(view).unwrap().selection(), 17..20);
    assert_eq!(
        h.tree.get::<FindBar>(bar).unwrap().status().current,
        Some(2)
    );
}

#[test]
fn the_toggles_change_how_the_bar_matches() {
    let (mut h, bar, view) = find_setup("Cat cat concat");
    h.type_text("cat");
    pump(&mut h, bar, view);
    assert_eq!(
        h.tree.get::<TextView>(view).unwrap().found_ranges().len(),
        3
    );
    // "Aa", 26 wide, left of the regex and word toggles, chevrons and Done.
    let width = h.tree.window_rect(bar).unwrap().width;
    let case_x = width - 6.0 - 52.0 - 4.0 - 5.0 * 30.0 - 4.0 + 13.0;
    h.click(h.at(bar, case_x, 19.0));
    pump(&mut h, bar, view);
    assert!(
        h.tree
            .get::<FindBar>(bar)
            .unwrap()
            .find_options()
            .case_sensitive
    );
    assert_eq!(
        h.tree.get::<TextView>(view).unwrap().found_ranges(),
        [4..7, 11..14]
    );
    // "W": whole words only.
    h.click(h.at(bar, case_x + 30.0, 19.0));
    pump(&mut h, bar, view);
    let found = h.tree.get::<TextView>(view).unwrap().found_ranges();
    assert_eq!((found.len(), found[0].clone()), (1, 4..7));
}

#[test]
fn replace_takes_the_current_match_and_all_takes_every_one_in_one_undo() {
    let (mut h, bar, view) = find_setup("a1 b22 c333");
    h.type_text(r"\d+");
    pump(&mut h, bar, view);
    assert_eq!(h.tree.get::<FindBar>(bar).unwrap().status().total, 0);
    let width = h.tree.window_rect(bar).unwrap().width;
    let regex_x = width - 6.0 - 52.0 - 4.0 - 3.0 * 30.0 - 4.0 + 13.0;
    h.click(h.at(bar, regex_x, 19.0));
    pump(&mut h, bar, view);
    assert_eq!(h.tree.get::<FindBar>(bar).unwrap().status().total, 3);
    h.key(Key::Tab);
    h.type_text("#");
    for _ in 0..3 {
        h.key(Key::Enter);
        pump(&mut h, bar, view);
    }
    assert_eq!(h.tree.get::<TextView>(view).unwrap().text(), "a# b# c333");
    h.key_with(Key::Char('a'), command());
    h.type_text("<$0>");
    // "All", on the replace row.
    h.click(h.at(bar, width - 6.0 - 32.0, 6.0 + 26.0 + 4.0 + 13.0));
    let actions = pump(&mut h, bar, view);
    assert_eq!(actions, [FindAction::ReplaceAll("<$0>".into())]);
    assert_eq!(h.tree.get::<TextView>(view).unwrap().text(), "a# b# c<333>");
    assert_eq!(
        h.tree.get::<FindBar>(bar).unwrap().status().replaced,
        Some(1)
    );
    h.focus(Some(view));
    h.key_with(Key::Char('z'), command());
    assert_eq!(h.tree.get::<TextView>(view).unwrap().text(), "a# b# c333");
}

#[test]
fn escape_closes_the_bar_clears_the_highlights_and_refocuses_the_text() {
    let (mut h, bar, view) = find_setup("one two one");
    h.type_text("one");
    pump(&mut h, bar, view);
    assert_eq!(
        h.tree.get::<TextView>(view).unwrap().found_ranges().len(),
        2
    );
    h.key(Key::Escape);
    let actions = pump(&mut h, bar, view);
    assert_eq!(actions, [FindAction::Close]);
    assert!(h.tree.is_hidden(bar));
    assert_eq!(h.tree.focused(), Some(view));
    assert!(
        h.tree
            .get::<TextView>(view)
            .unwrap()
            .found_ranges()
            .is_empty()
    );
}

#[test]
fn a_find_bar_also_drives_a_text_area() {
    let mut h = Harness::new();
    let bar = h.add(FindBar::new());
    let area = h.add(TextArea::new("").width(300.0));
    h.tree
        .update::<TextArea, _>(area, |a, cx| a.set_text(cx, "ab ab ab"));
    h.focus(Some(bar));
    h.type_text("ab");
    pump(&mut h, bar, area);
    assert_eq!(h.get::<TextArea>(area).found_ranges().len(), 3);
    h.key(Key::Enter);
    pump(&mut h, bar, area);
    assert_eq!(h.get::<TextArea>(area).selection(), 3..5);
}

#[test]
fn reopening_the_bar_searches_again() {
    let (mut h, bar, view) = find_setup("one two one");
    h.type_text("one");
    pump(&mut h, bar, view);
    h.key(Key::Escape);
    pump(&mut h, bar, view);
    assert!(h.get::<TextView>(view).found_ranges().is_empty());
    h.tree.set_hidden(bar, false);
    h.focus(Some(bar));
    pump(&mut h, bar, view);
    assert_eq!(h.get::<TextView>(view).found_ranges().len(), 2);
    h.key(Key::Enter);
    pump(&mut h, bar, view);
    assert_eq!(h.get::<FindBar>(bar).status().current, Some(1));
}

#[test]
fn replacing_leaves_a_read_only_text_alone() {
    let mut h = Harness::new();
    let bar = h.add(FindBar::new().replacing(true));
    let view = h.add(TextView::new(100.0).width(300.0).read_only(true));
    h.tree
        .update::<TextView, _>(view, |v, cx| v.set_text(cx, "aa aa"));
    h.focus(Some(bar));
    h.type_text("aa");
    pump(&mut h, bar, view);
    let messages = [
        FindAction::Replace("b".into()),
        FindAction::ReplaceAll("b".into()),
    ];
    for action in &messages {
        h.tree
            .update::<TextView, _>(view, |v, cx| v.find(cx, action));
    }
    h.prepare();
    assert_eq!(h.get::<TextView>(view).text(), "aa aa");
    assert_eq!(h.get::<TextView>(view).found_ranges().len(), 2);
}

#[test]
fn replacing_with_text_that_matches_moves_on_to_the_next_match() {
    let mut h = Harness::new();
    let view = h.add(TextView::new(100.0).width(300.0));
    h.tree
        .update::<TextView, _>(view, |v, cx| v.set_text(cx, "a b a"));
    let find = |h: &mut Harness, action: FindAction| {
        h.tree
            .update::<TextView, _>(view, |v, cx| v.find(cx, &action));
        h.prepare();
    };
    let query = FindAction::Search {
        query: "a".into(),
        options: Default::default(),
    };
    find(&mut h, query);
    find(&mut h, FindAction::Replace("aa".into()));
    find(&mut h, FindAction::Replace("aa".into()));
    assert_eq!(h.get::<TextView>(view).text(), "aa b a");
    let view_ = h.get::<TextView>(view);
    let current = view_.found_ranges()[view_.current_found().unwrap()].clone();
    assert_eq!(current, 5..6);
    find(&mut h, FindAction::Replace("aa".into()));
    assert_eq!(h.get::<TextView>(view).text(), "aa b aa");
}

#[test]
fn matches_follow_edits_to_the_text() {
    let (mut h, bar, view) = find_setup("x y x");
    h.type_text("x");
    pump(&mut h, bar, view);
    h.focus(Some(view));
    h.type_text(" x");
    assert_eq!(
        h.tree.get::<TextView>(view).unwrap().found_ranges().len(),
        3
    );
}

const KNOB: f32 = 18.0;
const LENGTH: f32 = 160.0;

fn slider_x(t: f32) -> f32 {
    KNOB / 2.0 + t * (LENGTH - KNOB)
}

#[test]
fn a_range_slider_moves_the_nearer_knob_and_knobs_do_not_cross() {
    let mut h = Harness::new();
    let id = h.add(Slider::range(0.0, 100.0, 20.0, 80.0));
    let range = |h: &Harness| h.tree.get::<Slider>(id).unwrap().range_values().unwrap();
    h.click(h.at(id, slider_x(0.9), 9.0));
    assert_eq!(range(&h).1.round(), 90.0);
    h.click(h.at(id, slider_x(0.1), 9.0));
    assert_eq!(range(&h).0.round(), 10.0);
    h.down(h.at(id, slider_x(0.1), 9.0));
    h.drag_to(h.at(id, slider_x(1.0), 9.0));
    h.up(h.at(id, slider_x(1.0), 9.0));
    let (low, high) = range(&h);
    assert_eq!(low, high);
    let changes = values::<RangeChanged>(&h.messages());
    assert!(changes.iter().all(|c| c.low <= c.high));
    assert!(values::<ValueChanged>(&h.messages()).is_empty());
}

#[test]
fn tab_moves_between_the_knobs_before_leaving_the_slider() {
    let mut h = Harness::new();
    let id = h.add(Slider::range(0.0, 10.0, 2.0, 8.0).step(1.0));
    let after = h.add(TextField::new("", 100.0));
    h.focus(Some(id));
    assert!(!h.get::<Slider>(id).upper_knob_active());
    h.key(Key::ArrowRight);
    assert_eq!(h.get::<Slider>(id).range_values(), Some((3.0, 8.0)));
    h.key(Key::Tab);
    assert_eq!(h.tree.focused(), Some(id));
    assert!(h.get::<Slider>(id).upper_knob_active());
    h.key(Key::ArrowLeft);
    assert_eq!(h.get::<Slider>(id).range_values(), Some((3.0, 7.0)));
    h.key(Key::Tab);
    assert_eq!(h.tree.focused(), Some(after));
    assert_eq!(
        values::<RangeChanged>(&h.messages()),
        [
            RangeChanged {
                low: 3.0,
                high: 8.0
            },
            RangeChanged {
                low: 3.0,
                high: 7.0
            }
        ]
    );
}

fn date(year: i32, month: u32, day: u32) -> DateTime {
    DateTime::date(year, month, day).unwrap()
}

/// The center of `day` in November 2026 (which starts on a Sunday).
fn november(day: u32) -> (f32, f32) {
    let i = day - 1;
    (
        8.0 + (i % 7) as f32 * 28.0 + 14.0,
        8.0 + 28.0 + 20.0 + (i / 7) as f32 * 24.0 + 12.0,
    )
}

#[test]
fn dragging_across_days_selects_a_range_and_shift_extends_it() {
    let mut h = Harness::new();
    let id = h.add(
        DatePicker::new(date(2026, 11, 12))
            .style(DatePickerStyle::Graphical)
            .range_selection(true),
    );
    assert_eq!(
        h.get::<DatePicker>(id).date_range(),
        Some((date(2026, 11, 12), date(2026, 11, 12)))
    );
    let (x, y) = november(10);
    h.down(h.at(id, x, y));
    let (x2, y2) = november(17);
    h.drag_to(h.at(id, x2, y2));
    h.up(h.at(id, x2, y2));
    assert_eq!(
        h.get::<DatePicker>(id).date_range(),
        Some((date(2026, 11, 10), date(2026, 11, 17)))
    );
    let (x3, y3) = november(5);
    h.wait(1.0);
    h.down(h.at(id, x, y));
    h.drag_to(h.at(id, x3, y3));
    h.up(h.at(id, x3, y3));
    assert_eq!(
        h.get::<DatePicker>(id).date_range(),
        Some((date(2026, 11, 5), date(2026, 11, 10)))
    );
    h.messages();
    let (x4, y4) = november(20);
    h.wait(1.0);
    h.down(h.at(id, x4, y4));
    h.up(h.at(id, x4, y4));
    let shift_click = |h: &mut Harness, p: Point| {
        h.tree
            .dispatch(Event::ModifiersChanged(shift()), &mut h.text);
        h.click(p);
        h.tree
            .dispatch(Event::ModifiersChanged(Modifiers::default()), &mut h.text);
    };
    let (x5, y5) = november(24);
    h.wait(1.0);
    let p = h.at(id, x5, y5);
    shift_click(&mut h, p);
    assert_eq!(
        h.get::<DatePicker>(id).date_range(),
        Some((date(2026, 11, 20), date(2026, 11, 24)))
    );
    let last = values::<DateRangeChanged>(&h.messages()).pop();
    assert_eq!(
        last,
        Some(DateRangeChanged {
            start: date(2026, 11, 20),
            end: date(2026, 11, 24)
        })
    );
}

#[test]
fn shift_arrows_extend_a_date_range_and_plain_arrows_move_one_day() {
    let mut h = Harness::new();
    let id = h.add(
        DatePicker::new(date(2026, 11, 12))
            .style(DatePickerStyle::Graphical)
            .range_selection(true),
    );
    h.focus(Some(id));
    h.key_with(Key::ArrowRight, shift());
    h.key_with(Key::ArrowDown, shift());
    assert_eq!(
        h.get::<DatePicker>(id).date_range(),
        Some((date(2026, 11, 12), date(2026, 11, 20)))
    );
    h.key(Key::ArrowLeft);
    assert_eq!(
        h.get::<DatePicker>(id).date_range(),
        Some((date(2026, 11, 19), date(2026, 11, 19)))
    );
}

const FRUITS: [&str; 5] = ["Apple", "Apricot", "Banana", "Grape", "Pineapple"];

fn completion_row(h: &Harness, i: usize) -> Point {
    let list = h.tree.overlays().last().expect("a list is open");
    h.at(list, 40.0, 4.0 + i as f32 * 24.0 + 12.0)
}

#[test]
fn typing_in_a_token_field_lists_matching_completions_best_first() {
    let mut h = Harness::new();
    let id = h.add(TokenField::new("Fruit", 300.0).completions(FRUITS));
    h.focus(Some(id));
    h.type_text("ap");
    assert!(h.get::<TokenField>(id).completions_open());
    assert_eq!(h.tree.overlays().count(), 1);
    // Word starts first; "Grape" matches inside a word.
    let shown = h.get::<TokenField>(id).completions_shown();
    assert_eq!(&shown[..2], ["Apple", "Apricot"]);
    assert!(shown.contains(&"Grape".to_owned()));
    assert_eq!(h.tree.focused(), Some(id));
    h.key(Key::ArrowDown);
    assert_eq!(h.get::<TokenField>(id).highlighted_completion(), Some(1));
    h.key(Key::Enter);
    assert_eq!(h.get::<TokenField>(id).tokens(), ["Apricot"]);
    assert_eq!(h.get::<TokenField>(id).input(), "");
    assert!(!h.get::<TokenField>(id).completions_open());
    assert_eq!(h.tree.overlays().count(), 0);
    assert_eq!(
        values::<TokensChanged>(&h.messages()),
        [TokensChanged(vec!["Apricot".into()])]
    );
    h.type_text("apr");
    assert!(!h.get::<TokenField>(id).completions_open());
    h.key(Key::Backspace);
    h.key(Key::Backspace);
    assert!(h.get::<TokenField>(id).completions_open());
    h.key(Key::Escape);
    assert!(!h.get::<TokenField>(id).completions_open());
    assert_eq!(h.get::<TokenField>(id).input(), "a");
}

#[test]
fn tokens_in_any_letter_case_are_not_offered_again() {
    let mut h = Harness::new();
    let id = h.add(
        TokenField::new("Fruit", 300.0)
            .completions(FRUITS)
            .with_tokens(["APPLE", "grape"]),
    );
    h.focus(Some(id));
    h.type_text("ap");
    let shown = h.get::<TokenField>(id).completions_shown();
    assert_eq!(shown, ["Apricot", "Pineapple"]);
}

#[test]
fn leaving_the_field_closes_its_completions_and_the_focus_moves_on() {
    let mut h = Harness::new();
    // Above the field, so the list (which opens below it) does not cover it.
    let next = h.add(TextField::new("", 100.0));
    let id = h.add(TokenField::new("", 300.0).completions(FRUITS));
    h.focus(Some(id));
    h.type_text("ap");
    assert!(h.get::<TokenField>(id).completions_open());
    h.focus(Some(next));
    assert!(!h.get::<TokenField>(id).completions_open());
    assert_eq!(h.tree.overlays().count(), 0);
    assert_eq!(h.tree.focused(), Some(next));
    h.focus(Some(id));
    h.key(Key::Backspace);
    assert!(h.get::<TokenField>(id).completions_open());
    h.click(h.at(next, 10.0, 10.0));
    assert!(!h.get::<TokenField>(id).completions_open());
    assert_eq!(h.tree.focused(), Some(next));
}

#[test]
fn a_click_in_the_list_takes_a_completion_without_moving_the_focus() {
    let mut h = Harness::new();
    let id = h.add(TokenField::new("", 300.0).completions(FRUITS));
    h.focus(Some(id));
    h.type_text("an");
    let shown = h.tree.get::<TokenField>(id).unwrap().completions_shown();
    assert_eq!(shown[0], "Banana");
    let row = completion_row(&h, 0);
    h.drag_to(row);
    h.click(row);
    assert_eq!(h.tree.get::<TokenField>(id).unwrap().tokens(), ["Banana"]);
    assert_eq!(h.tree.focused(), Some(id));
}

#[test]
fn completions_can_come_from_a_source_called_for_each_query() {
    let mut h = Harness::new();
    let calls = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    let seen = calls.clone();
    let id = h.add(TokenField::new("", 300.0).completion_source(move |query| {
        seen.borrow_mut().push(query.to_owned());
        (1..=3).map(|i| format!("{query}{i}@example.com")).collect()
    }));
    h.focus(Some(id));
    h.type_text("bo");
    let shown = h.tree.get::<TokenField>(id).unwrap().completions_shown();
    assert_eq!(
        shown,
        ["bo1@example.com", "bo2@example.com", "bo3@example.com"]
    );
    h.type_text("b");
    assert_eq!(*calls.borrow(), ["bo", "bob"]);
    let shown = h.tree.get::<TokenField>(id).unwrap().completions_shown();
    assert_eq!(shown[0], "bob1@example.com");
}

#[test]
fn several_completions_can_be_checked_and_added_together() {
    let mut h = Harness::new();
    let id = h.add(
        TokenField::new("", 300.0)
            .completions(FRUITS)
            .multiple_completions(true),
    );
    h.focus(Some(id));
    h.type_text("ap");
    h.key_with(Key::Enter, command());
    h.click(Point::new(
        completion_row(&h, 1).x - 30.0,
        completion_row(&h, 1).y,
    ));
    assert_eq!(h.tree.get::<TokenField>(id).unwrap().tokens().len(), 0);
    h.key(Key::Enter);
    assert_eq!(
        h.tree.get::<TokenField>(id).unwrap().tokens(),
        ["Apple", "Apricot"]
    );
}

#[test]
fn a_combo_box_lists_completions_and_can_hold_several_values() {
    let mut h = Harness::new();
    let single = h.add(ComboBox::new(FRUITS, 220.0).completion_list(true));
    h.focus(Some(single));
    h.type_text("gr");
    assert_eq!(h.get::<ComboBox>(single).completions_shown(), ["Grape"]);
    assert_eq!(h.get::<ComboBox>(single).text(), "gr");
    h.key(Key::Enter);
    assert_eq!(h.get::<ComboBox>(single).text(), "Grape");
    let messages = h.messages();
    assert_eq!(values::<SelectionChanged>(&messages), [SelectionChanged(3)]);
    assert_eq!(
        values::<TextChanged>(&messages).last(),
        Some(&TextChanged("Grape".into()))
    );

    let many = h.add(
        ComboBox::new(FRUITS, 220.0)
            .completion_list(true)
            .multiple(true),
    );
    h.focus(Some(many));
    h.type_text("ban");
    h.key(Key::Enter);
    assert_eq!(h.get::<ComboBox>(many).text(), "Banana, ");
    h.type_text("ap");
    h.key(Key::Enter);
    assert_eq!(h.get::<ComboBox>(many).values(), ["Banana", "Apple"]);
    h.type_text("ban");
    assert!(!h.get::<ComboBox>(many).completions_open());
}

#[test]
fn the_last_allowed_day_can_be_clicked_whatever_the_time_of_day() {
    let mut h = Harness::new();
    let id = h.add(
        DatePicker::new(DateTime::new(2026, 11, 12, 14, 5).unwrap())
            .style(DatePickerStyle::Graphical)
            .range(Some(date(2026, 11, 1)), Some(date(2026, 11, 30))),
    );
    let (x, y) = november(30);
    h.click(h.at(id, x, y));
    assert_eq!(h.get::<DatePicker>(id).value(), date(2026, 11, 30));
    let (x, y) = november(1);
    h.click(h.at(id, x, y));
    assert_eq!(
        h.get::<DatePicker>(id).value(),
        DateTime::new(2026, 11, 1, 0, 0).unwrap()
    );
}

#[test]
fn a_range_of_days_starts_inside_the_allowed_range() {
    let picker = DatePicker::new(date(2026, 11, 12))
        .style(DatePickerStyle::Graphical)
        .range_selection(true)
        .range(Some(date(2026, 11, 20)), None);
    assert_eq!(picker.value(), date(2026, 11, 20));
    assert_eq!(
        picker.date_range(),
        Some((date(2026, 11, 20), date(2026, 11, 20)))
    );
}

#[test]
fn a_number_input_survives_bounds_and_values_that_are_not_numbers() {
    let input = NumberInput::new(10.0, 0.0, 5.0);
    assert_eq!(input.value(), 5.0);
    assert_eq!(NumberInput::new(10.0, 0.0, 50.0).value(), 10.0);
    assert_eq!(NumberInput::new(f64::NAN, 10.0, 50.0).value(), 10.0);
    assert_eq!(NumberInput::new(0.0, 10.0, f64::NAN).value(), 0.0);

    let mut h = Harness::new();
    let id = h.add(NumberInput::new(0.0, 10.0, 3.0).step(f64::NAN));
    h.tree
        .update::<NumberInput, _>(id, |n, cx| n.set_value(cx, f64::NAN));
    assert_eq!(h.get::<NumberInput>(id).value(), 3.0);
    h.focus(Some(id));
    h.messages();
    h.key(Key::ArrowUp);
    assert_eq!(h.get::<NumberInput>(id).value(), 4.0);
    assert_eq!(values::<ValueChanged>(&h.messages()), [ValueChanged(4.0)]);
}

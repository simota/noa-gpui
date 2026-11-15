use noa_gpui_core::style::{FlexDirection, Size as LayoutSize, length, percent};
use noa_gpui_core::widgets::{
    ComboBox, Container, FindAction, FindOptions, Label, LogLine, LogView, Menu, ReadingChanged,
    SearchField, SearchSubmitted, SecureTextField, SelectionChanged, TextArea, TextChanged,
    TextField, TextSubmitted, TextView, TokenField, TokensChanged, Truncation,
};
use noa_gpui_core::{
    Event, Key, Message, Modifiers, PointerButton, Style, UiTree, WidgetId, access_id, accesskit,
    kana,
};
use noa_gpui_scene::{Color, DrawCommand, Point, Size};
use noa_gpui_text::{FontOptions, TextAlign, TextStore};

const VIEWPORT: Size = Size::new(800.0, 600.0);
const ELLIPSIS: char = '\u{2026}';

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
        self.add_to(self.root, widget)
    }

    fn add_to(&mut self, parent: WidgetId, widget: impl noa_gpui_core::Widget) -> WidgetId {
        let id = self.tree.insert(Some(parent), widget);
        self.prepare();
        id
    }

    fn boxed(&mut self, width: f32) -> WidgetId {
        self.add(Container::new(Style {
            size: LayoutSize {
                width: length(width),
                height: noa_gpui_core::style::auto(),
            },
            ..Default::default()
        }))
    }

    fn prepare(&mut self) {
        self.tree.prepare(VIEWPORT, &mut self.text);
    }

    fn at(&self, id: WidgetId, x: f32, y: f32) -> Point {
        let r = self.tree.window_rect(id).unwrap();
        Point::new(r.x + x, r.y + y)
    }

    fn press(&mut self, pos: Point) {
        let button = PointerButton::Primary;
        self.tree
            .dispatch(Event::PointerDown { pos, button }, &mut self.text);
        self.tree
            .dispatch(Event::PointerUp { pos, button }, &mut self.text);
        self.prepare();
    }

    fn focus(&mut self, id: WidgetId) {
        self.tree.set_focus(Some(id));
    }

    fn key(&mut self, key: Key) {
        self.key_with(key, Modifiers::default());
    }

    fn command(&mut self, c: char) {
        self.key_with(
            Key::Char(c),
            Modifiers {
                command: true,
                meta: true,
                ..Default::default()
            },
        );
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

    fn compose(&mut self, text: &str) {
        let cursor = Some((text.len(), text.len()));
        self.tree.dispatch(
            Event::Composition {
                text: text.into(),
                cursor,
            },
            &mut self.text,
        );
        self.prepare();
    }

    fn clipboard(&mut self) -> Option<String> {
        self.tree.clipboard().read_text()
    }

    fn set_clipboard(&mut self, text: &str) {
        self.tree.clipboard().write_text(text);
    }

    fn messages(&mut self) -> Vec<Message> {
        self.tree.take_messages()
    }

    fn access_value(&self, id: WidgetId) -> Option<String> {
        let update = self.tree.accessibility_update(1.0);
        update
            .nodes
            .iter()
            .find(|(n, _)| *n == access_id(id))
            .and_then(|(_, node)| node.value().map(str::to_owned))
    }
}

fn values<T: 'static + Clone>(messages: &[Message]) -> Vec<T> {
    messages
        .iter()
        .filter_map(|m| m.downcast_ref::<T>().cloned())
        .collect()
}

const LONG: &str = "The quick brown fox jumps over the lazy dog";

#[test]
fn labels_truncate_at_the_tail_middle_or_head_to_fit() {
    let mut h = Harness::new();
    for (mode, check) in [
        (
            Truncation::Tail,
            (|s: &str| s.starts_with("The") && s.ends_with(ELLIPSIS)) as fn(&str) -> bool,
        ),
        (Truncation::Middle, |s: &str| {
            s.starts_with("The") && s.ends_with("dog") && s.contains(ELLIPSIS)
        }),
        (Truncation::Head, |s: &str| {
            s.starts_with(ELLIPSIS) && s.ends_with("dog")
        }),
    ] {
        let parent = h.boxed(150.0);
        let label = h.add_to(parent, Label::new(LONG).truncation(mode));
        let shown = h.tree.get::<Label>(label).unwrap().shown_text().to_owned();
        assert!(check(&shown), "{mode:?}: {shown}");
        assert!(shown.chars().count() < LONG.chars().count());
        assert!(h.tree.window_rect(label).unwrap().width <= 150.0);
    }
    let parent = h.boxed(600.0);
    let label = h.add_to(parent, Label::new(LONG).truncation(Truncation::Tail));
    assert_eq!(h.tree.get::<Label>(label).unwrap().shown_text(), LONG);
}

#[test]
fn aligned_labels_center_or_end_their_lines_and_their_text_in_a_wider_label() {
    let mut h = Harness::new();
    let mut drawn_x = |align: TextAlign| {
        let column = h.add(Container::new(Style {
            flex_direction: FlexDirection::Column,
            size: LayoutSize {
                width: length(300.0),
                height: noa_gpui_core::style::auto(),
            },
            ..Default::default()
        }));
        let label = h.add_to(column, Label::new("Hi\nA longer second line").align(align));
        let rect = h.tree.window_rect(label).unwrap();
        assert_eq!(rect.width, 300.0, "stretched by the column");
        let text = h
            .tree
            .display_list()
            .commands()
            .iter()
            .rev()
            .find_map(|c| match c {
                DrawCommand::Text { rect, layout, .. } => Some((*rect, *layout)),
                _ => None,
            })
            .unwrap();
        let first_glyph = h
            .text
            .layouts()
            .buffer(text.1)
            .layout_runs()
            .next()
            .and_then(|run| run.glyphs.first().map(|g| g.x))
            .unwrap();
        (text.0.x - rect.x, text.0.width, first_glyph)
    };
    let (start_x, width, start_glyph) = drawn_x(TextAlign::Start);
    assert_eq!((start_x, start_glyph), (0.0, 0.0));
    let (center_x, _, center_glyph) = drawn_x(TextAlign::Center);
    assert!(
        (center_x - ((300.0 - width) / 2.0).round()).abs() < 0.5,
        "{center_x}"
    );
    assert!(
        center_glyph > 1.0,
        "the short line is centered under the long one"
    );
    let (end_x, _, end_glyph) = drawn_x(TextAlign::End);
    assert_eq!(end_x, 300.0 - width);
    assert!(end_glyph > center_glyph);
}

#[test]
fn a_line_limit_ends_the_last_line_with_an_ellipsis() {
    let mut h = Harness::new();
    let parent = h.boxed(120.0);
    let label = h.add_to(parent, Label::new(LONG).max_lines(2));
    let shown = h.tree.get::<Label>(label).unwrap().shown_text().to_owned();
    assert!(shown.ends_with(ELLIPSIS), "{shown}");
    let height = h.tree.window_rect(label).unwrap().height;
    let line = noa_gpui_text::TextStyle::new(14.0).line_height;
    assert!(height <= 2.0 * line + 0.5, "{height}");
    assert!(height > line);
}

#[test]
fn selectable_aligned_labels_draw_and_select_where_the_text_is() {
    let mut h = Harness::new();
    let column = h.add(Container::new(Style {
        flex_direction: FlexDirection::Column,
        size: LayoutSize {
            width: length(300.0),
            height: noa_gpui_core::style::auto(),
        },
        ..Default::default()
    }));
    let label = h.add_to(
        column,
        Label::new("Copy this")
            .selectable(true)
            .align(TextAlign::End),
    );
    let rect = h.tree.window_rect(label).unwrap();
    let drawn = h
        .tree
        .display_list()
        .commands()
        .iter()
        .rev()
        .find_map(|c| match c {
            DrawCommand::Text { rect, .. } => Some(*rect),
            _ => None,
        })
        .unwrap();
    assert!(
        drawn.x - rect.x > 150.0,
        "drawn at the end: {drawn:?} in {rect:?}"
    );
    let word = Point::new(drawn.x + 10.0, rect.y + 8.0);
    h.press(word);
    h.press(word);
    assert_eq!(h.tree.get::<Label>(label).unwrap().selection(), Some(0..4));
}

fn quarter_labels(h: &mut Harness) -> Vec<WidgetId> {
    let quarter = Style {
        size: LayoutSize {
            width: percent(0.25),
            height: noa_gpui_core::style::auto(),
        },
        ..Default::default()
    };
    [Truncation::Tail, Truncation::Middle, Truncation::Head]
        .into_iter()
        .map(|truncation| {
            let parent = h.add(Container::new(quarter.clone()));
            h.add_to(parent, Label::new(LONG).truncation(truncation))
        })
        .collect()
}

#[test]
fn labels_resized_through_many_widths_are_cut_as_new_labels_at_each_width_are() {
    let mut h = Harness::new();
    let labels = quarter_labels(&mut h);
    let shown = |h: &Harness, labels: &[WidgetId]| -> Vec<String> {
        let label = |&id: &WidgetId| h.tree.get::<Label>(id).unwrap().shown_text().to_owned();
        labels.iter().map(label).collect()
    };
    // More widths than a label remembers the cut for, twice over, a pixel apart and
    // far apart: the second time, the candidates' sizes are known.
    let widths = [560.0, 561.0, 600.0, 640.0, 500.0, 563.0, 700.0, 480.0];
    let mut changes = Vec::new();
    for width in widths.into_iter().chain(widths) {
        let viewport = Size::new(width, 600.0);
        let revision = h.text.layouts().changes();
        h.tree.prepare(viewport, &mut h.text);
        changes.push(h.text.layouts().changes() - revision);
        let mut fresh = Harness::new();
        let fresh_labels = quarter_labels(&mut fresh);
        fresh.tree.prepare(viewport, &mut fresh.text);
        assert_eq!(
            shown(&h, &labels),
            shown(&fresh, &fresh_labels),
            "{width} px"
        );
    }
    // Only the text shown at the new width is shaped (a create and a remove per label).
    let again = &changes[widths.len()..];
    let most = 2 * labels.len() as u64;
    assert!(again.iter().all(|&c| c <= most), "{changes:?}");
}

#[test]
fn truncated_labels_are_not_reshaped_when_laid_out_again_at_a_known_width() {
    let mut h = Harness::new();
    let quarter = Style {
        size: LayoutSize {
            width: percent(0.25),
            height: noa_gpui_core::style::auto(),
        },
        ..Default::default()
    };
    let labels: Vec<WidgetId> = [
        Label::new(LONG).truncation(Truncation::Tail),
        Label::new(LONG).truncation(Truncation::Middle),
        Label::new(LONG).truncation(Truncation::Head),
        Label::new(LONG).max_lines(2),
    ]
    .into_iter()
    .map(|label| {
        let parent = h.add(Container::new(quarter.clone()));
        h.add_to(parent, label)
    })
    .collect();
    let shown = |h: &Harness| -> Vec<String> {
        let label = |&id: &WidgetId| h.tree.get::<Label>(id).unwrap().shown_text().to_owned();
        labels.iter().map(label).collect()
    };
    let wide = shown(&h);
    let narrow_viewport = Size::new(560.0, 600.0);
    h.tree.prepare(narrow_viewport, &mut h.text);
    let narrow = shown(&h);
    assert_ne!(wide, narrow, "a narrower window cuts more");

    // Both widths seen: going back and forth only shapes the text shown at the new
    // width, once per label (a create and a remove), not the candidates to cut it to.
    let revision = h.text.layouts().changes();
    let passes = 6;
    for _ in 0..passes / 2 {
        h.prepare();
        assert_eq!(shown(&h), wide);
        h.tree.prepare(narrow_viewport, &mut h.text);
        assert_eq!(shown(&h), narrow);
    }
    let changes = h.text.layouts().changes() - revision;
    let most = (2 * labels.len() * passes) as u64;
    assert!(changes <= most, "{changes} layouts made or dropped");

    h.tree.update::<Label, _>(labels[0], |label, cx| {
        label.set_text(cx, "A different and also rather long text to cut")
    });
    h.tree.prepare(narrow_viewport, &mut h.text);
    let parent = h.add(Container::new(quarter));
    let fresh = h.add_to(
        parent,
        Label::new("A different and also rather long text to cut").truncation(Truncation::Tail),
    );
    h.tree.prepare(narrow_viewport, &mut h.text);
    let updated = h.tree.get::<Label>(labels[0]).unwrap().shown_text();
    assert!(
        updated.starts_with("A different") && updated.ends_with(ELLIPSIS),
        "{updated}"
    );
    assert_eq!(updated, h.tree.get::<Label>(fresh).unwrap().shown_text());
}

#[test]
fn selectable_labels_select_and_copy_but_do_not_edit() {
    let mut h = Harness::new();
    let label = h.add(Label::new("Copy this text").selectable(true));
    let word = h.at(label, 10.0, 8.0);
    h.press(word);
    h.press(word);
    assert_eq!(h.tree.get::<Label>(label).unwrap().selection(), Some(0..4));
    h.command('c');
    assert_eq!(h.clipboard().as_deref(), Some("Copy"));
    h.type_text("x");
    h.key(Key::Backspace);
    h.command('a');
    h.command('c');
    assert_eq!(h.clipboard().as_deref(), Some("Copy this text"));
    assert_eq!(h.tree.get::<Label>(label).unwrap().text(), "Copy this text");
    assert!(values::<TextChanged>(&h.messages()).is_empty());
}

#[test]
fn text_fields_cut_copy_paste_undo_and_submit() {
    let mut h = Harness::new();
    let field = h.add(TextField::new("", 200.0));
    h.focus(field);
    h.type_text("hello world");
    h.command('a');
    h.command('x');
    assert_eq!(h.tree.get::<TextField>(field).unwrap().text(), "");
    h.command('v');
    h.command('v');
    assert_eq!(
        h.tree.get::<TextField>(field).unwrap().text(),
        "hello worldhello world"
    );
    h.command('z');
    assert_eq!(
        h.tree.get::<TextField>(field).unwrap().text(),
        "hello world"
    );
    h.messages();
    h.key(Key::Enter);
    assert_eq!(
        values::<TextSubmitted>(&h.messages()),
        [TextSubmitted("hello world".into())]
    );
}

#[test]
fn read_only_fields_select_and_copy_but_refuse_edits_and_the_ime() {
    let mut h = Harness::new();
    let field = h.add(TextField::new("", 200.0).read_only(true));
    h.tree
        .update::<TextField, _>(field, |f, cx| f.set_text(cx, "fixed value"));
    h.prepare();
    h.focus(field);
    assert_eq!(h.tree.text_input_area(), None);
    h.set_clipboard("pasted");
    h.type_text("x");
    h.compose("か");
    h.key(Key::Backspace);
    h.command('v');
    h.command('a');
    h.command('x');
    assert_eq!(
        h.tree.get::<TextField>(field).unwrap().text(),
        "fixed value"
    );
    assert_eq!(h.tree.get::<TextField>(field).unwrap().composition(), "");
    h.command('c');
    assert_eq!(h.clipboard().as_deref(), Some("fixed value"));
    assert!(values::<TextChanged>(&h.messages()).is_empty());
}

#[test]
fn disabled_fields_take_no_focus_or_input() {
    let mut h = Harness::new();
    let field = h.add(TextField::new("", 200.0));
    h.tree.set_enabled(field, false);
    h.press(h.at(field, 20.0, 10.0));
    assert_ne!(h.tree.focused(), Some(field));
    h.type_text("x");
    assert_eq!(h.tree.get::<TextField>(field).unwrap().text(), "");
}

#[test]
fn secure_fields_hide_the_text_and_refuse_copying() {
    let mut h = Harness::new();
    let field = h.add(SecureTextField::new("Password", 200.0));
    h.focus(field);
    assert_eq!(h.tree.text_input_area(), None);
    h.type_text("pässwörd");
    assert_eq!(
        h.tree.get::<SecureTextField>(field).unwrap().text(),
        "pässwörd"
    );
    assert_eq!(h.access_value(field).as_deref(), Some("••••••••"));
    h.set_clipboard("clipboard");
    h.command('a');
    h.command('c');
    h.command('x');
    assert_eq!(h.clipboard().as_deref(), Some("clipboard"));
    assert_eq!(
        h.tree.get::<SecureTextField>(field).unwrap().text(),
        "pässwörd"
    );
    h.command('v');
    assert_eq!(
        h.tree.get::<SecureTextField>(field).unwrap().text(),
        "clipboard"
    );
    h.key(Key::Backspace);
    assert_eq!(
        h.tree.get::<SecureTextField>(field).unwrap().text(),
        "clipboar"
    );
    // Word movement goes to the ends instead of revealing word breaks.
    h.type_text(" two words");
    h.key_with(
        Key::ArrowLeft,
        Modifiers {
            alt: true,
            ..Default::default()
        },
    );
    h.type_text("^");
    assert_eq!(
        h.tree.get::<SecureTextField>(field).unwrap().text(),
        "^clipboar two words"
    );
    h.command('a');
    h.key(Key::Backspace);
    h.type_text("clipboar");
    // Clicking places the caret by bullets: the first bullet's right half is index 1.
    h.press(h.at(field, 10.0 + 6.0, 15.0));
    h.type_text("X");
    assert_eq!(
        h.tree.get::<SecureTextField>(field).unwrap().text(),
        "cXlipboar"
    );
}

#[test]
fn search_fields_start_with_saved_recents_and_keep_adding_to_them() {
    let mut h = Harness::new();
    let saved = ["gpu", "", "text", "gpu"];
    let search = h.add(SearchField::new("Search", 240.0).with_recents(saved));
    let recents = |h: &Harness| {
        h.tree
            .get::<SearchField>(search)
            .unwrap()
            .recents()
            .to_vec()
    };
    assert_eq!(
        recents(&h),
        ["gpu", "text"],
        "empty and repeated ones dropped"
    );
    h.focus(search);
    h.type_text("layout");
    h.key(Key::Enter);
    assert_eq!(recents(&h), ["layout", "gpu", "text"]);
    // Replaced, at most ten.
    h.tree.update::<SearchField, _>(search, |field, cx| {
        field.set_recents(cx, (0..15).map(|i| format!("q{i}")))
    });
    assert_eq!(recents(&h).len(), 10);
    assert_eq!(recents(&h)[0], "q0");
}

#[test]
fn search_fields_submit_remember_and_clear() {
    let mut h = Harness::new();
    let search = h.add(SearchField::new("Search", 240.0));
    h.focus(search);
    h.type_text("gpu");
    assert_eq!(
        values::<TextChanged>(&h.messages()),
        [TextChanged("gpu".into())]
    );
    h.key(Key::Enter);
    assert_eq!(
        values::<SearchSubmitted>(&h.messages()),
        [SearchSubmitted("gpu".into())]
    );
    h.key(Key::Escape);
    assert_eq!(h.tree.get::<SearchField>(search).unwrap().text(), "");
    h.type_text("text");
    h.key(Key::Enter);
    assert_eq!(
        h.tree.get::<SearchField>(search).unwrap().recents(),
        ["text", "gpu"]
    );
    let r = h.tree.window_rect(search).unwrap();
    h.press(Point::new(r.right() - 14.0, r.y + r.height / 2.0));
    assert_eq!(h.tree.get::<SearchField>(search).unwrap().text(), "");
    h.messages();

    h.press(h.at(search, 14.0, r.height / 2.0));
    let menu = h.tree.overlays().last().expect("recents menu");
    let labels: Vec<String> = h
        .tree
        .get::<Menu>(menu)
        .unwrap()
        .items()
        .iter()
        .map(|i| i.label().to_owned())
        .collect();
    assert_eq!(
        labels,
        [
            "Recent Searches",
            "text",
            "gpu",
            "",
            "Clear Recent Searches"
        ]
    );
    h.key(Key::ArrowDown);
    h.key(Key::ArrowDown);
    h.key(Key::Enter);
    assert_eq!(h.tree.get::<SearchField>(search).unwrap().text(), "gpu");
    assert_eq!(
        values::<SearchSubmitted>(&h.messages()),
        [SearchSubmitted("gpu".into())]
    );
}

#[test]
fn token_fields_make_tokens_and_delete_them_from_the_keyboard() {
    let mut h = Harness::new();
    let field = h.add(TokenField::new("Tags", 300.0));
    h.focus(field);
    let tokens = |h: &Harness| h.tree.get::<TokenField>(field).unwrap().tokens().to_vec();
    h.type_text("rust,");
    h.type_text("gpu");
    h.key(Key::Enter);
    h.type_text("ui, text");
    h.key(Key::Enter);
    assert_eq!(tokens(&h), ["rust", "gpu", "ui", "text"]);
    let changes = values::<TokensChanged>(&h.messages());
    assert_eq!(changes.len(), 3);

    h.key(Key::Backspace);
    assert_eq!(h.tree.get::<TokenField>(field).unwrap().selected(), Some(3));
    h.key(Key::Backspace);
    assert_eq!(tokens(&h), ["rust", "gpu", "ui"]);
    h.key(Key::ArrowLeft);
    h.key(Key::Delete);
    assert_eq!(tokens(&h), ["rust", "ui"]);
    h.type_text("x");
    assert_eq!(h.tree.get::<TokenField>(field).unwrap().selected(), None);
    assert_eq!(h.tree.get::<TokenField>(field).unwrap().input(), "x");
}

#[test]
fn token_fields_wrap_and_select_tokens_by_click() {
    let mut h = Harness::new();
    let field = h.add(
        TokenField::new("", 200.0).with_tokens(["alpha", "beta", "gamma", "delta", "epsilon"]),
    );
    let one_row = {
        let short = h.add(TokenField::new("", 200.0).with_tokens(["a"]));
        h.tree.window_rect(short).unwrap().height
    };
    assert!(h.tree.window_rect(field).unwrap().height > one_row);
    h.press(h.at(field, 12.0, 14.0));
    assert_eq!(h.tree.focused(), Some(field));
    assert_eq!(h.tree.get::<TokenField>(field).unwrap().selected(), Some(0));
}

#[test]
fn combo_boxes_complete_typing_and_choose_from_the_list() {
    let mut h = Harness::new();
    let combo = h.add(ComboBox::new(["Apple", "Apricot", "Banana"], 200.0));
    h.focus(combo);
    let state = |h: &Harness| {
        let c = h.tree.get::<ComboBox>(combo).unwrap();
        (c.text().to_owned(), c.selection())
    };
    h.type_text("a");
    h.type_text("p");
    assert_eq!(state(&h), ("apple".into(), 2..5));
    h.type_text("r");
    assert_eq!(state(&h), ("apricot".into(), 3..7));
    h.key(Key::Backspace);
    assert_eq!(state(&h), ("apr".into(), 3..3));
    h.messages();

    h.key(Key::ArrowDown);
    assert!(h.tree.overlays().last().is_some());
    h.key(Key::ArrowDown);
    h.key(Key::ArrowDown);
    h.key(Key::ArrowDown);
    h.key(Key::Enter);
    let messages = h.messages();
    assert_eq!(values::<SelectionChanged>(&messages), [SelectionChanged(2)]);
    assert_eq!(h.tree.get::<ComboBox>(combo).unwrap().text(), "Banana");
    assert_eq!(h.tree.focused(), Some(combo));
}

#[test]
fn text_views_follow_the_caret_and_scroll_with_the_wheel() {
    let mut h = Harness::new();
    let view = h.add(TextView::new(100.0).width(300.0));
    h.focus(view);
    for i in 0..12 {
        h.type_text(&format!("line {i}"));
        h.key(Key::Enter);
    }
    let scroll = |h: &Harness| h.tree.get::<TextView>(view).unwrap().scroll_y();
    let bottom = scroll(&h);
    assert!(bottom > 100.0, "{bottom}");
    assert_eq!(h.tree.window_rect(view).unwrap().height, 100.0);
    let pos = h.at(view, 50.0, 50.0);
    h.tree.dispatch(
        Event::Wheel {
            pos,
            delta: Point::new(0.0, 1000.0),
        },
        &mut h.text,
    );
    assert_eq!(scroll(&h), 0.0);
    h.type_text("!");
    assert_eq!(scroll(&h), bottom);
    for _ in 0..12 {
        h.key(Key::ArrowUp);
    }
    assert_eq!(scroll(&h), 0.0);
}

#[test]
fn cancelling_composition_preserves_the_selected_text_and_selection() {
    for reverse in [false, true] {
        let mut h = Harness::new();
        let field = h.add(TextField::new("", 200.0));
        let original = "A東京Z";
        let selection = 1..original.len() - 1;
        h.tree.update::<TextField, _>(field, |f, cx| {
            f.set_text(cx, original);
            if reverse {
                f.select(cx, selection.end..selection.start);
            } else {
                f.select(cx, selection.clone());
            }
        });
        h.focus(field);
        h.prepare();
        h.messages();
        h.compose("日本");
        assert_eq!(h.tree.get::<TextField>(field).unwrap().text(), original);
        assert_eq!(h.access_value(field).as_deref(), Some(original));
        assert!(values::<TextChanged>(&h.messages()).is_empty());
        assert!(h.tree.display_list().commands().iter().any(|command| {
            matches!(command, DrawCommand::Text { layout, .. }
                if h.text.layouts().buffer(*layout).lines[0].text() == "A日本Z")
        }));

        h.compose("");
        let input = h.tree.get::<TextField>(field).unwrap();
        assert_eq!(input.text(), original);
        assert_eq!(input.selection(), selection);
        assert_eq!(input.caret(), if reverse { 1 } else { original.len() - 1 });
        assert!(values::<TextChanged>(&h.messages()).is_empty());
    }
}

#[test]
fn committing_over_a_selection_is_one_change_and_one_undo_step() {
    for clear_before_commit in [false, true] {
        let mut h = Harness::new();
        let field = h.add(TextField::new("", 200.0));
        h.tree.update::<TextField, _>(field, |f, cx| {
            f.set_text(cx, "A東京Z");
            f.select(cx, 1..7);
        });
        h.focus(field);
        h.prepare();
        h.messages();
        h.compose("にほん");
        h.compose("日本");
        if clear_before_commit {
            // winit may send an empty preedit immediately before the commit.
            h.compose("");
        }
        h.type_text("日本");
        assert_eq!(h.tree.get::<TextField>(field).unwrap().text(), "A日本Z");
        assert_eq!(
            values::<TextChanged>(&h.messages()),
            [TextChanged("A日本Z".into())]
        );
        h.command('z');
        let input = h.tree.get::<TextField>(field).unwrap();
        assert_eq!(input.text(), "A東京Z");
        assert_eq!(input.selection(), 1..7);
        h.key_with(
            Key::Char('z'),
            Modifiers {
                command: true,
                meta: true,
                shift: true,
                ..Default::default()
            },
        );
        assert_eq!(h.tree.get::<TextField>(field).unwrap().text(), "A日本Z");
    }
}

#[test]
fn losing_focus_cancels_composition_in_every_text_input() {
    let mut h = Harness::new();
    let field = h.add(TextField::new("", 200.0));
    let search = h.add(SearchField::new("", 200.0));
    let tokens = h.add(TokenField::new("", 200.0).completions(["unrelated"]));
    let combo = h.add(
        ComboBox::new(["unrelated"], 200.0)
            .completion_list(true)
            .completes(false),
    );
    let view = h.add(TextView::new(80.0).width(200.0));
    let area = h.add(TextArea::new("").width(200.0));
    let other = h.add(TextField::new("", 200.0));
    for input in [field, search, tokens, combo, view, area] {
        h.focus(input);
        h.type_text("ab");
        h.compose("仮");
        h.messages();
        h.focus(other);
        h.prepare();
        assert_eq!(h.access_value(input).as_deref(), Some("ab"));
        assert!(values::<TextChanged>(&h.messages()).is_empty());
        h.focus(input);
        h.key(Key::Home);
        h.type_text("z");
        assert_eq!(h.access_value(input).as_deref(), Some("zab"), "{input:?}");
    }
}

#[test]
fn losing_focus_cancels_composition_before_a_number_input_commits() {
    use noa_gpui_core::widgets::NumberInput;

    let mut h = Harness::new();
    let number = h.add(NumberInput::new(0.0, 100.0, 12.0));
    let other = h.add(TextField::new("", 200.0));
    h.focus(number);
    h.prepare();
    h.compose("仮");
    h.focus(other);
    h.prepare();
    assert_eq!(h.tree.get::<NumberInput>(number).unwrap().text(), "12");
    assert_eq!(h.tree.get::<NumberInput>(number).unwrap().value(), 12.0);
    h.focus(number);
    h.prepare();
    h.key(Key::ArrowRight);
    h.key(Key::Backspace);
    assert_eq!(h.tree.get::<NumberInput>(number).unwrap().text(), "1");
}

#[test]
fn inserting_joiners_keeps_the_caret_on_a_grapheme_boundary() {
    use noa_gpui_core::text_edit::TextEditor;

    for (original, inserted, combined) in [
        ("\u{301}", "a", "a\u{301}"),
        ("👩", "👨\u{200d}", "👨\u{200d}👩"),
        ("🇵", "🇯", "🇯🇵"),
    ] {
        let mut editor = TextEditor::single_line();
        editor.set_text(original);
        editor.move_to(0, false);
        assert!(editor.insert(inserted));
        assert_eq!(editor.text(), combined);
        assert_eq!(editor.caret(), combined.len());
        assert!(editor.backspace(false));
        assert_eq!(editor.text(), "", "the joined grapheme is deleted together");
        assert!(editor.undo());
        assert_eq!(editor.text(), combined);
        assert_eq!(editor.caret(), combined.len());
    }
}

#[test]
fn removing_a_separator_cannot_leave_the_caret_inside_a_grapheme() {
    use noa_gpui_core::text_edit::TextEditor;

    for action in [0, 1, 2] {
        let mut editor = TextEditor::multi_line();
        editor.set_text("a\n\u{301}");
        match action {
            0 => {
                editor.select(1..2);
                assert!(editor.insert(""));
            }
            1 => {
                editor.move_to(2, false);
                assert!(editor.backspace(false));
            }
            _ => {
                editor.move_to(1, false);
                assert!(editor.delete_forward(false));
            }
        }
        assert_eq!(editor.text(), "a\u{301}");
        assert_eq!(editor.caret(), "a\u{301}".len());
        assert_eq!(editor.selection(), editor.caret()..editor.caret());
        assert!(editor.backspace(false));
        assert_eq!(editor.text(), "");
    }
}

#[test]
fn the_ime_composes_in_every_new_input() {
    let mut h = Harness::new();
    let search = h.add(SearchField::new("", 200.0));
    let tokens = h.add(TokenField::new("", 200.0));
    let combo = h.add(ComboBox::new(["東京"], 200.0).completes(false));
    let view = h.add(TextView::new(80.0).width(200.0));
    for id in [search, tokens, combo, view] {
        h.focus(id);
        assert!(h.tree.text_input_area().is_some(), "{id:?} enables the IME");
        h.compose("にほん");
        h.compose("日本");
        h.type_text("日本");
    }
    assert_eq!(h.tree.get::<SearchField>(search).unwrap().text(), "日本");
    assert_eq!(h.tree.get::<TokenField>(tokens).unwrap().input(), "日本");
    assert_eq!(h.tree.get::<ComboBox>(combo).unwrap().text(), "日本");
    assert_eq!(h.tree.get::<TextView>(view).unwrap().text(), "日本");
    // While composing, Return belongs to the IME: no token and no submission.
    h.focus(tokens);
    h.compose("か");
    assert_eq!(
        h.tree.get::<TokenField>(tokens).unwrap().composition(),
        "か"
    );
    h.key(Key::Enter);
    assert_eq!(h.tree.get::<TokenField>(tokens).unwrap().tokens().len(), 0);
}

#[test]
fn accessibility_roles_of_the_new_inputs() {
    let mut h = Harness::new();
    let ids = [
        (
            h.add(SecureTextField::new("", 100.0)),
            accesskit::Role::PasswordInput,
        ),
        (
            h.add(SearchField::new("", 100.0)),
            accesskit::Role::SearchInput,
        ),
        (
            h.add(ComboBox::new(["a"], 100.0)),
            accesskit::Role::ComboBox,
        ),
        (
            h.add(TextView::new(60.0)),
            accesskit::Role::MultilineTextInput,
        ),
    ];
    let update = h.tree.accessibility_update(1.0);
    for (id, role) in ids {
        let node = update
            .nodes
            .iter()
            .find(|(n, _)| *n == access_id(id))
            .unwrap();
        assert_eq!(node.1.role(), role);
    }
}

fn log_lines(n: usize) -> Vec<String> {
    (0..n)
        .map(|i| format!("line {i}: GET /health 200"))
        .collect()
}

fn log(h: &Harness, id: WidgetId) -> &LogView {
    h.tree.get::<LogView>(id).unwrap()
}

fn drawn_lines(h: &Harness) -> Vec<(String, Color)> {
    (h.tree.display_list().commands().iter())
        .filter_map(|c| match c {
            DrawCommand::Text { layout, color, .. } => {
                let line = &h.text.layouts().buffer(*layout).lines[0];
                Some((line.text().to_owned(), *color))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_log_view_lays_out_only_the_lines_on_screen_and_follows_the_tail() {
    let mut h = Harness::new();
    let id = h.add(LogView::new().height(242.0).lines(log_lines(100_000)));
    h.prepare();
    let lh = log(&h, id).line_height();
    let visible = log(&h, id).visible_lines();
    assert_eq!(visible.end, 100_000);
    let drawn = drawn_lines(&h);
    assert!(drawn.len() <= (240.0 / lh).ceil() as usize + 1);
    assert_eq!(drawn.last().unwrap().0, "line 99999: GET /health 200");

    h.tree
        .update::<LogView, _>(id, |l, cx| l.push(cx, "the newest"));
    h.prepare();
    assert_eq!(drawn_lines(&h).last().unwrap().0, "the newest");

    h.focus(id);
    h.key(Key::Home);
    h.tree
        .update::<LogView, _>(id, |l, cx| l.push(cx, "newer still"));
    h.prepare();
    assert_eq!(log(&h, id).scroll_y(), 0.0);
    assert_eq!(drawn_lines(&h)[0].0, "line 0: GET /health 200");
    h.key(Key::End);
    assert_eq!(drawn_lines(&h).last().unwrap().0, "newer still");
}

#[test]
fn a_log_view_keeps_at_most_its_line_limit() {
    let mut h = Harness::new();
    let id = h.add(LogView::new().max_lines(3).lines(["a", "b", "c", "d"]));
    h.tree
        .update::<LogView, _>(id, |l, cx| l.extend(cx, ["e", "f"]));
    h.prepare();
    assert_eq!(log(&h, id).line_count(), 3);
    assert_eq!(log(&h, id).all_text(), "d\ne\nf");
}

#[test]
fn lines_have_their_own_colors() {
    let mut h = Harness::new();
    let red = Color::hex(0xff3b30);
    let id = h.add(LogView::new().lines([
        LogLine::new("ok"),
        LogLine::new("error: no such container").color(red),
    ]));
    h.prepare();
    let drawn = drawn_lines(&h);
    assert_eq!(drawn[1], ("error: no such container".to_owned(), red));
    assert_ne!(drawn[0].1, red);
    assert_eq!(
        log(&h, id).line(1).unwrap().text(),
        "error: no such container"
    );
}

#[test]
fn searching_a_log_highlights_the_matches_and_steps_through_them() {
    let mut h = Harness::new();
    let mut lines = log_lines(1_000);
    lines[10] = "ERROR one".to_owned();
    lines[500] = "error two, error three".to_owned();
    let id = h.add(
        LogView::new()
            .height(242.0)
            .follows_tail(false)
            .lines(lines),
    );
    h.prepare();
    let find = |h: &mut Harness, action: FindAction| {
        let status = h.tree.update::<LogView, _>(id, |l, cx| l.find(cx, &action));
        h.prepare();
        status.unwrap()
    };
    let status = find(
        &mut h,
        FindAction::Search {
            query: "error".into(),
            options: FindOptions::default(),
        },
    );
    assert_eq!((status.total, status.current), (3, Some(0)));
    let found: Vec<_> = (log(&h, id).found().iter())
        .map(|m| (m.line, m.range.clone()))
        .collect();
    assert_eq!(found, [(10, 0..5), (500, 0..5), (500, 11..16)]);
    // The matches on screen are highlighted (one, the current one, here).
    let highlights = |h: &Harness| {
        (h.tree.display_list().commands().iter())
            .filter(|c| matches!(c, DrawCommand::Quad { color, .. } if color.b < 0.3 && color.r > 0.9 && color.g > 0.7))
            .count()
    };
    assert_eq!(highlights(&h), 1);

    // Next scrolls to line 500, with both its matches highlighted.
    let status = find(&mut h, FindAction::Next);
    assert_eq!(status.current, Some(1));
    assert!(log(&h, id).visible_lines().contains(&500));
    assert_eq!(highlights(&h), 2);
    find(&mut h, FindAction::Previous);
    let status = find(&mut h, FindAction::Previous);
    assert_eq!(status.current, Some(2));

    h.tree
        .update::<LogView, _>(id, |l, cx| l.push(cx, "late error"));
    assert_eq!(log(&h, id).found().len(), 4);

    let status = find(&mut h, FindAction::Close);
    assert_eq!(status.total, 0);
    assert_eq!(highlights(&h), 0);

    let status = find(
        &mut h,
        FindAction::Search {
            query: "(".into(),
            options: FindOptions {
                regex: true,
                ..Default::default()
            },
        },
    );
    assert!(status.error.is_some());
}

#[test]
fn log_lines_are_selected_by_clicking_and_copied() {
    let mut h = Harness::new();
    let id = h.add(LogView::new().follows_tail(false).lines(log_lines(20)));
    h.prepare();
    let lh = log(&h, id).line_height();
    let line = |h: &Harness, i: usize| h.at(id, 40.0, 1.0 + i as f32 * lh + lh / 2.0);
    h.press(line(&h, 2));
    assert_eq!(log(&h, id).selected_lines(), Some(2..3));
    h.key_with(
        Key::ArrowDown,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(log(&h, id).selected_lines(), Some(2..4));
    h.command('c');
    assert_eq!(
        h.clipboard().as_deref(),
        Some("line 2: GET /health 200\nline 3: GET /health 200")
    );
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    h.tree.dispatch(Event::ModifiersChanged(shift), &mut h.text);
    h.press(line(&h, 5));
    h.tree
        .dispatch(Event::ModifiersChanged(Modifiers::default()), &mut h.text);
    assert_eq!(log(&h, id).selected_lines(), Some(2..6));
    h.command('a');
    assert_eq!(log(&h, id).selected_lines(), Some(0..20));
    assert_eq!(log(&h, id).copy_text().unwrap(), log(&h, id).all_text());
}

#[test]
fn a_log_view_glides_after_a_flick_unless_momentum_is_off() {
    let mut h = Harness::new();
    let gliding = h.add(LogView::new().follows_tail(false).lines(log_lines(1_000)));
    let still = h.add(
        LogView::new()
            .follows_tail(false)
            .momentum(false)
            .lines(log_lines(1_000)),
    );
    // A quick flick on each: 30 px per 1/60 s.
    for id in [gliding, still] {
        let at = h.at(id, 40.0, 40.0);
        for i in 0..3 {
            h.tree.set_now(1.0 + f64::from(i) / 60.0);
            h.tree.dispatch(
                Event::Wheel {
                    pos: at,
                    delta: Point::new(0.0, -30.0),
                },
                &mut h.text,
            );
            h.prepare();
        }
        assert_eq!(log(&h, id).scroll_y(), 90.0);
    }
    for i in 1..=60 {
        h.tree
            .animate(1.0 + 2.0 / 60.0 + f64::from(i) / 60.0, &mut h.text);
        h.prepare();
    }
    assert!(
        log(&h, gliding).scroll_y() > 150.0,
        "{}",
        log(&h, gliding).scroll_y()
    );
    assert_eq!(log(&h, still).scroll_y(), 90.0);
}

#[test]
fn a_search_field_can_have_the_corners_of_a_text_field() {
    let mut h = Harness::new();
    let capsule = h.add(SearchField::new("Search", 200.0));
    let square = h.add(SearchField::new("Search", 200.0).capsule(false));
    let field = h.add(TextField::new("Name", 200.0));
    h.prepare();
    let radius = |h: &Harness, id: WidgetId| {
        let bounds = h.tree.window_rect(id).unwrap();
        (h.tree.display_list().commands().iter())
            .find_map(|c| match c {
                DrawCommand::Quad { rect, radius, .. } if *rect == bounds => Some(*radius),
                _ => None,
            })
            .unwrap()
    };
    let height = h.tree.window_rect(capsule).unwrap().height;
    assert_eq!(radius(&h, capsule), [height / 2.0; 4]);
    assert_eq!(radius(&h, square), radius(&h, field));
    assert_ne!(radius(&h, square), radius(&h, capsule));
    h.focus(square);
    h.type_text("gpu");
    let r = h.tree.window_rect(square).unwrap();
    h.press(Point::new(r.right() - 14.0, r.y + r.height / 2.0));
    assert_eq!(h.tree.get::<SearchField>(square).unwrap().text(), "");
}

fn drawn_text(tree: &UiTree, text: &TextStore) -> Vec<String> {
    let commands = tree.display_list().commands().iter();
    commands
        .filter_map(|c| match c {
            DrawCommand::Text { rect, layout, .. } => Some((rect, layout)),
            _ => None,
        })
        .map(|(rect, layout)| {
            let runs: Vec<String> = text
                .layouts()
                .buffer(*layout)
                .layout_runs()
                .map(|run| format!("{} {} {:?}", run.line_top, run.line_w, run.glyphs))
                .collect();
            format!("{rect:?} {:?} {}", text.size(*layout), runs.join("\n"))
        })
        .collect()
}

#[test]
fn wrapping_labels_in_a_resized_window_draw_as_in_a_new_one() {
    // A layout pass measures a wrapping label at several widths and keeps its glyphs
    // between them: whatever widths came before, the lines are those of a new label.
    const PARAGRAPH: &str = "Resizing a window wraps its paragraphs again and again, \
        each time at a width nothing was laid out for before.\nA second paragraph.";
    let build = || {
        let mut h = Harness::new();
        for align in [TextAlign::Start, TextAlign::Center, TextAlign::End] {
            let column = h.tree.insert(
                Some(h.root),
                Container::new(Style {
                    flex_direction: FlexDirection::Column,
                    size: LayoutSize {
                        width: percent(0.5),
                        height: noa_gpui_core::style::auto(),
                    },
                    ..Default::default()
                }),
            );
            h.tree
                .insert(Some(column), Label::new(PARAGRAPH).wrap(true).align(align));
        }
        h
    };
    let mut resized = build();
    for width in [800.0, 310.0, 1000.0, 120.0, 4000.0, 311.0, 800.0] {
        // Tall enough that no label is culled, however narrow.
        let viewport = Size::new(width, 6000.0);
        resized.tree.prepare(viewport, &mut resized.text);
        let mut new = build();
        new.tree.prepare(viewport, &mut new.text);
        let drawn = drawn_text(&resized.tree, &resized.text);
        assert_eq!(drawn.len(), 3, "three labels at {width}");
        assert_eq!(drawn, drawn_text(&new.tree, &new.text), "at {width}");
        assert_eq!(resized.text.layouts().len(), new.text.layouts().len());
    }
}

#[test]
fn text_fields_capture_the_reading_of_converted_text() {
    let mut h = Harness::new();
    let field = h.add(TextField::new("", 200.0).capture_reading(true));
    h.focus(field);
    h.prepare();
    // Romaji typing shows a trailing letter before the kana appear.
    for composition in ["y", "や", "やm", "やま", "やまだ", "山田", ""] {
        h.compose(composition);
    }
    h.type_text("山田");
    // Kana committed as is are their own reading; ASCII typed directly has none.
    h.compose("たろう");
    h.type_text("たろう");
    h.type_text("A");
    let input = h.tree.get::<TextField>(field).unwrap();
    assert_eq!(input.text(), "山田たろうA");
    assert_eq!(input.reading(), "やまだたろう");
    assert_eq!(
        values::<ReadingChanged>(&h.messages()),
        [
            ReadingChanged("やまだ".into()),
            ReadingChanged("やまだたろう".into()),
        ]
    );

    // A cancelled composition is not the reading of what is typed next.
    h.compose("かな");
    h.compose("");
    h.key(Key::ArrowLeft);
    h.type_text("x");
    assert_eq!(
        h.tree.get::<TextField>(field).unwrap().reading(),
        "やまだたろう"
    );

    h.command('a');
    h.key(Key::Backspace);
    assert_eq!(h.tree.get::<TextField>(field).unwrap().reading(), "");
    assert_eq!(
        values::<ReadingChanged>(&h.messages()),
        [ReadingChanged(String::new())]
    );

    // A name composed with a space between family and given name keeps both readings.
    h.command('a');
    h.key(Key::Backspace);
    for composition in ["やまだ", "やまだ　たろう", "山田　太郎", ""] {
        h.compose(composition);
    }
    h.type_text("山田　太郎");
    assert_eq!(
        h.tree.get::<TextField>(field).unwrap().reading(),
        "やまだ　たろう"
    );

    h.compose("すずき");
    h.compose("鈴木");
    h.type_text("鈴木");
    h.tree
        .update::<TextField, _>(field, |f, cx| f.set_text(cx, "佐藤"));
    assert_eq!(h.tree.get::<TextField>(field).unwrap().reading(), "");
}

#[test]
fn text_fields_capture_no_reading_unless_asked() {
    let mut h = Harness::new();
    let field = h.add(TextField::new("", 200.0));
    h.focus(field);
    h.compose("やまだ");
    h.compose("山田");
    h.type_text("山田");
    assert_eq!(h.tree.get::<TextField>(field).unwrap().reading(), "");
    assert!(values::<ReadingChanged>(&h.messages()).is_empty());
}

#[test]
fn kana_helpers_convert_scripts_and_normalize_widths() {
    assert!(kana::is_kana("やまだ"));
    assert!(kana::is_kana("ヤマダ・タロー"));
    assert!(!kana::is_kana(""));
    assert!(!kana::is_kana("やm"));
    assert!(!kana::is_kana("山田"));
    assert_eq!(kana::to_katakana("やまだ ゝ ー"), "ヤマダ ヽ ー");
    assert_eq!(kana::to_hiragana("ヤマダ・ヴ ヽ"), "やまだ・ゔ ゝ");
    assert_eq!(
        kana::normalize_width("ＡＢＣ１２３－ｶﾞｯｺｳ ﾊﾟﾝ ﾞ　山田"),
        "ABC123-ガッコウ パン ゛　山田"
    );
    // Normalizing after each keystroke merges a mark typed after its letter.
    assert_eq!(kana::normalize_width("カﾞ"), "ガ");
    assert_eq!(kana::normalize_width("ﾍﾟ"), "ペ");
    let typed = ["ﾊ", "ﾟ", "ﾝ"].iter().fold(String::new(), |text, key| {
        kana::normalize_width(&(text + key))
    });
    assert_eq!(typed, "パン");
}

/// What an input method sees of a field: its text, selection and characters in window
/// coordinates, with a composition in place of the selection.
#[test]
fn the_input_method_sees_the_selection_and_where_each_character_is() {
    let mut h = Harness::new();
    let field = h.add(TextField::new("", 240.0));
    h.focus(field);
    h.type_text("alpha beta");
    // Select "beta" from the end.
    for _ in 0..4 {
        h.key_with(
            Key::ArrowLeft,
            Modifiers {
                shift: true,
                ..Default::default()
            },
        );
    }
    let context = h
        .tree
        .ime_context(&h.text)
        .expect("a focused field answers");
    assert_eq!(context.text, "alpha beta");
    assert_eq!((context.base, context.selection.clone()), (0, 6..10));
    assert_eq!(context.composition, None);
    // The selected characters are inside the field, left to right on one line.
    let field_rect = h.tree.window_rect(field).unwrap();
    let (beta, _) = context.first_rect(6..10);
    assert!(
        field_rect.contains(Point::new(beta.x, beta.y + 1.0)),
        "{beta:?}"
    );
    assert!(beta.width > 10.0);
    let inside = Point::new(beta.x + 1.0, beta.y + beta.height / 2.0);
    assert_eq!(context.index_at(inside), Some(6));

    // A composition takes the selection's place.
    h.compose("べーた");
    let context = h.tree.ime_context(&h.text).unwrap();
    assert_eq!(context.text, "alpha べーた");
    assert_eq!(context.composition, Some(6..15));
    assert_eq!(context.selection, 15..15);
    assert_eq!(context.first_rect(6..15).0.x, beta.x);

    // Secure fields tell the input method nothing.
    let secure = h.add(SecureTextField::new("Password", 200.0));
    h.focus(secure);
    assert_eq!(h.tree.ime_context(&h.text), None);
}

/// Reconversion: the input method selects the text it converts again, composes over
/// it and commits the result in its place.
#[test]
fn the_input_method_can_select_text_to_convert_it_again() {
    let mut h = Harness::new();
    let area = h.add(TextArea::new(""));
    h.focus(area);
    h.type_text("first line\nにほんご です");
    let context = h.tree.ime_context(&h.text).unwrap();
    // Only the caret's paragraph.
    assert_eq!(context.text, "にほんご です");
    assert_eq!(context.base, "first line\n".len());
    let word = context.base..context.base + "にほんご".len();
    h.tree.dispatch(Event::ImeSelect(word), &mut h.text);
    h.prepare();
    h.compose("日本語");
    h.type_text("日本語");
    let area = h.tree.get::<TextArea>(area).unwrap();
    assert_eq!(area.text(), "first line\n日本語 です");
}

type Insert = fn(&mut UiTree, WidgetId) -> WidgetId;

/// Every field that tells the input method its text also lets it convert a word again.
#[test]
fn every_field_the_input_method_reads_takes_a_reconversion() {
    use noa_gpui_core::widgets::NumberInput;
    let cases: [(&str, Insert, &str, &str, &str); 4] = [
        (
            "search",
            |t, p| t.insert(Some(p), SearchField::new("", 240.0)),
            "にほんご です",
            "にほんご",
            "日本語",
        ),
        (
            "combo",
            |t, p| t.insert(Some(p), ComboBox::new(["a"], 240.0)),
            "にほんご です",
            "にほんご",
            "日本語",
        ),
        (
            "tokens",
            |t, p| t.insert(Some(p), TokenField::new("", 240.0)),
            "にほんご です",
            "にほんご",
            "日本語",
        ),
        (
            "number",
            |t, p| t.insert(Some(p), NumberInput::new(0.0, 100.0, 0.0)),
            "12",
            "1",
            "9",
        ),
    ];
    for (name, make, typed, word, converted) in cases {
        let mut h = Harness::new();
        let root = h.root;
        let id = make(&mut h.tree, root);
        h.prepare();
        h.focus(id);
        // Start from an empty field (the number input shows its value).
        h.command('a');
        h.key(Key::Backspace);
        h.type_text(typed);
        let context = h.tree.ime_context(&h.text).expect(name);
        assert_eq!(context.text, typed, "{name}");
        let start = context.base + typed.find(word).unwrap();
        h.tree
            .dispatch(Event::ImeSelect(start..start + word.len()), &mut h.text);
        h.prepare();
        h.compose(converted);
        h.type_text(converted);
        let after = h.tree.ime_context(&h.text).expect(name).text;
        assert_eq!(after, typed.replacen(word, converted, 1), "{name}");
    }
}

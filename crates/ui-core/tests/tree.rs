use noa_gpui_core::style::{FlexDirection, Size as LayoutSize, length, percent};
use noa_gpui_core::widgets::{
    Button, ButtonClicked, Container, Label, ScrollView, TextArea, TextChanged, TextField,
};
use noa_gpui_core::{
    Design, DragData, Event, Key, Modifiers, OverlayDismissed, OverlayOptions, Placement,
    PointerButton, Role, Shortcut, ShortcutPressed, Style, Theme, UiTree, WidgetId,
};
use noa_gpui_scene::{Color, DrawCommand, Point, Size};
use noa_gpui_text::{FontOptions, TextStore};

const VIEWPORT: Size = Size::new(400.0, 300.0);

fn text_store() -> TextStore {
    TextStore::new(FontOptions {
        locale: "en-US".into(),
        load_system_fonts: false,
    })
}

fn column() -> Style {
    Style {
        flex_direction: FlexDirection::Column,
        size: LayoutSize {
            width: percent(1.0),
            height: percent(1.0),
        },
        ..Default::default()
    }
}

fn fixed(w: f32, h: f32) -> Style {
    Style {
        size: LayoutSize {
            width: length(w),
            height: length(h),
        },
        flex_shrink: 0.0,
        ..Default::default()
    }
}

fn click(tree: &mut UiTree, text: &mut TextStore, pos: Point) {
    let button = PointerButton::Primary;
    tree.dispatch(Event::PointerDown { pos, button }, text);
    tree.dispatch(Event::PointerUp { pos, button }, text);
}

fn key(tree: &mut UiTree, text: &mut TextStore, key: Key, shift: bool) {
    let modifiers = Modifiers {
        shift,
        ..Default::default()
    };
    tree.dispatch(Event::KeyDown { key, modifiers }, text);
}

fn center(tree: &UiTree, id: WidgetId) -> Point {
    let origin = tree.window_origin(id).unwrap();
    let rect = tree.layout(id).unwrap().rect;
    Point::new(origin.x + rect.width / 2.0, origin.y + rect.height / 2.0)
}

#[test]
fn removed_ids_never_alias_new_widgets() {
    let mut text = text_store();
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()));
    let a = tree.insert(Some(root), Label::new("a"));
    tree.remove(a, &mut text);
    let b = tree.insert(Some(root), Label::new("b"));

    assert_ne!(a, b);
    assert!(!tree.contains(a));
    assert!(tree.get::<Label>(a).is_none());
    assert_eq!(tree.get::<Label>(b).unwrap().text(), "b");
    assert_eq!(tree.children(root), &[b]);
}

#[test]
fn removing_a_subtree_releases_text_layouts() {
    let mut text = text_store();
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()));
    let group = tree.insert(Some(root), Container::new(Style::default()));
    for i in 0..5 {
        tree.insert(Some(group), Label::new(format!("item {i}")));
    }
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(text.layouts().len(), 5);

    tree.remove(group, &mut text);
    assert_eq!(tree.len(), 1);
    assert_eq!(text.layouts().len(), 0);
    tree.prepare(VIEWPORT, &mut text).unwrap();
}

#[test]
fn lays_out_with_flexbox() {
    let mut text = text_store();
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()));
    let a = tree.insert(Some(root), Container::new(fixed(100.0, 40.0)));
    let b = tree.insert(Some(root), Container::new(fixed(50.0, 20.0)));
    tree.prepare(VIEWPORT, &mut text).unwrap();

    assert_eq!(tree.layout(root).unwrap().rect.width, 400.0);
    assert_eq!(tree.layout(a).unwrap().rect.y, 0.0);
    assert_eq!(tree.layout(b).unwrap().rect.y, 40.0);
}

/// A tree with fixed-size rows so a change in one row cannot resize another.
fn rows(text: &mut TextStore, count: usize) -> (UiTree, Vec<WidgetId>, Vec<WidgetId>) {
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()));
    let mut rows = Vec::new();
    let mut labels = Vec::new();
    for i in 0..count {
        let row = tree.insert(
            Some(root),
            Container::new(fixed(400.0, 20.0)).background(Color::WHITE),
        );
        labels.push(tree.insert(Some(row), Label::new(format!("row {i}"))));
        rows.push(row);
    }
    tree.prepare(VIEWPORT, text).unwrap();
    (tree, rows, labels)
}

#[test]
fn unchanged_tree_produces_no_frame() {
    let mut text = text_store();
    let (mut tree, _, _) = rows(&mut text, 10);
    assert!(!tree.needs_update(VIEWPORT));
    assert!(tree.prepare(VIEWPORT, &mut text).is_none());
}

#[test]
fn text_change_relayouts_and_repaints_only_that_label() {
    let mut text = text_store();
    let (mut tree, _, labels) = rows(&mut text, 10);

    tree.update::<Label, _>(labels[3], |l, cx| l.set_text(cx, "a much longer label"));
    tree.prepare(VIEWPORT, &mut text).unwrap();
    let stats = tree.stats();
    assert!(stats.layout);
    assert_eq!(stats.repainted, 1, "{stats:?}");
    assert!(
        stats.layouts_read < 2 * 10,
        "read back {} layouts",
        stats.layouts_read
    );
}

#[test]
fn color_change_repaints_without_layout() {
    let mut text = text_store();
    let (mut tree, rows, _) = rows(&mut text, 10);

    tree.update::<Container, _>(rows[5], |c, cx| {
        c.set_background(cx, Some(Color::BLACK.into()))
    });
    tree.prepare(VIEWPORT, &mut text).unwrap();
    let stats = tree.stats();
    assert!(!stats.layout);
    assert_eq!(stats.repainted, 1);
    let black = tree
        .display_list()
        .commands()
        .iter()
        .filter(|c| matches!(c, DrawCommand::Quad { color, .. } if *color == Color::BLACK))
        .count();
    assert_eq!(black, 1);
}

#[test]
fn border_change_repaints_without_layout() {
    let mut text = text_store();
    let (mut tree, rows, _) = rows(&mut text, 10);
    let framed = |tree: &UiTree| {
        tree.display_list()
            .commands()
            .iter()
            .filter_map(|c| match c {
                DrawCommand::Quad { border, radius, .. } if border.color == Color::BLACK => {
                    Some((border.width, radius[0]))
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };

    tree.update::<Container, _>(rows[5], |c, cx| {
        c.set_border(cx, Some((2.0, Color::BLACK.into())));
        c.set_radius(cx, 4.0);
    });
    tree.prepare(VIEWPORT, &mut text).unwrap();
    let stats = tree.stats();
    assert!(!stats.layout);
    assert_eq!(stats.repainted, 1);
    assert_eq!(framed(&tree), [(2.0, 4.0)]);

    tree.update::<Container, _>(rows[5], |c, cx| c.set_border(cx, None));
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert!(framed(&tree).is_empty());
}

fn scrolling_list(text: &mut TextStore, count: usize) -> (UiTree, WidgetId, Vec<WidgetId>) {
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()));
    let scroll = tree.insert(
        Some(root),
        ScrollView::new(Style {
            flex_direction: FlexDirection::Column,
            size: LayoutSize {
                width: length(200.0),
                height: length(100.0),
            },
            ..Default::default()
        }),
    );
    let items = (0..count)
        .map(|_| {
            tree.insert(
                Some(scroll),
                Container::new(fixed(200.0, 20.0)).background(Color::WHITE),
            )
        })
        .collect();
    tree.prepare(VIEWPORT, text).unwrap();
    (tree, scroll, items)
}

#[test]
fn scrolling_composites_without_layout_or_repainting_content() {
    let mut text = text_store();
    let (mut tree, _, items) = scrolling_list(&mut text, 50);
    assert_eq!(tree.layout(items[49]).unwrap().rect.y, 980.0);

    let pos = Point::new(50.0, 50.0);
    tree.dispatch(
        Event::Wheel {
            pos,
            delta: Point::new(0.0, -45.0),
        },
        &mut text,
    );
    tree.prepare(VIEWPORT, &mut text).unwrap();
    let stats = tree.stats();
    assert!(!stats.layout);
    assert_eq!(stats.repainted, 1, "only the scroll indicator repaints");
    assert_eq!(tree.window_origin(items[2]).unwrap().y, 40.0 - 45.0);

    // Scrolling past the end pulls the content a little further (rubber band), then
    // it springs back to the end once the gesture is over.
    tree.dispatch(
        Event::Wheel {
            pos,
            delta: Point::new(0.0, -10_000.0),
        },
        &mut text,
    );
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert!(tree.window_origin(items[49]).unwrap().y < 100.0 - 20.0);
    // Two seconds: the spring settles, then the overlay scroll bar fades out.
    for frame in 1..=120 {
        tree.animate(f64::from(frame) / 60.0, &mut text);
        tree.prepare(VIEWPORT, &mut text);
    }
    assert_eq!(tree.window_origin(items[49]).unwrap().y, 100.0 - 20.0);
    assert!(!tree.wants_animation_frame());
}

#[test]
fn offscreen_children_are_culled() {
    let mut text = text_store();
    let (tree, _, _) = scrolling_list(&mut text, 1000);
    let stats = tree.stats();
    assert!(stats.culled >= 990, "{stats:?}");
    // Root has no paint; scroll view clip push/pop + a handful of visible rows.
    assert!(stats.commands < 20, "{stats:?}");
}

#[test]
fn hit_testing_respects_clip_and_scroll() {
    let mut text = text_store();
    let (mut tree, scroll, items) = scrolling_list(&mut text, 50);
    assert_eq!(tree.hit_test(Point::new(10.0, 25.0)), Some(items[1]));
    // Row 10 lies at y=200, below the 100px viewport: clipped away.
    assert_eq!(tree.hit_test(Point::new(10.0, 205.0)), tree.root());
    tree.update::<ScrollView, _>(scroll, |s, cx| s.set_offset(cx, 200.0));
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(tree.hit_test(Point::new(10.0, 5.0)), Some(items[10]));
}

fn form(text: &mut TextStore) -> (UiTree, WidgetId, WidgetId, WidgetId) {
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()));
    let ok = tree.insert(Some(root), Button::new("OK"));
    let field = tree.insert(Some(root), TextField::new("name", 160.0));
    let cancel = tree.insert(Some(root), Button::new("Cancel"));
    tree.prepare(VIEWPORT, text).unwrap();
    (tree, ok, field, cancel)
}

#[test]
fn tab_cycles_focus_in_tree_order() {
    let mut text = text_store();
    let (mut tree, ok, field, cancel) = form(&mut text);
    assert_eq!(tree.focused(), None);
    key(&mut tree, &mut text, Key::Tab, false);
    assert_eq!(tree.focused(), Some(ok));
    key(&mut tree, &mut text, Key::Tab, false);
    assert_eq!(tree.focused(), Some(field));
    key(&mut tree, &mut text, Key::Tab, false);
    assert_eq!(tree.focused(), Some(cancel));
    key(&mut tree, &mut text, Key::Tab, false);
    assert_eq!(tree.focused(), Some(ok));
    key(&mut tree, &mut text, Key::Tab, true);
    assert_eq!(tree.focused(), Some(cancel));

    // Focus change repaints exactly the two affected widgets.
    tree.prepare(VIEWPORT, &mut text).unwrap();
    key(&mut tree, &mut text, Key::Tab, false);
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(tree.stats().repainted, 2);
}

#[test]
fn clicking_focuses_and_activates_buttons() {
    let mut text = text_store();
    let (mut tree, ok, _, cancel) = form(&mut text);

    let pos = center(&tree, cancel);
    click(&mut tree, &mut text, pos);
    assert_eq!(tree.focused(), Some(cancel));
    let messages = tree.take_messages();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].source, cancel);
    assert!(messages[0].downcast_ref::<ButtonClicked>().is_some());

    let button = PointerButton::Primary;
    let pos = center(&tree, ok);
    tree.dispatch(Event::PointerDown { pos, button }, &mut text);
    tree.dispatch(
        Event::PointerUp {
            pos: Point::new(390.0, 290.0),
            button,
        },
        &mut text,
    );
    assert!(tree.take_messages().is_empty());

    // Keyboard activation of the focused button (Space; Return is for the default button).
    key(&mut tree, &mut text, Key::Space, false);
    let messages = tree.take_messages();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].source, ok);

    click(&mut tree, &mut text, Point::new(390.0, 290.0));
    assert_eq!(tree.focused(), None);
}

#[test]
fn text_field_edits_when_focused() {
    let mut text = text_store();
    let (mut tree, _, field, _) = form(&mut text);

    tree.dispatch(Event::Text("ignored".into()), &mut text);
    assert_eq!(tree.get::<TextField>(field).unwrap().text(), "");

    let pos = center(&tree, field);
    click(&mut tree, &mut text, pos);
    assert_eq!(tree.focused(), Some(field));
    tree.dispatch(Event::Text("héllo".into()), &mut text);
    key(&mut tree, &mut text, Key::Backspace, false);
    assert_eq!(tree.get::<TextField>(field).unwrap().text(), "héll");

    let changes: Vec<_> = tree
        .take_messages()
        .iter()
        .filter_map(|m| m.downcast_ref::<TextChanged>().cloned())
        .collect();
    assert_eq!(
        changes,
        vec![TextChanged("héllo".into()), TextChanged("héll".into())]
    );
}

#[test]
fn hover_repaints_old_and_new_targets() {
    let mut text = text_store();
    let (mut tree, ok, _, cancel) = form(&mut text);
    let pos = center(&tree, ok);
    tree.dispatch(Event::PointerMove { pos }, &mut text);
    assert_eq!(tree.hovered(), Some(ok));
    tree.prepare(VIEWPORT, &mut text).unwrap();
    let pos = center(&tree, cancel);
    tree.dispatch(Event::PointerMove { pos }, &mut text);
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(tree.stats().repainted, 2);
    tree.dispatch(Event::PointerLeave, &mut text);
    assert_eq!(tree.hovered(), None);
}

#[test]
fn resizing_the_viewport_relayouts() {
    let mut text = text_store();
    let (mut tree, rows, _) = rows(&mut text, 3);
    assert!(tree.needs_update(Size::new(800.0, 600.0)));
    tree.prepare(Size::new(800.0, 600.0), &mut text).unwrap();
    assert!(tree.stats().layout);
    assert_eq!(tree.layout(tree.root().unwrap()).unwrap().rect.width, 800.0);
    assert_eq!(tree.layout(rows[0]).unwrap().rect.width, 400.0);
}

fn focused_field(text: &mut TextStore) -> (UiTree, WidgetId) {
    let (mut tree, _, field, _) = form(text);
    tree.set_focus(Some(field));
    tree.prepare(VIEWPORT, text).unwrap();
    (tree, field)
}

fn field_text(tree: &UiTree, field: WidgetId) -> (String, usize, String) {
    let f = tree.get::<TextField>(field).unwrap();
    (f.text().to_owned(), f.caret(), f.composition().to_owned())
}

fn changes(tree: &mut UiTree) -> Vec<String> {
    tree.take_messages()
        .iter()
        .filter_map(|m| m.downcast_ref::<TextChanged>().map(|c| c.0.clone()))
        .collect()
}

#[test]
fn composition_is_shown_but_not_committed_until_commit() {
    let mut text = text_store();
    let (mut tree, field) = focused_field(&mut text);
    let compose = |tree: &mut UiTree, text: &mut TextStore, s: &str, cursor| {
        tree.dispatch(
            Event::Composition {
                text: s.into(),
                cursor,
            },
            text,
        );
    };

    compose(&mut tree, &mut text, "にほん", Some((9, 9)));
    assert_eq!(field_text(&tree, field), ("".into(), 0, "にほん".into()));
    assert!(changes(&mut tree).is_empty(), "composition is not a change");
    tree.prepare(VIEWPORT, &mut text).unwrap();
    let caret_during = tree.text_input_area().unwrap().x;

    // Keys go to the IME while composing: no caret movement, no focus change.
    key(&mut tree, &mut text, Key::Tab, false);
    key(&mut tree, &mut text, Key::Backspace, false);
    assert_eq!(tree.focused(), Some(field));
    assert_eq!(field_text(&tree, field).2, "にほん");

    // winit clears the composition right before committing.
    compose(&mut tree, &mut text, "", None);
    tree.dispatch(Event::Text("日本".into()), &mut text);
    assert_eq!(
        field_text(&tree, field),
        ("日本".into(), "日本".len(), "".into())
    );
    assert_eq!(changes(&mut tree), vec!["日本".to_owned()]);
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert!(tree.text_input_area().unwrap().x > tree.window_origin(field).unwrap().x);
    assert!(caret_during > tree.window_origin(field).unwrap().x);
}

#[test]
fn composition_cursor_moves_the_ime_area() {
    let mut text = text_store();
    let (mut tree, _) = focused_field(&mut text);
    let mut area_at = |cursor: usize| {
        let event = Event::Composition {
            text: "abcd".into(),
            cursor: Some((cursor, cursor)),
        };
        tree.dispatch(event, &mut text);
        tree.prepare(VIEWPORT, &mut text).unwrap();
        tree.text_input_area().unwrap().x
    };
    let (start, middle, end) = (area_at(0), area_at(2), area_at(4));
    assert!(start < middle && middle < end, "{start} {middle} {end}");
}

#[test]
fn caret_sits_at_the_end_of_the_clause_being_converted() {
    let mut text = text_store();
    let (mut tree, _) = focused_field(&mut text);
    let mut area_for = |cursor| {
        let event = Event::Composition {
            text: "日本語".into(),
            cursor,
        };
        tree.dispatch(event, &mut text);
        tree.prepare(VIEWPORT, &mut text).unwrap();
        tree.text_input_area().unwrap().x
    };
    let len = "日本語".len();
    let typing = area_for(Some((len, len)));
    // macOS selects the whole clause while converting: (0, len).
    let converting = area_for(Some((0, len)));
    let no_cursor = area_for(None);
    let first_clause = area_for(Some((0, "日本".len())));
    assert_eq!(converting, typing, "caret stays at the right end");
    assert_eq!(no_cursor, typing);
    assert!(first_clause < typing);
}

#[test]
fn commit_inserts_at_the_caret() {
    let mut text = text_store();
    let (mut tree, field) = focused_field(&mut text);
    tree.dispatch(Event::Text("ac".into()), &mut text);
    key(&mut tree, &mut text, Key::ArrowLeft, false);
    tree.dispatch(Event::Text("b".into()), &mut text);
    assert_eq!(field_text(&tree, field), ("abc".into(), 2, "".into()));
    key(&mut tree, &mut text, Key::Home, false);
    tree.dispatch(Event::Text("ず".into()), &mut text);
    key(&mut tree, &mut text, Key::End, false);
    tree.dispatch(Event::Text("!".into()), &mut text);
    assert_eq!(field_text(&tree, field).0, "ずabc!");
}

#[test]
fn editing_never_splits_grapheme_clusters() {
    let mut text = text_store();
    let (mut tree, field) = focused_field(&mut text);
    let family = "👨\u{200d}👩\u{200d}👧";
    let e_acute = "e\u{301}";
    tree.dispatch(Event::Text(format!("a{family}{e_acute}")), &mut text);

    key(&mut tree, &mut text, Key::Backspace, false);
    assert_eq!(field_text(&tree, field).0, format!("a{family}"));
    key(&mut tree, &mut text, Key::ArrowLeft, false);
    assert_eq!(
        field_text(&tree, field).1,
        1,
        "one step skips the whole emoji"
    );
    key(&mut tree, &mut text, Key::Delete, false);
    assert_eq!(field_text(&tree, field).0, "a");
    key(&mut tree, &mut text, Key::Backspace, false);
    assert_eq!(field_text(&tree, field), ("".into(), 0, "".into()));
    key(&mut tree, &mut text, Key::Backspace, false);
    assert_eq!(field_text(&tree, field).0, "");
}

#[test]
fn ime_area_exists_only_while_a_text_field_is_focused() {
    let mut text = text_store();
    let (mut tree, ok, field, _) = form(&mut text);
    assert_eq!(tree.text_input_area(), None);
    tree.set_focus(Some(ok));
    assert_eq!(tree.text_input_area(), None, "buttons take no text input");
    tree.set_focus(Some(field));
    tree.prepare(VIEWPORT, &mut text).unwrap();
    let empty = tree.text_input_area().unwrap();
    tree.dispatch(Event::Text("hello".into()), &mut text);
    tree.prepare(VIEWPORT, &mut text).unwrap();
    let typed = tree.text_input_area().unwrap();
    assert!(typed.x > empty.x);
    assert_eq!(typed.y, empty.y);
}

#[test]
fn composition_in_an_empty_field_is_drawn_as_composition() {
    // The exact sequence macOS sent (logged from the demo), starting from an empty field.
    let mut text = text_store();
    let (mut tree, _) = focused_field(&mut text);
    let empty_caret = tree.text_input_area().unwrap().x;
    for (s, c) in [
        ("ｎ", 3),
        ("に", 3),
        ("にｈ", 6),
        ("にほ", 6),
        ("にほｎ", 9),
        ("にほん", 9),
        ("にほんｇ", 12),
        ("にほんご", 12),
    ] {
        tree.dispatch(
            Event::Composition {
                text: s.into(),
                cursor: Some((c, c)),
            },
            &mut text,
        );
        tree.prepare(VIEWPORT, &mut text).unwrap();
    }
    assert!(
        tree.text_input_area().unwrap().x > empty_caret + 20.0,
        "caret follows the composition"
    );
    let underlines = tree
        .display_list()
        .commands()
        .iter()
        .filter(|c| matches!(c, DrawCommand::Quad { rect, .. } if rect.height <= 2.5 && rect.width > 20.0))
        .count();
    assert_eq!(underlines, 1, "composition is underlined");
}

fn all_layouts(tree: &UiTree) -> Vec<(WidgetId, noa_gpui_layout::NodeLayout)> {
    fn walk(tree: &UiTree, id: WidgetId, out: &mut Vec<(WidgetId, noa_gpui_layout::NodeLayout)>) {
        out.push((id, tree.layout(id).unwrap()));
        for &child in tree.children(id) {
            walk(tree, child, out);
        }
    }
    let mut out = Vec::new();
    walk(tree, tree.root().unwrap(), &mut out);
    out
}

/// Lay out the whole tree from scratch (a resize forces a full pass) and return it.
fn full_layout(
    tree: &mut UiTree,
    text: &mut TextStore,
) -> Vec<(WidgetId, noa_gpui_layout::NodeLayout)> {
    tree.prepare(Size::new(VIEWPORT.width, VIEWPORT.height + 1.0), text);
    tree.prepare(VIEWPORT, text).unwrap();
    assert_eq!(tree.stats().partial_layouts, 0);
    all_layouts(tree)
}

#[test]
fn change_inside_a_fixed_row_lays_out_only_that_row() {
    let mut text = text_store();
    let (mut tree, _, labels) = rows(&mut text, 50);

    tree.update::<Label, _>(labels[7], |l, cx| {
        l.set_text(cx, "a considerably longer label")
    });
    tree.prepare(VIEWPORT, &mut text).unwrap();
    let stats = tree.stats();
    assert_eq!(stats.partial_layouts, 1, "{stats:?}");
    assert!(
        stats.layouts_read <= 2,
        "read back only the row's content: {stats:?}"
    );
    assert_eq!(stats.repainted, 1);

    let partial = all_layouts(&tree);
    assert_eq!(
        partial,
        full_layout(&mut tree, &mut text),
        "partial layout must match a full one"
    );
}

#[test]
fn changes_in_many_rows_lay_out_each_row_once_and_match_a_full_layout() {
    let mut text = text_store();
    let (mut tree, rows, labels) = rows(&mut text, 60);
    // A fixed box inside row 3 (a boundary inside a boundary), changed too: laid out
    // with row 3.
    let inner = tree.insert(Some(rows[3]), Container::new(fixed(100.0, 10.0)));
    let nested = tree.insert(Some(inner), Label::new("nested"));
    tree.prepare(VIEWPORT, &mut text).unwrap();

    let changed: Vec<usize> = (0..60).filter(|i| i % 3 != 1).collect();
    for &i in &changed {
        tree.update::<Label, _>(labels[i], |l, cx| l.set_text(cx, format!("changed {i}")));
    }
    tree.update::<Label, _>(nested, |l, cx| l.set_text(cx, "nested, changed"));
    tree.update::<Label, _>(labels[0], |l, cx| l.set_text(cx, "changed again"));
    tree.prepare(VIEWPORT, &mut text).unwrap();
    let stats = tree.stats();
    assert_eq!(stats.partial_layouts, changed.len(), "{stats:?}");

    let partial = all_layouts(&tree);
    assert_eq!(
        partial,
        full_layout(&mut tree, &mut text),
        "partial layouts must match a full one"
    );
}

#[test]
fn changes_outside_boundaries_lay_out_the_whole_tree() {
    let mut text = text_store();
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()));
    let group = tree.insert(Some(root), Container::new(Style::default()));
    let label = tree.insert(Some(group), Label::new("short"));
    let below = tree.insert(Some(root), Container::new(fixed(50.0, 10.0)));
    tree.prepare(VIEWPORT, &mut text).unwrap();
    let before = tree.layout(below).unwrap().rect.y;

    tree.update::<Label, _>(label, |l, cx| {
        l.set_text(cx, "line one\nline two\nline three")
    });
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(tree.stats().partial_layouts, 0);
    assert!(
        tree.layout(below).unwrap().rect.y > before,
        "the sibling below moved down"
    );
    let after = all_layouts(&tree);
    assert_eq!(after, full_layout(&mut tree, &mut text));
}

#[test]
fn inserting_a_widget_lays_out_the_whole_tree() {
    let mut text = text_store();
    let (mut tree, rows, _) = rows(&mut text, 5);
    tree.insert(Some(rows[2]), Label::new("extra"));
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(tree.stats().partial_layouts, 0);
}

#[test]
fn scrolled_list_row_update_matches_full_layout() {
    let mut text = text_store();
    let (mut tree, scroll, items) = scrolling_list(&mut text, 200);
    let label = tree.insert(Some(items[3]), Label::new("x"));
    tree.update::<ScrollView, _>(scroll, |s, cx| s.set_offset(cx, 40.0));
    tree.prepare(VIEWPORT, &mut text).unwrap();

    tree.update::<Label, _>(label, |l, cx| l.set_text(cx, "much wider text"));
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(tree.stats().partial_layouts, 1);
    let partial = all_layouts(&tree);
    assert_eq!(partial, full_layout(&mut tree, &mut text));
    assert_eq!(tree.window_origin(items[3]).unwrap().y, 60.0 - 40.0);
}

#[test]
fn reversed_lists_are_culled_and_hit_tested_correctly() {
    // Column-reverse places later children above earlier ones: not sorted top to bottom,
    // so culling and hit testing must not rely on binary search.
    let mut text = text_store();
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()));
    let scroll = tree.insert(
        Some(root),
        ScrollView::new(Style {
            flex_direction: FlexDirection::ColumnReverse,
            size: LayoutSize {
                width: length(200.0),
                height: length(100.0),
            },
            ..Default::default()
        }),
    );
    let items: Vec<_> = (0..100)
        .map(|_| {
            tree.insert(
                Some(scroll),
                Container::new(fixed(200.0, 20.0)).background(Color::WHITE),
            )
        })
        .collect();
    tree.prepare(VIEWPORT, &mut text).unwrap();

    let hit = tree.hit_test(Point::new(10.0, 10.0)).unwrap();
    let origin = tree.window_origin(hit).unwrap();
    assert!(items.contains(&hit));
    assert!(origin.y <= 10.0 && 10.0 < origin.y + 20.0, "{origin:?}");
    let quads = tree
        .display_list()
        .commands()
        .iter()
        .filter(|c| matches!(c, DrawCommand::Quad { .. }))
        .count();
    assert_eq!(quads, 5, "exactly the five rows in the 100px viewport");
}

#[test]
fn content_overflowing_past_later_rows_is_still_drawn() {
    // Each row is a layout boundary. When row 0's label grows past every row below,
    // the rows are no longer ordered top to bottom and culling must not binary-search
    // them.
    let mut text = text_store();
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()));
    let scroll = tree.insert(
        Some(root),
        ScrollView::new(Style {
            flex_direction: FlexDirection::Column,
            ..fixed(200.0, 100.0)
        }),
    );
    let row = || {
        Container::new(Style {
            flex_direction: FlexDirection::Column,
            ..fixed(200.0, 20.0)
        })
    };
    let first = tree.insert(Some(scroll), row());
    let label = tree.insert(Some(first), Label::new("short"));
    for _ in 1..100 {
        tree.insert(Some(scroll), row());
    }
    tree.prepare(VIEWPORT, &mut text).unwrap();

    tree.update::<Label, _>(label, |l, cx| l.set_text(cx, "line\n".repeat(300)));
    tree.update::<ScrollView, _>(scroll, |s, cx| s.set_offset(cx, 1000.0));
    tree.prepare(VIEWPORT, &mut text).unwrap();

    assert!(tree.layout(label).unwrap().rect.height > 1100.0);
    let drawn = tree
        .display_list()
        .commands()
        .iter()
        .any(|c| matches!(c, DrawCommand::Text { .. }));
    assert!(drawn, "row 0's label covers the viewport at offset 1000");
}

#[test]
fn binary_search_culling_matches_the_visible_rows() {
    let mut text = text_store();
    let (mut tree, scroll, items) = scrolling_list(&mut text, 1000);
    for offset in [0.0, 333.0, 10_000.0, 19_900.0] {
        tree.update::<ScrollView, _>(scroll, |s, cx| s.set_offset(cx, offset));
        // Offset 0 is the current one: nothing changes and no frame is produced.
        tree.prepare(VIEWPORT, &mut text);
        let drawn = tree
            .display_list()
            .commands()
            .iter()
            .filter(|c| matches!(c, DrawCommand::Quad { rect, .. } if rect.width == 200.0))
            .count();
        let visible = items
            .iter()
            .filter(|&&id| {
                let y = tree.window_origin(id).unwrap().y;
                y + 20.0 > 0.0 && y < 100.0
            })
            .count();
        assert_eq!(drawn, visible, "offset {offset}");
        let hit = tree.hit_test(Point::new(5.0, 55.0)).unwrap();
        let y = tree.window_origin(hit).unwrap().y;
        assert!(
            y <= 55.0 && 55.0 < y + 20.0,
            "offset {offset}: hit row at {y}"
        );
    }
}

fn text_colors(tree: &UiTree) -> Vec<Color> {
    tree.display_list()
        .commands()
        .iter()
        .filter_map(|c| match c {
            DrawCommand::Text { color, .. } => Some(*color),
            _ => None,
        })
        .collect()
}

#[test]
fn switching_the_theme_repaints_every_widget_with_role_colors() {
    let mut text = text_store();
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()).background(Role::Background));
    tree.insert(Some(root), Label::new("role"));
    tree.insert(Some(root), Label::new("fixed").color(Color::BLACK));
    tree.prepare(VIEWPORT, &mut text);
    assert_eq!(text_colors(&tree), [Theme::light().text, Color::BLACK]);

    tree.set_theme(Theme::dark());
    assert!(tree.prepare(VIEWPORT, &mut text).is_some());
    assert_eq!(tree.stats().repainted, 3);
    assert_eq!(text_colors(&tree), [Theme::dark().text, Color::BLACK]);

    tree.set_theme(Theme::dark());
    assert!(tree.prepare(VIEWPORT, &mut text).is_none());
}

#[test]
fn disabled_subtrees_ignore_input_lose_focus_and_draw_dimmed() {
    let mut text = text_store();
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()));
    let group = tree.insert(Some(root), Container::new(fixed(300.0, 100.0)));
    let button = tree.insert(Some(group), Button::new("Save"));
    let outside = tree.insert(Some(root), Button::new("Other"));
    tree.prepare(VIEWPORT, &mut text);
    let pos = center(&tree, button);

    tree.set_focus(Some(button));
    tree.set_enabled(group, false);
    assert!(!tree.is_enabled(button));
    assert_eq!(tree.focused(), None, "disabling drops focus");
    tree.prepare(VIEWPORT, &mut text);

    // The pointer reaches the nearest enabled ancestor (the group itself is disabled).
    assert_eq!(tree.hit_test(pos), Some(root));
    click(&mut tree, &mut text, pos);
    assert!(tree.take_messages().is_empty());
    key(&mut tree, &mut text, Key::Tab, false);
    assert_eq!(tree.focused(), Some(outside));
    tree.set_focus(Some(button));
    assert_eq!(
        tree.focused(),
        Some(outside),
        "cannot focus a disabled widget"
    );

    let fills: Vec<f32> = tree
        .display_list()
        .commands()
        .iter()
        .filter_map(|c| match c {
            DrawCommand::Quad { color, .. } if color.a > 0.0 => Some(color.a),
            _ => None,
        })
        .collect();
    assert_eq!(
        fills[0],
        Theme::light().disabled_opacity,
        "disabled button fill"
    );
    assert_eq!(fills[1], 1.0, "enabled button fill");

    tree.set_enabled(group, true);
    tree.prepare(VIEWPORT, &mut text);
    click(&mut tree, &mut text, pos);
    let messages = tree.take_messages();
    assert_eq!(messages.len(), 1);
    assert!(messages[0].downcast_ref::<ButtonClicked>().is_some());
}

fn tree_with_overlay(options: OverlayOptions) -> (UiTree, TextStore, WidgetId, WidgetId, WidgetId) {
    let mut text = text_store();
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()));
    let anchor = tree.insert(Some(root), Button::new("Open"));
    tree.prepare(VIEWPORT, &mut text);
    let origin = tree.window_origin(anchor).unwrap();
    let size = tree.layout(anchor).unwrap().rect;
    let anchor_rect = noa_gpui_scene::Rect::new(origin.x, origin.y, size.width, size.height);
    let overlay = tree.open_overlay(
        Container::new(Style {
            flex_direction: FlexDirection::Column,
            ..Default::default()
        })
        .background(Color::BLACK),
        Placement::Below(anchor_rect),
        options,
    );
    let item = tree.insert(Some(overlay), Button::new("First item"));
    tree.insert(Some(overlay), Button::new("Second"));
    tree.prepare(VIEWPORT, &mut text);
    (tree, text, anchor, overlay, item)
}

#[test]
fn overlays_size_to_content_sit_below_the_anchor_and_draw_on_top() {
    let (tree, _text, anchor, overlay, item) = tree_with_overlay(OverlayOptions::MENU);
    let anchor_box = tree.layout(anchor).unwrap().rect;
    let menu = tree.layout(overlay).unwrap().rect;
    assert!(menu.width < VIEWPORT.width / 2.0, "{menu:?}");
    assert_eq!(menu.width, tree.layout(item).unwrap().rect.width);
    assert_eq!(
        menu.y,
        anchor_box.bottom() + noa_gpui_core::overlay::ANCHOR_GAP
    );
    assert_eq!(tree.window_origin(item).unwrap().y, menu.y);

    // Drawn after (above) the main tree, and hit first where they overlap it.
    let commands = tree.display_list().commands();
    let last_quad = commands
        .iter()
        .rposition(|c| matches!(c, DrawCommand::Quad { .. }))
        .unwrap();
    let menu_fill = commands
        .iter()
        .position(|c| matches!(c, DrawCommand::Quad { color, .. } if *color == Color::BLACK))
        .unwrap();
    assert!(menu_fill > 0 && menu_fill < last_quad);
    assert_eq!(tree.hit_test(center(&tree, item)), Some(item));
}

#[test]
fn pressing_outside_a_menu_closes_it_and_consumes_the_press() {
    let (mut tree, mut text, anchor, overlay, item) = tree_with_overlay(OverlayOptions::MENU);
    let pos = center(&tree, item);
    click(&mut tree, &mut text, pos);
    assert!(tree.contains(overlay));
    let messages = tree.take_messages();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].source, item);

    let pos = center(&tree, anchor);
    click(&mut tree, &mut text, pos);
    assert!(!tree.contains(overlay));
    assert_eq!(tree.overlays().count(), 0);
    let messages = tree.take_messages();
    assert_eq!(messages.len(), 1, "the dismissing press is consumed");
    assert_eq!(messages[0].source, overlay);
    assert!(messages[0].downcast_ref::<OverlayDismissed>().is_some());
    assert!(tree.prepare(VIEWPORT, &mut text).is_some());
}

#[test]
fn a_press_outside_a_popover_also_reaches_what_is_under_it() {
    let (mut tree, mut text, anchor, overlay, _) = tree_with_overlay(OverlayOptions::POPOVER);
    let pos = center(&tree, anchor);
    click(&mut tree, &mut text, pos);
    assert!(!tree.contains(overlay));
    let sources: Vec<WidgetId> = tree.take_messages().iter().map(|m| m.source).collect();
    assert_eq!(sources, [overlay, anchor]);
}

#[test]
fn escape_closes_the_top_light_dismiss_overlay_only() {
    let (mut tree, mut text, _, overlay, _) = tree_with_overlay(OverlayOptions::PASSIVE);
    key(&mut tree, &mut text, Key::Escape, false);
    assert!(tree.contains(overlay), "passive overlays stay");

    let menu = tree.open_overlay(
        Container::new(fixed(50.0, 50.0)),
        Placement::At(Point::new(200.0, 200.0)),
        OverlayOptions::MENU,
    );
    tree.prepare(VIEWPORT, &mut text);
    key(&mut tree, &mut text, Key::Escape, false);
    assert!(!tree.contains(menu));
    assert!(tree.contains(overlay));
    assert_eq!(tree.overlays().collect::<Vec<_>>(), [overlay]);
}

#[test]
fn modal_overlays_block_input_below_and_keep_tab_inside() {
    let (mut tree, mut text, anchor, overlay, item) = tree_with_overlay(OverlayOptions::MODAL);
    assert_eq!(tree.hit_test(center(&tree, anchor)), Some(overlay));
    let pos = center(&tree, anchor);
    click(&mut tree, &mut text, pos);
    assert!(tree.take_messages().is_empty());
    assert!(
        tree.contains(overlay),
        "modal overlays are not light-dismissed"
    );

    let second = tree.children(overlay)[1];
    key(&mut tree, &mut text, Key::Tab, false);
    assert_eq!(tree.focused(), Some(item));
    let pos = center(&tree, anchor);
    click(&mut tree, &mut text, pos);
    assert_eq!(
        tree.focused(),
        Some(item),
        "a press outside keeps focus in the modal"
    );
    key(&mut tree, &mut text, Key::Tab, false);
    assert_eq!(tree.focused(), Some(second));
    key(&mut tree, &mut text, Key::Tab, false);
    assert_eq!(tree.focused(), Some(item), "focus wraps inside the modal");

    tree.close_overlay(overlay, &mut text);
    tree.prepare(VIEWPORT, &mut text);
    let pos = center(&tree, anchor);
    click(&mut tree, &mut text, pos);
    assert_eq!(tree.take_messages()[0].source, anchor);
}

#[test]
fn hover_animates_over_frames_and_stops_asking_for_frames_when_done() {
    let mut text = text_store();
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()));
    let button = tree.insert(Some(root), Button::new("Hover me"));
    tree.prepare(VIEWPORT, &mut text);
    assert!(!tree.wants_animation_frame(), "idle trees draw nothing");

    let amount = |tree: &UiTree| tree.get::<Button>(button).unwrap().hover_amount();
    tree.set_now(10.0);
    let pos = center(&tree, button);
    tree.dispatch(Event::PointerMove { pos }, &mut text);
    assert!(tree.wants_animation_frame());

    tree.animate(10.05, &mut text);
    let halfway = amount(&tree);
    assert!(halfway > 0.0 && halfway < 1.0, "{halfway}");
    assert!(tree.wants_animation_frame());
    assert!(tree.prepare(VIEWPORT, &mut text).is_some());

    tree.animate(10.5, &mut text);
    assert_eq!(amount(&tree), 1.0);
    assert!(!tree.wants_animation_frame(), "done: no more frames");
    tree.prepare(VIEWPORT, &mut text);
    assert!(tree.prepare(VIEWPORT, &mut text).is_none());

    tree.set_now(20.0);
    tree.dispatch(Event::PointerLeave, &mut text);
    tree.animate(20.5, &mut text);
    assert_eq!(amount(&tree), 0.0);
    assert!(!tree.wants_animation_frame());
}

#[test]
fn arrow_keys_move_inside_a_focus_group_and_tab_treats_it_as_one_stop() {
    let mut text = text_store();
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()));
    let before = tree.insert(Some(root), Button::new("Before"));
    let group = tree.insert(
        Some(root),
        Container::new(Style::default()).focus_group(true),
    );
    let items: Vec<WidgetId> = ["A", "B", "C"]
        .iter()
        .map(|name| tree.insert(Some(group), Button::new(*name)))
        .collect();
    let after = tree.insert(Some(root), Button::new("After"));
    tree.prepare(VIEWPORT, &mut text);

    let tab = |tree: &mut UiTree, text: &mut TextStore, shift| key(tree, text, Key::Tab, shift);
    tab(&mut tree, &mut text, false);
    assert_eq!(tree.focused(), Some(before));
    tab(&mut tree, &mut text, false);
    assert_eq!(
        tree.focused(),
        Some(items[0]),
        "Tab enters the group at its first item"
    );
    key(&mut tree, &mut text, Key::ArrowRight, false);
    key(&mut tree, &mut text, Key::ArrowDown, false);
    assert_eq!(tree.focused(), Some(items[2]));
    key(&mut tree, &mut text, Key::ArrowRight, false);
    assert_eq!(
        tree.focused(),
        Some(items[0]),
        "arrows wrap inside the group"
    );
    key(&mut tree, &mut text, Key::ArrowLeft, false);
    assert_eq!(tree.focused(), Some(items[2]));
    tab(&mut tree, &mut text, false);
    assert_eq!(
        tree.focused(),
        Some(after),
        "Tab leaves the group in one step"
    );
    tab(&mut tree, &mut text, true);
    assert_eq!(
        tree.focused(),
        Some(items[2]),
        "and comes back to the last item focused"
    );

    tree.set_focus(Some(after));
    key(&mut tree, &mut text, Key::ArrowLeft, false);
    assert_eq!(tree.focused(), Some(after));
}

#[test]
fn hidden_widgets_cannot_take_programmatic_focus() {
    let mut text = text_store();
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()));
    let visible = tree.insert(Some(root), Button::new("Visible"));
    let hidden = tree.insert(Some(root), Button::new("Hidden"));
    tree.set_hidden(hidden, true);
    let parent = tree.insert(Some(root), Container::new(Style::default()));
    let nested = tree.insert(Some(parent), Button::new("Hidden parent"));
    tree.set_hidden(parent, true);
    let styled = tree.insert(Some(root), Button::new("Display none"));
    tree.set_inline_style(
        styled,
        noa_gpui_core::StylePatch::parse("display: none;").unwrap(),
    );
    let tabs = tree.insert(Some(root), noa_gpui_core::widgets::TabView::new(["A", "B"]));
    tree.insert(Some(tabs), Label::new("Shown page"));
    let page = tree.insert(Some(tabs), Button::new("Hidden page"));
    tree.prepare(VIEWPORT, &mut text);
    tree.set_focus(Some(visible));

    for target in [hidden, nested, styled, page] {
        assert!(tree.is_hidden(target));
        tree.set_focus(Some(target));
        assert_eq!(tree.focused(), Some(visible), "hidden target {target:?}");
    }
}

#[test]
fn focus_groups_skip_hidden_items_and_hidden_subtrees() {
    let mut text = text_store();
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()));
    let before = tree.insert(Some(root), Button::new("Before"));
    let group = tree.insert(
        Some(root),
        Container::new(Style::default()).focus_group(true),
    );
    let hidden = tree.insert(Some(group), Button::new("Hidden"));
    let first = tree.insert(Some(group), Button::new("First"));
    let nested = tree.insert(Some(group), Container::new(Style::default()));
    tree.insert(Some(nested), Button::new("Hidden subtree"));
    let styled = tree.insert(Some(group), Button::new("Display none"));
    let last = tree.insert(Some(group), Button::new("Last"));
    let after = tree.insert(Some(root), Button::new("After"));
    tree.set_hidden(hidden, true);
    tree.set_hidden(nested, true);
    tree.set_inline_style(
        styled,
        noa_gpui_core::StylePatch::parse("display: none;").unwrap(),
    );
    tree.prepare(VIEWPORT, &mut text);

    tree.set_focus(Some(before));
    key(&mut tree, &mut text, Key::Tab, false);
    assert_eq!(tree.focused(), Some(first));
    key(&mut tree, &mut text, Key::ArrowRight, false);
    assert_eq!(tree.focused(), Some(last));
    key(&mut tree, &mut text, Key::ArrowRight, false);
    assert_eq!(tree.focused(), Some(first));
    key(&mut tree, &mut text, Key::ArrowLeft, false);
    assert_eq!(tree.focused(), Some(last));

    key(&mut tree, &mut text, Key::Tab, false);
    assert_eq!(tree.focused(), Some(after));
    tree.set_hidden(last, true);
    key(&mut tree, &mut text, Key::Tab, true);
    assert_eq!(tree.focused(), Some(first));
    tree.set_hidden(first, true);
    tree.set_focus(Some(before));
    key(&mut tree, &mut text, Key::Tab, false);
    assert_eq!(tree.focused(), Some(after), "an empty group is skipped");
}

#[test]
fn moving_a_subtree_preserves_explicit_hidden_and_disabled_state() {
    let mut text = text_store();
    let mut source = UiTree::new();
    let root = source.insert(None, Container::new(column()));
    let group = source.insert(Some(root), Container::new(Style::default()));
    let disabled = source.insert(Some(group), Button::new("Disabled"));
    let hidden = source.insert(Some(group), Button::new("Hidden"));
    let visible = source.insert(Some(group), Button::new("Visible"));
    source.set_enabled(disabled, false);
    source.set_hidden(hidden, true);
    source.set_enabled(group, false);
    source.set_hidden(group, true);
    source.prepare(VIEWPORT, &mut text);

    let mut target = UiTree::new();
    let moved = target.insert_subtree(None, source.take_subtree(group).unwrap());
    let children = target.children(moved).to_vec();
    assert!(!target.is_enabled(moved));
    assert!(target.is_hidden(moved));
    target.set_enabled(moved, true);
    target.set_hidden(moved, false);
    target.prepare(VIEWPORT, &mut text);
    assert!(!target.is_enabled(children[0]));
    assert!(target.is_hidden(children[1]));
    assert!(target.is_enabled(children[2]));
    assert!(!target.is_hidden(children[2]));
    assert!(!source.contains(disabled));
    assert!(!source.contains(hidden));
    assert!(!source.contains(visible));

    // Being on an inactive page is the old parent's state, not the widget's own.
    let tabs = source.insert(Some(root), noa_gpui_core::widgets::TabView::new(["A", "B"]));
    source.insert(Some(tabs), Label::new("Shown page"));
    let inactive = source.insert(Some(tabs), Button::new("Inactive page"));
    assert!(source.is_hidden(inactive));
    let active = target.insert_subtree(Some(moved), source.take_subtree(inactive).unwrap());
    assert!(!target.is_hidden(active));
}

#[test]
fn shortcuts_fire_when_the_focused_widget_leaves_the_key_unhandled() {
    let mut text = text_store();
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()));
    let field = tree.insert(Some(root), TextField::new("", 200.0));
    let save = tree.insert(Some(root), Button::new("Save"));
    tree.add_shortcut(Shortcut::command('s'), save);
    tree.prepare(VIEWPORT, &mut text);
    tree.set_focus(Some(field));

    let press = |tree: &mut UiTree, text: &mut TextStore, modifiers| {
        tree.dispatch(
            Event::KeyDown {
                key: Key::Char('s'),
                modifiers,
            },
            text,
        )
    };
    let command = Modifiers {
        command: true,
        meta: true,
        ..Default::default()
    };
    press(&mut tree, &mut text, Modifiers::default());
    assert!(
        tree.take_messages().is_empty(),
        "a plain 's' is not the shortcut"
    );
    press(&mut tree, &mut text, command);
    let messages = tree.take_messages();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].source, save);
    assert_eq!(
        messages[0].downcast_ref::<ShortcutPressed>(),
        Some(&ShortcutPressed(Shortcut::command('s')))
    );

    tree.remove_shortcut(Shortcut::command('s'));
    press(&mut tree, &mut text, command);
    assert!(tree.take_messages().is_empty());
}

fn type_text(tree: &mut UiTree, text: &mut TextStore, s: &str) {
    tree.dispatch(Event::Text(s.into()), text);
}

fn command(shift: bool) -> Modifiers {
    Modifiers {
        meta: true,
        command: true,
        shift,
        ..Default::default()
    }
}

fn press(tree: &mut UiTree, text: &mut TextStore, key: Key, modifiers: Modifiers) {
    tree.dispatch(Event::KeyDown { key, modifiers }, text);
}

#[derive(Clone, Default)]
struct SharedClipboard(std::rc::Rc<std::cell::RefCell<Option<String>>>);

impl noa_gpui_core::clipboard::Clipboard for SharedClipboard {
    fn read_text(&mut self) -> Option<String> {
        self.0.borrow().clone()
    }
    fn write_text(&mut self, text: &str) {
        *self.0.borrow_mut() = Some(text.to_owned());
    }
}

#[test]
fn text_fields_select_copy_paste_and_undo_through_the_tree() {
    let mut text = text_store();
    let mut tree = UiTree::new();
    let clipboard = SharedClipboard::default();
    tree.set_clipboard(Box::new(clipboard.clone()));
    let root = tree.insert(None, Container::new(column()));
    let field = tree.insert(Some(root), TextField::new("", 300.0));
    tree.prepare(VIEWPORT, &mut text);
    tree.set_focus(Some(field));

    type_text(&mut tree, &mut text, "hello world");
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    for _ in 0..5 {
        press(&mut tree, &mut text, Key::ArrowLeft, shift);
    }
    fn get(tree: &UiTree, id: WidgetId) -> &TextField {
        tree.get::<TextField>(id).unwrap()
    }
    assert_eq!(get(&tree, field).selection(), 6..11);
    press(&mut tree, &mut text, Key::Char('c'), command(false));
    assert_eq!(clipboard.0.borrow().as_deref(), Some("world"));

    type_text(&mut tree, &mut text, "there ");
    press(&mut tree, &mut text, Key::Char('v'), command(false));
    assert_eq!(get(&tree, field).text(), "hello there world");

    press(&mut tree, &mut text, Key::Char('z'), command(false));
    assert_eq!(get(&tree, field).text(), "hello there ");
    press(&mut tree, &mut text, Key::Char('z'), command(true));
    assert_eq!(get(&tree, field).text(), "hello there world");

    let messages = tree.take_messages();
    let last = messages
        .iter()
        .rev()
        .find_map(|m| m.downcast_ref::<TextChanged>())
        .unwrap();
    assert_eq!(last.0, "hello there world");
    press(&mut tree, &mut text, Key::Char('a'), command(false));
    tree.prepare(VIEWPORT, &mut text);
    assert!(tree.display_list().commands().iter().any(
        |c| matches!(c, DrawCommand::Quad { color, .. } if *color == Theme::light().selection)
    ));
}

#[test]
fn dragging_and_double_clicking_select_text() {
    let mut text = text_store();
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()));
    let field = tree.insert(Some(root), TextField::new("", 300.0));
    tree.prepare(VIEWPORT, &mut text);
    tree.set_focus(Some(field));
    type_text(&mut tree, &mut text, "alpha beta gamma");
    tree.prepare(VIEWPORT, &mut text);

    let origin = tree.window_origin(field).unwrap();
    let y = origin.y + tree.layout(field).unwrap().rect.height / 2.0;
    let primary = PointerButton::Primary;
    tree.dispatch(
        Event::PointerDown {
            pos: Point::new(origin.x + 2.0, y),
            button: primary,
        },
        &mut text,
    );
    tree.dispatch(
        Event::PointerMove {
            pos: Point::new(origin.x + 290.0, y),
        },
        &mut text,
    );
    tree.dispatch(
        Event::PointerUp {
            pos: Point::new(origin.x + 290.0, y),
            button: primary,
        },
        &mut text,
    );
    assert_eq!(tree.get::<TextField>(field).unwrap().selection(), 0..16);

    tree.set_now(5.0);
    let probe = text.create(
        "alpha beta gamma",
        noa_gpui_text::TextStyle::new(14.0),
        None,
    );
    let beta = text.caret(probe, 8);
    let pos = Point::new(origin.x + 10.0 + beta.x, y);
    for t in [5.0, 5.1] {
        tree.set_now(t);
        tree.dispatch(
            Event::PointerDown {
                pos,
                button: primary,
            },
            &mut text,
        );
        tree.dispatch(
            Event::PointerUp {
                pos,
                button: primary,
            },
            &mut text,
        );
    }
    let field_ref = tree.get::<TextField>(field).unwrap();
    assert_eq!(&field_ref.text()[field_ref.selection()], "beta");
}

#[derive(Default)]
struct ClickProbe {
    counts: Vec<u8>,
}

impl noa_gpui_core::Widget for ClickProbe {
    fn style(&self) -> Style {
        fixed(100.0, 40.0)
    }
    fn paint(&mut self, _cx: &mut noa_gpui_core::PaintCx) {}
    fn event(&mut self, cx: &mut noa_gpui_core::EventCx, event: &Event) {
        if let Event::PointerDown { .. } = event {
            self.counts.push(cx.click_count());
            cx.set_handled();
        }
    }
}

#[test]
fn presses_close_in_time_and_place_count_up() {
    let mut text = text_store();
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()));
    let probe = tree.insert(Some(root), ClickProbe::default());
    tree.prepare(VIEWPORT, &mut text);
    let origin = tree.window_origin(probe).unwrap();
    let press = |tree: &mut UiTree, text: &mut TextStore, at: f64, dx: f32, button| {
        tree.set_now(at);
        let pos = Point::new(origin.x + 20.0 + dx, origin.y + 20.0);
        tree.dispatch(Event::PointerDown { pos, button }, text);
        tree.dispatch(Event::PointerUp { pos, button }, text);
    };
    let primary = PointerButton::Primary;
    // Three quick presses count up; a fourth after a pause starts over.
    press(&mut tree, &mut text, 1.0, 0.0, primary);
    press(&mut tree, &mut text, 1.2, 1.0, primary);
    press(&mut tree, &mut text, 1.4, 0.0, primary);
    press(&mut tree, &mut text, 2.4, 0.0, primary);
    // Moving too far, or another button, starts over too.
    press(&mut tree, &mut text, 2.5, 10.0, primary);
    press(&mut tree, &mut text, 2.6, 10.0, PointerButton::Secondary);
    // A longer OS interval lets slower presses count.
    tree.set_multi_click(1.0, 4.0);
    press(&mut tree, &mut text, 3.0, 0.0, primary);
    press(&mut tree, &mut text, 3.8, 0.0, primary);
    assert_eq!(
        tree.get::<ClickProbe>(probe).unwrap().counts,
        [1, 2, 3, 1, 1, 1, 1, 2]
    );
}

#[test]
fn a_triple_click_selects_the_line_and_a_fourth_starts_over() {
    let mut text = text_store();
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()));
    let area = tree.insert(Some(root), TextArea::new(""));
    tree.prepare(VIEWPORT, &mut text);
    tree.set_focus(Some(area));
    type_text(&mut tree, &mut text, "alpha beta");
    tree.dispatch(
        Event::KeyDown {
            key: Key::Enter,
            modifiers: Modifiers::default(),
        },
        &mut text,
    );
    type_text(&mut tree, &mut text, "gamma");
    tree.prepare(VIEWPORT, &mut text);
    let origin = tree.window_origin(area).unwrap();
    let pos = Point::new(origin.x + 14.0, origin.y + 14.0);
    let primary = PointerButton::Primary;
    let mut selections = Vec::new();
    for t in [5.0, 5.1, 5.2, 5.3] {
        tree.set_now(t);
        tree.dispatch(
            Event::PointerDown {
                pos,
                button: primary,
            },
            &mut text,
        );
        tree.dispatch(
            Event::PointerUp {
                pos,
                button: primary,
            },
            &mut text,
        );
        let area = tree.get::<TextArea>(area).unwrap();
        selections.push(area.text()[area.selection()].to_owned());
    }
    assert_eq!(selections, ["", "alpha", "alpha beta", ""]);
}

#[test]
fn long_text_scrolls_to_keep_the_caret_visible() {
    let mut text = text_store();
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()));
    let field = tree.insert(Some(root), TextField::new("", 120.0));
    tree.prepare(VIEWPORT, &mut text);
    tree.set_focus(Some(field));
    type_text(
        &mut tree,
        &mut text,
        "a long sentence that cannot fit in the field",
    );
    tree.prepare(VIEWPORT, &mut text);
    let scroll = tree.get::<TextField>(field).unwrap().scroll_x();
    assert!(scroll > 100.0, "{scroll}");
    let area = tree.text_input_area().unwrap();
    let field_rect = tree.window_rect(field).unwrap();
    assert!(
        area.x > field_rect.x && area.x < field_rect.right(),
        "{area:?} in {field_rect:?}"
    );
    assert_eq!(field_rect.width, 120.0);
    assert!(
        tree.display_list()
            .commands()
            .iter()
            .any(|c| matches!(c, DrawCommand::PushClip { .. }))
    );

    press(&mut tree, &mut text, Key::Home, Modifiers::default());
    tree.prepare(VIEWPORT, &mut text);
    assert_eq!(tree.get::<TextField>(field).unwrap().scroll_x(), 0.0);
}

#[test]
fn leaves_draw_their_own_commands_and_clipping_leaves_their_clip() {
    // Childless widgets: a plain box, a clipping box (its clip is still pushed and
    // popped, around nothing), and a clipping box out of the window (no clip at all).
    let mut text = text_store();
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()));
    let red = Color::hex(0xff0000);
    let blue = Color::hex(0x0000ff);
    let green = Color::hex(0x00ff00);
    tree.insert(
        Some(root),
        Container::new(fixed(40.0, 20.0)).background(red),
    );
    tree.insert(
        Some(root),
        Container::new(fixed(50.0, 30.0))
            .background(blue)
            .clip(true),
    );
    tree.insert(
        Some(root),
        Container::new(fixed(60.0, 400.0))
            .background(green)
            .clip(true),
    );
    tree.insert(
        Some(root),
        Container::new(fixed(70.0, 10.0)).background(red).clip(true),
    );
    tree.prepare(VIEWPORT, &mut text);
    let shown: Vec<String> = tree
        .display_list()
        .commands()
        .iter()
        .map(|c| match c {
            DrawCommand::Quad { rect, .. } => format!("quad {rect:?}"),
            DrawCommand::PushClip { rect } => format!("push {rect:?}"),
            DrawCommand::PopClip => "pop".to_owned(),
            other => format!("{other:?}"),
        })
        .collect();
    let rect =
        |x: f32, y: f32, w: f32, h: f32| format!("{:?}", noa_gpui_scene::Rect::new(x, y, w, h));
    assert_eq!(
        shown,
        [
            format!("quad {}", rect(0.0, 0.0, 40.0, 20.0)),
            format!("quad {}", rect(0.0, 20.0, 50.0, 30.0)),
            format!("push {}", rect(0.0, 20.0, 50.0, 30.0)),
            "pop".to_owned(),
            format!("quad {}", rect(0.0, 50.0, 60.0, 400.0)),
            format!("push {}", rect(0.0, 50.0, 60.0, 400.0)),
            "pop".to_owned(),
        ],
        "the last box is below the window"
    );
}

#[test]
fn text_areas_insert_line_breaks_move_between_lines_and_grow() {
    let mut text = text_store();
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()));
    let area = tree.insert(Some(root), TextArea::new("Notes").width(200.0).min_lines(2));
    tree.prepare(VIEWPORT, &mut text);
    let empty_height = tree.layout(area).unwrap().rect.height;
    tree.set_focus(Some(area));

    type_text(&mut tree, &mut text, "first");
    press(&mut tree, &mut text, Key::Enter, Modifiers::default());
    type_text(&mut tree, &mut text, "second line");
    press(&mut tree, &mut text, Key::Enter, Modifiers::default());
    type_text(&mut tree, &mut text, "third");
    tree.prepare(VIEWPORT, &mut text);
    fn get(tree: &UiTree, id: WidgetId) -> &TextArea {
        tree.get::<TextArea>(id).unwrap()
    }
    assert_eq!(get(&tree, area).text(), "first\nsecond line\nthird");
    assert!(
        tree.layout(area).unwrap().rect.height > empty_height,
        "grows with lines"
    );

    press(&mut tree, &mut text, Key::ArrowUp, Modifiers::default());
    let caret = get(&tree, area).caret();
    assert!((6..18).contains(&caret), "second line: {caret}");
    press(&mut tree, &mut text, Key::ArrowUp, Modifiers::default());
    assert!(get(&tree, area).caret() <= 5, "first line");
    press(&mut tree, &mut text, Key::ArrowUp, Modifiers::default());
    assert_eq!(
        get(&tree, area).caret(),
        0,
        "up from the first line goes to the start"
    );
    press(&mut tree, &mut text, Key::End, Modifiers::default());
    assert_eq!(get(&tree, area).caret(), 5);
    for _ in 0..3 {
        press(&mut tree, &mut text, Key::ArrowDown, Modifiers::default());
    }
    assert_eq!(get(&tree, area).caret(), get(&tree, area).text().len());

    tree.prepare(VIEWPORT, &mut text);
    let low = tree.text_input_area().unwrap();
    press(&mut tree, &mut text, Key::ArrowUp, Modifiers::default());
    press(&mut tree, &mut text, Key::ArrowUp, Modifiers::default());
    tree.prepare(VIEWPORT, &mut text);
    assert!(tree.text_input_area().unwrap().y < low.y);
}

#[test]
fn text_area_height_fits_wrapped_text() {
    let mut text = text_store();
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()));
    let card = tree.insert(
        Some(root),
        Container::new(Style {
            flex_direction: FlexDirection::Column,
            align_items: noa_gpui_core::style::AlignItems::START,
            ..Default::default()
        }),
    );
    let area = tree.insert(Some(card), TextArea::new("").width(264.0).min_lines(2));
    let body = "Shopping list\n- coffee beans\n- oat milk\n\nLong lines wrap at the edge of the box, and the box grows as you add lines.";
    tree.update::<TextArea, _>(area, |a, cx| a.set_text(cx, body));
    tree.prepare(VIEWPORT, &mut text);
    let rect = tree.layout(area).unwrap().rect;
    // What the text needs at the content width (264 - 2 * 8 padding).
    let probe = text.create(body, noa_gpui_text::TextStyle::new(14.0), Some(248.0));
    let needed = text.size(probe).height + 16.0;
    assert!(rect.height >= needed, "{} < {needed}", rect.height);
}

struct DragSource {
    tag: &'static str,
    pressed: bool,
    ended: Option<bool>,
}

impl noa_gpui_core::Widget for DragSource {
    fn style(&self) -> Style {
        fixed(100.0, 40.0)
    }
    fn paint(&mut self, _cx: &mut noa_gpui_core::PaintCx) {}
    fn event(&mut self, cx: &mut noa_gpui_core::EventCx, event: &Event) {
        match event {
            Event::PointerDown { .. } => {
                self.pressed = true;
                cx.capture_pointer();
                cx.set_handled();
            }
            Event::PointerMove { .. } if self.pressed && cx.has_capture() => {
                cx.start_drag(DragData::Text(self.tag.into()));
                self.pressed = false;
            }
            Event::DragEnd { dropped } => self.ended = Some(*dropped),
            _ => {}
        }
    }
}

#[derive(Default)]
struct DropTarget {
    over: bool,
    dropped: Vec<DragData>,
}

impl noa_gpui_core::Widget for DropTarget {
    fn style(&self) -> Style {
        fixed(100.0, 40.0)
    }
    fn paint(&mut self, _cx: &mut noa_gpui_core::PaintCx) {}
    fn event(&mut self, cx: &mut noa_gpui_core::EventCx, event: &Event) {
        match event {
            Event::DragOver { .. } => {
                self.over = true;
                cx.accept_drop();
            }
            Event::DragLeave => self.over = false,
            Event::Drop { data, .. } => {
                self.over = false;
                self.dropped.push(data.clone());
            }
            _ => {}
        }
    }
}

fn drag_tree() -> (UiTree, TextStore, WidgetId, WidgetId, WidgetId) {
    let mut text = text_store();
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()));
    let source = tree.insert(
        Some(root),
        DragSource {
            tag: "row 3",
            pressed: false,
            ended: None,
        },
    );
    let target = tree.insert(Some(root), DropTarget::default());
    let empty = tree.insert(Some(root), Container::new(fixed(100.0, 40.0)));
    tree.prepare(VIEWPORT, &mut text);
    (tree, text, source, target, empty)
}

#[test]
fn widgets_drag_data_to_targets_that_accept_it() {
    let (mut tree, mut text, source, target, empty) = drag_tree();
    let primary = PointerButton::Primary;
    let start = center(&tree, source);
    tree.dispatch(
        Event::PointerDown {
            pos: start,
            button: primary,
        },
        &mut text,
    );
    tree.dispatch(Event::PointerMove { pos: start }, &mut text);
    assert!(tree.is_dragging());

    let over = center(&tree, target);
    tree.dispatch(Event::PointerMove { pos: over }, &mut text);
    assert!(tree.get::<DropTarget>(target).unwrap().over);
    let off = center(&tree, empty);
    tree.dispatch(Event::PointerMove { pos: off }, &mut text);
    assert!(!tree.get::<DropTarget>(target).unwrap().over);
    tree.dispatch(Event::PointerMove { pos: over }, &mut text);
    tree.dispatch(
        Event::PointerUp {
            pos: over,
            button: primary,
        },
        &mut text,
    );

    assert!(!tree.is_dragging());
    assert_eq!(
        tree.get::<DropTarget>(target).unwrap().dropped,
        [DragData::Text("row 3".into())]
    );
    assert_eq!(tree.get::<DragSource>(source).unwrap().ended, Some(true));
}

#[test]
fn releasing_outside_a_target_or_escape_cancels_the_drag() {
    let (mut tree, mut text, source, target, empty) = drag_tree();
    let primary = PointerButton::Primary;
    let start = center(&tree, source);
    let off = center(&tree, empty);
    tree.dispatch(
        Event::PointerDown {
            pos: start,
            button: primary,
        },
        &mut text,
    );
    tree.dispatch(Event::PointerMove { pos: off }, &mut text);
    tree.dispatch(
        Event::PointerUp {
            pos: off,
            button: primary,
        },
        &mut text,
    );
    assert_eq!(tree.get::<DragSource>(source).unwrap().ended, Some(false));

    tree.dispatch(
        Event::PointerDown {
            pos: start,
            button: primary,
        },
        &mut text,
    );
    let over = center(&tree, target);
    tree.dispatch(Event::PointerMove { pos: over }, &mut text);
    key(&mut tree, &mut text, Key::Escape, false);
    assert!(!tree.is_dragging());
    assert!(!tree.get::<DropTarget>(target).unwrap().over);
    assert!(tree.get::<DropTarget>(target).unwrap().dropped.is_empty());
    assert_eq!(tree.get::<DragSource>(source).unwrap().ended, Some(false));
}

#[test]
fn files_from_the_os_drop_through_the_same_events() {
    let (mut tree, mut text, _, target, _) = drag_tree();
    let files = DragData::Files(vec!["/tmp/a.png".into()]);
    let pos = center(&tree, target);
    tree.dispatch(
        Event::DragOver {
            pos,
            data: files.clone(),
        },
        &mut text,
    );
    assert!(tree.get::<DropTarget>(target).unwrap().over);
    tree.dispatch(
        Event::Drop {
            pos,
            data: files.clone(),
        },
        &mut text,
    );
    assert_eq!(
        tree.get::<DropTarget>(target).unwrap().dropped,
        std::slice::from_ref(&files)
    );
    tree.dispatch(Event::DragOver { pos, data: files }, &mut text);
    tree.dispatch(Event::DragLeave, &mut text);
    assert!(!tree.is_dragging());
    assert_eq!(tree.get::<DropTarget>(target).unwrap().dropped.len(), 1);
}

#[test]
fn move_child_reorders_siblings_and_their_layout() {
    let mut text = text_store();
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()));
    let rows: Vec<WidgetId> = (0..3)
        .map(|_| tree.insert(Some(root), Container::new(fixed(100.0, 20.0))))
        .collect();
    tree.prepare(VIEWPORT, &mut text);
    tree.move_child(rows[2], 0);
    tree.prepare(VIEWPORT, &mut text);
    assert_eq!(tree.children(root), &[rows[2], rows[0], rows[1]]);
    let ys: Vec<f32> = rows
        .iter()
        .map(|&r| tree.layout(r).unwrap().rect.y)
        .collect();
    assert_eq!(ys, [20.0, 40.0, 0.0]);
}

#[test]
fn the_accessibility_tree_exposes_roles_labels_values_and_focus() {
    use noa_gpui_core::accesskit::{Action, ActionRequest, Role as AxRole, TreeId};
    use noa_gpui_core::{ACCESS_ROOT, access_id};

    let mut text = text_store();
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()));
    // A plain container is flattened away; its children hang off the window node.
    let group = tree.insert(Some(root), Container::new(Style::default()));
    let label = tree.insert(Some(group), Label::new("Name"));
    let field = tree.insert(Some(group), TextField::new("Your name", 200.0));
    let save = tree.insert(Some(root), Button::new("Save"));
    let off = tree.insert(Some(root), Button::new("Delete"));
    tree.set_enabled(off, false);
    tree.prepare(VIEWPORT, &mut text);
    tree.set_focus(Some(field));
    type_text(&mut tree, &mut text, "Ada");
    tree.prepare(VIEWPORT, &mut text);

    let update = tree.accessibility_update(2.0);
    assert_eq!(update.tree_id, TreeId::ROOT);
    assert_eq!(update.focus, access_id(field));
    let node = |id| {
        update
            .nodes
            .iter()
            .find(|(n, _)| *n == id)
            .map(|(_, node)| node.clone())
            .unwrap()
    };
    let window = node(ACCESS_ROOT);
    assert_eq!(window.role(), AxRole::Window);
    assert_eq!(
        window.children(),
        [
            access_id(label),
            access_id(field),
            access_id(save),
            access_id(off)
        ]
    );
    assert_eq!(node(access_id(label)).value(), Some("Name"));
    let field_node = node(access_id(field));
    assert_eq!(field_node.role(), AxRole::TextInput);
    assert_eq!(field_node.value(), Some("Ada"));
    assert_eq!(field_node.placeholder(), Some("Your name"));
    assert!(field_node.supports_action(Action::Focus));
    let save_node = node(access_id(save));
    assert_eq!(
        (save_node.role(), save_node.label()),
        (AxRole::Button, Some("Save"))
    );
    assert!(save_node.supports_action(Action::Click));
    let bounds = save_node.bounds().unwrap();
    let rect = tree.window_rect(save).unwrap();
    assert_eq!((bounds.x0, bounds.y0), (rect.x as f64, rect.y as f64));
    assert!(node(access_id(off)).is_disabled());
    assert!(!node(access_id(off)).supports_action(Action::Focus));

    let request = |action, target| ActionRequest {
        action,
        target_tree: TreeId::ROOT,
        target_node: target,
        data: None,
    };
    tree.take_messages();
    tree.accessibility_action(&request(Action::Click, access_id(save)), &mut text);
    let messages = tree.take_messages();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].source, save);
    assert!(messages[0].downcast_ref::<ButtonClicked>().is_some());
    assert_eq!(tree.focused(), Some(save));
    tree.accessibility_action(&request(Action::Click, access_id(off)), &mut text);
    assert!(
        tree.take_messages().is_empty(),
        "disabled widgets ignore actions"
    );
    tree.accessibility_action(&request(Action::Focus, access_id(field)), &mut text);
    assert_eq!(tree.focused(), Some(field));
}

#[test]
fn accessibility_actions_ignore_widgets_hidden_since_the_previous_update() {
    use noa_gpui_core::accesskit::{Action, ActionRequest, TreeId};
    use noa_gpui_core::widgets::RadioGroup;
    use noa_gpui_core::{access_id, access_item_id};

    let mut text = text_store();
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()));
    let visible = tree.insert(Some(root), Button::new("Visible"));
    let button = tree.insert(Some(root), Button::new("Hidden"));
    let parent = tree.insert(Some(root), Container::new(Style::default()));
    let radio = tree.insert(Some(parent), RadioGroup::new(["A", "B"]));
    tree.prepare(VIEWPORT, &mut text);
    tree.set_focus(Some(visible));
    let _previous_update = tree.accessibility_update(1.0);
    tree.set_hidden(button, true);
    tree.set_hidden(parent, true);
    tree.take_messages();

    for target in [access_id(button), access_item_id(radio, 1)] {
        tree.accessibility_action(
            &ActionRequest {
                action: Action::Click,
                target_tree: TreeId::ROOT,
                target_node: target,
                data: None,
            },
            &mut text,
        );
    }
    assert!(tree.take_messages().is_empty());
    assert_eq!(tree.focused(), Some(visible));
    assert_eq!(tree.get::<RadioGroup>(radio).unwrap().selected(), 0);
}

#[test]
fn accessibility_item_actions_reject_a_reused_widget_slot() {
    use noa_gpui_core::accesskit::{Action, ActionRequest, TreeId};
    use noa_gpui_core::widgets::RadioGroup;
    use noa_gpui_core::{access_id, access_item_id};

    for detach in [false, true] {
        let mut text = text_store();
        let mut tree = UiTree::new();
        let root = tree.insert(None, Container::new(column()));
        let old = tree.insert(Some(root), RadioGroup::new(["A", "B"]));
        tree.prepare(VIEWPORT, &mut text);
        let old_item = access_item_id(old, 1);
        let current = if detach {
            let subtree = tree.take_subtree(old).unwrap();
            tree.insert_subtree(Some(root), subtree)
        } else {
            tree.remove(old, &mut text);
            tree.insert(Some(root), RadioGroup::new(["A", "B"]))
        };
        // The arena reuses the slot, but both forms of accessibility id change.
        assert_eq!(access_id(current).0, access_id(old).0 + (1 << 32));
        assert_ne!(access_item_id(current, 1), old_item);
        assert!(!tree.contains(old));
        let request = |action, target| ActionRequest {
            action,
            target_tree: TreeId::ROOT,
            target_node: target,
            data: None,
        };
        for (action, target) in [(Action::Focus, access_id(old)), (Action::Click, old_item)] {
            tree.accessibility_action(&request(action, target), &mut text);
        }
        assert_eq!(tree.focused(), None);
        assert_eq!(tree.get::<RadioGroup>(current).unwrap().selected(), 0);
        assert!(tree.take_messages().is_empty());

        tree.accessibility_action(&request(Action::Focus, access_id(current)), &mut text);
        assert_eq!(tree.focused(), Some(current));
        tree.accessibility_action(
            &request(Action::Click, access_item_id(current, 1)),
            &mut text,
        );
        assert_eq!(tree.get::<RadioGroup>(current).unwrap().selected(), 1);
    }
}

#[test]
fn accessibility_ids_stay_unique_after_exhausting_a_slots_generations() {
    use noa_gpui_core::accesskit::{Action, ActionRequest, TreeId};
    use noa_gpui_core::widgets::RadioGroup;
    use noa_gpui_core::{ACCESS_ROOT, access_id, access_item_id};
    use std::collections::HashSet;

    for detach in [false, true] {
        let mut text = text_store();
        let mut tree = UiTree::new();
        let root = tree.insert(None, Container::new(column()));
        let mut current = tree.insert(Some(root), RadioGroup::new(["A", "B"]));
        let first = access_item_id(current, 1);
        let mut seen = HashSet::from([ACCESS_ROOT.0, access_id(root).0]);
        // Cross the 11-bit generation limit through both removal paths. Normal node
        // ids and the first and last encodable items must remain distinct.
        for _ in 0..=2048 {
            assert!(seen.insert(access_id(current).0));
            for item in [0, (1 << 20) - 1] {
                assert!(seen.insert(access_item_id(current, item).0));
            }
            let previous = current;
            current = if detach {
                let subtree = tree.take_subtree(current).unwrap();
                tree.insert_subtree(Some(root), subtree)
            } else {
                tree.remove(current, &mut text);
                tree.insert(Some(root), RadioGroup::new(["A", "B"]))
            };
            assert!(!tree.contains(previous));
            assert_eq!(tree.len(), 2);
            assert_eq!(tree.children(root), &[current]);
        }
        let request = |target| ActionRequest {
            action: Action::Click,
            target_tree: TreeId::ROOT,
            target_node: target,
            data: None,
        };
        tree.accessibility_action(&request(first), &mut text);
        assert_eq!(tree.get::<RadioGroup>(current).unwrap().selected(), 0);
        assert_eq!(tree.focused(), None);
        tree.accessibility_action(&request(access_item_id(current, 1)), &mut text);
        assert_eq!(tree.get::<RadioGroup>(current).unwrap().selected(), 1);
        assert_eq!(tree.focused(), Some(current));
    }
}

#[test]
#[should_panic(expected = "accessibility item index exceeds 20 bits")]
fn accessibility_item_ids_reject_indices_that_overlap_the_widget_slot() {
    let mut tree = UiTree::new();
    let id = tree.insert(None, Label::new("item"));
    noa_gpui_core::access_item_id(id, 1 << 20);
}

#[test]
fn controls_take_an_accessible_name_from_the_app_or_their_form_field() {
    use noa_gpui_core::access_id;
    use noa_gpui_core::accesskit::Node;
    use noa_gpui_core::widgets::{FormField, Switch};

    let mut text = text_store();
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()));
    let switch = tree.insert(Some(root), Switch::new(true));
    let field = tree.insert(Some(root), FormField::new("Email"));
    let input = tree.insert(Some(field), TextField::new("name@example.com", 200.0));
    let named = tree.insert(Some(field), Button::new("Verify"));
    let save = tree.insert(Some(root), Button::new("Save"));
    tree.prepare(VIEWPORT, &mut text);
    let node = |tree: &UiTree, id: WidgetId| -> Node {
        let update = tree.accessibility_update(1.0);
        let (_, node) = update
            .nodes
            .iter()
            .find(|(n, _)| *n == access_id(id))
            .unwrap();
        node.clone()
    };

    assert_eq!(node(&tree, switch).label(), None);
    // The form field's label names the text field inside it, as an HTML <label> does.
    assert_eq!(node(&tree, input).label(), None);
    assert_eq!(node(&tree, input).labelled_by(), [access_id(field)]);
    assert_eq!(node(&tree, named).label(), Some("Verify"));
    assert!(node(&tree, named).labelled_by().is_empty());

    // The app names a control; the name replaces the widget's own and needs a frame,
    // which is when the platform sends the tree again.
    tree.set_accessible_label(switch, "Wi-Fi");
    tree.set_accessible_label(save, "Save document");
    assert!(tree.prepare(VIEWPORT, &mut text).is_some());
    assert_eq!(node(&tree, switch).label(), Some("Wi-Fi"));
    assert_eq!(node(&tree, save).label(), Some("Save document"));
    assert_eq!(tree.accessible_label(switch), Some("Wi-Fi"));
    tree.set_accessible_label(switch, "Wi-Fi");
    assert!(tree.prepare(VIEWPORT, &mut text).is_none());

    let subtree = tree.take_subtree(switch).unwrap();
    let moved = tree.insert_subtree(Some(root), subtree);
    tree.prepare(VIEWPORT, &mut text);
    assert_eq!(node(&tree, moved).label(), Some("Wi-Fi"));

    tree.set_accessible_label(save, "");
    tree.prepare(VIEWPORT, &mut text);
    assert_eq!(node(&tree, save).label(), Some("Save"));
    assert_eq!(tree.accessible_label(save), None);
}

/// The corner radius and the number of commands a lone button paints in `design`, and
/// whether it laid out the same size.
fn button_look(design: Design) -> (f32, usize, Size) {
    let mut text = text_store();
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()));
    let button = tree.insert(Some(root), Button::new("OK"));
    tree.set_theme(Theme::light().with_design(design));
    tree.prepare(VIEWPORT, &mut text);
    let commands = tree.display_list().commands();
    let radius = commands
        .iter()
        .find_map(|c| match c {
            DrawCommand::Quad { radius, .. } => Some(radius[0]),
            _ => None,
        })
        .unwrap();
    let rect = tree.window_rect(button).unwrap();
    (radius, commands.len(), Size::new(rect.width, rect.height))
}

#[test]
fn designs_reshape_widgets_without_resizing_them() {
    let (flat_r, flat_n, flat_size) = button_look(Design::FlatVibrancy);
    let (sur_r, sur_n, sur_size) = button_look(Design::BigSur);
    let (glass_r, glass_n, glass_size) = button_look(Design::LiquidGlass);
    assert_eq!(sur_r, 6.0);
    assert!(flat_r < sur_r);
    assert_eq!(glass_r, glass_size.height / 2.0, "a capsule");
    assert_eq!(flat_n, sur_n);
    assert_eq!(glass_n, sur_n + 2, "a sheen and a rim");
    assert_eq!(flat_size, sur_size);
    assert_eq!(glass_size, sur_size);
    // Noa: tight corners, flat (no sheen), the same size.
    let (noa_r, noa_n, noa_size) = button_look(Design::Noa);
    assert_eq!(noa_r, 4.0);
    assert_eq!(noa_n, sur_n);
    assert_eq!(noa_size, sur_size);
}

#[test]
fn noa_draws_switch_knobs_as_cursor_blocks() {
    use noa_gpui_core::widgets::Switch;
    // The knob's (width, height, corner radius): the last quad a switch paints.
    let knob = |design: Design| {
        let mut text = text_store();
        let mut tree = UiTree::new();
        let root = tree.insert(None, Container::new(column()));
        tree.insert(Some(root), Switch::new(false));
        tree.set_theme(Theme::dark().with_design(design));
        tree.prepare(VIEWPORT, &mut text);
        tree.display_list()
            .commands()
            .iter()
            .rev()
            .find_map(|c| match c {
                DrawCommand::Quad { rect, radius, .. } => {
                    Some((rect.width, rect.height, radius[0]))
                }
                _ => None,
            })
            .unwrap()
    };
    let (w, h, r) = knob(Design::BigSur);
    assert_eq!((w, r), (h, h / 2.0), "a round knob");
    let (noa_w, noa_h, noa_r) = knob(Design::Noa);
    assert_eq!(noa_h, h);
    assert_eq!((noa_w, noa_r), (h * 0.6, 2.0), "a block, taller than wide");
}

#[test]
fn materials_blur_their_backdrop_except_in_metro() {
    use noa_gpui_core::widgets::{Material, VisualEffectView};
    let backdrops = |design: Design| {
        let mut text = text_store();
        let mut tree = UiTree::new();
        tree.set_theme(Theme::light().with_design(design));
        let root = tree.insert(None, Container::new(column()));
        tree.insert(
            Some(root),
            VisualEffectView::new(fixed(100.0, 50.0), Material::Sidebar),
        );
        tree.prepare(VIEWPORT, &mut text).unwrap();
        let commands = tree.display_list().commands();
        let count = |f: fn(&DrawCommand) -> bool| commands.iter().filter(|c| f(c)).count();
        (
            count(|c| matches!(c, DrawCommand::Backdrop { .. })),
            count(|c| matches!(c, DrawCommand::Quad { color, .. } if color.a == 1.0)),
        )
    };
    assert_eq!(backdrops(Design::BigSur).0, 1);
    // Metro's materials are opaque without blur: a plain fill, no offscreen passes.
    let (blurred, opaque) = backdrops(Design::Metro);
    assert_eq!(blurred, 0);
    assert!(opaque >= 1);
}

/// A leaf that, like many widgets, reads state from its last measure when it paints.
#[derive(Default)]
struct NeedsMeasure {
    measured: bool,
    paints: u32,
}

impl noa_gpui_core::Widget for NeedsMeasure {
    fn style(&self) -> Style {
        Style::default()
    }

    fn measures(&self) -> bool {
        true
    }

    fn measure(
        &mut self,
        _cx: &mut noa_gpui_core::MeasureCx,
        _input: noa_gpui_core::MeasureInput,
    ) -> Size {
        self.measured = true;
        Size::new(20.0, 10.0)
    }

    fn paint(&mut self, _cx: &mut noa_gpui_core::PaintCx) {
        assert!(self.measured, "painted before it was measured");
        self.paints += 1;
    }
}

#[test]
fn widgets_created_hidden_are_measured_before_they_paint() {
    let mut text = text_store();
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()));
    let alone = tree.insert(Some(root), NeedsMeasure::default());
    tree.set_hidden(alone, true);
    let parent = tree.insert(Some(root), Container::new(Style::default()));
    let nested = tree.insert(Some(parent), NeedsMeasure::default());
    tree.set_hidden(parent, true);
    let tabs = tree.insert(Some(root), noa_gpui_core::widgets::TabView::new(["A", "B"]));
    let _first = tree.insert(Some(tabs), Label::new("first"));
    let page = tree.insert(Some(tabs), NeedsMeasure::default());
    tree.prepare(VIEWPORT, &mut text);
    let paints = |tree: &UiTree, id| tree.get::<NeedsMeasure>(id).unwrap().paints;
    for id in [alone, nested, page] {
        assert_eq!(paints(&tree, id), 0);
    }

    tree.set_hidden(alone, false);
    tree.set_hidden(parent, false);
    tree.update::<noa_gpui_core::widgets::TabView, _>(tabs, |t, cx| t.set_selected(cx, 1));
    tree.prepare(VIEWPORT, &mut text);
    for id in [alone, nested, page] {
        assert_eq!(paints(&tree, id), 1);
        assert_eq!(tree.window_rect(id).unwrap().height, 10.0);
    }
}

struct Lifted;

impl noa_gpui_core::Widget for Lifted {
    fn style(&self) -> Style {
        fixed(100.0, 40.0)
    }
    fn paint(&mut self, cx: &mut noa_gpui_core::PaintCx) {
        let shadow = noa_gpui_scene::BoxShadow {
            color: Color::rgba(0.0, 0.0, 0.0, 0.3),
            offset: Point::new(0.0, 4.0),
            blur: 12.0,
            spread: 0.0,
        };
        cx.box_shadow(cx.bounds(), [8.0; 4], &shadow);
        // A transparent shadow draws nothing.
        let none = noa_gpui_scene::BoxShadow {
            color: Color::TRANSPARENT,
            ..shadow
        };
        cx.box_shadow(cx.bounds(), [8.0; 4], &none);
    }
}

#[test]
fn a_box_shadow_is_one_shape_in_window_coordinates() {
    let mut text = text_store();
    let mut tree = UiTree::new();
    let root = tree.insert(None, Container::new(column()));
    tree.insert(Some(root), Container::new(fixed(100.0, 30.0)));
    let lifted = tree.insert(Some(root), Lifted);
    tree.prepare(VIEWPORT, &mut text);
    let rect = tree.window_rect(lifted).unwrap();
    let shadows: Vec<_> = tree
        .display_list()
        .commands()
        .iter()
        .filter_map(|c| match c {
            DrawCommand::Shape {
                shape: noa_gpui_scene::Shape::Shadow { rect, radius, blur },
                color,
            } => Some((*rect, *radius, *blur, *color)),
            _ => None,
        })
        .collect();
    assert_eq!(
        shadows,
        [(
            rect.translate(0.0, 4.0),
            [8.0; 4],
            12.0,
            Color::rgba(0.0, 0.0, 0.0, 0.3)
        )]
    );
}

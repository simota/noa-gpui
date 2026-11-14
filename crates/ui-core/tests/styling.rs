use ui_core::style::{FlexDirection, Size as LayoutSize, length, percent};
use ui_core::widgets::{Button, Checkbox, Container, Label, TextArea, TextField, TextView};
use ui_core::{Event, Role, Style, StyleMeta, StylePatch, StyleSheet, TextProperty, UiTree};
use ui_scene::{Color, DrawCommand, Point, Size};
use ui_text::{FontOptions, TextStore};

const VIEWPORT: Size = Size::new(400.0, 300.0);

fn text_store() -> TextStore {
    TextStore::new(FontOptions {
        locale: "en-US".into(),
        load_system_fonts: false,
    })
}

fn root(tree: &mut UiTree) -> ui_core::WidgetId {
    tree.insert(
        None,
        Container::new(Style {
            flex_direction: FlexDirection::Column,
            size: LayoutSize {
                width: percent(1.0),
                height: percent(1.0),
            },
            ..Default::default()
        }),
    )
}

#[test]
fn stylesheet_overrides_layout_and_container_paint_then_restores_defaults() {
    let mut tree = UiTree::new();
    let root = root(&mut tree);
    let child = tree.insert(
        Some(root),
        Container::new(Style {
            size: LayoutSize {
                width: length(44.0),
                height: length(20.0),
            },
            flex_shrink: 0.0,
            ..Default::default()
        }),
    );
    tree.set_style_meta(root, StyleMeta::new().class("panel"));
    tree.set_style_meta(child, StyleMeta::new().id("item"));
    let sheet = StyleSheet::parse(
        r#"
        .panel { padding: 12px; gap: 8px; }
        #item {
            width: 80px;
            height: 24px;
            background-color: theme(accent);
            border: 1px solid theme(outline);
            border-radius: 6px;
        }
        "#,
    )
    .unwrap();
    tree.set_stylesheet(sheet);
    let mut text = text_store();
    tree.prepare(VIEWPORT, &mut text).unwrap();

    let rect = tree.layout(child).unwrap().rect;
    assert_eq!(rect.width, 80.0);
    assert_eq!(rect.height, 24.0);
    assert_eq!(rect.x, 12.0);
    assert_eq!(rect.y, 12.0);
    let accent = tree.theme().color(Role::Accent);
    assert!(tree.display_list().commands().iter().any(|command| {
        matches!(
            command,
            DrawCommand::Quad { color, border, .. }
                if *color == accent && border.width == 1.0
        )
    }));

    tree.set_stylesheet(StyleSheet::default());
    tree.prepare(VIEWPORT, &mut text).unwrap();
    let restored = tree.layout(child).unwrap().rect;
    assert_eq!(restored.width, 44.0);
    assert_eq!(restored.height, 20.0);
    assert_eq!(restored.x, 0.0);
    assert_eq!(restored.y, 0.0);
    assert!(tree.display_list().commands().is_empty());
}

#[test]
fn hover_restyles_without_relayout() {
    let mut tree = UiTree::new();
    let root = root(&mut tree);
    let tile = tree.insert(
        Some(root),
        Container::new(Style {
            size: LayoutSize {
                width: length(100.0),
                height: length(40.0),
            },
            ..Default::default()
        }),
    );
    tree.set_style_meta(tile, StyleMeta::new().class("tile"));
    tree.set_stylesheet(
        StyleSheet::parse(
            r#"
            .tile { background-color: #123456; }
            .tile:hover { background-color: theme(accent); }
            "#,
        )
        .unwrap(),
    );
    let mut text = text_store();
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(
        tree.computed_style(tile).unwrap().background_color,
        Some(Color::hex(0x123456).into())
    );

    tree.dispatch(
        Event::PointerMove {
            pos: Point::new(10.0, 10.0),
        },
        &mut text,
    );
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(
        tree.computed_style(tile).unwrap().background_color,
        Some(Role::Accent.into())
    );
    assert!(!tree.stats().layout);

    tree.dispatch(
        Event::PointerMove {
            pos: Point::new(200.0, 200.0),
        },
        &mut text,
    );
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(
        tree.computed_style(tile).unwrap().background_color,
        Some(Color::hex(0x123456).into())
    );
    assert!(!tree.stats().layout);
}

#[test]
fn focus_and_disabled_selectors_follow_tree_state() {
    let mut tree = UiTree::new();
    let root = root(&mut tree);
    let button = tree.insert(Some(root), Button::new("Save"));
    tree.set_style_meta(button, StyleMeta::new().class("action"));
    tree.set_stylesheet(
        StyleSheet::parse(
            r#"
            .action { color: theme(text); }
            .action:focus { color: theme(accent); }
            .action:disabled { color: theme(muted); }
            "#,
        )
        .unwrap(),
    );
    let mut text = text_store();
    tree.prepare(VIEWPORT, &mut text).unwrap();
    tree.set_focus(Some(button));
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(
        tree.computed_style(button).unwrap().color,
        Some(Role::Accent.into())
    );
    tree.set_enabled(button, false);
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(
        tree.computed_style(button).unwrap().color,
        Some(Role::Muted.into())
    );
    tree.set_enabled(button, true);
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(
        tree.computed_style(button).unwrap().color,
        Some(Role::Text.into())
    );
}

#[test]
fn inline_style_wins_and_can_be_removed() {
    let mut tree = UiTree::new();
    let root = root(&mut tree);
    let child = tree.insert(Some(root), Container::new(Style::default()));
    tree.set_style_meta(child, StyleMeta::new().class("box"));
    tree.set_stylesheet(StyleSheet::parse(".box { width: 40px; }").unwrap());
    tree.set_inline_style(child, StylePatch::parse("width: 80px;").unwrap());

    let mut text = text_store();
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(tree.layout(child).unwrap().rect.width, 80.0);

    tree.set_inline_style(child, StylePatch::default());
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(tree.layout(child).unwrap().rect.width, 40.0);
    assert!(tree.prepare(VIEWPORT, &mut text).is_none());
}

#[test]
fn label_font_size_remeasures_and_color_overrides_native_value() {
    let mut tree = UiTree::new();
    let root = root(&mut tree);
    let label = tree.insert(Some(root), Label::new("Hello").color(Color::BLACK));
    tree.set_style_meta(label, StyleMeta::new().id("heading"));

    let mut text = text_store();
    tree.prepare(VIEWPORT, &mut text).unwrap();
    let base = tree.layout(label).unwrap().rect.height;
    tree.set_stylesheet(
        StyleSheet::parse("#heading { font-size: 30px; color: #123abc; }").unwrap(),
    );
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert!(tree.layout(label).unwrap().rect.height > base);
    assert!(tree.display_list().commands().iter().any(|command| {
        matches!(command, DrawCommand::Text { color, .. }
            if *color == Color::hex(0x123abc))
    }));
    tree.set_stylesheet(StyleSheet::default());
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(tree.layout(label).unwrap().rect.height, base);
}

#[test]
fn moving_a_subtree_keeps_selector_metadata_and_inline_declarations() {
    let mut first = UiTree::new();
    let root_id = root(&mut first);
    let item = first.insert(Some(root_id), Container::new(Style::default()));
    first.set_style_meta(item, StyleMeta::new().class("tile"));
    first.set_inline_style(item, StylePatch::parse("height: 18px;").unwrap());
    let subtree = first.take_subtree(item).unwrap();

    let mut second = UiTree::new();
    let parent = root(&mut second);
    second.set_stylesheet(StyleSheet::parse(".tile { width: 67px; }").unwrap());
    let restored = second.insert_subtree(Some(parent), subtree);
    let mut text = text_store();
    second.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(second.layout(restored).unwrap().rect.width, 67.0);
    assert_eq!(second.layout(restored).unwrap().rect.height, 18.0);
}

fn sized(width: f32, height: f32) -> Container {
    Container::new(Style {
        size: LayoutSize {
            width: length(width),
            height: length(height),
        },
        flex_shrink: 0.0,
        ..Default::default()
    })
}

#[test]
fn descendant_and_child_selectors_follow_inserts_and_class_changes() {
    let mut tree = UiTree::new();
    let root = root(&mut tree);
    tree.set_stylesheet(
        StyleSheet::parse(
            r#"
            .panel Container { width: 70px; }
            .panel > Container { height: 30px; }
            "#,
        )
        .unwrap(),
    );
    let panel = tree.insert(Some(root), sized(200.0, 100.0));
    let child = tree.insert(Some(panel), sized(10.0, 10.0));
    let grandchild = tree.insert(Some(child), sized(10.0, 10.0));
    let mut text = text_store();
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(tree.layout(child).unwrap().rect.width, 10.0);

    tree.set_style_meta(panel, StyleMeta::new().class("panel"));
    tree.prepare(VIEWPORT, &mut text).unwrap();
    let rect = tree.layout(child).unwrap().rect;
    assert_eq!((rect.width, rect.height), (70.0, 30.0));
    let rect = tree.layout(grandchild).unwrap().rect;
    assert_eq!((rect.width, rect.height), (70.0, 10.0));

    let late = tree.insert(Some(panel), sized(10.0, 10.0));
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(tree.layout(late).unwrap().rect.width, 70.0);

    tree.set_style_meta(panel, StyleMeta::new());
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(tree.layout(grandchild).unwrap().rect.width, 10.0);
}

#[test]
fn ancestor_hover_restyles_descendants_and_leaves_siblings_alone() {
    let mut tree = UiTree::new();
    let root = root(&mut tree);
    let card = tree.insert(Some(root), sized(100.0, 40.0));
    let inner = tree.insert(Some(card), sized(20.0, 20.0));
    let other = tree.insert(Some(root), sized(100.0, 40.0));
    let other_inner = tree.insert(Some(other), sized(20.0, 20.0));
    for id in [card, other] {
        tree.set_style_meta(id, StyleMeta::new().class("card"));
    }
    tree.set_stylesheet(
        StyleSheet::parse(".card:hover > Container { background-color: theme(accent); }").unwrap(),
    );
    let mut text = text_store();
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert!(tree.computed_style(inner).unwrap().is_empty());

    // Over the card but outside the inner box.
    tree.dispatch(
        Event::PointerMove {
            pos: Point::new(60.0, 30.0),
        },
        &mut text,
    );
    assert_eq!(
        tree.computed_style(inner).unwrap().background_color,
        Some(Role::Accent.into())
    );
    assert!(tree.computed_style(other_inner).unwrap().is_empty());

    tree.dispatch(
        Event::PointerMove {
            pos: Point::new(60.0, 70.0),
        },
        &mut text,
    );
    assert!(tree.computed_style(inner).unwrap().is_empty());
    assert_eq!(
        tree.computed_style(other_inner).unwrap().background_color,
        Some(Role::Accent.into())
    );
}

#[test]
fn active_follows_the_pressed_pointer() {
    let mut tree = UiTree::new();
    let root = root(&mut tree);
    let button = tree.insert(Some(root), Button::new("Save"));
    tree.set_stylesheet(
        StyleSheet::parse(
            r#"
            Button:active { background-color: theme(accent-pressed); }
            :root:active { border-color: theme(accent); }
            "#,
        )
        .unwrap(),
    );
    let mut text = text_store();
    tree.prepare(VIEWPORT, &mut text).unwrap();
    let rect = tree.layout(button).unwrap().rect;
    let pos = Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);

    tree.dispatch(Event::PointerMove { pos }, &mut text);
    tree.dispatch(
        Event::PointerDown {
            pos,
            button: ui_core::PointerButton::Primary,
        },
        &mut text,
    );
    assert_eq!(
        tree.computed_style(button).unwrap().background_color,
        Some(Role::AccentPressed.into())
    );
    assert_eq!(
        tree.computed_style(root).unwrap().border_color,
        Some(Role::Accent.into())
    );
    tree.dispatch(
        Event::PointerUp {
            pos,
            button: ui_core::PointerButton::Primary,
        },
        &mut text,
    );
    assert!(tree.computed_style(button).unwrap().is_empty());
    assert!(tree.computed_style(root).unwrap().is_empty());
}

#[test]
fn flex_alignment_and_absolute_position_reach_layout() {
    let mut tree = UiTree::new();
    let root = root(&mut tree);
    let row = tree.insert(Some(root), sized(200.0, 100.0));
    let fixed = tree.insert(Some(row), sized(50.0, 20.0));
    let grow = tree.insert(Some(row), sized(10.0, 20.0));
    let pinned = tree.insert(Some(row), sized(10.0, 10.0));
    tree.set_style_meta(row, StyleMeta::new().id("row"));
    tree.set_style_meta(grow, StyleMeta::new().class("grow"));
    tree.set_style_meta(pinned, StyleMeta::new().class("pinned"));
    tree.set_stylesheet(
        StyleSheet::parse(
            r#"
            :root { --inset: 4px; }
            #row { flex-direction: row; align-items: center; }
            .grow { flex: 1; max-width: 100px; margin-left: -10px; }
            .pinned { position: absolute; right: var(--inset); bottom: var(--inset); }
            "#,
        )
        .unwrap(),
    );
    let mut text = text_store();
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(tree.layout(fixed).unwrap().rect.y, 40.0);
    let rect = tree.layout(grow).unwrap().rect;
    assert_eq!((rect.x, rect.width), (40.0, 100.0));
    let rect = tree.layout(pinned).unwrap().rect;
    assert_eq!((rect.x, rect.y), (186.0, 86.0));
}

#[test]
fn important_beats_inline_and_label_takes_weight_and_alignment() {
    let mut tree = UiTree::new();
    let root = root(&mut tree);
    let label = tree.insert(Some(root), Label::new("Hello"));
    tree.set_style_meta(label, StyleMeta::new().class("title"));
    let mut text = text_store();
    tree.prepare(VIEWPORT, &mut text).unwrap();
    let text_x = |tree: &UiTree| {
        tree.display_list()
            .commands()
            .iter()
            .find_map(|command| match command {
                DrawCommand::Text { rect, .. } => Some(rect.x),
                _ => None,
            })
            .unwrap()
    };
    let start = text_x(&tree);

    tree.set_stylesheet(
        StyleSheet::parse(
            ".title { width: 200px !important; text-align: right; font-weight: bold; }",
        )
        .unwrap(),
    );
    tree.set_inline_style(label, StylePatch::parse("width: 90px;").unwrap());
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(tree.layout(label).unwrap().rect.width, 200.0);
    assert_eq!(tree.computed_style(label).unwrap().font_weight, Some(700));
    assert!(text_x(&tree) > start + 100.0);
}

#[test]
fn text_properties_inherit_but_not_into_buttons() {
    let mut tree = UiTree::new();
    let root = root(&mut tree);
    let panel = tree.insert(Some(root), sized(200.0, 100.0));
    let inner = tree.insert(Some(panel), sized(100.0, 50.0));
    let label = tree.insert(Some(inner), Label::new("Hello"));
    let own = tree.insert(Some(inner), Label::new("Own"));
    let button = tree.insert(Some(inner), Button::new("Save"));
    tree.set_style_meta(panel, StyleMeta::new().class("panel"));
    tree.set_style_meta(own, StyleMeta::new().class("own"));
    tree.set_stylesheet(
        StyleSheet::parse(
            r#"
            .panel { color: #123456; font-size: 30px; width: 150px; }
            .own { color: theme(accent); }
            "#,
        )
        .unwrap(),
    );
    let mut text = text_store();
    tree.prepare(VIEWPORT, &mut text).unwrap();
    let style = tree.computed_style(label).unwrap();
    assert_eq!(style.color, Some(Color::hex(0x123456).into()));
    assert_eq!(style.font_size, Some(30.0));
    assert_eq!(style.width, None);
    let own_style = tree.computed_style(own).unwrap();
    assert_eq!(own_style.color, Some(Role::Accent.into()));
    assert_eq!(own_style.font_size, Some(30.0));
    assert!(tree.computed_style(button).unwrap().is_empty());
    assert!(tree.display_list().commands().iter().any(|command| {
        matches!(command, DrawCommand::Text { color, .. } if *color == Color::hex(0x123456))
    }));

    let late = tree.insert(Some(inner), Label::new("Late"));
    assert_eq!(tree.computed_style(late).unwrap().font_size, Some(30.0));
    tree.set_inline_style(panel, StylePatch::parse("font-size: 12px;").unwrap());
    assert_eq!(tree.computed_style(late).unwrap().font_size, Some(12.0));
    tree.set_inline_style(panel, StylePatch::default());
    tree.set_stylesheet(StyleSheet::default());
    assert!(tree.computed_style(label).unwrap().is_empty());
}

#[test]
fn a_labels_own_settings_outrank_inherited_values_but_not_its_own_rules() {
    let mut tree = UiTree::new();
    let root = root(&mut tree);
    let panel = tree.insert(Some(root), Container::column());
    let plain = tree.insert(Some(panel), Label::new("Plain"));
    let title = tree.insert(
        Some(panel),
        Label::new("Title").font_size(22.0).color(Color::BLACK),
    );
    tree.set_style_meta(panel, StyleMeta::new().class("panel"));
    let mut text = text_store();
    tree.prepare(VIEWPORT, &mut text).unwrap();
    let (plain_height, title_height) = (
        tree.layout(plain).unwrap().rect.height,
        tree.layout(title).unwrap().rect.height,
    );
    let drawn = |tree: &UiTree, wanted: Color| {
        tree.display_list()
            .commands()
            .iter()
            .filter(|c| matches!(c, DrawCommand::Text { color, .. } if *color == wanted))
            .count()
    };

    tree.set_stylesheet(StyleSheet::parse(".panel { font-size: 10px; color: #123456; }").unwrap());
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert!(tree.layout(plain).unwrap().rect.height < plain_height);
    assert_eq!(tree.layout(title).unwrap().rect.height, title_height);
    assert_eq!(drawn(&tree, Color::hex(0x123456)), 1);
    assert_eq!(drawn(&tree, Color::BLACK), 1);
    let computed = tree.computed_style(title).unwrap();
    assert_eq!(computed.font_size, Some(10.0));
    assert!(computed.is_inherited(TextProperty::FontSize));

    tree.set_style_meta(title, StyleMeta::new().class("title"));
    tree.set_stylesheet(
        StyleSheet::parse(".panel { font-size: 10px; color: #123456; } .title { color: #abcdef; }")
            .unwrap(),
    );
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert!(
        !tree
            .computed_style(title)
            .unwrap()
            .is_inherited(TextProperty::Color)
    );
    assert_eq!(drawn(&tree, Color::hex(0xabcdef)), 1);
    assert_eq!(tree.layout(title).unwrap().rect.height, title_height);

    // Setting the plain label's color to the value it already stores still makes it
    // its own, which changes what is drawn: that needs a frame.
    tree.update::<Label, _>(plain, |l, cx| l.set_color(cx, Role::Text));
    assert!(tree.prepare(VIEWPORT, &mut text).is_some());
    assert_eq!(drawn(&tree, Color::hex(0x123456)), 0);
    // Choosing the font it already stores makes the size its own too: relaid out.
    let inherited_height = tree.layout(plain).unwrap().rect.height;
    tree.update::<Label, _>(plain, |l, cx| l.set_font(cx, None, 14.0, 400, false));
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(tree.layout(plain).unwrap().rect.height, plain_height);
    assert!(inherited_height < plain_height);
}

#[test]
fn a_page_a_tab_view_hides_loses_focus_within_and_hover() {
    use ui_core::widgets::TabView;
    let mut tree = UiTree::new();
    let root = root(&mut tree);
    let tabs = tree.insert(Some(root), TabView::new(["A", "B"]).width(300.0));
    let page = tree.insert(Some(tabs), Container::column());
    let field = tree.insert(Some(page), TextField::new("", 200.0));
    tree.insert(Some(tabs), Label::new("second"));
    tree.set_style_meta(page, StyleMeta::new().class("page"));
    tree.set_stylesheet(
        StyleSheet::parse(
            ".page:focus-within { background-color: #ff0000; } .page:hover { color: #00ff00; }",
        )
        .unwrap(),
    );
    let mut text = text_store();
    tree.prepare(VIEWPORT, &mut text).unwrap();
    let center = {
        let r = tree.window_rect(field).unwrap();
        Point::new(r.x + r.width / 2.0, r.y + r.height / 2.0)
    };
    tree.dispatch(Event::PointerMove { pos: center }, &mut text);
    tree.set_focus(Some(field));
    tree.prepare(VIEWPORT, &mut text).unwrap();
    let style = tree.computed_style(page).unwrap().clone();
    assert_eq!(style.background_color, Some(Color::hex(0xff0000).into()));
    assert_eq!(style.color, Some(Color::hex(0x00ff00).into()));

    tree.update::<TabView, _>(tabs, |t, cx| t.set_selected(cx, 1));
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(tree.focused(), None);
    let style = tree.computed_style(page).unwrap();
    assert_eq!(style.background_color, None, "no longer :focus-within");
    assert_eq!(style.color, None, "no longer :hover");
    // The field hears that it lost focus in the next frame, which the tree asks for.
    assert!(tree.needs_update(VIEWPORT));
    tree.prepare(VIEWPORT, &mut text);
    assert!(!tree.needs_update(VIEWPORT));
}

#[test]
fn display_none_hides_and_opacity_and_overflow_reach_the_frame() {
    let mut tree = UiTree::new();
    let root = root(&mut tree);
    let first = tree.insert(Some(root), sized(100.0, 40.0));
    let second = tree.insert(Some(root), sized(100.0, 40.0));
    let inner = tree.insert(Some(second), sized(300.0, 10.0));
    tree.set_style_meta(first, StyleMeta::new().class("first"));
    tree.set_style_meta(second, StyleMeta::new().class("second"));
    tree.set_style_meta(inner, StyleMeta::new().class("inner"));
    let mut text = text_store();
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(tree.layout(second).unwrap().rect.y, 40.0);

    tree.set_stylesheet(
        StyleSheet::parse(
            r#"
            .first { display: none; }
            .second { opacity: 0.5; overflow: hidden; }
            .inner { background-color: #ff0000; }
            "#,
        )
        .unwrap(),
    );
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert!(tree.is_hidden(first));
    assert_eq!(tree.layout(second).unwrap().rect.y, 0.0);
    let commands = tree.display_list().commands();
    assert!(commands.iter().any(|command| {
        matches!(command, DrawCommand::Quad { color, .. }
            if *color == Color::hex(0xff0000).with_alpha(0.5))
    }));
    assert!(commands.iter().any(|command| {
        matches!(command, DrawCommand::PushClip { rect } if rect.width == 100.0)
    }));

    tree.set_inline_style(first, StylePatch::parse("display: flex;").unwrap());
    tree.set_inline_style(second, StylePatch::parse("opacity: 1;").unwrap());
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert!(!tree.is_hidden(first));
    assert_eq!(tree.layout(second).unwrap().rect.y, 40.0);
    assert!(tree.display_list().commands().iter().any(|command| {
        matches!(command, DrawCommand::Quad { color, .. } if *color == Color::hex(0xff0000))
    }));
}

#[test]
fn media_rules_switch_when_the_viewport_resizes() {
    let mut tree = UiTree::new();
    let root = root(&mut tree);
    let side = tree.insert(Some(root), sized(10.0, 10.0));
    tree.set_style_meta(side, StyleMeta::new().class("side"));
    tree.set_stylesheet(
        StyleSheet::parse(
            r#"
            .side { width: 200px; }
            @media (max-width: 300px) { .side { width: 80px; } }
            "#,
        )
        .unwrap(),
    );
    let mut text = text_store();
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(tree.layout(side).unwrap().rect.width, 200.0);
    tree.prepare(Size::new(250.0, 300.0), &mut text).unwrap();
    assert_eq!(tree.layout(side).unwrap().rect.width, 80.0);
    tree.set_stylesheet(
        StyleSheet::parse("@media (max-width: 300px) { .side { width: 60px; } }").unwrap(),
    );
    assert_eq!(
        tree.computed_style(side).unwrap().width,
        Some(ui_core::Length::Px(60.0))
    );
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(tree.layout(side).unwrap().rect.width, 10.0);
}

#[test]
fn positions_and_siblings_follow_structure_and_state() {
    let mut tree = UiTree::new();
    let root = root(&mut tree);
    tree.set_stylesheet(
        StyleSheet::parse(
            r#"
            Container > Container:last-child { height: 30px; }
            Container > Container:nth-child(odd) { width: 50px; }
            .lead:hover + Container { background-color: theme(accent); }
            "#,
        )
        .unwrap(),
    );
    let a = tree.insert(Some(root), sized(100.0, 10.0));
    let b = tree.insert(Some(root), sized(100.0, 10.0));
    let c = tree.insert(Some(root), sized(100.0, 10.0));
    tree.set_style_meta(a, StyleMeta::new().class("lead"));
    let mut text = text_store();
    tree.prepare(VIEWPORT, &mut text).unwrap();
    let size = |tree: &UiTree, id| {
        let rect = tree.layout(id).unwrap().rect;
        (rect.width, rect.height)
    };
    assert_eq!(size(&tree, a), (50.0, 10.0));
    assert_eq!(size(&tree, b), (100.0, 10.0));
    assert_eq!(size(&tree, c), (50.0, 30.0));

    tree.remove(c, &mut text);
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(size(&tree, b), (100.0, 30.0));

    tree.move_child(b, 0);
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(size(&tree, b), (50.0, 10.0));
    assert_eq!(size(&tree, a), (100.0, 30.0));
    tree.move_child(a, 0);
    tree.prepare(VIEWPORT, &mut text).unwrap();

    tree.dispatch(
        Event::PointerMove {
            pos: Point::new(5.0, 5.0),
        },
        &mut text,
    );
    assert_eq!(
        tree.computed_style(b).unwrap().background_color,
        Some(Role::Accent.into())
    );
    tree.dispatch(
        Event::PointerMove {
            pos: Point::new(5.0, 200.0),
        },
        &mut text,
    );
    assert_eq!(tree.computed_style(b).unwrap().background_color, None);
}

#[test]
fn focus_visible_is_for_focus_that_did_not_come_from_a_press() {
    let mut tree = UiTree::new();
    let root = root(&mut tree);
    let first = tree.insert(Some(root), Button::new("One"));
    let second = tree.insert(Some(root), Button::new("Two"));
    tree.set_stylesheet(
        StyleSheet::parse("Button:focus-visible { border-color: theme(focus-ring); }").unwrap(),
    );
    let mut text = text_store();
    tree.prepare(VIEWPORT, &mut text).unwrap();
    let ring = |tree: &UiTree, id| tree.computed_style(id).unwrap().border_color.is_some();

    let rect = tree.layout(first).unwrap().rect;
    let pos = Point::new(rect.x + 4.0, rect.y + 4.0);
    let button = ui_core::PointerButton::Primary;
    tree.dispatch(Event::PointerDown { pos, button }, &mut text);
    tree.set_focus(Some(first));
    assert!(!ring(&tree, first));
    tree.dispatch(Event::PointerUp { pos, button }, &mut text);

    tree.dispatch(
        Event::KeyDown {
            key: ui_core::Key::Tab,
            modifiers: Default::default(),
        },
        &mut text,
    );
    tree.set_focus(Some(second));
    assert!(ring(&tree, second));
    assert!(!ring(&tree, first));
}

#[test]
fn removing_a_node_clears_the_state_it_gave_its_ancestors() {
    let sheet = || {
        StyleSheet::parse(
            r#"
            :root:active { background-color: red; }
            :root:hover { border-color: blue; }
            :root:focus-within { color: green; }
            "#,
        )
        .unwrap()
    };
    let button = ui_core::PointerButton::Primary;
    for take in [false, true] {
        let mut tree = UiTree::new();
        let root = root(&mut tree);
        let target = tree.insert(Some(root), Button::new("Save"));
        tree.set_stylesheet(sheet());
        let mut text = text_store();
        tree.prepare(VIEWPORT, &mut text).unwrap();
        let rect = tree.layout(target).unwrap().rect;
        let pos = Point::new(rect.x + 4.0, rect.y + 4.0);
        tree.dispatch(Event::PointerMove { pos }, &mut text);
        tree.dispatch(Event::PointerDown { pos, button }, &mut text);
        tree.set_focus(Some(target));
        let style = tree.computed_style(root).unwrap();
        assert!(style.background_color.is_some());
        assert!(style.border_color.is_some());
        assert!(style.color.is_some());

        if take {
            tree.take_subtree(target).unwrap();
        } else {
            tree.remove(target, &mut text);
        }
        assert!(
            tree.computed_style(root).unwrap().is_empty(),
            "take: {take}"
        );
        tree.dispatch(Event::PointerUp { pos, button }, &mut text);
        assert!(
            tree.computed_style(root).unwrap().is_empty(),
            "take: {take}"
        );
    }
}

/// Building a list under `~` costs about the square of its length (each row looks
/// back over the earlier ones), not the cube.
#[test]
fn a_long_list_builds_in_reasonable_time_under_sibling_selectors() {
    let mut tree = UiTree::new();
    let root = root(&mut tree);
    tree.set_stylesheet(
        StyleSheet::parse(
            ".lead ~ Container { width: 50px; } Container:nth-child(odd) { height: 4px; }",
        )
        .unwrap(),
    );
    let start = std::time::Instant::now();
    let mut rows = Vec::new();
    for index in 0..3000 {
        let row = tree.insert(Some(root), sized(10.0, 2.0));
        let class = if index == 0 { "lead" } else { "row" };
        tree.set_style_meta(row, StyleMeta::new().class(class));
        rows.push(row);
    }
    let mut text = text_store();
    tree.prepare(Size::new(400.0, 10000.0), &mut text).unwrap();
    let elapsed = start.elapsed();
    let size = |id| {
        let rect = tree.layout(id).unwrap().rect;
        (rect.width, rect.height)
    };
    assert_eq!(size(rows[0]), (10.0, 4.0));
    assert_eq!(size(rows[1]), (50.0, 2.0));
    assert_eq!(size(rows[2998]), (50.0, 4.0));
    assert!(elapsed.as_secs() < 20, "took {elapsed:?}");
}

fn quad_with(tree: &UiTree, fill: Color) -> Option<(f32, f32)> {
    tree.display_list()
        .commands()
        .iter()
        .find_map(|command| match command {
            DrawCommand::Quad {
                color,
                radius,
                border,
                ..
            } if *color == fill => Some((radius[0], border.width)),
            _ => None,
        })
}

#[test]
fn box_properties_reach_widgets_that_do_not_draw_them_and_go_away_again() {
    let mut tree = UiTree::new();
    let root = root(&mut tree);
    let checkbox = tree.insert(Some(root), Checkbox::new("Remember me"));
    tree.set_style_meta(checkbox, StyleMeta::new().class("row"));
    let mut text = text_store();
    tree.prepare(VIEWPORT, &mut text).unwrap();
    let fill = Color::hex(0x204060);
    assert!(quad_with(&tree, fill).is_none());

    tree.set_stylesheet(
        StyleSheet::parse(
            ".row { background-color: #204060; border: 2px solid red; border-radius: 5px; }",
        )
        .unwrap(),
    );
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(quad_with(&tree, fill), Some((5.0, 2.0)));
    let commands = tree.display_list().commands();
    let box_at = commands
        .iter()
        .position(|c| matches!(c, DrawCommand::Quad { color, .. } if *color == fill))
        .unwrap();
    assert!(
        commands[box_at + 1..]
            .iter()
            .any(|c| matches!(c, DrawCommand::Text { .. }))
    );

    tree.set_stylesheet(StyleSheet::default());
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert!(quad_with(&tree, fill).is_none());
}

#[test]
fn widgets_that_draw_their_own_box_are_not_given_a_second_one() {
    let mut tree = UiTree::new();
    let root = root(&mut tree);
    let button = tree.insert(Some(root), Button::new("Save"));
    let field = tree.insert(Some(root), TextField::new("Name", 160.0));
    for id in [button, field] {
        tree.set_style_meta(id, StyleMeta::new().class("tinted"));
    }
    tree.set_stylesheet(StyleSheet::parse(".tinted { background-color: #204060; }").unwrap());
    let mut text = text_store();
    tree.prepare(VIEWPORT, &mut text).unwrap();
    let fill = Color::hex(0x204060);
    let boxes = tree
        .display_list()
        .commands()
        .iter()
        .filter(|c| matches!(c, DrawCommand::Quad { color, .. } if *color == fill))
        .count();
    assert_eq!(boxes, 2, "one fill each, drawn by the widgets themselves");
}

#[test]
fn text_fields_take_the_box_and_text_properties() {
    let mut tree = UiTree::new();
    let root = root(&mut tree);
    let field = tree.insert(Some(root), TextField::new("Name", 160.0));
    tree.set_style_meta(field, StyleMeta::new().id("name"));
    tree.update::<TextField, _>(field, |f, cx| f.set_text(cx, "Ada"));
    let mut text = text_store();
    tree.prepare(VIEWPORT, &mut text).unwrap();
    let base_height = tree.layout(field).unwrap().rect.height;
    let surface = tree.theme().color(Role::Surface);
    assert!(quad_with(&tree, surface).is_some(), "native bezel");

    tree.set_stylesheet(
        StyleSheet::parse(
            "#name { background-color: #fafad2; border: 3px solid #800000; \
             border-radius: 0px; color: #123abc; font-size: 24px; }",
        )
        .unwrap(),
    );
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert!(tree.layout(field).unwrap().rect.height > base_height);
    assert_eq!(quad_with(&tree, Color::hex(0xfafad2)), Some((0.0, 3.0)));
    assert!(tree.display_list().commands().iter().any(|command| {
        matches!(command, DrawCommand::Text { color, .. } if *color == Color::hex(0x123abc))
    }));

    // Text color does not reach a field from its ancestors (like a browser's input).
    tree.set_stylesheet(StyleSheet::parse(":root { color: #123abc; }").unwrap());
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert!(!tree.display_list().commands().iter().any(|command| {
        matches!(command, DrawCommand::Text { color, .. } if *color == Color::hex(0x123abc))
    }));

    tree.set_stylesheet(StyleSheet::default());
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(tree.layout(field).unwrap().rect.height, base_height);
    assert!(quad_with(&tree, surface).is_some());
}

#[test]
fn a_focused_field_keeps_the_accent_unless_a_focus_rule_sets_the_border() {
    let mut tree = UiTree::new();
    let root = root(&mut tree);
    let field = tree.insert(Some(root), TextField::new("Name", 160.0));
    tree.set_stylesheet(
        StyleSheet::parse("TextField { background-color: #fafad2; border-color: #808080; }")
            .unwrap(),
    );
    let mut text = text_store();
    tree.prepare(VIEWPORT, &mut text).unwrap();
    let border_of = |tree: &UiTree| {
        tree.display_list()
            .commands()
            .iter()
            .find_map(|command| match command {
                DrawCommand::Quad { color, border, .. } if *color == Color::hex(0xfafad2) => {
                    Some(*border)
                }
                _ => None,
            })
            .unwrap()
    };
    assert_eq!(border_of(&tree).color, Color::hex(0x808080));

    tree.set_stylesheet(StyleSheet::parse("TextField { background-color: #fafad2; }").unwrap());
    tree.set_focus(Some(field));
    tree.prepare(VIEWPORT, &mut text).unwrap();
    let focused = border_of(&tree);
    assert_eq!(focused.color, tree.theme().color(Role::Accent));
    assert_eq!(focused.width, 2.0);

    tree.set_stylesheet(
        StyleSheet::parse(
            "TextField { background-color: #fafad2; } TextField:focus { border-color: #ff8800; }",
        )
        .unwrap(),
    );
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(border_of(&tree).color, Color::hex(0xff8800));
}

#[test]
fn a_field_widened_by_the_stylesheet_scrolls_inside_its_new_width() {
    let mut tree = UiTree::new();
    let root = root(&mut tree);
    let field = tree.insert(Some(root), TextField::new("", 80.0));
    tree.set_style_meta(field, StyleMeta::new().id("wide"));
    tree.update::<TextField, _>(field, |f, cx| {
        f.set_text(cx, "a sentence that is wider than eighty pixels")
    });
    let mut text = text_store();
    tree.prepare(VIEWPORT, &mut text).unwrap();
    let scrolled = |tree: &UiTree| tree.get::<TextField>(field).unwrap().scroll_x();
    assert!(
        scrolled(&tree) > 0.0,
        "the caret at the end is kept in view"
    );

    tree.set_stylesheet(StyleSheet::parse("#wide { width: 380px; }").unwrap());
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(tree.layout(field).unwrap().rect.width, 380.0);
    assert_eq!(scrolled(&tree), 0.0, "everything fits in the wider box");

    tree.set_stylesheet(StyleSheet::default());
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert_eq!(tree.layout(field).unwrap().rect.width, 80.0);
    assert!(scrolled(&tree) > 0.0);
}

#[test]
fn text_areas_and_text_views_take_color_and_font_size() {
    let mut tree = UiTree::new();
    let root = root(&mut tree);
    let area = tree.insert(Some(root), TextArea::new(""));
    let view = tree.insert(Some(root), TextView::new(120.0));
    tree.update::<TextArea, _>(area, |a, cx| a.set_text(cx, "notes"));
    tree.update::<TextView, _>(view, |v, cx| v.set_text(cx, "body"));
    let mut text = text_store();
    tree.prepare(VIEWPORT, &mut text).unwrap();
    let base = tree.layout(area).unwrap().rect.height;
    tree.set_stylesheet(
        StyleSheet::parse("TextArea, TextView { color: #123abc; font-size: 22px; }").unwrap(),
    );
    tree.prepare(VIEWPORT, &mut text).unwrap();
    assert!(tree.layout(area).unwrap().rect.height > base);
    let inked = tree
        .display_list()
        .commands()
        .iter()
        .filter(|c| matches!(c, DrawCommand::Text { color, .. } if *color == Color::hex(0x123abc)))
        .count();
    assert_eq!(inked, 2);
}
#[test]
fn incomplete_compounds_are_not_accepted_as_a_shorter_selector() {
    for selector in [
        "Label .unused:not(",
        "Container Label:nth-child(",
        "Container .card:not(:not(.selected)",
    ] {
        let source = format!("{selector} {{ color: red; }}");
        assert!(StyleSheet::parse(&source).is_err(), "{selector}");
    }
}

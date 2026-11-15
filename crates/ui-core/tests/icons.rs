use noa_gpui_core::style::{FlexDirection, Size as LayoutSize, percent};
use noa_gpui_core::widgets::{Button, Container, SvgIcon, Toolbar, ToolbarItem};
use noa_gpui_core::{Role, Style, UiTree, WidgetId};
use noa_gpui_scene::{Color, DrawCommand, Size, VectorIcon};
use noa_gpui_text::{FontOptions, TextStore};

const VIEWPORT: Size = Size::new(400.0, 300.0);

fn setup() -> (UiTree, TextStore, WidgetId) {
    let mut tree = UiTree::new();
    let root = tree.insert(
        None,
        Container::new(Style {
            flex_direction: FlexDirection::Column,
            size: LayoutSize {
                width: percent(1.0),
                height: percent(1.0),
            },
            ..Default::default()
        }),
    );
    let text = TextStore::new(FontOptions {
        locale: "en-US".into(),
        load_system_fonts: false,
    });
    (tree, text, root)
}

fn star() -> VectorIcon {
    VectorIcon::from_svg(
        r#"<svg viewBox="0 0 24 24"><path d="M12 2l3 7h7l-6 4 2 8-6-5-6 5 2-8-6-4h7z"/></svg>"#,
    )
    .unwrap()
}

/// The icon commands in the display list: rectangle size and color.
fn icons(tree: &UiTree) -> Vec<(Size, Color)> {
    tree.display_list()
        .commands()
        .iter()
        .filter_map(|c| match c {
            DrawCommand::Icon { rect, color, .. } => {
                Some((Size::new(rect.width, rect.height), *color))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn an_svg_icon_draws_in_the_theme_color_at_its_size() {
    let (mut tree, mut text, root) = setup();
    let icon = tree.insert(Some(root), SvgIcon::new(star(), 24.0).label("Favorite"));
    tree.insert(
        Some(root),
        SvgIcon::new(star(), 16.0)
            .size(32.0, 16.0)
            .color(Role::Accent),
    );
    tree.prepare(VIEWPORT, &mut text);
    let theme = tree.theme().clone();
    assert_eq!(
        icons(&tree),
        [
            (Size::new(24.0, 24.0), theme.color(Role::Text)),
            (Size::new(32.0, 16.0), theme.color(Role::Accent)),
        ]
    );
    let update = tree.accessibility_update(1.0);
    let node = &update
        .nodes
        .iter()
        .find(|(n, _)| *n == noa_gpui_core::access_id(icon))
        .unwrap()
        .1;
    assert_eq!(node.role(), noa_gpui_core::accesskit::Role::Image);
    assert_eq!(node.label(), Some("Favorite"));

    tree.set_enabled(icon, false);
    tree.prepare(VIEWPORT, &mut text);
    let dimmed = icons(&tree)[0].1;
    assert!(dimmed.a < theme.color(Role::Text).a);
}

#[test]
fn buttons_and_toolbar_items_take_vector_icons() {
    let (mut tree, mut text, root) = setup();
    tree.insert(Some(root), Button::new("Favorite").icon(star()));
    tree.insert(Some(root), Button::icon_only(star()));
    tree.insert(
        Some(root),
        Toolbar::new(vec![ToolbarItem::button("Star").image(star())]),
    );
    tree.prepare(VIEWPORT, &mut text);
    let drawn = icons(&tree);
    assert_eq!(drawn.len(), 3, "{drawn:?}");
    let ink = tree.theme().color(Role::Text);
    assert!(drawn.iter().all(|&(_, color)| color == ink), "{drawn:?}");
}

fn emblem() -> VectorIcon {
    VectorIcon::from_svg(
        r##"<svg viewBox="0 0 24 24">
          <circle cx="12" cy="12" r="10" fill="#22aa44"/>
          <path d="M7 12l3 3 7-7" fill="none" stroke="#fff" stroke-width="2"/>
          <circle cx="12" cy="12" r="11" fill="none" stroke="currentColor"/>
        </svg>"##,
    )
    .unwrap()
}

#[test]
fn a_multicolor_svg_icon_draws_each_part_in_its_own_color() {
    let (mut tree, mut text, root) = setup();
    tree.insert(
        Some(root),
        SvgIcon::new(emblem(), 24.0)
            .multicolor(true)
            .color(Role::Accent),
    );
    tree.insert(Some(root), SvgIcon::new(emblem(), 24.0));
    tree.prepare(VIEWPORT, &mut text);
    let theme = tree.theme().clone();
    let size = Size::new(24.0, 24.0);
    assert_eq!(
        icons(&tree),
        [
            // In drawing order: the disc, the check, the ring in the icon's color.
            (size, Color::hex(0x22aa44)),
            (size, Color::WHITE),
            (size, theme.color(Role::Accent)),
            (size, theme.color(Role::Text)),
        ]
    );
}

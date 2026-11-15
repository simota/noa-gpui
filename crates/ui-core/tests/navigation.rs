use noa_gpui_core::style::{AlignItems, FlexDirection, Size as LayoutSize, length, percent};
use noa_gpui_core::widgets::{
    AnchorNav, AnchorSelected, COMMAND_PALETTE_SHORTCUTS, CommandChosen, CommandPalette, Container,
    DIVIDER, Menu, MenuBarChosen, MenuBarMenu, MenuItem, OUTLINE_RAIL_WIDTH, OutlineItem,
    OutlineView, PaletteCommand, PanesChanged, PathControl, PathItem, PathItemClicked,
    PathSeparator, RAIL_DURATION, RecentCommands, ScrollView, SplitPane, SplitView, menu_commands,
    menu_item,
};
use noa_gpui_core::{
    Event, Key, Message, Modifiers, PointerButton, Shortcut, ShortcutPressed, Style, UiTree,
    Widget, WidgetId,
};
use noa_gpui_scene::{DrawCommand, Point, Rect, Size, VectorIcon};
use noa_gpui_text::{FontOptions, TextStore};

const VIEWPORT: Size = Size::new(800.0, 500.0);

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
                align_items: AlignItems::START,
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
        };
        h.prepare();
        h
    }

    fn add(&mut self, widget: impl Widget) -> WidgetId {
        let root = self.root;
        self.add_to(root, widget)
    }

    fn add_to(&mut self, parent: WidgetId, widget: impl Widget) -> WidgetId {
        let id = self.tree.insert(Some(parent), widget);
        self.prepare();
        id
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

    fn prepare(&mut self) {
        self.tree.prepare(VIEWPORT, &mut self.text);
    }

    fn frame(&mut self, now: f64) {
        self.tree.animate(now, &mut self.text);
        self.prepare();
    }

    fn dispatch(&mut self, event: Event) {
        self.tree.dispatch(event, &mut self.text);
        self.prepare();
    }

    fn rect(&self, id: WidgetId) -> Rect {
        self.tree.window_rect(id).unwrap()
    }

    fn at(&self, id: WidgetId, x: f32, y: f32) -> Point {
        let r = self.rect(id);
        Point::new(r.x + x, r.y + y)
    }

    fn click(&mut self, pos: Point) {
        let button = PointerButton::Primary;
        self.dispatch(Event::PointerDown { pos, button });
        self.dispatch(Event::PointerUp { pos, button });
    }

    fn key(&mut self, key: Key) {
        self.key_with(key, Modifiers::default());
    }

    fn key_with(&mut self, key: Key, modifiers: Modifiers) {
        self.dispatch(Event::KeyDown { key, modifiers });
    }

    fn type_text(&mut self, text: &str) {
        self.dispatch(Event::Text(text.into()));
    }

    fn messages(&mut self) -> Vec<Message> {
        self.tree.take_messages()
    }

    fn text_commands(&self) -> usize {
        self.tree
            .display_list()
            .commands()
            .iter()
            .filter(|c| matches!(c, DrawCommand::Text { .. }))
            .count()
    }
}

fn values<T: 'static + Clone>(messages: &[Message]) -> Vec<T> {
    messages
        .iter()
        .filter_map(|m| m.downcast_ref::<T>().cloned())
        .collect()
}

fn icon() -> VectorIcon {
    VectorIcon::from_svg(r#"<svg viewBox="0 0 16 16"><path d="M2 2h12v12H2z"/></svg>"#).unwrap()
}

fn commands() -> Vec<PaletteCommand> {
    vec![
        PaletteCommand::new("Toggle Sidebar")
            .detail("View")
            .shortcut(Shortcut::command('s').with_alt()),
        PaletteCommand::new("Go to Buttons").detail("Go to"),
        PaletteCommand::new("Go to Sliders").detail("Go to"),
        PaletteCommand::new("Dark Appearance")
            .detail("View")
            .keywords("theme night"),
        PaletteCommand::new("Show Keyboard Shortcuts").icon(icon()),
    ]
}

#[test]
fn menu_commands_list_every_item_that_can_be_chosen_with_its_place_in_the_menu_bar() {
    let menus = vec![
        MenuBarMenu::new(
            "File",
            vec![
                MenuItem::new("New").key(Shortcut::command('n')),
                MenuItem::separator(),
                MenuItem::new("Revert").enabled(false),
                MenuItem::new("Export").submenu(vec![
                    MenuItem::new("PDF"),
                    MenuItem::new("Image").submenu(vec![MenuItem::new("PNG")]),
                ]),
                MenuItem::new("Share")
                    .enabled(false)
                    .submenu(vec![MenuItem::new("Mail")]),
                MenuItem::new("Close"),
            ],
        ),
        MenuBarMenu::new("View", vec![MenuItem::new("Dark").checked(true)]),
    ];
    let commands = menu_commands(&menus);
    let expected = [
        (
            PaletteCommand::new("New")
                .detail("File")
                .shortcut(Shortcut::command('n')),
            0,
            0,
        ),
        (PaletteCommand::new("Export: PDF").detail("File"), 0, 4),
        (
            PaletteCommand::new("Export: Image: PNG").detail("File"),
            0,
            6,
        ),
        (PaletteCommand::new("Close").detail("File"), 0, 9),
        (
            PaletteCommand::new("Dark").detail("View").checked(true),
            1,
            0,
        ),
    ];
    assert_eq!(commands.len(), expected.len());
    for ((command, chosen), (want, menu, item)) in commands.iter().zip(expected) {
        assert_eq!(*command, want);
        assert_eq!(*chosen, MenuBarChosen { menu, item });
        let found = menu_item(&menus[menu].items, item).unwrap();
        assert!(want.title().ends_with(found.label()));
    }
}

#[test]
fn choosing_a_menu_command_in_the_palette_gives_its_menu_item() {
    let mut h = Harness::new();
    let menus = vec![MenuBarMenu::new(
        "Edit",
        vec![
            MenuItem::new("Undo"),
            MenuItem::new("Find").submenu(vec![MenuItem::new("Find Next")]),
        ],
    )];
    let (commands, chosen): (Vec<_>, Vec<_>) = menu_commands(&menus).into_iter().unzip();
    CommandPalette::new(commands).open(&mut h.tree);
    h.prepare();
    h.type_text("next");
    h.key(Key::Enter);
    let picked = values::<CommandChosen>(&h.messages());
    assert_eq!(picked.len(), 1);
    let picked = picked[0];
    assert_eq!(chosen[picked.0], MenuBarChosen { menu: 0, item: 2 });
}

#[test]
fn a_checked_command_shows_a_checkmark_and_reads_as_checked() {
    use noa_gpui_core::access_item_id;
    use noa_gpui_core::accesskit::Toggled;
    let open = |checked: bool| {
        let mut h = Harness::new();
        let id = CommandPalette::new(vec![
            PaletteCommand::new("Show Sidebar"),
            PaletteCommand::new("Dark").checked(checked),
        ])
        .open(&mut h.tree);
        h.prepare();
        let commands = h.tree.display_list().commands();
        let lines = commands.iter().filter(|c| {
            matches!(
                c,
                DrawCommand::Shape {
                    shape: noa_gpui_scene::Shape::Line { .. },
                    ..
                }
            )
        });
        let lines = lines.count();
        (h, id, lines)
    };
    let (_, _, unchecked) = open(false);
    let (h, id, checked) = open(true);
    // The checkmark's two strokes.
    assert_eq!(checked, unchecked + 2);
    assert!(palette(&h, id).commands()[1].is_checked());
    let update = h.tree.accessibility_update(1.0);
    let toggled = |i| {
        let node = update
            .nodes
            .iter()
            .find(|(n, _)| *n == access_item_id(id, i));
        node.unwrap().1.toggled()
    };
    assert_eq!((toggled(0), toggled(1)), (None, Some(Toggled::True)));
}

fn palette(h: &Harness, id: WidgetId) -> &CommandPalette {
    h.tree.get::<CommandPalette>(id).unwrap()
}

#[test]
fn the_palette_opens_from_its_shortcuts_with_focus() {
    let mut h = Harness::new();
    let owner = h.add(Harness::sized(10.0, 10.0));
    for shortcut in COMMAND_PALETTE_SHORTCUTS {
        h.tree.add_shortcut(shortcut, owner);
    }
    let command = Modifiers {
        command: true,
        meta: true,
        ..Default::default()
    };
    h.key_with(Key::Char('k'), command);
    let shift = Modifiers {
        shift: true,
        ..command
    };
    h.key_with(Key::Char('p'), shift);
    let pressed = values::<ShortcutPressed>(&h.messages());
    assert_eq!(
        pressed,
        [
            ShortcutPressed(Shortcut::command('k')),
            ShortcutPressed(Shortcut::command('p').with_shift())
        ]
    );

    let id = CommandPalette::new(commands()).open(&mut h.tree);
    h.prepare();
    assert_eq!(h.tree.overlays().last(), Some(id));
    assert_eq!(h.tree.focused(), Some(id));
    assert_eq!(palette(&h, id).matches(), [0, 1, 2, 3, 4]);
    assert_eq!(palette(&h, id).highlighted(), Some(0));
    let rect = h.rect(id);
    assert_eq!(rect.y, 72.0, "hangs near the top");
    assert_eq!(rect.x, (VIEWPORT.width - rect.width) / 2.0, "centered");
}

#[test]
fn typing_filters_by_fuzzy_match_and_return_chooses() {
    let mut h = Harness::new();
    let id = CommandPalette::new(commands()).open(&mut h.tree);
    h.prepare();
    let before = h.rect(id).height;

    h.type_text("gos");
    assert_eq!(palette(&h, id).query_text(), "gos");
    // "Go to Sliders" matches at word starts; "Go to Buttons" only loosely.
    assert_eq!(palette(&h, id).matches()[0], 2);
    assert!(!palette(&h, id).matches().contains(&0));
    assert!(
        h.rect(id).height < before,
        "the list shrinks to the matches"
    );

    h.dispatch(Event::KeyDown {
        key: Key::Char('a'),
        modifiers: Modifiers {
            command: true,
            meta: true,
            ..Default::default()
        },
    });
    h.type_text("night");
    assert_eq!(palette(&h, id).matches(), [3]);

    h.type_text("zzz");
    assert!(palette(&h, id).matches().is_empty());
    h.key(Key::Enter);
    assert!(values::<CommandChosen>(&h.messages()).is_empty());

    for _ in 0.."nightzzz".len() {
        h.key(Key::Backspace);
    }
    assert_eq!(palette(&h, id).matches().len(), 5);
    h.key(Key::ArrowDown);
    h.key(Key::ArrowDown);
    assert_eq!(palette(&h, id).highlighted(), Some(2));
    h.key(Key::ArrowUp);
    h.key(Key::ArrowUp);
    h.key(Key::ArrowUp);
    assert_eq!(palette(&h, id).highlighted(), Some(4));
    h.key(Key::Enter);
    let messages = h.messages();
    assert_eq!(values::<CommandChosen>(&messages), [CommandChosen(4)]);
    assert!(messages.iter().any(|m| m.source == id));

    h.frame(10.0);
    h.frame(11.0);
    assert!(!h.tree.contains(id));
    assert_eq!(h.tree.overlays().count(), 0);
}

#[test]
fn clicking_a_row_chooses_it_and_escape_or_an_outside_click_closes() {
    let mut h = Harness::new();
    let field = h.add(noa_gpui_core::widgets::TextField::new("", 100.0));
    h.tree.set_focus(Some(field));
    let id = CommandPalette::new(commands()).open(&mut h.tree);
    h.prepare();
    // The second row: field (52) + separator (1) + padding (8) + one row (40) + half.
    h.dispatch(Event::PointerMove {
        pos: h.at(id, 100.0, 52.0 + 1.0 + 8.0 + 60.0),
    });
    assert_eq!(palette(&h, id).highlighted(), Some(1));
    h.click(h.at(id, 100.0, 52.0 + 1.0 + 8.0 + 60.0));
    assert_eq!(values::<CommandChosen>(&h.messages()), [CommandChosen(1)]);
    h.frame(10.0);
    h.frame(11.0);
    assert!(!h.tree.contains(id));
    assert_eq!(h.tree.focused(), Some(field), "focus goes back");

    let id = CommandPalette::new(commands()).open(&mut h.tree);
    h.prepare();
    h.key(Key::Escape);
    h.frame(20.0);
    h.frame(21.0);
    assert!(!h.tree.contains(id));
    assert!(values::<CommandChosen>(&h.messages()).is_empty());

    let id = CommandPalette::new(commands()).open(&mut h.tree);
    h.prepare();
    h.click(Point::new(5.0, VIEWPORT.height - 5.0));
    assert!(!h.tree.contains(id));
    let messages = h.messages();
    assert!(values::<CommandChosen>(&messages).is_empty());
    assert!(values::<noa_gpui_core::OverlayDismissed>(&messages).len() == 1);
    assert_eq!(h.tree.focused(), Some(field), "the press went no further");

    h.tree.add_shortcut(Shortcut::command('k'), field);
    CommandPalette::new(commands()).open(&mut h.tree);
    h.prepare();
    h.key_with(
        Key::Char('k'),
        Modifiers {
            command: true,
            meta: true,
            ..Default::default()
        },
    );
    assert!(values::<ShortcutPressed>(&h.messages()).is_empty());
}

#[test]
fn the_palette_draws_shortcuts_as_keycaps_and_marks_matches() {
    let mut h = Harness::new();
    let id = CommandPalette::new(commands()).open(&mut h.tree);
    h.prepare();
    let quads_in = |h: &Harness, area: Rect| {
        h.tree
            .display_list()
            .commands()
            .iter()
            .filter(|c| matches!(c, DrawCommand::Quad { rect, .. } if area.contains(Point::new(rect.x, rect.y))))
            .count()
    };
    let first_row = {
        let r = h.rect(id);
        Rect::new(r.x + 8.0, r.y + 61.0, r.width - 16.0, 40.0)
    };
    // "Toggle Sidebar" has ⌥⌘S: three keycaps of two quads each, plus the highlight.
    assert!(quads_in(&h, first_row) >= 7);
    let plain = quads_in(&h, first_row);
    h.type_text("tog");
    assert_eq!(quads_in(&h, first_row), plain + 1);
}

const DEEP: [&str; 7] = [
    "Home",
    "Projects",
    "noa-gpui",
    "crates",
    "noa-gpui-core",
    "widgets",
    "path_control.rs",
];

#[test]
fn breadcrumbs_take_icons_and_slashes() {
    let mut h = Harness::new();
    let plain = h.add(PathControl::new(["Home", "Projects"]));
    let crumbs = h.add(
        PathControl::new([
            PathItem::new("Home").icon(icon()),
            PathItem::new("Projects"),
        ])
        .separator(PathSeparator::Slash),
    );
    assert_eq!(
        h.tree.get::<PathControl>(crumbs).unwrap().items()[0].title(),
        "Home"
    );
    assert!(
        h.rect(crumbs).width > h.rect(plain).width,
        "room for the icon"
    );
    let icons = h
        .tree
        .display_list()
        .commands()
        .iter()
        .filter(|c| matches!(c, DrawCommand::Icon { .. }))
        .count();
    assert_eq!(icons, 1);
    h.click(h.at(crumbs, 10.0, 12.0));
    assert_eq!(
        values::<PathItemClicked>(&h.messages()),
        [PathItemClicked(0)]
    );
}

#[test]
fn a_long_path_collapses_its_middle_into_a_menu() {
    let mut h = Harness::new();
    let wide = h.add(PathControl::new(DEEP));
    assert_eq!(h.tree.get::<PathControl>(wide).unwrap().collapsed(), 0..0);
    let full = h.rect(wide).width;

    let path = h.add(PathControl::new(DEEP).width(full * 0.6));
    let collapsed = h.tree.get::<PathControl>(path).unwrap().collapsed();
    assert_eq!(collapsed.start, 1, "the first component stays");
    assert!(collapsed.end < DEEP.len(), "the last component stays");
    assert!(collapsed.len() >= 2);

    let first = h
        .tree
        .accessibility_update(1.0)
        .nodes
        .iter()
        .filter(|(_, n)| n.role() == noa_gpui_core::accesskit::Role::Button)
        .find_map(|(_, n)| n.bounds())
        .expect("the … is a button");
    let more = Point::new(first.x0 as f32 + 4.0, (first.y0 + first.y1) as f32 / 2.0);
    h.click(more);
    let menu = h
        .tree
        .overlays()
        .last()
        .expect("a menu of the collapsed part");
    let labels: Vec<String> = h
        .tree
        .get::<Menu>(menu)
        .unwrap()
        .items()
        .iter()
        .map(|i| i.label().to_owned())
        .collect();
    assert_eq!(labels, DEEP[collapsed.clone()]);
    h.key(Key::ArrowDown);
    h.key(Key::ArrowDown);
    h.key(Key::Enter);
    assert_eq!(
        values::<PathItemClicked>(&h.messages()),
        [PathItemClicked(collapsed.start + 1)]
    );

    h.tree
        .update::<PathControl, _>(path, |p, cx| p.set_items(cx, DEEP[..3].iter().copied()));
    h.prepare();
    assert_eq!(h.tree.get::<PathControl>(path).unwrap().collapsed(), 0..0);
}

#[test]
fn a_path_is_never_wider_than_its_column() {
    let mut h = Harness::new();
    let column = h.add(Container::new(Style {
        flex_direction: FlexDirection::Column,
        size: LayoutSize {
            width: length(300.0),
            height: length(60.0),
        },
        ..Default::default()
    }));
    let path = h.add_to(column, PathControl::new(DEEP));
    assert!(h.rect(path).width <= 300.0);
    assert!(
        !h.tree
            .get::<PathControl>(path)
            .unwrap()
            .collapsed()
            .is_empty()
    );
}

#[test]
fn a_path_shrinks_with_its_container() {
    let mut h = Harness::new();
    let narrow = h.add(Harness::sized(300.0, 30.0));
    let path = h.add_to(narrow, PathControl::new(DEEP));
    assert!(h.rect(path).width <= 300.0);
    assert!(
        !h.tree
            .get::<PathControl>(path)
            .unwrap()
            .collapsed()
            .is_empty()
    );
    h.tree.set_focus(Some(path));
    h.key(Key::Home);
    for _ in 0..DEEP.len() {
        h.key(Key::ArrowLeft);
    }
    h.key(Key::Space);
    assert_eq!(
        values::<PathItemClicked>(&h.messages()),
        [PathItemClicked(0)]
    );
    h.key(Key::ArrowRight);
    h.key(Key::Space);
    assert!(h.tree.overlays().last().is_some(), "… opened its menu");
}

fn page(h: &mut Harness) -> (WidgetId, WidgetId, Vec<WidgetId>) {
    let scroll = h.add(ScrollView::new(Style {
        flex_direction: FlexDirection::Column,
        size: LayoutSize {
            width: length(400.0),
            height: length(300.0),
        },
        flex_shrink: 0.0,
        ..Default::default()
    }));
    let sections: Vec<WidgetId> = (0..5)
        .map(|_| h.add_to(scroll, Harness::sized(400.0, 300.0)))
        .collect();
    let mut nav = AnchorNav::new(scroll).heading("GUIDE");
    for (i, &s) in sections.iter().enumerate() {
        nav = nav.anchor(format!("Section {i}"), s);
    }
    let nav = h.add(nav);
    (scroll, nav, sections)
}

fn active(h: &Harness, nav: WidgetId) -> Option<usize> {
    h.tree.get::<AnchorNav>(nav).unwrap().active()
}

fn scroll_by(h: &mut Harness, scroll: WidgetId, dy: f32) {
    let pos = h.at(scroll, 10.0, 10.0);
    h.dispatch(Event::Wheel {
        pos,
        delta: Point::new(0.0, -dy),
    });
}

#[test]
fn the_anchor_nav_follows_the_scroll_position() {
    let mut h = Harness::new();
    let (scroll, nav, _) = page(&mut h);
    assert_eq!(active(&h, nav), Some(0));
    assert!(!AnchorNav::sync(&mut h.tree, nav), "nothing scrolled yet");

    // A section counts once its top passes a third of the way down the viewport.
    scroll_by(&mut h, scroll, 150.0);
    assert_eq!(h.tree.get::<ScrollView>(scroll).unwrap().offset(), 150.0);
    assert!(
        !AnchorNav::sync(&mut h.tree, nav),
        "section 1 is at 150, below 100"
    );
    assert_eq!(active(&h, nav), Some(0));
    scroll_by(&mut h, scroll, 100.0);
    assert!(AnchorNav::sync(&mut h.tree, nav));
    assert_eq!(active(&h, nav), Some(1));
    scroll_by(&mut h, scroll, 610.0);
    AnchorNav::sync(&mut h.tree, nav);
    assert_eq!(active(&h, nav), Some(3));
    assert!(!AnchorNav::sync(&mut h.tree, nav));
}

#[test]
fn choosing_an_anchor_scrolls_its_section_to_the_top() {
    let mut h = Harness::new();
    let (scroll, nav, _) = page(&mut h);
    let accessibility = h.tree.accessibility_update(1.0);
    let rows: Vec<_> = accessibility
        .nodes
        .iter()
        .filter(|(_, n)| n.role() == noa_gpui_core::accesskit::Role::Link)
        .filter_map(|(_, n)| n.bounds())
        .collect();
    assert_eq!(rows.len(), 5);
    let third = rows[2];
    h.click(Point::new(
        third.x0 as f32 + 20.0,
        (third.y0 + third.y1) as f32 / 2.0,
    ));
    let chosen = values::<AnchorSelected>(&h.messages());
    assert_eq!(chosen, [AnchorSelected(2)]);

    AnchorNav::scroll_to(&mut h.tree, nav, chosen[0].0);
    h.prepare();
    assert_eq!(h.tree.get::<ScrollView>(scroll).unwrap().offset(), 600.0);
    assert_eq!(active(&h, nav), Some(2));
    assert!(!AnchorNav::sync(&mut h.tree, nav), "already in sync");

    h.tree.set_focus(Some(nav));
    h.key(Key::ArrowDown);
    h.key(Key::ArrowDown);
    h.key(Key::Enter);
    assert_eq!(values::<AnchorSelected>(&h.messages()), [AnchorSelected(4)]);
}

#[test]
fn a_scroll_margin_keeps_space_above_the_section() {
    let mut h = Harness::new();
    let scroll = h.add(ScrollView::new(Style {
        flex_direction: FlexDirection::Column,
        padding: noa_gpui_core::style::Rect::length(20.0),
        size: LayoutSize {
            width: length(400.0),
            height: length(300.0),
        },
        flex_shrink: 0.0,
        ..Default::default()
    }));
    let a = h.add_to(scroll, Harness::sized(300.0, 400.0));
    let b = h.add_to(scroll, Harness::sized(300.0, 400.0));
    let _ = h.add_to(scroll, Harness::sized(300.0, 400.0));
    let nav = h.add(
        AnchorNav::new(scroll)
            .anchor("A", a)
            .anchor("B", b)
            .scroll_margin(20.0),
    );
    AnchorNav::scroll_to(&mut h.tree, nav, 1);
    h.prepare();
    assert_eq!(h.rect(b).y - h.rect(scroll).y, 20.0);
}

fn rail_split(h: &mut Harness) -> (WidgetId, WidgetId, WidgetId) {
    let frame = h.add(Harness::sized(700.0, 300.0));
    let split = h.add_to(
        frame,
        SplitView::horizontal()
            .pane(SplitPane::fixed(220.0).min(160.0).rail(OUTLINE_RAIL_WIDTH))
            .pane(SplitPane::flexible().min(200.0)),
    );
    let sidebar = h.add_to(
        split,
        OutlineView::new(vec![OutlineItem::new("Library").children([
            OutlineItem::new("Photos").icon(icon()).badge("12"),
            OutlineItem::new("Albums").icon(icon()),
        ])])
        .sidebar()
        .height(300.0),
    );
    let content = h.add_to(split, Container::new(Style::default()).clip(true));
    (split, sidebar, content)
}

#[test]
fn a_rail_pane_narrows_to_its_rail_with_an_animation() {
    let mut h = Harness::new();
    let (split, sidebar, content) = rail_split(&mut h);
    assert_eq!(h.rect(sidebar).width, 220.0);
    h.tree.set_now(1.0);
    h.tree
        .update::<SplitView, _>(split, |s, cx| s.set_collapsed(cx, 0, true));
    h.prepare();
    assert!(h.tree.get::<SplitView>(split).unwrap().is_animating());

    h.frame(1.0 + RAIL_DURATION / 2.0);
    let midway = h.rect(sidebar).width;
    assert!(midway < 220.0 && midway > OUTLINE_RAIL_WIDTH, "{midway}");
    assert!(values::<PanesChanged>(&h.messages()).is_empty());

    h.frame(1.0 + RAIL_DURATION + 0.01);
    assert!(!h.tree.get::<SplitView>(split).unwrap().is_animating());
    assert_eq!(h.rect(sidebar).width, OUTLINE_RAIL_WIDTH);
    assert!(!h.tree.is_hidden(sidebar), "a rail is not hidden");
    assert_eq!(h.rect(content).x, h.rect(sidebar).right() + DIVIDER);
    assert_eq!(
        values::<PanesChanged>(&h.messages()),
        [PanesChanged(vec![
            OUTLINE_RAIL_WIDTH,
            700.0 - OUTLINE_RAIL_WIDTH - DIVIDER
        ])]
    );
    assert!(h.tree.get::<SplitView>(split).unwrap().is_collapsed(0));

    let divider = Point::new(h.rect(sidebar).right() + DIVIDER / 2.0, 100.0);
    let button = PointerButton::Primary;
    h.dispatch(Event::PointerDown {
        pos: divider,
        button,
    });
    h.dispatch(Event::PointerMove {
        pos: Point::new(divider.x + 80.0, 100.0),
    });
    h.dispatch(Event::PointerUp {
        pos: Point::new(divider.x + 80.0, 100.0),
        button,
    });
    assert_eq!(h.rect(sidebar).width, OUTLINE_RAIL_WIDTH);
    h.tree.set_now(5.0);
    h.click(divider);
    h.tree.set_now(5.1);
    h.click(divider);
    h.frame(5.1 + RAIL_DURATION + 0.01);
    assert_eq!(h.rect(sidebar).width, 220.0);
    assert!(!h.tree.get::<SplitView>(split).unwrap().is_collapsed(0));
}

#[test]
fn with_reduced_motion_a_rail_pane_narrows_on_the_next_frame() {
    let mut h = Harness::new();
    h.tree.set_reduce_motion(true);
    let (split, sidebar, _) = rail_split(&mut h);
    h.tree.set_now(1.0);
    h.tree
        .update::<SplitView, _>(split, |s, cx| s.set_collapsed(cx, 0, true));
    h.prepare();
    h.frame(1.0 + 1.0 / 60.0);
    assert!(!h.tree.get::<SplitView>(split).unwrap().is_animating());
    assert_eq!(h.rect(sidebar).width, OUTLINE_RAIL_WIDTH);
    assert_eq!(
        values::<PanesChanged>(&h.messages()),
        [PanesChanged(vec![
            OUTLINE_RAIL_WIDTH,
            700.0 - OUTLINE_RAIL_WIDTH - DIVIDER
        ])],
        "reported as for the animated change"
    );
}

#[test]
fn a_sidebar_at_rail_width_shows_only_its_icons() {
    let mut h = Harness::new();
    let (split, sidebar, _) = rail_split(&mut h);
    fn outline(h: &Harness, id: WidgetId) -> &OutlineView {
        h.tree.get::<OutlineView>(id).unwrap()
    }
    assert_eq!(outline(&h, sidebar).title_fade(), 1.0);
    let wide_text = h.text_commands();
    assert!(
        outline(&h, sidebar)
            .tooltip(Point::new(20.0, 42.0))
            .is_none()
    );

    h.tree.update::<SplitView, _>(split, |s, cx| {
        s.set_collapsed(cx, 0, true);
    });
    h.frame(100.0);
    assert_eq!(h.rect(sidebar).width, OUTLINE_RAIL_WIDTH);
    assert_eq!(outline(&h, sidebar).title_fade(), 0.0);
    // Heading, two titles and a badge are no longer drawn; the icons still are.
    assert_eq!(h.text_commands(), wide_text - 4);
    let icons = h
        .tree
        .display_list()
        .commands()
        .iter()
        .filter(|c| matches!(c, DrawCommand::Icon { .. }))
        .count();
    assert_eq!(icons, 2);
    // The pointer gets a row's title instead (row 1: "Photos").
    assert_eq!(
        outline(&h, sidebar)
            .tooltip(Point::new(20.0, 42.0))
            .as_deref(),
        Some("Photos")
    );
    assert_eq!(
        outline(&h, sidebar).tooltip(Point::new(20.0, 14.0)),
        None,
        "heading"
    );
}

#[test]
fn commands_chosen_lately_come_first_until_something_is_typed() {
    let mut recent = RecentCommands::new(3);
    for title in commands().iter().map(|c| c.title().to_owned()) {
        recent.note(title);
    }
    // Five noted, three kept: the last three chosen, latest first.
    let titles: Vec<String> = commands()[2..]
        .iter()
        .rev()
        .map(|c| c.title().to_owned())
        .collect();
    assert_eq!(recent.titles(), titles);
    recent.note(commands()[3].title());
    assert_eq!(recent.titles()[0], commands()[3].title());
    assert_eq!(recent.titles().len(), 3);
    let saved = RecentCommands::from_text(&recent.to_text(), 3);
    assert_eq!(saved, recent);

    let mut h = Harness::new();
    let id = CommandPalette::new(commands())
        .recent(&recent)
        .open(&mut h.tree);
    h.prepare();
    // 3, 4, 2 first, then the rest in their order; the latest is highlighted.
    assert_eq!(palette(&h, id).matches(), [3, 4, 2, 0, 1]);
    assert_eq!(palette(&h, id).highlighted(), Some(3));
    h.type_text("gos");
    assert_eq!(palette(&h, id).matches()[0], 2);
    let id = CommandPalette::new(commands())
        .recent(&RecentCommands::from_text("No such command", 3))
        .open(&mut h.tree);
    h.prepare();
    assert_eq!(palette(&h, id).matches(), [0, 1, 2, 3, 4]);
}

#[test]
fn a_command_chosen_lately_is_told_apart_from_one_with_the_same_title() {
    let commands = || {
        vec![
            PaletteCommand::new("Close").detail("File"),
            PaletteCommand::new("Close").detail("Window"),
            PaletteCommand::new("Minimize").detail("Window"),
        ]
    };
    let mut recent = RecentCommands::default();
    recent.note_command(&commands()[1]);
    recent.note_command(&commands()[0]);
    recent.note_command(&commands()[1]);
    assert_eq!(recent.titles(), ["Close", "Close"]);
    let saved = RecentCommands::from_text(&recent.to_text(), 8);
    assert_eq!(saved, recent);

    let mut h = Harness::new();
    let id = CommandPalette::new(commands())
        .recent(&saved)
        .open(&mut h.tree);
    h.prepare();
    // The one chosen last (Window's) is first and highlighted: Enter runs it.
    assert_eq!(palette(&h, id).matches(), [1, 0, 2]);
    assert_eq!(palette(&h, id).highlighted(), Some(1));
    h.key(Key::Enter);
    assert_eq!(values::<CommandChosen>(&h.messages()), [CommandChosen(1)]);

    // A title noted alone (as `note` and older saved lists do) is the first command
    // with that title.
    let id = CommandPalette::new(commands())
        .recent(&RecentCommands::from_text("Close", 8))
        .open(&mut h.tree);
    h.prepare();
    assert_eq!(palette(&h, id).matches(), [0, 1, 2]);

    let commands = || {
        vec![
            PaletteCommand::new("Close").detail("File"),
            PaletteCommand::new("Close"),
        ]
    };
    let mut recent = RecentCommands::default();
    recent.note_command(&commands()[1]);
    let saved = RecentCommands::from_text(&recent.to_text(), 8);
    assert_eq!(saved, recent);
    let id = CommandPalette::new(commands())
        .recent(&saved)
        .open(&mut h.tree);
    h.prepare();
    assert_eq!(palette(&h, id).highlighted(), Some(1));
}

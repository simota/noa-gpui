use noa_gpui_core::style::{FlexDirection, Size as LayoutSize, length, percent};
use noa_gpui_core::widgets::{
    Button, Container, ContextMenuArea, Menu, MenuBar, MenuBarChosen, MenuBarMenu, MenuChosen,
    MenuItem, TextChanged, TextSubmitted, Toolbar, ToolbarItem, ToolbarItemClicked, ToolbarSymbol,
    menu_index, menu_item,
};
use noa_gpui_core::{
    Event, Key, Message, Modifiers, PointerButton, Shortcut, Style, UiTree, WidgetId, access_id,
    access_item_id, accesskit,
};
use noa_gpui_scene::{Point, Rect, Size};
use noa_gpui_text::{FontOptions, TextStore};

const VIEWPORT: Size = Size::new(900.0, 600.0);
const PAD: f32 = 5.0;
const ROW: f32 = 24.0;
const SEPARATOR: f32 = 9.0;

struct Harness {
    tree: UiTree,
    text: TextStore,
    root: WidgetId,
}

impl Harness {
    fn new() -> Self {
        let mut tree = UiTree::new();
        tree.set_shortcut_glyphs(true);
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

    fn add_to(&mut self, parent: WidgetId, widget: impl noa_gpui_core::Widget) -> WidgetId {
        let id = self.tree.insert(Some(parent), widget);
        self.prepare();
        id
    }

    fn add(&mut self, widget: impl noa_gpui_core::Widget) -> WidgetId {
        self.add_to(self.root, widget)
    }

    fn prepare(&mut self) {
        self.tree.prepare(VIEWPORT, &mut self.text);
    }

    fn rect(&self, id: WidgetId) -> Rect {
        self.tree.window_rect(id).unwrap()
    }

    fn modifiers(&mut self, modifiers: Modifiers) {
        self.tree
            .dispatch(Event::ModifiersChanged(modifiers), &mut self.text);
    }

    fn press(&mut self, pos: Point, button: PointerButton) {
        self.tree
            .dispatch(Event::PointerDown { pos, button }, &mut self.text);
        self.prepare();
    }

    fn release(&mut self, pos: Point, button: PointerButton) {
        self.tree
            .dispatch(Event::PointerUp { pos, button }, &mut self.text);
        self.prepare();
    }

    fn click(&mut self, pos: Point) {
        self.press(pos, PointerButton::Primary);
        self.release(pos, PointerButton::Primary);
    }

    fn right_click(&mut self, pos: Point) {
        self.press(pos, PointerButton::Secondary);
        self.release(pos, PointerButton::Secondary);
    }

    fn hover(&mut self, pos: Point) {
        self.tree
            .dispatch(Event::PointerMove { pos }, &mut self.text);
        self.prepare();
    }

    fn key(&mut self, key: Key) {
        self.key_with(key, Modifiers::default());
    }

    fn key_with(&mut self, key: Key, modifiers: Modifiers) {
        self.tree
            .dispatch(Event::KeyDown { key, modifiers }, &mut self.text);
        self.modifiers(Modifiers::default());
        self.prepare();
    }

    fn type_text(&mut self, text: &str) {
        self.tree.dispatch(Event::Text(text.into()), &mut self.text);
        self.prepare();
    }

    fn messages(&mut self) -> Vec<Message> {
        self.tree.take_messages()
    }

    fn menus(&self) -> Vec<WidgetId> {
        self.tree
            .overlays()
            .filter(|&o| self.tree.get::<Menu>(o).is_some())
            .collect()
    }

    fn menu(&self, id: WidgetId) -> &Menu {
        self.tree.get::<Menu>(id).unwrap()
    }

    fn row(&self, menu: WidgetId, row: usize) -> Point {
        let r = self.rect(menu);
        let items = self.menu(menu).items();
        let top: f32 = PAD
            + items[..row]
                .iter()
                .map(|i| if i.is_separator() { SEPARATOR } else { ROW })
                .sum::<f32>();
        Point::new(r.x + 30.0, r.y + top + ROW / 2.0)
    }
}

fn values<T: 'static + Clone>(messages: &[Message]) -> Vec<T> {
    messages
        .iter()
        .filter_map(|m| m.downcast_ref::<T>().cloned())
        .collect()
}

const CMD: Modifiers = Modifiers {
    shift: false,
    ctrl: false,
    alt: false,
    meta: true,
    command: true,
};
/// Ctrl on a Mac, where it is not the command key.
const MAC_CTRL: Modifiers = Modifiers {
    shift: false,
    ctrl: true,
    alt: false,
    meta: false,
    command: false,
};
const SHIFT: Modifiers = Modifiers {
    shift: true,
    ctrl: false,
    alt: false,
    meta: false,
    command: false,
};

fn edit_menu() -> Vec<MenuItem> {
    vec![
        MenuItem::new("Cut").key(Shortcut::command('x')),
        MenuItem::new("Copy").key(Shortcut::command('c')),
        MenuItem::new("Paste").enabled(false),
        MenuItem::separator(),
        MenuItem::new("Share").submenu(vec![
            MenuItem::new("Mail"),
            MenuItem::new("Messages").submenu(vec![MenuItem::new("Alice"), MenuItem::new("Bob")]),
        ]),
        MenuItem::separator(),
        MenuItem::new("Show Grid")
            .checked(true)
            .key(Shortcut::command('g').with_shift()),
    ]
}
// Depth-first indices: 0 Cut, 1 Copy, 2 Paste, 3 ─, 4 Share, 5 Mail, 6 Messages,
// 7 Alice, 8 Bob, 9 ─, 10 Show Grid.

fn area(h: &mut Harness) -> (WidgetId, WidgetId) {
    let area = h.add(ContextMenuArea::new(
        Style {
            size: LayoutSize {
                width: length(300.0),
                height: length(200.0),
            },
            ..Default::default()
        },
        edit_menu(),
    ));
    let button = h.add_to(area, Button::new("Inside"));
    (area, button)
}

fn center(r: Rect) -> Point {
    Point::new(r.x + r.width / 2.0, r.y + r.height / 2.0)
}

#[test]
fn submenu_items_are_numbered_depth_first() {
    let items = edit_menu();
    assert_eq!(menu_item(&items, 4).unwrap().label(), "Share");
    assert_eq!(menu_item(&items, 7).unwrap().label(), "Alice");
    assert_eq!(menu_item(&items, 10).unwrap().label(), "Show Grid");
    assert!(menu_item(&items, 11).is_none());
    assert_eq!(menu_index(&items, &[4, 1, 1]), Some(8));
    assert_eq!(menu_index(&items, &[6]), Some(10));
    assert_eq!(menu_index(&items, &[4, 5]), None);
}

#[test]
fn right_click_opens_the_context_menu_at_the_pointer_and_a_click_chooses() {
    let mut h = Harness::new();
    let (area, _) = area(&mut h);
    let at = Point::new(40.0, 150.0);
    h.right_click(at);
    let menus = h.menus();
    assert_eq!(menus.len(), 1);
    let r = h.rect(menus[0]);
    assert_eq!((r.x, r.y), (at.x, at.y), "the menu opens at the pointer");
    assert_eq!(h.tree.focused(), Some(menus[0]));
    let menu = h.menu(menus[0]);
    assert!(menu.items()[6].is_checked());
    assert_eq!(menu.shortcut_text(1), Some("⌘C"));
    assert_eq!(menu.shortcut_text(6), Some("⇧⌘G"));

    h.hover(h.row(menus[0], 1));
    assert_eq!(h.menu(menus[0]).highlighted(), Some(1));
    h.click(h.row(menus[0], 2));
    assert_eq!(h.menus().len(), 1, "Paste is disabled");
    h.click(h.row(menus[0], 1));
    assert!(h.menus().is_empty());
    let chosen: Vec<(WidgetId, MenuChosen)> = h
        .messages()
        .iter()
        .filter_map(|m| m.downcast_ref::<MenuChosen>().map(|c| (m.source, *c)))
        .collect();
    assert_eq!(chosen, [(area, MenuChosen(1))]);
}

#[test]
fn ctrl_click_opens_it_on_a_mac_and_a_plain_click_does_not() {
    let mut h = Harness::new();
    let (_, button) = area(&mut h);
    h.click(Point::new(40.0, 150.0));
    assert!(h.menus().is_empty());
    h.modifiers(MAC_CTRL);
    h.press(Point::new(40.0, 150.0), PointerButton::Primary);
    h.modifiers(Modifiers::default());
    assert_eq!(h.menus().len(), 1);
    h.click(center(h.rect(button)));
    assert!(h.menus().is_empty());
    assert!(values::<MenuChosen>(&h.messages()).is_empty());
}

#[test]
fn hovering_a_submenu_opens_it_beside_and_choosing_inside_closes_every_level() {
    let mut h = Harness::new();
    let (area, _) = area(&mut h);
    h.right_click(Point::new(40.0, 150.0));
    let root = h.menus()[0];
    h.hover(h.row(root, 4)); // Share
    let menus = h.menus();
    assert_eq!(menus.len(), 2);
    assert_eq!(h.menu(root).open_submenu(), Some(4));
    let share = menus[1];
    assert!(
        h.rect(share).x >= h.rect(root).right() - 2.0,
        "beside the parent"
    );
    h.hover(h.row(root, 1));
    assert_eq!(h.menus(), [root]);
    assert_eq!(h.menu(root).open_submenu(), None);
    h.hover(h.row(root, 4));
    let share = h.menus()[1];
    h.hover(h.row(share, 1));
    let messages = h.menus()[2];
    h.hover(h.row(messages, 1));
    h.click(h.row(messages, 1));
    assert!(h.menus().is_empty());
    let chosen: Vec<(WidgetId, MenuChosen)> = h
        .messages()
        .iter()
        .filter_map(|m| m.downcast_ref::<MenuChosen>().map(|c| (m.source, *c)))
        .collect();
    assert_eq!(chosen, [(area, MenuChosen(8))]);
    h.right_click(Point::new(40.0, 150.0));
    let root = h.menus()[0];
    h.click(h.row(root, 4));
    assert_eq!(h.menus().len(), 2);
}

#[test]
fn the_context_menu_key_and_shift_f10_open_the_menu_of_the_focused_widget() {
    let mut h = Harness::new();
    let (_, button) = area(&mut h);
    h.tree.set_focus(Some(button));
    h.key(Key::ContextMenu);
    assert_eq!(h.menus().len(), 1);
    let r = h.rect(h.menus()[0]);
    assert_eq!(r.y, h.rect(button).bottom(), "below the focused widget");
    h.key(Key::Escape);
    assert!(h.menus().is_empty());
    assert_eq!(h.tree.focused(), Some(button), "focus comes back");
    h.key(Key::F10);
    assert!(h.menus().is_empty(), "F10 alone is not the context menu");
    h.key_with(Key::F10, SHIFT);
    assert_eq!(h.menus().len(), 1);
}

#[test]
fn arrows_skip_separators_and_disabled_items_and_return_chooses() {
    let mut h = Harness::new();
    let (area, button) = area(&mut h);
    h.tree.set_focus(Some(button));
    h.key(Key::ContextMenu);
    let root = h.menus()[0];
    assert_eq!(h.menu(root).highlighted(), None);
    h.key(Key::ArrowDown);
    assert_eq!(h.menu(root).highlighted(), Some(0));
    h.key(Key::ArrowDown);
    h.key(Key::ArrowDown); // over disabled Paste and the separator
    assert_eq!(h.menu(root).highlighted(), Some(4));
    h.key(Key::ArrowDown);
    assert_eq!(h.menu(root).highlighted(), Some(6));
    h.key(Key::ArrowDown);
    assert_eq!(h.menu(root).highlighted(), Some(0));
    h.key(Key::ArrowUp);
    assert_eq!(h.menu(root).highlighted(), Some(6));
    h.key(Key::Enter);
    assert!(h.menus().is_empty());
    let chosen: Vec<(WidgetId, MenuChosen)> = h
        .messages()
        .iter()
        .filter_map(|m| m.downcast_ref::<MenuChosen>().map(|c| (m.source, *c)))
        .collect();
    assert_eq!(chosen, [(area, MenuChosen(10))]);
}

#[test]
fn right_and_left_open_and_close_submenus_and_escape_closes_all() {
    let mut h = Harness::new();
    let (area, button) = area(&mut h);
    h.tree.set_focus(Some(button));
    h.key(Key::ContextMenu);
    let root = h.menus()[0];
    h.type_text("s"); // Share (the first item starting with S)
    assert_eq!(h.menu(root).highlighted(), Some(4));
    h.key(Key::ArrowRight);
    let menus = h.menus();
    assert_eq!(menus.len(), 2);
    let share = menus[1];
    assert_eq!(h.tree.focused(), Some(share));
    assert_eq!(h.menu(share).highlighted(), Some(0), "the first item");
    h.key(Key::ArrowDown);
    h.key(Key::Enter); // Messages: opens its submenu
    let messages = h.menus()[2];
    assert_eq!(h.menu(messages).highlighted(), Some(0));
    h.key(Key::ArrowLeft);
    assert_eq!(h.menus(), [root, share]);
    assert_eq!(h.tree.focused(), Some(share));
    assert_eq!(h.menu(share).highlighted(), Some(1));
    h.key(Key::Space);
    h.key(Key::ArrowDown);
    h.key(Key::Enter); // Bob
    let chosen: Vec<(WidgetId, MenuChosen)> = h
        .messages()
        .iter()
        .filter_map(|m| m.downcast_ref::<MenuChosen>().map(|c| (m.source, *c)))
        .collect();
    assert_eq!(chosen, [(area, MenuChosen(8))]);
    assert!(h.menus().is_empty());

    h.key(Key::ContextMenu);
    h.type_text("s");
    h.key(Key::ArrowRight);
    assert_eq!(h.menus().len(), 2);
    h.key(Key::Escape);
    assert!(h.menus().is_empty());
    assert!(values::<MenuChosen>(&h.messages()).is_empty());
    assert_eq!(h.tree.focused(), Some(button));
}

#[test]
fn typing_jumps_to_items_and_repeating_a_letter_cycles() {
    let mut h = Harness::new();
    let (_, button) = area(&mut h);
    h.tree.set_focus(Some(button));
    h.key(Key::ContextMenu);
    let root = h.menus()[0];
    h.tree.set_now(1.0);
    h.type_text("c");
    assert_eq!(h.menu(root).highlighted(), Some(0)); // Cut
    h.type_text("c");
    assert_eq!(h.menu(root).highlighted(), Some(1)); // Copy
    h.tree.set_now(3.0);
    h.type_text("s");
    h.type_text("h");
    h.type_text("o");
    assert_eq!(h.menu(root).highlighted(), Some(6));
}

#[test]
fn menu_items_are_exposed_to_assistive_technology() {
    let mut h = Harness::new();
    let (area, button) = area(&mut h);
    h.tree.set_focus(Some(button));
    h.key(Key::ContextMenu);
    let root = h.menus()[0];
    let update = h.tree.accessibility_update(1.0);
    let node = |id| &update.nodes.iter().find(|(n, _)| *n == id).unwrap().1;
    assert_eq!(node(access_id(root)).role(), accesskit::Role::Menu);
    // Separators are left out: item 2 is Paste, item 4 is Show Grid.
    assert!(node(access_item_id(root, 2)).is_disabled());
    let grid = node(access_item_id(root, 4));
    assert_eq!(grid.role(), accesskit::Role::MenuItemCheckBox);
    assert_eq!(grid.keyboard_shortcut(), Some("⇧⌘G"));
    h.tree.accessibility_action(
        &accesskit::ActionRequest {
            action: accesskit::Action::Click,
            target_tree: accesskit::TreeId::ROOT,
            target_node: access_item_id(root, 4),
            data: None,
        },
        &mut h.text,
    );
    h.prepare();
    let chosen: Vec<(WidgetId, MenuChosen)> = h
        .messages()
        .iter()
        .filter_map(|m| m.downcast_ref::<MenuChosen>().map(|c| (m.source, *c)))
        .collect();
    assert_eq!(chosen, [(area, MenuChosen(10))]);
}

#[test]
fn shortcuts_are_written_with_names_off_apple_platforms() {
    let mut h = Harness::new();
    h.tree.set_shortcut_glyphs(false);
    let (_, button) = area(&mut h);
    h.tree.set_focus(Some(button));
    h.key(Key::ContextMenu);
    let root = h.menus()[0];
    assert_eq!(h.menu(root).shortcut_text(1), Some("Ctrl+C"));
    assert_eq!(h.menu(root).shortcut_text(6), Some("Ctrl+Shift+G"));
}

fn bar_menus() -> Vec<MenuBarMenu> {
    vec![
        MenuBarMenu::new(
            "File",
            vec![
                MenuItem::new("New").key(Shortcut::command('n')),
                MenuItem::new("Open Recent")
                    .submenu(vec![MenuItem::new("a.txt"), MenuItem::new("b.txt")]),
                MenuItem::new("Close")
                    .key(Shortcut::command('w'))
                    .enabled(false),
            ],
        ),
        MenuBarMenu::new("Edit", edit_menu()),
        MenuBarMenu::new("Help", vec![MenuItem::new("About")]),
    ]
}

fn bar(h: &mut Harness) -> WidgetId {
    h.add(MenuBar::new(bar_menus()))
}

fn title(h: &Harness, bar: WidgetId, i: usize) -> Point {
    let r = h.tree.get::<MenuBar>(bar).unwrap().title_rect(i).unwrap();
    let origin = h.rect(bar);
    Point::new(
        origin.x + r.x + r.width / 2.0,
        origin.y + r.y + r.height / 2.0,
    )
}

#[test]
fn menu_bar_titles_open_their_menus_and_hovering_switches_while_open() {
    let mut h = Harness::new();
    let b = bar(&mut h);
    h.click(title(&h, b, 0));
    let file = h.menus();
    assert_eq!(file.len(), 1);
    assert_eq!(h.menu(file[0]).items()[0].label(), "New");
    assert!(
        h.rect(file[0]).y >= h.rect(b).bottom() - 4.0,
        "below the bar"
    );
    assert_eq!(h.tree.get::<MenuBar>(b).unwrap().open_menu(), Some(0));
    h.hover(title(&h, b, 1));
    let menus = h.menus();
    assert_eq!(menus.len(), 1);
    assert_eq!(h.menu(menus[0]).items()[0].label(), "Cut");
    assert_eq!(h.tree.get::<MenuBar>(b).unwrap().open_menu(), Some(1));
    h.click(h.row(menus[0], 1));
    assert_eq!(
        values::<MenuBarChosen>(&h.messages()),
        [MenuBarChosen { menu: 1, item: 1 }]
    );
    assert!(h.menus().is_empty());
    assert_eq!(h.tree.get::<MenuBar>(b).unwrap().open_menu(), None);
    h.hover(title(&h, b, 2));
    assert!(h.menus().is_empty());
    h.click(title(&h, b, 2));
    h.click(title(&h, b, 2));
    assert!(h.menus().is_empty());
}

#[test]
fn menu_bar_keyboard_moves_between_titles_and_menus() {
    let mut h = Harness::new();
    let b = bar(&mut h);
    h.tree.set_focus(Some(b));
    h.key(Key::ArrowRight);
    h.key(Key::ArrowDown);
    let edit = h.menus()[0];
    assert_eq!(h.menu(edit).items()[0].label(), "Cut");
    assert_eq!(h.menu(edit).highlighted(), Some(0));
    h.key(Key::ArrowRight); // Cut has no submenu: the next menu, Help
    let help = h.menus()[0];
    assert_eq!(h.menu(help).items()[0].label(), "About");
    h.key(Key::ArrowRight);
    let file = h.menus()[0];
    assert_eq!(h.menu(file).items()[0].label(), "New");
    h.key(Key::ArrowDown); // Open Recent
    h.key(Key::ArrowRight);
    assert_eq!(h.menus().len(), 2);
    h.key(Key::ArrowDown);
    h.key(Key::Enter); // b.txt
    assert_eq!(
        values::<MenuBarChosen>(&h.messages()),
        [MenuBarChosen { menu: 0, item: 3 }]
    );
    h.tree.set_focus(Some(b));
    h.key(Key::Enter);
    h.key(Key::ArrowLeft);
    assert_eq!(h.tree.get::<MenuBar>(b).unwrap().open_menu(), Some(2));
    h.key(Key::Escape);
    assert!(h.menus().is_empty());
    assert_eq!(h.tree.focused(), Some(b));
}

#[test]
fn menu_bar_shortcuts_choose_items_without_opening_menus() {
    let mut h = Harness::new();
    let b = bar(&mut h);
    let field = h.add(noa_gpui_core::widgets::TextField::new("", 100.0));
    h.tree.set_focus(Some(field));
    h.key_with(Key::Char('n'), CMD);
    // ⇧⌘G sits in the second menu at depth-first index 10.
    h.key_with(Key::Char('g'), Modifiers { shift: true, ..CMD });
    assert_eq!(
        values::<MenuBarChosen>(&h.messages()),
        [
            MenuBarChosen { menu: 0, item: 0 },
            MenuBarChosen { menu: 1, item: 10 }
        ]
    );
    assert!(h.menus().is_empty());
    h.key_with(Key::Char('w'), CMD);
    h.tree.set_hidden(b, true);
    h.key_with(Key::Char('n'), CMD);
    assert!(values::<MenuBarChosen>(&h.messages()).is_empty());
}

fn toolbar_items() -> Vec<ToolbarItem> {
    vec![
        ToolbarItem::button("Back").symbol(ToolbarSymbol::Back),
        ToolbarItem::button("Forward")
            .symbol(ToolbarSymbol::Forward)
            .enabled(false),
        ToolbarItem::separator(),
        ToolbarItem::button("Add").symbol(ToolbarSymbol::Add),
        ToolbarItem::flexible_space(),
        ToolbarItem::button("Share").symbol(ToolbarSymbol::Share),
        ToolbarItem::button("Info").symbol(ToolbarSymbol::Info),
        ToolbarItem::search("Search", 200.0, 100.0),
    ]
}

fn toolbar(h: &mut Harness, width: f32) -> WidgetId {
    let holder = h.add(Container::new(Style {
        size: LayoutSize {
            width: length(width),
            height: length(60.0),
        },
        flex_direction: FlexDirection::Column,
        ..Default::default()
    }));
    h.add_to(holder, Toolbar::new(toolbar_items()))
}

fn tb(h: &Harness, id: WidgetId) -> &Toolbar {
    h.tree.get::<Toolbar>(id).unwrap()
}

#[test]
fn wide_toolbars_show_every_item_and_push_later_ones_right() {
    let mut h = Harness::new();
    let t = toolbar(&mut h, 800.0);
    assert!(tb(&h, t).overflow_items().is_empty());
    let search = tb(&h, t).item_rect(7).unwrap();
    assert_eq!(search.width, 200.0);
    assert_eq!(
        search.right(),
        800.0 - 8.0,
        "the flexible space pushes it right"
    );
    let back = center(tb(&h, t).item_rect(0).unwrap());
    let forward = center(tb(&h, t).item_rect(1).unwrap());
    let origin = h.rect(t);
    h.click(Point::new(origin.x + back.x, origin.y + back.y));
    h.click(Point::new(origin.x + forward.x, origin.y + forward.y));
    assert_eq!(
        values::<ToolbarItemClicked>(&h.messages()),
        [ToolbarItemClicked(0)]
    );
}

#[test]
fn narrow_toolbars_shrink_the_search_field_then_move_items_to_the_overflow_menu() {
    let mut h = Harness::new();
    // Buttons are 40 wide, spaced 6: the search field shrinks first.
    let t = toolbar(&mut h, 420.0);
    assert!(tb(&h, t).overflow_items().is_empty());
    assert!(tb(&h, t).item_rect(7).unwrap().width < 200.0);
    assert!(tb(&h, t).item_rect(7).unwrap().width >= 100.0);

    let mut h = Harness::new();
    let t = toolbar(&mut h, 250.0);
    let hidden = tb(&h, t).overflow_items();
    assert!(hidden.contains(&7) && hidden.contains(&6), "{hidden:?}");
    assert!(tb(&h, t).item_rect(0).is_some());
    let origin = h.rect(t);
    h.click(Point::new(origin.right() - 8.0 - 13.0, origin.y + 27.0));
    let menu = h.menus()[0];
    let labels: Vec<&str> = h.menu(menu).items().iter().map(|i| i.label()).collect();
    assert_eq!(labels.last(), Some(&"Search…"));
    assert!(labels.contains(&"Info"));
    let info = labels.iter().position(|&l| l == "Info").unwrap();
    h.click(h.row(menu, info));
    assert_eq!(
        values::<ToolbarItemClicked>(&h.messages()),
        [ToolbarItemClicked(6)]
    );
}

#[test]
fn the_toolbar_search_field_takes_typing_and_reports_it() {
    let mut h = Harness::new();
    let t = toolbar(&mut h, 800.0);
    let origin = h.rect(t);
    let field = tb(&h, t).item_rect(7).unwrap();
    h.click(Point::new(
        origin.x + field.x + 60.0,
        origin.y + field.y + 10.0,
    ));
    assert_eq!(h.tree.focused(), Some(t));
    h.type_text("owl");
    assert_eq!(tb(&h, t).search_text(), "owl");
    h.key(Key::Enter);
    let messages = h.messages();
    assert_eq!(
        values::<TextChanged>(&messages).last(),
        Some(&TextChanged("owl".into()))
    );
    assert_eq!(
        values::<TextSubmitted>(&messages),
        [TextSubmitted("owl".into())]
    );
    let add = center(tb(&h, t).item_rect(3).unwrap());
    h.click(Point::new(origin.x + add.x, origin.y + add.y));
    h.type_text("x");
    assert_eq!(tb(&h, t).search_text(), "owl");
}

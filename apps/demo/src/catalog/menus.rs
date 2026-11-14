use ui_core::style::{AlignItems, FlexDirection, Size as LayoutSize, length};
use ui_core::widgets::{
    Container, ContextMenuArea, Label, MenuBar, MenuBarChosen, MenuBarMenu, MenuChosen, MenuItem,
    SegmentMode, SegmentedControl, SelectionChanged, Switch, TextChanged, TextField, TextSubmitted,
    Toggled, Toolbar, ToolbarDisplay, ToolbarItem, ToolbarItemClicked, ToolbarSymbol, menu_item,
};
use ui_core::{Message, Role, Shortcut, Style, WidgetId};

use super::kit::{Kit, gap};
use super::{Demo, DemoCx, SectionSpec};

pub const SECTIONS: [SectionSpec; 4] = [
    SectionSpec {
        title: "Context menu",
        description: "Right-click (or Ctrl-click on a Mac) the box, or focus the field and \
                      press Shift+F10 or the context menu key. Items can have checkmarks, \
                      shortcuts, submenus and be disabled. Hover or Right opens a submenu, \
                      Left closes it, typing jumps to an item, Escape closes every level.",
        build: context_menu,
    },
    SectionSpec {
        title: "Menu bar",
        description: "The application's menus (File, View, Go, Help). On macOS the desktop \
                      app gives them to the system menu bar, since the global menu bar cannot \
                      be drawn by the app; on Windows, Linux and the Web they are drawn in the \
                      window, at the top of this catalog. The bar below is a second, \
                      self-contained one: click a title, hover across titles, use Left / Right \
                      inside a menu, or press an item's shortcut.",
        build: menu_bar,
    },
    SectionSpec {
        title: "Toolbar",
        description: "Buttons with built-in symbols, spaces, a flexible space, a separator and \
                      a search field. When the window is too narrow, the search field shrinks \
                      first, then items move into the » menu from the right.",
        build: toolbar,
    },
    SectionSpec {
        title: "Menu bar extra",
        description: "An icon in the macOS menu bar (a tray icon on Windows) with its own menu. \
                      It needs the operating system, so it exists only in the desktop app on \
                      macOS and Windows; the switch below shows or hides it.",
        build: menu_bar_extra,
    },
];

fn status_label(kit: &mut Kit, parent: WidgetId, text: &str) -> WidgetId {
    kit.tree.insert(
        Some(parent),
        Label::new(text).font_size(12.0).color(Role::Muted),
    )
}

fn set_label(cx: &mut DemoCx, label: WidgetId, text: String) {
    cx.tree
        .update::<Label, _>(label, |l, cx| l.set_text(cx, text));
}

fn edit_items(grid: bool) -> Vec<MenuItem> {
    vec![
        MenuItem::new("Cut").key(Shortcut::command('x')),
        MenuItem::new("Copy").key(Shortcut::command('c')),
        MenuItem::new("Paste").enabled(false),
        MenuItem::separator(),
        MenuItem::new("Share").submenu(vec![
            MenuItem::new("Mail"),
            MenuItem::new("Messages").submenu(vec![
                MenuItem::new("Alice"),
                MenuItem::new("Bob"),
                MenuItem::new("Carol"),
            ]),
            MenuItem::new("AirDrop").enabled(false),
        ]),
        MenuItem::separator(),
        MenuItem::new("Show Grid")
            .checked(grid)
            .key(Shortcut::command('g').with_shift()),
    ]
}

struct ContextDemo {
    area: WidgetId,
    status: WidgetId,
    grid: bool,
}

impl Demo for ContextDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.source != self.area {
            return false;
        }
        if let Some(&MenuChosen(i)) = message.downcast_ref() {
            let items = edit_items(self.grid);
            let label = menu_item(&items, i).map_or("", MenuItem::label).to_owned();
            if label == "Show Grid" {
                self.grid = !self.grid;
                let items = edit_items(self.grid);
                cx.tree
                    .update::<ContextMenuArea, _>(self.area, |a, cx| a.set_items(cx, items));
            }
            set_label(cx, self.status, format!("chose {label} (item {i})"));
        }
        true
    }
}

fn context_menu(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.wide_card(
        "ContextMenuArea::new(style, items) · MenuItem::new(..).key(..).submenu(..)",
        420.0,
    );
    let area = kit.tree.insert(
        Some(card),
        ContextMenuArea::new(
            Style {
                flex_direction: FlexDirection::Column,
                ..Default::default()
            },
            edit_items(true),
        ),
    );
    let well = kit.tree.insert(
        Some(area),
        Container::new(Style {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::CENTER,
            justify_content: ui_core::style::JustifyContent::CENTER,
            gap: gap(0.0, 10.0),
            size: LayoutSize {
                width: length(380.0),
                height: length(150.0),
            },
            ..Default::default()
        })
        .background(Role::SurfaceAlt)
        .border(1.0, Role::Outline)
        .radius(8.0),
    );
    kit.tree.insert(
        Some(well),
        Label::new("Right-click or Ctrl-click here").color(Role::Muted),
    );
    let field = kit.tree.insert(
        Some(well),
        TextField::new("Focus me, then Shift+F10", 220.0),
    );
    kit.tree.set_accessible_label(field, "Sample text");
    let status = status_label(kit, card, "nothing chosen yet");
    Box::new(ContextDemo {
        area,
        status,
        grid: true,
    })
}

fn document_menus(bold: bool) -> Vec<MenuBarMenu> {
    vec![
        MenuBarMenu::new(
            "Document",
            vec![
                MenuItem::new("Duplicate"),
                MenuItem::new("Rename…"),
                MenuItem::new("Move To").submenu(vec![
                    MenuItem::new("Desktop"),
                    MenuItem::new("Documents"),
                    MenuItem::separator(),
                    MenuItem::new("Other…"),
                ]),
                MenuItem::separator(),
                MenuItem::new("Revert").enabled(false),
            ],
        ),
        MenuBarMenu::new(
            "Format",
            vec![
                MenuItem::new("Bold")
                    .checked(bold)
                    .key(Shortcut::command('b')),
                MenuItem::new("Italic").key(Shortcut::command('i')),
                MenuItem::separator(),
                MenuItem::new("Text Size").submenu(vec![
                    MenuItem::new("Bigger").key(Shortcut::command('=')),
                    MenuItem::new("Smaller").key(Shortcut::command('-')),
                ]),
            ],
        ),
        MenuBarMenu::new(
            "Share",
            vec![MenuItem::new("Mail"), MenuItem::new("Messages")],
        ),
    ]
}

struct MenuBarDemo {
    bar: WidgetId,
    status: WidgetId,
    app: WidgetId,
    bold: bool,
}

impl Demo for MenuBarDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.source != self.bar {
            return false;
        }
        if let Some(&MenuBarChosen { menu, item }) = message.downcast_ref() {
            let menus = document_menus(self.bold);
            let title = menus[menu].title.clone();
            let label = menu_item(&menus[menu].items, item)
                .map_or("", MenuItem::label)
                .to_owned();
            if label == "Bold" {
                self.bold = !self.bold;
                let menus = document_menus(self.bold);
                cx.tree
                    .update::<MenuBar, _>(self.bar, |b, cx| b.set_menus(cx, menus));
            }
            set_label(cx, self.status, format!("chose {title} › {label}"));
        }
        true
    }

    fn app_menu_chosen(&mut self, cx: &mut DemoCx, path: &str) {
        set_label(cx, self.app, format!("app menus: chose {path}"));
    }
}

fn menu_bar(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.wide_card(
        "MenuBar::new(menus) · MenuBarMenu::new(title, items)",
        560.0,
    );
    let frame = kit.tree.insert(
        Some(card),
        Container::new(Style {
            flex_direction: FlexDirection::Column,
            size: LayoutSize {
                width: length(520.0),
                height: length(120.0),
            },
            ..Default::default()
        })
        .background(Role::Background)
        .border(1.0, Role::Outline)
        .clip(true),
    );
    let bar = kit
        .tree
        .insert(Some(frame), MenuBar::new(document_menus(false)));
    kit.tree.set_accessible_label(bar, "Document menus");
    let status = status_label(kit, card, "nothing chosen yet");
    let app = status_label(kit, card, "app menus: nothing chosen yet");
    Box::new(MenuBarDemo {
        bar,
        status,
        app,
        bold: false,
    })
}

fn toolbar_items() -> Vec<ToolbarItem> {
    vec![
        ToolbarItem::button("Sidebar").symbol(ToolbarSymbol::Sidebar),
        ToolbarItem::space(),
        ToolbarItem::button("Back").symbol(ToolbarSymbol::Back),
        ToolbarItem::button("Forward")
            .symbol(ToolbarSymbol::Forward)
            .enabled(false),
        ToolbarItem::flexible_space(),
        ToolbarItem::button("New").symbol(ToolbarSymbol::Add),
        ToolbarItem::separator(),
        ToolbarItem::button("Share").symbol(ToolbarSymbol::Share),
        ToolbarItem::button("Info").symbol(ToolbarSymbol::Info),
        ToolbarItem::search("Search", 180.0, 110.0),
    ]
}

struct ToolbarDemo {
    toolbars: Vec<WidgetId>,
    mode: WidgetId,
    status: WidgetId,
}

impl Demo for ToolbarDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.source == self.mode
            && let Some(&SelectionChanged(i)) = message.downcast_ref()
        {
            let display = [
                ToolbarDisplay::IconAndLabel,
                ToolbarDisplay::IconOnly,
                ToolbarDisplay::LabelOnly,
            ][i];
            for &t in &self.toolbars {
                cx.tree
                    .update::<Toolbar, _>(t, |t, cx| t.set_display(cx, display));
            }
            return true;
        }
        if !self.toolbars.contains(&message.source) {
            return false;
        }
        let text = if let Some(&ToolbarItemClicked(i)) = message.downcast_ref() {
            format!("clicked {}", toolbar_items()[i].label())
        } else if let Some(TextChanged(s)) = message.downcast_ref() {
            format!("searching for \"{s}\"")
        } else if let Some(TextSubmitted(s)) = message.downcast_ref() {
            format!("searched for \"{s}\"")
        } else {
            return true;
        };
        set_label(cx, self.status, text);
        true
    }
}

fn toolbar(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.wide_card(
        "Toolbar::new(items) · ToolbarItem::button(..).symbol(..) / ::search(..)",
        720.0,
    );
    let mode = kit.tree.insert(
        Some(card),
        SegmentedControl::new(
            ["Icon and label", "Icon only", "Label only"],
            SegmentMode::SelectOne,
        )
        .with_selected(&[0]),
    );
    kit.tree.set_accessible_label(mode, "Toolbar display");
    let mut toolbars = Vec::new();
    for (width, name) in [(680.0, "Actions, wide"), (340.0, "Actions, narrow")] {
        let frame = kit.tree.insert(
            Some(card),
            Container::new(Style {
                flex_direction: FlexDirection::Column,
                size: LayoutSize {
                    width: length(width),
                    height: ui_core::style::auto(),
                },
                ..Default::default()
            })
            .border(1.0, Role::Outline)
            .clip(true),
        );
        let toolbar = kit.tree.insert(Some(frame), Toolbar::new(toolbar_items()));
        kit.tree.set_accessible_label(toolbar, name);
        toolbars.push(toolbar);
    }
    let status = status_label(kit, card, "the narrow one keeps the rest in its » menu");
    Box::new(ToolbarDemo {
        toolbars,
        mode,
        status,
    })
}

struct ExtraDemo {
    switch: Option<WidgetId>,
    status: WidgetId,
}

impl ExtraDemo {
    fn describe(cx: &DemoCx) -> String {
        let item = &cx.status_item;
        if item.visible {
            format!("shown · count {}", item.count)
        } else {
            format!("hidden · count {}", item.count)
        }
    }
}

impl Demo for ExtraDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if Some(message.source) != self.switch {
            return false;
        }
        if let Some(&Toggled(on)) = message.downcast_ref() {
            cx.status_item.visible = on;
            cx.status_item.changed = true;
            let text = Self::describe(cx);
            set_label(cx, self.status, text);
        }
        true
    }

    #[cfg(all(
        feature = "native",
        any(target_os = "macos", target_os = "windows", target_os = "linux")
    ))]
    fn status_menu_chosen(&mut self, cx: &mut DemoCx, label: &str) {
        if let Some(switch) = self.switch {
            let on = cx.status_item.visible;
            cx.tree
                .update::<Switch, _>(switch, |s, cx| s.set_on(cx, on));
        }
        let text = format!("{} · chose {label}", Self::describe(cx));
        set_label(cx, self.status, text);
    }
}

fn menu_bar_extra(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.card("Screen::status_menu() → menu bar extra / tray icon (desktop)");
    let available = cfg!(all(
        feature = "native",
        any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux"
        )
    ));
    if !available {
        let note = "The Web has no menu bar to add an icon to.";
        let status = kit
            .tree
            .insert(Some(card), Label::new(note).color(Role::Muted).wrap(true));
        return Box::new(ExtraDemo {
            switch: None,
            status,
        });
    }
    let row = kit.tree.insert(
        Some(card),
        Container::new(Style {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::CENTER,
            gap: gap(10.0, 0.0),
            ..Default::default()
        }),
    );
    let switch = kit.tree.insert(Some(row), Switch::new(true));
    kit.tree
        .set_accessible_label(switch, "Show in the menu bar");
    kit.tree
        .insert(Some(row), Label::new("Show in the menu bar"));
    let status = status_label(kit, card, "shown · count 0");
    Box::new(ExtraDemo {
        switch: Some(switch),
        status,
    })
}

use ui_core::style::{
    AlignItems, Display, FlexDirection, GridTemplateComponent, JustifyContent, Rect as LayoutRect,
    Size as LayoutSize, fr, length, percent,
};
use ui_core::widgets::{
    Alert, Badge, Button, ButtonClicked, ButtonStyle, Container, Label, MenuBar, MenuBarChosen,
    MenuBarMenu, MenuItem, OutlineItem, OutlineView, RowSelectionChanged, SearchField,
    SelectionMode, TableColumn, TableView, TextArea, TextChanged, WindowEvent, WindowManager,
    menu_item,
};
use ui_core::{
    Appearance, Design, Event, Message, OverlayDismissed, Role, Style, UiTree, WidgetId,
};
use ui_scene::{Color, Rect, Size};
use ui_text::TextStore;

use crate::bench::Meter;
use ui_platform::{FrameReport, Screen};

mod notifications;
mod switcher;

use notifications::NotificationCenter;
#[cfg(test)]
use switcher::SwitcherView;
use switcher::{SwitchStep, Switcher, SwitcherPicked};

const APPS: [&str; 4] = ["Finder", "Notes", "Calculator", "Settings"];

/// A file in the Finder: name, kind, size, date modified.
type File = (&'static str, &'static str, &'static str, &'static str);

const FOLDERS: [(&str, &[File]); 4] = [
    (
        "Desktop",
        &[
            ("Screenshot 1.png", "PNG image", "1.2 MB", "Nov 12, 2026"),
            ("Screenshot 2.png", "PNG image", "840 KB", "Nov 12, 2026"),
            ("todo.txt", "Plain text", "2 KB", "Nov 3, 2026"),
        ],
    ),
    (
        "Documents",
        &[
            ("Budget.numbers", "Spreadsheet", "310 KB", "Oct 30, 2026"),
            ("Letter.pages", "Document", "96 KB", "Sep 18, 2026"),
            ("Notes", "Folder", "—", "Aug 2, 2026"),
            ("Report.pdf", "PDF document", "2.4 MB", "Nov 10, 2026"),
            ("Resume.pdf", "PDF document", "180 KB", "Jun 1, 2026"),
        ],
    ),
    (
        "Downloads",
        &[
            ("installer.dmg", "Disk image", "64 MB", "Nov 13, 2026"),
            ("photo.jpg", "JPEG image", "3.1 MB", "Oct 21, 2026"),
        ],
    ),
    (
        "Applications",
        &[
            ("Calculator", "Application", "5 MB", "Jan 5, 2026"),
            ("Finder", "Application", "12 MB", "Jan 5, 2026"),
            ("Notes", "Application", "9 MB", "Jan 5, 2026"),
            ("Settings", "Application", "7 MB", "Jan 5, 2026"),
        ],
    ),
];

const NOTES: [(&str, &str); 3] = [
    (
        "Welcome",
        "This is a tiny desktop running inside one window.\n\nDrag windows by their title bars, resize them from the edges, and send them to the dock with the yellow button.",
    ),
    ("Groceries", "Milk\nBread\nCoffee beans\nApples"),
    (
        "Ideas",
        "A status bar clock.\nA Trash that opens.\nMore apps.",
    ),
];

const ACCENTS: [(&str, u32); 5] = [
    ("Blue", 0x2f6fde),
    ("Purple", 0x8e5bd8),
    ("Pink", 0xd6455d),
    ("Orange", 0xe8812d),
    ("Green", 0x2f9e5b),
];

const KEYS: [&str; 20] = [
    "C", "±", "%", "÷", "7", "8", "9", "×", "4", "5", "6", "−", "1", "2", "3", "+", "0", ".", "⌫",
    "=",
];

#[derive(Debug, Default)]
struct Calculator {
    entry: String,
    left: Option<f64>,
    op: Option<char>,
    /// The next digit starts a new number (after an operator or `=`).
    fresh: bool,
}

impl Calculator {
    fn value(&self) -> f64 {
        self.entry.parse().unwrap_or(0.0)
    }

    fn display(&self) -> String {
        if self.entry.is_empty() {
            "0".into()
        } else {
            self.entry.clone()
        }
    }

    fn set(&mut self, value: f64) {
        self.entry = if value.is_finite() {
            // Up to 10 significant decimals, without trailing zeros.
            let text = format!("{value:.10}");
            text.trim_end_matches('0').trim_end_matches('.').to_owned()
        } else {
            "Error".into()
        };
    }

    fn apply(&mut self) {
        if let (Some(left), Some(op)) = (self.left.take(), self.op.take()) {
            let right = self.value();
            self.set(match op {
                '+' => left + right,
                '−' => left - right,
                '×' => left * right,
                _ => left / right,
            });
        }
    }

    fn press(&mut self, key: &str) {
        match key {
            "C" => *self = Self::default(),
            "±" => {
                let value = -self.value();
                self.set(value);
            }
            "%" => {
                let value = self.value() / 100.0;
                self.set(value);
            }
            "⌫" => {
                self.entry.pop();
            }
            "=" => {
                self.apply();
                self.fresh = true;
            }
            "+" | "−" | "×" | "÷" => {
                self.apply();
                self.left = Some(self.value());
                self.op = key.chars().next();
                self.fresh = true;
            }
            digit => {
                if self.fresh || self.entry == "Error" {
                    self.entry.clear();
                    self.fresh = false;
                }
                if digit == "." {
                    if self.entry.contains('.') {
                        return;
                    }
                    if self.entry.is_empty() {
                        self.entry.push('0');
                    }
                }
                self.entry.push_str(digit);
            }
        }
    }
}

enum App {
    Finder {
        sidebar: WidgetId,
        search: WidgetId,
        table: WidgetId,
    },
    Notes {
        list: WidgetId,
        area: WidgetId,
        search: WidgetId,
        new: WidgetId,
        delete: WidgetId,
        texts: Vec<String>,
        /// `None` when no note is shown.
        current: Option<usize>,
    },
    Calculator {
        display: WidgetId,
        keys: Vec<WidgetId>,
        calc: Calculator,
    },
    Settings {
        designs: [WidgetId; Design::ALL.len()],
        appearance: [WidgetId; 2],
        accents: Vec<WidgetId>,
    },
}

pub struct MiniOs {
    tree: UiTree,
    manager: WindowManager,
    menu_bar: WidgetId,
    menus: Vec<MenuBarMenu>,
    launcher: Vec<WidgetId>,
    status: WidgetId,
    apps: Vec<(WidgetId, App)>,
    about: Option<WidgetId>,
    notifications: NotificationCenter,
    bell: WidgetId,
    badge: WidgetId,
    unread_shown: usize,
    switcher: Switcher,
    meter: Meter,
}

fn column(gap: f32, padding: f32) -> Style {
    Style {
        flex_direction: FlexDirection::Column,
        gap: LayoutSize {
            width: length(0.0),
            height: length(gap),
        },
        padding: LayoutRect::length(padding),
        flex_grow: 1.0,
        ..Default::default()
    }
}

fn row(gap: f32) -> Style {
    Style {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::CENTER,
        gap: LayoutSize {
            width: length(gap),
            height: length(0.0),
        },
        ..Default::default()
    }
}

/// A note's title: its first line that is not blank.
fn note_title(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("New Note")
        .to_owned()
}

fn note_titles(texts: &[String]) -> Vec<Vec<String>> {
    texts.iter().map(|t| vec![note_title(t)]).collect()
}

/// Whether `text` holds each word of `query`, in any letter case.
fn holds_words(text: &str, query: &str) -> bool {
    let text = text.to_lowercase();
    query
        .split_whitespace()
        .all(|word| text.contains(&word.to_lowercase()))
}

fn show_note(
    tree: &mut UiTree,
    (list, area): (WidgetId, WidgetId),
    texts: &[String],
    current: &mut Option<usize>,
    row: Option<usize>,
) {
    *current = row.filter(|&r| r < texts.len());
    let text = current.map_or(String::new(), |r| texts[r].clone());
    tree.update::<TextArea, _>(area, |a, cx| a.set_text(cx, text));
    tree.update::<TableView, _>(list, |t, cx| t.select_rows(cx, *current));
}

fn files(folder: usize) -> Vec<Vec<String>> {
    FOLDERS[folder]
        .1
        .iter()
        .map(|(name, kind, size, modified)| {
            vec![
                (*name).into(),
                (*kind).into(),
                (*size).into(),
                (*modified).into(),
            ]
        })
        .collect()
}

impl MiniOs {
    pub fn new() -> Self {
        let mut tree = UiTree::new();
        tree.set_shortcut_glyphs(ui_platform::command_is_meta());
        let root = tree.insert(
            None,
            Container::new(Style {
                flex_direction: FlexDirection::Column,
                size: LayoutSize {
                    width: percent(1.0),
                    height: percent(1.0),
                },
                ..Default::default()
            })
            .background(Role::Background),
        );
        let menus = vec![
            MenuBarMenu::new(
                "noa",
                vec![
                    MenuItem::new("About This Mini OS"),
                    MenuItem::separator(),
                    MenuItem::new("Settings…"),
                    MenuItem::separator(),
                    MenuItem::new("Notification Center"),
                    MenuItem::new("Send Test Notification"),
                ],
            ),
            MenuBarMenu::new("Apps", APPS.iter().map(|a| MenuItem::new(*a)).collect()),
            MenuBarMenu::new(
                "Window",
                vec![
                    MenuItem::new("Minimize"),
                    MenuItem::new("Zoom"),
                    MenuItem::new("Close"),
                    MenuItem::separator(),
                    MenuItem::new("Minimize All"),
                    MenuItem::new("Switch Windows…"),
                ],
            ),
        ];
        let menu_bar = tree.insert(Some(root), MenuBar::new(menus.clone()));
        tree.set_accessible_label(menu_bar, "App menus");
        let manager = WindowManager::new(
            &mut tree,
            root,
            Style {
                flex_grow: 1.0,
                flex_basis: length(0.0),
                min_size: LayoutSize {
                    width: length(0.0),
                    height: length(0.0),
                },
                ..Default::default()
            },
        );
        let bar = tree.insert(
            Some(root),
            Container::new(Style {
                justify_content: JustifyContent::CENTER,
                padding: LayoutRect::length(8.0),
                flex_shrink: 0.0,
                ..row(8.0)
            })
            .background(Role::Surface),
        );
        let launcher = APPS
            .iter()
            .map(|app| {
                tree.insert(
                    Some(bar),
                    Button::new(*app)
                        .style(ButtonStyle::Textured)
                        .tooltip(format!("Open {app}")),
                )
            })
            .collect();
        let status = tree.insert(Some(bar), Label::new("").font_size(11.0).color(Role::Muted));
        let bell = tree.insert(
            Some(bar),
            Button::new("Notifications")
                .style(ButtonStyle::Textured)
                .tooltip("Notification Center"),
        );
        let badge = tree.insert(Some(bar), Badge::count(0));
        let mut os = Self {
            meter: Meter::default(),
            tree,
            manager,
            menu_bar,
            menus,
            launcher,
            status,
            apps: Vec::new(),
            about: None,
            notifications: NotificationCenter::default(),
            bell,
            badge,
            unread_shown: 0,
            switcher: Switcher::default(),
        };
        os.launch("Finder");
        os.launch("Notes");
        os.notifications.post(
            &mut os.tree,
            None,
            "Mini OS",
            "Welcome",
            "Press ⌥Tab (Ctrl+Tab elsewhere) to switch windows.",
        );
        os.sync_badge();
        os
    }

    /// Calculator and Settings have a single window, which comes forward.
    fn launch(&mut self, app: &str) {
        let single = matches!(app, "Calculator" | "Settings");
        let existing = self.apps.iter().find(|(_, a)| {
            matches!(
                (app, a),
                ("Calculator", App::Calculator { .. }) | ("Settings", App::Settings { .. })
            )
        });
        if let (true, Some(&(window, _))) = (single, existing) {
            self.manager.activate(&mut self.tree, window);
            return;
        }
        let n = self.apps.len() as f32;
        let at = |w: f32, h: f32| {
            Some(Rect::new(
                40.0 + (n % 5.0) * 120.0,
                24.0 + (n % 5.0) * 56.0,
                w,
                h,
            ))
        };
        let tree = &mut self.tree;
        let (window, state) = match app {
            "Finder" => {
                let window = self.manager.open(tree, "Finder", at(520.0, 300.0));
                let body = tree.insert(
                    Some(window),
                    Container::new(Style {
                        flex_grow: 1.0,
                        align_items: AlignItems::STRETCH,
                        ..row(0.0)
                    }),
                );
                let sidebar = tree.insert(
                    Some(body),
                    OutlineView::new(vec![
                        OutlineItem::new("Favorites")
                            .children(FOLDERS.iter().map(|(name, _)| OutlineItem::new(*name))),
                    ])
                    .sidebar()
                    .selection_mode(SelectionMode::Single)
                    .width(150.0)
                    .height(260.0),
                );
                tree.set_accessible_label(sidebar, "Sidebar");
                let content = tree.insert(
                    Some(body),
                    Container::new(Style {
                        align_items: AlignItems::END,
                        ..column(6.0, 6.0)
                    }),
                );
                let search =
                    tree.insert(Some(content), SearchField::new("Search this folder", 180.0));
                tree.set_accessible_label(search, "Search files");
                let table = tree.insert(
                    Some(content),
                    TableView::new(
                        vec![
                            TableColumn::new("Name", 170.0).hideable(false),
                            TableColumn::new("Kind", 110.0),
                            TableColumn::new("Size", 70.0),
                            // Shown from the header's menu (right-click a title).
                            TableColumn::new("Date Modified", 110.0).hidden(true),
                        ],
                        files(1),
                    )
                    .column_chooser(true)
                    .width(354.0)
                    .height(224.0),
                );
                tree.set_accessible_label(table, "Files");
                (
                    window,
                    App::Finder {
                        sidebar,
                        search,
                        table,
                    },
                )
            }
            "Notes" => {
                let window = self.manager.open(tree, "Notes", at(470.0, 300.0));
                let body = tree.insert(
                    Some(window),
                    Container::new(Style {
                        flex_grow: 1.0,
                        align_items: AlignItems::START,
                        padding: LayoutRect::length(8.0),
                        ..row(8.0)
                    }),
                );
                let side = tree.insert(
                    Some(body),
                    Container::new(Style {
                        flex_grow: 0.0,
                        ..column(6.0, 0.0)
                    }),
                );
                let search = tree.insert(Some(side), SearchField::new("Search", 140.0));
                tree.set_accessible_label(search, "Search notes");
                let texts: Vec<String> = NOTES
                    .iter()
                    .map(|(title, body)| format!("{title}\n{body}"))
                    .collect();
                let list = tree.insert(
                    Some(side),
                    TableView::new(vec![TableColumn::new("Notes", 130.0)], note_titles(&texts))
                        .selection_mode(SelectionMode::Single)
                        .width(140.0)
                        .height(180.0),
                );
                tree.set_accessible_label(list, "Notes");
                let buttons = tree.insert(Some(side), Container::new(row(6.0)));
                let new = tree.insert(Some(buttons), Button::new("New"));
                let delete = tree.insert(Some(buttons), Button::new("Delete"));
                let area = tree.insert(
                    Some(body),
                    TextArea::new("Write a note…").width(300.0).min_lines(10),
                );
                tree.set_accessible_label(area, "Note");
                let mut current = None;
                show_note(tree, (list, area), &texts, &mut current, Some(0));
                (
                    window,
                    App::Notes {
                        list,
                        area,
                        search,
                        new,
                        delete,
                        texts,
                        current,
                    },
                )
            }
            "Calculator" => {
                let window = self.manager.open(tree, "Calculator", at(230.0, 300.0));
                let body = tree.insert(Some(window), Container::new(column(8.0, 10.0)));
                let screen = tree.insert(
                    Some(body),
                    Container::new(Style {
                        justify_content: JustifyContent::END,
                        padding: LayoutRect::length(8.0),
                        ..row(0.0)
                    })
                    .background(Role::SurfaceAlt)
                    .radius(6.0),
                );
                let display = tree.insert(Some(screen), Label::new("0").font_size(24.0));
                let grid = tree.insert(
                    Some(body),
                    Container::new(Style {
                        display: Display::Grid,
                        grid_template_columns: vec![GridTemplateComponent::Single(fr(1.0)); 4],
                        gap: LayoutSize {
                            width: length(6.0),
                            height: length(6.0),
                        },
                        ..Default::default()
                    }),
                );
                let keys = KEYS
                    .iter()
                    .map(|key| {
                        let button = Button::new(*key).default_button(*key == "=");
                        tree.insert(Some(grid), button)
                    })
                    .collect();
                (
                    window,
                    App::Calculator {
                        display,
                        keys,
                        calc: Calculator::default(),
                    },
                )
            }
            _ => {
                let window = self.manager.open(tree, "Settings", at(340.0, 300.0));
                let body = tree.insert(Some(window), Container::new(column(10.0, 14.0)));
                tree.insert(Some(body), Label::new("Style").weight(600));
                let styles = tree.insert(
                    Some(body),
                    Container::new(Style {
                        flex_wrap: ui_core::style::FlexWrap::Wrap,
                        ..row(8.0)
                    }),
                );
                let designs = Design::ALL.map(|d| tree.insert(Some(styles), Button::new(d.name())));
                tree.insert(Some(body), Label::new("Appearance").weight(600));
                let modes = tree.insert(Some(body), Container::new(row(8.0)));
                let appearance = [
                    tree.insert(Some(modes), Button::new("Light")),
                    tree.insert(Some(modes), Button::new("Dark")),
                ];
                tree.insert(Some(body), Label::new("Accent color").weight(600));
                let colors = tree.insert(
                    Some(body),
                    Container::new(Style {
                        flex_wrap: ui_core::style::FlexWrap::Wrap,
                        ..row(8.0)
                    }),
                );
                let accents = ACCENTS
                    .iter()
                    .map(|(name, _)| tree.insert(Some(colors), Button::new(*name)))
                    .collect();
                (
                    window,
                    App::Settings {
                        designs,
                        appearance,
                        accents,
                    },
                )
            }
        };
        // The first window of each app keeps its frame between runs (with --view-state),
        // and the Finder and Notes also keep their column widths and sidebar. Later
        // windows of the same app open where the manager puts them.
        let name = format!("minios.{app}");
        let taken = self
            .apps
            .iter()
            .any(|&(w, _)| self.tree.autosave_name(w) == Some(name.as_str()));
        if !taken {
            self.tree.set_autosave_name(window, name.as_str());
            match &state {
                App::Finder { sidebar, table, .. } => {
                    self.tree
                        .set_autosave_name(*sidebar, format!("{name}.sidebar"));
                    self.tree.set_autosave_name(*table, format!("{name}.files"));
                }
                App::Notes { list, .. } => {
                    self.tree.set_autosave_name(*list, format!("{name}.list"));
                }
                _ => {}
            }
        }
        self.apps.push((window, state));
        self.update_status();
    }

    /// The desktop's rectangle in window coordinates.
    fn desktop_rect(&self) -> Rect {
        self.tree
            .window_rect(self.manager.desktop())
            .unwrap_or_default()
    }

    fn sync_badge(&mut self) {
        let unread = self.notifications.unread();
        if unread != self.unread_shown {
            self.unread_shown = unread;
            self.tree.update::<Badge, _>(self.badge, |b, cx| {
                b.set_count(cx, u32::try_from(unread).unwrap_or(u32::MAX));
            });
        }
    }

    fn notify(&mut self, text: &mut TextStore, app: &str, title: &str, body: &str) {
        self.notifications
            .post(&mut self.tree, Some(text), app, title, body);
    }

    fn app_color(apps: &[(WidgetId, App)], window: WidgetId) -> Color {
        let kind = apps.iter().find(|(w, _)| *w == window).map(|(_, a)| a);
        Color::hex(match kind {
            Some(App::Finder { .. }) => 0x2f7de1,
            Some(App::Notes { .. }) => 0xe0a526,
            Some(App::Calculator { .. }) => 0xe8762d,
            Some(App::Settings { .. }) => 0x7a7f8c,
            None => 0x5b6170,
        })
    }

    fn update_status(&mut self) {
        let text = format!("{} windows", self.apps.len());
        self.tree
            .update::<Label, _>(self.status, |l, cx| l.set_text(cx, text));
    }

    fn about(&mut self) {
        if self.about.is_none() {
            let alert = Alert::new("Mini OS")
                .informative_text(
                    "A desktop inside a window, drawn by noa-gpui: inner windows, a menu bar, \
                     a dock and four small apps.",
                )
                .button("OK");
            self.about = Some(alert.open(&mut self.tree));
        }
    }

    fn menu_chosen(&mut self, menu: usize, item: usize, text: &mut TextStore) {
        let Some(label) = self
            .menus
            .get(menu)
            .and_then(|m| menu_item(&m.items, item))
            .map(|i| i.label().to_owned())
        else {
            return;
        };
        let active = self.manager.active();
        match label.as_str() {
            "About This Mini OS" => self.about(),
            "Settings…" => self.launch("Settings"),
            "Notification Center" => {
                let desktop = self.desktop_rect();
                self.notifications.toggle(&mut self.tree, text, desktop);
            }
            "Send Test Notification" => {
                let n = self.notifications.items().len() + 1;
                let body = format!("This is test notification number {n}.");
                self.notify(text, "Mini OS", "Test Notification", &body);
            }
            "Switch Windows…" => {
                let apps = &self.apps;
                self.switcher.open(
                    &mut self.tree,
                    &self.manager,
                    |w| Self::app_color(apps, w),
                    ui_core::Modifiers::default(),
                    false,
                );
            }
            "Minimize" => {
                if let Some(window) = active {
                    self.manager.minimize(&mut self.tree, window);
                }
            }
            "Zoom" => {
                if let Some(window) = active {
                    self.manager.zoom(&mut self.tree, window);
                }
            }
            "Close" => {
                if let Some(window) = active {
                    self.manager.close(&mut self.tree, text, window);
                    self.closed(window);
                }
            }
            "Minimize All" => {
                while let Some(window) = self.manager.active() {
                    self.manager.minimize(&mut self.tree, window);
                }
            }
            app => self.launch(app),
        }
    }

    fn closed(&mut self, window: WidgetId) {
        self.apps.retain(|(w, _)| *w != window);
        self.update_status();
    }

    fn app_message(&mut self, message: &Message) {
        let source = message.source;
        let clicked = message.downcast_ref::<ButtonClicked>().is_some();
        let tree = &mut self.tree;
        for (_, app) in &mut self.apps {
            match app {
                App::Finder { search, table, .. } if source == *search => {
                    if let Some(TextChanged(query)) = message.downcast_ref() {
                        tree.update::<TableView, _>(*table, |t, cx| t.set_filter_text(cx, query));
                    }
                    return;
                }
                App::Finder { sidebar, table, .. } if source == *sidebar => {
                    if message.downcast_ref::<RowSelectionChanged>().is_none() {
                        return;
                    }
                    // Item 0 is the "Favorites" group; the folders follow.
                    let folder = tree
                        .get::<OutlineView>(*sidebar)
                        .and_then(|o| o.selected_items().first().copied())
                        .and_then(|item| item.checked_sub(1));
                    if let Some(folder) = folder.filter(|&f| f < FOLDERS.len()) {
                        tree.update::<TableView, _>(*table, |t, cx| t.set_data(cx, files(folder)));
                    }
                    return;
                }
                App::Notes {
                    list,
                    area,
                    search,
                    new,
                    delete,
                    texts,
                    current,
                } => {
                    let views = (*list, *area);
                    if source == *area
                        && let Some(TextChanged(text)) = message.downcast_ref()
                    {
                        let Some(row) = *current else { return };
                        let retitled = note_title(&texts[row]) != note_title(text);
                        texts[row] = text.clone();
                        if retitled {
                            // The first line is the title, so the list follows typing.
                            let titles = note_titles(texts);
                            tree.update::<TableView, _>(*list, |t, cx| {
                                t.set_data_keeping_selection(cx, titles, |_, row| row)
                            });
                        }
                        return;
                    }
                    if source == *list && message.downcast_ref::<RowSelectionChanged>().is_some() {
                        let row = tree
                            .get::<TableView>(*list)
                            .and_then(|t| t.selected_rows().first().copied());
                        if row.is_some() {
                            show_note(tree, views, texts, current, row);
                        }
                        return;
                    }
                    if source == *search
                        && let Some(TextChanged(query)) = message.downcast_ref()
                    {
                        // Search the current text of each note, so notes made after the
                        // search starts are shown.
                        let found: Vec<bool> =
                            texts.iter().map(|t| holds_words(t, query)).collect();
                        tree.update::<TableView, _>(*list, |t, cx| {
                            if query.trim().is_empty() {
                                t.clear_filter(cx);
                            } else {
                                t.set_filter(cx, move |_, row| found.get(row).is_none_or(|&f| f));
                            }
                        });
                        let table = tree.get::<TableView>(*list);
                        let shown =
                            table.is_some_and(|t| current.is_some_and(|r| t.is_row_shown(r)));
                        if !shown {
                            let first = table.and_then(|t| t.displayed_rows().first().copied());
                            show_note(tree, views, texts, current, first);
                        }
                        return;
                    }
                    if clicked && source == *new {
                        texts.push(String::new());
                        tree.update::<SearchField, _>(*search, |f, cx| f.set_text(cx, ""));
                        let titles = note_titles(texts);
                        tree.update::<TableView, _>(*list, |t, cx| {
                            t.clear_filter(cx);
                            t.set_data(cx, titles);
                        });
                        show_note(tree, views, texts, current, Some(texts.len() - 1));
                        tree.set_focus(Some(*area));
                        return;
                    }
                    if clicked && source == *delete {
                        let Some(row) = *current else { return };
                        // The next note shown takes its place, or the one before it.
                        let next = tree.get::<TableView>(*list).and_then(|t| {
                            let shown = t.displayed_rows();
                            let pos = shown.iter().position(|&r| r == row)?;
                            shown
                                .get(pos + 1)
                                .or(pos.checked_sub(1).and_then(|p| shown.get(p)))
                                .copied()
                        });
                        texts.remove(row);
                        let titles = note_titles(texts);
                        tree.update::<TableView, _>(*list, |t, cx| t.set_data(cx, titles));
                        let next = next.map(|r| if r > row { r - 1 } else { r });
                        show_note(tree, views, texts, current, next);
                        return;
                    }
                }
                App::Calculator {
                    display,
                    keys,
                    calc,
                } if clicked => {
                    if let Some(key) = keys.iter().position(|&k| k == source) {
                        calc.press(KEYS[key]);
                        let text = calc.display();
                        tree.update::<Label, _>(*display, |l, cx| l.set_text(cx, text));
                        return;
                    }
                }
                App::Settings {
                    designs,
                    appearance,
                    accents,
                } if clicked => {
                    let theme = tree.theme().clone();
                    if let Some(design) = designs.iter().position(|&b| b == source) {
                        tree.set_theme(theme.with_design(Design::ALL[design]));
                        return;
                    }
                    if let Some(mode) = appearance.iter().position(|&b| b == source) {
                        let appearance = [Appearance::Light, Appearance::Dark][mode];
                        tree.set_theme(theme.with_appearance(appearance));
                        return;
                    }
                    if let Some(accent) = accents.iter().position(|&b| b == source) {
                        tree.set_theme(theme.with_accent(Color::hex(ACCENTS[accent].1)));
                        return;
                    }
                }
                _ => {}
            }
        }
    }
}

impl MiniOs {
    pub fn metered(mut self, meter: Meter) -> Self {
        self.meter = meter;
        self
    }
}

impl Default for MiniOs {
    fn default() -> Self {
        Self::new()
    }
}

impl Screen for MiniOs {
    fn tree(&self) -> &UiTree {
        &self.tree
    }

    fn tree_mut(&mut self) -> &mut UiTree {
        &mut self.tree
    }

    fn handle(&mut self, message: Message, text: &mut TextStore) {
        let source = message.source;
        if Some(source) == self.switcher.view() {
            if message.downcast_ref::<OverlayDismissed>().is_some() {
                self.switcher.dismissed(source);
            } else if message.downcast_ref::<SwitcherPicked>().is_some() {
                self.switcher
                    .choose(&mut self.tree, text, &mut self.manager);
            }
            return;
        }
        let desktop = self.desktop_rect();
        if self
            .notifications
            .handle(&mut self.tree, text, desktop, &message)
        {
            return;
        }
        if source == self.bell && message.downcast_ref::<ButtonClicked>().is_some() {
            self.notifications.toggle(&mut self.tree, text, desktop);
            return;
        }
        if let Some(event) = self.manager.handle(&mut self.tree, text, &message) {
            if let WindowEvent::Closed(window) = event {
                self.closed(window);
            }
            return;
        }
        if source == self.menu_bar
            && let Some(&MenuBarChosen { menu, item }) = message.downcast_ref()
        {
            self.menu_chosen(menu, item, text);
            return;
        }
        if Some(source) == self.about {
            // The alert was answered; it removes itself.
            self.about = None;
            return;
        }
        if message.downcast_ref::<ButtonClicked>().is_some()
            && let Some(app) = self.launcher.iter().position(|&b| b == source)
        {
            self.launch(APPS[app]);
            return;
        }
        self.app_message(&message);
    }

    /// The OS appearance is the starting point; Settings changes it afterwards.
    fn set_system_appearance(&mut self, appearance: Appearance) {
        let theme = self.tree.theme().clone().with_appearance(appearance);
        self.tree.set_theme(theme);
    }

    fn frame_presented(&mut self, report: &FrameReport<'_>) {
        let widgets = self.widget_count();
        self.meter.presented(report, Some(widgets));
    }

    fn continuous(&self) -> bool {
        self.meter.bench()
    }

    fn wants_close(&self) -> bool {
        self.meter.finished()
    }

    /// The app switcher sees keys before the tree: Tab would otherwise move the focus.
    fn dispatch(&mut self, event: Event, text: &mut TextStore) {
        let apps = &self.apps;
        let step = self.switcher.event(
            &mut self.tree,
            text,
            &mut self.manager,
            |w| Self::app_color(apps, w),
            &event,
        );
        let released = matches!(event, Event::PointerUp { .. });
        if matches!(event, Event::PointerDown { .. }) {
            self.notifications.pointer(true);
        }
        if step == SwitchStep::Pass {
            self.tree.dispatch(event, text);
        }
        for message in self.tree.take_messages() {
            self.handle(message, text);
        }
        if released {
            self.notifications.pointer(false);
        }
        self.after_input();
    }

    fn after_input(&mut self) {
        self.sync_badge();
    }

    fn prepare(&mut self, viewport: Size, text: &mut TextStore) -> bool {
        let changed = self.tree.prepare(viewport, text).is_some();
        // The panel follows the desktop when the window is resized. The desktop's
        // position is known only after layout, so the panel is laid out after it.
        let desktop = self.desktop_rect();
        if self.notifications.fit(&mut self.tree, desktop) {
            self.tree.prepare(viewport, text);
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::center_of;
    use ui_core::widgets::InnerWindow;
    use ui_core::{Key, Modifiers, PointerButton};
    use ui_scene::Point;
    use ui_text::FontOptions;

    const VIEWPORT: Size = Size::new(1100.0, 720.0);
    const ALT: Modifiers = Modifiers {
        shift: false,
        ctrl: false,
        alt: true,
        meta: false,
        command: false,
    };

    fn setup() -> (MiniOs, TextStore) {
        let mut text = TextStore::new(FontOptions {
            locale: "en-US".into(),
            load_system_fonts: false,
        });
        let mut os = MiniOs::new();
        os.prepare(VIEWPORT, &mut text);
        (os, text)
    }

    #[test]
    fn every_control_has_an_accessible_name() {
        use crate::a11y_audit::{describe, unnamed_controls};
        let (os, _text) = setup();
        let unnamed: Vec<String> = unnamed_controls(&os.tree)
            .into_iter()
            .map(|c| describe(&os.tree, c))
            .collect();
        assert!(
            unnamed.is_empty(),
            "controls without a name:\n{}",
            unnamed.join("\n")
        );
    }

    #[test]
    fn windows_and_the_finder_come_back_as_they_were_left() {
        use ui_core::ViewState;
        use ui_core::widgets::{TableView, WindowPlacement};
        let (mut os, mut text) = setup();
        let finder = os.apps[0].0;
        let start = os.tree.window_rect(finder).unwrap();
        let grab = Point::new(start.x + 200.0, start.y + 10.0);
        let to = Point::new(grab.x + 90.0, grab.y + 40.0);
        let button = PointerButton::Primary;
        send(&mut os, &mut text, Event::PointerDown { pos: grab, button });
        send(&mut os, &mut text, Event::PointerMove { pos: to });
        send(&mut os, &mut text, Event::PointerUp { pos: to, button });
        let moved = os.tree.window_rect(finder).unwrap();
        assert_eq!((moved.x, moved.y), (start.x + 90.0, start.y + 40.0));
        let App::Finder { table, .. } = os.apps[0].1 else {
            panic!("the Finder opens first");
        };
        os.tree
            .update::<TableView, _>(table, |t, cx| t.set_column_width(cx, 0, 210.0));
        let saved = ViewState::parse(&os.tree.view_state().to_text());
        assert!(saved.get("minios.Notes").is_some());

        let (mut next, mut text) = setup();
        next.tree.restore_view_state(saved);
        next.prepare(VIEWPORT, &mut text);
        let finder = next.apps[0].0;
        let WindowPlacement::Frame(frame) =
            next.tree.get::<InnerWindow>(finder).unwrap().placement()
        else {
            panic!("a framed window");
        };
        let desk = next.tree.window_rect(next.manager.desktop()).unwrap();
        assert_eq!(
            (frame.x + desk.x, frame.y + desk.y),
            (moved.x, moved.y),
            "where it was dragged"
        );
        let App::Finder { table, .. } = next.apps[0].1 else {
            panic!("the Finder opens first");
        };
        assert_eq!(
            next.tree.get::<TableView>(table).unwrap().columns()[0].width(),
            210.0
        );
        // A second Finder window has no autosave name, so it opens where the manager
        // puts it.
        next.launch("Finder");
        let second = next.apps.last().unwrap().0;
        assert_eq!(next.tree.autosave_name(second), None);
    }

    fn send(os: &mut MiniOs, text: &mut TextStore, event: Event) {
        os.dispatch(event, text);
        os.prepare(VIEWPORT, text);
    }

    fn key(os: &mut MiniOs, text: &mut TextStore, key: Key, modifiers: Modifiers) {
        send(os, text, Event::KeyDown { key, modifiers });
    }

    fn hold(os: &mut MiniOs, text: &mut TextStore, modifiers: Modifiers) {
        send(os, text, Event::ModifiersChanged(modifiers));
    }

    fn click(os: &mut MiniOs, text: &mut TextStore, id: WidgetId) {
        let pos = center_of(&os.tree, id);
        let button = PointerButton::Primary;
        send(os, text, Event::PointerDown { pos, button });
        send(os, text, Event::PointerUp { pos, button });
    }

    fn title(os: &MiniOs, window: Option<WidgetId>) -> String {
        let window = window.expect("a window");
        os.tree
            .get::<InnerWindow>(window)
            .unwrap()
            .title()
            .to_owned()
    }

    fn chosen(os: &MiniOs) -> String {
        let view = os.switcher.view().expect("switcher open");
        let view = os.tree.get::<SwitcherView>(view).unwrap();
        view.items()[view.selected()].title.clone()
    }

    #[test]
    fn option_tab_shows_the_windows_and_letting_go_switches() {
        let (mut os, mut text) = setup();
        assert_eq!(title(&os, os.manager.active()), "Notes");
        hold(&mut os, &mut text, ALT);
        key(&mut os, &mut text, Key::Tab, ALT);
        assert!(os.switcher.is_open());
        let view = os.switcher.view().unwrap();
        let titles: Vec<String> = (os.tree.get::<SwitcherView>(view).unwrap().items())
            .iter()
            .map(|i| i.title.clone())
            .collect();
        assert_eq!(titles, ["Notes", "Finder"]);
        assert_eq!(chosen(&os), "Finder");
        key(&mut os, &mut text, Key::Tab, ALT);
        assert_eq!(chosen(&os), "Notes");
        let back = Modifiers { shift: true, ..ALT };
        key(&mut os, &mut text, Key::Tab, back);
        assert_eq!(chosen(&os), "Finder");
        assert_eq!(title(&os, os.manager.active()), "Notes");

        hold(&mut os, &mut text, Modifiers::default());
        assert!(!os.switcher.is_open());
        assert!(os.tree.overlays().all(|o| Some(o) != Some(view)));
        assert_eq!(title(&os, os.manager.active()), "Finder");
    }

    #[test]
    fn escape_closes_the_switcher_without_switching() {
        let (mut os, mut text) = setup();
        hold(&mut os, &mut text, ALT);
        key(&mut os, &mut text, Key::Tab, ALT);
        key(&mut os, &mut text, Key::Escape, ALT);
        assert!(!os.switcher.is_open());
        hold(&mut os, &mut text, Modifiers::default());
        assert_eq!(title(&os, os.manager.active()), "Notes");
    }

    #[test]
    fn a_plain_tab_still_moves_the_focus() {
        let (mut os, mut text) = setup();
        key(&mut os, &mut text, Key::Tab, Modifiers::default());
        assert!(!os.switcher.is_open());
    }

    #[test]
    fn the_switcher_brings_back_minimized_windows_and_takes_clicks() {
        let (mut os, mut text) = setup();
        let finder = os.apps[0].0;
        os.manager.minimize(&mut os.tree, finder);
        os.prepare(VIEWPORT, &mut text);
        os.menu_chosen(2, 5, &mut text);
        os.prepare(VIEWPORT, &mut text);
        assert!(os.switcher.is_open());
        assert_eq!(chosen(&os), "Finder");
        key(&mut os, &mut text, Key::Enter, Modifiers::default());
        assert!(!os.manager.is_minimized(finder));
        assert_eq!(os.manager.active(), Some(finder));

        os.launch("Calculator");
        os.menu_chosen(2, 5, &mut text);
        os.prepare(VIEWPORT, &mut text);
        let view = os.switcher.view().unwrap();
        let rect = os.tree.window_rect(view).unwrap();
        // The third tile is Notes (Calculator, Finder, Notes from the front).
        let pos = Point::new(rect.x + 14.0 + 2.0 * 110.0 + 50.0, rect.y + 60.0);
        let button = PointerButton::Primary;
        send(&mut os, &mut text, Event::PointerMove { pos });
        send(&mut os, &mut text, Event::PointerDown { pos, button });
        send(&mut os, &mut text, Event::PointerUp { pos, button });
        assert!(!os.switcher.is_open());
        assert_eq!(title(&os, os.manager.active()), "Notes");
    }

    fn unread_badge(os: &MiniOs) -> Option<String> {
        os.tree.get::<Badge>(os.badge).unwrap().label()
    }

    #[test]
    fn notifications_toast_count_as_unread_and_are_read_in_the_panel() {
        let (mut os, mut text) = setup();
        // The welcome notification.
        assert_eq!(os.notifications.unread(), 1);
        assert_eq!(unread_badge(&os).as_deref(), Some("1"));
        let toast = os.notifications.toasts().next().expect("a toast");
        assert!(os.tree.get::<ui_core::widgets::Toast>(toast).is_some());

        os.notify(&mut text, "Notes", "Saved", "Shopping list");
        os.after_input();
        assert_eq!(unread_badge(&os).as_deref(), Some("2"));

        let bell = os.bell;
        click(&mut os, &mut text, bell);
        assert!(os.notifications.is_open());
        assert_eq!(os.notifications.unread(), 0);
        assert_eq!(unread_badge(&os), None);
        assert_eq!(os.notifications.toasts().count(), 0);
        let desktop = os.desktop_rect();
        let panel = os
            .tree
            .window_rect(os.notifications.panel().unwrap())
            .unwrap();
        assert!((panel.right() - (desktop.right() - 8.0)).abs() < 0.5);
        assert!((panel.height - (desktop.height - 16.0)).abs() < 0.5);

        os.notify(&mut text, "Calculator", "Result", "42");
        os.prepare(VIEWPORT, &mut text);
        assert_eq!(os.notifications.items().len(), 3);
        assert_eq!(os.notifications.unread(), 0);
        assert_eq!(os.notifications.toasts().count(), 0);
    }

    #[test]
    fn notifications_are_removed_one_by_one_or_all_at_once() {
        let (mut os, mut text) = setup();
        os.notify(&mut text, "Notes", "Saved", "Shopping list");
        os.notify(&mut text, "Finder", "Copied", "3 files");
        let desktop = os.desktop_rect();
        os.notifications.open(&mut os.tree, &mut text, desktop);
        os.prepare(VIEWPORT, &mut text);
        assert_eq!(os.notifications.items().len(), 3);

        let remove = os.notifications.remove_button(1).unwrap();
        click(&mut os, &mut text, remove);
        let titles: Vec<&str> = (os.notifications.items().iter())
            .map(|n| n.title.as_str())
            .collect();
        assert_eq!(titles, ["Welcome", "Copied"]);

        let clear = os.notifications.clear_button().unwrap();
        click(&mut os, &mut text, clear);
        assert!(os.notifications.items().is_empty());
        assert!(os.notifications.is_open());
        assert!(!os.tree.is_enabled(clear));

        let pos = Point::new(desktop.x + 20.0, desktop.y + desktop.height - 20.0);
        let button = PointerButton::Primary;
        send(&mut os, &mut text, Event::PointerDown { pos, button });
        send(&mut os, &mut text, Event::PointerUp { pos, button });
        assert!(!os.notifications.is_open());
    }

    #[test]
    fn the_panel_follows_the_desktop_in_the_frame_that_resizes_it() {
        let (mut os, mut text) = setup();
        let bell = os.bell;
        click(&mut os, &mut text, bell);
        // One frame at the new size, as a resize of the window gives.
        os.prepare(Size::new(800.0, 500.0), &mut text);
        let desktop = os.desktop_rect();
        let panel = os
            .tree
            .window_rect(os.notifications.panel().unwrap())
            .unwrap();
        assert!((panel.right() - (desktop.right() - 8.0)).abs() < 0.5);
        assert!((panel.height - (desktop.height - 16.0)).abs() < 0.5);
    }

    #[test]
    fn the_bell_closes_the_open_panel() {
        let (mut os, mut text) = setup();
        let bell = os.bell;
        click(&mut os, &mut text, bell);
        assert!(os.notifications.is_open());
        // The press dismisses the panel as a press outside it, so the click that
        // follows must not open it again.
        click(&mut os, &mut text, bell);
        assert!(!os.notifications.is_open());
        click(&mut os, &mut text, bell);
        assert!(os.notifications.is_open());
        key(&mut os, &mut text, Key::Escape, Modifiers::default());
        assert!(!os.notifications.is_open());
        os.tree.set_focus(Some(bell));
        os.prepare(VIEWPORT, &mut text);
        key(&mut os, &mut text, Key::Space, Modifiers::default());
        assert!(os.notifications.is_open());
    }

    #[test]
    fn the_switcher_sets_the_chosen_title_in_bold() {
        let (mut os, mut text) = setup();
        hold(&mut os, &mut text, ALT);
        key(&mut os, &mut text, Key::Tab, ALT);
        let view = os.switcher.view().unwrap();
        let widths = |os: &MiniOs, text: &TextStore| -> Vec<f32> {
            let view = os.tree.get::<SwitcherView>(view).unwrap();
            (0..view.items().len())
                .map(|i| text.size(view.title_layout(i).unwrap()).width)
                .collect()
        };
        let mut width = |title: &str, weight: u16| {
            let style = ui_text::TextStyle::new(12.0).weight(weight);
            let id = text.create(title, style, None);
            let width = text.size(id).width;
            text.remove(id);
            width
        };
        let (plain, bold) = (
            [width("Notes", 400), width("Finder", 400)],
            [width("Notes", 600), width("Finder", 600)],
        );
        assert!(bold[0] > plain[0]);
        // Finder is chosen first, then Notes.
        assert_eq!(widths(&os, &text), [plain[0], bold[1]]);
        key(&mut os, &mut text, Key::Tab, ALT);
        assert_eq!(widths(&os, &text), [bold[0], plain[1]]);
    }

    fn run(keys: &[&str]) -> String {
        let mut calc = Calculator::default();
        for key in keys {
            calc.press(key);
        }
        calc.display()
    }

    #[test]
    fn calculator_and_settings_have_one_window_that_comes_forward() {
        let mut os = MiniOs::new();
        assert_eq!(os.apps.len(), 2, "Finder and Notes at start");
        os.launch("Calculator");
        os.launch("Settings");
        let calculator = os.apps[2].0;
        assert_eq!(os.manager.active(), Some(os.apps[3].0));
        os.launch("Calculator");
        assert_eq!(os.apps.len(), 4, "no second calculator");
        assert_eq!(os.manager.active(), Some(calculator), "it came forward");
        os.launch("Finder");
        assert_eq!(os.apps.len(), 5);
    }

    #[test]
    fn the_calculator_computes() {
        assert_eq!(run(&[]), "0");
        assert_eq!(run(&["1", "2", "+", "3", "="]), "15");
        assert_eq!(run(&["7", "×", "6", "−", "2", "="]), "40");
        assert_eq!(run(&["1", "÷", "4", "="]), "0.25");
        assert_eq!(run(&[".", "5", "+", ".", "2", "5", "="]), "0.75");
        assert_eq!(run(&["9", "±"]), "-9");
        assert_eq!(run(&["5", "0", "%"]), "0.5");
        assert_eq!(run(&["1", "2", "3", "⌫"]), "12");
        assert_eq!(run(&["1", "÷", "0", "="]), "Error");
        assert_eq!(run(&["1", "÷", "0", "=", "7"]), "7");
        assert_eq!(run(&["8", "+", "1", "=", "C"]), "0");
        assert_eq!(run(&["2", "+", "2", "=", "5"]), "5");
        assert_eq!(run(&["1", ".", ".", "5"]), "1.5");
    }

    /// The window, sidebar, search field and table of the Finder window open at start.
    fn finder(os: &MiniOs) -> (WidgetId, WidgetId, WidgetId, WidgetId) {
        os.apps
            .iter()
            .find_map(|(w, a)| match a {
                App::Finder {
                    sidebar,
                    search,
                    table,
                } => Some((*w, *sidebar, *search, *table)),
                _ => None,
            })
            .unwrap()
    }

    fn type_into(os: &mut MiniOs, text: &mut TextStore, field: WidgetId, typed: &str) {
        click(os, text, field);
        send(os, text, Event::Text(typed.into()));
    }

    fn notes(os: &MiniOs) -> (&[String], Option<usize>, [WidgetId; 5]) {
        os.apps
            .iter()
            .find_map(|(_, a)| match a {
                App::Notes {
                    list,
                    area,
                    search,
                    new,
                    delete,
                    texts,
                    current,
                } => Some((&texts[..], *current, [*list, *area, *search, *new, *delete])),
                _ => None,
            })
            .unwrap()
    }

    fn note_list(os: &MiniOs) -> Vec<String> {
        let [list, ..] = notes(os).2;
        let table = os.tree.get::<TableView>(list).unwrap();
        (table.displayed_rows().iter())
            .map(|&r| note_title(&notes(os).0[r]))
            .collect()
    }

    #[test]
    fn the_finder_searches_the_folder_it_shows() {
        let (mut os, mut text) = setup();
        let (window, sidebar, search, table) = finder(&os);
        os.manager.activate(&mut os.tree, window);
        os.prepare(VIEWPORT, &mut text);
        type_into(&mut os, &mut text, search, "pdf");
        let shown = |os: &MiniOs| {
            os.tree
                .get::<TableView>(table)
                .unwrap()
                .displayed_rows()
                .to_vec()
        };
        // Documents: Report.pdf and Resume.pdf.
        assert_eq!(shown(&os), [3, 4]);
        let rect = os.tree.window_rect(sidebar).unwrap();
        let desktop = Point::new(rect.x + 60.0, rect.y + 28.0 + 14.0);
        let button = PointerButton::Primary;
        send(
            &mut os,
            &mut text,
            Event::PointerDown {
                pos: desktop,
                button,
            },
        );
        send(
            &mut os,
            &mut text,
            Event::PointerUp {
                pos: desktop,
                button,
            },
        );
        assert_eq!(shown(&os), [] as [usize; 0]);
        click(&mut os, &mut text, search);
        key(&mut os, &mut text, Key::Escape, Modifiers::default());
        assert_eq!(shown(&os), [0, 1, 2]);
    }

    #[test]
    fn the_finder_shows_the_date_column_from_its_header_menu() {
        let (mut os, mut text) = setup();
        let (window, _, _, table) = finder(&os);
        os.manager.activate(&mut os.tree, window);
        os.prepare(VIEWPORT, &mut text);
        let visible = |os: &MiniOs| (os.tree.get::<TableView>(table).unwrap()).visible_columns();
        assert_eq!(visible(&os), [0, 1, 2], "Date Modified starts hidden");
        let rect = os.tree.window_rect(table).unwrap();
        let header = Point::new(rect.x + 40.0, rect.y + 10.0);
        let button = PointerButton::Secondary;
        send(
            &mut os,
            &mut text,
            Event::PointerDown {
                pos: header,
                button,
            },
        );
        send(
            &mut os,
            &mut text,
            Event::PointerUp {
                pos: header,
                button,
            },
        );
        // Kind, Size, Date Modified: the third.
        for _ in 0..3 {
            key(&mut os, &mut text, Key::ArrowDown, Modifiers::default());
        }
        key(&mut os, &mut text, Key::Enter, Modifiers::default());
        assert_eq!(visible(&os), [0, 1, 2, 3]);
        let cell = os
            .tree
            .get::<TableView>(table)
            .unwrap()
            .export_text(ui_core::widgets::TableText::Csv);
        assert!(
            cell.contains("Report.pdf,PDF document,2.4 MB,\"Nov 10, 2026\""),
            "{cell}"
        );
    }

    #[test]
    fn notes_are_made_titled_by_their_first_line_searched_and_deleted() {
        let (mut os, mut text) = setup();
        assert_eq!(note_list(&os), ["Welcome", "Groceries", "Ideas"]);
        assert_eq!(notes(&os).1, Some(0));
        let [list, area, search, new, delete] = notes(&os).2;

        click(&mut os, &mut text, new);
        assert_eq!(notes(&os).1, Some(3));
        assert_eq!(os.tree.focused(), Some(area));
        assert_eq!(
            note_list(&os),
            ["Welcome", "Groceries", "Ideas", "New Note"]
        );
        send(
            &mut os,
            &mut text,
            Event::Text("Trip\nPack the tent".into()),
        );
        assert_eq!(note_list(&os)[3], "Trip");
        assert_eq!(notes(&os).0[3], "Trip\nPack the tent");
        let selected = |os: &MiniOs| os.tree.get::<TableView>(list).unwrap().selected_rows();
        assert_eq!(
            selected(&os),
            [3],
            "the selection stays on the retitled note"
        );

        type_into(&mut os, &mut text, search, "coffee");
        assert_eq!(note_list(&os), ["Groceries"]);
        assert_eq!(notes(&os).1, Some(1));
        let shown = os.tree.get::<TextArea>(area).unwrap().text().to_owned();
        assert!(shown.contains("Coffee beans"), "{shown}");

        click(&mut os, &mut text, search);
        key(&mut os, &mut text, Key::Escape, Modifiers::default());
        assert_eq!(note_list(&os).len(), 4);
        click(&mut os, &mut text, delete);
        assert_eq!(note_list(&os), ["Welcome", "Ideas", "Trip"]);
        assert_eq!(notes(&os).1, Some(1));
        assert_eq!(note_title(&notes(&os).0[1]), "Ideas");
        for _ in 0..3 {
            click(&mut os, &mut text, delete);
        }
        assert!(notes(&os).0.is_empty());
        assert_eq!(notes(&os).1, None);
        click(&mut os, &mut text, delete);
        assert_eq!(os.tree.get::<TextArea>(area).unwrap().text(), "");
    }
}

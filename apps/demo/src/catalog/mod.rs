mod controls;
mod data;
mod foundation;
mod icons;
mod inputs;
mod kit;
mod layout;
mod menus;
mod navigation;
#[cfg(feature = "native")]
mod note_window;
mod notices;
mod presentation;
mod status;
mod text;
mod values;

use noa_gpui_core::style::{
    AlignItems, Dimension, FlexDirection, JustifyContent, Rect as LayoutRect, Size as LayoutSize,
    auto, length, percent,
};
use noa_gpui_core::widgets::{
    AnchorNav, AnchorSelected, COMMAND_PALETTE_SHORTCUTS, CommandChosen, CommandPalette, Container,
    Label, MenuBar, MenuBarChosen, MenuBarMenu, MenuItem, PaletteCommand, RecentCommands,
    ScrollView, menu_commands, menu_item,
};
use noa_gpui_core::{
    Appearance, Event, Message, Role, Shortcut, ShortcutPressed, Style, UiTree, WidgetId,
};
use noa_gpui_scene::{Color, Point};
use noa_gpui_text::TextStore;

use crate::assets::Assets;
use crate::bench::{Meter, center_of};
use kit::{DIVIDER, Kit, gap, zero};
#[cfg(feature = "native")]
use noa_gpui_platform::{
    AlertRequest, AlertResult, FileDialogRequest, FileDialogResult, WindowRequest,
};
use noa_gpui_platform::{FrameReport, Screen};

const HEADER_BG: Color = Color::hex(0x1e2230);
const SECTION_GAP: f32 = 40.0;
const CONTENT_PAD: f32 = 32.0;

pub struct SectionSpec {
    pub title: &'static str,
    pub description: &'static str,
    pub build: fn(&mut Kit) -> Box<dyn Demo>,
}

pub trait Demo {
    /// Returns whether the message was this section's.
    fn handle(&mut self, _cx: &mut DemoCx, _message: &Message) -> bool {
        false
    }

    /// `path` is "Menu › Item".
    fn app_menu_chosen(&mut self, _cx: &mut DemoCx, _path: &str) {}

    #[cfg(all(
        feature = "native",
        any(target_os = "macos", target_os = "windows", target_os = "linux")
    ))]
    /// Called after the catalog has acted on the item.
    fn status_menu_chosen(&mut self, _cx: &mut DemoCx, _label: &str) {}

    /// The event loop was woken (see [`Kit::waker`]): take what arrived in the
    /// background.
    fn woken(&mut self, _cx: &mut DemoCx) {}

    /// Called after every input event, to follow state that changes without a message,
    /// such as a scroll position.
    fn after_input(&mut self, _tree: &mut UiTree) {}

    /// Takes content coming back from a secondary window if it is this section's.
    #[cfg(feature = "native")]
    fn adopt(&mut self, _cx: &mut DemoCx, _detached: &mut Option<noa_gpui_platform::Detached>) {}

    #[cfg(feature = "native")]
    fn file_dialog_finished(&mut self, _cx: &mut DemoCx, _result: &FileDialogResult) {}

    #[cfg(feature = "native")]
    fn alert_finished(&mut self, _cx: &mut DemoCx, _result: AlertResult) {}
}

#[derive(Debug, Clone)]
pub struct StatusItem {
    pub visible: bool,
    pub count: u32,
    /// Set when the menu must be rebuilt.
    pub changed: bool,
}

impl Demo for () {}

pub struct DemoCx<'a> {
    pub tree: &'a mut UiTree,
    pub text: &'a mut TextStore,
    /// The OS appearance, for the theme section's "System" choice.
    pub system_appearance: Appearance,
    pub follow_system: &'a mut bool,
    #[cfg(feature = "native")]
    pub windows: &'a mut Vec<WindowRequest>,
    #[cfg(feature = "native")]
    pub file_dialogs: &'a mut Vec<FileDialogRequest>,
    #[cfg(feature = "native")]
    pub alerts: &'a mut Vec<AlertRequest>,
    pub status_item: &'a mut StatusItem,
    /// The top of the content area (below the menu bar and header), where sheets hang.
    pub content_top: f32,
}

const GROUPS: [(&str, &[SectionSpec]); 12] = [
    ("LAYOUT", &layout::SECTIONS),
    ("TEXT", &text::SECTIONS),
    ("CONTROLS", &controls::SECTIONS),
    ("VALUES", &values::SECTIONS),
    ("INPUTS", &inputs::SECTIONS),
    ("STATUS", &status::SECTIONS),
    ("NOTICES", &notices::SECTIONS),
    ("DATA", &data::SECTIONS),
    ("MENUS", &menus::SECTIONS),
    ("NAVIGATION", &navigation::SECTIONS),
    ("PRESENTATION", &presentation::SECTIONS),
    ("FOUNDATION", &foundation::SECTIONS),
];

pub struct Catalog {
    tree: UiTree,
    /// The app menus drawn in the window (hidden when the system menu bar shows them).
    menu_bar: WidgetId,
    menus: Vec<MenuBarMenu>,
    /// The state `menus` was built from, so rebuilding is skipped when it is unchanged.
    menu_key: Option<(bool, bool, Appearance)>,
    sidebar: WidgetId,
    sidebar_visible: bool,
    system_menus_changed: bool,
    status_item: StatusItem,
    #[cfg(feature = "native")]
    notes_opened: u32,
    status: WidgetId,
    scroll: WidgetId,
    nav: WidgetId,
    demos: Vec<Box<dyn Demo>>,
    palette: Option<(WidgetId, Vec<PaletteAction>)>,
    palette_commands: Vec<PaletteCommand>,
    /// Recently chosen palette commands, listed first the next time it opens.
    recent: RecentCommands,
    system_appearance: Appearance,
    follow_system: bool,
    #[cfg(feature = "native")]
    window_requests: Vec<WindowRequest>,
    #[cfg(feature = "native")]
    file_dialogs: Vec<FileDialogRequest>,
    #[cfg(feature = "native")]
    alerts: Vec<AlertRequest>,
    meter: Meter,
}

impl Catalog {
    /// Without a waker, as the tests draw it.
    #[cfg(test)]
    pub fn new(assets: &Assets) -> Self {
        Self::with_waker(assets, None)
    }

    /// `waker` wakes the event loop for sections that load in the background; without
    /// one (drawing without a window) they show what they have.
    pub fn with_waker(assets: &Assets, waker: Option<noa_gpui_platform::Waker>) -> Self {
        let mut tree = UiTree::new();
        // Menus write shortcuts as ⌘S where the command key is Cmd (also on the Web).
        tree.set_shortcut_glyphs(noa_gpui_platform::command_is_meta());
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

        // The app menus, unless the system menu bar shows them (macOS desktop).
        let menu_bar = tree.insert(Some(root), MenuBar::new(Vec::new()));
        tree.set_accessible_label(menu_bar, "App menus");
        let header = tree.insert(
            Some(root),
            Container::new(Style {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::CENTER,
                justify_content: JustifyContent::SPACE_BETWEEN,
                padding: LayoutRect {
                    left: length(16.0),
                    right: length(16.0),
                    top: zero(),
                    bottom: zero(),
                },
                size: LayoutSize {
                    width: percent(1.0),
                    height: length(44.0),
                },
                flex_shrink: 0.0,
                ..Default::default()
            })
            .background(HEADER_BG),
        );
        tree.insert(
            Some(header),
            Label::new("noa-gpui · UI catalog")
                .font_size(17.0)
                .weight(700)
                .color(Color::WHITE),
        );
        let status = tree.insert(
            Some(header),
            Label::new("").font_size(12.0).color(Color::hex(0xb8c0d8)),
        );

        let body = tree.insert(
            Some(root),
            Container::new(Style {
                flex_direction: FlexDirection::Row,
                flex_grow: 1.0,
                flex_basis: length(0.0),
                min_size: LayoutSize {
                    width: auto(),
                    height: length(0.0),
                },
                ..Default::default()
            }),
        );

        let sidebar = tree.insert(
            Some(body),
            Container::new(Style {
                flex_direction: FlexDirection::Column,
                size: LayoutSize {
                    width: length(200.0),
                    height: Dimension::auto(),
                },
                flex_shrink: 0.0,
                ..Default::default()
            })
            .background(Role::Surface)
            .border(1.0, DIVIDER),
        );
        let nav_list = tree.insert(
            Some(sidebar),
            ScrollView::new(Style {
                flex_direction: FlexDirection::Column,
                gap: gap(0.0, 2.0),
                padding: LayoutRect::length(12.0),
                flex_grow: 1.0,
                flex_basis: length(0.0),
                min_size: LayoutSize {
                    width: auto(),
                    height: length(0.0),
                },
                ..Default::default()
            }),
        );

        let scroll = tree.insert(
            Some(body),
            ScrollView::new(Style {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                flex_basis: length(0.0),
                min_size: LayoutSize {
                    width: length(0.0),
                    height: auto(),
                },
                gap: gap(0.0, SECTION_GAP),
                padding: LayoutRect::length(CONTENT_PAD),
                ..Default::default()
            })
            // Wide specimens (tables, the desktop, the terminal) stay reachable in a
            // narrow window.
            .horizontal(true),
        );

        // Opens again at the section that was being read (with --view-state).
        tree.set_autosave_name(scroll, "catalog.content");
        let mut nav = AnchorNav::new(scroll).scroll_margin(CONTENT_PAD);
        let mut demos = Vec::new();
        for (group, specs) in GROUPS {
            nav = nav.heading(group);
            for spec in specs {
                let (section, grid) = kit::section(&mut tree, scroll, spec.title, spec.description);
                nav = nav.anchor(spec.title, section);
                demos.push((spec.build)(&mut Kit {
                    tree: &mut tree,
                    assets,
                    grid,
                    waker: waker.clone(),
                }));
            }
        }
        let nav = tree.insert(Some(nav_list), nav);
        tree.set_accessible_label(nav, "Sections");
        // Opening the inner windows specimen focused one of its windows, far down the
        // page and out of sight. Start with nothing focused: Tab begins at the top.
        tree.set_focus(None);

        // A spacer so the last section can scroll to the top of the viewport.
        tree.insert(
            Some(scroll),
            Container::new(Style {
                size: LayoutSize {
                    width: auto(),
                    height: length(400.0),
                },
                flex_shrink: 0.0,
                ..Default::default()
            }),
        );

        let mut catalog = Self {
            meter: Meter::default(),
            tree,
            menu_bar,
            menus: Vec::new(),
            menu_key: None,
            sidebar,
            sidebar_visible: true,
            system_menus_changed: false,
            status_item: StatusItem {
                visible: true,
                count: 0,
                changed: false,
            },
            #[cfg(feature = "native")]
            notes_opened: 0,
            status,
            scroll,
            nav,
            demos,
            palette: None,
            palette_commands: Vec::new(),
            recent: RecentCommands::default(),
            system_appearance: Appearance::Light,
            follow_system: true,
            #[cfg(feature = "native")]
            window_requests: Vec::new(),
            #[cfg(feature = "native")]
            file_dialogs: Vec::new(),
            #[cfg(feature = "native")]
            alerts: Vec::new(),
        };
        // ⌘K is the View menu's; ⇧⌘P is only a shortcut.
        catalog
            .tree
            .add_shortcut(COMMAND_PALETTE_SHORTCUTS[1], catalog.scroll);
        catalog.refresh_menus();
        catalog
    }

    fn build_menus(&self) -> Vec<MenuBarMenu> {
        let appearance = if self.follow_system {
            0
        } else if self.tree.theme().appearance == Appearance::Light {
            1
        } else {
            2
        };
        let file = vec![
            MenuItem::new("New Note Window")
                .key(Shortcut::command('n'))
                .enabled(cfg!(feature = "native")),
            MenuItem::separator(),
            MenuItem::new("Go to Top").key(Shortcut::command('t')),
        ];
        let view = vec![
            MenuItem::new("Command Palette…").key(COMMAND_PALETTE_SHORTCUTS[0]),
            MenuItem::separator(),
            MenuItem::new("Show Sidebar")
                .checked(self.sidebar_visible)
                .key(Shortcut::command('s').with_alt()),
            MenuItem::separator(),
            MenuItem::new("Appearance").submenu(
                ["System", "Light", "Dark"]
                    .iter()
                    .enumerate()
                    .map(|(i, name)| MenuItem::new(*name).checked(i == appearance))
                    .collect(),
            ),
        ];
        let go = GROUPS
            .iter()
            .map(|(group, specs)| {
                MenuItem::new(*group)
                    .submenu(specs.iter().map(|s| MenuItem::new(s.title)).collect())
            })
            .collect();
        let help = vec![
            MenuItem::new("Keyboard Shortcuts"),
            MenuItem::new("About This Catalog"),
        ];
        vec![
            MenuBarMenu::new("File", file),
            MenuBarMenu::new("View", view),
            MenuBarMenu::new("Go", go),
            MenuBarMenu::new("Help", help),
        ]
    }

    fn refresh_menus(&mut self) {
        // Everything `build_menus` reads that can change.
        let key = Some((
            self.sidebar_visible,
            self.follow_system,
            self.tree.theme().appearance,
        ));
        if key == self.menu_key {
            return;
        }
        self.menu_key = key;
        let menus = self.build_menus();
        if menus != self.menus {
            self.menus = menus.clone();
            self.tree
                .update::<MenuBar, _>(self.menu_bar, |b, cx| b.set_menus(cx, menus));
            self.system_menus_changed = true;
        }
    }

    fn section_index(&self, title: &str) -> Option<usize> {
        GROUPS
            .iter()
            .flat_map(|(_, specs)| specs.iter())
            .position(|s| s.title == title)
    }

    fn set_appearance(&mut self, choice: &str) {
        let appearance = match choice {
            "Light" => Appearance::Light,
            "Dark" => Appearance::Dark,
            _ => self.system_appearance,
        };
        self.follow_system = choice == "System";
        let theme = self.tree.theme().clone().with_appearance(appearance);
        self.tree.set_theme(theme);
    }

    /// `item` counts the items of menu `menu` depth first.
    fn perform_app_menu(&mut self, menu: usize, item: usize, text: &mut TextStore) {
        let Some(m) = self.menus.get(menu) else {
            return;
        };
        let title = m.title.clone();
        let Some(label) = menu_item(&m.items, item).map(|i| i.label().to_owned()) else {
            return;
        };
        self.perform(&title, &label, text);
    }

    fn perform(&mut self, title: &str, label: &str, text: &mut TextStore) {
        match (title, label) {
            #[cfg(feature = "native")]
            ("File", "New Note Window") => {
                self.notes_opened += 1;
                self.window_requests.push(WindowRequest {
                    title: format!("Note {}", self.notes_opened),
                    size: (420.0, 360.0),
                    screen: Box::new(note_window::NoteWindow::new(
                        self.notes_opened,
                        self.tree.theme().clone(),
                    )),
                    floating: false,
                    titlebar: Default::default(),
                    min_size: None,
                });
            }
            ("File", "Go to Top") | ("Help", "About This Catalog") => self.scroll_to(0),
            ("View", "Command Palette…") => self.open_palette(),
            ("Help", "Keyboard Shortcuts") => {
                if let Some(i) = self.section_index("Keyboard") {
                    self.scroll_to(i);
                }
            }
            ("View", "Show Sidebar") => {
                self.sidebar_visible = !self.sidebar_visible;
                self.tree.set_hidden(self.sidebar, !self.sidebar_visible);
            }
            ("View", choice @ ("System" | "Light" | "Dark")) => self.set_appearance(choice),
            ("Go", section) => {
                if let Some(i) = self.section_index(section) {
                    self.scroll_to(i);
                }
            }
            _ => {}
        }
        let path = format!("{title} › {label}");
        self.with_demos(text, |demo, cx| demo.app_menu_chosen(cx, &path));
        self.refresh_menus();
    }

    fn open_palette(&mut self) {
        if self
            .palette
            .as_ref()
            .is_some_and(|(id, _)| self.tree.contains(*id))
        {
            return;
        }
        let mut commands = Vec::new();
        let mut actions = Vec::new();
        for (command, chosen) in menu_commands(&self.menus) {
            let menu = &self.menus[chosen.menu];
            // The sections are listed below; the palette does not open itself.
            if menu.title == "Go" || command.title() == "Command Palette…" {
                continue;
            }
            let Some(item) = menu_item(&menu.items, chosen.item) else {
                continue;
            };
            actions.push(PaletteAction::Menu(
                menu.title.clone(),
                item.label().to_owned(),
            ));
            commands.push(command);
        }
        let sections = GROUPS
            .iter()
            .flat_map(|(group, specs)| specs.iter().map(move |s| (*group, s.title)));
        for (index, (group, title)) in sections.enumerate() {
            commands.push(
                PaletteCommand::new(format!("Go to {title}"))
                    .detail(group.to_lowercase())
                    .keywords(group),
            );
            actions.push(PaletteAction::Section(index));
        }
        let top = self.content_top() + 12.0;
        self.palette_commands = commands.clone();
        let id = CommandPalette::new(commands)
            .recent(&self.recent)
            .placeholder("Go to a section or run a command…")
            .top(top)
            .open(&mut self.tree);
        self.palette = Some((id, actions));
    }

    fn with_demos(&mut self, text: &mut TextStore, mut f: impl FnMut(&mut dyn Demo, &mut DemoCx)) {
        let content_top = self.content_top();
        let mut cx = DemoCx {
            tree: &mut self.tree,
            text,
            system_appearance: self.system_appearance,
            follow_system: &mut self.follow_system,
            #[cfg(feature = "native")]
            windows: &mut self.window_requests,
            #[cfg(feature = "native")]
            file_dialogs: &mut self.file_dialogs,
            #[cfg(feature = "native")]
            alerts: &mut self.alerts,
            status_item: &mut self.status_item,
            content_top,
        };
        for demo in &mut self.demos {
            f(demo.as_mut(), &mut cx);
        }
    }

    #[cfg(all(
        feature = "native",
        any(target_os = "macos", target_os = "windows", target_os = "linux")
    ))]
    fn status_items(&self) -> Vec<MenuItem> {
        vec![
            MenuItem::new(format!("Count: {}", self.status_item.count)).enabled(false),
            MenuItem::separator(),
            MenuItem::new("Count Up"),
            MenuItem::new("Reset").enabled(self.status_item.count > 0),
            MenuItem::separator(),
            MenuItem::new("Show Catalog Sidebar").checked(self.sidebar_visible),
            MenuItem::new("Hide Menu Bar Extra"),
        ]
    }

    fn scroll_to(&mut self, index: usize) {
        AnchorNav::scroll_to(&mut self.tree, self.nav, index);
    }

    /// The top of the scrolling content in window coordinates.
    fn content_top(&self) -> f32 {
        self.tree.window_rect(self.scroll).map_or(0.0, |r| r.y)
    }
}

#[derive(Debug, Clone)]
enum PaletteAction {
    /// An app menu item: menu title and item label.
    Menu(String, String),
    Section(usize),
}

impl Catalog {
    pub fn metered(mut self, meter: Meter) -> Self {
        self.meter = meter;
        self
    }
    fn bench_step(&mut self, text: &mut TextStore) {
        let pos = center_of(&self.tree, self.scroll);
        self.dispatch(
            Event::Wheel {
                pos,
                delta: Point::new(0.0, -7.0),
            },
            text,
        );
    }
}

impl Screen for Catalog {
    fn tree(&self) -> &UiTree {
        &self.tree
    }

    fn tree_mut(&mut self) -> &mut UiTree {
        &mut self.tree
    }

    fn before_frame(&mut self, text: &mut TextStore) {
        if self.meter.bench() {
            self.bench_step(text);
        }
        let status = self.meter.status();
        self.tree
            .update::<Label, _>(self.status, |l, cx| l.set_text(cx, status));
    }

    fn woken(&mut self, text: &mut TextStore) {
        self.with_demos(text, |demo, cx| demo.woken(cx));
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

    fn set_system_appearance(&mut self, appearance: Appearance) {
        self.system_appearance = appearance;
        if self.follow_system {
            let theme = self.tree.theme().clone().with_appearance(appearance);
            self.tree.set_theme(theme);
        }
    }

    fn handle(&mut self, message: Message, text: &mut TextStore) {
        if message.source == self.nav
            && let Some(&AnchorSelected(index)) = message.downcast_ref()
        {
            self.scroll_to(index);
            return;
        }
        if message.source == self.scroll && message.downcast_ref::<ShortcutPressed>().is_some() {
            self.open_palette();
            return;
        }
        if let Some((id, actions)) = &self.palette
            && message.source == *id
            && let Some(&CommandChosen(i)) = message.downcast_ref()
        {
            if let Some(command) = self.palette_commands.get(i) {
                self.recent.note_command(command);
            }
            match actions[i].clone() {
                PaletteAction::Menu(title, label) => self.perform(&title, &label, text),
                PaletteAction::Section(index) => self.scroll_to(index),
            }
            return;
        }
        if message.source == self.menu_bar
            && let Some(&MenuBarChosen { menu, item }) = message.downcast_ref()
        {
            self.perform_app_menu(menu, item, text);
            return;
        }
        let content_top = self.content_top();
        let mut cx = DemoCx {
            tree: &mut self.tree,
            text,
            system_appearance: self.system_appearance,
            follow_system: &mut self.follow_system,
            #[cfg(feature = "native")]
            windows: &mut self.window_requests,
            #[cfg(feature = "native")]
            file_dialogs: &mut self.file_dialogs,
            #[cfg(feature = "native")]
            alerts: &mut self.alerts,
            status_item: &mut self.status_item,
            content_top,
        };
        for demo in &mut self.demos {
            if demo.handle(&mut cx, &message) {
                break;
            }
        }
        if std::mem::take(&mut self.status_item.changed) {
            self.system_menus_changed = true;
        }
    }

    #[cfg(all(feature = "native", target_os = "macos"))]
    fn app_menus(&self) -> Vec<MenuBarMenu> {
        self.menus.clone()
    }

    #[cfg(all(feature = "native", target_os = "macos"))]
    fn app_menu_chosen(&mut self, menu: usize, item: usize, text: &mut TextStore) {
        self.perform_app_menu(menu, item, text);
    }

    #[cfg(all(feature = "native", target_os = "macos"))]
    fn use_system_menu_bar(&mut self) {
        self.tree.set_hidden(self.menu_bar, true);
    }

    #[cfg(all(
        feature = "native",
        any(target_os = "macos", target_os = "windows", target_os = "linux")
    ))]
    fn take_system_menus_changed(&mut self) -> bool {
        std::mem::take(&mut self.system_menus_changed)
    }

    #[cfg(all(
        feature = "native",
        any(target_os = "macos", target_os = "windows", target_os = "linux")
    ))]
    fn status_menu(&self) -> Option<Vec<MenuItem>> {
        self.status_item.visible.then(|| self.status_items())
    }

    #[cfg(all(
        feature = "native",
        any(target_os = "macos", target_os = "windows", target_os = "linux")
    ))]
    fn status_menu_chosen(&mut self, item: usize, text: &mut TextStore) {
        let items = self.status_items();
        let Some(label) = menu_item(&items, item).map(|i| i.label().to_owned()) else {
            return;
        };
        match label.as_str() {
            "Count Up" => self.status_item.count += 1,
            "Reset" => self.status_item.count = 0,
            "Show Catalog Sidebar" => {
                self.sidebar_visible = !self.sidebar_visible;
                self.tree.set_hidden(self.sidebar, !self.sidebar_visible);
            }
            "Hide Menu Bar Extra" => self.status_item.visible = false,
            _ => return,
        }
        self.system_menus_changed = true;
        self.with_demos(text, |demo, cx| demo.status_menu_chosen(cx, &label));
        self.refresh_menus();
    }

    #[cfg(feature = "native")]
    fn take_window_requests(&mut self) -> Vec<WindowRequest> {
        std::mem::take(&mut self.window_requests)
    }

    #[cfg(feature = "native")]
    fn take_file_dialogs(&mut self) -> Vec<FileDialogRequest> {
        std::mem::take(&mut self.file_dialogs)
    }

    #[cfg(feature = "native")]
    fn adopt(&mut self, detached: noa_gpui_platform::Detached, text: &mut TextStore) {
        let mut detached = Some(detached);
        self.with_demos(text, |demo, cx| demo.adopt(cx, &mut detached));
    }

    #[cfg(feature = "native")]
    fn file_dialog_finished(&mut self, result: FileDialogResult, text: &mut TextStore) {
        self.with_demos(text, |demo, cx| demo.file_dialog_finished(cx, &result));
    }

    #[cfg(feature = "native")]
    fn take_alerts(&mut self) -> Vec<AlertRequest> {
        std::mem::take(&mut self.alerts)
    }

    #[cfg(feature = "native")]
    fn alert_finished(&mut self, result: AlertResult, text: &mut TextStore) {
        self.with_demos(text, |demo, cx| demo.alert_finished(cx, result));
    }

    fn after_input(&mut self) {
        // The theme section's buttons change what the View menu checks.
        self.refresh_menus();
        AnchorNav::sync(&mut self.tree, self.nav);
        for demo in &mut self.demos {
            demo.after_input(&mut self.tree);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::a11y_audit::{describe, unnamed_controls};
    use noa_gpui_scene::{ImageId, Size};
    use noa_gpui_text::FontOptions;

    #[test]
    fn every_control_has_an_accessible_name() {
        let mut next = 0;
        let assets = Assets::generate(|_, _, _| {
            next += 1;
            ImageId(next)
        });
        let mut catalog = Catalog::new(&assets);
        let mut text = TextStore::new(FontOptions {
            locale: "en-US".into(),
            load_system_fonts: false,
        });
        catalog.tree.prepare(Size::new(1280.0, 800.0), &mut text);
        let unnamed: Vec<String> = unnamed_controls(&catalog.tree)
            .into_iter()
            .map(|c| describe(&catalog.tree, c))
            .collect();
        eprintln!("AUDIT catalog unnamed {}", unnamed.len());
        assert!(
            unnamed.is_empty(),
            "controls without a name:\n{}",
            unnamed.join("\n")
        );
    }

    #[test]
    fn the_palette_runs_the_app_menus_items_including_those_in_submenus() {
        let mut next = 0;
        let assets = Assets::generate(|_, _, _| {
            next += 1;
            ImageId(next)
        });
        let mut catalog = Catalog::new(&assets);
        let mut text = TextStore::new(FontOptions {
            locale: "en-US".into(),
            load_system_fonts: false,
        });
        catalog.tree.prepare(Size::new(1280.0, 800.0), &mut text);
        catalog.open_palette();
        let (id, _) = catalog.palette.clone().unwrap();
        let titles: Vec<String> = catalog
            .tree
            .get::<CommandPalette>(id)
            .unwrap()
            .commands()
            .iter()
            .map(|c| c.title().to_owned())
            .collect();
        assert!(titles.iter().any(|t| t == "Go to Top"));
        assert!(titles.iter().any(|t| t == "Appearance: Dark"));
        assert!(!titles.iter().any(|t| t == "Command Palette…"));
        catalog.tree.prepare(Size::new(1280.0, 800.0), &mut text);
        assert_ne!(catalog.tree.theme().appearance, Appearance::Dark);
        catalog.dispatch(Event::Text("Appearance: Dark".into()), &mut text);
        let modifiers = noa_gpui_core::Modifiers::default();
        catalog.dispatch(
            Event::KeyDown {
                key: noa_gpui_core::Key::Enter,
                modifiers,
            },
            &mut text,
        );
        assert_eq!(catalog.tree.theme().appearance, Appearance::Dark);

        catalog.tree.prepare(Size::new(1280.0, 800.0), &mut text);
        catalog.open_palette();
        let (id, _) = catalog.palette.clone().unwrap();
        let palette = catalog.tree.get::<CommandPalette>(id).unwrap();
        let first = palette.highlighted().unwrap();
        assert_eq!(palette.commands()[first].title(), "Appearance: Dark");
    }

    #[test]
    fn the_catalog_reopens_at_the_section_and_sort_it_was_left_with() {
        use noa_gpui_core::ViewState;
        use noa_gpui_core::widgets::{ScrollView, TableView};
        let build = || {
            let mut next = 0;
            let assets = Assets::generate(|_, _, _| {
                next += 1;
                ImageId(next)
            });
            Catalog::new(&assets)
        };
        let mut text = TextStore::new(FontOptions {
            locale: "en-US".into(),
            load_system_fonts: false,
        });
        let viewport = Size::new(1280.0, 800.0);
        let mut catalog = build();
        catalog.tree.prepare(viewport, &mut text);
        catalog.scroll_to(5);
        catalog.tree.prepare(viewport, &mut text);
        let offset = catalog
            .tree
            .get::<ScrollView>(catalog.scroll)
            .unwrap()
            .offset();
        assert!(offset > 0.0);
        let mut state = ViewState::parse(&catalog.tree.view_state().to_text());
        state.set("catalog.files", "columns:4;sort:1,desc");

        let mut next = build();
        next.tree.restore_view_state(state);
        next.tree.prepare(viewport, &mut text);
        let restored = next.tree.get::<ScrollView>(next.scroll).unwrap().offset();
        assert!((restored - offset).abs() < 0.01, "{restored} vs {offset}");
        let mut stack = vec![next.tree.root().unwrap()];
        let mut files = None;
        while let Some(id) = stack.pop() {
            if next.tree.autosave_name(id) == Some("catalog.files") {
                files = Some(id);
            }
            stack.extend_from_slice(next.tree.children(id));
        }
        let files = files.expect("the files table is named");
        assert_eq!(
            next.tree.get::<TableView>(files).unwrap().sort_column(),
            Some((1, false))
        );
    }

    #[test]
    fn the_project_outline_adds_names_and_deletes_folders() {
        use noa_gpui_core::widgets::{Button, OutlineView};
        let mut next = 0;
        let assets = Assets::generate(|_, _, _| {
            next += 1;
            ImageId(next)
        });
        let mut catalog = Catalog::new(&assets);
        let mut text = TextStore::new(FontOptions {
            locale: "en-US".into(),
            load_system_fonts: false,
        });
        let viewport = Size::new(1280.0, 800.0);
        catalog.tree.prepare(viewport, &mut text);
        let index = catalog.section_index("Outline view").unwrap();
        catalog.scroll_to(index);
        catalog.tree.prepare(viewport, &mut text);
        let mut ids = vec![catalog.tree.root().unwrap()];
        let (mut outline, mut buttons) = (None, Vec::new());
        while let Some(id) = ids.pop() {
            if catalog.tree.autosave_name(id) == Some("catalog.project-files") {
                outline = Some(id);
            }
            if let Some(b) = catalog.tree.get::<Button>(id)
                && matches!(b.label(), "New Folder" | "Delete")
            {
                buttons.push((b.label().to_owned(), id));
            }
            ids.extend_from_slice(catalog.tree.children(id));
        }
        let outline = outline.expect("the project outline is named");
        let button = |name: &str| buttons.iter().find(|(n, _)| n == name).unwrap().1;
        let click = |catalog: &mut Catalog, text: &mut TextStore, id: WidgetId| {
            let r = catalog.tree.window_rect(id).unwrap();
            let pos = noa_gpui_scene::Point::new(r.x + r.width / 2.0, r.y + r.height / 2.0);
            let button = noa_gpui_core::PointerButton::Primary;
            catalog.dispatch(Event::PointerDown { pos, button }, text);
            catalog.dispatch(Event::PointerUp { pos, button }, text);
            catalog.tree.prepare(viewport, text);
        };
        let count = |c: &Catalog| c.tree.get::<OutlineView>(outline).unwrap().item_count();
        let before = count(&catalog);

        catalog
            .tree
            .update::<OutlineView, _>(outline, |v, cx| v.set_filter_text(cx, "lib"));
        click(&mut catalog, &mut text, button("New Folder"));
        assert!(
            !catalog
                .tree
                .get::<OutlineView>(outline)
                .unwrap()
                .is_filtered()
        );
        assert_eq!(count(&catalog), before + 1);
        let view = catalog.tree.get::<OutlineView>(outline).unwrap();
        let new = view.renaming().expect("the new folder is being named");
        assert_eq!(view.parent(new), None);
        assert_eq!(view.selected_items(), [new]);
        catalog.dispatch(Event::Text("assets".into()), &mut text);
        let modifiers = noa_gpui_core::Modifiers::default();
        let key = noa_gpui_core::Key::Enter;
        catalog.dispatch(Event::KeyDown { key, modifiers }, &mut text);
        let view = catalog.tree.get::<OutlineView>(outline).unwrap();
        assert_eq!(view.title(new), "assets");
        assert!(view.renaming().is_none());

        click(&mut catalog, &mut text, button("Delete"));
        assert_eq!(count(&catalog), before);
        let view = catalog.tree.get::<OutlineView>(outline).unwrap();
        assert!(!view.items().iter().any(|i| i.title() == "assets"));
    }

    #[test]
    fn document_tabs_open_and_ask_before_closing_a_changed_one() {
        use noa_gpui_core::widgets::{Alert, TabView, TextArea};
        let mut next = 0;
        let assets = Assets::generate(|_, _, _| {
            next += 1;
            ImageId(next)
        });
        let mut catalog = Catalog::new(&assets);
        let mut text = TextStore::new(FontOptions {
            locale: "en-US".into(),
            load_system_fonts: false,
        });
        let viewport = Size::new(1280.0, 800.0);
        let index = catalog.section_index("Tab view").unwrap();
        catalog.scroll_to(index);
        catalog.tree.prepare(viewport, &mut text);
        let mut ids = vec![catalog.tree.root().unwrap()];
        let mut tabs = None;
        while let Some(id) = ids.pop() {
            if catalog.tree.get::<TabView>(id).is_some_and(|t| {
                t.titles()
                    .first()
                    .is_some_and(|t| t.starts_with("Untitled"))
            }) {
                tabs = Some(id);
            }
            ids.extend_from_slice(catalog.tree.children(id));
        }
        let tabs = tabs.expect("the document tabs");
        let act = |catalog: &mut Catalog, text: &mut TextStore, item: usize| {
            use noa_gpui_core::accesskit;
            let request = accesskit::ActionRequest {
                action: accesskit::Action::Click,
                target_tree: accesskit::TreeId::ROOT,
                target_node: noa_gpui_core::access_item_id(tabs, item),
                data: None,
            };
            catalog.tree.accessibility_action(&request, text);
            for message in catalog.tree.take_messages() {
                catalog.handle(message, text);
            }
            catalog.tree.prepare(viewport, text);
        };
        let key = |catalog: &mut Catalog, text: &mut TextStore, key: noa_gpui_core::Key| {
            let modifiers = noa_gpui_core::Modifiers::default();
            catalog.dispatch(Event::KeyDown { key, modifiers }, text);
            catalog.tree.prepare(viewport, text);
        };
        let titles = |c: &Catalog| c.tree.get::<TabView>(tabs).unwrap().titles().to_vec();
        assert_eq!(titles(&catalog), ["Untitled 1", "Untitled 2"]);
        // "+" (the item after the two tabs and their close buttons) opens a third.
        act(&mut catalog, &mut text, 4);
        assert_eq!(titles(&catalog), ["Untitled 1", "Untitled 2", "Untitled 3"]);
        let page = TabView::page(&catalog.tree, tabs, 2).unwrap();
        assert!(catalog.tree.get::<TextArea>(page).is_some());
        catalog.tree.set_focus(Some(page));
        catalog.dispatch(Event::Text("draft".into()), &mut text);
        catalog.tree.prepare(viewport, &mut text);
        assert!(catalog.tree.get::<TabView>(tabs).unwrap().is_modified(2));
        let draft = catalog
            .tree
            .get::<TextArea>(page)
            .unwrap()
            .text()
            .to_owned();
        let alerts = |c: &Catalog| {
            c.tree
                .overlays()
                .filter(|&o| c.tree.get::<Alert>(o).is_some())
                .count()
        };
        catalog.tree.set_focus(Some(tabs));
        key(&mut catalog, &mut text, noa_gpui_core::Key::Delete);
        assert_eq!(alerts(&catalog), 1);
        let update = catalog.tree.accessibility_update(1.0);
        let prompt = update
            .nodes
            .iter()
            .find(|(_, node)| node.role() == noa_gpui_core::accesskit::Role::AlertDialog)
            .unwrap();
        assert!(
            prompt
                .1
                .description()
                .unwrap_or_default()
                .contains("This demo does not save documents"),
            "the demo must not promise to save a document it will discard"
        );
        let buttons: Vec<_> = update
            .nodes
            .iter()
            .filter(|(_, node)| node.role() == noa_gpui_core::accesskit::Role::Button)
            .filter_map(|(_, node)| node.label())
            .collect();
        assert!(buttons.contains(&"Simulate Save"));
        assert!(buttons.contains(&"Don’t Save"));
        assert!(buttons.contains(&"Cancel"));
        key(&mut catalog, &mut text, noa_gpui_core::Key::Escape);
        for now in [10.0, 11.0] {
            catalog.animate(now, &mut text);
        }
        catalog.tree.prepare(viewport, &mut text);
        assert_eq!(alerts(&catalog), 0);
        assert_eq!(titles(&catalog).len(), 3, "Cancel keeps the tab");
        assert_eq!(catalog.tree.get::<TextArea>(page).unwrap().text(), draft);
        assert!(catalog.tree.get::<TabView>(tabs).unwrap().is_modified(2));
        catalog.tree.set_focus(Some(tabs));
        key(&mut catalog, &mut text, noa_gpui_core::Key::Delete);
        key(&mut catalog, &mut text, noa_gpui_core::Key::Enter);
        assert_eq!(titles(&catalog), ["Untitled 1", "Untitled 2"]);
        assert!(!catalog.tree.contains(page));
        for now in [20.0, 21.0] {
            catalog.animate(now, &mut text);
        }
        // An unchanged tab closes without asking (Close Untitled 1).
        act(&mut catalog, &mut text, 2);
        assert_eq!(alerts(&catalog), 0);
        assert_eq!(titles(&catalog), ["Untitled 2"]);
    }

    #[test]
    fn every_card_can_be_scrolled_into_view() {
        use noa_gpui_core::widgets::GroupBox;
        for (width, height) in [
            (1280.0, 800.0),
            (1024.0, 768.0),
            (900.0, 700.0),
            (640.0, 480.0),
        ] {
            let mut next = 0;
            let assets = Assets::generate(|_, _, _| {
                next += 1;
                ImageId(next)
            });
            let mut catalog = Catalog::new(&assets);
            let mut text = TextStore::new(FontOptions {
                locale: "en-US".into(),
                load_system_fonts: false,
            });
            catalog.tree.prepare(Size::new(width, height), &mut text);
            let tree = &catalog.tree;
            let view = tree.window_rect(catalog.scroll).unwrap();
            let layout = tree.layout(catalog.scroll).unwrap();
            let reach = view.x + layout.content_size.width;
            let mut cut = 0;
            for &section in tree.children(catalog.scroll) {
                for &grid in tree.children(section) {
                    for &card in tree.children(grid) {
                        if tree.get::<GroupBox>(card).is_none() {
                            continue;
                        }
                        let right = tree.window_rect(card).unwrap().right();
                        cut += usize::from(right > view.right() + 0.5);
                        assert!(
                            right <= reach + 0.5,
                            "{width}: a card ends at {right}, past {reach}"
                        );
                    }
                }
            }
            if width >= 1024.0 {
                assert_eq!(cut, 0, "{width}: cards cut off");
                assert_eq!(
                    layout.content_size.width, layout.rect.width,
                    "{width}: nothing to scroll to sideways"
                );
                continue;
            }
            assert!(cut > 0, "{width}: the test no longer covers a cut-off card");
            catalog.tree.dispatch(
                Event::Wheel {
                    pos: Point::new(view.x + 8.0, view.y + 8.0),
                    delta: Point::new(-40.0, 0.0),
                },
                &mut text,
            );
            let scroll = catalog.tree.get::<ScrollView>(catalog.scroll).unwrap();
            assert!(
                scroll.scroll_position().x > 0.0,
                "{width}: no sideways scrolling"
            );
        }
    }

    #[test]
    fn the_keyboard_reaches_every_control_and_sees_it() {
        use noa_gpui_core::{Key, Modifiers};
        use std::collections::HashSet;
        for (width, height) in [(1280.0, 800.0), (640.0, 480.0)] {
            let mut next = 0;
            let assets = Assets::generate(|_, _, _| {
                next += 1;
                ImageId(next)
            });
            let mut catalog = Catalog::new(&assets);
            let mut text = TextStore::new(FontOptions {
                locale: "en-US".into(),
                load_system_fonts: false,
            });
            let size = Size::new(width, height);
            catalog.tree.prepare(size, &mut text);
            assert_eq!(catalog.tree.focused(), None, "focused before any key");
            let view = catalog.tree.window_rect(catalog.scroll).unwrap();
            let mut seen = HashSet::new();
            let (mut first, mut ctrl, mut escapes) = (None, false, 0);
            loop {
                let modifiers = Modifiers {
                    ctrl,
                    ..Default::default()
                };
                let key = Event::KeyDown {
                    key: Key::Tab,
                    modifiers,
                };
                catalog.tree.dispatch(key, &mut text);
                catalog.tree.prepare(size, &mut text);
                let focused = catalog.tree.focused().unwrap();
                if Some(focused) == first {
                    break;
                }
                if !seen.insert(focused) {
                    // Tab cycled inside something, so Ctrl+Tab must leave it.
                    assert!(!ctrl, "{width}: Ctrl+Tab did not leave {focused:?}");
                    (ctrl, escapes) = (true, escapes + 1);
                    continue;
                }
                (first, ctrl) = (first.or(Some(focused)), false);
                if catalog.tree.ancestors(focused).any(|a| a == catalog.scroll) {
                    // All of it, or its start when it is larger than the view.
                    let r = catalog.tree.window_rect(focused).unwrap();
                    let fits_x = r.right() <= view.right() + 0.5 || r.width > view.width;
                    let fits_y = r.bottom() <= view.bottom() + 0.5 || r.height > view.height;
                    assert!(
                        r.x >= view.x && r.y >= view.y && fits_x && fits_y,
                        "{width}: {focused:?} at {r:?} is out of view"
                    );
                }
            }
            assert!(seen.len() > 250, "{width}: only {} controls", seen.len());
            assert!(escapes > 0, "{width}: the test no longer covers Ctrl+Tab");
        }
    }
}

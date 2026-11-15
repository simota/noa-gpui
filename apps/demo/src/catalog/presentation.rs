use noa_gpui_core::style::{AlignItems, FlexDirection, JustifyContent, Size as LayoutSize, length};
use noa_gpui_core::widgets::{
    Alert, AlertResponse, AlertStyle, Button, ButtonClicked, ColorPanel, Container, FontChoice,
    FontPanel, Label, Panel, PanelClosed, PanelStyle, PopUpButton, Popover, Sheet, SheetClosed,
    Slider, Switch, TextField, Toolbar, ToolbarDisplay, ToolbarItem, ToolbarSymbol, WindowEvent,
    WindowManager,
};
use noa_gpui_core::{Edge, Message, OverlayDismissed, Role, Style, WidgetId};
use noa_gpui_scene::{Color, Point};

use super::kit::{Kit, gap, row_style, window_rect};
use super::{Demo, DemoCx, SectionSpec};

pub const SECTIONS: [SectionSpec; 8] = [
    SectionSpec {
        title: "Alert",
        description: "A message, details, an icon and buttons. The first button is the \
                      default (Return), Cancel answers Escape and ⌘-period, and Tab reaches \
                      every button and the \"do not show again\" checkbox. Shown in the \
                      middle of the window, or as a sheet. Nothing below it takes input until \
                      it is answered.",
        build: alert,
    },
    SectionSpec {
        title: "Sheet",
        description: "A modal panel hanging from the top of the content area, sliding down \
                      and back up. It blocks the window until finished or cancelled; its own \
                      controls work as usual.",
        build: sheet,
    },
    SectionSpec {
        title: "Drawer",
        description: "A sheet that slides in from an edge of the window instead and runs its \
                      whole length below the header, following the window as it is resized. \
                      Like a sheet it blocks the window until it is closed: Done or Escape.",
        build: drawer,
    },
    SectionSpec {
        title: "Popover",
        description: "A bubble with an arrow pointing at what it is about. It goes on the side \
                      asked for, or the opposite side when there is no room. A click outside \
                      or Escape closes it, and that click still reaches what was clicked.",
        build: popover,
    },
    SectionSpec {
        title: "Tooltip",
        description: "Rest the pointer on a control for a moment to see its help text. Moving \
                      to the next control right after shows its tooltip at once; a click, a \
                      key or scrolling hides it. Screen readers get the text as the control's \
                      description.",
        build: tooltip,
    },
    SectionSpec {
        title: "Panel",
        description: "Floating utility panels and inspectors that do not block the window: drag \
                      them by the title bar, close them with the button on the left. The HUD \
                      style is dark and translucent in either appearance. On the desktop an \
                      inspector can also be its own window that stays above the others.",
        build: panel,
    },
    SectionSpec {
        title: "Standard panels",
        description: "Open and save use the operating system's own panels (NSOpenPanel / \
                      NSSavePanel on macOS), so they know the user's files and places; the \
                      Web build has no access to them. The color and font panels are drawn by \
                      the framework, the same on every platform.",
        build: standard_panels,
    },
    SectionSpec {
        title: "Inner windows",
        description: "Windows inside the window, managed like an operating system's: drag a \
                      title bar to move (edges snap to the desktop and to other windows; the \
                      left, right and top edges offer a half or the whole desktop), drag an \
                      edge or corner to resize, click to bring forward, double-click the title \
                      bar or use the green button to zoom, the yellow one to send a window to \
                      the dock. ⌘` cycles, ⌘W closes, Tab stays inside the window and \
                      Ctrl+Tab leaves it. A sheet \
                      blocks only its own window. On the desktop, dropping a window outside \
                      the area moves it to its own OS window.",
        build: inner_windows,
    },
];

fn note(kit: &mut Kit, parent: WidgetId, text: &str) -> WidgetId {
    kit.tree.insert(
        Some(parent),
        Label::new(text).font_size(12.0).color(Role::Muted),
    )
}

fn set_label(cx: &mut DemoCx, label: WidgetId, text: impl Into<String>) {
    let text = text.into();
    cx.tree
        .update::<Label, _>(label, |l, cx| l.set_text(cx, text));
}

fn clicked(message: &Message, button: WidgetId) -> bool {
    message.source == button && message.downcast_ref::<ButtonClicked>().is_some()
}

fn row(kit: &mut Kit, parent: WidgetId) -> WidgetId {
    kit.tree
        .insert(Some(parent), Container::new(row_style(8.0)))
}

struct AlertDemo {
    informational: WidgetId,
    critical: WidgetId,
    as_sheet: WidgetId,
    result: WidgetId,
    /// The open alert and its button labels.
    open: Option<(WidgetId, Vec<String>)>,
}

impl Demo for AlertDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if let Some((alert, labels)) = &self.open
            && message.source == *alert
            && let Some(response) = message.downcast_ref::<AlertResponse>()
        {
            let text = format!(
                "Answered: {}{}",
                labels[response.button],
                if response.suppressed {
                    " · do not show again"
                } else {
                    ""
                }
            );
            self.open = None;
            set_label(cx, self.result, text);
            return true;
        }
        let alert = if clicked(message, self.informational) {
            Alert::new("Software Update")
                .informative_text("Version 2.0 is ready to install. It takes about a minute.")
                .button("Install")
                .button("Later")
        } else if clicked(message, self.critical) {
            Alert::new("Delete “Quarterly Report”?")
                .informative_text("This item will be deleted immediately. You can’t undo this.")
                .style(AlertStyle::Critical)
                .button("Delete")
                .button("Cancel")
                .suppression("Do not ask again")
        } else if clicked(message, self.as_sheet) {
            Alert::new("Do you want to keep the changes you made to “Notes”?")
                .informative_text("Your changes will be lost if you don’t save them.")
                .style(AlertStyle::Warning)
                .button("Save")
                .button("Cancel")
                .button("Don’t Save")
        } else {
            return false;
        };
        let labels = alert.buttons().to_vec();
        let id = if message.source == self.as_sheet {
            alert.open_as_sheet(cx.tree, cx.content_top)
        } else {
            alert.open(cx.tree)
        };
        self.open = Some((id, labels));
        true
    }
}

fn alert(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.card("Alert::open: centered, fades in");
    let buttons = row(kit, card);
    let informational = kit
        .tree
        .insert(Some(buttons), Button::new("Informational…"));
    let critical = kit.tree.insert(Some(buttons), Button::new("Critical…"));
    let card = kit.card("Alert::open_as_sheet: slides down");
    let as_sheet = kit.tree.insert(Some(card), Button::new("Close Document…"));
    let card = kit.card("AlertResponse");
    let result = note(kit, card, "Answered: —");
    Box::new(AlertDemo {
        informational,
        critical,
        as_sheet,
        result,
        open: None,
    })
}

struct SheetDemo {
    open_button: WidgetId,
    result: WidgetId,
    open: Option<OpenSheet>,
}

struct OpenSheet {
    sheet: WidgetId,
    name: WidgetId,
    format: WidgetId,
    create: WidgetId,
    cancel: WidgetId,
}

const FORMATS: [&str; 3] = ["Rich Text", "Plain Text", "Markdown"];

impl Demo for SheetDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if clicked(message, self.open_button) {
            if self.open.is_none() {
                self.open = Some(open_sheet(cx));
            }
            return true;
        }
        let Some(open) = &self.open else {
            return false;
        };
        if message.source == open.sheet && message.downcast_ref::<SheetClosed>().is_some() {
            self.open = None;
            return true;
        }
        let create = clicked(message, open.create);
        if !create && !clicked(message, open.cancel) {
            return false;
        }
        if cx
            .tree
            .get::<Sheet>(open.sheet)
            .is_some_and(Sheet::is_closing)
        {
            return true;
        }
        let text = if create {
            let name = cx
                .tree
                .get::<TextField>(open.name)
                .map_or(String::new(), |f| f.text().to_owned());
            let format = cx
                .tree
                .get::<PopUpButton>(open.format)
                .map_or(0, PopUpButton::selected);
            let name = if name.is_empty() { "Untitled" } else { &name };
            format!("Created “{name}” ({})", FORMATS[format])
        } else {
            "Cancelled".to_owned()
        };
        set_label(cx, self.result, text);
        Sheet::close(cx.tree, open.sheet);
        true
    }
}

fn open_sheet(cx: &mut DemoCx) -> OpenSheet {
    let tree = &mut *cx.tree;
    let sheet = Sheet::new(360.0).open(tree, cx.content_top);
    tree.insert(
        Some(sheet),
        Label::new("New Document").font_size(15.0).weight(700),
    );
    let name = tree.insert(Some(sheet), TextField::new("Name", 320.0));
    tree.set_accessible_label(name, "Name");
    let options = tree.insert(Some(sheet), Container::new(row_style(8.0)));
    tree.insert(Some(options), Label::new("Format:"));
    let format = tree.insert(Some(options), PopUpButton::new(FORMATS));
    tree.set_accessible_label(format, "Format");
    let buttons = tree.insert(
        Some(sheet),
        Container::new(Style {
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::END,
            gap: gap(8.0, 0.0),
            margin: noa_gpui_core::style::Rect {
                top: length(6.0),
                ..noa_gpui_core::style::Rect::zero()
            },
            ..Default::default()
        }),
    );
    let cancel = tree.insert(Some(buttons), Button::new("Cancel").cancel_button(true));
    let create = tree.insert(Some(buttons), Button::new("Create").default_button(true));
    tree.set_focus(Some(name));
    OpenSheet {
        sheet,
        name,
        format,
        create,
        cancel,
    }
}

fn sheet(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.card("Sheet::open · Sheet::close");
    let open_button = kit.tree.insert(Some(card), Button::new("New Document…"));
    let result = note(kit, card, "—");
    Box::new(SheetDemo {
        open_button,
        result,
        open: None,
    })
}

struct DrawerDemo {
    buttons: Vec<(WidgetId, Edge)>,
    /// The open drawer and its Done button.
    open: Option<(WidgetId, WidgetId)>,
}

impl Demo for DrawerDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if let Some(&(_, edge)) = self.buttons.iter().find(|(b, _)| clicked(message, *b)) {
            if self.open.is_none() {
                self.open = Some(open_drawer(cx, edge));
            }
            return true;
        }
        let Some((drawer, done)) = self.open else {
            return false;
        };
        if message.source == drawer && message.downcast_ref::<SheetClosed>().is_some() {
            self.open = None;
            return true;
        }
        if clicked(message, done) {
            Sheet::close(cx.tree, drawer);
            return true;
        }
        false
    }
}

fn open_drawer(cx: &mut DemoCx, edge: Edge) -> (WidgetId, WidgetId) {
    let tree = &mut *cx.tree;
    let extent = match edge {
        Edge::Left | Edge::Right => 280.0,
        Edge::Top | Edge::Bottom => 180.0,
    };
    let drawer = Sheet::drawer(edge, extent).open(tree, cx.content_top);
    tree.insert(
        Some(drawer),
        Label::new("Filters").font_size(15.0).weight(700),
    );
    for (label, on) in [("Only unread", true), ("Show attachments", false)] {
        let row = tree.insert(Some(drawer), Container::new(row_style(8.0)));
        let switch = tree.insert(Some(row), Switch::new(on));
        tree.set_accessible_label(switch, label);
        tree.insert(Some(row), Label::new(label));
    }
    let done = tree.insert(
        Some(drawer),
        Button::new("Done").default_button(true).cancel_button(true),
    );
    tree.set_focus(Some(done));
    (drawer, done)
}

fn drawer(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.card("Sheet::drawer(Edge::Left | Right | Bottom, extent).open(tree, top)");
    let row = row(kit, card);
    let buttons = [
        ("Left", Edge::Left),
        ("Right", Edge::Right),
        ("Bottom", Edge::Bottom),
    ]
    .into_iter()
    .map(|(title, edge)| (kit.tree.insert(Some(row), Button::new(title)), edge))
    .collect();
    Box::new(DrawerDemo {
        buttons,
        open: None,
    })
}

struct PopoverDemo {
    buttons: Vec<(WidgetId, Edge)>,
    open: Option<WidgetId>,
}

impl Demo for PopoverDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if self.open == Some(message.source) && message.downcast_ref::<OverlayDismissed>().is_some()
        {
            self.open = None;
            return true;
        }
        let Some(&(button, edge)) = self.buttons.iter().find(|(b, _)| clicked(message, *b)) else {
            return false;
        };
        if let Some(open) = self.open.take() {
            cx.tree.close_overlay(open, cx.text);
        }
        let anchor = window_rect(cx.tree, button);
        let popover = Popover::new().open(cx.tree, anchor, edge);
        cx.tree
            .insert(Some(popover), Label::new("Brightness").weight(600));
        let brightness = cx.tree.insert(Some(popover), Slider::new(0.0, 100.0, 60.0));
        cx.tree.set_accessible_label(brightness, "Brightness");
        let row = cx
            .tree
            .insert(Some(popover), Container::new(row_style(8.0)));
        let automatic = cx.tree.insert(Some(row), Switch::new(true));
        cx.tree.set_accessible_label(automatic, "Automatic");
        cx.tree.insert(Some(row), Label::new("Automatic"));
        cx.tree.insert(
            Some(popover),
            Label::new("Click outside or press Escape to close.")
                .font_size(12.0)
                .color(Role::Muted),
        );
        self.open = Some(popover);
        true
    }
}

fn popover(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.card("Popover::open(anchor, edge): the preferred side");
    let buttons = row(kit, card);
    let edges = [
        ("Below", Edge::Bottom),
        ("Above", Edge::Top),
        ("Right", Edge::Right),
        ("Left", Edge::Left),
    ];
    let buttons = edges
        .iter()
        .map(|&(label, edge)| (kit.tree.insert(Some(buttons), Button::new(label)), edge))
        .collect();
    Box::new(PopoverDemo {
        buttons,
        open: None,
    })
}

fn tooltip(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.card("Button::tooltip");
    let buttons = row(kit, card);
    for (label, tip) in [
        ("Save", "Save the document (⌘S)"),
        ("Share", "Send a copy by Mail, Messages or AirDrop"),
        (
            "Archive",
            "Move the selected messages to the Archive mailbox. They stay searchable and \
             can be moved back at any time.",
        ),
    ] {
        kit.tree
            .insert(Some(buttons), Button::new(label).tooltip(tip));
    }
    let card = kit.card("Icon-only toolbar items show their names");
    kit.tree.insert(
        Some(card),
        Toolbar::new(vec![
            ToolbarItem::button("Back").symbol(ToolbarSymbol::Back),
            ToolbarItem::button("Forward").symbol(ToolbarSymbol::Forward),
            ToolbarItem::button("New Item").symbol(ToolbarSymbol::Add),
            ToolbarItem::button("Share").symbol(ToolbarSymbol::Share),
            ToolbarItem::button("Get Info").symbol(ToolbarSymbol::Info),
        ])
        .display(ToolbarDisplay::IconOnly),
    );
    Box::new(())
}

struct PanelDemo {
    inspector_button: WidgetId,
    hud_button: WidgetId,
    #[cfg(feature = "native")]
    window_button: WidgetId,
    inspector: Option<WidgetId>,
    hud: Option<WidgetId>,
    #[cfg(feature = "native")]
    windows_opened: u32,
}

impl Demo for PanelDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.downcast_ref::<PanelClosed>().is_some() {
            for slot in [&mut self.inspector, &mut self.hud] {
                if *slot == Some(message.source) {
                    *slot = None;
                    return true;
                }
            }
            return false;
        }
        if clicked(message, self.inspector_button) {
            if self.inspector.is_none() {
                let at = beside(cx, self.inspector_button);
                self.inspector = Some(inspector(cx, at));
            }
            return true;
        }
        if clicked(message, self.hud_button) {
            if self.hud.is_none() {
                let at = beside(cx, self.hud_button);
                self.hud = Some(hud(cx, at));
            }
            return true;
        }
        #[cfg(feature = "native")]
        if clicked(message, self.window_button) {
            self.windows_opened += 1;
            cx.windows.push(noa_gpui_platform::WindowRequest {
                title: format!("Inspector {}", self.windows_opened),
                size: (260.0, 220.0),
                screen: Box::new(super::note_window::PanelWindow::new(
                    cx.tree.theme().clone(),
                )),
                floating: true,
                titlebar: Default::default(),
                min_size: None,
            });
            return true;
        }
        false
    }
}

/// Where to open a panel for `button` (in a row of buttons in a card): right of the
/// section's cards and level with the button, so it covers none of them.
fn beside(cx: &DemoCx, button: WidgetId) -> Point {
    let r = window_rect(cx.tree, button);
    // button → row → card content → card → the section's cards.
    let cards = cx.tree.ancestors(button).nth(4).unwrap_or(button);
    let right = cx
        .tree
        .children(cards)
        .iter()
        .map(|&card| window_rect(cx.tree, card).right())
        .fold(r.right(), f32::max);
    Point::new(right + 16.0, r.y - 40.0)
}

fn inspector(cx: &mut DemoCx, at: Point) -> WidgetId {
    let tree = &mut *cx.tree;
    let panel = Panel::new("Inspector").open(tree, at);
    tree.insert(Some(panel), Label::new("Opacity").font_size(12.0));
    let opacity = tree.insert(Some(panel), Slider::new(0.0, 100.0, 80.0));
    tree.set_accessible_label(opacity, "Opacity");
    let row = tree.insert(Some(panel), Container::new(row_style(8.0)));
    let snap = tree.insert(Some(row), Switch::new(false));
    tree.set_accessible_label(snap, "Snap to grid");
    tree.insert(Some(row), Label::new("Snap to grid").font_size(12.0));
    tree.insert(
        Some(panel),
        Label::new("The window below still takes clicks.")
            .font_size(11.0)
            .color(Role::Muted),
    );
    panel
}

fn hud(cx: &mut DemoCx, at: Point) -> WidgetId {
    let tree = &mut *cx.tree;
    let white = Color::rgba(1.0, 1.0, 1.0, 0.92);
    let panel = Panel::new("Adjust")
        .style(PanelStyle::Hud)
        .width(220.0)
        .open(tree, at);
    for (name, value) in [("Exposure", 0.4), ("Contrast", 0.6)] {
        tree.insert(Some(panel), Label::new(name).font_size(12.0).color(white));
        let slider = tree.insert(Some(panel), Slider::new(0.0, 1.0, value));
        tree.set_accessible_label(slider, name);
    }
    panel
}

fn panel(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.card("Panel::open (PanelStyle::Utility / Hud)");
    let buttons = row(kit, card);
    let inspector_button = kit.tree.insert(Some(buttons), Button::new("Inspector"));
    let hud_button = kit.tree.insert(Some(buttons), Button::new("HUD"));
    let card = kit.card("A panel in its own window (desktop)");
    let window_button = kit.tree.insert(Some(card), Button::new("Inspector Window"));
    if cfg!(feature = "native") {
        note(kit, card, "Stays above the app's other windows.");
    } else {
        kit.tree.set_enabled(window_button, false);
        note(kit, card, "The Web build draws into a single canvas.");
    }
    Box::new(PanelDemo {
        inspector_button,
        hud_button,
        #[cfg(feature = "native")]
        window_button,
        inspector: None,
        hud: None,
        #[cfg(feature = "native")]
        windows_opened: 0,
    })
}

struct StandardDemo {
    #[cfg(feature = "native")]
    open_button: WidgetId,
    #[cfg(feature = "native")]
    save_button: WidgetId,
    #[cfg(feature = "native")]
    file_result: WidgetId,
    #[cfg(feature = "native")]
    choosing: bool,
    colors_button: WidgetId,
    fonts_button: WidgetId,
    sample: WidgetId,
    color: Color,
    font: FontChoice,
    color_panel: Option<ColorPanel>,
    font_panel: Option<FontPanel>,
}

impl Demo for StandardDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if let Some(panel) = &self.color_panel {
            if let Some(color) = panel.handle(message) {
                self.color = color;
                cx.tree
                    .update::<Label, _>(self.sample, |l, cx| l.set_color(cx, color));
                return true;
            }
            if message.source == panel.panel() && message.downcast_ref::<PanelClosed>().is_some() {
                self.color_panel = None;
                return true;
            }
        }
        if let Some(panel) = &mut self.font_panel {
            if let Some(font) = panel.handle(cx.tree, message) {
                cx.tree.update::<Label, _>(self.sample, |l, cx| {
                    l.set_font(
                        cx,
                        Some(font.family.clone()),
                        font.size.min(36.0),
                        font.weight(),
                        font.italic,
                    );
                });
                self.font = font;
                return true;
            }
            if message.source == panel.panel() && message.downcast_ref::<PanelClosed>().is_some() {
                self.font_panel = None;
                return true;
            }
        }
        if clicked(message, self.colors_button) {
            if self.color_panel.is_none() {
                let at = beside(cx, self.colors_button);
                self.color_panel = Some(ColorPanel::open(cx.tree, self.color, at));
            }
            return true;
        }
        if clicked(message, self.fonts_button) {
            if self.font_panel.is_none() {
                let at = beside(cx, self.fonts_button);
                self.font_panel = Some(FontPanel::open(cx.tree, cx.text, self.font.clone(), at));
            }
            return true;
        }
        #[cfg(feature = "native")]
        {
            use noa_gpui_platform::FileDialogRequest;
            let text = vec![("Text".to_owned(), vec!["txt".to_owned(), "md".to_owned()])];
            if clicked(message, self.open_button) {
                cx.file_dialogs.push(FileDialogRequest {
                    save: false,
                    title: "Open".into(),
                    file_name: String::new(),
                    filters: text,
                });
                self.choosing = true;
                return true;
            }
            if clicked(message, self.save_button) {
                cx.file_dialogs.push(FileDialogRequest {
                    save: true,
                    title: "Save".into(),
                    file_name: "Untitled.txt".into(),
                    filters: text,
                });
                self.choosing = true;
                return true;
            }
        }
        false
    }

    #[cfg(feature = "native")]
    fn file_dialog_finished(
        &mut self,
        cx: &mut DemoCx,
        result: &noa_gpui_platform::FileDialogResult,
    ) {
        use noa_gpui_platform::FileDialogResult;
        if !std::mem::take(&mut self.choosing) {
            return;
        }
        let text = match result {
            FileDialogResult::Opened(paths) => match paths.as_slice() {
                [one] => format!("Opened {}", one.display()),
                many => format!("Opened {} files", many.len()),
            },
            FileDialogResult::Saved(path) => format!("Save to {}", path.display()),
            FileDialogResult::Cancelled => "Cancelled".into(),
        };
        set_label(cx, self.file_result, text);
    }
}

fn standard_panels(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.card("The OS's open and save panels (desktop)");
    let buttons = row(kit, card);
    let open_button = kit.tree.insert(Some(buttons), Button::new("Open…"));
    let save_button = kit.tree.insert(Some(buttons), Button::new("Save…"));
    let available = cfg!(all(
        feature = "native",
        any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux"
        )
    ));
    let status = if available {
        "—"
    } else {
        "Not available in this build."
    };
    #[cfg(feature = "native")]
    let file_result = note(kit, card, status);
    #[cfg(not(feature = "native"))]
    note(kit, card, status);
    if !available {
        kit.tree.set_enabled(open_button, false);
        kit.tree.set_enabled(save_button, false);
    }

    let card = kit.card("ColorPanel · FontPanel (drawn by the framework)");
    let buttons = row(kit, card);
    let colors_button = kit.tree.insert(Some(buttons), Button::new("Colors…"));
    let fonts_button = kit.tree.insert(Some(buttons), Button::new("Fonts…"));
    let color = Color::hex(0x2f6fde);
    let sample_box = kit.tree.insert(
        Some(card),
        Container::new(Style {
            align_items: AlignItems::CENTER,
            size: LayoutSize {
                width: length(260.0),
                height: length(56.0),
            },
            ..Default::default()
        }),
    );
    let sample = kit
        .tree
        .insert(Some(sample_box), Label::new("Sample text").color(color));
    Box::new(StandardDemo {
        #[cfg(feature = "native")]
        open_button,
        #[cfg(feature = "native")]
        save_button,
        #[cfg(feature = "native")]
        file_result,
        #[cfg(feature = "native")]
        choosing: false,
        colors_button,
        fonts_button,
        sample,
        color,
        font: FontChoice {
            family: "Inter".into(),
            size: 14.0,
            bold: false,
            italic: false,
        },
        color_panel: None,
        font_panel: None,
    })
}

struct InnerWindowsDemo {
    manager: WindowManager,
    new_button: WidgetId,
    sheet_button: WidgetId,
    alert_button: WidgetId,
    status: WidgetId,
    opened: u32,
    /// The open sheet and its buttons.
    sheet: Option<(WidgetId, WidgetId, WidgetId)>,
}

fn window_content(tree: &mut noa_gpui_core::UiTree, window: WidgetId, note: &str) {
    let body = tree.insert(
        Some(window),
        Container::new(Style {
            flex_direction: FlexDirection::Column,
            gap: gap(0.0, 8.0),
            padding: noa_gpui_core::style::Rect::length(12.0),
            ..Default::default()
        }),
    );
    tree.insert(
        Some(body),
        Label::new(note).font_size(12.0).color(Role::Muted),
    );
    let text = tree.insert(Some(body), TextField::new("Type here", 180.0));
    tree.set_accessible_label(text, "Sample text");
    let amount = tree.insert(Some(body), Slider::new(0.0, 100.0, 40.0));
    tree.set_accessible_label(amount, "Amount");
    let row = tree.insert(Some(body), Container::new(row_style(8.0)));
    tree.insert(Some(row), Button::new("Cancel").cancel_button(true));
    tree.insert(Some(row), Button::new("OK").default_button(true));
}

impl InnerWindowsDemo {
    fn open_window(&mut self, tree: &mut noa_gpui_core::UiTree) {
        self.opened += 1;
        let window = self
            .manager
            .open(tree, &format!("Window {}", self.opened), None);
        window_content(tree, window, "Return presses this window's OK.");
    }
}

impl Demo for InnerWindowsDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if let Some(event) = self.manager.handle(cx.tree, cx.text, message) {
            // Closing a window removes its sheet without a SheetClosed.
            if let Some((sheet, ..)) = self.sheet
                && !cx.tree.contains(sheet)
            {
                self.sheet = None;
            }
            match event {
                WindowEvent::Activated(_) | WindowEvent::Closed(_) => {}
                #[cfg(feature = "native")]
                WindowEvent::TornOff { window, .. } => {
                    // The sheet's buttons are handled here, not in the torn-off window,
                    // so dismiss it instead of passing it on.
                    if let Some((sheet, ..)) = self.sheet
                        && cx.tree.parent(sheet) == Some(window)
                    {
                        cx.tree.remove(sheet, cx.text);
                        self.sheet = None;
                    }
                    if let Some((title, content)) =
                        self.manager.take_window(cx.tree, cx.text, window)
                    {
                        cx.windows.push(noa_gpui_platform::WindowRequest {
                            title: title.clone(),
                            size: (380.0, 280.0),
                            screen: Box::new(super::note_window::TornWindow::new(
                                noa_gpui_platform::Detached { title, content },
                                cx.tree.theme().clone(),
                            )),
                            floating: false,
                            titlebar: Default::default(),
                            min_size: None,
                        });
                        set_label(cx, self.status, "Moved to its own OS window");
                    }
                }
                #[cfg(not(feature = "native"))]
                WindowEvent::TornOff { .. } => {}
            }
            return true;
        }
        if clicked(message, self.new_button) {
            self.open_window(cx.tree);
            return true;
        }
        // One sheet or alert per window at a time.
        let active = self
            .manager
            .active()
            .filter(|&w| !self.manager.is_blocked(cx.tree, w));
        if clicked(message, self.sheet_button) {
            if let (Some(window), None) = (active, &self.sheet) {
                let sheet = self
                    .manager
                    .present_sheet(cx.tree, window, Sheet::new(240.0));
                cx.tree
                    .insert(Some(sheet), Label::new("Rename window").weight(700));
                let name = cx.tree.insert(Some(sheet), TextField::new("Name", 200.0));
                cx.tree.set_accessible_label(name, "Window name");
                let row = cx.tree.insert(Some(sheet), Container::new(row_style(8.0)));
                let cancel = cx
                    .tree
                    .insert(Some(row), Button::new("Cancel").cancel_button(true));
                let done = cx
                    .tree
                    .insert(Some(row), Button::new("Done").default_button(true));
                cx.tree.set_focus(Some(name));
                self.sheet = Some((sheet, cancel, done));
            }
            return true;
        }
        if clicked(message, self.alert_button) {
            if let Some(window) = active {
                self.manager.present_alert(
                    cx.tree,
                    window,
                    Alert::new("Discard the changes in this window?")
                        .informative_text("Only this window waits for the answer.")
                        .button("Discard")
                        .button("Cancel"),
                );
            }
            return true;
        }
        if let Some((sheet, cancel, done)) = self.sheet {
            if message.source == sheet && message.downcast_ref::<SheetClosed>().is_some() {
                self.sheet = None;
                return true;
            }
            if clicked(message, cancel) || clicked(message, done) {
                Sheet::close(cx.tree, sheet);
                return true;
            }
        }
        if let Some(response) = message.downcast_ref::<AlertResponse>()
            && cx
                .tree
                .parent(message.source)
                .is_some_and(|p| self.manager.windows().contains(&p))
        {
            let text = if response.button == 0 {
                "Alert: Discard"
            } else {
                "Alert: Cancel"
            };
            set_label(cx, self.status, text);
            return true;
        }
        false
    }

    #[cfg(feature = "native")]
    fn adopt(&mut self, cx: &mut DemoCx, detached: &mut Option<noa_gpui_platform::Detached>) {
        if let Some(detached) = detached.take() {
            self.manager
                .adopt(cx.tree, &detached.title, detached.content);
            set_label(cx, self.status, "Back on the desktop");
        }
    }
}

fn inner_windows(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.wide_card("Desktop · InnerWindow · WindowManager", 720.0);
    let buttons = row(kit, card);
    let new_button = kit.tree.insert(Some(buttons), Button::new("New Window"));
    let sheet_button = kit
        .tree
        .insert(Some(buttons), Button::new("Sheet in Active Window"));
    let alert_button = kit
        .tree
        .insert(Some(buttons), Button::new("Alert in Active Window"));
    let mut manager = WindowManager::new(
        kit.tree,
        card,
        Style {
            size: LayoutSize {
                width: length(684.0),
                height: length(440.0),
            },
            flex_shrink: 0.0,
            ..Default::default()
        },
    );
    let first = manager.open(
        kit.tree,
        "Window 1",
        Some(noa_gpui_scene::Rect::new(24.0, 20.0, 300.0, 230.0)),
    );
    window_content(kit.tree, first, "Drag my title bar; resize from my edges.");
    let second = manager.open(
        kit.tree,
        "Window 2",
        Some(noa_gpui_scene::Rect::new(250.0, 110.0, 320.0, 240.0)),
    );
    window_content(kit.tree, second, "Return presses this window's OK.");
    let status = note(kit, card, "—");
    Box::new(InnerWindowsDemo {
        manager,
        new_button,
        sheet_button,
        alert_button,
        status,
        opened: 2,
        sheet: None,
    })
}

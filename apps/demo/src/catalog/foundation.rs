use ui_core::animation::{Easing, Transition};
use ui_core::style::{FlexDirection, Size as LayoutSize, length};
#[cfg(feature = "native")]
use ui_core::widgets::FileDialogRequested;
use ui_core::widgets::{
    Button, ButtonClicked, CheckState, Checkbox, CheckboxChanged, Container, FileDropZone,
    FilesDropped, Label, TextArea, TextField,
};
use ui_core::{
    Appearance, Cursor, Design, DragData, Event, MeasureCx, MeasureInput, Message,
    OverlayDismissed, OverlayOptions, Placement, Role, Shortcut, ShortcutPressed, Style, WidgetId,
};
use ui_core::{EventCx, PaintCx, UpdateCx, Widget};
use ui_scene::{Border, Color, Point, Rect, Size, TextLayoutId};
use ui_text::{TextStore, TextStyle};

use super::kit::{
    DIVIDER, Kit, NavItem, NavSelected, gap, overlay_panel, row_style, swatch, window_rect,
};
#[cfg(feature = "native")]
use super::note_window::NoteWindow;
use super::{Demo, DemoCx, SectionSpec};
#[cfg(feature = "native")]
use ui_platform::WindowRequest;

pub const SECTIONS: [SectionSpec; 8] = [
    SectionSpec {
        title: "Theme",
        description: "Widgets take colors by role (Role::Text, Role::Accent, …) or as fixed \
                      colors, and round corners through Theme::corner. UiTree::set_theme \
                      switches the design (Flat + Vibrancy, Big Sur, Liquid Glass, Aero, Metro, \
                      Fluent 2, Noa), light / \
                      dark and the accent, and repaints everything. System follows the OS \
                      appearance. Increase contrast draws control borders at 3:1 or more; \
                      reduce motion makes transitions jump to their end.",
        build: theme,
    },
    SectionSpec {
        title: "Disabled state",
        description: "UiTree::set_enabled(id, false) disables a whole subtree: it draws \
                      dimmed, ignores the pointer, and drops out of Tab order. A focused \
                      widget loses focus when it is disabled.",
        build: disabled,
    },
    SectionSpec {
        title: "Overlay",
        description: "UiTree::open_overlay adds a separate root drawn above the main tree and \
                      hit-tested first. It sizes to its content and is placed next to an \
                      anchor, flipping to stay in the window. Menus and popovers close on an \
                      outside click or Escape; modal overlays block everything below.",
        build: overlay,
    },
    SectionSpec {
        title: "Animation",
        description: "A widget asks for frames with request_animation_frame and gets \
                      Event::AnimationFrame until it stops asking; with nothing animating the \
                      app draws nothing. Transition interpolates values with easing.",
        build: animation,
    },
    SectionSpec {
        title: "Keyboard",
        description: "Arrow keys move focus inside a focus group (radio buttons, segments, \
                      toolbars), which Tab treats as a single stop. Shortcuts registered on the \
                      tree fire when the focused widget does not handle the key.",
        build: keyboard,
    },
    SectionSpec {
        title: "Text editing",
        description: "TextEditor is the editing model behind TextField and TextArea: \
                      grapheme-safe selection, word and line movement, undo / redo with typing \
                      grouped into steps, and the clipboard (the OS clipboard on desktop, an \
                      in-app one on the Web).",
        build: text_editing,
    },
    SectionSpec {
        title: "Drag and drop",
        description: "A widget starts a drag with start_drag; widgets under the pointer get \
                      DragOver and accept with accept_drop; the target gets Drop and the \
                      source DragEnd. Files dragged in from the OS arrive the same way.",
        build: drag_and_drop,
    },
    SectionSpec {
        title: "Windows",
        description: "A screen asks the app for more windows. Each window keeps its own focus, \
                      IME and theme; the GPU device and the fonts are shared.",
        build: windows,
    },
];

const ACCENTS: [(&str, Color); 5] = [
    ("Blue", ui_core::theme::DEFAULT_ACCENT),
    ("Purple", Color::hex(0x8e5bd8)),
    ("Pink", Color::hex(0xd6455d)),
    ("Orange", Color::hex(0xe8812d)),
    ("Green", Color::hex(0x2f9e5b)),
];

struct ThemeDemo {
    designs: [WidgetId; Design::ALL.len()],
    light: WidgetId,
    dark: WidgetId,
    system: WidgetId,
    accents: Vec<(WidgetId, Color)>,
    contrast: WidgetId,
    motion: WidgetId,
}

impl Demo for ThemeDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if let Some(CheckboxChanged(state)) = message.downcast_ref::<CheckboxChanged>() {
            let on = *state == CheckState::On;
            if message.source == self.contrast {
                let theme = cx.tree.theme().clone().with_increased_contrast(on);
                cx.tree.set_theme(theme);
            } else if message.source == self.motion {
                cx.tree.set_reduce_motion(on);
            } else {
                return false;
            }
            return true;
        }
        if message.downcast_ref::<ButtonClicked>().is_none() {
            return false;
        }
        let source = message.source;
        let current = cx.tree.theme().clone();
        let theme = if let Some(i) = self.designs.iter().position(|&b| b == source) {
            current.with_design(Design::ALL[i])
        } else if source == self.light || source == self.dark {
            *cx.follow_system = false;
            let appearance = if source == self.light {
                Appearance::Light
            } else {
                Appearance::Dark
            };
            current.with_appearance(appearance)
        } else if source == self.system {
            *cx.follow_system = true;
            current.with_appearance(cx.system_appearance)
        } else if let Some(&(_, accent)) = self.accents.iter().find(|(id, _)| *id == source) {
            current.with_accent(accent)
        } else {
            return false;
        };
        cx.tree.set_theme(theme);
        true
    }
}

fn theme(kit: &mut Kit) -> Box<dyn Demo> {
    let design = kit.card("Theme::with_design(Design::…)");
    let row = kit
        .tree
        .insert(Some(design), Container::new(row_style(8.0)));
    let designs = Design::ALL.map(|d| kit.tree.insert(Some(row), Button::new(d.name())));

    let appearance = kit.card("Appearance: Light / Dark / System");
    let row = kit
        .tree
        .insert(Some(appearance), Container::new(row_style(8.0)));
    let light = kit.tree.insert(Some(row), Button::new("Light"));
    let dark = kit.tree.insert(Some(row), Button::new("Dark"));
    let system = kit.tree.insert(Some(row), Button::new("System"));

    let accent = kit.card("Theme::with_accent(color)");
    let row = kit
        .tree
        .insert(Some(accent), Container::new(row_style(8.0)));
    let accents = ACCENTS
        .iter()
        .map(|&(name, color)| (kit.tree.insert(Some(row), Button::new(name)), color))
        .collect();

    let settings = kit.card("Accessibility: with_increased_contrast / set_reduce_motion");
    let column = kit.tree.insert(
        Some(settings),
        Container::new(Style {
            flex_direction: FlexDirection::Column,
            gap: gap(0.0, 8.0),
            ..Default::default()
        }),
    );
    let contrast = kit
        .tree
        .insert(Some(column), Checkbox::new("Increase contrast"));
    let motion = kit
        .tree
        .insert(Some(column), Checkbox::new("Reduce motion"));

    let roles = kit.card("Role → color in the current theme");
    let list = kit.tree.insert(
        Some(roles),
        Container::new(Style {
            flex_direction: FlexDirection::Column,
            gap: gap(0.0, 6.0),
            ..Default::default()
        }),
    );
    for (role, name) in [
        (Role::Background, "Background"),
        (Role::Surface, "Surface"),
        (Role::SurfaceAlt, "SurfaceAlt"),
        (Role::Text, "Text"),
        (Role::Muted, "Muted"),
        (Role::Accent, "Accent"),
        (Role::Outline, "Outline"),
        (Role::Separator, "Separator"),
        (Role::FocusRing, "FocusRing"),
        (Role::Selection, "Selection"),
    ] {
        let row = kit.tree.insert(Some(list), Container::new(row_style(10.0)));
        kit.tree.insert(
            Some(row),
            swatch(28.0, 20.0)
                .background(role)
                .border(1.0, DIVIDER)
                .radius(4.0),
        );
        kit.tree.insert(Some(row), Label::new(name).font_size(13.0));
    }
    Box::new(ThemeDemo {
        designs,
        light,
        dark,
        system,
        accents,
        contrast,
        motion,
    })
}

struct DisabledDemo {
    toggle: WidgetId,
    group: WidgetId,
    enabled: bool,
}

impl Demo for DisabledDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.source != self.toggle || message.downcast_ref::<ButtonClicked>().is_none() {
            return false;
        }
        self.enabled = !self.enabled;
        cx.tree.set_enabled(self.group, self.enabled);
        let label = if self.enabled {
            "Disable the group"
        } else {
            "Enable the group"
        };
        cx.tree
            .update::<Button, _>(self.toggle, |b, cx| b.set_label(cx, label));
        true
    }
}

fn disabled(kit: &mut Kit) -> Box<dyn Demo> {
    let content = kit.card("UiTree::set_enabled(group, false)");
    let toggle = kit
        .tree
        .insert(Some(content), Button::new("Disable the group"));
    let group = kit.tree.insert(
        Some(content),
        Container::new(Style {
            flex_direction: FlexDirection::Column,
            gap: gap(0.0, 8.0),
            padding: ui_core::style::Rect::length(12.0),
            size: LayoutSize {
                width: length(264.0),
                height: ui_core::style::auto(),
            },
            ..Default::default()
        })
        .background(Role::SurfaceAlt)
        .radius(8.0),
    );
    kit.tree
        .insert(Some(group), Label::new("Shipping address").weight(600));
    let street = kit
        .tree
        .insert(Some(group), TextField::new("Street", 240.0));
    kit.tree.set_accessible_label(street, "Street");
    kit.tree.insert(Some(group), Button::new("Save address"));
    Box::new(DisabledDemo {
        toggle,
        group,
        enabled: true,
    })
}

struct OverlayDemo {
    menu_button: WidgetId,
    popover_button: WidgetId,
    alert_button: WidgetId,
    choice: WidgetId,
    menu: Option<(WidgetId, Vec<WidgetId>)>,
    popover: Option<WidgetId>,
    alert: Option<(WidgetId, WidgetId)>,
}

const MENU_ITEMS: [&str; 4] = ["Cut", "Copy", "Paste", "Select All"];

impl Demo for OverlayDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        let source = message.source;
        if message.downcast_ref::<OverlayDismissed>().is_some() {
            if self.menu.as_ref().is_some_and(|(m, _)| *m == source) {
                self.menu = None;
            } else if self.popover == Some(source) {
                self.popover = None;
            } else {
                return false;
            }
            return true;
        }
        if message.downcast_ref::<NavSelected>().is_some() {
            let Some((menu, items)) = &self.menu else {
                return false;
            };
            let Some(index) = items.iter().position(|&i| i == source) else {
                return false;
            };
            let text = format!("Chose: {}", MENU_ITEMS[index]);
            cx.tree.close_overlay(*menu, cx.text);
            self.menu = None;
            cx.tree
                .update::<Label, _>(self.choice, |l, cx| l.set_text(cx, text));
            return true;
        }
        if message.downcast_ref::<ButtonClicked>().is_none() {
            return false;
        }
        if source == self.menu_button && self.menu.is_none() {
            let anchor = window_rect(cx.tree, source);
            let menu = cx.tree.open_overlay(
                overlay_panel(6.0),
                Placement::Below(anchor),
                OverlayOptions::MENU,
            );
            let items = MENU_ITEMS
                .iter()
                .map(|name| cx.tree.insert(Some(menu), NavItem::new(name)))
                .collect();
            self.menu = Some((menu, items));
        } else if source == self.popover_button && self.popover.is_none() {
            let anchor = window_rect(cx.tree, source);
            let popover = cx.tree.open_overlay(
                overlay_panel(14.0),
                Placement::Below(anchor),
                OverlayOptions::POPOVER,
            );
            cx.tree
                .insert(Some(popover), Label::new("Rename layer").weight(600));
            let name = cx
                .tree
                .insert(Some(popover), TextField::new("Layer name", 220.0));
            cx.tree.set_accessible_label(name, "Layer name");
            cx.tree.insert(
                Some(popover),
                Label::new("Click outside or press Escape to close.")
                    .font_size(12.0)
                    .color(Role::Muted),
            );
            self.popover = Some(popover);
        } else if source == self.alert_button && self.alert.is_none() {
            let alert = cx.tree.open_overlay(
                overlay_panel(20.0),
                Placement::Center,
                OverlayOptions::MODAL,
            );
            cx.tree.insert(
                Some(alert),
                Label::new("Delete “Untitled”?").font_size(17.0).weight(700),
            );
            cx.tree.insert(
                Some(alert),
                Label::new("Everything below is blocked until you choose.").color(Role::Muted),
            );
            let ok = cx.tree.insert(Some(alert), Button::new("OK"));
            cx.tree.set_focus(Some(ok));
            self.alert = Some((alert, ok));
        } else if let Some((alert, ok)) = self.alert
            && source == ok
        {
            cx.tree.close_overlay(alert, cx.text);
            self.alert = None;
        } else {
            return false;
        }
        true
    }
}

fn overlay(kit: &mut Kit) -> Box<dyn Demo> {
    let menu = kit.card("OverlayOptions::MENU · Placement::Below");
    let menu_button = kit.tree.insert(Some(menu), Button::new("Edit menu"));
    let choice = kit
        .tree
        .insert(Some(menu), Label::new("Chose: —").color(Role::Muted));
    let popover = kit.card("OverlayOptions::POPOVER: outside clicks pass through");
    let popover_button = kit.tree.insert(Some(popover), Button::new("Rename…"));
    let alert = kit.card("OverlayOptions::MODAL · Placement::Center");
    let alert_button = kit.tree.insert(Some(alert), Button::new("Delete…"));
    Box::new(OverlayDemo {
        menu_button,
        popover_button,
        alert_button,
        choice,
        menu: None,
        popover: None,
        alert: None,
    })
}

struct Meter {
    value: Transition,
    shown: f32,
    sweeping: bool,
    phase: f32,
}

impl Meter {
    fn new() -> Self {
        Self {
            value: Transition::new(0.2),
            shown: 0.2,
            sweeping: false,
            phase: 0.0,
        }
    }

    fn set_target(&mut self, cx: &mut UpdateCx, target: f32) {
        self.value
            .animate_to(target, cx.now(), 0.6, Easing::EaseInOut);
        cx.request_animation_frame();
    }

    fn set_sweeping(&mut self, cx: &mut UpdateCx, sweeping: bool) {
        self.sweeping = sweeping;
        cx.request_animation_frame();
        cx.request_paint();
    }
}

impl Widget for Meter {
    fn style(&self) -> Style {
        Style {
            size: LayoutSize {
                width: length(264.0),
                height: length(10.0),
            },
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let radius = bounds.height / 2.0;
        let track = cx.color(Role::SurfaceAlt);
        cx.rounded_rect(bounds, radius, track, Border::default());
        let fill = cx.color(Role::Accent);
        let width = (bounds.width * self.shown).max(bounds.height);
        cx.rounded_rect(
            Rect::new(0.0, 0.0, width, bounds.height),
            radius,
            fill,
            Border::default(),
        );
        if self.sweeping {
            let band = bounds.width * 0.25;
            let x = (bounds.width + band) * self.phase - band;
            let left = x.max(0.0);
            let right = (x + band).min(bounds.width);
            if right > left {
                cx.rounded_rect(
                    Rect::new(left, 0.0, right - left, bounds.height),
                    radius,
                    Color::WHITE.with_alpha(0.45),
                    Border::default(),
                );
            }
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        if let Event::AnimationFrame { now } = event {
            self.shown = self.value.value(*now);
            self.phase = (*now * 0.8).fract() as f32;
            cx.request_paint();
            if self.sweeping || !self.value.is_done(*now) {
                cx.request_animation_frame();
            }
        }
    }
}

struct AnimationDemo {
    meter: WidgetId,
    targets: Vec<(WidgetId, f32)>,
    sweep: WidgetId,
    sweeping: bool,
}

impl Demo for AnimationDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.downcast_ref::<ButtonClicked>().is_none() {
            return false;
        }
        let source = message.source;
        if let Some(&(_, target)) = self.targets.iter().find(|(id, _)| *id == source) {
            cx.tree
                .update::<Meter, _>(self.meter, |m, cx| m.set_target(cx, target));
        } else if source == self.sweep {
            self.sweeping = !self.sweeping;
            let sweeping = self.sweeping;
            cx.tree
                .update::<Meter, _>(self.meter, |m, cx| m.set_sweeping(cx, sweeping));
            let label = if sweeping { "Stop sweep" } else { "Sweep" };
            cx.tree
                .update::<Button, _>(self.sweep, |b, cx| b.set_label(cx, label));
        } else {
            return false;
        }
        true
    }
}

fn animation(kit: &mut Kit) -> Box<dyn Demo> {
    let content = kit.card(
        "Transition::animate_to(target, now, 0.6 s, EaseInOut). Frames are drawn only while \
         something moves.",
    );
    let meter = kit.tree.insert(Some(content), Meter::new());
    let row = kit
        .tree
        .insert(Some(content), Container::new(row_style(8.0)));
    let targets = [("0 %", 0.0), ("40 %", 0.4), ("100 %", 1.0)]
        .into_iter()
        .map(|(name, target)| (kit.tree.insert(Some(row), Button::new(name)), target))
        .collect();
    let sweep = kit.tree.insert(Some(row), Button::new("Sweep"));
    let hover = kit.card("Button hover fades in 120 ms (Event::HoverChanged)");
    kit.tree.insert(Some(hover), Button::new("Hover over me"));
    Box::new(AnimationDemo {
        meter,
        targets,
        sweep,
        sweeping: false,
    })
}

struct KeyboardDemo {
    counter: WidgetId,
    presses: u32,
}

impl Demo for KeyboardDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.source != self.counter || message.downcast_ref::<ShortcutPressed>().is_none() {
            return false;
        }
        self.presses += 1;
        let text = format!("Shortcut pressed {} time(s)", self.presses);
        cx.tree
            .update::<Label, _>(self.counter, |l, cx| l.set_text(cx, text));
        true
    }
}

fn keyboard(kit: &mut Kit) -> Box<dyn Demo> {
    let group = kit.card(
        "Container::focus_group(true): Tab stops once, ← → ↑ ↓ move inside, Tab returns to \
         the last item",
    );
    let row = kit.tree.insert(
        Some(group),
        Container::new(row_style(6.0)).focus_group(true),
    );
    for name in ["Left", "Center", "Right", "Justify"] {
        kit.tree.insert(Some(row), Button::new(name));
    }
    kit.tree.insert(
        Some(group),
        Label::new("Click a button, then use the arrow keys.")
            .font_size(12.0)
            .color(Role::Muted),
    );

    let shortcut = kit.card(
        "UiTree::add_shortcut(Shortcut::command('k'), id): Cmd+K on Apple platforms, Ctrl+K \
         elsewhere",
    );
    let counter = kit.tree.insert(
        Some(shortcut),
        Label::new("Press Cmd+K / Ctrl+K").weight(600),
    );
    kit.tree.add_shortcut(Shortcut::command('k'), counter);
    Box::new(KeyboardDemo {
        counter,
        presses: 0,
    })
}

fn text_editing(kit: &mut Kit) -> Box<dyn Demo> {
    let field = kit.card(
        "Shift+arrows, drag, double / triple click select; Cmd/Ctrl+C X V copy, cut, paste; \
         Cmd/Ctrl+Z undo, Shift+Cmd+Z redo",
    );
    let input = kit.tree.insert(Some(field), TextField::new("", 264.0));
    kit.tree.set_accessible_label(input, "Sample text");
    kit.tree.update::<TextField, _>(input, |f, cx| {
        f.set_text(
            cx,
            "A long line scrolls sideways to keep the caret in view while you type",
        )
    });
    let area = kit.card("TextArea: Enter for new lines, ↑ ↓ between lines; wraps and grows");
    let notes = kit
        .tree
        .insert(Some(area), TextArea::new("Notes").width(264.0).min_lines(4));
    kit.tree.set_accessible_label(notes, "Notes");
    kit.tree.update::<TextArea, _>(notes, |a, cx| {
        a.set_text(
            cx,
            "Shopping list\n- coffee beans\n- oat milk\n\nLong lines wrap at the edge of the \
             box, and the box grows as you add lines.",
        )
    });
    Box::new(())
}

/// Emitted by a [`DragRow`] when another row is dropped on it.
struct RowDropped {
    dragged: WidgetId,
    /// Dropped on the upper half: insert before this row.
    before: bool,
}

struct DragRow {
    label: String,
    layout: Option<TextLayoutId>,
    text_size: Size,
    pressed_at: Option<Point>,
    dragging: bool,
    /// Where a row hovering over this one would go: `Some(true)` above, `Some(false)` below.
    drop_hint: Option<bool>,
}

impl DragRow {
    fn new(label: &str) -> Self {
        Self {
            label: label.to_owned(),
            layout: None,
            text_size: Size::default(),
            pressed_at: None,
            dragging: false,
            drop_hint: None,
        }
    }
}

impl Widget for DragRow {
    /// The tree shows the closed hand while the row is dragged.
    fn cursor(&self, _local: Point, _size: Size) -> Option<Cursor> {
        Some(Cursor::Grab)
    }

    fn style(&self) -> Style {
        Style {
            padding: ui_core::style::Rect {
                left: length(30.0),
                right: length(12.0),
                top: length(8.0),
                bottom: length(8.0),
            },
            size: LayoutSize {
                width: length(264.0),
                height: ui_core::style::auto(),
            },
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn measures(&self) -> bool {
        true
    }

    fn measure(&mut self, cx: &mut MeasureCx, _input: MeasureInput) -> Size {
        if self.layout.is_none() {
            let id = cx.text.create(&self.label, TextStyle::new(14.0), None);
            self.text_size = cx.text.size(id);
            self.layout = Some(id);
        }
        self.text_size
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let fill = cx.color(if cx.is_hovered() {
            Role::SurfaceAlt
        } else {
            Role::Surface
        });
        let border = Border {
            width: 1.0,
            color: cx.color(Role::Separator),
        };
        cx.rounded_rect(bounds, 6.0, fill, border);
        let grip = cx.color(Role::Muted);
        for i in 0..3 {
            let y = bounds.height / 2.0 - 5.0 + i as f32 * 4.0;
            cx.fill_rect(Rect::new(10.0, y, 12.0, 1.5), grip);
        }
        if let Some(layout) = self.layout {
            let color = cx.color(Role::Text);
            let y = (bounds.height - self.text_size.height) / 2.0;
            let rect = Rect::new(30.0, y, self.text_size.width, self.text_size.height);
            cx.text(rect, layout, color);
        }
        if self.dragging {
            let veil = cx.theme().background.with_alpha(0.6);
            cx.rounded_rect(bounds, 6.0, veil, Border::default());
        }
        if let Some(above) = self.drop_hint {
            let y = if above { -3.0 } else { bounds.height + 1.0 };
            let accent = cx.color(Role::Accent);
            cx.rounded_rect(
                Rect::new(0.0, y, bounds.width, 2.0),
                1.0,
                accent,
                Border::default(),
            );
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            Event::PointerDown { pos, .. } => {
                self.pressed_at = Some(*pos);
                cx.capture_pointer();
                cx.set_handled();
            }
            Event::PointerMove { pos } => {
                if let Some(start) = self.pressed_at
                    && cx.has_capture()
                    && ((pos.x - start.x).abs() > 4.0 || (pos.y - start.y).abs() > 4.0)
                {
                    self.pressed_at = None;
                    self.dragging = true;
                    cx.start_drag(DragData::Widget(cx.id()));
                    cx.request_paint();
                }
            }
            Event::PointerUp { .. } => {
                self.pressed_at = None;
                cx.release_pointer();
            }
            Event::DragOver {
                pos,
                data: DragData::Widget(dragged),
            } if *dragged != cx.id() => {
                let above = cx.local(*pos).y < cx.size().height / 2.0;
                if self.drop_hint != Some(above) {
                    self.drop_hint = Some(above);
                    cx.request_paint();
                }
                cx.accept_drop();
            }
            Event::DragLeave => {
                self.drop_hint = None;
                cx.request_paint();
            }
            Event::Drop {
                data: DragData::Widget(dragged),
                ..
            } => {
                cx.emit(RowDropped {
                    dragged: *dragged,
                    before: self.drop_hint != Some(false),
                });
                self.drop_hint = None;
                cx.request_paint();
            }
            Event::DragEnd { .. } => {
                self.dragging = false;
                cx.request_paint();
            }
            _ => {}
        }
    }

    fn removed(&mut self, text: &mut TextStore) {
        if let Some(id) = self.layout.take() {
            text.remove(id);
        }
    }
}

struct DragDemo {
    rows: Vec<WidgetId>,
    zone: WidgetId,
    dropped: WidgetId,
    #[cfg(feature = "native")]
    choosing: bool,
}

impl DragDemo {
    fn show_files(&self, cx: &mut DemoCx, verb: &str, files: &[std::path::PathBuf]) {
        let names: Vec<String> = files
            .iter()
            .map(|p| {
                p.file_name()
                    .map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into())
            })
            .collect();
        let text = format!("{verb}: {}", names.join(", "));
        cx.tree
            .update::<Label, _>(self.dropped, |l, cx| l.set_text(cx, text));
    }
}

impl Demo for DragDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if let Some(RowDropped { dragged, before }) = message.downcast_ref::<RowDropped>() {
            if !self.rows.contains(&message.source) || !self.rows.contains(dragged) {
                return false;
            }
            let Some(parent) = cx.tree.parent(message.source) else {
                return false;
            };
            let siblings = cx.tree.children(parent);
            let from = siblings.iter().position(|id| id == dragged).unwrap_or(0);
            let target = siblings
                .iter()
                .position(|&id| id == message.source)
                .unwrap_or(0);
            let mut index = if *before { target } else { target + 1 };
            if from < index {
                index -= 1;
            }
            cx.tree.move_child(*dragged, index);
            return true;
        }
        if message.source != self.zone {
            return false;
        }
        if let Some(FilesDropped(files)) = message.downcast_ref() {
            self.show_files(cx, "Dropped", files);
        }
        #[cfg(feature = "native")]
        if let Some(request) = message.downcast_ref::<FileDialogRequested>() {
            let filters = if request.extensions.is_empty() {
                Vec::new()
            } else {
                vec![("Images".to_owned(), request.extensions.clone())]
            };
            cx.file_dialogs.push(ui_platform::FileDialogRequest {
                save: false,
                title: "Choose Images".into(),
                file_name: String::new(),
                filters,
            });
            self.choosing = true;
        }
        true
    }

    #[cfg(feature = "native")]
    fn file_dialog_finished(&mut self, cx: &mut DemoCx, result: &ui_platform::FileDialogResult) {
        if !std::mem::take(&mut self.choosing) {
            return;
        }
        if let ui_platform::FileDialogResult::Opened(files) = result {
            self.show_files(cx, "Chosen", files);
        }
    }
}

fn drag_and_drop(kit: &mut Kit) -> Box<dyn Demo> {
    let list = kit.card("Drag a row onto another (DragData::Widget, UiTree::move_child)");
    let column = kit.tree.insert(
        Some(list),
        Container::new(Style {
            flex_direction: FlexDirection::Column,
            gap: gap(0.0, 6.0),
            ..Default::default()
        }),
    );
    let rows = ["Inbox", "Drafts", "Sent", "Archive", "Trash"]
        .iter()
        .map(|name| kit.tree.insert(Some(column), DragRow::new(name)))
        .collect();

    let files = kit.card("FileDropZone::new().extensions([\"png\", \"jpg\"]) (desktop only)");
    let zone = kit.tree.insert(
        Some(files),
        FileDropZone::new()
            .title("Drop images here")
            .extensions(["png", "jpg", "jpeg", "gif"])
            .width(264.0),
    );
    let dropped = kit.tree.insert(
        Some(files),
        Label::new("Drop images on the box above, or click it to choose them")
            .color(Role::Muted)
            .wrap(true),
    );
    Box::new(DragDemo {
        rows,
        zone,
        dropped,
        #[cfg(feature = "native")]
        choosing: false,
    })
}

struct WindowsDemo {
    #[cfg(feature = "native")]
    open: WidgetId,
    #[cfg(feature = "native")]
    opened: u32,
}

impl Demo for WindowsDemo {
    #[cfg(feature = "native")]
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.source != self.open || message.downcast_ref::<ButtonClicked>().is_none() {
            return false;
        }
        self.opened += 1;
        cx.windows.push(WindowRequest {
            title: format!("Note {}", self.opened),
            size: (420.0, 360.0),
            screen: Box::new(NoteWindow::new(self.opened, cx.tree.theme().clone())),
            floating: false,
        });
        true
    }
}

fn windows(kit: &mut Kit) -> Box<dyn Demo> {
    let content = kit.card(
        "Each window: its own surface, renderer and UiTree, sharing the GPU device and the \
         text store (desktop only)",
    );
    let open = kit.tree.insert(Some(content), Button::new("Open a window"));
    #[cfg(feature = "native")]
    return Box::new(WindowsDemo { open, opened: 0 });
    #[cfg(not(feature = "native"))]
    {
        kit.tree.set_enabled(open, false);
        kit.tree.insert(
            Some(content),
            Label::new("The Web build draws into a single canvas.")
                .font_size(12.0)
                .color(Role::Muted),
        );
        Box::new(WindowsDemo {})
    }
}

use ui_layout::style::{Size as LayoutSize, length};
use ui_layout::{MeasureInput, Style};
use ui_scene::{Border, Color, Direction, Point, Rect, Size};
use ui_text::{TextAlign, TextStore, TextStyle};

use super::appear::{Appear, shadow};
use super::{CachedText, Icon};
use crate::event::{Event, Key, PointerButton};
use crate::overlay::{OverlayOptions, Placement};
use crate::theme::{Corner, Role};
use crate::tree::{UiTree, WidgetId};
use crate::widget::{EventCx, MeasureCx, PaintCx, Widget};

const WIDTH: f32 = 260.0;
const PAD: f32 = 18.0;
const ICON: f32 = 56.0;
const BUTTON_H: f32 = 28.0;
const BUTTON_GAP: f32 = 8.0;
const CHECK: f32 = 14.0;
const RADIUS: f32 = 12.0;
/// Seconds.
const APPEAR: f64 = 0.18;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlertStyle {
    #[default]
    Informational,
    Warning,
    /// A caution triangle with the app's icon as a badge: the action may destroy data.
    Critical,
}

/// Emitted (with the alert as the source) when the user answers: the index of the
/// button, and whether the suppression checkbox was checked. The alert then closes
/// itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlertResponse {
    pub button: usize,
    pub suppressed: bool,
}

/// A modal alert (`NSAlert`), drawn as one widget. The first button is the default; a
/// button titled "Cancel" (or the one given to [`Alert::cancel_button`]) answers Escape
/// and ⌘. Emits [`AlertResponse`].
#[derive(Debug)]
pub struct Alert {
    message: String,
    informative: String,
    style: AlertStyle,
    icon: Option<Icon>,
    buttons: Vec<String>,
    cancel: Option<usize>,
    suppression: Option<(String, bool)>,
    sheet: bool,
    /// Inside an inner window: the distance from the window's top to hang from.
    in_window: Option<f32>,

    /// Index over the controls: the buttons, then the checkbox.
    focus: usize,
    pressed: Option<usize>,
    hovered: Option<usize>,
    answered: bool,
    appear: Appear,

    message_text: CachedText,
    informative_text: CachedText,
    suppression_text: CachedText,
    button_texts: Vec<CachedText>,
    layout: Layout,
}

#[derive(Debug, Default, Clone)]
struct Layout {
    icon: Rect,
    message: Rect,
    informative: Rect,
    checkbox: Rect,
    buttons: Vec<Rect>,
    height: f32,
}

impl Alert {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            informative: String::new(),
            style: AlertStyle::default(),
            icon: None,
            buttons: Vec::new(),
            cancel: None,
            suppression: None,
            sheet: false,
            in_window: None,
            focus: 0,
            pressed: None,
            hovered: None,
            answered: false,
            appear: Appear::new(APPEAR),
            message_text: CachedText::default(),
            informative_text: CachedText::default(),
            suppression_text: CachedText::default(),
            button_texts: Vec::new(),
            layout: Layout::default(),
        }
    }

    pub fn informative_text(mut self, text: impl Into<String>) -> Self {
        self.informative = text.into();
        self
    }

    pub fn style(mut self, style: AlertStyle) -> Self {
        self.style = style;
        self
    }

    /// Vector icons are drawn in the accent color.
    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Add a button. The first one added is the default. Without buttons the alert has
    /// a single "OK".
    pub fn button(mut self, label: impl Into<String>) -> Self {
        self.buttons.push(label.into());
        self
    }

    /// Defaults to the button titled "Cancel".
    pub fn cancel_button(mut self, index: usize) -> Self {
        self.cancel = Some(index);
        self
    }

    pub fn suppression(mut self, label: impl Into<String>) -> Self {
        self.suppression = Some((label.into(), false));
        self
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn buttons(&self) -> &[String] {
        &self.buttons
    }

    pub fn is_suppressed(&self) -> bool {
        self.suppression.as_ref().is_some_and(|(_, on)| *on)
    }

    /// Buttons first, then the checkbox.
    pub fn focused_control(&self) -> usize {
        self.focus
    }

    pub fn open(self, tree: &mut UiTree) -> WidgetId {
        open_modal(tree, self, Placement::Center)
    }

    /// `top` is the bottom edge of the title bar or toolbar the sheet hangs from.
    pub fn open_as_sheet(mut self, tree: &mut UiTree, top: f32) -> WidgetId {
        self.sheet = true;
        open_modal(tree, self, Placement::Sheet { top })
    }

    /// Make it a sheet of an inner window: a child of the window, hanging `top` below
    /// its top edge, modal within it. Used by
    /// [`crate::widgets::WindowManager::present_alert`].
    pub(crate) fn in_window(mut self, top: f32) -> Self {
        self.sheet = true;
        self.in_window = Some(top);
        self.with_default_button();
        self
    }

    fn with_default_button(&mut self) {
        if self.buttons.is_empty() {
            self.buttons.push("OK".into());
        }
    }

    fn cancel_index(&self) -> Option<usize> {
        self.cancel
            .or_else(|| self.buttons.iter().position(|b| b == "Cancel"))
            .filter(|&i| i < self.buttons.len())
    }

    fn controls(&self) -> usize {
        self.buttons.len() + usize::from(self.suppression.is_some())
    }

    fn control_rect(&self, index: usize) -> Rect {
        match self.layout.buttons.get(index) {
            Some(&rect) => rect,
            None => self.layout.checkbox,
        }
    }

    fn control_at(&self, p: Point) -> Option<usize> {
        (0..self.controls()).find(|&i| {
            let r = self.control_rect(i);
            let r = if i == self.buttons.len() {
                Rect::new(r.x, r.y - 3.0, WIDTH - PAD - r.x, r.height + 6.0)
            } else {
                r
            };
            r.contains(p)
        })
    }

    fn activate(&mut self, cx: &mut EventCx, index: usize) {
        if index < self.buttons.len() {
            self.answer(cx, index);
        } else if let Some((_, on)) = &mut self.suppression {
            *on = !*on;
            self.focus = index;
            cx.request_paint();
        }
    }

    fn answer(&mut self, cx: &mut EventCx, button: usize) {
        if self.answered {
            return;
        }
        self.answered = true;
        cx.emit(AlertResponse {
            button,
            suppressed: self.is_suppressed(),
        });
        self.appear.close(cx.now());
        cx.request_animation_frame();
    }

    /// 0 when fully shown.
    fn slide(&self) -> f32 {
        if self.sheet {
            -(1.0 - self.appear.value()) * self.layout.height
        } else {
            0.0
        }
    }

    /// Two buttons go side by side when each label fits in half the width. Valid only
    /// after the labels are shaped.
    fn side_by_side(&self) -> bool {
        let half = (WIDTH - 2.0 * PAD - BUTTON_GAP) / 2.0;
        self.button_texts.len() == 2
            && self
                .button_texts
                .iter()
                .all(|t| t.size().width + 20.0 <= half)
    }

    fn paint_icon(&self, cx: &mut PaintCx, rect: Rect) {
        if let Some(icon) = &self.icon {
            let accent = cx.color(Role::Accent);
            icon.paint(cx, rect, accent);
            return;
        }
        if self.style == AlertStyle::Critical {
            caution(cx, rect);
            let badge = Rect::new(
                rect.right() - rect.width * 0.45,
                rect.bottom() - rect.height * 0.45,
                rect.width * 0.45,
                rect.height * 0.45,
            );
            app_icon(cx, badge);
        } else {
            app_icon(cx, rect);
        }
    }
}

fn open_modal(tree: &mut UiTree, mut alert: Alert, placement: Placement) -> WidgetId {
    alert.with_default_button();
    let id = tree.open_overlay(alert, placement, OverlayOptions::MODAL);
    tree.set_focus(Some(id));
    tree.update::<Alert, _>(id, |_, cx| cx.request_animation_frame());
    id
}

/// Each line centered, as macOS sets alert text.
fn message_style() -> TextStyle {
    TextStyle::new(14.0).weight(700).align(TextAlign::Center)
}

fn informative_style() -> TextStyle {
    TextStyle::new(12.0).align(TextAlign::Center)
}

fn suppression_style() -> TextStyle {
    TextStyle::new(12.0)
}

fn button_style() -> TextStyle {
    TextStyle::new(13.0).weight(500)
}

fn app_icon(cx: &mut PaintCx, rect: Rect) {
    let start = cx.color(Role::Accent);
    let end = cx.color(Role::AccentPressed);
    cx.gradient(rect, rect.width * 0.22, start, end, true);
    let ink = cx.color(Role::OnAccent);
    let center = Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    cx.arc(center, rect.width * 0.26, rect.width * 0.07, 1.0, ink);
    cx.arc(center, rect.width * 0.04, rect.width * 0.08, 1.0, ink);
}

fn caution(cx: &mut PaintCx, rect: Rect) {
    let yellow = Color::hex(0xFAC72E);
    cx.triangle(rect, Direction::Up, yellow);
    let ink = Color::hex(0x40330D);
    let x = rect.x + rect.width / 2.0;
    let w = rect.width * 0.09;
    cx.line(
        Point::new(x, rect.y + rect.height * 0.38),
        Point::new(x, rect.y + rect.height * 0.66),
        w,
        ink,
    );
    cx.arc(
        Point::new(x, rect.y + rect.height * 0.8),
        w * 0.1,
        w,
        1.0,
        ink,
    );
}

impl Widget for Alert {
    fn style(&self) -> Style {
        let (position, inset, margin) = super::sheet::window_modal_position(self.in_window);
        Style {
            position,
            inset,
            margin,
            size: LayoutSize {
                width: length(WIDTH),
                height: ui_layout::style::auto(),
            },
            ..Default::default()
        }
    }

    fn measures(&self) -> bool {
        true
    }

    fn measure(&mut self, cx: &mut MeasureCx, _input: MeasureInput) -> Size {
        self.with_default_button();
        let inner = WIDTH - 2.0 * PAD;
        let mut y = PAD + 4.0;
        let mut layout = Layout {
            icon: Rect::new((WIDTH - ICON) / 2.0, y, ICON, ICON),
            ..Default::default()
        };
        y += ICON + 12.0;
        let size = self
            .message_text
            .shape(cx.text, &self.message, message_style(), Some(inner));
        layout.message = Rect::new((WIDTH - size.width) / 2.0, y, size.width, size.height);
        y += size.height;
        if !self.informative.is_empty() {
            y += 6.0;
            let size = self.informative_text.shape(
                cx.text,
                &self.informative,
                informative_style(),
                Some(inner),
            );
            layout.informative = Rect::new((WIDTH - size.width) / 2.0, y, size.width, size.height);
            y += size.height;
        }
        if let Some((label, _)) = &self.suppression {
            y += 14.0;
            let size = self.suppression_text.shape(
                cx.text,
                label,
                suppression_style(),
                Some(inner - CHECK - 6.0),
            );
            layout.checkbox = Rect::new(PAD, y + 1.0, CHECK, CHECK);
            y += size.height.max(CHECK);
        }
        y += 16.0;
        self.button_texts
            .resize_with(self.buttons.len(), CachedText::default);
        for (text, label) in self.button_texts.iter_mut().zip(&self.buttons) {
            text.shape(cx.text, label, button_style(), None);
        }
        if self.side_by_side() {
            let half = (inner - BUTTON_GAP) / 2.0;
            // The default (first) button on the right.
            layout.buttons = vec![
                Rect::new(PAD + half + BUTTON_GAP, y, half, BUTTON_H),
                Rect::new(PAD, y, half, BUTTON_H),
            ];
            y += BUTTON_H;
        } else {
            for _ in &self.buttons {
                layout.buttons.push(Rect::new(PAD, y, inner, BUTTON_H));
                y += BUTTON_H + BUTTON_GAP;
            }
            y -= BUTTON_GAP;
        }
        y += PAD;
        layout.height = y;
        self.layout = layout;
        Size::new(WIDTH, y)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let slide = self.slide();
        if self.sheet {
            cx.push_clip(Rect::new(
                bounds.x - 20.0,
                bounds.y,
                bounds.width + 40.0,
                bounds.height + 20.0,
            ));
        }
        let panel = bounds.translate(0.0, slide);
        let radius = cx.corner(Corner::Container, RADIUS, panel.height);
        shadow(cx, panel, radius);
        let fill = cx.color(Role::Surface);
        let border = Border {
            width: 1.0,
            color: cx.color(Role::Outline),
        };
        cx.floating_surface(panel, radius, fill, border);
        let at = |r: Rect| r.translate(0.0, slide);
        self.paint_icon(cx, at(self.layout.icon));
        let ink = cx.color(Role::Text);
        if let Some(layout) = self.message_text.layout() {
            cx.text(at(self.layout.message), layout, ink);
        }
        if let Some(layout) = self.informative_text.layout() {
            cx.text(at(self.layout.informative), layout, ink);
        }
        let focused = cx.is_focused();
        let check_index = self.buttons.len();
        if let Some((_, on)) = &self.suppression {
            let rect = at(self.layout.checkbox);
            if *on {
                let accent = cx.color(Role::Accent);
                let radius = cx.corner(Corner::Inner, 3.5, rect.height);
                cx.bezel(rect, radius, accent, Border::default());
                let mark = cx.color(Role::OnAccent);
                cx.checkmark(rect, 1.8, mark);
            } else {
                let field = cx.color(Role::Background);
                let outline = Border {
                    width: 1.0,
                    color: cx.color(Role::Outline),
                };
                let radius = cx.corner(Corner::Inner, 3.5, rect.height);
                cx.bezel(rect, radius, field, outline);
            }
            if focused && self.focus == check_index {
                super::focus_ring(cx, rect, cx.corner(Corner::Inner, 3.5, rect.height));
            }
            if let Some(layout) = self.suppression_text.layout() {
                let size = self.suppression_text.size();
                let text = Rect::new(
                    rect.right() + 6.0,
                    rect.y + (CHECK - size.height) / 2.0,
                    size.width,
                    size.height,
                );
                cx.text(text, layout, ink);
            }
        }
        for (i, text) in self.button_texts.iter().enumerate() {
            let Some(&rect) = self.layout.buttons.get(i) else {
                continue;
            };
            let rect = at(rect);
            let default = i == 0;
            let (mut fill, label) = if default {
                (cx.color(Role::Accent), cx.color(Role::OnAccent))
            } else {
                (cx.color(Role::SurfaceAlt), ink)
            };
            if self.pressed == Some(i) && self.hovered == Some(i) {
                fill = super::pressed_tint(cx, fill);
            }
            let border = if default {
                Border::default()
            } else {
                Border {
                    width: 1.0,
                    color: cx.color(Role::Outline),
                }
            };
            let radius = cx.corner(Corner::Control, 7.0, rect.height);
            cx.bezel(rect, radius, fill, border);
            if focused && self.focus == i {
                super::focus_ring(cx, rect, radius);
            }
            if let Some(layout) = text.layout() {
                let size = text.size();
                let r = Rect::new(
                    rect.x + (rect.width - size.width) / 2.0,
                    rect.y + (rect.height - size.height) / 2.0,
                    size.width,
                    size.height,
                );
                cx.text(r, layout, label);
            }
        }
        if self.sheet {
            cx.pop_clip();
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        if let Event::AnimationFrame { now } = event {
            if self.appear.frame(cx, *now) {
                cx.close_overlay();
            }
            if self.sheet {
                cx.request_paint();
            } else {
                cx.request_composite();
            }
            return;
        }
        if self.answered {
            cx.set_handled();
            return;
        }
        match event {
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                cx.set_handled();
                if let Some(i) = self.control_at(cx.local(*pos)) {
                    self.pressed = Some(i);
                    self.hovered = Some(i);
                    cx.capture_pointer();
                    cx.request_paint();
                }
            }
            Event::PointerMove { pos } => {
                if self.pressed.is_some() {
                    let over = self.control_at(cx.local(*pos));
                    if over != self.hovered {
                        self.hovered = over;
                        cx.request_paint();
                    }
                }
            }
            Event::PointerUp { pos, .. } => {
                cx.set_handled();
                if let Some(pressed) = self.pressed.take() {
                    cx.release_pointer();
                    cx.request_paint();
                    if self.control_at(cx.local(*pos)) == Some(pressed) {
                        self.activate(cx, pressed);
                    }
                }
            }
            Event::KeyDown { key, modifiers } => {
                cx.set_handled();
                match key {
                    Key::Enter => self.answer(cx, 0),
                    Key::Escape => {
                        if let Some(cancel) = self.cancel_index() {
                            self.answer(cx, cancel);
                        }
                    }
                    Key::Char('.') if modifiers.command => {
                        if let Some(cancel) = self.cancel_index() {
                            self.answer(cx, cancel);
                        }
                    }
                    Key::Space => {
                        let focus = self.focus;
                        self.activate(cx, focus);
                    }
                    Key::Tab => {
                        let n = self.controls();
                        self.focus = if modifiers.shift {
                            (self.focus + n - 1) % n
                        } else {
                            (self.focus + 1) % n
                        };
                        cx.request_paint();
                    }
                    _ => {}
                }
            }
            Event::Activate => self.answer(cx, 0),
            Event::ActivateItem(i) if *i < self.controls() => self.activate(cx, *i),
            _ => {}
        }
    }

    fn opacity(&self) -> f32 {
        if self.sheet { 1.0 } else { self.appear.value() }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn modal(&self) -> bool {
        self.in_window.is_some()
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::AlertDialog)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        node.set_label(self.message.clone());
        if !self.informative.is_empty() {
            node.set_description(self.informative.clone());
        }
        node.set_modal();
    }

    fn accessibility_items(&self) -> Vec<accesskit::Node> {
        let bounds = |r: Rect| accesskit::Rect {
            x0: r.x as f64,
            y0: r.y as f64,
            x1: r.right() as f64,
            y1: r.bottom() as f64,
        };
        let mut items: Vec<accesskit::Node> = self
            .buttons
            .iter()
            .enumerate()
            .map(|(i, label)| {
                let mut node = accesskit::Node::new(accesskit::Role::Button);
                node.set_label(label.clone());
                if let Some(&r) = self.layout.buttons.get(i) {
                    node.set_bounds(bounds(r));
                }
                node.add_action(accesskit::Action::Click);
                node
            })
            .collect();
        if let Some((label, on)) = &self.suppression {
            let mut node = accesskit::Node::new(accesskit::Role::CheckBox);
            node.set_label(label.clone());
            node.set_toggled(if *on {
                accesskit::Toggled::True
            } else {
                accesskit::Toggled::False
            });
            node.set_bounds(bounds(self.layout.checkbox));
            node.add_action(accesskit::Action::Click);
            items.push(node);
        }
        items
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.message_text.release(text);
        self.informative_text.release(text);
        self.suppression_text.release(text);
        for t in &mut self.button_texts {
            t.release(text);
        }
    }
}

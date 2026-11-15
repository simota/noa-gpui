use noa_gpui_layout::{MeasureInput, Style};
use noa_gpui_scene::{Border, Color, Direction, Point, Rect, Size};
use noa_gpui_text::{TextStore, TextStyle};

use super::{CachedText, ControlSize, SelectionChanged, Toggled, focus_ring, knob_stretch};
use crate::animation::{Easing, Transition};
use crate::event::{Event, Key, PointerButton};
use crate::theme::{Corner, Role};
use crate::widget::{EventCx, MeasureCx, PaintCx, UpdateCx, Widget};

const BOX: f32 = 16.0;
const GAP: f32 = 7.0;

fn label_style() -> TextStyle {
    TextStyle::new(14.0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CheckState {
    #[default]
    Off,
    On,
    /// Some but not all of what the box stands for is on (shown as a dash). Clicking a
    /// mixed box turns it on. Set it with [`Checkbox::set_state`].
    Mixed,
}

/// Emitted by a [`Checkbox`] after the user changes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckboxChanged(pub CheckState);

/// A checkbox with a label.
#[derive(Debug)]
pub struct Checkbox {
    label: String,
    state: CheckState,
    size: ControlSize,
    shaped: CachedText,
    pressed: bool,
}

impl Checkbox {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            state: CheckState::Off,
            size: ControlSize::Regular,
            shaped: CachedText::default(),
            pressed: false,
        }
    }

    pub fn control_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    fn bx(&self) -> f32 {
        BOX * self.size.scale()
    }

    fn gap(&self) -> f32 {
        GAP * self.size.scale()
    }

    fn label_style(&self) -> TextStyle {
        TextStyle::new(self.size.font(14.0))
    }

    pub fn checked(mut self, on: bool) -> Self {
        self.state = if on { CheckState::On } else { CheckState::Off };
        self
    }

    pub fn state(&self) -> CheckState {
        self.state
    }

    pub fn set_state(&mut self, cx: &mut UpdateCx, state: CheckState) {
        if state != self.state {
            self.state = state;
            cx.request_paint();
        }
    }

    fn toggle(&mut self, cx: &mut EventCx) {
        self.state = match self.state {
            CheckState::On => CheckState::Off,
            CheckState::Off | CheckState::Mixed => CheckState::On,
        };
        cx.emit(CheckboxChanged(self.state));
        cx.request_paint();
        cx.set_handled();
    }
}

/// The box of a checkbox in `b` (a square) with its mark, for a [`Checkbox`] and the
/// checkboxes a table draws. Returns the radius of its corners.
pub(crate) fn paint_check_box(
    cx: &mut PaintCx,
    b: Rect,
    state: CheckState,
    pressed: bool,
    scale: f32,
) -> f32 {
    let (fill, border) = if state == CheckState::Off {
        (cx.color(Role::Surface), Some(cx.color(Role::Outline)))
    } else {
        (cx.color(Role::Accent), None)
    };
    let fill = if pressed {
        super::pressed_tint(cx, fill)
    } else {
        fill
    };
    let border = border.map_or(Border::default(), |color| Border { width: 1.0, color });
    let radius = cx.corner(Corner::Inner, 4.0, b.height);
    cx.bezel(b, radius, fill, border);
    let mark = cx.color(Role::OnAccent);
    match state {
        CheckState::On => cx.checkmark(
            Rect::new(
                b.x + b.width * 0.19,
                b.y + b.width * 0.19,
                b.width * 0.62,
                b.width * 0.62,
            ),
            2.0 * scale,
            mark,
        ),
        CheckState::Mixed => cx.line(
            Point::new(b.x + 4.5, b.y + b.height / 2.0),
            Point::new(b.right() - 4.5, b.y + b.height / 2.0),
            2.0,
            mark,
        ),
        CheckState::Off => {}
    }
    radius
}

impl Widget for Checkbox {
    fn style(&self) -> Style {
        Style {
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn measures(&self) -> bool {
        true
    }

    fn measure(&mut self, cx: &mut MeasureCx, _input: MeasureInput) -> Size {
        let style = self.label_style();
        let text = self.shaped.shape(cx.text, &self.label, style, None);
        Size::new(
            self.bx() + self.gap() + text.width,
            text.height.max(self.bx()),
        )
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let height = cx.size().height;
        let b = Rect::new(0.0, (height - self.bx()) / 2.0, self.bx(), self.bx());
        let radius = paint_check_box(cx, b, self.state, self.pressed, self.size.scale());
        if let Some(layout) = self.shaped.layout() {
            let size = self.shaped.size();
            let ink = cx.color(Role::Text);
            cx.text(
                Rect::new(
                    self.bx() + self.gap(),
                    (height - size.height) / 2.0,
                    size.width,
                    size.height,
                ),
                layout,
                ink,
            );
        }
        if cx.is_focused() {
            focus_ring(cx, b, radius);
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            Event::PointerDown {
                button: PointerButton::Primary,
                ..
            } => {
                self.pressed = true;
                cx.capture_pointer();
                cx.request_paint();
                cx.set_handled();
            }
            Event::PointerUp { pos, .. } if self.pressed => {
                self.pressed = false;
                cx.release_pointer();
                cx.request_paint();
                if cx.contains(*pos) {
                    self.toggle(cx);
                }
            }
            Event::KeyDown {
                key: Key::Space, ..
            }
            | Event::Activate => self.toggle(cx),
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::CheckBox)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        node.set_label(self.label.clone());
        node.set_toggled(match self.state {
            CheckState::Off => accesskit::Toggled::False,
            CheckState::On => accesskit::Toggled::True,
            CheckState::Mixed => accesskit::Toggled::Mixed,
        });
        node.add_action(accesskit::Action::Click);
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.shaped.release(text);
    }
}

/// A set of radio buttons of which exactly one is selected. The arrow keys wrap, as in
/// macOS. Emits [`SelectionChanged`].
#[derive(Debug)]
pub struct RadioGroup {
    options: Vec<String>,
    selected: usize,
    horizontal: bool,
    size: ControlSize,
    shaped: Vec<CachedText>,
    rows: Vec<Rect>,
}

const ROW_GAP_V: f32 = 6.0;
const ROW_GAP_H: f32 = 18.0;

impl RadioGroup {
    pub fn new<S: Into<String>>(options: impl IntoIterator<Item = S>) -> Self {
        let options: Vec<String> = options.into_iter().map(Into::into).collect();
        let shaped = options.iter().map(|_| CachedText::default()).collect();
        Self {
            options,
            selected: 0,
            horizontal: false,
            size: ControlSize::Regular,
            shaped,
            rows: Vec::new(),
        }
    }

    pub fn control_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    fn bx(&self) -> f32 {
        BOX * self.size.scale()
    }

    fn gap(&self) -> f32 {
        GAP * self.size.scale()
    }

    fn label_style(&self) -> TextStyle {
        TextStyle::new(self.size.font(14.0))
    }

    /// Lay the options out in a row instead of a column.
    pub fn horizontal(mut self, horizontal: bool) -> Self {
        self.horizontal = horizontal;
        self
    }

    pub fn selected_index(mut self, index: usize) -> Self {
        self.selected = index.min(self.options.len().saturating_sub(1));
        self
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn set_selected(&mut self, cx: &mut UpdateCx, index: usize) {
        if index < self.options.len() && index != self.selected {
            self.selected = index;
            cx.request_paint();
        }
    }

    fn select(&mut self, cx: &mut EventCx, index: usize) {
        if index != self.selected {
            self.selected = index;
            cx.emit(SelectionChanged(index));
            cx.request_paint();
        }
        cx.set_handled();
    }
}

impl Widget for RadioGroup {
    fn style(&self) -> Style {
        Style {
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn measures(&self) -> bool {
        true
    }

    fn measure(&mut self, cx: &mut MeasureCx, _input: MeasureInput) -> Size {
        self.rows.clear();
        let (bx, gap, style) = (self.bx(), self.gap(), self.label_style());
        let (mut x, mut y) = (0.0_f32, 0.0_f32);
        let mut size = Size::default();
        for (i, option) in self.options.iter().enumerate() {
            let text = self.shaped[i].shape(cx.text, option, style, None);
            let row = Rect::new(x, y, bx + gap + text.width, text.height.max(bx) + 2.0);
            self.rows.push(row);
            size.width = size.width.max(row.right());
            size.height = size.height.max(row.bottom());
            if self.horizontal {
                x = row.right() + ROW_GAP_H;
            } else {
                y = row.bottom() + ROW_GAP_V;
            }
        }
        size
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let ink = cx.color(Role::Text);
        let (bx, gap, k) = (self.bx(), self.gap(), self.size.scale());
        for (i, row) in self.rows.iter().enumerate() {
            let circle = Rect::new(row.x, row.y + (row.height - bx) / 2.0, bx, bx);
            if i == self.selected {
                let accent = cx.color(Role::Accent);
                cx.rounded_rect(circle, bx / 2.0, accent, Border::default());
                let dot = cx.color(Role::OnAccent);
                cx.rounded_rect(
                    Rect::new(
                        circle.x + bx / 2.0 - 3.0 * k,
                        circle.y + bx / 2.0 - 3.0 * k,
                        6.0 * k,
                        6.0 * k,
                    ),
                    3.0 * k,
                    dot,
                    Border::default(),
                );
                if cx.is_focused() {
                    focus_ring(cx, circle, bx / 2.0);
                }
            } else {
                let fill = cx.color(Role::Surface);
                let outline = cx.color(Role::Outline);
                cx.rounded_rect(
                    circle,
                    bx / 2.0,
                    fill,
                    Border {
                        width: 1.0,
                        color: outline,
                    },
                );
            }
            if let Some(layout) = self.shaped[i].layout() {
                let size = self.shaped[i].size();
                let y = row.y + (row.height - size.height) / 2.0;
                cx.text(
                    Rect::new(row.x + bx + gap, y, size.width, size.height),
                    layout,
                    ink,
                );
            }
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        let n = self.options.len();
        if n == 0 {
            return;
        }
        match event {
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                let p = cx.local(*pos);
                if let Some(i) = self.rows.iter().position(|r| r.contains(p)) {
                    self.select(cx, i);
                }
                cx.set_handled();
            }
            Event::KeyDown { key, .. } => match key {
                Key::ArrowUp | Key::ArrowLeft => self.select(cx, (self.selected + n - 1) % n),
                Key::ArrowDown | Key::ArrowRight => self.select(cx, (self.selected + 1) % n),
                Key::Space => cx.set_handled(),
                _ => {}
            },
            Event::ActivateItem(i) if *i < n => self.select(cx, *i),
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::RadioGroup)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        if let Some(option) = self.options.get(self.selected) {
            node.set_value(option.clone());
        }
    }

    fn accessibility_items(&self) -> Vec<accesskit::Node> {
        self.options
            .iter()
            .zip(&self.rows)
            .enumerate()
            .map(|(i, (option, row))| {
                let mut node = accesskit::Node::new(accesskit::Role::RadioButton);
                node.set_label(option.clone());
                node.set_toggled(accesskit::Toggled::from(i == self.selected));
                node.set_bounds(accesskit::Rect {
                    x0: row.x as f64,
                    y0: row.y as f64,
                    x1: row.right() as f64,
                    y1: row.bottom() as f64,
                });
                node.add_action(accesskit::Action::Click);
                node
            })
            .collect()
    }

    fn removed(&mut self, text: &mut TextStore) {
        for cached in &mut self.shaped {
            cached.release(text);
        }
    }
}

const TRACK_W: f32 = 32.0;
const TRACK_H: f32 = 18.0;
const KNOB: f32 = 14.0;
/// Seconds for a switch's knob to slide across.
const KNOB_SLIDE: f64 = 0.15;

/// An on / off switch (`NSSwitch`). Emits [`Toggled`].
#[derive(Debug)]
pub struct Switch {
    on: bool,
    size: ControlSize,
    /// Knob position, 0 (off) to 1 (on), animated.
    position: Transition,
    shown: f32,
    pressed: bool,
}

impl Switch {
    pub fn new(on: bool) -> Self {
        let value = if on { 1.0 } else { 0.0 };
        Self {
            on,
            size: ControlSize::Regular,
            position: Transition::new(value),
            shown: value,
            pressed: false,
        }
    }

    pub fn control_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    pub fn is_on(&self) -> bool {
        self.on
    }

    /// Where the knob is drawn (0 off, 1 on), for tests of the animation.
    pub fn knob_position(&self) -> f32 {
        self.shown
    }

    pub fn set_on(&mut self, cx: &mut UpdateCx, on: bool) {
        if on != self.on {
            self.on = on;
            let target = if on { 1.0 } else { 0.0 };
            self.position
                .animate_to(target, cx.now(), cx.motion(KNOB_SLIDE), Easing::EaseOut);
            cx.request_animation_frame();
        }
    }

    fn toggle(&mut self, cx: &mut EventCx) {
        self.on = !self.on;
        let target = if self.on { 1.0 } else { 0.0 };
        self.position
            .animate_to(target, cx.now(), cx.motion(KNOB_SLIDE), Easing::EaseOut);
        cx.request_animation_frame();
        cx.emit(Toggled(self.on));
        cx.set_handled();
    }
}

impl Widget for Switch {
    fn style(&self) -> Style {
        Style {
            size: noa_gpui_layout::style::Size {
                width: noa_gpui_layout::style::length(TRACK_W * self.size.scale()),
                height: noa_gpui_layout::style::length(TRACK_H * self.size.scale()),
            },
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let k = self.size.scale();
        let (track_w, track_h, knob) = (TRACK_W * k, TRACK_H * k, KNOB * k);
        let track = cx.bounds();
        let off = cx.color(Role::Muted).with_alpha(0.35);
        let on = cx.color(Role::Accent);
        let fill = crate::animation::lerp_color(off, on, self.shown);
        let track_radius = cx.pill(track_h);
        cx.rounded_rect(track, track_radius, fill, Border::default());
        let inset = (track_h - knob) / 2.0;
        let knob_w = knob * knob_stretch(cx);
        let x = inset + (track_w - knob_w - 2.0 * inset) * self.shown;
        let knob_color = if self.pressed {
            Color::hex(0xe8e8ec)
        } else {
            Color::WHITE
        };
        let knob_fill = cx.color(knob_color);
        let knob_rect = Rect::new(x, inset, knob_w, knob);
        cx.bezel(
            knob_rect,
            cx.pill(knob),
            knob_fill,
            Border {
                width: 0.5,
                color: Color::BLACK.with_alpha(0.15),
            },
        );
        if cx.is_focused() {
            focus_ring(cx, track, track_radius);
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            Event::PointerDown {
                button: PointerButton::Primary,
                ..
            } => {
                self.pressed = true;
                cx.capture_pointer();
                cx.request_paint();
                cx.set_handled();
            }
            Event::PointerUp { pos, .. } if self.pressed => {
                self.pressed = false;
                cx.release_pointer();
                cx.request_paint();
                if cx.contains(*pos) {
                    self.toggle(cx);
                }
            }
            Event::KeyDown {
                key: Key::Space, ..
            }
            | Event::Activate => self.toggle(cx),
            Event::AnimationFrame { now } => {
                self.shown = self.position.value(*now);
                cx.request_paint();
                if !self.position.is_done(*now) {
                    cx.request_animation_frame();
                }
            }
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Switch)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        node.set_toggled(accesskit::Toggled::from(self.on));
        node.add_action(accesskit::Action::Click);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisclosureStyle {
    Triangle,
    Button,
}

/// Shows whether something is expanded and toggles it. Emits [`Toggled`] with the new
/// expanded state.
#[derive(Debug)]
pub struct Disclosure {
    style: DisclosureStyle,
    label: String,
    expanded: bool,
    shaped: CachedText,
    pressed: bool,
}

const TRIANGLE: f32 = 9.0;
const DISCLOSURE_BUTTON: f32 = 20.0;

impl Disclosure {
    pub fn triangle(label: impl Into<String>) -> Self {
        Self {
            style: DisclosureStyle::Triangle,
            label: label.into(),
            expanded: false,
            shaped: CachedText::default(),
            pressed: false,
        }
    }

    pub fn button() -> Self {
        Self {
            style: DisclosureStyle::Button,
            ..Self::triangle("")
        }
    }

    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }

    pub fn is_expanded(&self) -> bool {
        self.expanded
    }

    pub fn set_expanded(&mut self, cx: &mut UpdateCx, expanded: bool) {
        if expanded != self.expanded {
            self.expanded = expanded;
            cx.request_paint();
        }
    }

    fn toggle(&mut self, cx: &mut EventCx) {
        self.expanded = !self.expanded;
        cx.emit(Toggled(self.expanded));
        cx.request_paint();
        cx.set_handled();
    }
}

impl Widget for Disclosure {
    fn style(&self) -> Style {
        Style {
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn measures(&self) -> bool {
        true
    }

    fn measure(&mut self, cx: &mut MeasureCx, _input: MeasureInput) -> Size {
        match self.style {
            DisclosureStyle::Button => Size::new(DISCLOSURE_BUTTON, DISCLOSURE_BUTTON),
            DisclosureStyle::Triangle if self.label.is_empty() => Size::new(14.0, 18.0),
            DisclosureStyle::Triangle => {
                let text = self.shaped.shape(cx.text, &self.label, label_style(), None);
                Size::new(14.0 + 4.0 + text.width, text.height.max(18.0))
            }
        }
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        match self.style {
            DisclosureStyle::Triangle => {
                let ink = cx.color(Role::Muted);
                let (rect, direction) = if self.expanded {
                    (
                        Rect::new(2.5, (bounds.height - 6.0) / 2.0, TRIANGLE, 6.0),
                        Direction::Down,
                    )
                } else {
                    (
                        Rect::new(4.0, (bounds.height - TRIANGLE) / 2.0, 6.0, TRIANGLE),
                        Direction::Right,
                    )
                };
                cx.triangle(rect, direction, ink);
                if let Some(layout) = self.shaped.layout() {
                    let size = self.shaped.size();
                    let text = cx.color(Role::Text);
                    cx.text(
                        Rect::new(
                            18.0,
                            (bounds.height - size.height) / 2.0,
                            size.width,
                            size.height,
                        ),
                        layout,
                        text,
                    );
                }
                if cx.is_focused() {
                    focus_ring(cx, Rect::new(0.0, 0.0, 14.0, bounds.height), 4.0);
                }
            }
            DisclosureStyle::Button => {
                let fill = cx.color(Role::Surface);
                let fill = if self.pressed {
                    super::pressed_tint(cx, fill)
                } else {
                    fill
                };
                let outline = cx.color(Role::Outline);
                let radius = cx.corner(Corner::Control, 5.0, bounds.height);
                cx.bezel(
                    bounds,
                    radius,
                    fill,
                    Border {
                        width: 1.0,
                        color: outline,
                    },
                );
                let ink = cx.color(Role::Text);
                let direction = if self.expanded {
                    Direction::Up
                } else {
                    Direction::Down
                };
                cx.chevron(Rect::new(6.0, 8.0, 8.0, 4.5), direction, 1.6, ink);
                if cx.is_focused() {
                    focus_ring(cx, bounds, radius);
                }
            }
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            Event::PointerDown {
                button: PointerButton::Primary,
                ..
            } => {
                self.pressed = true;
                cx.capture_pointer();
                cx.request_paint();
                cx.set_handled();
            }
            Event::PointerUp { pos, .. } if self.pressed => {
                self.pressed = false;
                cx.release_pointer();
                cx.request_paint();
                if cx.contains(*pos) {
                    self.toggle(cx);
                }
            }
            Event::KeyDown {
                key: Key::Space, ..
            }
            | Event::Activate => self.toggle(cx),
            Event::KeyDown {
                key: Key::ArrowRight,
                ..
            } if !self.expanded => self.toggle(cx),
            Event::KeyDown {
                key: Key::ArrowLeft,
                ..
            } if self.expanded => self.toggle(cx),
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(match self.style {
            DisclosureStyle::Triangle => accesskit::Role::DisclosureTriangle,
            DisclosureStyle::Button => accesskit::Role::Button,
        })
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        if !self.label.is_empty() {
            node.set_label(self.label.clone());
        }
        node.set_expanded(self.expanded);
        node.add_action(accesskit::Action::Click);
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.shaped.release(text);
    }
}

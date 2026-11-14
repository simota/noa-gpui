use std::f32::consts::TAU;

use ui_layout::{MeasureInput, Style};
use ui_scene::{Border, Color, Point, Rect, Size};

use super::{ControlSize, ValueChanged, focus_ring, knob_stretch};
use crate::event::{Event, Key, PointerButton};
use crate::theme::Role;
use crate::widget::{EventCx, MeasureCx, PaintCx, UpdateCx, Widget};

const KNOB: f32 = 18.0;
const TRACK: f32 = 4.0;
const TICK_SPACE: f32 = 8.0;
const CIRCLE: f32 = 32.0;

/// A slider that picks a value in `min..=max` (`NSSlider`). Emits [`ValueChanged`]
/// whenever the value changes, including during a drag; a [`Slider::range`] slider
/// emits [`RangeChanged`] instead.
#[derive(Debug)]
pub struct Slider {
    min: f64,
    max: f64,
    value: f64,
    step: Option<f64>,
    ticks: usize,
    only_ticks: bool,
    vertical: bool,
    circular: bool,
    length: f32,
    size: ControlSize,
    dragging: bool,
    /// The upper value of a range slider (`value` is the lower one).
    high: Option<f64>,
    high_active: bool,
}

/// Emitted by a range [`Slider`] when its lower or upper value changes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RangeChanged {
    pub low: f64,
    pub high: f64,
}

impl Slider {
    /// Horizontal; `value` is clamped.
    pub fn new(min: f64, max: f64, value: f64) -> Self {
        Self {
            min,
            max,
            value: value.clamp(min, max),
            step: None,
            ticks: 0,
            only_ticks: false,
            vertical: false,
            circular: false,
            length: 160.0,
            size: ControlSize::Regular,
            dragging: false,
            high: None,
            high_active: false,
        }
    }

    /// A slider with two knobs picking `low..=high` within `min..=max` (horizontal, or
    /// vertical with [`Slider::vertical`]; not circular).
    pub fn range(min: f64, max: f64, low: f64, high: f64) -> Self {
        let low = low.clamp(min, max);
        let mut slider = Self::new(min, max, low);
        slider.high = Some(high.clamp(low, max));
        slider
    }

    pub fn range_values(&self) -> Option<(f64, f64)> {
        self.high.map(|high| (self.value, high))
    }

    pub fn upper_knob_active(&self) -> bool {
        self.high_active
    }

    /// Set a range slider's range (clamped and ordered) without emitting.
    pub fn set_range(&mut self, cx: &mut UpdateCx, low: f64, high: f64) {
        if self.high.is_none() {
            return;
        }
        let (low, high) = (self.constrain(low.min(high)), self.constrain(low.max(high)));
        self.value = low;
        self.high = Some(high);
        cx.request_paint();
    }

    pub fn vertical(mut self, vertical: bool) -> Self {
        self.vertical = vertical;
        self
    }

    /// A round knob turned by dragging around it; the value grows clockwise from the top.
    pub fn circular(mut self) -> Self {
        self.circular = true;
        self
    }

    /// `count` evenly spaced tick marks (including both ends; at least 2 to show).
    pub fn tick_marks(mut self, count: usize) -> Self {
        self.ticks = count;
        self.constrain_values();
        self
    }

    pub fn snap_to_ticks(mut self, snap: bool) -> Self {
        self.only_ticks = snap;
        self.constrain_values();
        self
    }

    /// The keyboard step (default: a twentieth of the range).
    pub fn step(mut self, step: f64) -> Self {
        self.step = Some(step);
        self
    }

    /// Length of the track in pixels (ignored when circular).
    pub fn length(mut self, length: f32) -> Self {
        self.length = length;
        self
    }

    pub fn control_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    pub fn value(&self) -> f64 {
        self.value
    }

    /// Set the value (clamped, and snapped when restricted to ticks) without emitting.
    /// In a range slider, this sets the lower knob without passing the upper one.
    pub fn set_value(&mut self, cx: &mut UpdateCx, value: f64) {
        let value = self.constrain(value);
        self.value = self.high.map_or(value, |high| value.min(high));
        cx.request_paint();
    }

    fn constrain_values(&mut self) {
        self.value = self.constrain(self.value);
        self.high = self.high.map(|high| self.constrain(high));
    }

    fn snapping(&self) -> bool {
        self.only_ticks && self.ticks >= 2
    }

    fn constrain(&self, value: f64) -> f64 {
        let value = value.clamp(self.min, self.max);
        if !self.snapping() || self.max <= self.min {
            return value;
        }
        let intervals = (self.ticks - 1) as f64;
        let t = ((value - self.min) / (self.max - self.min) * intervals).round() / intervals;
        self.min + t * (self.max - self.min)
    }

    /// Position of the value in `0..=1`.
    fn fraction(&self) -> f32 {
        if self.max <= self.min {
            0.0
        } else {
            ((self.value - self.min) / (self.max - self.min)) as f32
        }
    }

    fn keyboard_step(&self) -> f64 {
        if self.snapping() {
            (self.max - self.min) / (self.ticks - 1) as f64
        } else {
            self.step.unwrap_or((self.max - self.min) / 20.0)
        }
    }

    fn change(&mut self, cx: &mut EventCx, value: f64) {
        let value = self.constrain(value);
        if let Some(high) = self.high {
            // The moving knob stops at the other one.
            let (low, high) = if self.high_active {
                (self.value, value.max(self.value))
            } else {
                (value.min(high), high)
            };
            if low != self.value || Some(high) != self.high {
                self.value = low;
                self.high = Some(high);
                cx.emit(RangeChanged { low, high });
                cx.request_paint();
            }
            return;
        }
        if value != self.value {
            self.value = value;
            cx.emit(ValueChanged(value));
            cx.request_paint();
        }
    }

    fn key_value(&self) -> f64 {
        match self.high {
            Some(high) if self.high_active => high,
            _ => self.value,
        }
    }

    fn fraction_of(&self, value: f64) -> f32 {
        if self.max <= self.min {
            0.0
        } else {
            ((value - self.min) / (self.max - self.min)) as f32
        }
    }

    fn knob(&self) -> f32 {
        KNOB * self.size.scale()
    }

    fn diameter(&self) -> f32 {
        CIRCLE * self.size.scale()
    }

    /// The value under a pointer position (widget coordinates).
    fn value_at(&self, p: Point, size: Size) -> f64 {
        let t = if self.circular {
            let (dx, dy) = (p.x - size.width / 2.0, p.y - size.height / 2.0);
            // Clockwise from the top.
            dx.atan2(-dy).rem_euclid(TAU) / TAU
        } else {
            let knob = self.knob();
            let travel = self.length - knob;
            if self.vertical {
                1.0 - (p.y - knob / 2.0) / travel
            } else {
                (p.x - knob / 2.0) / travel
            }
        };
        self.min + t.clamp(0.0, 1.0) as f64 * (self.max - self.min)
    }

    fn paint_linear(&self, cx: &mut PaintCx) {
        let k = self.size.scale();
        let knob = self.knob();
        let track = TRACK * k;
        let travel = self.length - knob;
        let t = self.fraction();
        let t_high = self.high.map(|h| self.fraction_of(h));
        let along = |f: f32| {
            knob / 2.0
                + if self.vertical {
                    (1.0 - f) * travel
                } else {
                    f * travel
                }
        };
        let groove = cx.color(Role::Muted).with_alpha(0.3);
        let accent = cx.color(Role::Accent);
        let mid = knob / 2.0;
        let (full, filled) = if self.vertical {
            let x = mid - track / 2.0;
            (
                Rect::new(x, mid - track / 2.0, track, travel + track),
                Rect::new(x, along(t), track, travel - along(t) + mid + track / 2.0),
            )
        } else {
            let y = mid - track / 2.0;
            (
                Rect::new(mid - track / 2.0, y, travel + track, track),
                Rect::new(mid - track / 2.0, y, along(t) - mid + track / 2.0, track),
            )
        };
        let track_radius = cx.pill(track);
        cx.rounded_rect(full, track_radius, groove, Border::default());
        if let Some(th) = t_high {
            let (a, b) = (along(t), along(th));
            let between = if self.vertical {
                // The upper value is nearer the top.
                Rect::new(mid - track / 2.0, b, track, (a - b).max(0.0))
            } else {
                Rect::new(a, mid - track / 2.0, (b - a).max(0.0), track)
            };
            cx.rounded_rect(between, track_radius, accent, Border::default());
        } else if !self.snapping() {
            cx.rounded_rect(filled, track_radius, accent, Border::default());
        }
        if self.ticks >= 2 {
            let tick = cx.color(Role::Muted);
            for i in 0..self.ticks {
                let p = along(i as f32 / (self.ticks - 1) as f32);
                let rect = if self.vertical {
                    Rect::new(knob + 2.0 * k, p - 0.5, 5.0 * k, 1.0)
                } else {
                    Rect::new(p - 0.5, knob + 2.0 * k, 1.0, 5.0 * k)
                };
                cx.fill_rect(rect, tick);
            }
        }
        let long = knob * knob_stretch(cx);
        let knob_radius = cx.pill(knob);
        let knobs = [
            (t, !self.high_active),
            (t_high.unwrap_or(t), self.high_active),
        ];
        let count = if t_high.is_some() { 2 } else { 1 };
        for &(f, active) in &knobs[..count] {
            let center = along(f);
            let rect = if self.vertical {
                Rect::new(0.0, center - long / 2.0, knob, long)
            } else {
                Rect::new(center - long / 2.0, 0.0, long, knob)
            };
            let fill = cx.color(if self.dragging && active {
                Color::hex(0xe8e8ec)
            } else {
                Color::WHITE
            });
            cx.bezel(
                rect,
                knob_radius,
                fill,
                Border {
                    width: 0.5,
                    color: Color::BLACK.with_alpha(0.25),
                },
            );
            if cx.is_focused() && (active || count == 1) {
                focus_ring(cx, rect, knob_radius);
            }
        }
    }

    fn paint_circular(&self, cx: &mut PaintCx) {
        let k = self.size.scale();
        let d = self.diameter();
        let body = Rect::new(0.0, 0.0, d, d);
        let fill = cx.color(Role::Surface);
        let outline = cx.color(Role::Outline);
        cx.bezel(
            body,
            d / 2.0,
            fill,
            Border {
                width: 1.0,
                color: outline,
            },
        );
        let c = d / 2.0;
        let at = |t: f32, r: f32| {
            let a = t * TAU;
            Point::new(c + r * a.sin(), c - r * a.cos())
        };
        if self.ticks >= 2 {
            let tick = cx.color(Role::Muted);
            // A full turn: the last tick coincides with the first.
            for i in 0..self.ticks - 1 {
                let t = i as f32 / (self.ticks - 1) as f32;
                cx.line(at(t, c - 2.0 * k), at(t, c - 5.0 * k), 1.0, tick);
            }
        }
        let dot = 5.0 * k;
        let p = at(self.fraction(), c - 9.0 * k);
        let accent = cx.color(Role::Accent);
        cx.rounded_rect(
            Rect::new(p.x - dot / 2.0, p.y - dot / 2.0, dot, dot),
            dot / 2.0,
            accent,
            Border::default(),
        );
        if cx.is_focused() {
            focus_ring(cx, body, d / 2.0);
        }
    }
}

impl Widget for Slider {
    fn style(&self) -> Style {
        Style {
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn measures(&self) -> bool {
        true
    }

    fn measure(&mut self, _cx: &mut MeasureCx, _input: MeasureInput) -> Size {
        if self.circular {
            let d = self.diameter();
            return Size::new(d, d);
        }
        let k = self.size.scale();
        let thickness = self.knob() + if self.ticks >= 2 { TICK_SPACE * k } else { 0.0 };
        if self.vertical {
            Size::new(thickness, self.length)
        } else {
            Size::new(self.length, thickness)
        }
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        if self.circular {
            self.paint_circular(cx);
        } else {
            self.paint_linear(cx);
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                self.dragging = true;
                cx.capture_pointer();
                let value = self.value_at(cx.local(*pos), cx.size());
                if let Some(high) = self.high {
                    // The nearer knob; past the upper one (or on it, to the right),
                    // the upper one.
                    let (dl, dh) = ((value - self.value).abs(), (value - high).abs());
                    self.high_active = dh < dl || (dh == dl && value >= high);
                }
                self.change(cx, value);
                cx.request_paint();
                cx.set_handled();
            }
            Event::PointerMove { pos } if self.dragging => {
                let value = self.value_at(cx.local(*pos), cx.size());
                self.change(cx, value);
                cx.set_handled();
            }
            Event::PointerUp { .. } if self.dragging => {
                self.dragging = false;
                cx.release_pointer();
                cx.request_paint();
                cx.set_handled();
            }
            Event::KeyDown {
                key: Key::Tab,
                modifiers,
            } if self.high.is_some() && self.high_active == modifiers.shift => {
                self.high_active = !modifiers.shift;
                cx.request_paint();
                cx.set_handled();
            }
            Event::KeyDown { key, .. } => {
                let step = self.keyboard_step();
                let page = (self.max - self.min) / 10.0;
                let current = self.key_value();
                let value = match key {
                    Key::ArrowRight | Key::ArrowUp => current + step,
                    Key::ArrowLeft | Key::ArrowDown => current - step,
                    Key::PageUp => current + page,
                    Key::PageDown => current - page,
                    Key::Home => self.min,
                    Key::End => self.max,
                    _ => return,
                };
                self.change(cx, value);
                cx.set_handled();
            }
            Event::Increment => {
                let value = self.key_value() + self.keyboard_step();
                self.change(cx, value);
            }
            Event::Decrement => {
                let value = self.key_value() - self.keyboard_step();
                self.change(cx, value);
            }
            // A press chose its knob already.
            Event::FocusChanged(true) if self.high.is_some() && !self.dragging => {
                // Entering backwards (Shift+Tab) lands on the upper knob.
                self.high_active = cx.modifiers().shift;
                cx.request_paint();
            }
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Slider)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        if let Some(high) = self.high {
            node.set_value(format!("{} to {}", self.value, high));
        }
        node.set_numeric_value(self.key_value());
        node.set_min_numeric_value(self.min);
        node.set_max_numeric_value(self.max);
        node.set_numeric_value_step(self.keyboard_step());
        if !self.circular {
            node.set_orientation(if self.vertical {
                accesskit::Orientation::Vertical
            } else {
                accesskit::Orientation::Horizontal
            });
        }
        node.add_action(accesskit::Action::Increment);
        node.add_action(accesskit::Action::Decrement);
    }
}

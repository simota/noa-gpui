use noa_gpui_layout::{MeasureInput, Style};
use noa_gpui_scene::{Border, Direction, Rect, Size};

use super::{ControlSize, ValueChanged, focus_ring, pressed_tint};
use crate::event::{Event, Key, PointerButton};
use crate::theme::{Corner, Role};
use crate::widget::{EventCx, MeasureCx, PaintCx, UpdateCx, Widget};

const WIDTH: f32 = 16.0;
const HEIGHT: f32 = 24.0;
/// Seconds before a held arrow starts repeating, and between repeats.
pub const REPEAT_DELAY: f64 = 0.4;
pub const REPEAT_INTERVAL: f64 = 0.08;

/// Up and down arrows that change a number in `min..=max` by `increment` (`NSStepper`).
/// Emits [`ValueChanged`].
#[derive(Debug)]
pub struct Stepper {
    min: f64,
    max: f64,
    value: f64,
    increment: f64,
    wraps: bool,
    autorepeat: bool,
    size: ControlSize,
    /// The arrow held down (`true` = up) and whether the pointer is still over it.
    pressed: Option<(bool, bool)>,
    next_repeat: f64,
}

impl Stepper {
    pub fn new(min: f64, max: f64, value: f64) -> Self {
        Self {
            min,
            max,
            value: value.clamp(min, max),
            increment: 1.0,
            wraps: false,
            autorepeat: true,
            size: ControlSize::Regular,
            pressed: None,
            next_repeat: 0.0,
        }
    }

    pub fn increment(mut self, increment: f64) -> Self {
        self.increment = increment;
        self
    }

    pub fn wraps(mut self, wraps: bool) -> Self {
        self.wraps = wraps;
        self
    }

    /// Repeat while an arrow is held (on by default).
    pub fn autorepeat(mut self, autorepeat: bool) -> Self {
        self.autorepeat = autorepeat;
        self
    }

    pub fn control_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    pub fn value(&self) -> f64 {
        self.value
    }

    /// Set the value (clamped) without emitting.
    pub fn set_value(&mut self, cx: &mut UpdateCx, value: f64) {
        self.value = value.clamp(self.min, self.max);
        cx.request_paint();
    }

    fn step(&mut self, cx: &mut EventCx, up: bool) {
        let delta = if up { self.increment } else { -self.increment };
        let mut value = self.value + delta;
        if self.wraps {
            if value > self.max + 1e-9 {
                value = self.min;
            } else if value < self.min - 1e-9 {
                value = self.max;
            }
        }
        // Drop float noise such as 0.1 + 0.2 = 0.30000000000000004.
        let value = ((value * 1e9).round() / 1e9).clamp(self.min, self.max);
        if value != self.value {
            self.value = value;
            cx.emit(ValueChanged(value));
        }
        cx.request_paint();
    }

    fn half(&self) -> f32 {
        HEIGHT * self.size.scale() / 2.0
    }
}

impl Widget for Stepper {
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
        let k = self.size.scale();
        Size::new(WIDTH * k, HEIGHT * k)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let k = self.size.scale();
        let surface = cx.color(Role::Surface);
        let outline = cx.color(Role::Outline);
        let radius = cx.corner(Corner::Inner, 5.0 * k, bounds.width);
        cx.bezel(
            bounds,
            radius,
            surface,
            Border {
                width: 1.0,
                color: outline,
            },
        );
        let half = self.half();
        if let Some((up, true)) = self.pressed {
            let tint = pressed_tint(cx, surface);
            let y = if up { 1.0 } else { half };
            cx.rounded_rect(
                Rect::new(1.0, y, bounds.width - 2.0, half - 1.0),
                (radius - 1.0).max(0.0),
                tint,
                Border::default(),
            );
        }
        cx.fill_rect(Rect::new(3.0, half - 0.5, bounds.width - 6.0, 1.0), outline);
        let ink = cx.color(Role::Text);
        let (w, h) = (7.0 * k, 3.5 * k);
        let x = (bounds.width - w) / 2.0;
        let at_max = !self.wraps && self.value >= self.max;
        let at_min = !self.wraps && self.value <= self.min;
        let dim = |at_end: bool| {
            if at_end {
                ink.with_alpha(ink.a * 0.35)
            } else {
                ink
            }
        };
        cx.chevron(
            Rect::new(x, (half - h) / 2.0, w, h),
            Direction::Up,
            1.5,
            dim(at_max),
        );
        cx.chevron(
            Rect::new(x, half + (half - h) / 2.0, w, h),
            Direction::Down,
            1.5,
            dim(at_min),
        );
        if cx.is_focused() {
            focus_ring(cx, bounds, radius);
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                let up = cx.local(*pos).y < self.half();
                self.pressed = Some((up, true));
                self.step(cx, up);
                if self.autorepeat {
                    self.next_repeat = cx.now() + REPEAT_DELAY;
                    cx.request_animation_frame();
                }
                cx.capture_pointer();
                cx.set_handled();
            }
            Event::PointerMove { pos } => {
                if let Some((up, over)) = self.pressed {
                    let p = cx.local(*pos);
                    let now_over = cx.contains(*pos) && (p.y < self.half()) == up;
                    if now_over != over {
                        self.pressed = Some((up, now_over));
                        cx.request_paint();
                    }
                }
            }
            Event::PointerUp { .. } if self.pressed.is_some() => {
                self.pressed = None;
                cx.release_pointer();
                cx.request_paint();
                cx.set_handled();
            }
            Event::AnimationFrame { now } => {
                let Some((up, over)) = self.pressed else {
                    return;
                };
                if over && *now >= self.next_repeat {
                    self.step(cx, up);
                    self.next_repeat = now + REPEAT_INTERVAL;
                }
                cx.request_animation_frame();
            }
            Event::KeyDown {
                key: key @ (Key::ArrowUp | Key::ArrowDown),
                ..
            } => {
                self.step(cx, *key == Key::ArrowUp);
                cx.set_handled();
            }
            Event::Increment => self.step(cx, true),
            Event::Decrement => self.step(cx, false),
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::SpinButton)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        node.set_numeric_value(self.value);
        node.set_min_numeric_value(self.min);
        node.set_max_numeric_value(self.max);
        node.set_numeric_value_step(self.increment);
        node.add_action(accesskit::Action::Increment);
        node.add_action(accesskit::Action::Decrement);
    }
}

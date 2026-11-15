use noa_gpui_layout::{MeasureInput, Style};
use noa_gpui_scene::{Border, Color, Rect, Size};

use super::{ValueChanged, focus_ring};
use crate::event::{Event, Key, PointerButton};
use crate::theme::Role;
use crate::widget::{EventCx, MeasureCx, PaintCx, UpdateCx, Widget};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelStyle {
    ContinuousCapacity,
    DiscreteCapacity,
    Rating,
    Relevancy,
}

pub const LEVEL_NORMAL: Color = Color::hex(0x34c759);
pub const LEVEL_WARNING: Color = Color::hex(0xffcc00);
pub const LEVEL_CRITICAL: Color = Color::hex(0xff3b30);

const HEIGHT: f32 = 18.0;
const STAR: f32 = 16.0;
const STAR_GAP: f32 = 3.0;

/// A level in `min..=max` (`NSLevelIndicator`). Emits [`ValueChanged`] when
/// [`LevelIndicator::editable`].
#[derive(Debug)]
pub struct LevelIndicator {
    style: LevelStyle,
    min: f64,
    max: f64,
    value: f64,
    warning: Option<f64>,
    critical: Option<f64>,
    editable: bool,
    width: f32,
    dragging: bool,
}

impl LevelIndicator {
    pub fn new(style: LevelStyle, min: f64, max: f64, value: f64) -> Self {
        Self {
            style,
            min,
            max,
            value: value.clamp(min, max),
            warning: None,
            critical: None,
            editable: false,
            width: 160.0,
            dragging: false,
        }
    }

    /// The value from which (or, if the critical level is lower, up to which) the
    /// capacity color turns yellow.
    pub fn warning(mut self, level: f64) -> Self {
        self.warning = Some(level);
        self
    }

    /// The value from which (or up to which, if lower than the warning level) the
    /// capacity color turns red.
    pub fn critical(mut self, level: f64) -> Self {
        self.critical = Some(level);
        self
    }

    pub fn editable(mut self, editable: bool) -> Self {
        self.editable = editable;
        self
    }

    /// Width of the bar, segments or stripes (a rating is sized by its stars).
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn value(&self) -> f64 {
        self.value
    }

    pub fn set_value(&mut self, cx: &mut UpdateCx, value: f64) {
        self.value = self.constrain(value);
        cx.request_paint();
    }

    pub fn color(&self) -> Color {
        let v = self.value;
        match (self.warning, self.critical) {
            // Lower is worse (for example, battery charge).
            (Some(w), Some(c)) if c < w => {
                if v <= c {
                    LEVEL_CRITICAL
                } else if v <= w {
                    LEVEL_WARNING
                } else {
                    LEVEL_NORMAL
                }
            }
            (w, c) => {
                if c.is_some_and(|c| v >= c) {
                    LEVEL_CRITICAL
                } else if w.is_some_and(|w| v >= w) {
                    LEVEL_WARNING
                } else {
                    LEVEL_NORMAL
                }
            }
        }
    }

    fn whole_units(&self) -> bool {
        matches!(
            self.style,
            LevelStyle::DiscreteCapacity | LevelStyle::Rating
        )
    }

    fn units(&self) -> usize {
        (self.max - self.min).round().clamp(0.0, 100.0) as usize
    }

    fn constrain(&self, value: f64) -> f64 {
        let value = value.clamp(self.min, self.max);
        if self.whole_units() {
            value.round()
        } else {
            value
        }
    }

    fn fraction(&self) -> f32 {
        if self.max <= self.min {
            0.0
        } else {
            ((self.value - self.min) / (self.max - self.min)) as f32
        }
    }

    fn change(&mut self, cx: &mut EventCx, value: f64) {
        let value = self.constrain(value);
        if value != self.value {
            self.value = value;
            cx.emit(ValueChanged(value));
            cx.request_paint();
        }
    }

    /// The value under `x` (widget coordinates).
    fn value_at(&self, x: f32, width: f32) -> f64 {
        let span = self.max - self.min;
        if self.style == LevelStyle::Rating {
            let pitch = STAR + STAR_GAP;
            let stars = if x < STAR_GAP {
                0.0
            } else {
                (x / pitch).floor() + 1.0
            };
            return self.min + f64::from(stars);
        }
        let t = (x / width).clamp(0.0, 1.0) as f64;
        if self.style == LevelStyle::DiscreteCapacity {
            self.min + (t * span).ceil()
        } else {
            self.min + t * span
        }
    }

    /// One unit, or a twentieth of the range for the continuous styles.
    fn step(&self) -> f64 {
        if self.whole_units() {
            1.0
        } else {
            (self.max - self.min) / 20.0
        }
    }

    fn star_rect(i: usize) -> Rect {
        Rect::new(i as f32 * (STAR + STAR_GAP), 1.0, STAR, STAR)
    }
}

impl Widget for LevelIndicator {
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
        if self.style == LevelStyle::Rating {
            let n = self.units() as f32;
            return Size::new(n * STAR + (n - 1.0).max(0.0) * STAR_GAP, HEIGHT);
        }
        Size::new(self.width, HEIGHT)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let fraction = self.fraction();
        let outline = cx.color(Role::Outline);
        let empty = cx.color(Role::Muted).with_alpha(0.2);
        let fill = cx.color(self.color());
        match self.style {
            LevelStyle::ContinuousCapacity => {
                let border = Border {
                    width: 1.0,
                    color: outline,
                };
                cx.rounded_rect(bounds, 4.0, empty, border);
                if fraction > 0.0 {
                    let inner = Rect::new(
                        1.0,
                        1.0,
                        (bounds.width - 2.0) * fraction,
                        bounds.height - 2.0,
                    );
                    cx.rounded_rect(inner, 3.0, fill, Border::default());
                }
            }
            LevelStyle::DiscreteCapacity => {
                let n = self.units().max(1);
                let gap = 2.0;
                let w = (bounds.width - gap * (n - 1) as f32) / n as f32;
                let filled = (self.value - self.min).round() as usize;
                for i in 0..n {
                    let rect = Rect::new(i as f32 * (w + gap), 0.0, w, bounds.height);
                    let color = if i < filled { fill } else { empty };
                    let border = Border {
                        width: 1.0,
                        color: outline,
                    };
                    cx.rounded_rect(rect, 2.0, color, border);
                }
            }
            LevelStyle::Rating => {
                let filled = (self.value - self.min).round() as usize;
                let star = cx.color(Role::Text);
                let muted = cx.color(Role::Muted);
                for i in 0..self.units() {
                    let rect = Self::star_rect(i);
                    if i < filled {
                        cx.star(rect, 0.0, star);
                    } else {
                        // An empty place is a small dot, as in macOS.
                        let c = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
                        cx.rounded_rect(
                            Rect::new(c.0 - 1.5, c.1 - 1.5, 3.0, 3.0),
                            1.5,
                            muted,
                            Border::default(),
                        );
                    }
                }
            }
            LevelStyle::Relevancy => {
                let ink = cx.color(Role::Muted);
                let end = bounds.width * fraction;
                let mut x = 0.0;
                while x + 1.0 <= end {
                    cx.fill_rect(Rect::new(x, 3.0, 1.0, bounds.height - 6.0), ink);
                    x += 3.0;
                }
            }
        }
        if cx.is_focused() {
            focus_ring(cx, bounds, 4.0);
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        if !self.editable {
            return;
        }
        let width = cx.size().width;
        match event {
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                self.dragging = true;
                cx.capture_pointer();
                let value = self.value_at(cx.local(*pos).x, width);
                self.change(cx, value);
                cx.set_handled();
            }
            Event::PointerMove { pos } if self.dragging => {
                let value = self.value_at(cx.local(*pos).x, width);
                self.change(cx, value);
                cx.set_handled();
            }
            Event::PointerUp { .. } if self.dragging => {
                self.dragging = false;
                cx.release_pointer();
                cx.set_handled();
            }
            Event::KeyDown { key, .. } => {
                let step = self.step();
                let value = match key {
                    Key::ArrowRight | Key::ArrowUp => self.value + step,
                    Key::ArrowLeft | Key::ArrowDown => self.value - step,
                    Key::Home => self.min,
                    Key::End => self.max,
                    _ => return,
                };
                self.change(cx, value);
                cx.set_handled();
            }
            Event::Increment => {
                let value = self.value + self.step();
                self.change(cx, value);
            }
            Event::Decrement => {
                let value = self.value - self.step();
                self.change(cx, value);
            }
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        self.editable
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(if self.editable {
            accesskit::Role::Slider
        } else {
            accesskit::Role::Meter
        })
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        node.set_numeric_value(self.value);
        node.set_min_numeric_value(self.min);
        node.set_max_numeric_value(self.max);
        if self.editable {
            node.add_action(accesskit::Action::Increment);
            node.add_action(accesskit::Action::Decrement);
        }
    }
}

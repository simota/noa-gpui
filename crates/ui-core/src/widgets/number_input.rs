use ui_layout::{MeasureInput, Style};
use ui_scene::{Border, Direction, Point, Rect, Size};
use ui_text::TextStore;

use super::editable::SingleLine;
use super::stepper::{REPEAT_DELAY, REPEAT_INTERVAL};
use super::text_field::{Affix, paint_field_box};
use super::{TextSubmitted, ValueChanged, pressed_tint};
use crate::event::{Cursor, Event, Key, PointerButton};
use crate::theme::{Corner, Role};
use crate::widget::{EventCx, MeasureCx, PaintCx, UpdateCx, Widget};

const PAD_X: f32 = 8.0;
const STEPPER_W: f32 = 16.0;
const SCRUB_SLOP: f32 = 3.0;
/// Pixels of horizontal movement per step while scrubbing.
pub const SCRUB_PIXELS: f32 = 4.0;

/// The number written in `text`, or `None` if it is not one. Spaces and a trailing
/// `unit` are ignored, and a decimal comma is read as a point when there is no point
/// (`"1,5"` is 1.5).
pub fn parse_number(text: &str, unit: &str) -> Option<f64> {
    let mut text = text.trim();
    if !unit.is_empty() {
        text = text.strip_suffix(unit).unwrap_or(text).trim_end();
    }
    let cleaned: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let cleaned = if cleaned.contains('.') {
        cleaned
    } else {
        cleaned.replacen(',', ".", 1)
    };
    cleaned.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// `value` with `decimals` digits after the point (no "-0").
pub fn format_number(value: f64, decimals: usize) -> String {
    let text = format!("{value:.decimals$}");
    if text.starts_with('-') && text[1..].chars().all(|c| c == '0' || c == '.') {
        text[1..].to_owned()
    } else {
        text
    }
}

#[derive(Debug, Clone, Copy)]
struct Scrub {
    start_x: f32,
    start_value: f64,
    moved: bool,
}

/// A number field kept in `min..=max` (`NSTextField` with a number formatter and an
/// `NSStepper`). Emits [`ValueChanged`] whenever the value changes, during scrubbing
/// too, and [`TextSubmitted`] on Return.
#[derive(Debug)]
pub struct NumberInput {
    line: SingleLine,
    min: f64,
    max: f64,
    value: f64,
    step: f64,
    decimals: usize,
    stepper: bool,
    scrubbable: bool,
    unit: Affix,
    scrub: Option<Scrub>,
    /// The text is being edited (focused by Tab, a click or typing; not by a scrub), so
    /// presses place the caret rather than scrub.
    editing: bool,
    /// The stepper arrow held down (`true` = up) and whether the pointer is over it.
    pressed: Option<(bool, bool)>,
    next_repeat: f64,
}

impl NumberInput {
    /// A field for a number in `min..=max`, starting at `value` (clamped). Bounds given
    /// the wrong way around are swapped, and one that is not a number is no bound.
    pub fn new(min: f64, max: f64, value: f64) -> Self {
        let min = if min.is_nan() { f64::NEG_INFINITY } else { min };
        let max = if max.is_nan() { f64::INFINITY } else { max };
        let (min, max) = (min.min(max), min.max(max));
        // Not a number: the bound nearest to zero.
        let value = if value.is_nan() { 0.0 } else { value };
        let mut input = Self {
            line: SingleLine::new(String::new(), 96.0, PAD_X),
            min,
            max,
            value: value.clamp(min, max),
            step: 1.0,
            decimals: 0,
            stepper: true,
            scrubbable: true,
            unit: Affix::default(),
            scrub: None,
            editing: false,
            pressed: None,
            next_repeat: 0.0,
        };
        input.line.pad_right = PAD_X + STEPPER_W;
        input.show_value();
        input
    }

    /// The amount a step changes the value (1 by default).
    pub fn step(mut self, step: f64) -> Self {
        if step.is_finite() {
            self.step = step.abs();
        }
        self
    }

    /// Digits after the decimal point (0 by default). The value is rounded to them.
    pub fn decimals(mut self, decimals: usize) -> Self {
        self.decimals = decimals;
        self.value = self.fit(self.value);
        self.show_value();
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.line.width = width;
        self
    }

    /// A unit shown after the number, such as `px` or `%` (not part of the text, and
    /// accepted when typed).
    pub fn unit(mut self, unit: impl Into<String>) -> Self {
        self.unit.text = unit.into();
        self
    }

    /// Show the stepper inside the field (on by default).
    pub fn stepper(mut self, stepper: bool) -> Self {
        self.stepper = stepper;
        self.line.pad_right = PAD_X + if stepper { STEPPER_W } else { 0.0 };
        self
    }

    /// Change the value by dragging across the field (on by default).
    pub fn scrubbable(mut self, scrubbable: bool) -> Self {
        self.scrubbable = scrubbable;
        self
    }

    pub fn value(&self) -> f64 {
        self.value
    }

    /// What is typed in the field (the value as shown, or an edit not yet committed).
    pub fn text(&self) -> &str {
        self.line.edit.editor.text()
    }

    pub fn is_editing(&self) -> bool {
        self.editing
    }

    pub fn is_scrubbing(&self) -> bool {
        self.scrub.is_some_and(|s| s.moved)
    }

    /// Set the value (clamped and rounded) without emitting.
    pub fn set_value(&mut self, cx: &mut UpdateCx, value: f64) {
        self.value = self.fit(value);
        self.show_value();
        cx.request_layout();
    }

    /// `value` clamped to the range and rounded to the decimals, rounding toward the
    /// inside where the nearest rounding would leave the range (0.75 with one decimal
    /// and a maximum of 0.75 is 0.7, not 0.8).
    fn fit(&self, value: f64) -> f64 {
        if value.is_nan() {
            // Not a number is no value: the field keeps the one it has.
            return self.value;
        }
        let value = value.clamp(self.min, self.max);
        let factor = 10f64.powi(self.decimals.min(15) as i32);
        let rounded = (value * factor).round() / factor;
        let inside = if rounded > self.max {
            (value * factor).floor() / factor
        } else if rounded < self.min {
            (value * factor).ceil() / factor
        } else {
            rounded
        };
        // A range narrower than the precision: keep the clamped value.
        if (self.min..=self.max).contains(&inside) {
            inside
        } else {
            value
        }
    }

    fn show_value(&mut self) {
        let text = format_number(self.value, self.decimals);
        if text != self.line.edit.editor.text() {
            self.line.edit.editor.set_text(&text);
        }
    }

    fn apply(&mut self, cx: &mut EventCx, value: f64) {
        let value = self.fit(value);
        if value != self.value {
            self.value = value;
            cx.emit(ValueChanged(value));
        }
        self.show_value();
        cx.request_layout();
    }

    fn commit(&mut self, cx: &mut EventCx) {
        match parse_number(self.text(), &self.unit.text) {
            Some(value) => self.apply(cx, value),
            None => {
                self.show_value();
                cx.request_layout();
            }
        }
    }

    /// Step `steps` times from the typed number (or the value).
    fn step_by(&mut self, cx: &mut EventCx, steps: f64) {
        let base = parse_number(self.text(), &self.unit.text).unwrap_or(self.value);
        let value = base + steps * self.step;
        // Drop float noise such as 0.1 + 0.2 = 0.30000000000000004.
        let value = (value * 1e9).round() / 1e9;
        self.apply(cx, value);
        self.line.edit.editor.select_all();
    }

    fn stepper_rect(&self, height: f32) -> Rect {
        Rect::new(
            self.line.width - STEPPER_W - 3.0,
            3.0,
            STEPPER_W,
            height - 6.0,
        )
    }

    fn paint_stepper(&self, cx: &mut PaintCx) {
        let rect = self.stepper_rect(cx.size().height);
        let half = rect.height / 2.0;
        if let Some((up, true)) = self.pressed {
            let surface = cx.color(Role::Surface);
            let tint = pressed_tint(cx, surface);
            let y = if up { rect.y } else { rect.y + half };
            let radius = cx.corner(Corner::Inner, 3.0, half);
            cx.rounded_rect(
                Rect::new(rect.x, y, rect.width, half),
                radius,
                tint,
                Border::default(),
            );
        }
        let ink = cx.color(Role::Text);
        let dim = |at_end: bool| {
            if at_end {
                ink.with_alpha(ink.a * 0.35)
            } else {
                ink
            }
        };
        let (w, h) = (7.0, 3.5);
        let x = rect.x + (rect.width - w) / 2.0;
        cx.chevron(
            Rect::new(x, rect.y + (half - h) / 2.0, w, h),
            Direction::Up,
            1.5,
            dim(self.value >= self.max),
        );
        cx.chevron(
            Rect::new(x, rect.y + half + (half - h) / 2.0, w, h),
            Direction::Down,
            1.5,
            dim(self.value <= self.min),
        );
    }

    fn scrub_to(&mut self, cx: &mut EventCx, x: f32) {
        let Some(scrub) = self.scrub.as_mut() else {
            return;
        };
        let dx = x - scrub.start_x;
        if !scrub.moved && dx.abs() < SCRUB_SLOP {
            return;
        }
        scrub.moved = true;
        let modifiers = cx.modifiers();
        let factor = if modifiers.shift {
            10.0
        } else if modifiers.alt {
            0.1
        } else {
            1.0
        };
        let steps = (dx / SCRUB_PIXELS).trunc() as f64 * factor;
        let value = scrub.start_value + steps * self.step;
        let value = (value * 1e9).round() / 1e9;
        self.apply(cx, value);
    }
}

impl Widget for NumberInput {
    fn paints_style_box(&self) -> bool {
        true
    }

    fn cursor(&self, local: Point, _size: Size) -> Option<Cursor> {
        self.line.cursor(local)
    }

    fn style(&self) -> Style {
        self.line.style()
    }

    fn measures(&self) -> bool {
        true
    }

    fn measure(&mut self, cx: &mut MeasureCx, _input: MeasureInput) -> Size {
        let style = self.line.edit.style;
        self.line.affix_right = self.unit.measure(cx.text, style);
        self.line.measure(cx.text)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        paint_field_box(cx, Corner::Control, 6.0);
        let focused = cx.is_focused() && self.editing;
        let shown = self.line.edit.shaped.size().width.min(
            self.line.width - self.line.pad_left - self.line.pad_right - self.line.affix_right,
        );
        if !self.unit.text.is_empty() {
            self.unit.paint(cx, self.line.pad_left + shown + 3.0);
        }
        self.line.paint_text(cx, focused);
        if self.stepper {
            self.paint_stepper(cx);
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        let composing = !self.line.edit.composition().is_empty();
        match event {
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } if !composing => {
                let p = cx.local(*pos);
                if self.stepper && self.stepper_rect(cx.size().height).contains(p) {
                    let rect = self.stepper_rect(cx.size().height);
                    let up = p.y < rect.y + rect.height / 2.0;
                    self.pressed = Some((up, true));
                    self.step_by(cx, if up { 1.0 } else { -1.0 });
                    self.next_repeat = cx.now() + REPEAT_DELAY;
                    cx.request_animation_frame();
                    cx.request_focus();
                    cx.capture_pointer();
                    cx.set_handled();
                    return;
                }
                if self.scrubbable && !self.editing {
                    self.scrub = Some(Scrub {
                        start_x: p.x,
                        start_value: self.value,
                        moved: false,
                    });
                    cx.capture_pointer();
                    cx.set_handled();
                    return;
                }
            }
            Event::PointerMove { pos } if self.scrub.is_some() => {
                let x = cx.local(*pos).x;
                self.scrub_to(cx, x);
                cx.set_handled();
                return;
            }
            Event::PointerUp { .. } if self.scrub.is_some() => {
                let scrub = self.scrub.take().expect("checked");
                cx.release_pointer();
                if !scrub.moved {
                    cx.request_focus();
                    self.editing = true;
                    self.line.edit.editor.select_all();
                }
                cx.request_layout();
                cx.set_handled();
                return;
            }
            Event::PointerMove { pos } if self.pressed.is_some() => {
                let (up, over) = self.pressed.expect("checked");
                let rect = self.stepper_rect(cx.size().height);
                let p = cx.local(*pos);
                let now_over = rect.contains(p) && (p.y < rect.y + rect.height / 2.0) == up;
                if now_over != over {
                    self.pressed = Some((up, now_over));
                    cx.request_paint();
                }
                return;
            }
            Event::PointerUp { .. } if self.pressed.is_some() => {
                self.pressed = None;
                cx.release_pointer();
                cx.request_paint();
                cx.set_handled();
                return;
            }
            Event::AnimationFrame { now } => {
                if let Some((up, over)) = self.pressed {
                    if over && *now >= self.next_repeat {
                        self.step_by(cx, if up { 1.0 } else { -1.0 });
                        self.next_repeat = now + REPEAT_INTERVAL;
                    }
                    cx.request_animation_frame();
                }
                return;
            }
            Event::KeyDown { key, modifiers } if !composing => {
                let big = if modifiers.shift { 10.0 } else { 1.0 };
                let steps = match key {
                    Key::ArrowUp => big,
                    Key::ArrowDown => -big,
                    Key::PageUp => 10.0,
                    Key::PageDown => -10.0,
                    Key::Enter => {
                        self.commit(cx);
                        self.line.edit.editor.select_all();
                        cx.emit(TextSubmitted(self.text().to_owned()));
                        // Left unhandled, so a default button still answers Return.
                        return;
                    }
                    Key::Tab => {
                        // Leaving: commit, and let the tree move the focus.
                        self.commit(cx);
                        return;
                    }
                    Key::Escape
                        if parse_number(self.text(), &self.unit.text) != Some(self.value) =>
                    {
                        self.show_value();
                        self.line.edit.editor.select_all();
                        cx.request_layout();
                        cx.set_handled();
                        return;
                    }
                    _ => 0.0,
                };
                if steps != 0.0 {
                    self.step_by(cx, steps);
                    cx.set_handled();
                    return;
                }
            }
            Event::FocusChanged(false) => {
                self.line.event(cx, event);
                self.editing = false;
                self.commit(cx);
                return;
            }
            Event::FocusChanged(true) => {
                // Tabbing in edits; a scrub only takes the focus.
                if self.scrub.is_none() {
                    self.editing = true;
                    self.line.edit.editor.select_all();
                    cx.request_layout();
                }
                return;
            }
            Event::Text(_) | Event::Composition { .. } => self.editing = true,
            Event::Increment => {
                self.step_by(cx, 1.0);
                return;
            }
            Event::Decrement => {
                self.step_by(cx, -1.0);
                return;
            }
            _ => {}
        }
        self.line.event(cx, event);
    }

    fn focusable(&self) -> bool {
        true
    }

    fn text_input_area(&self) -> Option<Rect> {
        Some(self.line.input_area())
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::SpinButton)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        node.set_numeric_value(self.value);
        node.set_min_numeric_value(self.min);
        node.set_max_numeric_value(self.max);
        node.set_numeric_value_step(self.step);
        node.set_value(self.text().to_owned());
        node.add_action(accesskit::Action::Increment);
        node.add_action(accesskit::Action::Decrement);
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.line.edit.removed(text);
        self.unit.shaped.release(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_parse_with_units_spaces_and_decimal_commas() {
        assert_eq!(parse_number(" 42 ", ""), Some(42.0));
        assert_eq!(parse_number("-3.5", ""), Some(-3.5));
        assert_eq!(parse_number("1,5", ""), Some(1.5));
        assert_eq!(parse_number("12 px", "px"), Some(12.0));
        assert_eq!(parse_number("12px", "px"), Some(12.0));
        assert_eq!(parse_number("abc", ""), None);
        assert_eq!(parse_number("", ""), None);
        assert_eq!(parse_number("inf", ""), None);
    }

    #[test]
    fn formatting_rounds_and_drops_negative_zero() {
        assert_eq!(format_number(1.25, 1), "1.2");
        assert_eq!(format_number(3.0, 0), "3");
        assert_eq!(format_number(-0.0001, 2), "0.00");
        assert_eq!(format_number(-2.5, 2), "-2.50");
    }
}

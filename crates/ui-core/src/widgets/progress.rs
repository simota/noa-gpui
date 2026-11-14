use std::f32::consts::TAU;

use ui_layout::{MeasureInput, Style};
use ui_scene::{Border, Point, Rect, Size};

use super::ControlSize;
use crate::event::Event;
use crate::theme::Role;
use crate::widget::{EventCx, MeasureCx, PaintCx, UpdateCx, Widget};

/// Seconds for the indeterminate bar's highlight to cross the track.
pub const BAR_PERIOD: f64 = 1.4;
/// Seconds per turn of the indeterminate spinner.
pub const SPINNER_PERIOD: f64 = 1.0;
const SPOKES: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Determinate { value: f64, max: f64 },
    Indeterminate,
}

/// The animation runs only between [`ProgressBar::start_animation`] and
/// [`ProgressBar::stop_animation`] (likewise for the spinner), and only while the
/// indicator is visible: the tree withholds frames from widgets scrolled out of view
/// or clipped away, so a hidden busy indicator costs nothing.
#[derive(Debug)]
struct Progress {
    mode: Mode,
    animating: bool,
    started: f64,
    now: f64,
}

impl Progress {
    fn new() -> Self {
        Self {
            mode: Mode::Determinate {
                value: 0.0,
                max: 1.0,
            },
            animating: false,
            started: 0.0,
            now: 0.0,
        }
    }

    fn fraction(&self) -> Option<f32> {
        match self.mode {
            Mode::Determinate { value, max } if max > 0.0 => {
                Some((value / max).clamp(0.0, 1.0) as f32)
            }
            Mode::Determinate { .. } => Some(0.0),
            Mode::Indeterminate => None,
        }
    }

    /// Position in the animation cycle of `period` seconds, in `0..1`.
    fn phase(&self, period: f64) -> f32 {
        ((self.now - self.started) / period).rem_euclid(1.0) as f32
    }

    fn start(&mut self, cx: &mut UpdateCx) {
        if !self.animating {
            self.animating = true;
            self.started = cx.now();
            self.now = cx.now();
            cx.request_animation_frame();
            cx.request_paint();
        }
    }

    fn stop(&mut self, cx: &mut UpdateCx) {
        if self.animating {
            self.animating = false;
            cx.request_paint();
        }
    }

    fn set_value(&mut self, cx: &mut UpdateCx, value: f64, max: f64) {
        self.mode = Mode::Determinate { value, max };
        cx.request_paint();
    }

    fn frame(&mut self, cx: &mut EventCx, event: &Event) {
        if let Event::AnimationFrame { now } = event
            && self.animating
        {
            self.now = *now;
            cx.request_paint();
            cx.request_animation_frame();
        }
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        if let Mode::Determinate { value, max } = self.mode {
            node.set_numeric_value(value);
            node.set_min_numeric_value(0.0);
            node.set_max_numeric_value(max);
        } else if self.animating {
            node.set_busy();
        }
    }
}

/// A horizontal progress bar (`NSProgressIndicator`, bar style).
#[derive(Debug)]
pub struct ProgressBar {
    progress: Progress,
    width: f32,
    size: ControlSize,
}

impl ProgressBar {
    pub fn new(width: f32) -> Self {
        Self {
            progress: Progress::new(),
            width,
            size: ControlSize::Regular,
        }
    }

    pub fn value(mut self, value: f64, max: f64) -> Self {
        self.progress.mode = Mode::Determinate { value, max };
        self
    }

    /// Start indeterminate (call [`ProgressBar::start_animation`] to animate).
    pub fn indeterminate(mut self) -> Self {
        self.progress.mode = Mode::Indeterminate;
        self
    }

    pub fn control_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    /// The fraction done, or `None` when indeterminate.
    pub fn fraction(&self) -> Option<f32> {
        self.progress.fraction()
    }

    pub fn is_animating(&self) -> bool {
        self.progress.animating
    }

    pub fn set_value(&mut self, cx: &mut UpdateCx, value: f64, max: f64) {
        self.progress.set_value(cx, value, max);
    }

    pub fn set_indeterminate(&mut self, cx: &mut UpdateCx) {
        self.progress.mode = Mode::Indeterminate;
        cx.request_paint();
    }

    pub fn start_animation(&mut self, cx: &mut UpdateCx) {
        self.progress.start(cx);
    }

    pub fn stop_animation(&mut self, cx: &mut UpdateCx) {
        self.progress.stop(cx);
    }

    /// Where the indeterminate highlight is: left edge and width in `0..1` of the
    /// track, or `None` when it is not drawn.
    pub fn highlight(&self) -> Option<(f32, f32)> {
        if self.progress.mode != Mode::Indeterminate || !self.progress.animating {
            return None;
        }
        const WIDTH: f32 = 0.3;
        let x = -WIDTH + self.progress.phase(BAR_PERIOD) * (1.0 + WIDTH);
        Some((x, WIDTH))
    }
}

impl Widget for ProgressBar {
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
        Size::new(self.width, 6.0 * self.size.scale())
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let radius = cx.pill(bounds.height);
        let track = cx.color(Role::Muted).with_alpha(0.25);
        cx.rounded_rect(bounds, radius, track, Border::default());
        let accent = cx.color(Role::Accent);
        if let Some(fraction) = self.fraction() {
            if fraction > 0.0 {
                let w = (bounds.width * fraction).max(bounds.height);
                let fill = Rect::new(0.0, 0.0, w, bounds.height);
                cx.bezel(fill, radius, accent, Border::default());
            }
        } else if let Some((x, w)) = self.highlight() {
            cx.push_clip(bounds);
            let rect = Rect::new(x * bounds.width, 0.0, w * bounds.width, bounds.height);
            cx.rounded_rect(rect, radius, accent, Border::default());
            cx.pop_clip();
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        self.progress.frame(cx, event);
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::ProgressIndicator)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        self.progress.accessibility(node);
    }
}

/// A round progress indicator (`NSProgressIndicator`, spinning style).
#[derive(Debug)]
pub struct Spinner {
    progress: Progress,
    size: ControlSize,
}

impl Default for Spinner {
    fn default() -> Self {
        Self::new()
    }
}

impl Spinner {
    pub fn new() -> Self {
        let mut progress = Progress::new();
        progress.mode = Mode::Indeterminate;
        Self {
            progress,
            size: ControlSize::Regular,
        }
    }

    pub fn determinate(mut self, value: f64, max: f64) -> Self {
        self.progress.mode = Mode::Determinate { value, max };
        self
    }

    pub fn control_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    pub fn fraction(&self) -> Option<f32> {
        self.progress.fraction()
    }

    pub fn is_animating(&self) -> bool {
        self.progress.animating
    }

    pub fn set_value(&mut self, cx: &mut UpdateCx, value: f64, max: f64) {
        self.progress.set_value(cx, value, max);
    }

    pub fn start_animation(&mut self, cx: &mut UpdateCx) {
        self.progress.start(cx);
    }

    pub fn stop_animation(&mut self, cx: &mut UpdateCx) {
        self.progress.stop(cx);
    }

    /// The brightest spoke (`0` = 12 o'clock), or `None` when not spinning.
    pub fn lead_spoke(&self) -> Option<usize> {
        (self.progress.mode == Mode::Indeterminate && self.progress.animating).then(|| {
            (self.progress.phase(SPINNER_PERIOD) * SPOKES as f32).floor() as usize % SPOKES
        })
    }

    fn diameter(&self) -> f32 {
        32.0 * self.size.scale()
    }
}

impl Widget for Spinner {
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
        let d = self.diameter();
        Size::new(d, d)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let d = self.diameter();
        let center = Point::new(d / 2.0, d / 2.0);
        let ink = cx.color(Role::Text);
        if let Some(fraction) = self.fraction() {
            let width = d * 0.12;
            let radius = d / 2.0 - width;
            let track = cx.color(Role::Muted).with_alpha(0.25);
            cx.arc(center, radius, width, 1.0, track);
            if fraction > 0.0 {
                let accent = cx.color(Role::Accent);
                cx.arc(center, radius, width, fraction, accent);
            }
            return;
        }
        let lead = self.lead_spoke();
        let (inner, outer) = (d * 0.25, d * 0.46);
        for i in 0..SPOKES {
            let alpha = match lead {
                Some(lead) => {
                    let behind = (lead + SPOKES - i) % SPOKES;
                    1.0 - behind as f32 / SPOKES as f32 * 0.85
                }
                None => 0.3,
            };
            let a = i as f32 / SPOKES as f32 * TAU;
            let (sin, cos) = a.sin_cos();
            let at = |r: f32| Point::new(center.x + r * sin, center.y - r * cos);
            cx.line(
                at(inner),
                at(outer),
                d * 0.08,
                ink.with_alpha(ink.a * alpha),
            );
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        self.progress.frame(cx, event);
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::ProgressIndicator)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        self.progress.accessibility(node);
    }
}

use noa_gpui_layout::{MeasureInput, Style};
use noa_gpui_scene::{Border, Color, Rect, Size};

use crate::event::Event;
use crate::theme::{Corner, Role, mix};
use crate::widget::{EventCx, MeasureCx, PaintCx, UpdateCx, Widget};

/// Seconds for the shimmer to cross a skeleton.
pub const SHIMMER_PERIOD: f64 = 1.6;
/// The shimmer band's width, as a fraction of the skeleton's.
const BAND: f32 = 0.45;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Shape {
    Rect { radius: f32 },
    Circle,
}

/// A placeholder for content that is loading.
///
/// It draws frames only while animating and visible.
/// Assistive technology skips it; mark the region it stands in for as busy instead.
#[derive(Debug)]
pub struct Skeleton {
    size: Size,
    shape: Shape,
    animating: bool,
    started: f64,
    now: f64,
}

impl Skeleton {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            size: Size::new(width, height),
            shape: Shape::Rect { radius: 6.0 },
            animating: false,
            started: 0.0,
            now: 0.0,
        }
    }

    pub fn line(width: f32) -> Self {
        Self::new(width, 12.0).radius(4.0)
    }

    pub fn circle(diameter: f32) -> Self {
        Self {
            shape: Shape::Circle,
            ..Self::new(diameter, diameter)
        }
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.shape = Shape::Rect { radius };
        self
    }

    pub fn is_animating(&self) -> bool {
        self.animating
    }

    pub fn start_animation(&mut self, cx: &mut UpdateCx) {
        if !self.animating {
            self.animating = true;
            self.started = cx.now();
            self.now = cx.now();
            cx.request_animation_frame();
            cx.request_paint();
        }
    }

    pub fn stop_animation(&mut self, cx: &mut UpdateCx) {
        if self.animating {
            self.animating = false;
            cx.request_paint();
        }
    }

    /// Where the shimmer is in its cycle, `0..1`, or `None` when still.
    pub fn phase(&self) -> Option<f32> {
        self.animating
            .then(|| ((self.now - self.started) / SHIMMER_PERIOD).rem_euclid(1.0) as f32)
    }
}

impl Widget for Skeleton {
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
        self.size
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let base = mix(cx.color(Role::Surface), cx.color(Role::Text), 0.1);
        let light = cx.color(Role::Surface).with_alpha(0.55);
        match self.shape {
            Shape::Circle => {
                let lift = self.phase().map_or(0.0, |p| {
                    (1.0 - (p * std::f32::consts::TAU).cos()) / 2.0 * 0.5
                });
                let fill = mix(base, light.with_alpha(1.0), lift * 0.6);
                cx.rounded_rect(bounds, bounds.height / 2.0, fill, Border::default());
            }
            Shape::Rect { radius } => {
                let radius = cx.corner(Corner::Inner, radius, bounds.height);
                cx.rounded_rect(bounds, radius, base, Border::default());
                if let Some(phase) = self.phase() {
                    let w = bounds.width * BAND;
                    let x = -w + phase * (bounds.width + w);
                    let clear = Color::TRANSPARENT;
                    cx.push_clip(bounds);
                    let half = w / 2.0;
                    let left = Rect::new(x, 0.0, half, bounds.height);
                    let right = Rect::new(x + half, 0.0, half, bounds.height);
                    cx.gradient(left, 0.0, clear, light, false);
                    cx.gradient(right, 0.0, light, clear, false);
                    cx.pop_clip();
                }
            }
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        if let Event::AnimationFrame { now } = event
            && self.animating
        {
            if cx.reduces_motion() {
                self.now = self.started;
                cx.request_paint();
                return;
            }
            self.now = *now;
            cx.request_paint();
            cx.request_animation_frame();
        }
    }
}

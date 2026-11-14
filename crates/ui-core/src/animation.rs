//! The tree never redraws on its own. An animating widget calls
//! [`crate::EventCx::request_animation_frame`], receives one
//! [`crate::Event::AnimationFrame`] on the next frame, and asks again until it is done.
//! When no widget asks, the application stops drawing.
//!
//! Times are seconds on the application's monotonic clock ([`crate::UiTree::set_now`]).

use ui_scene::Color;

/// How progress (0 to 1) maps to the interpolated value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Easing {
    Linear,
    /// Fast start, gentle stop (cubic). The default for state changes.
    #[default]
    EaseOut,
    /// Gentle start and stop (cubic). For movement across a distance.
    EaseInOut,
}

impl Easing {
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear => t,
            Easing::EaseOut => 1.0 - (1.0 - t).powi(3),
            Easing::EaseInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transition {
    from: f32,
    to: f32,
    start: f64,
    duration: f64,
    easing: Easing,
}

impl Transition {
    pub fn new(value: f32) -> Self {
        Self {
            from: value,
            to: value,
            start: 0.0,
            duration: 0.0,
            easing: Easing::default(),
        }
    }

    /// A transition already heading to `target` keeps going.
    pub fn animate_to(&mut self, target: f32, now: f64, duration: f64, easing: Easing) {
        if target == self.to {
            return;
        }
        self.from = self.value(now);
        self.to = target;
        self.start = now;
        self.duration = duration;
        self.easing = easing;
    }

    pub fn set(&mut self, value: f32) {
        *self = Self::new(value);
    }

    pub fn target(&self) -> f32 {
        self.to
    }

    pub fn value(&self, now: f64) -> f32 {
        if self.is_done(now) {
            return self.to;
        }
        let t = ((now - self.start) / self.duration) as f32;
        self.from + (self.to - self.from) * self.easing.apply(t)
    }

    pub fn is_done(&self, now: f64) -> bool {
        self.duration <= 0.0 || now >= self.start + self.duration
    }
}

/// Includes alpha.
pub fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_fixes_the_end_points() {
        for easing in [Easing::Linear, Easing::EaseOut, Easing::EaseInOut] {
            assert_eq!(easing.apply(0.0), 0.0);
            assert_eq!(easing.apply(1.0), 1.0);
            assert_eq!(easing.apply(2.0), 1.0);
        }
        assert!(Easing::EaseOut.apply(0.5) > 0.5);
        assert_eq!(Easing::EaseInOut.apply(0.5), 0.5);
    }

    #[test]
    fn transitions_interpolate_and_retarget_from_the_current_value() {
        let mut t = Transition::new(0.0);
        t.animate_to(1.0, 10.0, 0.2, Easing::Linear);
        assert_eq!(t.value(10.0), 0.0);
        assert!((t.value(10.1) - 0.5).abs() < 1e-5);
        assert!(!t.is_done(10.1));
        // Reversing halfway starts from 0.5, not from 1.0.
        t.animate_to(0.0, 10.1, 0.2, Easing::Linear);
        assert!((t.value(10.1) - 0.5).abs() < 1e-5);
        assert!((t.value(10.2) - 0.25).abs() < 1e-5);
        assert!(t.is_done(10.3));
        assert_eq!(t.value(99.0), 0.0);
    }
}

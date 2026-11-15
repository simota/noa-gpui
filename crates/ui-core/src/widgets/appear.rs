use crate::animation::{Easing, Transition};
use crate::widget::EventCx;

/// Starts on the first animation frame after the overlay opens, so the opener only has
/// to request one.
#[derive(Debug)]
pub(crate) struct Appear {
    progress: Transition,
    duration: f64,
    started: bool,
    closing: bool,
    shown: f32,
}

impl Appear {
    pub fn new(duration: f64) -> Self {
        Self {
            progress: Transition::new(0.0),
            duration,
            started: false,
            closing: false,
            shown: 0.0,
        }
    }

    /// Eased, 0 to 1. Fully shown until the first frame, so an overlay opened without
    /// requesting frames appears at once.
    pub fn value(&self) -> f32 {
        if self.started { self.shown } else { 1.0 }
    }

    pub fn is_closing(&self) -> bool {
        self.closing
    }

    /// Returns `false` if already hiding.
    pub fn close(&mut self, now: f64) -> bool {
        if self.closing {
            return false;
        }
        self.closing = true;
        if !self.started {
            self.started = true;
            self.shown = 1.0;
            self.progress.set(1.0);
        }
        self.progress
            .animate_to(0.0, now, self.duration * 0.75, Easing::EaseOut);
        true
    }

    /// Returns `true` once hiding has finished. With reduced motion
    /// ([`EventCx::reduces_motion`]) showing and hiding finish on this frame.
    pub fn frame(&mut self, cx: &mut EventCx, now: f64) -> bool {
        if !self.started {
            self.started = true;
            self.progress
                .animate_to(1.0, now, self.duration, Easing::EaseOut);
        }
        if cx.reduces_motion() {
            self.progress.set(self.progress.target());
        }
        self.shown = self.progress.value(now);
        if self.progress.is_done(now) {
            return self.closing;
        }
        cx.request_animation_frame();
        false
    }
}

/// `rect` is in widget coordinates. In Noa's dark theme, where a shadow would not show on
/// the charcoal, it draws a faint accent glow instead.
pub(crate) fn shadow(cx: &mut crate::widget::PaintCx, rect: noa_gpui_scene::Rect, radius: f32) {
    use crate::theme::Design;
    let dark = cx.theme().appearance == crate::theme::Appearance::Dark;
    let alpha = if dark { 0.22 } else { 0.07 };
    let mut color = noa_gpui_scene::Color::BLACK;
    let (steps, spread, drop, alpha) = match cx.theme().design {
        Design::FlatVibrancy => (3, 2.0, 1.0, alpha * 0.9),
        Design::BigSur => (4, 2.5, 1.5, alpha),
        Design::LiquidGlass => (6, 3.5, 2.5, alpha * 1.25),
        Design::Aero => (5, 3.0, 1.5, alpha * 1.4),
        Design::Metro => (1, 1.0, 0.0, alpha * 0.8),
        Design::Fluent2 => (5, 3.0, 2.0, alpha),
        Design::Noa if dark => {
            color = cx.theme().accent;
            (5, 3.0, 0.0, 0.07)
        }
        Design::Noa => (4, 2.5, 1.5, alpha),
    };
    for step in 1..=steps {
        let grow = step as f32 * spread;
        let r = noa_gpui_scene::Rect::new(
            rect.x - grow,
            rect.y - grow + step as f32 * drop,
            rect.width + 2.0 * grow,
            rect.height + 2.0 * grow,
        );
        cx.rounded_rect(
            r,
            radius + grow,
            color.with_alpha(alpha / step as f32),
            noa_gpui_scene::Border::default(),
        );
    }
}

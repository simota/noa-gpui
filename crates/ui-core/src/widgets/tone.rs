use noa_gpui_scene::Color;

use crate::theme::{Appearance, Role, TEXT_CONTRAST, Theme, mix, on_fill, over, readable};
use crate::widget::PaintCx;

/// What a [`super::Badge`], [`super::Tag`] or [`super::NotificationBanner`] says about
/// its subject. The status tones use the theme's [`Theme::success`], [`Theme::warning`]
/// and [`Theme::critical`] colors (the level indicator's green, yellow and red unless
/// changed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tone {
    /// No particular meaning (gray).
    #[default]
    Neutral,
    /// Information, or the app's own color.
    Accent,
    Success,
    Warning,
    Critical,
}

impl Tone {
    /// The tone's color, dimmed when the widget is disabled.
    pub(crate) fn color(self, cx: &PaintCx) -> Color {
        cx.color(self.base(cx.theme()))
    }

    /// Text and marks drawn on a solid fill of the tone's color: white, or dark ink
    /// on fills too light for white text (yellow, green, red; see [`on_fill`]).
    pub(crate) fn on_solid(self, cx: &PaintCx) -> Color {
        match self {
            Tone::Accent => cx.color(Role::OnAccent),
            _ => cx.color(on_fill(self.base(cx.theme()))),
        }
    }

    /// The tone's color before dimming for a disabled widget.
    fn base(self, theme: &Theme) -> Color {
        match self {
            Tone::Neutral => theme.muted,
            Tone::Accent => theme.accent,
            Tone::Success => theme.success,
            Tone::Warning => theme.warning,
            Tone::Critical => theme.critical,
        }
    }

    /// A light wash of the tone, for backgrounds behind regular text.
    pub(crate) fn wash(self, cx: &PaintCx, strength: f32) -> Color {
        let color = self.color(cx);
        // Saturated colors stand out more against a dark background.
        let strength = match cx.theme().appearance {
            Appearance::Dark => strength * 0.6,
            Appearance::Light => strength,
        };
        color.with_alpha(color.a * strength)
    }

    /// Text in the tone's color that stays readable on the window background: the
    /// color pulled toward the text color (yellow and green are too light alone), and
    /// further where needed to reach 4.5:1 on the background and surfaces.
    pub(crate) fn ink(self, cx: &PaintCx) -> Color {
        cx.color(self.ink_in(cx.theme()))
    }

    fn ink_in(self, theme: &Theme) -> Color {
        let text = theme.text;
        let tinted = match self {
            Tone::Neutral => return text,
            Tone::Accent => mix(self.base(theme), text, 0.15),
            Tone::Warning => mix(self.base(theme), text, 0.6),
            Tone::Success | Tone::Critical => mix(self.base(theme), text, 0.3),
        };
        let grounds = [theme.background, over(theme.surface, theme.background)];
        readable(tinted, text, &grounds, TEXT_CONTRAST)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Design, contrast};

    const TONES: [Tone; 5] = [
        Tone::Neutral,
        Tone::Accent,
        Tone::Success,
        Tone::Warning,
        Tone::Critical,
    ];

    /// Tinted text (tags, banners, error messages) reads at 4.5:1 on the window
    /// background and surfaces in every design and appearance.
    #[test]
    fn ink_is_readable_in_every_theme() {
        for design in Design::ALL {
            for appearance in [Appearance::Light, Appearance::Dark] {
                let theme = Theme::new(design, appearance);
                for ground in [theme.background, over(theme.surface, theme.background)] {
                    for tone in TONES {
                        let ratio = contrast(tone.ink_in(&theme), ground);
                        assert!(
                            ratio >= TEXT_CONTRAST,
                            "{design:?} {appearance:?} {tone:?}: {ratio:.2}"
                        );
                    }
                }
            }
        }
    }

    /// Text on a solid tone fill (badges) is the more legible of white and dark ink.
    #[test]
    fn text_on_solid_fills_is_legible() {
        for appearance in [Appearance::Light, Appearance::Dark] {
            let theme = Theme::new(Design::BigSur, appearance);
            for tone in [Tone::Neutral, Tone::Success, Tone::Warning, Tone::Critical] {
                let fill = tone.base(&theme);
                let ratio = contrast(on_fill(fill), fill);
                assert!(
                    ratio >= TEXT_CONTRAST,
                    "{appearance:?} {tone:?}: {ratio:.2}"
                );
            }
        }
    }
}

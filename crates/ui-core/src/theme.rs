//! Widgets refer to colors by [`Role`] (or a [`Paint`], which is a role or a fixed
//! color) and round their corners through [`Theme::corner`], so switching the tree's
//! [`Theme`] with [`crate::UiTree::set_theme`] restyles everything. A design changes
//! how widgets are drawn, never their size.

use ui_scene::Color;

use crate::widgets::{LEVEL_CRITICAL, LEVEL_NORMAL, LEVEL_WARNING};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Appearance {
    #[default]
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum Design {
    /// OS X 10.10 Yosemite to 10.15: flat fills, small corners, translucent menus and
    /// panels.
    FlatVibrancy,
    /// macOS 11 Big Sur to 15 Sequoia.
    #[default]
    BigSur,
    /// macOS 26 Tahoe: capsule controls, large corners and glass with a bright rim.
    LiquidGlass,
    /// Windows Vista and 7: glossy bezels and a blurred glass title bar.
    Aero,
    /// Windows 8: square corners, flat fills, no gloss, an accent-colored title bar.
    Metro,
    /// Windows 11: small corners, Acrylic menus and a Mica title bar.
    Fluent2,
    /// Noa's own look: translucent like a terminal with its background opacity turned
    /// down, flat fills, blocks shaped like the cursor `▮` for knobs and badges, and one
    /// orange accent.
    Noa,
}

impl Design {
    pub const ALL: [Design; 7] = [
        Design::FlatVibrancy,
        Design::BigSur,
        Design::LiquidGlass,
        Design::Aero,
        Design::Metro,
        Design::Fluent2,
        Design::Noa,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Design::FlatVibrancy => "Flat + Vibrancy",
            Design::BigSur => "Big Sur",
            Design::LiquidGlass => "Liquid Glass",
            Design::Aero => "Aero",
            Design::Metro => "Metro",
            Design::Fluent2 => "Fluent 2",
            Design::Noa => "Noa",
        }
    }

    /// Whether windows have Windows caption buttons (minimize, maximize, close at the
    /// right) instead of the macOS ones at the left.
    pub fn windows_chrome(self) -> bool {
        matches!(self, Design::Aero | Design::Metro | Design::Fluent2)
    }

    fn default_accent(self) -> Color {
        match self {
            Design::Aero => Color::hex(0x3399ff),
            Design::Metro => Color::hex(0x2d89ef),
            Design::Fluent2 => Color::hex(0x005fb8),
            Design::Noa => NOA_ACCENT,
            _ => DEFAULT_ACCENT,
        }
    }
}

/// What a rounded corner belongs to, which decides how a [`Design`] reshapes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Corner {
    /// The bezel of a button, field or segmented control: a capsule in Liquid Glass.
    Control,
    /// Something inside another shape: a checkbox, a highlighted row or segment.
    Inner,
    /// A menu, popover, dialog or window.
    Container,
}

/// What a color is used for. Resolved against the current [`Theme`] when painting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Background,
    /// Cards, fields and sidebars drawn on the background.
    Surface,
    /// A slightly different surface (alternating rows, pressed fields).
    SurfaceAlt,
    Text,
    /// Secondary text: captions, placeholders.
    Muted,
    Accent,
    AccentHover,
    AccentPressed,
    /// Text and icons drawn on the accent color.
    OnAccent,
    /// Borders of fields and controls.
    Outline,
    /// Hairlines between sections.
    Separator,
    FocusRing,
    /// Background of selected text.
    Selection,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Paint {
    Fixed(Color),
    Role(Role),
}

impl From<Color> for Paint {
    fn from(color: Color) -> Self {
        Paint::Fixed(color)
    }
}

impl From<Role> for Paint {
    fn from(role: Role) -> Self {
        Paint::Role(role)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub design: Design,
    pub appearance: Appearance,
    pub background: Color,
    pub surface: Color,
    pub surface_alt: Color,
    pub text: Color,
    pub muted: Color,
    pub accent: Color,
    pub accent_hover: Color,
    pub accent_pressed: Color,
    pub on_accent: Color,
    pub outline: Color,
    pub separator: Color,
    pub focus_ring: Color,
    pub selection: Color,
    /// Colors of the [`crate::widgets::Tone`]s. They default to
    /// [`crate::widgets::LEVEL_NORMAL`], `LEVEL_WARNING` and `LEVEL_CRITICAL`; change them
    /// with [`Theme::with_status_colors`].
    pub success: Color,
    pub warning: Color,
    pub critical: Color,
    /// Opacity applied to disabled widgets' colors.
    pub disabled_opacity: f32,
    /// Control borders for the "increase contrast" accessibility setting (see
    /// [`Theme::with_increased_contrast`]). Off by default.
    pub increased_contrast: bool,
}

pub const DEFAULT_ACCENT: Color = Color::hex(0x3b6cf6);

pub const NOA_ACCENT: Color = Color::hex(0xe8a33d);

impl Default for Theme {
    fn default() -> Self {
        Self::light()
    }
}

impl Theme {
    /// The Big Sur light theme.
    pub fn light() -> Self {
        Self {
            appearance: Appearance::Light,
            background: Color::hex(0xf4f5f8),
            surface: Color::WHITE,
            surface_alt: Color::hex(0xeef1f8),
            text: Color::hex(0x1e2230),
            muted: Color::hex(0x666d80),
            outline: Color::hex(0xc9cfdc),
            separator: Color::hex(0xe1e5ee),
            disabled_opacity: 0.4,
            increased_contrast: false,
            ..Self::accented(Appearance::Light, DEFAULT_ACCENT)
        }
        .with_accent(DEFAULT_ACCENT)
    }

    /// The Big Sur dark theme.
    pub fn dark() -> Self {
        Self {
            appearance: Appearance::Dark,
            background: Color::hex(0x1b1d24),
            surface: Color::hex(0x262932),
            surface_alt: Color::hex(0x2e323d),
            text: Color::hex(0xe8eaf0),
            muted: Color::hex(0x9aa1b4),
            outline: Color::hex(0x474c5a),
            separator: Color::hex(0x363a46),
            disabled_opacity: 0.4,
            increased_contrast: false,
            ..Self::accented(Appearance::Dark, DEFAULT_ACCENT)
        }
        .with_accent(DEFAULT_ACCENT)
    }

    /// The Big Sur theme for `appearance`.
    pub fn for_appearance(appearance: Appearance) -> Self {
        Self::new(Design::BigSur, appearance)
    }

    pub fn new(design: Design, appearance: Appearance) -> Self {
        let theme = match (design, appearance) {
            (Design::BigSur, Appearance::Light) => Self::light(),
            (Design::BigSur, Appearance::Dark) => Self::dark(),
            (Design::FlatVibrancy, Appearance::Light) => Self {
                design,
                background: Color::hex(0xececec),
                surface: Color::WHITE,
                surface_alt: Color::hex(0xf6f6f6),
                text: Color::hex(0x1d1d1f),
                muted: Color::hex(0x6b6b6b),
                outline: Color::hex(0xb8b8b8),
                separator: Color::hex(0xd9d9d9),
                ..Self::light()
            },
            (Design::FlatVibrancy, Appearance::Dark) => Self {
                design,
                background: Color::hex(0x2d2d2d),
                surface: Color::hex(0x3a3a3a),
                surface_alt: Color::hex(0x454545),
                text: Color::hex(0xf0f0f0),
                muted: Color::hex(0xb3b3b3),
                outline: Color::hex(0x5c5c5c),
                separator: Color::hex(0x484848),
                ..Self::dark()
            },
            (Design::LiquidGlass, Appearance::Light) => Self {
                design,
                background: Color::hex(0xe9ecf3),
                surface: Color::WHITE.with_alpha(0.74),
                surface_alt: Color::WHITE.with_alpha(0.46),
                text: Color::hex(0x111318),
                muted: Color::hex(0x5f6573),
                outline: Color::BLACK.with_alpha(0.1),
                separator: Color::BLACK.with_alpha(0.07),
                ..Self::light()
            },
            (Design::LiquidGlass, Appearance::Dark) => Self {
                design,
                background: Color::hex(0x0e1015),
                surface: Color::WHITE.with_alpha(0.1),
                surface_alt: Color::WHITE.with_alpha(0.06),
                text: Color::hex(0xf4f5f8),
                muted: Color::hex(0xa0a6b4),
                outline: Color::WHITE.with_alpha(0.16),
                separator: Color::WHITE.with_alpha(0.09),
                ..Self::dark()
            },
            (Design::Aero, Appearance::Light) => Self {
                design,
                background: Color::hex(0xf0f0f0),
                surface: Color::WHITE,
                surface_alt: Color::hex(0xe9eff7),
                text: Color::hex(0x1e1e1e),
                muted: Color::hex(0x6b6b6b),
                outline: Color::hex(0x8f9aa6),
                separator: Color::hex(0xd5dfe5),
                ..Self::light()
            },
            (Design::Aero, Appearance::Dark) => Self {
                design,
                background: Color::hex(0x1c2430),
                surface: Color::hex(0x28323f),
                surface_alt: Color::hex(0x313d4d),
                text: Color::hex(0xe8eef5),
                muted: Color::hex(0x9aa8b8),
                outline: Color::hex(0x55667c),
                separator: Color::hex(0x3a475a),
                ..Self::dark()
            },
            (Design::Metro, Appearance::Light) => Self {
                design,
                background: Color::hex(0xf3f3f3),
                surface: Color::WHITE,
                surface_alt: Color::hex(0xe6e6e6),
                text: Color::BLACK,
                muted: Color::hex(0x666666),
                outline: Color::hex(0xa0a0a0),
                separator: Color::hex(0xd9d9d9),
                ..Self::light()
            },
            (Design::Metro, Appearance::Dark) => Self {
                design,
                background: Color::BLACK,
                surface: Color::hex(0x1f1f1f),
                surface_alt: Color::hex(0x2b2b2b),
                text: Color::WHITE,
                muted: Color::hex(0x999999),
                outline: Color::hex(0x666666),
                separator: Color::hex(0x333333),
                ..Self::dark()
            },
            (Design::Fluent2, Appearance::Light) => Self {
                design,
                background: Color::hex(0xf3f3f3),
                surface: Color::hex(0xfbfbfb),
                surface_alt: Color::hex(0xeeeeee),
                text: Color::hex(0x1a1a1a),
                muted: Color::hex(0x5f5f5f),
                outline: Color::hex(0xd4d4d4),
                separator: Color::hex(0xe5e5e5),
                ..Self::light()
            },
            (Design::Fluent2, Appearance::Dark) => Self {
                design,
                background: Color::hex(0x202020),
                surface: Color::hex(0x2d2d2d),
                surface_alt: Color::hex(0x383838),
                text: Color::WHITE,
                muted: Color::hex(0xababab),
                outline: Color::hex(0x474747),
                separator: Color::hex(0x333333),
                ..Self::dark()
            },
            // Surfaces are washes over the base, so what is behind a panel shows
            // through.
            (Design::Noa, Appearance::Light) => Self {
                design,
                background: NOA_PAPER,
                surface: Color::WHITE.with_alpha(0.5),
                surface_alt: NOA_CHARCOAL.with_alpha(0.07),
                text: NOA_CHARCOAL,
                muted: Color::hex(0x5c5850),
                outline: NOA_CHARCOAL.with_alpha(0.2),
                separator: NOA_CHARCOAL.with_alpha(0.12),
                ..Self::light()
            }
            .with_status_colors(NOA_SUCCESS, NOA_WARNING, NOA_CRITICAL),
            // Surfaces are washes of the text white over the base, so what is behind a
            // panel shows through.
            (Design::Noa, Appearance::Dark) => Self {
                design,
                background: NOA_CHARCOAL,
                surface: NOA_EMBER.with_alpha(0.06),
                surface_alt: NOA_EMBER.with_alpha(0.11),
                text: NOA_EMBER,
                muted: Color::hex(0xaaa69d),
                outline: NOA_EMBER.with_alpha(0.2),
                separator: NOA_EMBER.with_alpha(0.11),
                ..Self::dark()
            }
            .with_status_colors(NOA_SUCCESS, NOA_WARNING, NOA_CRITICAL),
        };
        theme.with_accent(design.default_accent())
    }

    /// Half of `height` (a pill), square in Metro, or barely rounded in Noa (the cursor
    /// `▮`). For tracks, knobs and bars.
    pub fn pill(&self, height: f32) -> f32 {
        match self.design {
            Design::Metro => 0.0,
            Design::Noa => 2.0_f32.min(height / 2.0),
            _ => height / 2.0,
        }
    }

    /// Keeps the design, accent, status colors and contrast setting.
    pub fn with_appearance(self, appearance: Appearance) -> Self {
        Self::new(self.design, appearance)
            .with_accent(self.accent)
            .with_status_colors(self.success, self.warning, self.critical)
            .with_increased_contrast(self.increased_contrast)
    }

    /// Keeps the appearance, the contrast setting, and the accent and status colors
    /// chosen for this theme. An accent or status colors that are the current design's
    /// own give way to the new design's (Noa's orange does not follow into Big Sur, nor
    /// Big Sur's blue into Noa).
    pub fn with_design(self, design: Design) -> Self {
        let own = Self::new(self.design, self.appearance);
        let new = Self::new(design, self.appearance);
        let accent = if self.accent == own.accent {
            new.accent
        } else {
            self.accent
        };
        let status = |t: &Self| (t.success, t.warning, t.critical);
        let (success, warning, critical) = if status(&self) == status(&own) {
            status(&new)
        } else {
            status(&self)
        };
        new.with_accent(accent)
            .with_status_colors(success, warning, critical)
            .with_increased_contrast(self.increased_contrast)
    }

    /// `false` returns to the design's own look.
    ///
    /// The designs draw control borders ([`Role::Outline`]: text fields, bordered
    /// buttons, checkboxes) faintly, as the systems they follow do, below the 3:1 that
    /// WCAG 1.4.11 asks of a control's boundary. With increased contrast the border is
    /// made opaque and pulled toward the text color just far enough to reach
    /// [`MARK_CONTRAST`] on the background and on surfaces. The setting is kept by
    /// [`Self::with_appearance`], [`Self::with_design`] and [`Self::with_accent`].
    pub fn with_increased_contrast(self, increased: bool) -> Self {
        let outline = if increased {
            let grounds = [self.background, over(self.surface, self.background)];
            let opaque = over(self.outline, self.background);
            readable(opaque, self.text, &grounds, MARK_CONTRAST)
        } else {
            Self::new(self.design, self.appearance).outline
        };
        Self {
            outline,
            increased_contrast: increased,
            ..self
        }
    }

    pub fn with_status_colors(self, success: Color, warning: Color, critical: Color) -> Self {
        Self {
            success,
            warning,
            critical,
            ..self
        }
    }

    /// `radius` is the corner as drawn in Big Sur on a shape `height` tall. Other
    /// designs reshape it: smaller in Flat + Vibrancy, larger or a capsule in Liquid
    /// Glass, fixed per Windows design (square in Metro), and tight in Noa.
    pub fn corner(&self, corner: Corner, radius: f32, height: f32) -> f32 {
        match (self.design, corner) {
            (Design::Metro, _) => 0.0,
            (Design::Aero, Corner::Control) => 3.0,
            (Design::Aero, Corner::Inner) => 2.0,
            (Design::Aero, Corner::Container) => 6.0,
            (Design::Fluent2, Corner::Control | Corner::Inner) => 4.0_f32.min(height / 2.0),
            (Design::Fluent2, Corner::Container) => 8.0,
            (Design::Noa, Corner::Control) => 4.0_f32.min(height / 2.0),
            (Design::Noa, Corner::Inner) => 2.0_f32.min(height / 2.0),
            (Design::Noa, Corner::Container) => 8.0,
            (Design::BigSur, _) => radius,
            (Design::FlatVibrancy, _) => (radius * 0.6).round(),
            (Design::LiquidGlass, Corner::Control) => height / 2.0,
            (Design::LiquidGlass, Corner::Inner) => (radius * 1.6).min(height / 2.0),
            (Design::LiquidGlass, Corner::Container) => radius * 1.8,
        }
    }

    /// Hover, pressed, focus ring, selection and on-accent colors are derived from the
    /// accent.
    ///
    /// Text on the accent is white, or dark ink when white would fall below 4.5:1
    /// (WCAG 1.4.3), and stays as legible on the hover and pressed fills. The focus ring
    /// is made just opaque enough, or pulled toward the text color, to reach 3:1 against
    /// the background and surfaces it is drawn on (WCAG 1.4.11).
    pub fn with_accent(self, accent: Color) -> Self {
        let derived = Self::accented(self.appearance, accent);
        let grounds = [self.background, over(self.surface, self.background)];
        Self {
            accent: derived.accent,
            accent_hover: derived.accent_hover,
            accent_pressed: derived.accent_pressed,
            focus_ring: focus_ring_on(accent, self.text, &grounds),
            selection: derived.selection,
            on_accent: on_fill(accent),
            ..self
        }
    }

    /// Only the accent-derived fields are meaningful; the on-accent color and focus
    /// ring are left for [`Self::with_accent`], which knows the background.
    fn accented(appearance: Appearance, accent: Color) -> Self {
        let (lighten, darken, selection) = match appearance {
            Appearance::Light => (0.15, 0.2, 0.25),
            Appearance::Dark => (0.2, 0.25, 0.45),
        };
        let (hover, pressed) = state_fills(accent, on_fill(accent), lighten, darken);
        Self {
            design: Design::BigSur,
            appearance,
            background: Color::TRANSPARENT,
            surface: Color::TRANSPARENT,
            surface_alt: Color::TRANSPARENT,
            text: Color::TRANSPARENT,
            muted: Color::TRANSPARENT,
            accent,
            accent_hover: hover,
            accent_pressed: pressed,
            outline: Color::TRANSPARENT,
            separator: Color::TRANSPARENT,
            // Derived in `with_accent`, which knows the background.
            on_accent: Color::TRANSPARENT,
            focus_ring: Color::TRANSPARENT,
            selection: accent.with_alpha(selection),
            success: LEVEL_NORMAL,
            warning: LEVEL_WARNING,
            critical: LEVEL_CRITICAL,
            disabled_opacity: 1.0,
            increased_contrast: false,
        }
    }

    pub fn color(&self, role: Role) -> Color {
        match role {
            Role::Background => self.background,
            Role::Surface => self.surface,
            Role::SurfaceAlt => self.surface_alt,
            Role::Text => self.text,
            Role::Muted => self.muted,
            Role::Accent => self.accent,
            Role::AccentHover => self.accent_hover,
            Role::AccentPressed => self.accent_pressed,
            Role::OnAccent => self.on_accent,
            Role::Outline => self.outline,
            Role::Separator => self.separator,
            Role::FocusRing => self.focus_ring,
            Role::Selection => self.selection,
        }
    }

    pub fn resolve(&self, paint: Paint) -> Color {
        match paint {
            Paint::Fixed(color) => color,
            Paint::Role(role) => self.color(role),
        }
    }
}

const NOA_CHARCOAL: Color = Color::hex(0x1b1d21);
const NOA_EMBER: Color = Color::hex(0xf7f7f2);
const NOA_PAPER: Color = Color::hex(0xeeede6);

const NOA_SUCCESS: Color = Color::hex(0x7fc77a);
const NOA_WARNING: Color = Color::hex(0xf2d06b);
const NOA_CRITICAL: Color = Color::hex(0xf2685c);

/// Text drawn on fills too light for white: near-black, as in macOS.
pub const DARK_INK: Color = Color::hex(0x1d1d1f);

/// Minimum contrast for text (WCAG 1.4.3, AA).
pub const TEXT_CONTRAST: f32 = 4.5;
/// Minimum contrast for focus rings and other marks that identify a control's state
/// (WCAG 1.4.11, AA).
pub const MARK_CONTRAST: f32 = 3.0;

/// WCAG 2 relative luminance of an opaque sRGB color.
fn luminance(c: Color) -> f32 {
    let lin = |v: f32| {
        if v <= 0.040_45 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

/// WCAG 2 contrast ratio between two opaque colors, from 1 (same) to 21 (black on
/// white). Alpha is ignored; composite translucent colors with [`over`] first.
pub fn contrast(a: Color, b: Color) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// `fg` composited over `bg` (straight alpha), keeping `bg`'s alpha.
pub fn over(fg: Color, bg: Color) -> Color {
    mix(bg, fg, fg.a)
}

/// White, or [`DARK_INK`] when white text would fall below [`TEXT_CONTRAST`] on
/// `fill` and the ink reads better (yellow, green, light accents).
pub fn on_fill(fill: Color) -> Color {
    let white = contrast(Color::WHITE, fill);
    if white >= TEXT_CONTRAST || white >= contrast(DARK_INK, fill) {
        Color::WHITE
    } else {
        DARK_INK
    }
}

/// `color` pulled toward `toward` just far enough (in 5% steps) to reach `min`
/// contrast against every opaque color in `grounds`; `toward` itself if no step does.
pub fn readable(color: Color, toward: Color, grounds: &[Color], min: f32) -> Color {
    (0..=20)
        .map(|step| mix(color, toward, step as f32 / 20.0))
        .find(|c| grounds.iter().all(|&g| contrast(over(*c, g), g) >= min))
        .unwrap_or(toward)
}

/// The hover and pressed fills of `accent`, which carries `ink` (its [`on_fill`]).
///
/// As in macOS, hover lightens by `lighten` and pressed darkens by `darken`, unless
/// that step would leave `ink` less legible than on the accent itself: below
/// [`TEXT_CONTRAST`], or below the accent's own contrast for a mid-tone that reaches
/// 4.5:1 with neither ink. Such a step goes the other way, away from the ink, which
/// only raises the contrast; pressed then goes on past hover so the two still differ.
fn state_fills(accent: Color, ink: Color, lighten: f32, darken: f32) -> (Color, Color) {
    let floor = contrast(ink, accent).min(TEXT_CONTRAST);
    let keeps = |c: Color| contrast(ink, c) >= floor;
    let (lighter, darker) = (
        mix(accent, Color::WHITE, lighten),
        mix(accent, Color::BLACK, darken),
    );
    match (keeps(lighter), keeps(darker)) {
        (true, true) => (lighter, darker),
        // Light ink (white): both darken.
        (false, _) => (
            mix(accent, Color::BLACK, lighten),
            mix(accent, Color::BLACK, lighten + darken),
        ),
        // Dark ink: both lighten.
        (true, false) => (lighter, mix(accent, Color::WHITE, lighten + darken)),
    }
}

/// The focus ring for `accent`: the accent at 55% (the macOS look) made more opaque,
/// then pulled toward `text`, until it reaches [`MARK_CONTRAST`] on every ground.
fn focus_ring_on(accent: Color, text: Color, grounds: &[Color]) -> Color {
    let meets = |c: Color| {
        grounds
            .iter()
            .all(|&g| contrast(over(c, g), g) >= MARK_CONTRAST)
    };
    (11..=20)
        .map(|step| accent.with_alpha(step as f32 / 20.0))
        .find(|&c| meets(c))
        .unwrap_or_else(|| readable(accent, text, grounds, MARK_CONTRAST))
}

/// Linear blend from `a` to `b` by `t` (alpha taken from `a`).
pub fn mix(a: Color, b: Color, t: f32) -> Color {
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_resolve_per_appearance() {
        let (light, dark) = (Theme::light(), Theme::dark());
        assert_ne!(light.color(Role::Text), dark.color(Role::Text));
        assert_eq!(light.resolve(Role::Surface.into()), Color::WHITE);
        assert_eq!(dark.resolve(Color::BLACK.into()), Color::BLACK);
    }

    #[test]
    fn accent_derives_related_colors() {
        let red = Color::hex(0xd6455d);
        let theme = Theme::dark().with_accent(red);
        assert_eq!(theme.appearance, Appearance::Dark);
        assert_eq!(theme.accent, red);
        assert_eq!(theme.text, Theme::dark().text);
        // The ring keeps the accent's hue, made opaque enough to be seen (3:1).
        assert_eq!(theme.focus_ring.to_rgba8()[..3], red.to_rgba8()[..3]);
        assert!(theme.focus_ring.a >= 0.55);
        assert_ne!(theme.accent_pressed, red);
    }

    #[test]
    fn design_keeps_appearance_and_accent() {
        let red = Color::hex(0xd6455d);
        let theme = Theme::dark()
            .with_accent(red)
            .with_design(Design::LiquidGlass);
        assert_eq!(theme.design, Design::LiquidGlass);
        assert_eq!(theme.appearance, Appearance::Dark);
        assert_eq!(theme.accent, red);
        assert_ne!(theme.surface, Theme::dark().surface);
        assert_eq!(Theme::new(Design::BigSur, Appearance::Dark), Theme::dark());
        let light = theme.with_appearance(Appearance::Light);
        assert_eq!(light.design, Design::LiquidGlass);
        assert_eq!(light.accent, red);
    }

    #[test]
    fn every_theme_meets_wcag_contrast() {
        let mut failures = Vec::new();
        for design in Design::ALL {
            for appearance in [Appearance::Light, Appearance::Dark] {
                let t = Theme::new(design, appearance);
                let grounds = [
                    ("background", t.background),
                    ("surface", over(t.surface, t.background)),
                    ("surface_alt", over(t.surface_alt, t.background)),
                ];
                let mut check = |what: &str, ratio: f32, min: f32| {
                    if ratio < min {
                        failures.push(format!("{design:?} {appearance:?} {what}: {ratio:.2}"));
                    }
                };
                for (name, ground) in grounds {
                    check(
                        &format!("text on {name}"),
                        contrast(t.text, ground),
                        TEXT_CONTRAST,
                    );
                    check(
                        &format!("muted on {name}"),
                        contrast(t.muted, ground),
                        TEXT_CONTRAST,
                    );
                }
                for (name, ground) in &grounds[..2] {
                    let ring = contrast(over(t.focus_ring, *ground), *ground);
                    check(&format!("focus ring on {name}"), ring, MARK_CONTRAST);
                }
                check("on_accent", contrast(t.on_accent, t.accent), TEXT_CONTRAST);
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    fn increased_contrast_borders_meet_wcag_and_survive_theme_changes() {
        for design in Design::ALL {
            for appearance in [Appearance::Light, Appearance::Dark] {
                let base = Theme::new(design, appearance);
                let t = base.clone().with_increased_contrast(true);
                for ground in [t.background, over(t.surface, t.background)] {
                    let ratio = contrast(over(t.outline, ground), ground);
                    assert!(
                        ratio >= MARK_CONTRAST,
                        "{design:?} {appearance:?} outline: {ratio:.2}"
                    );
                }
                assert_eq!(
                    Theme {
                        outline: base.outline,
                        increased_contrast: false,
                        ..t.clone()
                    },
                    base
                );
                assert_eq!(t.clone().with_increased_contrast(false), base);
                let other = match appearance {
                    Appearance::Light => Appearance::Dark,
                    Appearance::Dark => Appearance::Light,
                };
                assert!(t.clone().with_appearance(other).increased_contrast);
                assert!(t.clone().with_design(Design::Metro).increased_contrast);
                let accented = t.clone().with_accent(Color::hex(0x2f9e5b));
                assert_eq!(accented.outline, t.outline);
                assert_eq!(
                    t.clone().with_appearance(other).with_appearance(appearance),
                    t
                );
            }
        }
    }

    #[test]
    fn derived_colors_follow_any_accent() {
        let yellow = Theme::light().with_accent(Color::hex(0xffcc00));
        assert_eq!(yellow.on_accent, DARK_INK);
        assert_eq!(Theme::light().on_accent, Color::WHITE);
        for accent in [0xffcc00, 0xc0ffee, 0x777777] {
            for theme in [Theme::light(), Theme::dark()] {
                let theme = theme.with_accent(Color::hex(accent));
                let bg = theme.background;
                assert!(contrast(over(theme.focus_ring, bg), bg) >= MARK_CONTRAST);
                // The more legible of white and dark ink (a mid gray reaches
                // 4.5:1 with neither).
                let best =
                    contrast(Color::WHITE, theme.accent).max(contrast(DARK_INK, theme.accent));
                assert_eq!(contrast(theme.on_accent, theme.accent), best);
            }
        }
    }

    #[test]
    fn text_on_the_accent_stays_legible_when_hovered_and_pressed() {
        // The design defaults, the demo's accents, the system colors and mid-tones
        // that reach 4.5:1 with neither ink (pink, #007aff, purple, gray).
        let accents = [
            None,
            Some(0x8e5bd8),
            Some(0xd6455d),
            Some(0xe8812d),
            Some(0x2f9e5b),
            Some(0x007aff),
            Some(0xff3b30),
            Some(0x34c759),
            Some(0xaf52de),
            Some(0xff9500),
            Some(0x5856d6),
            Some(0x8e8e93),
            Some(0xffcc00),
            Some(0x111111),
            Some(0xf0f0f0),
        ];
        for design in Design::ALL {
            for appearance in [Appearance::Light, Appearance::Dark] {
                for accent in accents {
                    let base = Theme::new(design, appearance);
                    let t = match accent {
                        Some(hex) => base.with_accent(Color::hex(hex)),
                        None => base,
                    };
                    let rest = contrast(t.on_accent, t.accent);
                    let floor = rest.min(TEXT_CONTRAST);
                    for (state, fill) in [("hover", t.accent_hover), ("pressed", t.accent_pressed)]
                    {
                        let ratio = contrast(t.on_accent, fill);
                        assert!(
                            ratio >= floor,
                            "{design:?} {appearance:?} {accent:x?} {state}: {ratio:.2} < {floor:.2}"
                        );
                    }
                    assert!(
                        t.accent_hover != t.accent
                            && t.accent_pressed != t.accent
                            && t.accent_hover != t.accent_pressed,
                        "{design:?} {appearance:?} {accent:x?}"
                    );
                }
            }
        }
        let navy = Theme::light().with_accent(Color::hex(0x1a2b6d));
        assert_eq!(navy.accent_hover, mix(navy.accent, Color::WHITE, 0.15));
        assert_eq!(navy.accent_pressed, mix(navy.accent, Color::BLACK, 0.2));
    }

    #[test]
    fn contrast_matches_wcag() {
        assert!((contrast(Color::BLACK, Color::WHITE) - 21.0).abs() < 1e-3);
        assert_eq!(contrast(Color::WHITE, Color::WHITE), 1.0);
        // #767676 on white is the classic 4.54:1.
        assert!((contrast(Color::hex(0x767676), Color::WHITE) - 4.54).abs() < 0.01);
        let half = over(Color::BLACK.with_alpha(0.5), Color::WHITE);
        assert!((half.r - 0.5).abs() < 1e-6 && half.a == 1.0);
    }

    #[test]
    fn corners_follow_the_design() {
        let flat = Theme::light().with_design(Design::FlatVibrancy);
        let glass = Theme::light().with_design(Design::LiquidGlass);
        assert_eq!(Theme::light().corner(Corner::Control, 6.0, 28.0), 6.0);
        assert_eq!(flat.corner(Corner::Container, 10.0, 200.0), 6.0);
        assert_eq!(glass.corner(Corner::Control, 6.0, 28.0), 14.0);
        assert_eq!(glass.corner(Corner::Inner, 4.0, 16.0), 6.4);
        assert_eq!(glass.corner(Corner::Container, 10.0, 200.0), 18.0);
        let metro = Theme::light().with_design(Design::Metro);
        assert_eq!(metro.corner(Corner::Control, 6.0, 28.0), 0.0);
        assert_eq!(metro.pill(18.0), 0.0);
        let fluent = Theme::light().with_design(Design::Fluent2);
        assert_eq!(fluent.corner(Corner::Container, 10.0, 200.0), 8.0);
    }

    #[test]
    fn noa_wears_one_orange_accent_on_charcoal_and_on_ember_white() {
        let dark = Theme::new(Design::Noa, Appearance::Dark);
        let light = Theme::new(Design::Noa, Appearance::Light);
        assert_eq!(Design::Noa.name(), "Noa");
        assert!(!Design::Noa.windows_chrome());
        for theme in [&dark, &light] {
            assert_eq!(theme.accent, NOA_ACCENT);
            assert_eq!(theme.on_accent, DARK_INK);
            assert_ne!(theme.success, Theme::dark().success);
        }
        assert_eq!(dark.background, Color::hex(0x1b1d21));
        assert_eq!(dark.text, Color::hex(0xf7f7f2), "Ember's white");
        assert_eq!(light.text, dark.background);
        for theme in [&dark, &light] {
            assert!(theme.surface.a < 1.0 && theme.surface_alt.a < 1.0);
            assert_ne!(
                over(theme.surface, theme.background),
                over(theme.surface_alt, theme.background)
            );
        }
        let near = |c: Color, hex: u32| {
            let (a, b) = (c.to_rgba8(), Color::hex(hex).to_rgba8());
            (0..3).all(|i| a[i].abs_diff(b[i]) <= 4)
        };
        assert!(near(over(dark.surface, dark.background), 0x26262b));
        assert!(near(over(light.surface, light.background), 0xf7f7f2));
        assert_eq!(dark.pill(18.0), 2.0);
        assert_eq!(dark.pill(2.0), 1.0);
        assert_eq!(dark.corner(Corner::Control, 6.0, 28.0), 4.0);
        assert_eq!(dark.corner(Corner::Inner, 4.0, 16.0), 2.0);
        assert_eq!(dark.corner(Corner::Container, 10.0, 200.0), 8.0);
        assert_eq!(dark.clone().with_appearance(Appearance::Light), light);
    }

    #[test]
    fn a_design_s_own_colors_give_way_and_chosen_ones_follow() {
        let noa = Theme::dark().with_design(Design::Noa);
        assert_eq!(noa, Theme::new(Design::Noa, Appearance::Dark));
        assert_eq!(noa.clone().with_design(Design::BigSur), Theme::dark());
        let (red, a, b, c) = (
            Color::hex(0xd6455d),
            Color::hex(0x00aa00),
            Color::hex(0xaaaa00),
            Color::hex(0xaa0000),
        );
        let chosen = Theme::dark()
            .with_accent(red)
            .with_status_colors(a, b, c)
            .with_design(Design::Noa);
        assert_eq!(chosen.accent, red);
        assert_eq!((chosen.success, chosen.warning, chosen.critical), (a, b, c));
        let back = chosen.with_design(Design::Metro);
        assert_eq!(back.accent, red);
        assert_eq!(back.success, a);
    }

    #[test]
    fn windows_designs_bring_their_own_accent_and_chrome() {
        let fluent = Theme::new(Design::Fluent2, Appearance::Light);
        assert_eq!(fluent.accent, Color::hex(0x005fb8));
        assert!(Design::Fluent2.windows_chrome());
        assert!(!Design::BigSur.windows_chrome());
        let red = Color::hex(0xd6455d);
        assert_eq!(
            fluent.with_accent(red).with_design(Design::Aero).accent,
            red
        );
    }
}

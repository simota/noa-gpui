use ui_layout::style::{Rect as LayoutRect, Size as LayoutSize, length};
use ui_layout::{MeasureInput, Style};
use ui_scene::{Border, Color, Point, Rect, Size};
use ui_text::{TextStore, TextStyle};

use super::{CachedText, ControlSize, Icon};
use crate::animation::{Easing, Transition, lerp_color};
use crate::event::{Event, Key, PointerButton};
use crate::shortcut::Shortcut;
use crate::theme::{Corner, Role, mix};
use crate::widget::{EventCx, MeasureCx, PaintCx, UpdateCx, Widget};

/// Seconds for the hover fill to fade in or out.
const HOVER_FADE: f64 = 0.12;
const ICON: f32 = 16.0;
const ICON_GAP: f32 = 6.0;
const DETAIL_GAP: f32 = 6.0;

/// Emitted by a [`Button`] when it is activated by pointer, Space, its key equivalent
/// or assistive technology.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonClicked;

/// The bezel styles of macOS push buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonStyle {
    /// The standard rounded button. With [`Button::default_button`] it is filled with
    /// the accent color.
    #[default]
    Push,
    Gradient,
    Recessed,
    Inline,
    RoundedRect,
    Textured,
    Circular,
    Help,
}

/// A focusable push button. A default button is activated by Return and a cancel button
/// by Escape wherever focus is, unless the focused widget uses the key. Emits
/// [`ButtonClicked`].
#[derive(Debug)]
pub struct Button {
    label: String,
    icon: Option<Icon>,
    style: ButtonStyle,
    is_default: bool,
    key: Option<Shortcut>,
    size: ControlSize,
    pressed: bool,
    /// 0 (not hovered) to 1 (hovered), animated.
    hover: Transition,
    hover_amount: f32,
    shaped: CachedText,
    detail: String,
    detail_family: Option<String>,
    detail_shaped: CachedText,
    tooltip: Option<String>,
    selected: bool,
}

impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            style: ButtonStyle::Push,
            is_default: false,
            key: None,
            size: ControlSize::Regular,
            pressed: false,
            hover: Transition::new(0.0),
            hover_amount: 0.0,
            tooltip: None,
            shaped: CachedText::default(),
            detail: String::new(),
            detail_family: None,
            detail_shaped: CachedText::default(),
            selected: false,
        }
    }

    pub fn icon_only(icon: impl Into<Icon>) -> Self {
        Self::new("").icon(icon)
    }

    pub fn help() -> Self {
        Self::new("?").style(ButtonStyle::Help)
    }

    pub fn style(mut self, style: ButtonStyle) -> Self {
        self.style = style;
        self
    }

    pub fn control_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    fn text_style(&self) -> TextStyle {
        let (base, weight) = match self.style {
            ButtonStyle::Inline => (12.0, 600),
            ButtonStyle::Help => (13.0, 600),
            _ => (14.0, 500),
        };
        TextStyle::new(self.size.font(base)).weight(weight)
    }

    fn icon_size(&self) -> f32 {
        ICON * self.size.scale()
    }

    /// A vector icon is drawn in the label's color.
    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Accent-filled and activated by Return.
    pub fn default_button(mut self, is_default: bool) -> Self {
        self.is_default = is_default;
        self.key = is_default.then_some(Shortcut::key(Key::Enter));
        self
    }

    /// Activated by Escape.
    pub fn cancel_button(mut self, is_cancel: bool) -> Self {
        self.key = is_cancel.then_some(Shortcut::key(Key::Escape));
        self
    }

    pub fn shortcut(mut self, shortcut: Shortcut) -> Self {
        self.key = Some(shortcut);
        self
    }

    /// Drawn 2 points smaller than the label, in the muted color. Empty for none (the
    /// default).
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = detail.into();
        self
    }

    /// Defaults to the label's font.
    pub fn detail_font_family(mut self, family: impl Into<String>) -> Self {
        self.detail_family = Some(family.into());
        self
    }

    pub fn detail_text(&self) -> &str {
        &self.detail
    }

    pub fn set_detail(&mut self, cx: &mut UpdateCx, detail: impl Into<String>) {
        let detail = detail.into();
        if detail != self.detail {
            self.detail = detail;
            cx.request_layout();
        }
    }

    fn detail_style(&self, text: &mut TextStore) -> TextStyle {
        let style = TextStyle::new(self.size.font(12.0));
        match self.detail_family.as_deref() {
            Some(family) => style.family(text.font_family(family)),
            None => style,
        }
    }

    /// Filled with the selection color and framed in the accent color, whatever the style.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn is_selected(&self) -> bool {
        self.selected
    }

    pub fn set_selected(&mut self, cx: &mut UpdateCx, selected: bool) {
        if self.selected != selected {
            self.selected = selected;
            cx.request_paint();
        }
    }

    pub fn tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn set_label(&mut self, cx: &mut UpdateCx, label: impl Into<String>) {
        let label = label.into();
        if label != self.label {
            self.label = label;
            cx.request_layout();
        }
    }

    /// 0 to 1.
    pub fn hover_amount(&self) -> f32 {
        self.hover_amount
    }

    fn padding(&self) -> (f32, f32) {
        let icon_only = self.label.is_empty();
        let k = self.size.scale();
        let (x, y) = match self.style {
            ButtonStyle::Help => (0.0, 0.0),
            ButtonStyle::Circular => (7.0, 7.0),
            ButtonStyle::Inline => (8.0, 2.0),
            ButtonStyle::Gradient => (6.0, 4.0),
            _ if icon_only => (7.0, 6.0),
            ButtonStyle::RoundedRect => (10.0, 5.0),
            _ => (14.0, 6.0),
        };
        (x * k, y * k)
    }

    /// Fill, border and text roles before hover / press / disabled adjustments.
    fn colors(&self, cx: &PaintCx) -> (Color, Option<Color>, Color) {
        let surface = cx.color(Role::Surface);
        let alt = cx.color(Role::SurfaceAlt);
        let outline = cx.color(Role::Outline);
        let text = cx.color(Role::Text);
        let (base, border, ink) = if self.selected {
            (
                cx.color(Role::Selection),
                Some(cx.color(Role::Accent)),
                text,
            )
        } else {
            match self.style {
                ButtonStyle::Push if self.is_default => {
                    (cx.color(Role::Accent), None, cx.color(Role::OnAccent))
                }
                ButtonStyle::Push
                | ButtonStyle::RoundedRect
                | ButtonStyle::Circular
                | ButtonStyle::Help => (surface, Some(outline), text),
                ButtonStyle::Gradient | ButtonStyle::Textured => (alt, Some(outline), text),
                ButtonStyle::Recessed => (Color::TRANSPARENT, None, text),
                ButtonStyle::Inline => (cx.color(Role::Muted).with_alpha(0.18), None, text),
            }
        };
        let style = cx.styling();
        (
            style.background_color.map(|p| cx.color(p)).unwrap_or(base),
            style.border_color.map(|p| cx.color(p)).or(border),
            style.color.map(|p| cx.color(p)).unwrap_or(ink),
        )
    }

    fn radius(&self, cx: &PaintCx, bounds: Rect) -> f32 {
        if let Some(radius) = cx.styling().border_radius {
            return radius;
        }
        let (corner, radius) = match self.style {
            ButtonStyle::Push | ButtonStyle::Textured => (Corner::Control, 6.0),
            ButtonStyle::Recessed => (Corner::Control, 5.0),
            ButtonStyle::RoundedRect => (Corner::Control, 4.0),
            ButtonStyle::Gradient => (Corner::Inner, 2.0),
            ButtonStyle::Inline | ButtonStyle::Circular | ButtonStyle::Help => {
                return bounds.width.min(bounds.height) / 2.0;
            }
        };
        cx.corner(corner, radius, bounds.height)
    }

    fn accent_filled(&self) -> bool {
        self.is_default && self.style == ButtonStyle::Push && !self.selected
    }

    fn activate(&mut self, cx: &mut EventCx) {
        cx.emit(ButtonClicked);
        cx.set_handled();
    }
}

impl Widget for Button {
    fn inherits_style(&self) -> bool {
        false
    }

    fn paints_style_box(&self) -> bool {
        true
    }

    fn style(&self) -> Style {
        let (x, y) = self.padding();
        let mut style = Style {
            padding: LayoutRect {
                left: length(x),
                right: length(x),
                top: length(y),
                bottom: length(y),
            },
            flex_shrink: 0.0,
            ..Default::default()
        };
        if self.style == ButtonStyle::Help {
            style.size = LayoutSize {
                width: length(22.0 * self.size.scale()),
                height: length(22.0 * self.size.scale()),
            };
        }
        style
    }

    fn measures(&self) -> bool {
        true
    }

    fn measure(&mut self, cx: &mut MeasureCx, _input: MeasureInput) -> Size {
        let mut text_style = self.text_style();
        if let Some(size) = cx.styling().font_size {
            text_style.font_size = size;
            text_style.line_height = TextStyle::new(size).line_height;
        }
        if let Some(weight) = cx.styling().font_weight {
            text_style = text_style.weight(weight);
        }
        let text = if self.label.is_empty() {
            self.shaped.release(cx.text);
            Size::new(0.0, text_style.line_height)
        } else {
            self.shaped.shape(cx.text, &self.label, text_style, None)
        };
        let icon_size = self.icon_size();
        let mut size = text;
        if self.detail.is_empty() {
            self.detail_shaped.release(cx.text);
        } else {
            let style = self.detail_style(cx.text);
            let detail = self.detail_shaped.shape(cx.text, &self.detail, style, None);
            let gap = if self.label.is_empty() {
                0.0
            } else {
                DETAIL_GAP
            };
            size.width += gap + detail.width;
            size.height = size.height.max(detail.height);
        }
        if self.icon.is_some() {
            let gap = if self.label.is_empty() { 0.0 } else { ICON_GAP };
            size.width += icon_size + gap;
            size.height = size.height.max(icon_size);
        }
        if self.style == ButtonStyle::Circular {
            let side = size.width.max(size.height);
            size = Size::new(side, side);
        }
        size
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let (fill, border, ink) = self.colors(cx);
        let text = ink;
        let hover_target = if cx.styling().background_color.is_some() {
            fill
        } else if self.selected {
            mix(fill, text, 0.06)
        } else if self.accent_filled() {
            cx.color(Role::AccentHover)
        } else if self.style == ButtonStyle::Recessed {
            cx.color(Role::SurfaceAlt)
        } else {
            mix(fill, text, 0.06).with_alpha(fill.a.max(0.0))
        };
        let mut fill = lerp_color(fill, hover_target, self.hover_amount);
        if self.pressed {
            fill = if self.accent_filled() {
                cx.color(Role::AccentPressed)
            } else {
                let base = if fill.a == 0.0 {
                    cx.color(Role::SurfaceAlt)
                } else {
                    fill
                };
                mix(base, text, 0.12)
            };
        }
        let mut border = border.map_or(Border::default(), |color| Border { width: 1.0, color });
        if let Some(width) = cx.styling().border_width {
            border.width = width;
            if border.color.a == 0.0 && width > 0.0 {
                border.color = cx.color(Role::Outline);
            }
        }
        let radius = self.radius(cx, bounds);
        cx.bezel(bounds, radius, fill, border);

        let text_size = self.shaped.size();
        let has_text = !self.label.is_empty() && self.shaped.layout().is_some();
        let detail = (self.detail_shaped.layout())
            .filter(|_| !self.detail.is_empty())
            .map(|layout| (layout, self.detail_shaped.size()));
        let detail_gap = if has_text { DETAIL_GAP } else { 0.0 };
        let icon_size = self.icon_size();
        let content_width = if has_text { text_size.width } else { 0.0 }
            + detail.map_or(0.0, |(_, size)| detail_gap + size.width)
            + if self.icon.is_some() {
                icon_size + if has_text { ICON_GAP } else { 0.0 }
            } else {
                0.0
            };
        let mut x = (bounds.width - content_width) / 2.0;
        if let Some(icon) = &self.icon {
            let rect = Rect::new(x, (bounds.height - icon_size) / 2.0, icon_size, icon_size);
            icon.paint(cx, rect, ink);
            x += icon_size + ICON_GAP;
        }
        if has_text && let Some(layout) = self.shaped.layout() {
            let y = (bounds.height - text_size.height) / 2.0;
            cx.text(
                Rect::new(x, y, text_size.width, text_size.height),
                layout,
                ink,
            );
            x += text_size.width;
        }
        if let Some((layout, size)) = detail {
            // Muted unless on an accent fill, where muted gray would not read.
            let muted = if self.accent_filled() {
                ink.with_alpha(ink.a * 0.75)
            } else {
                cx.color(Role::Muted)
            };
            let y = (bounds.height - size.height) / 2.0;
            let rect = Rect::new(x + detail_gap, y, size.width, size.height);
            cx.text(rect, layout, muted);
        }
    }

    fn paint_over(&mut self, cx: &mut PaintCx) {
        if cx.is_focused() {
            let b = cx.bounds();
            let radius = self.radius(cx, b) + 3.0;
            let ring = cx.color(Role::FocusRing);
            cx.rounded_rect(
                Rect::new(b.x - 3.0, b.y - 3.0, b.width + 6.0, b.height + 6.0),
                radius,
                Color::TRANSPARENT,
                Border {
                    width: 3.0,
                    color: ring,
                },
            );
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            Event::HoverChanged(hovered) => {
                let target = if *hovered { 1.0 } else { 0.0 };
                self.hover
                    .animate_to(target, cx.now(), cx.motion(HOVER_FADE), Easing::EaseOut);
                cx.request_animation_frame();
            }
            Event::AnimationFrame { now } => {
                self.hover_amount = self.hover.value(*now);
                cx.request_paint();
                if !self.hover.is_done(*now) {
                    cx.request_animation_frame();
                }
            }
            Event::PointerDown {
                button: PointerButton::Primary,
                ..
            } => {
                self.pressed = true;
                cx.capture_pointer();
                cx.request_paint();
                cx.set_handled();
            }
            Event::PointerUp {
                pos,
                button: PointerButton::Primary,
            } if self.pressed => {
                self.pressed = false;
                cx.release_pointer();
                cx.request_paint();
                if cx.contains(*pos) {
                    cx.emit(ButtonClicked);
                }
                cx.set_handled();
            }
            Event::KeyDown {
                key: Key::Space, ..
            } => self.activate(cx),
            Event::Activate => self.activate(cx),
            _ => {}
        }
    }

    fn tooltip(&self, _local: Point) -> Option<String> {
        self.tooltip.clone()
    }

    fn focusable(&self) -> bool {
        true
    }

    fn key_equivalent(&self) -> Option<Shortcut> {
        self.key
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Button)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        let label = if self.style == ButtonStyle::Help {
            "Help".to_owned()
        } else {
            self.label.clone()
        };
        if self.detail.is_empty() {
            node.set_label(label);
        } else {
            node.set_label(format!("{label} {}", self.detail));
        }
        if self.selected {
            node.set_selected(true);
        }
        node.add_action(accesskit::Action::Click);
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.shaped.release(text);
        self.detail_shaped.release(text);
    }
}

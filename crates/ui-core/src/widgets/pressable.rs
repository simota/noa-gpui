use ui_layout::Style;
use ui_scene::{Border, Color, Point, Rect, Size};

use super::ButtonClicked;
use crate::animation::{Easing, Transition, lerp_color};
use crate::event::{Event, Key, PointerButton};
use crate::shortcut::Shortcut;
use crate::theme::{Corner, Role, mix};
use crate::widget::{EventCx, PaintCx, UpdateCx, Widget};

/// Seconds for the hover fill to fade in or out (as a [`Button`](super::Button)'s).
const HOVER_FADE: f64 = 0.12;
const RADIUS: f32 = 5.0;

/// A box pressed as a whole; its children only show. Emits [`ButtonClicked`]. Give it a
/// [`Pressable::label`], since its children are not announced.
#[derive(Debug)]
pub struct Pressable {
    style: Style,
    label: String,
    tooltip: Option<String>,
    key: Option<Shortcut>,
    selected: bool,
    radius: f32,
    pressed: bool,
    hover: Transition,
    hover_amount: f32,
}

impl Pressable {
    pub fn new(style: Style) -> Self {
        Self {
            style,
            label: String::new(),
            tooltip: None,
            key: None,
            selected: false,
            radius: RADIUS,
            pressed: false,
            hover: Transition::new(0.0),
            hover_amount: 0.0,
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    pub fn tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    pub fn shortcut(mut self, shortcut: Shortcut) -> Self {
        self.key = Some(shortcut);
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Corner radius before the theme's corner style is applied (5 by default).
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
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

    pub fn label_text(&self) -> &str {
        &self.label
    }

    pub fn set_label(&mut self, cx: &mut UpdateCx, label: impl Into<String>) {
        let label = label.into();
        if label != self.label {
            self.label = label;
            cx.request_paint();
        }
    }

    pub fn set_tooltip(&mut self, tooltip: Option<String>) {
        self.tooltip = tooltip;
    }

    /// Fraction of the hover fill currently shown (0 to 1).
    pub fn hover_amount(&self) -> f32 {
        self.hover_amount
    }

    fn activate(&mut self, cx: &mut EventCx) {
        cx.emit(ButtonClicked);
        cx.set_handled();
    }
}

impl Widget for Pressable {
    fn style(&self) -> Style {
        self.style.clone()
    }

    fn claims_pointer(&self, _local: Point, _size: Size) -> bool {
        true
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let text = cx.color(Role::Text);
        let (fill, border) = if self.selected {
            let border = Border {
                width: 1.0,
                color: cx.color(Role::Accent),
            };
            (cx.color(Role::Selection), border)
        } else {
            (Color::TRANSPARENT, Border::default())
        };
        let hover_target = if self.selected {
            mix(fill, text, 0.06)
        } else {
            cx.color(Role::SurfaceAlt)
        };
        let mut fill = lerp_color(fill, hover_target, self.hover_amount);
        if self.pressed {
            let base = if fill.a == 0.0 {
                cx.color(Role::SurfaceAlt)
            } else {
                fill
            };
            fill = mix(base, text, 0.12);
        }
        if fill.a == 0.0 && border.width == 0.0 {
            return;
        }
        let radius = cx.corner(Corner::Control, self.radius, bounds.height);
        cx.rounded_rect(bounds, radius, fill, border);
    }

    fn paint_over(&mut self, cx: &mut PaintCx) {
        if cx.is_focused() {
            let b = cx.bounds();
            let radius = cx.corner(Corner::Control, self.radius, b.height) + 3.0;
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
        if !self.label.is_empty() {
            node.set_label(self.label.clone());
        }
        if self.selected {
            node.set_selected(true);
        }
        node.add_action(accesskit::Action::Click);
    }
}

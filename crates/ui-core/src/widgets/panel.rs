use ui_layout::Style;
use ui_layout::style::{FlexDirection, Rect as LayoutRect, Size as LayoutSize, auto, length};
use ui_scene::{Border, Color, Point, Rect};
use ui_text::{TextStore, TextStyle};

use super::CachedText;
use super::appear::{Appear, shadow};
use crate::event::{Event, PointerButton};
use crate::overlay::{OverlayOptions, Placement};
use crate::theme::{Corner, Role};
use crate::tree::{UiTree, WidgetId};
use crate::widget::{EventCx, PaintCx, Widget};

pub const PANEL_TITLE_BAR: f32 = 26.0;
const RADIUS: f32 = 10.0;
const CLOSE: f32 = 13.0;
const FADE: f64 = 0.15;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PanelStyle {
    #[default]
    Utility,
    /// The same in light and dark mode; give its content light colors.
    Hud,
}

/// Emitted (with the panel as the source) when the user closes a panel with its close
/// button. The panel then fades out and removes itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelClosed;

/// A floating panel (`NSPanel`) that does not block the window under it. Emits
/// [`PanelClosed`].
#[derive(Debug)]
pub struct Panel {
    title: String,
    style: PanelStyle,
    width: f32,
    title_text: CachedText,
    /// The last pointer position (window coordinates) while dragging the title bar.
    dragging: Option<Point>,
    close_pressed: bool,
    close_hovered: bool,
    appear: Appear,
    closed: bool,
}

impl Panel {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            style: PanelStyle::default(),
            width: 240.0,
            title_text: CachedText::default(),
            dragging: None,
            close_pressed: false,
            close_hovered: false,
            appear: Appear::new(FADE),
            closed: false,
        }
    }

    pub fn style(mut self, style: PanelStyle) -> Self {
        self.style = style;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn panel_style(&self) -> PanelStyle {
        self.style
    }

    /// Open with its top-left corner at `at` (window coordinates), fading in.
    pub fn open(self, tree: &mut UiTree, at: Point) -> WidgetId {
        let id = tree.open_overlay(self, Placement::At(at), OverlayOptions::PASSIVE);
        tree.update::<Panel, _>(id, |_, cx| cx.request_animation_frame());
        id
    }

    fn close_rect() -> Rect {
        Rect::new(9.0, (PANEL_TITLE_BAR - CLOSE) / 2.0, CLOSE, CLOSE)
    }

    fn close(&mut self, cx: &mut EventCx) {
        if self.closed {
            return;
        }
        self.closed = true;
        cx.emit(PanelClosed);
        self.appear.close(cx.now());
        cx.request_animation_frame();
    }

    fn hud(&self) -> bool {
        self.style == PanelStyle::Hud
    }
}

fn title_style() -> TextStyle {
    TextStyle::new(12.0).weight(600)
}

impl Widget for Panel {
    fn style(&self) -> Style {
        Style {
            flex_direction: FlexDirection::Column,
            gap: LayoutSize {
                width: length(0.0),
                height: length(8.0),
            },
            padding: LayoutRect {
                left: length(12.0),
                right: length(12.0),
                top: length(PANEL_TITLE_BAR + 6.0),
                bottom: length(12.0),
            },
            size: LayoutSize {
                width: length(self.width),
                height: auto(),
            },
            ..Default::default()
        }
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let radius = cx.corner(Corner::Container, RADIUS, bounds.height);
        shadow(cx, bounds, radius);
        let ink = if self.hud() {
            Color::rgba(1.0, 1.0, 1.0, 0.92)
        } else {
            cx.color(Role::Text)
        };
        if self.hud() {
            cx.backdrop(bounds, radius, 24.0, Color::rgba(0.08, 0.08, 0.09, 0.78));
            cx.rounded_rect(
                bounds,
                radius,
                Color::TRANSPARENT,
                Border {
                    width: 1.0,
                    color: Color::rgba(1.0, 1.0, 1.0, 0.16),
                },
            );
        } else {
            let fill = cx.color(Role::Surface);
            let border = Border {
                width: 1.0,
                color: cx.color(Role::Outline),
            };
            cx.floating_surface(bounds, radius, fill, border);
            let line = cx.color(Role::Separator);
            cx.fill_rect(Rect::new(0.0, PANEL_TITLE_BAR, bounds.width, 1.0), line);
        }
        let size = self
            .title_text
            .shape(cx.text_store(), &self.title, title_style(), None);
        if let Some(layout) = self.title_text.layout() {
            let x = ((bounds.width - size.width) / 2.0).max(Self::close_rect().right() + 6.0);
            let y = (PANEL_TITLE_BAR - size.height) / 2.0;
            cx.text(Rect::new(x, y, size.width, size.height), layout, ink);
        }
        let close = Self::close_rect();
        let fill = match (self.hud(), self.close_hovered && self.close_pressed) {
            (true, false) => Color::rgba(1.0, 1.0, 1.0, 0.22),
            (true, true) => Color::rgba(1.0, 1.0, 1.0, 0.4),
            (false, false) => cx.color(Role::SurfaceAlt),
            (false, true) => super::pressed_tint(cx, cx.color(Role::SurfaceAlt)),
        };
        cx.rounded_rect(close, CLOSE / 2.0, fill, Border::default());
        let k = 3.2;
        let (cx0, cy0) = (close.x + CLOSE / 2.0, close.y + CLOSE / 2.0);
        let mark = if self.hud() {
            ink
        } else {
            cx.color(Role::Muted)
        };
        cx.line(
            Point::new(cx0 - k, cy0 - k),
            Point::new(cx0 + k, cy0 + k),
            1.4,
            mark,
        );
        cx.line(
            Point::new(cx0 - k, cy0 + k),
            Point::new(cx0 + k, cy0 - k),
            1.4,
            mark,
        );
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            Event::AnimationFrame { now } => {
                if self.appear.frame(cx, *now) {
                    cx.close_overlay();
                }
                cx.request_composite();
            }
            _ if self.closed => {}
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                let local = cx.local(*pos);
                if local.y >= PANEL_TITLE_BAR {
                    return;
                }
                cx.set_handled();
                cx.capture_pointer();
                if Self::close_rect().contains(local) {
                    self.close_pressed = true;
                    self.close_hovered = true;
                    cx.request_paint();
                } else {
                    self.dragging = Some(*pos);
                }
            }
            Event::PointerMove { pos } => {
                if let Some(last) = self.dragging {
                    cx.move_overlay(Point::new(pos.x - last.x, pos.y - last.y));
                    self.dragging = Some(*pos);
                    cx.set_handled();
                } else if self.close_pressed {
                    let over = Self::close_rect().contains(cx.local(*pos));
                    if over != self.close_hovered {
                        self.close_hovered = over;
                        cx.request_paint();
                    }
                }
            }
            Event::PointerUp { pos, .. } => {
                if self.dragging.take().is_some() {
                    cx.release_pointer();
                    cx.set_handled();
                } else if self.close_pressed {
                    self.close_pressed = false;
                    cx.release_pointer();
                    cx.set_handled();
                    cx.request_paint();
                    if Self::close_rect().contains(cx.local(*pos)) {
                        self.close(cx);
                    }
                }
            }
            Event::ActivateItem(0) => self.close(cx),
            _ => {}
        }
    }

    fn opacity(&self) -> f32 {
        self.appear.value()
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Window)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        node.set_label(self.title.clone());
    }

    fn accessibility_items(&self) -> Vec<accesskit::Node> {
        let r = Self::close_rect();
        let mut close = accesskit::Node::new(accesskit::Role::Button);
        close.set_label("Close");
        close.set_bounds(accesskit::Rect {
            x0: r.x as f64,
            y0: r.y as f64,
            x1: r.right() as f64,
            y1: r.bottom() as f64,
        });
        close.add_action(accesskit::Action::Click);
        vec![close]
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.title_text.release(text);
    }
}

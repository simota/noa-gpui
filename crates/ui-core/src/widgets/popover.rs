use noa_gpui_layout::Style;
use noa_gpui_layout::style::{FlexDirection, Rect as LayoutRect, Size as LayoutSize, length};
use noa_gpui_scene::{Border, Direction, Rect};

use super::appear::{Appear, shadow};
use crate::event::Event;
use crate::overlay::{Edge, OverlayOptions, Placement};
use crate::theme::{Corner, Role};
use crate::tree::{UiTree, WidgetId};
use crate::widget::{EventCx, PaintCx, Widget};

pub const POPOVER_ARROW: f32 = 9.0;
const RADIUS: f32 = 10.0;
const FADE: f64 = 0.15;

/// A popover (`NSPopover`). A press outside or Escape closes it and emits
/// [`crate::OverlayDismissed`]; the press still reaches what is under the pointer.
#[derive(Debug)]
pub struct Popover {
    padding: f32,
    closes_on_click: bool,
    /// The anchor in window coordinates.
    anchor: Option<Rect>,
    /// Where the overlay was placed, in window coordinates.
    placed: Rect,
    appear: Appear,
}

impl Default for Popover {
    fn default() -> Self {
        Self::new()
    }
}

impl Popover {
    pub fn new() -> Self {
        Self {
            padding: 14.0,
            closes_on_click: false,
            anchor: None,
            placed: Rect::default(),
            appear: Appear::new(FADE),
        }
    }

    pub fn padding(mut self, padding: f32) -> Self {
        self.padding = padding;
        self
    }

    /// Close when a button in the popover is clicked (off by default). The button's
    /// [`super::ButtonClicked`] comes first, then [`crate::OverlayDismissed`] from the
    /// popover, as for a click outside.
    pub fn closes_on_click(mut self, closes: bool) -> Self {
        self.closes_on_click = closes;
        self
    }

    pub(crate) fn closes_when_clicked(&self) -> bool {
        self.closes_on_click
    }

    /// The rectangle (window coordinates) the arrow points at.
    pub fn anchor(mut self, anchor: Rect) -> Self {
        self.anchor = Some(anchor);
        self
    }

    /// Open on the `edge` side of `anchor` (window coordinates), fading in.
    pub fn open(self, tree: &mut UiTree, anchor: Rect, edge: Edge) -> WidgetId {
        let id = tree.open_overlay(
            self.anchor(anchor),
            Placement::Edge { anchor, edge },
            OverlayOptions::POPOVER,
        );
        tree.update::<Popover, _>(id, |_, cx| cx.request_animation_frame());
        id
    }

    /// Open on the `edge` side of widget `anchor` (a button, a [`super::Pressable`]),
    /// pointing at it. A press on `anchor` while the popover is open closes it without
    /// reaching `anchor`, so the click that would open it again toggles it closed, as
    /// a popover's button does on macOS. `anchor` must have been laid out.
    pub fn open_from(self, tree: &mut UiTree, anchor: WidgetId, edge: Edge) -> WidgetId {
        let rect = tree.window_rect(anchor).unwrap_or_default();
        let id = self.open(tree, rect, edge);
        tree.set_overlay_anchor(id, anchor);
        id
    }

    pub fn edge(&self) -> Option<Edge> {
        let anchor = self.anchor?;
        let p = self.placed;
        Some(if p.y >= anchor.bottom() - 0.5 {
            Edge::Bottom
        } else if p.bottom() <= anchor.y + 0.5 {
            Edge::Top
        } else if p.x >= anchor.right() - 0.5 {
            Edge::Right
        } else {
            Edge::Left
        })
    }
}

impl Widget for Popover {
    fn style(&self) -> Style {
        Style {
            flex_direction: FlexDirection::Column,
            gap: LayoutSize {
                width: length(0.0),
                height: length(8.0),
            },
            padding: LayoutRect::length(POPOVER_ARROW + self.padding),
            ..Default::default()
        }
    }

    fn overlay_placed(&mut self, rect: Rect) -> bool {
        let moved = rect != self.placed;
        self.placed = rect;
        moved && self.anchor.is_some()
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let a = POPOVER_ARROW;
        let bubble = Rect::new(a, a, bounds.width - 2.0 * a, bounds.height - 2.0 * a);
        let radius = cx.corner(Corner::Container, RADIUS, bubble.height);
        shadow(cx, bubble, radius);
        let fill = cx.color(Role::Surface);
        let outline = cx.color(Role::Outline);
        // The arrow: an outline triangle under the bubble, then a fill over the
        // bubble's border so the two read as one shape.
        let arrow = self.anchor.zip(self.edge()).map(|(anchor, edge)| {
            let (min, max) = (radius + a, |len: f32| len - radius - a);
            let along_x = (anchor.x + anchor.width / 2.0 - self.placed.x)
                .clamp(min, max(bounds.width).max(min));
            let along_y = (anchor.y + anchor.height / 2.0 - self.placed.y)
                .clamp(min, max(bounds.height).max(min));
            match edge {
                Edge::Bottom => (Rect::new(along_x - a, 0.0, 2.0 * a, a + 1.0), Direction::Up),
                Edge::Top => (
                    Rect::new(along_x - a, bounds.height - a - 1.0, 2.0 * a, a + 1.0),
                    Direction::Down,
                ),
                Edge::Right => (
                    Rect::new(0.0, along_y - a, a + 1.0, 2.0 * a),
                    Direction::Left,
                ),
                Edge::Left => (
                    Rect::new(bounds.width - a - 1.0, along_y - a, a + 1.0, 2.0 * a),
                    Direction::Right,
                ),
            }
        });
        if let Some((rect, direction)) = arrow {
            let grown = match direction {
                Direction::Up => {
                    Rect::new(rect.x - 1.0, rect.y - 1.0, rect.width + 2.0, rect.height)
                }
                Direction::Down => {
                    Rect::new(rect.x - 1.0, rect.y + 1.0, rect.width + 2.0, rect.height)
                }
                Direction::Left => {
                    Rect::new(rect.x - 1.0, rect.y - 1.0, rect.width, rect.height + 2.0)
                }
                Direction::Right => {
                    Rect::new(rect.x + 1.0, rect.y - 1.0, rect.width, rect.height + 2.0)
                }
            };
            cx.triangle(grown, direction, outline);
        }
        cx.floating_surface(
            bubble,
            radius,
            fill,
            Border {
                width: 1.0,
                color: outline,
            },
        );
        if let Some((rect, direction)) = arrow {
            cx.triangle(rect, direction, fill);
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        if let Event::AnimationFrame { now } = event {
            self.appear.frame(cx, *now);
            cx.request_composite();
        }
    }

    fn opacity(&self) -> f32 {
        self.appear.value()
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Dialog)
    }
}

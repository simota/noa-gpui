use noa_gpui_layout::Style;
use noa_gpui_layout::style::{
    AlignItems, FlexDirection, Rect as LayoutRect, Size as LayoutSize, length,
};
use noa_gpui_scene::Border;

use super::appear::{Appear, shadow};
use crate::event::Event;
use crate::overlay::Edge;
use crate::theme::{Corner, Role};
use crate::widget::{EventCx, PaintCx, Widget};

const PAD: f32 = 12.0;
const RADIUS: f32 = 10.0;
const FADE: f64 = 0.15;

/// Shows a card when the pointer rests on it. The first child is shown in the layout;
/// the second is the card's content, which keeps its widget ids and state while it
/// moves between the widget and the card.
#[derive(Debug)]
pub struct HoverCard {
    edge: Edge,
}

impl Default for HoverCard {
    fn default() -> Self {
        Self::new()
    }
}

impl HoverCard {
    pub fn new() -> Self {
        Self { edge: Edge::Bottom }
    }

    /// Below by default; the opposite side when there is no room.
    pub fn edge(mut self, edge: Edge) -> Self {
        self.edge = edge;
        self
    }
}

impl Widget for HoverCard {
    fn style(&self) -> Style {
        Style {
            flex_shrink: 0.0,
            // As wide as what it shows, so the card points at that and not at the
            // stretched width of a column.
            align_self: Some(AlignItems::START),
            ..Default::default()
        }
    }

    fn paint(&mut self, _cx: &mut PaintCx) {}

    fn hides_child(&self, index: usize) -> bool {
        index == 1
    }

    fn hover_card(&self) -> Option<Edge> {
        Some(self.edge)
    }
}

/// The overlay the tree lifts a [`HoverCard`]'s content into.
#[derive(Debug)]
pub(crate) struct HoverCardPanel {
    appear: Appear,
}

impl HoverCardPanel {
    pub fn new() -> Self {
        Self {
            appear: Appear::new(FADE),
        }
    }
}

impl Widget for HoverCardPanel {
    fn style(&self) -> Style {
        Style {
            flex_direction: FlexDirection::Column,
            gap: LayoutSize {
                width: length(0.0),
                height: length(6.0),
            },
            padding: LayoutRect::length(PAD),
            ..Default::default()
        }
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let radius = cx.corner(Corner::Container, RADIUS, bounds.height);
        shadow(cx, bounds, radius);
        let fill = cx.color(Role::Surface);
        let border = Border {
            width: 1.0,
            color: cx.color(Role::Outline),
        };
        cx.floating_surface(bounds, radius, fill, border);
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
        Some(accesskit::Role::Group)
    }
}

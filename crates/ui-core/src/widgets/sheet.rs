use ui_layout::Style;
use ui_layout::style::Rect as LayoutRect;
use ui_layout::style::{FlexDirection, Size as LayoutSize, auto, length, percent};
use ui_scene::{Border, Point, Rect};

use super::appear::{Appear, shadow};
use crate::event::Event;
use crate::overlay::{Edge, OverlayOptions, Placement};
use crate::theme::{Corner, Role};
use crate::tree::{UiTree, WidgetId};
use crate::widget::{EventCx, PaintCx, Widget};

/// Sliding time, seconds.
const SLIDE: f64 = 0.25;
const RADIUS: f32 = 10.0;

/// Emitted (with the sheet as the source) when a sheet closed by [`Sheet::close`] has
/// finished sliding up and is gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SheetClosed;

/// A window-modal panel (`NSWindow.beginSheet`) whose children are laid out in a column.
///
/// Everything below it is blocked while it is open: give its Cancel button
/// [`crate::widgets::Button::cancel_button`] so Escape answers.
#[derive(Debug)]
pub struct Sheet {
    /// The width, or for a drawer its size across its edge.
    width: f32,
    appear: Appear,
    size: ui_scene::Size,
    /// Inside an inner window: the distance from the window's top to hang from.
    in_window: Option<f32>,
    drawer: Option<Edge>,
}

impl Sheet {
    pub fn new(width: f32) -> Self {
        Self {
            width,
            appear: Appear::new(SLIDE),
            size: ui_scene::Size::default(),
            in_window: None,
            drawer: None,
        }
    }

    /// A drawer against `edge` of the window: `extent` is its width for a left or right
    /// drawer and its height for a top or bottom one; along the edge it fills the
    /// window. Open it with [`Sheet::open`] (it is not for inner windows).
    pub fn drawer(edge: Edge, extent: f32) -> Self {
        Self {
            drawer: Some(edge),
            ..Self::new(extent)
        }
    }

    /// The edge a drawer slides in from (`None` for a sheet).
    pub fn drawer_edge(&self) -> Option<Edge> {
        self.drawer
    }

    /// Make it a sheet of an inner window: a child of the window, hanging `top` below
    /// its top edge, modal within it.
    pub(crate) fn in_window(mut self, top: f32) -> Self {
        self.in_window = Some(top);
        self
    }

    /// Open below `top` (the bottom of the title bar or toolbar), centered, sliding down;
    /// a drawer against its edge below `top`, sliding in from the edge.
    pub fn open(self, tree: &mut UiTree, top: f32) -> WidgetId {
        let placement = match self.drawer {
            Some(edge) => Placement::Drawer { edge, top },
            None => Placement::Sheet { top },
        };
        let id = tree.open_overlay(self, placement, OverlayOptions::MODAL);
        tree.update::<Sheet, _>(id, |_, cx| cx.request_animation_frame());
        id
    }

    /// Slide the sheet `id` up and remove it; [`SheetClosed`] follows.
    pub fn close(tree: &mut UiTree, id: WidgetId) {
        tree.update::<Sheet, _>(id, |sheet, cx| {
            if sheet.appear.close(cx.now()) {
                cx.request_animation_frame();
            }
        });
    }

    pub fn is_closing(&self) -> bool {
        self.appear.is_closing()
    }

    /// How far the content is pushed out of view toward the edge it slides from, as
    /// a [`Widget::child_offset`].
    fn hidden(&self) -> Point {
        let shown = self.appear.value();
        let (w, h) = (
            (1.0 - shown) * self.size.width,
            (1.0 - shown) * self.size.height,
        );
        match self.edge() {
            Edge::Top => Point::new(0.0, h),
            Edge::Bottom => Point::new(0.0, -h),
            Edge::Left => Point::new(w, 0.0),
            Edge::Right => Point::new(-w, 0.0),
        }
    }

    fn edge(&self) -> Edge {
        self.drawer.unwrap_or(Edge::Top)
    }
}

impl Widget for Sheet {
    fn style(&self) -> Style {
        if let Some(edge) = self.drawer {
            let (width, height) = match edge {
                Edge::Left | Edge::Right => (length(self.width), percent(1.0)),
                Edge::Top | Edge::Bottom => (percent(1.0), length(self.width)),
            };
            return Style {
                flex_direction: FlexDirection::Column,
                gap: LayoutSize {
                    width: length(0.0),
                    height: length(12.0),
                },
                padding: LayoutRect::length(20.0),
                size: LayoutSize { width, height },
                ..Default::default()
            };
        }
        let (position, inset, margin) = window_modal_position(self.in_window);
        Style {
            position,
            inset,
            margin,
            flex_direction: FlexDirection::Column,
            gap: LayoutSize {
                width: length(0.0),
                height: length(12.0),
            },
            padding: LayoutRect::length(20.0),
            size: LayoutSize {
                width: length(self.width),
                height: auto(),
            },
            ..Default::default()
        }
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        self.size = cx.size();
        // Room for the shadow, except above the top it hangs from (the title bar or
        // toolbar) and past the edge it slides in from: nothing shows there.
        const SHADOW: f32 = 24.0;
        let (mut left, mut top, mut right, mut bottom) = (
            bounds.x - SHADOW,
            bounds.y,
            bounds.right() + SHADOW,
            bounds.bottom() + SHADOW,
        );
        match self.edge() {
            Edge::Top => {}
            // A bottom drawer's top edge is free, with its shadow above it.
            Edge::Bottom => (top, bottom) = (bounds.y - SHADOW, bounds.bottom()),
            Edge::Left => left = bounds.x,
            Edge::Right => right = bounds.right(),
        }
        cx.push_clip(Rect::new(left, top, right - left, bottom - top));
        let offset = self.hidden();
        let panel = bounds.translate(-offset.x, -offset.y);
        // A drawer is square: it lies against the window's edge.
        let radius = if self.drawer.is_some() {
            0.0
        } else {
            cx.corner(Corner::Container, RADIUS, panel.height)
        };
        shadow(cx, panel, radius);
        let fill = cx.color(Role::Surface);
        let border = Border {
            width: 1.0,
            color: cx.color(Role::Outline),
        };
        cx.floating_surface(panel, radius, fill, border);
        cx.pop_clip();
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        if let Event::AnimationFrame { now } = event {
            if self.appear.frame(cx, *now) {
                cx.emit(SheetClosed);
                cx.close_overlay();
            }
            cx.request_paint();
        }
    }

    fn clips_children(&self) -> bool {
        true
    }

    fn modal(&self) -> bool {
        self.in_window.is_some()
    }

    fn child_offset(&self) -> Point {
        self.hidden()
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Dialog)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        node.set_modal();
    }
}

/// Position, inset and margin for a sheet or alert: in the normal flow, or (inside an
/// inner window) absolutely placed `top` below the window's top edge and centered.
pub(crate) fn window_modal_position(
    top: Option<f32>,
) -> (
    ui_layout::style::Position,
    LayoutRect<ui_layout::style::LengthPercentageAuto>,
    LayoutRect<ui_layout::style::LengthPercentageAuto>,
) {
    use ui_layout::style::Position;
    match top {
        Some(top) => (
            Position::Absolute,
            LayoutRect {
                left: length(0.0),
                right: length(0.0),
                top: length(top),
                bottom: auto(),
            },
            LayoutRect {
                left: auto(),
                right: auto(),
                top: length(0.0),
                bottom: length(0.0),
            },
        ),
        None => (Position::Relative, LayoutRect::auto(), LayoutRect::zero()),
    }
}

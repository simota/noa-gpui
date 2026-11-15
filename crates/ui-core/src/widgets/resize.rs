use noa_gpui_layout::Style;
use noa_gpui_layout::style::{FlexDirection, Size as LayoutSize, length};
use noa_gpui_scene::{Border, Point, Rect, Size};

use crate::event::{Cursor, Event, PointerButton};
use crate::theme::Role;
use crate::widget::{EventCx, PaintCx, UpdateCx, Widget};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ResizeEdges {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
}

impl ResizeEdges {
    pub const NONE: Self = Self {
        left: false,
        right: false,
        top: false,
        bottom: false,
    };
    pub const ALL: Self = Self {
        left: true,
        right: true,
        top: true,
        bottom: true,
    };
    pub const BOTTOM_RIGHT: Self = Self {
        left: false,
        right: true,
        top: false,
        bottom: true,
    };

    pub fn any(self) -> bool {
        self.left || self.right || self.top || self.bottom
    }

    /// The resize pointer for these edges (`None` when no edge moves).
    pub fn cursor(self) -> Option<Cursor> {
        Cursor::for_edges(self.left, self.right, self.top, self.bottom)
    }

    /// The edges of `allowed` that a press at `p` would move, in a box of `size` whose
    /// edges take the pointer `band` wide (corners move both of their edges).
    pub fn at(p: Point, size: Size, band: f32, allowed: Self) -> Self {
        Self {
            left: allowed.left && p.x < band,
            right: allowed.right && p.x >= size.width - band,
            top: allowed.top && p.y < band,
            bottom: allowed.bottom && p.y >= size.height - band,
        }
    }
}

/// `v` limited to `lo..=hi`; `lo` wins if they cross.
fn between(v: f32, lo: f32, hi: f32) -> f32 {
    v.min(hi).max(lo)
}

/// `start` with its `edges` moved by `delta`, kept between `min` and `max` in size.
/// With an `area`, moved edges also stay inside `0..area` (an edge already beyond it
/// does not have to come back in).
pub(crate) fn resize_rect(
    start: Rect,
    edges: ResizeEdges,
    delta: Point,
    min: Size,
    max: Size,
    area: Option<Size>,
) -> Rect {
    let (area_left, area_top) = if area.is_some() {
        (0.0, 0.0)
    } else {
        (f32::NEG_INFINITY, f32::NEG_INFINITY)
    };
    let (area_right, area_bottom) = area.map_or((f32::INFINITY, f32::INFINITY), |a| {
        (a.width.max(start.right()), a.height.max(start.bottom()))
    });
    let (mut left, mut top, mut right, mut bottom) =
        (start.x, start.y, start.right(), start.bottom());
    if edges.left {
        left = between(
            left + delta.x,
            area_left.max(right - max.width),
            right - min.width,
        );
    }
    if edges.right {
        right = between(
            right + delta.x,
            left + min.width,
            area_right.min(left + max.width),
        );
    }
    if edges.top {
        top = between(
            top + delta.y,
            area_top.max(bottom - max.height),
            bottom - min.height,
        );
    }
    if edges.bottom {
        bottom = between(
            bottom + delta.y,
            top + min.height,
            area_bottom.min(top + max.height),
        );
    }
    Rect::new(left, top, right - left, bottom - top)
}

const BAND: f32 = 6.0;
const GRIP: f32 = 14.0;
const RADIUS: f32 = 6.0;

/// Emitted by a [`ResizableBox`] when a resize by the user ends, with the new size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxResized(pub Size);

/// A box the user resizes by dragging its edges; its children are clipped to it. The
/// box stays where the layout puts it, so a left or top edge suits a box whose parent
/// aligns it to the right or the bottom.
#[derive(Debug, Clone)]
pub struct ResizableBox {
    size: Size,
    min: Size,
    max: Size,
    edges: ResizeEdges,
    /// While dragging: the edges, where the press was (window coordinates) and the size
    /// then.
    drag: Option<(ResizeEdges, Point, Size)>,
}

impl ResizableBox {
    pub fn new(size: Size) -> Self {
        Self {
            size,
            min: Size::new(2.0 * GRIP, 2.0 * GRIP),
            max: Size::new(f32::INFINITY, f32::INFINITY),
            edges: ResizeEdges::BOTTOM_RIGHT,
            drag: None,
        }
    }

    pub fn min_size(mut self, min: Size) -> Self {
        self.min = min;
        self.size = self.limit(self.size);
        self
    }

    pub fn max_size(mut self, max: Size) -> Self {
        self.max = max;
        self.size = self.limit(self.size);
        self
    }

    pub fn edges(mut self, edges: ResizeEdges) -> Self {
        self.edges = edges;
        self
    }

    pub fn size(&self) -> Size {
        self.size
    }

    pub fn is_resizing(&self) -> bool {
        self.drag.is_some()
    }

    /// Resize from the app (kept between the minimum and maximum; nothing is emitted).
    pub fn set_size(&mut self, cx: &mut UpdateCx, size: Size) {
        let size = self.limit(size);
        if size != self.size {
            self.size = size;
            cx.request_layout();
            cx.request_paint();
        }
    }

    fn limit(&self, size: Size) -> Size {
        Size::new(
            between(size.width, self.min.width, self.max.width),
            between(size.height, self.min.height, self.max.height),
        )
    }

    fn grips(&self) -> bool {
        self.edges.right && self.edges.bottom
    }

    /// The edges a press at `p` (widget coordinates) would move.
    fn edges_at(&self, p: Point, size: Size) -> ResizeEdges {
        let in_grip = self.grips() && p.x >= size.width - GRIP && p.y >= size.height - GRIP;
        if in_grip {
            return ResizeEdges::BOTTOM_RIGHT;
        }
        ResizeEdges::at(p, size, BAND, self.edges)
    }
}

impl Widget for ResizableBox {
    fn style(&self) -> Style {
        Style {
            flex_direction: FlexDirection::Column,
            size: LayoutSize {
                width: length(self.size.width),
                height: length(self.size.height),
            },
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let outline = cx.color(Role::Outline);
        cx.rounded_rect(
            cx.bounds(),
            RADIUS,
            noa_gpui_scene::Color::TRANSPARENT,
            Border {
                width: 1.0,
                color: outline,
            },
        );
    }

    fn paint_over(&mut self, cx: &mut PaintCx) {
        let b = cx.bounds();
        if let Some((edges, ..)) = self.drag {
            let accent = cx.color(Role::Accent);
            let line = 2.0;
            if edges.left {
                cx.fill_rect(Rect::new(0.0, 0.0, line, b.height), accent);
            }
            if edges.right {
                cx.fill_rect(Rect::new(b.width - line, 0.0, line, b.height), accent);
            }
            if edges.top {
                cx.fill_rect(Rect::new(0.0, 0.0, b.width, line), accent);
            }
            if edges.bottom {
                cx.fill_rect(Rect::new(0.0, b.height - line, b.width, line), accent);
            }
        }
        if self.grips() {
            let ink = cx.color(Role::Muted);
            let (r, bt) = (b.width - 3.0, b.height - 3.0);
            for step in [4.0, 7.5, 11.0] {
                cx.line(Point::new(r - step, bt), Point::new(r, bt - step), 1.2, ink);
            }
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                let edges = self.edges_at(cx.local(*pos), cx.size());
                if edges.any() {
                    self.drag = Some((edges, *pos, self.size));
                    cx.capture_pointer();
                    cx.request_paint();
                    cx.set_handled();
                }
            }
            Event::PointerMove { pos } => {
                let Some((edges, from, start)) = self.drag else {
                    return;
                };
                let delta = Point::new(pos.x - from.x, pos.y - from.y);
                let start = Rect::new(0.0, 0.0, start.width, start.height);
                let rect = resize_rect(start, edges, delta, self.min, self.max, None);
                let size = Size::new(rect.width, rect.height);
                if size != self.size {
                    self.size = size;
                    cx.request_layout();
                    cx.request_paint();
                }
                cx.set_handled();
            }
            Event::PointerUp { .. } if self.drag.take().is_some() => {
                cx.release_pointer();
                cx.emit(BoxResized(self.size));
                cx.request_paint();
                cx.set_handled();
            }
            _ => {}
        }
    }

    fn claims_pointer(&self, local: Point, size: Size) -> bool {
        self.edges_at(local, size).any()
    }

    fn cursor(&self, local: Point, size: Size) -> Option<Cursor> {
        match self.drag {
            Some((edges, ..)) => edges.cursor(),
            None => self.edges_at(local, size).cursor(),
        }
    }

    fn clips_children(&self) -> bool {
        true
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Group)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INF: Size = Size::new(f32::INFINITY, f32::INFINITY);

    #[test]
    fn edges_move_within_the_size_limits() {
        let start = Rect::new(10.0, 10.0, 100.0, 80.0);
        let min = Size::new(40.0, 30.0);
        let grown = resize_rect(
            start,
            ResizeEdges::BOTTOM_RIGHT,
            Point::new(20.0, 15.0),
            min,
            INF,
            None,
        );
        assert_eq!(grown, Rect::new(10.0, 10.0, 120.0, 95.0));
        // The left edge stops at the minimum width; the top at the maximum height.
        let max = Size::new(500.0, 100.0);
        let squeezed = resize_rect(
            start,
            ResizeEdges {
                left: true,
                top: true,
                ..ResizeEdges::NONE
            },
            Point::new(90.0, -50.0),
            min,
            max,
            None,
        );
        assert_eq!(squeezed, Rect::new(70.0, -10.0, 40.0, 100.0));
    }

    #[test]
    fn moved_edges_stay_in_the_area() {
        let start = Rect::new(10.0, 10.0, 100.0, 80.0);
        let area = Some(Size::new(200.0, 150.0));
        let r = resize_rect(
            start,
            ResizeEdges::ALL,
            Point::new(-50.0, -50.0),
            Size::new(40.0, 30.0),
            INF,
            area,
        );
        assert_eq!((r.x, r.y), (0.0, 0.0));
        let r = resize_rect(
            start,
            ResizeEdges::BOTTOM_RIGHT,
            Point::new(500.0, 500.0),
            Size::new(40.0, 30.0),
            INF,
            area,
        );
        assert_eq!((r.right(), r.bottom()), (200.0, 150.0));
    }

    #[test]
    fn corners_move_both_edges() {
        let size = Size::new(100.0, 80.0);
        let corner = ResizeEdges::at(Point::new(98.0, 78.0), size, 6.0, ResizeEdges::ALL);
        assert_eq!(corner, ResizeEdges::BOTTOM_RIGHT);
        let inside = ResizeEdges::at(Point::new(50.0, 40.0), size, 6.0, ResizeEdges::ALL);
        assert!(!inside.any());
        let not_allowed =
            ResizeEdges::at(Point::new(2.0, 40.0), size, 6.0, ResizeEdges::BOTTOM_RIGHT);
        assert!(!not_allowed.any());
    }
}

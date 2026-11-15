use noa_gpui_scene::{Border, Point, Rect, Size};

use crate::event::{Event, PointerButton};
use crate::theme::Role;
use crate::widget::{EventCx, PaintCx};

use super::ScrollerStyle;

pub const LEGACY_TRACK: f32 = 14.0;
/// Width of the edge area where pressing hits an overlay scroll bar.
const OVERLAY_HIT: f32 = 10.0;
/// Seconds overlay bars stay after scrolling, then the fade.
const SHOW_FOR: f64 = 1.0;
const FADE: f64 = 0.3;
/// Seconds without wheel input after which a scroll gesture counts as finished.
pub const GESTURE_END: f64 = 0.05;
/// How much overscroll moves per pixel of input past an edge.
const RUBBER: f32 = 0.35;
const MAX_OVERSCROLL: f32 = 90.0;
/// Speeds below this (pixels per second) end momentum.
const MIN_SPEED: f32 = 20.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    X,
    Y,
}

/// Scroll state over a viewport rectangle (widget coordinates) showing `content`.
#[derive(Debug, Clone)]
pub(crate) struct Scroller {
    pub offset: Point,
    pub horizontal: bool,
    pub vertical: bool,
    pub style: ScrollerStyle,
    pub elastic: bool,
    pub momentum: bool,
    /// Also request a composite when the offset changes (for views whose children move).
    pub composite: bool,
    velocity: Point,
    last_wheel: f64,
    last_frame: f64,
    animating: bool,
    show_until: f64,
    now: f64,
    dragging: Option<(Axis, f32)>,
    viewport: Rect,
    content: Size,
}

impl Scroller {
    pub fn new() -> Self {
        Self {
            offset: Point::default(),
            horizontal: false,
            vertical: true,
            style: ScrollerStyle::Overlay,
            elastic: true,
            momentum: true,
            composite: false,
            velocity: Point::default(),
            last_wheel: f64::NEG_INFINITY,
            last_frame: 0.0,
            animating: false,
            show_until: f64::NEG_INFINITY,
            now: 0.0,
            dragging: None,
            viewport: Rect::default(),
            content: Size::default(),
        }
    }

    /// Call it before handling an event or painting.
    pub fn set_geometry(&mut self, viewport: Rect, content: Size) {
        self.viewport = viewport;
        self.content = content;
    }

    pub fn viewport(&self) -> Rect {
        self.viewport
    }

    /// Clamped to the scrollable range; stops momentum. Returns whether it changed.
    pub fn set_offset(&mut self, position: Point) -> bool {
        let max = self.max();
        let position = Point::new(
            position.x.clamp(0.0, max.x.max(0.0)),
            position.y.clamp(0.0, max.y.max(0.0)),
        );
        let changed = position != self.offset;
        self.offset = position;
        self.velocity = Point::default();
        changed
    }

    /// Like [`Scroller::set_offset`] but only clamped at 0, for positions set before the
    /// content size is known.
    pub fn set_offset_unclamped(&mut self, position: Point) -> bool {
        let position = Point::new(position.x.max(0.0), position.y.max(0.0));
        let changed = position != self.offset;
        self.offset = position;
        self.velocity = Point::default();
        changed
    }

    /// The part of the viewport the scroll bars leave uncovered: [`ScrollerStyle::Legacy`]
    /// tracks are drawn over the viewport's right and bottom edges.
    pub fn unobscured(&self) -> Rect {
        let mut v = self.viewport;
        if self.style == ScrollerStyle::Legacy {
            if self.scrolls(Axis::X) {
                v.height = (v.height - LEGACY_TRACK).max(0.0);
            }
            if self.scrolls(Axis::Y) {
                v.width = (v.width - LEGACY_TRACK).max(0.0);
            }
        }
        v
    }

    /// Scroll the least needed for content rectangle `rect` to be inside the viewport
    /// (the part no scroll bar covers), along the axes this scroller scrolls. Returns
    /// whether the position changed.
    pub fn reveal(&mut self, rect: Rect) -> bool {
        let v = self.unobscured();
        let mut target = self.offset;
        if !self.vertical {
        } else if rect.y < target.y {
            target.y = rect.y;
        } else if rect.bottom() > target.y + v.height {
            target.y = rect.bottom() - v.height;
        }
        if !self.horizontal {
        } else if rect.x < target.x {
            target.x = rect.x;
        } else if rect.right() > target.x + v.width {
            target.x = rect.right() - v.width;
        }
        target != self.offset && self.set_offset(target)
    }

    pub fn max(&self) -> Point {
        Point::new(
            (self.content.width - self.viewport.width).max(0.0),
            (self.content.height - self.viewport.height).max(0.0),
        )
    }

    fn scrolls(&self, axis: Axis) -> bool {
        let max = self.max();
        match axis {
            Axis::X => self.horizontal && max.x > 0.0,
            Axis::Y => self.vertical && max.y > 0.0,
        }
    }

    /// How far the position is past an edge on each axis (signed).
    fn overscroll(&self) -> Point {
        let max = self.max();
        let past = |v: f32, m: f32| {
            if v < 0.0 {
                v
            } else if v > m {
                v - m
            } else {
                0.0
            }
        };
        Point::new(past(self.offset.x, max.x), past(self.offset.y, max.y))
    }

    fn scroll_axis(&self, value: f32, delta: f32, max: f32) -> f32 {
        let target = value + delta;
        if !self.elastic {
            return target.clamp(0.0, max);
        }
        let (low, high) = (-MAX_OVERSCROLL, max + MAX_OVERSCROLL);
        if (0.0..=max).contains(&target) {
            target
        } else if (0.0..=max).contains(&value) {
            let edge = if target < 0.0 { 0.0 } else { max };
            (edge + (target - edge) * RUBBER).clamp(low, high)
        } else {
            (value + delta * RUBBER).clamp(low, high)
        }
    }

    fn moved(&self, cx: &mut EventCx) {
        if self.composite {
            cx.request_composite();
        }
        cx.request_paint();
    }

    fn wake(&mut self, cx: &mut EventCx) {
        if !self.animating {
            self.animating = true;
            self.last_frame = cx.now();
            cx.request_animation_frame();
        }
    }

    pub fn bar_width(&self) -> f32 {
        match self.style {
            ScrollerStyle::Overlay => 8.0,
            ScrollerStyle::Legacy => LEGACY_TRACK,
        }
    }

    /// Thumb rectangle for `axis` (widget coordinates), and the track's start and
    /// length along the axis.
    fn thumb(&self, axis: Axis) -> Option<(Rect, f32, f32)> {
        if !self.scrolls(axis) {
            return None;
        }
        let v = self.viewport;
        let (view, content, offset, max) = match axis {
            Axis::X => (v.width, self.content.width, self.offset.x, self.max().x),
            Axis::Y => (v.height, self.content.height, self.offset.y, self.max().y),
        };
        let both = self.scrolls(Axis::X) && self.scrolls(Axis::Y);
        let corner = if both { self.bar_width() } else { 0.0 };
        let track = view - corner - 4.0;
        // Overscroll shrinks the thumb against the edge, as in macOS.
        let squeeze = match axis {
            Axis::X => self.overscroll().x.abs(),
            Axis::Y => self.overscroll().y.abs(),
        };
        let length = (track * view / (content + squeeze)).max(24.0).min(track);
        let t = (offset / max).clamp(0.0, 1.0);
        let at = 2.0 + t * (track - length);
        let w = self.bar_width();
        let rect = match axis {
            Axis::X => Rect::new(v.x + at, v.bottom() - w, length, w),
            Axis::Y => Rect::new(v.right() - w, v.y + at, w, length),
        };
        Some((rect, 2.0, track))
    }

    /// In widget coordinates.
    fn bar_at(&self, p: Point) -> Option<Axis> {
        self.bar_in(p, self.viewport)
    }

    fn bar_in(&self, p: Point, v: Rect) -> Option<Axis> {
        if !v.contains(p) {
            return None;
        }
        let edge = match self.style {
            ScrollerStyle::Overlay => OVERLAY_HIT,
            ScrollerStyle::Legacy => LEGACY_TRACK,
        };
        if self.scrolls(Axis::Y) && p.x >= v.right() - edge {
            Some(Axis::Y)
        } else if self.scrolls(Axis::X) && p.y >= v.bottom() - edge {
            Some(Axis::X)
        } else {
            None
        }
    }

    /// Whether `p` (widget coordinates) is on a scroll bar of a viewport `viewport`
    /// (the current one may be stale when the widget's size is passed in).
    pub fn claims(&self, p: Point, viewport: Rect) -> bool {
        self.bar_in(p, viewport).is_some()
    }

    fn bar_alpha(&self) -> f32 {
        match self.style {
            ScrollerStyle::Legacy => 1.0,
            _ if self.dragging.is_some() => 1.0,
            ScrollerStyle::Overlay => {
                let since = self.now - self.show_until;
                if since <= 0.0 {
                    1.0
                } else {
                    (1.0 - since / FADE).clamp(0.0, 1.0) as f32
                }
            }
        }
    }

    /// Show overlay bars briefly (after a programmatic scroll, such as keyboard
    /// navigation).
    pub fn flash(&mut self, cx: &mut EventCx) {
        self.now = cx.now();
        self.show_until = cx.now() + SHOW_FOR;
        self.wake(cx);
    }

    fn frame(&mut self, cx: &mut EventCx, now: f64) {
        let dt = (now - self.last_frame).clamp(0.0, 0.1) as f32;
        self.last_frame = now;
        self.now = now;
        let max = self.max();
        let mut moving = false;
        if now - self.last_wheel >= GESTURE_END && self.dragging.is_none() {
            let over = self.overscroll();
            if over != Point::default() {
                let k = (-dt * 14.0).exp();
                let settle = |v: f32, o: f32| {
                    if o.abs() < 0.5 {
                        v - o
                    } else {
                        v - o * (1.0 - k)
                    }
                };
                self.offset =
                    Point::new(settle(self.offset.x, over.x), settle(self.offset.y, over.y));
                self.velocity = Point::default();
                moving = self.overscroll() != Point::default();
            } else if self.momentum
                && (self.velocity.x.abs() > MIN_SPEED || self.velocity.y.abs() > MIN_SPEED)
            {
                let x = self.scroll_axis(self.offset.x, self.velocity.x * dt, max.x);
                let y = self.scroll_axis(self.offset.y, self.velocity.y * dt, max.y);
                self.offset = Point::new(
                    if self.horizontal { x } else { self.offset.x },
                    if self.vertical { y } else { self.offset.y },
                );
                let friction = (-dt * 3.5).exp();
                self.velocity = Point::new(self.velocity.x * friction, self.velocity.y * friction);
                // Hitting an edge ends the glide; the spring takes over.
                if self.overscroll() != Point::default() {
                    self.velocity = Point::default();
                }
                self.show_until = now + SHOW_FOR;
                moving = true;
            } else {
                self.velocity = Point::default();
            }
        } else {
            moving = true;
        }
        self.moved(cx);
        let fading = self.style == ScrollerStyle::Overlay && now < self.show_until + FADE;
        if moving || fading {
            cx.request_animation_frame();
        } else {
            self.animating = false;
        }
    }

    /// Handle wheel, scroll bar and animation events. Returns whether the event was
    /// used (a wheel event counts only when it moved the content).
    pub fn event(&mut self, cx: &mut EventCx, event: &Event) -> bool {
        self.now = cx.now();
        let max = self.max();
        match event {
            Event::Wheel { delta, .. } => {
                // Shift turns the wheel sideways; a horizontal-only view takes it too.
                let sideways = cx.modifiers().shift || (self.horizontal && !self.vertical);
                let (dx, dy) = if sideways && delta.x == 0.0 {
                    (delta.y, 0.0)
                } else {
                    (delta.x, delta.y)
                };
                let before = self.offset;
                let x = if self.horizontal {
                    self.scroll_axis(self.offset.x, -dx, max.x)
                } else {
                    self.offset.x
                };
                let y = if self.vertical {
                    self.scroll_axis(self.offset.y, -dy, max.y)
                } else {
                    self.offset.y
                };
                // Rubber-banding only when the content can scroll that way.
                self.offset = Point::new(
                    if max.x > 0.0 { x } else { x.clamp(0.0, max.x) },
                    if max.y > 0.0 { y } else { y.clamp(0.0, max.y) },
                );
                let now = cx.now();
                let dt = ((now - self.last_wheel) as f32).clamp(1.0 / 120.0, 0.1);
                let v = Point::new(
                    (self.offset.x - before.x) / dt,
                    (self.offset.y - before.y) / dt,
                );
                // Smooth the estimate; a fresh gesture starts from this event.
                let fresh = now - self.last_wheel > 0.2;
                self.velocity = if fresh {
                    v
                } else {
                    Point::new(
                        0.6 * v.x + 0.4 * self.velocity.x,
                        0.6 * v.y + 0.4 * self.velocity.y,
                    )
                };
                self.last_wheel = now;
                self.show_until = now + SHOW_FOR;
                let moved = self.offset != before;
                if moved {
                    self.moved(cx);
                    cx.set_handled();
                }
                self.wake(cx);
                moved
            }
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                let p = cx.local(*pos);
                let Some(axis) = self.bar_at(p) else {
                    return false;
                };
                cx.set_handled();
                let Some((thumb, _, _)) = self.thumb(axis) else {
                    return true;
                };
                let along = |p: Point| if axis == Axis::X { p.x } else { p.y };
                let (thumb_start, thumb_len) = match axis {
                    Axis::X => (thumb.x, thumb.width),
                    Axis::Y => (thumb.y, thumb.height),
                };
                if along(p) >= thumb_start && along(p) < thumb_start + thumb_len {
                    self.dragging = Some((axis, along(p) - thumb_start));
                    cx.capture_pointer();
                } else {
                    let page = match axis {
                        Axis::X => self.viewport.width,
                        Axis::Y => self.viewport.height,
                    } * 0.9;
                    let sign = if along(p) < thumb_start { -1.0 } else { 1.0 };
                    match axis {
                        Axis::X => self.offset.x = (self.offset.x + sign * page).clamp(0.0, max.x),
                        Axis::Y => self.offset.y = (self.offset.y + sign * page).clamp(0.0, max.y),
                    }
                }
                self.velocity = Point::default();
                self.show_until = cx.now() + SHOW_FOR;
                self.moved(cx);
                self.wake(cx);
                true
            }
            Event::PointerMove { pos } => {
                let Some((axis, grab)) = self.dragging else {
                    return false;
                };
                if let Some((thumb, start, track)) = self.thumb(axis) {
                    let p = cx.local(*pos);
                    let (at, len, origin) = match axis {
                        Axis::X => (p.x, thumb.width, self.viewport.x),
                        Axis::Y => (p.y, thumb.height, self.viewport.y),
                    };
                    let t = ((at - origin - grab - start) / (track - len).max(1.0)).clamp(0.0, 1.0);
                    match axis {
                        Axis::X => self.offset.x = t * max.x,
                        Axis::Y => self.offset.y = t * max.y,
                    }
                    self.show_until = cx.now() + SHOW_FOR;
                    self.moved(cx);
                }
                cx.set_handled();
                true
            }
            Event::PointerUp { .. } => {
                if self.dragging.take().is_none() {
                    return false;
                }
                cx.release_pointer();
                self.show_until = cx.now() + SHOW_FOR;
                self.wake(cx);
                cx.set_handled();
                true
            }
            Event::AnimationFrame { now } => {
                self.frame(cx, *now);
                true
            }
            _ => false,
        }
    }

    pub fn paint(&mut self, cx: &mut PaintCx) {
        let alpha = self.bar_alpha();
        if alpha <= 0.0 {
            return;
        }
        let v = self.viewport;
        let thumb = cx.color(Role::Text).with_alpha(0.4 * alpha);
        if self.style == ScrollerStyle::Legacy {
            let track = cx.color(Role::SurfaceAlt);
            let line = cx.color(Role::Separator);
            for axis in [Axis::X, Axis::Y] {
                if !self.scrolls(axis) {
                    continue;
                }
                let rect = match axis {
                    Axis::X => Rect::new(v.x, v.bottom() - LEGACY_TRACK, v.width, LEGACY_TRACK),
                    Axis::Y => Rect::new(v.right() - LEGACY_TRACK, v.y, LEGACY_TRACK, v.height),
                };
                cx.rounded_rect(
                    rect,
                    0.0,
                    track,
                    Border {
                        width: 0.5,
                        color: line,
                    },
                );
            }
        }
        for axis in [Axis::X, Axis::Y] {
            if let Some((rect, _, _)) = self.thumb(axis) {
                let inset = if self.style == ScrollerStyle::Legacy {
                    3.5
                } else {
                    1.5
                };
                let r = Rect::new(
                    rect.x + if axis == Axis::Y { inset } else { 0.0 },
                    rect.y + if axis == Axis::X { inset } else { 0.0 },
                    rect.width - if axis == Axis::Y { 2.0 * inset } else { 0.0 },
                    rect.height - if axis == Axis::X { 2.0 * inset } else { 0.0 },
                );
                let radius = r.width.min(r.height) / 2.0;
                cx.rounded_rect(r, radius, thumb, Border::default());
            }
        }
    }

    pub fn accessibility(&self, node: &mut accesskit::Node) {
        let max = self.max();
        if self.vertical {
            node.set_scroll_y(f64::from(self.offset.y));
            node.set_scroll_y_min(0.0);
            node.set_scroll_y_max(f64::from(max.y));
        }
        if self.horizontal {
            node.set_scroll_x(f64::from(self.offset.x));
            node.set_scroll_x_min(0.0);
            node.set_scroll_x_max(f64::from(max.x));
        }
    }
}

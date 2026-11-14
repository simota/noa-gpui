use ui_layout::Style;
use ui_layout::style::{Overflow, Point as LayoutPoint, Rect as LayoutRect, length};
use ui_scene::{Point, Rect, Size};

use super::scroller::{LEGACY_TRACK, Scroller};
use crate::event::Event;
use crate::view_state;
use crate::widget::{EventCx, PaintCx, UpdateCx, Widget};

/// How a [`ScrollView`] shows its scroll bars (`NSScroller.Style`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollerStyle {
    #[default]
    Overlay,
    /// Bars in a track beside the content, which is inset to make room.
    Legacy,
}

/// A clipping viewport that scrolls its content (`NSScrollView`).
///
/// Platforms that send their own momentum events (macOS trackpads) keep sending them;
/// [`ScrollView::momentum`] only adds the tail after they end.
///
/// Scrolling only moves the cached child commands, so it costs a composite, never a
/// re-layout or a repaint of the content. Animations run only while the view is
/// visible.
#[derive(Debug, Clone)]
pub struct ScrollView {
    style: Style,
    scroller: Scroller,
    follows_end: bool,
    /// Following the end now: at the bottom when the content last changed size.
    pinned: bool,
    pending: Pending,
}

impl ScrollView {
    /// `style` sizes the viewport; overflow is set from the scrolling axes.
    pub fn new(style: Style) -> Self {
        let mut scroller = Scroller::new();
        scroller.composite = true;
        Self {
            style,
            scroller,
            follows_end: false,
            pinned: true,
            pending: Pending::None,
        }
    }

    /// Stay at the bottom while the content grows and the view is scrolled to the bottom
    /// (off by default).
    pub fn follows_end(mut self, follows: bool) -> Self {
        self.follows_end = follows;
        self
    }

    /// A position set with [`ScrollView::set_offset`] counts from the next paint.
    pub fn is_following_end(&self) -> bool {
        self.follows_end && self.pinned
    }

    /// A view that [`ScrollView::follows_end`] follows the end again.
    pub fn scroll_to_end(&mut self, cx: &mut UpdateCx) {
        self.pinned = true;
        let max = self.scroller.max().y;
        self.set_scroll_position(cx, Point::new(self.scroller.offset.x, max));
        // The content may have grown since the last frame: the end is applied again
        // when the view is painted, once its size is known.
        self.pending = Pending::End;
        cx.request_paint();
    }

    pub fn horizontal(mut self, horizontal: bool) -> Self {
        self.scroller.horizontal = horizontal;
        self
    }

    pub fn vertical(mut self, vertical: bool) -> Self {
        self.scroller.vertical = vertical;
        self
    }

    pub fn scroller_style(mut self, style: ScrollerStyle) -> Self {
        self.scroller.style = style;
        self
    }

    /// Rubber-band past the edges (on by default).
    pub fn elastic(mut self, elastic: bool) -> Self {
        self.scroller.elastic = elastic;
        self
    }

    /// Keep moving after a wheel gesture ends (on by default).
    pub fn momentum(mut self, momentum: bool) -> Self {
        self.scroller.momentum = momentum;
        self
    }

    pub fn offset(&self) -> f32 {
        self.scroller.offset.y
    }

    /// Both scroll positions (negative or past the end while rubber-banding).
    pub fn scroll_position(&self) -> Point {
        self.scroller.offset
    }

    /// Scroll vertically to `offset` (clamped at 0; the far end is applied on the next
    /// scroll, once the content size is known).
    pub fn set_offset(&mut self, cx: &mut UpdateCx, offset: f32) {
        self.set_scroll_position(cx, Point::new(self.scroller.offset.x, offset));
    }

    pub fn set_scroll_position(&mut self, cx: &mut UpdateCx, position: Point) {
        // Whether this is the end is only known with the content size of the next
        // paint: the content may have changed, or not have been laid out at all yet.
        self.pending = Pending::Position;
        if self.scroller.set_offset_unclamped(position) {
            cx.request_composite();
        }
        cx.request_paint();
    }

    fn sync(&mut self, size: Size, content: Size) {
        let before = self.scroller.max();
        self.scroller
            .set_geometry(Rect::new(0.0, 0.0, size.width, size.height), content);
        let max = self.scroller.max();
        // The content got shorter (or the view larger): what was past the new end is
        // gone. Rubber-banding past an end that stays settles there by itself.
        let offset = &mut self.scroller.offset;
        if max.x < before.x && offset.x > max.x {
            offset.x = max.x;
        }
        if max.y < before.y && offset.y > max.y {
            offset.y = max.y;
        }
        match std::mem::take(&mut self.pending) {
            Pending::None => {}
            Pending::End => {
                self.pinned = true;
                offset.y = offset.y.max(max.y);
            }
            Pending::Position => self.pinned = offset.y >= max.y,
            Pending::Restored => {
                // Saved for content that may have grown shorter since.
                offset.x = offset.x.min(max.x);
                offset.y = offset.y.min(max.y);
                self.pinned = offset.y >= max.y;
            }
        }
        if self.follows_end && self.pinned && offset.y < max.y {
            offset.y = max.y;
        }
    }

    /// After the user scrolled from `before`: moving up from the end lets go of it
    /// (however little: slow scrolling comes in steps of less than a pixel), and the
    /// view follows again once it is back at the bottom.
    fn update_pinned(&mut self, before: f32) {
        if self.follows_end {
            let (y, end) = (self.scroller.offset.y, self.scroller.max().y);
            self.pinned = if y < before && y < end {
                false
            } else {
                y >= end - 1.0
            };
        }
    }
}

/// What [`ScrollView::sync`] still has to do for a position set from outside.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Pending {
    #[default]
    None,
    End,
    Position,
    /// A saved position: keep it within the content, then as `Position`.
    Restored,
}

impl Widget for ScrollView {
    /// The scroll position, `position:0,1240`, or `end` for a view following its end
    /// ([`ScrollView::follows_end`]) that is at the end.
    fn save_state(&self) -> Option<String> {
        if self.follows_end && self.pinned {
            return Some("end".into());
        }
        let offset = self.scroller.offset;
        let mut out = String::from("position:");
        view_state::push_number(&mut out, offset.x.max(0.0));
        out.push(',');
        view_state::push_number(&mut out, offset.y.max(0.0));
        Some(out)
    }

    /// The position is kept within the content as it is laid out now.
    fn restore_state(&mut self, cx: &mut UpdateCx, state: &str) {
        if view_state::field(state, "end").is_some() {
            self.scroll_to_end(cx);
            return;
        }
        if let Some(&[x, y]) = view_state::field(state, "position")
            .and_then(view_state::parse_numbers)
            .as_deref()
        {
            self.set_scroll_position(cx, Point::new(x, y));
            self.pending = Pending::Restored;
        }
    }

    fn style(&self) -> Style {
        let s = &self.scroller;
        let legacy = s.style == ScrollerStyle::Legacy;
        let mut padding = self.style.padding;
        if legacy && s.vertical {
            padding.right = length(LEGACY_TRACK);
        }
        if legacy && s.horizontal {
            padding.bottom = length(LEGACY_TRACK);
        }
        let axis = |on: bool| {
            if on {
                Overflow::Scroll
            } else {
                Overflow::Hidden
            }
        };
        Style {
            overflow: LayoutPoint {
                x: axis(s.horizontal),
                y: axis(s.vertical),
            },
            scrollbar_width: 0.0,
            padding: LayoutRect { ..padding },
            ..self.style.clone()
        }
    }

    fn paint(&mut self, _cx: &mut PaintCx) {}

    fn paint_over(&mut self, cx: &mut PaintCx) {
        self.sync(cx.size(), cx.content_size());
        self.scroller.paint(cx);
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        self.sync(cx.size(), cx.content_size());
        let before = self.scroller.offset.y;
        if self.scroller.event(cx, event) {
            self.update_pinned(before);
        }
    }

    fn claims_pointer(&self, local: Point, size: Size) -> bool {
        // Pressing the scroll bar edge scrolls instead of reaching the content.
        self.scroller
            .claims(local, Rect::new(0.0, 0.0, size.width, size.height))
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::ScrollView)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        self.scroller.accessibility(node);
    }

    fn clips_children(&self) -> bool {
        true
    }

    fn child_offset(&self) -> Point {
        self.scroller.offset
    }

    /// Scrolls the least that brings `rect` into view; of a rectangle larger than the
    /// view, its start.
    fn reveal(&mut self, cx: &mut UpdateCx, size: Size, content_size: Size, rect: Rect) -> bool {
        self.sync(size, content_size);
        let before = self.scroller.offset.y;
        let view = self.scroller.unobscured();
        let rect = Rect::new(
            rect.x,
            rect.y,
            rect.width.min(view.width),
            rect.height.min(view.height),
        );
        if !self.scroller.reveal(rect) {
            return false;
        }
        self.update_pinned(before);
        // The content moves in the composite; the scroll bars are painted.
        cx.request_composite();
        cx.request_paint();
        true
    }
}

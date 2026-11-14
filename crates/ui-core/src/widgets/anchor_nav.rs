use ui_layout::style::{Dimension, Size as LayoutSize, percent};
use ui_layout::{MeasureInput, Style};
use ui_scene::{Border, Rect, Size};
use ui_text::{TextStore, TextStyle};

use super::{CachedText, ScrollView, focus_ring};
use crate::event::{Event, Key, PointerButton};
use crate::theme::Role;
use crate::tree::{UiTree, WidgetId};
use crate::widget::{EventCx, MeasureCx, PaintCx, UpdateCx, Widget};

/// Emitted by an [`AnchorNav`] when an anchor is clicked (or chosen with the keyboard),
/// with its index among the anchors. Pass it to [`AnchorNav::scroll_to`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnchorSelected(pub usize);

const PAD_X: f32 = 12.0;
const ROW_PAD_Y: f32 = 6.0;
const HEADING_TOP: f32 = 16.0;
const HEADING_BOTTOM: f32 = 6.0;
const ROW_GAP: f32 = 2.0;

#[derive(Debug)]
struct Entry {
    title: String,
    /// The section an anchor scrolls to; `None` for a heading.
    target: Option<WidgetId>,
    text: CachedText,
    rect: Rect,
}

/// Links to the sections of a [`ScrollView`] that highlight the section being read (a
/// scroll spy). The nav cannot see the scroll view, so the application calls
/// [`AnchorNav::sync`] after input and [`AnchorNav::scroll_to`] on [`AnchorSelected`].
#[derive(Debug)]
pub struct AnchorNav {
    scroll: WidgetId,
    entries: Vec<Entry>,
    anchors: Vec<usize>,
    active: Option<usize>,
    active_text: CachedText,
    margin: f32,
    synced: Option<f32>,
    hovered: Option<usize>,
    pressed: Option<usize>,
    cursor: usize,
    width: f32,
}

impl AnchorNav {
    /// The first anchor added is highlighted until the page scrolls.
    pub fn new(scroll: WidgetId) -> Self {
        Self {
            scroll,
            entries: Vec::new(),
            anchors: Vec::new(),
            active: Some(0),
            active_text: CachedText::default(),
            margin: 0.0,
            synced: Some(0.0),
            hovered: None,
            pressed: None,
            cursor: 0,
            width: 0.0,
        }
    }

    pub fn heading(mut self, title: impl Into<String>) -> Self {
        self.push(title.into(), None);
        self
    }

    /// `target` is a widget inside the scroll view.
    pub fn anchor(mut self, title: impl Into<String>, target: WidgetId) -> Self {
        self.anchors.push(self.entries.len());
        self.push(title.into(), Some(target));
        self
    }

    /// Usually the scroll view's top padding; 0 by default.
    pub fn scroll_margin(mut self, margin: f32) -> Self {
        self.margin = margin;
        self
    }

    fn push(&mut self, title: String, target: Option<WidgetId>) {
        self.entries.push(Entry {
            title,
            target,
            text: CachedText::default(),
            rect: Rect::default(),
        });
    }

    pub fn scroll_view(&self) -> WidgetId {
        self.scroll
    }

    pub fn anchor_count(&self) -> usize {
        self.anchors.len()
    }

    pub fn title(&self, index: usize) -> Option<&str> {
        let entry = self.anchors.get(index)?;
        Some(&self.entries[*entry].title)
    }

    pub fn target(&self, index: usize) -> Option<WidgetId> {
        self.entries[*self.anchors.get(index)?].target
    }

    pub fn active(&self) -> Option<usize> {
        self.active
    }

    /// Does not scroll.
    pub fn set_active(&mut self, cx: &mut UpdateCx, index: Option<usize>) {
        let index = index.filter(|&i| i < self.anchors.len());
        if index != self.active {
            self.active = index;
            if let Some(i) = index {
                self.cursor = i;
            }
            // The active title is bolder, so it is reshaped.
            cx.request_layout();
            cx.request_paint();
        }
    }

    /// Does nothing unless the page scrolled since the last call, so it is cheap to call
    /// on every event. Highlights the last section whose top is above a line a third of
    /// the way down the viewport. Returns whether the highlight changed.
    pub fn sync(tree: &mut UiTree, nav: WidgetId) -> bool {
        let Some(this) = tree.get::<AnchorNav>(nav) else {
            return false;
        };
        let scroll = this.scroll;
        let Some(offset) = tree.get::<ScrollView>(scroll).map(ScrollView::offset) else {
            return false;
        };
        // Only when the offset changed: input can arrive before the first layout, when
        // every section is still at the top.
        if this.synced == Some(offset) {
            return false;
        }
        let Some(view) = tree.window_rect(scroll) else {
            return false;
        };
        let probe = offset + this.margin + view.height / 3.0;
        let tops: Vec<Option<f32>> = (0..this.anchors.len())
            .map(|i| section_top(tree, this.target(i)?, view.y, offset))
            .collect();
        let active = tops
            .iter()
            .rposition(|top| top.is_some_and(|y| y <= probe))
            .unwrap_or(0);
        let before = this.active;
        tree.update::<AnchorNav, _>(nav, |n, cx| {
            n.synced = Some(offset);
            n.set_active(cx, Some(active));
        });
        before != Some(active)
    }

    /// Scrolls to the top of the scroll view, less the [`AnchorNav::scroll_margin`].
    pub fn scroll_to(tree: &mut UiTree, nav: WidgetId, index: usize) {
        let Some(this) = tree.get::<AnchorNav>(nav) else {
            return;
        };
        let scroll = this.scroll;
        let margin = this.margin;
        let (Some(target), Some(view), Some(offset)) = (
            this.target(index),
            tree.window_rect(scroll),
            tree.get::<ScrollView>(scroll).map(ScrollView::offset),
        ) else {
            return;
        };
        let Some(top) = section_top(tree, target, view.y, offset) else {
            return;
        };
        let to = (top - margin).max(0.0);
        tree.update::<ScrollView, _>(scroll, |s, cx| s.set_offset(cx, to));
        tree.update::<AnchorNav, _>(nav, |n, cx| {
            n.synced = Some(to);
            n.set_active(cx, Some(index));
        });
    }

    fn text_style(active: bool) -> TextStyle {
        TextStyle::new(14.0).weight(if active { 700 } else { 500 })
    }

    fn heading_style() -> TextStyle {
        TextStyle::new(11.0).weight(700)
    }

    fn anchor_at(&self, y: f32) -> Option<usize> {
        self.anchors
            .iter()
            .position(|&e| y >= self.entries[e].rect.y && y < self.entries[e].rect.bottom())
    }

    fn follow(&mut self, cx: &mut EventCx, index: usize) {
        self.cursor = index;
        cx.emit(AnchorSelected(index));
        cx.request_paint();
        cx.set_handled();
    }
}

/// The top of `target` in the scroll view's content, whose viewport starts at
/// `view_top` (window coordinates) and is scrolled by `offset`.
fn section_top(tree: &UiTree, target: WidgetId, view_top: f32, offset: f32) -> Option<f32> {
    Some(tree.window_rect(target)?.y - view_top + offset)
}

impl Widget for AnchorNav {
    fn style(&self) -> Style {
        Style {
            size: LayoutSize {
                width: percent(1.0),
                height: Dimension::auto(),
            },
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn measures(&self) -> bool {
        true
    }

    fn measure(&mut self, cx: &mut MeasureCx, _input: MeasureInput) -> Size {
        let active_entry = self.active.map(|i| self.anchors[i]);
        if let Some(e) = active_entry {
            self.active_text.shape(
                cx.text,
                &self.entries[e].title,
                Self::text_style(true),
                None,
            );
        }
        let mut y = 0.0;
        let mut width: f32 = 0.0;
        for (i, entry) in self.entries.iter_mut().enumerate() {
            let heading = entry.target.is_none();
            let style = if heading {
                Self::heading_style()
            } else {
                Self::text_style(false)
            };
            let size = entry.text.shape(cx.text, &entry.title, style, None);
            width = width.max(size.width + 2.0 * PAD_X);
            let (top, bottom) = if heading {
                (if i == 0 { 4.0 } else { HEADING_TOP }, HEADING_BOTTOM)
            } else {
                (ROW_PAD_Y, ROW_PAD_Y)
            };
            if i > 0 && !heading {
                y += ROW_GAP;
            }
            let height = top + size.height + bottom;
            entry.rect = Rect::new(0.0, y, 0.0, height);
            y += height;
        }
        Size::new(width, y)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let width = cx.size().width;
        self.width = width;
        let accent = cx.color(Role::Accent);
        let ink = cx.color(Role::Text);
        let muted = cx.color(Role::Muted);
        let hover_fill = cx.color(Role::SurfaceAlt);
        let active_fill = cx.theme().accent.with_alpha(0.14);
        let active_entry = self.active.map(|i| self.anchors[i]);
        let cursor_entry = self.anchors.get(self.cursor).copied();
        let hovered = self.hovered.map(|i| self.anchors[i]);
        for (e, entry) in self.entries.iter().enumerate() {
            let row = Rect::new(0.0, entry.rect.y, width, entry.rect.height);
            if entry.target.is_none() {
                if let Some(layout) = entry.text.layout() {
                    let size = entry.text.size();
                    let y = row.bottom() - HEADING_BOTTOM - size.height;
                    cx.text(Rect::new(PAD_X, y, size.width, size.height), layout, muted);
                }
                continue;
            }
            let active = active_entry == Some(e);
            if active {
                cx.rounded_rect(row, 6.0, active_fill, Border::default());
                cx.rounded_rect(
                    Rect::new(0.0, row.y + 6.0, 3.0, row.height - 12.0),
                    1.5,
                    accent,
                    Border::default(),
                );
            } else if hovered == Some(e) {
                cx.rounded_rect(row, 6.0, hover_fill, Border::default());
            }
            let text = if active {
                &self.active_text
            } else {
                &entry.text
            };
            if let Some(layout) = text.layout() {
                let size = text.size();
                let y = row.y + (row.height - size.height) / 2.0;
                let room = (width - 2.0 * PAD_X).max(0.0);
                cx.push_clip(Rect::new(PAD_X, row.y, room, row.height));
                let color = if active { accent } else { ink };
                cx.text(Rect::new(PAD_X, y, size.width, size.height), layout, color);
                cx.pop_clip();
            }
            if cx.is_focused() && cursor_entry == Some(e) {
                focus_ring(cx, row, 6.0);
            }
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            Event::PointerMove { pos } => {
                let hovered = self.anchor_at(cx.local(*pos).y);
                if hovered != self.hovered {
                    self.hovered = hovered;
                    cx.request_paint();
                }
            }
            Event::HoverChanged(false) => {
                if self.hovered.take().is_some() {
                    cx.request_paint();
                }
            }
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                self.pressed = self.anchor_at(cx.local(*pos).y);
                if self.pressed.is_some() {
                    cx.capture_pointer();
                    cx.set_handled();
                }
            }
            Event::PointerUp { pos, .. } => {
                let Some(pressed) = self.pressed.take() else {
                    return;
                };
                cx.release_pointer();
                if cx.contains(*pos) && self.anchor_at(cx.local(*pos).y) == Some(pressed) {
                    self.follow(cx, pressed);
                }
            }
            Event::KeyDown { key, .. } if !self.anchors.is_empty() => match key {
                Key::ArrowUp | Key::ArrowDown => {
                    let last = self.anchors.len() - 1;
                    let cursor = self.cursor.min(last);
                    self.cursor = if *key == Key::ArrowUp {
                        cursor.saturating_sub(1)
                    } else {
                        (cursor + 1).min(last)
                    };
                    cx.request_paint();
                    cx.set_handled();
                }
                Key::Enter | Key::Space => self.follow(cx, self.cursor.min(self.anchors.len() - 1)),
                _ => {}
            },
            Event::ActivateItem(i) if *i < self.anchors.len() => self.follow(cx, *i),
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Navigation)
    }

    fn accessibility_items(&self) -> Vec<accesskit::Node> {
        self.anchors
            .iter()
            .enumerate()
            .map(|(i, &e)| {
                let entry = &self.entries[e];
                let mut node = accesskit::Node::new(accesskit::Role::Link);
                node.set_label(entry.title.clone());
                node.set_bounds(accesskit::Rect {
                    x0: 0.0,
                    y0: f64::from(entry.rect.y),
                    x1: f64::from(self.width),
                    y1: f64::from(entry.rect.bottom()),
                });
                if self.active == Some(i) {
                    node.set_selected(true);
                }
                node.add_action(accesskit::Action::Click);
                node
            })
            .collect()
    }

    fn removed(&mut self, text: &mut TextStore) {
        for entry in &mut self.entries {
            entry.text.release(text);
        }
        self.active_text.release(text);
    }
}

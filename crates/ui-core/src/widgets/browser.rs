use noa_gpui_layout::Style;
use noa_gpui_layout::style::{Dimension, Size as LayoutSize, length, percent};
use noa_gpui_scene::{Border, Direction, Point, Rect, Size};
use noa_gpui_text::{TextStore, TextStyle};

use super::focus_ring;
use super::outline::{Node, OutlineItem, flatten};
use super::rows::TextCache;
use super::scroller::Scroller;
use super::table::{RowActivated, selection_fill};
use crate::event::{Event, Key, PointerButton};
use crate::theme::Role;
use crate::widget::{EventCx, PaintCx, UpdateCx, Widget};

const ROW: f32 = 22.0;
const PAD: f32 = 8.0;
const ICON: f32 = 16.0;

/// Emitted when the selected path of a [`BrowserView`] changes: one item per column,
/// from the root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserPathChanged(pub Vec<usize>);

/// A hierarchy as columns (`NSBrowser`). Return or a double click emits [`RowActivated`].
/// Items are identified as in [`super::OutlineView`]: by their position in a depth-first
/// walk of the items given.
pub struct BrowserView {
    nodes: Vec<Node>,
    roots: Vec<usize>,
    path: Vec<usize>,
    columns: Vec<Scroller>,
    sideways: Scroller,
    column_width: f32,
    width: Dimension,
    height: f32,
    titles: TextCache<usize>,
    last_click: Option<(f64, usize)>,
    size: Size,
}

impl std::fmt::Debug for BrowserView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrowserView")
            .field("items", &self.nodes.len())
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl BrowserView {
    pub fn new(items: Vec<OutlineItem>) -> Self {
        let (nodes, roots) = flatten(items);
        let mut sideways = Scroller::new();
        sideways.horizontal = true;
        sideways.vertical = false;
        Self {
            nodes,
            roots,
            path: Vec::new(),
            columns: Vec::new(),
            sideways,
            column_width: 180.0,
            width: percent(1.0),
            height: 260.0,
            titles: TextCache::default(),
            last_click: None,
            size: Size::default(),
        }
    }

    pub fn column_width(mut self, width: f32) -> Self {
        self.column_width = width;
        self
    }

    /// Fills the parent when not set.
    pub fn width(mut self, width: f32) -> Self {
        self.width = length(width);
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    /// From the root.
    pub fn path(&self) -> &[usize] {
        &self.path
    }

    pub fn title(&self, item: usize) -> &str {
        &self.nodes[item].title
    }

    /// How many columns are shown: one per selected item, plus the children of the
    /// last one if it has any.
    pub fn column_count(&self) -> usize {
        let open = self
            .path
            .last()
            .is_none_or(|&id| !self.nodes[id].children.is_empty());
        self.path.len() + usize::from(open)
    }

    pub fn column_items(&self, column: usize) -> &[usize] {
        if column == 0 {
            &self.roots
        } else {
            self.path
                .get(column - 1)
                .map_or(&[], |&id| &self.nodes[id].children)
        }
    }

    /// Select the path of items (each a child of the one before).
    pub fn set_path(&mut self, cx: &mut UpdateCx, path: Vec<usize>) {
        self.path = path;
        self.follow();
        cx.request_paint();
    }

    fn sync(&mut self, size: Size) {
        self.size = size;
        let count = self.column_count();
        while self.columns.len() < count {
            self.columns.push(Scroller::new());
        }
        let h = (size.height - 2.0).max(0.0);
        for c in 0..self.columns.len() {
            let x = 1.0 + c as f32 * self.column_width - self.sideways.offset.x;
            let rows = self.column_items(c).len() as f32;
            self.columns[c].set_geometry(
                Rect::new(x, 1.0, self.column_width, h),
                Size::new(self.column_width, rows * ROW),
            );
        }
        let content = Size::new(count as f32 * self.column_width, h);
        self.sideways
            .set_geometry(Rect::new(1.0, 1.0, (size.width - 2.0).max(0.0), h), content);
    }

    fn follow(&mut self) {
        let size = self.size;
        self.sync(size);
        let count = self.column_count();
        let x = (count as f32 * self.column_width - self.sideways.viewport().width).max(0.0);
        if x > self.sideways.offset.x {
            self.sideways.set_offset(Point::new(x, 0.0));
        }
        self.sync(size);
        for (c, &id) in self.path.iter().enumerate() {
            if let Some(row) = self.column_items(c).iter().position(|&i| i == id) {
                let rect = Rect::new(0.0, row as f32 * ROW, 1.0, ROW);
                self.columns[c].reveal(rect);
            }
        }
    }

    /// `p` is in widget coordinates.
    fn hit(&self, p: Point) -> Option<(usize, Option<usize>)> {
        let c = (0..self.column_count()).find(|&c| self.columns[c].viewport().contains(p))?;
        let v = self.columns[c].viewport();
        let row = ((p.y - v.y + self.columns[c].offset.y) / ROW).floor();
        let items = self.column_items(c);
        Some((
            c,
            (row >= 0.0 && (row as usize) < items.len()).then_some(row as usize),
        ))
    }

    fn select(&mut self, cx: &mut EventCx, column: usize, id: usize) {
        if self.path.len() == column + 1 && self.path[column] == id {
            return;
        }
        self.path.truncate(column);
        self.path.push(id);
        self.columns.truncate(self.column_count());
        self.follow();
        cx.emit(BrowserPathChanged(self.path.clone()));
        cx.request_paint();
    }

    fn key(&mut self, cx: &mut EventCx, key: Key) -> bool {
        let column = self.path.len().saturating_sub(1);
        match key {
            Key::ArrowUp | Key::ArrowDown => {
                let items = self.column_items(column).to_vec();
                if items.is_empty() {
                    return true;
                }
                let current = self
                    .path
                    .get(column)
                    .and_then(|id| items.iter().position(|i| i == id));
                let row = match (key, current) {
                    (Key::ArrowUp, Some(r)) => r.saturating_sub(1),
                    (Key::ArrowDown, Some(r)) => (r + 1).min(items.len() - 1),
                    (Key::ArrowUp, None) => items.len() - 1,
                    _ => 0,
                };
                self.select(cx, column, items[row]);
            }
            Key::ArrowRight => {
                if let Some(&id) = self.path.last()
                    && let Some(&first) = self.nodes[id].children.first()
                {
                    self.select(cx, self.path.len(), first);
                } else if self.path.is_empty()
                    && let Some(&first) = self.roots.first()
                {
                    self.select(cx, 0, first);
                }
            }
            Key::ArrowLeft => {
                if self.path.len() > 1 {
                    self.path.pop();
                    self.follow();
                    cx.emit(BrowserPathChanged(self.path.clone()));
                    cx.request_paint();
                }
            }
            Key::Enter => {
                if let Some(&id) = self.path.last() {
                    cx.emit(RowActivated(id));
                }
            }
            _ => return false,
        }
        true
    }
}

impl Widget for BrowserView {
    fn style(&self) -> Style {
        Style {
            size: LayoutSize {
                width: self.width,
                height: length(self.height),
            },
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        self.sync(cx.size());
        let bounds = cx.bounds();
        let surface = cx.color(Role::Surface);
        let outline = cx.color(Role::Outline);
        cx.rounded_rect(
            bounds,
            0.0,
            surface,
            Border {
                width: 1.0,
                color: outline,
            },
        );
        let focused = cx.is_focused();
        let (active_fill, active_ink) = selection_fill(cx, focused);
        let (path_fill, path_ink) = selection_fill(cx, false);
        let active_ink = cx.color(active_ink);
        let path_ink = cx.color(path_ink);
        let ink = cx.color(Role::Text);
        let muted = cx.color(Role::Muted);
        let line = cx.color(Role::Separator);
        let style = TextStyle::new(13.0);
        let area = self.sideways.viewport();
        cx.push_clip(area);
        self.titles.begin();
        for c in 0..self.column_count() {
            let v = self.columns[c].viewport();
            if v.right() < area.x || v.x > area.right() {
                continue;
            }
            cx.fill_rect(Rect::new(v.right() - 1.0, v.y, 1.0, v.height), line);
            cx.push_clip(Rect::new(v.x, v.y, v.width - 1.0, v.height));
            let off = self.columns[c].offset.y;
            let items = self.column_items(c).to_vec();
            let first = (off.max(0.0) / ROW) as usize;
            let last = (((off + v.height) / ROW).ceil() as usize).min(items.len());
            let active = c + 1 == self.path.len();
            for (row, &id) in items.iter().enumerate().take(last).skip(first) {
                let y = v.y + row as f32 * ROW - off;
                let selected = self.path.get(c) == Some(&id);
                let color = if selected {
                    let (fill, color) = if active {
                        (active_fill, active_ink)
                    } else {
                        (path_fill, path_ink)
                    };
                    cx.fill_rect(Rect::new(v.x, y, v.width - 1.0, ROW), fill);
                    color
                } else {
                    ink
                };
                let node = &self.nodes[id];
                let (icon, has_children) = (node.icon.clone(), !node.children.is_empty());
                let title = node.title.clone();
                let mut x = v.x + PAD;
                if let Some(icon) = icon {
                    let rect = Rect::new(x, y + (ROW - ICON) / 2.0, ICON, ICON);
                    icon.paint(cx, rect, color);
                    x += ICON + 6.0;
                }
                let right = v.right() - PAD - if has_children { 12.0 } else { 0.0 };
                if has_children {
                    let r = Rect::new(v.right() - PAD - 5.0, y + ROW / 2.0 - 4.0, 4.0, 8.0);
                    let tint = if selected { color } else { muted };
                    cx.chevron(r, Direction::Right, 1.5, tint);
                }
                let (layout, size) = self.titles.get(cx.text_store(), id, &title, style);
                if let Some(layout) = layout {
                    let room = right - x;
                    let clip = size.width > room;
                    if clip {
                        cx.push_clip(Rect::new(x, y, room.max(0.0), ROW));
                    }
                    let r = Rect::new(x, y + (ROW - size.height) / 2.0, size.width, size.height);
                    cx.text(r, layout, color);
                    if clip {
                        cx.pop_clip();
                    }
                }
            }
            cx.pop_clip();
            self.columns[c].paint(cx);
        }
        self.titles.end(cx.text_store());
        cx.pop_clip();
        self.sideways.paint(cx);
        if focused {
            focus_ring(cx, bounds, 0.0);
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        self.sync(cx.size());
        match event {
            Event::AnimationFrame { .. } => {
                self.sideways.event(cx, event);
                for column in &mut self.columns {
                    column.event(cx, event);
                }
            }
            Event::Wheel { pos, delta } => {
                let p = cx.local(*pos);
                let sideways = delta.x.abs() > delta.y.abs() || cx.modifiers().shift;
                if sideways {
                    self.sideways.event(cx, event);
                } else if let Some((c, _)) = self.hit(p) {
                    self.columns[c].event(cx, event);
                }
            }
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                let p = cx.local(*pos);
                if self.sideways.event(cx, event) {
                    return;
                }
                if let Some((c, _)) = self.hit(p)
                    && self.columns[c].event(cx, event)
                {
                    return;
                }
                cx.set_handled();
                if let Some((c, Some(row))) = self.hit(p) {
                    let id = self.column_items(c)[row];
                    let now = cx.now();
                    let double = self
                        .last_click
                        .is_some_and(|(t, last)| last == id && now - t <= 0.4);
                    self.last_click = Some((now, id));
                    self.select(cx, c, id);
                    if double {
                        cx.emit(RowActivated(id));
                        self.last_click = None;
                    }
                }
            }
            Event::PointerMove { .. } | Event::PointerUp { .. } => {
                if !self.sideways.event(cx, event) {
                    for column in &mut self.columns {
                        if column.event(cx, event) {
                            break;
                        }
                    }
                }
            }
            Event::KeyDown { key, .. } if cx.is_focused() && self.key(cx, *key) => {
                cx.set_handled();
            }
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Tree)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        let path: Vec<&str> = self
            .path
            .iter()
            .map(|&id| self.nodes[id].title.as_str())
            .collect();
        node.set_value(path.join(" › "));
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.titles.release(text);
    }
}

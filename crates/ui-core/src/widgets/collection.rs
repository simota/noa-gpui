use std::ops::Range;

use noa_gpui_layout::Style;
use noa_gpui_layout::style::{Dimension, Size as LayoutSize, length, percent};
use noa_gpui_scene::{Border, Color, ImageId, Point, Rect, Size};
use noa_gpui_text::{TextStore, TextStyle};

use super::focus_ring;
use super::inline_label::{ItemRenamed, RenameStep, Renamer};
use super::rows::{EdgeScroll, Selection, SelectionMode, TextCache};
use super::scroller::Scroller;
use super::table::{RowActivated, RowsMoved, selection_fill};
use crate::event::{Event, Key, PointerButton};
use crate::theme::{Corner, Role, mix};
use crate::widget::{EventCx, PaintCx, UpdateCx, Widget};

const TITLE: f32 = 22.0;
/// Seconds between the presses of a double click.
const DOUBLE_CLICK: f64 = 0.4;
const DRAG_START: f32 = 4.0;

#[derive(Debug, Clone, Copy)]
struct ItemDrag {
    /// Widget coordinates.
    at: Point,
    /// The press kept a selection of several items: if it is released without
    /// dragging, it selects only the pressed item.
    click: bool,
    dragging: bool,
    /// Where the items would go: before item `.0` (counting the dragged ones), shown in
    /// line `.1` at slot `.2` (0: before its first item).
    drop: Option<(usize, usize, usize)>,
}

#[derive(Debug, Clone)]
pub struct CollectionItem {
    title: String,
    image: Option<ImageId>,
    color: Option<Color>,
}

impl CollectionItem {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            image: None,
            color: None,
        }
    }

    pub fn image(mut self, image: ImageId) -> Self {
        self.image = Some(image);
        self
    }

    /// Drawn under the image when both are set.
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    pub fn title(&self) -> &str {
        &self.title
    }
}

/// A wrapping grid of items (`NSCollectionView` with a flow layout). Emits
/// [`super::RowSelectionChanged`], [`RowActivated`], [`ItemRenamed`] and [`RowsMoved`].
/// Only the rows on screen are drawn.
pub struct CollectionView {
    items: Vec<CollectionItem>,
    order: Vec<usize>,
    item_size: Size,
    spacing: f32,
    selection: Selection,
    scroller: Scroller,
    width: Dimension,
    height: f32,
    titles: TextCache<usize>,
    /// Marquee: where the drag started (content coordinates), the current corner, and
    /// the selection before it (for Cmd).
    marquee: Option<(Point, Point, Vec<usize>)>,
    last_click: Option<(f64, usize)>,
    size: Size,
    renamable: bool,
    renamer: Renamer,
    reorderable: bool,
    drag: Option<ItemDrag>,
    autoscroll: EdgeScroll,
}

impl std::fmt::Debug for CollectionView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CollectionView")
            .field("items", &self.items.len())
            .finish_non_exhaustive()
    }
}

impl CollectionView {
    pub fn new(items: Vec<CollectionItem>) -> Self {
        let n = items.len();
        Self {
            items,
            order: (0..n).collect(),
            item_size: Size::new(88.0, 88.0 + TITLE),
            spacing: 12.0,
            selection: Selection::new(SelectionMode::Multiple, n),
            scroller: Scroller::new(),
            width: percent(1.0),
            height: 320.0,
            titles: TextCache::default(),
            marquee: None,
            last_click: None,
            size: Size::default(),
            renamable: false,
            renamer: Renamer::default(),
            reorderable: false,
            drag: None,
            autoscroll: EdgeScroll::default(),
        }
    }

    pub fn reorderable(mut self, reorderable: bool) -> Self {
        self.reorderable = reorderable;
        self
    }

    /// `to` counts among the items left without `items`, as [`RowsMoved::to`] does. The
    /// selection and a rename follow the moved items. Returns whether anything moved.
    /// Emits nothing.
    pub fn move_items(&mut self, cx: &mut UpdateCx, items: &[usize], to: usize) -> bool {
        let moved = self.move_to(items, to);
        if moved {
            cx.request_paint();
        }
        moved
    }

    fn move_to(&mut self, items: &[usize], to: usize) -> bool {
        let n = self.items.len();
        let mut moving = vec![false; n];
        for &i in items {
            if let Some(m) = moving.get_mut(i) {
                *m = true;
            }
        }
        let mut order: Vec<usize> = (0..n).filter(|&i| !moving[i]).collect();
        let to = to.min(order.len());
        order.splice(to..to, (0..n).filter(|&i| moving[i]));
        if order.iter().enumerate().all(|(new, &old)| new == old) {
            return false;
        }
        let mut map = vec![None; n];
        for (new, &old) in order.iter().enumerate() {
            map[old] = Some(new);
        }
        let mut old: Vec<Option<CollectionItem>> = self.items.drain(..).map(Some).collect();
        self.items = order
            .iter()
            .map(|&i| old[i].take().expect("once"))
            .collect();
        self.selection.remap(n, &map);
        self.renamer.remap(|i| map.get(i).copied().flatten());
        true
    }

    /// `p` is in widget coordinates. Returns the item to insert before (counting the
    /// dragged ones), the line, and the slot in that line.
    fn drop_at(&self, p: Point) -> (usize, usize, usize) {
        let n = self.items.len();
        if n == 0 {
            return (0, 0, 0);
        }
        let c = self.to_content(p);
        let cols = self.columns();
        let lines = self.lines();
        let step = self.item_size.width + self.gap();
        let line = (c.y / self.line_height())
            .floor()
            .clamp(0.0, (lines - 1) as f32) as usize;
        let slot = ((c.x - self.gap() / 2.0) / step)
            .round()
            .clamp(0.0, cols as f32) as usize;
        let index = line * cols + slot;
        if index >= n {
            return (n, (n - 1) / cols, (n - 1) % cols + 1);
        }
        (index, line, slot)
    }

    fn key_move(&mut self, cx: &mut EventCx, key: Key) {
        let items = self.selection.ids();
        let (Some(&first), Some(&last)) = (items.first(), items.last()) else {
            return;
        };
        let cols = self.columns();
        let end = self.items.len() - 1;
        let stuck = match key {
            Key::ArrowLeft => first == 0,
            Key::ArrowRight => last == end,
            Key::ArrowUp => first < cols,
            _ => last / cols == end / cols,
        };
        if stuck {
            return;
        }
        let step = match key {
            Key::ArrowUp | Key::ArrowDown => cols,
            _ => 1,
        };
        // Places counted among the items left without the moving ones.
        let rest = self.items.len() - items.len();
        let at = match key {
            Key::ArrowLeft | Key::ArrowUp => first.saturating_sub(step),
            _ => (last + 1 - items.len() + step).min(rest),
        };
        if self.move_to(&items, at) {
            cx.emit(RowsMoved {
                rows: items,
                to: at,
            });
            self.changed(cx);
            if let Some(lead) = self.selection.lead
                && self.reveal(lead)
            {
                cx.request_paint();
            }
        }
    }

    /// `to` counts the selected items.
    fn drop_selected(&mut self, cx: &mut EventCx, to: usize) {
        let items = self.selection.ids();
        let at = to - items.iter().filter(|&&i| i < to).count();
        if self.move_to(&items, at) {
            cx.emit(RowsMoved {
                rows: items,
                to: at,
            });
            self.changed(cx);
            if let Some(lead) = self.selection.lead {
                self.reveal(lead);
            }
        }
        cx.request_paint();
    }

    pub fn renamable(mut self, renamable: bool) -> Self {
        self.renamable = renamable;
        self
    }

    /// Does not focus the view; call [`crate::UiTree::set_focus`] too.
    pub fn rename_item(&mut self, cx: &mut UpdateCx, item: usize) {
        if item < self.items.len() {
            self.start_rename(item);
            cx.request_paint();
        }
    }

    pub fn renaming(&self) -> Option<usize> {
        self.renamer.item()
    }

    pub fn rename_text(&self) -> Option<&str> {
        self.renamer.text()
    }

    fn start_rename(&mut self, item: usize) {
        self.reveal(item);
        self.renamer
            .start(item, &self.items[item].title, TextStyle::new(12.0));
    }

    /// Returns whether the rename editor took `event`.
    fn rename_event(&mut self, cx: &mut EventCx, event: &Event) -> bool {
        match self.renamer.event(cx, event) {
            RenameStep::Ignored => false,
            RenameStep::Handled => true,
            RenameStep::Done {
                item,
                name,
                pass_on,
            } => {
                if let Some(name) = name
                    && !name.trim().is_empty()
                    && self.items.get(item).is_some_and(|i| i.title != name)
                {
                    self.items[item].title.clone_from(&name);
                    cx.emit(ItemRenamed { item, name });
                }
                !pass_on
            }
        }
    }

    /// The title goes below the thumbnail.
    pub fn item_size(mut self, width: f32, height: f32) -> Self {
        self.item_size = Size::new(width, height + TITLE);
        self
    }

    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = spacing;
        self
    }

    pub fn selection_mode(mut self, mode: SelectionMode) -> Self {
        self.selection.mode = mode;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = length(width);
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    pub fn items(&self) -> &[CollectionItem] {
        &self.items
    }

    pub fn selected_items(&self) -> Vec<usize> {
        self.selection.ids()
    }

    pub fn lead_item(&self) -> Option<usize> {
        self.selection.lead
    }

    pub fn columns(&self) -> usize {
        let w = self.viewport().width - self.spacing;
        ((w / (self.item_size.width + self.spacing)).floor() as usize).max(1)
    }

    /// Includes items whose row is only partly inside the view.
    pub fn visible_items(&self) -> Range<usize> {
        let (first, last) = self.visible_lines();
        let cols = self.columns();
        (first * cols).min(self.items.len())..(last * cols).min(self.items.len())
    }

    /// Cancels a rename in progress.
    pub fn set_items(&mut self, cx: &mut UpdateCx, items: Vec<CollectionItem>) {
        self.renamer.cancel();
        self.drag = None;
        self.marquee = None;
        self.last_click = None;
        self.autoscroll.stop();
        let n = items.len();
        self.items = items;
        self.order = (0..n).collect();
        self.selection.resize(n);
        cx.request_paint();
    }

    pub fn select_items(&mut self, cx: &mut UpdateCx, items: impl IntoIterator<Item = usize>) {
        self.selection.set_many(items);
        if let Some(lead) = self.selection.lead {
            self.reveal(lead);
        }
        cx.request_paint();
    }

    fn viewport(&self) -> Rect {
        Rect::new(
            1.0,
            1.0,
            (self.size.width - 2.0).max(0.0),
            (self.size.height - 2.0).max(0.0),
        )
    }

    fn line_height(&self) -> f32 {
        self.item_size.height + self.spacing
    }

    fn lines(&self) -> usize {
        self.items.len().div_ceil(self.columns())
    }

    fn visible_lines(&self) -> (usize, usize) {
        let lh = self.line_height();
        let off = self.scroller.offset.y - self.spacing;
        let first = (off.max(0.0) / lh) as usize;
        let last = ((off + self.viewport().height) / lh).ceil().max(0.0) as usize;
        (first, last.min(self.lines()))
    }

    fn gap(&self) -> f32 {
        let cols = self.columns() as f32;
        ((self.viewport().width - cols * self.item_size.width) / (cols + 1.0)).max(self.spacing)
    }

    /// Content coordinates.
    fn item_rect(&self, i: usize) -> Rect {
        let cols = self.columns();
        let gap = self.gap();
        let (col, line) = (i % cols, i / cols);
        Rect::new(
            gap + col as f32 * (self.item_size.width + gap),
            self.spacing + line as f32 * self.line_height(),
            self.item_size.width,
            self.item_size.height,
        )
    }

    fn to_content(&self, p: Point) -> Point {
        let v = self.viewport();
        Point::new(p.x - v.x, p.y - v.y + self.scroller.offset.y)
    }

    fn item_at(&self, p: Point) -> Option<usize> {
        if !self.viewport().contains(p) {
            return None;
        }
        let c = self.to_content(p);
        let cols = self.columns();
        let gap = self.gap();
        let line = ((c.y - self.spacing) / self.line_height()).floor();
        let col = ((c.x - gap) / (self.item_size.width + gap)).floor();
        if line < 0.0 || col < 0.0 || col as usize >= cols {
            return None;
        }
        let i = line as usize * cols + col as usize;
        (i < self.items.len() && self.item_rect(i).contains(c)).then_some(i)
    }

    fn sync(&mut self, size: Size) {
        self.size = size;
        let height = self.lines() as f32 * self.line_height() + self.spacing;
        let content = Size::new(self.viewport().width, height);
        self.scroller.set_geometry(self.viewport(), content);
    }

    fn reveal(&mut self, i: usize) -> bool {
        let r = self.item_rect(i);
        let rect = Rect::new(0.0, r.y - self.spacing, 1.0, r.height + 2.0 * self.spacing);
        self.scroller.reveal(rect)
    }

    fn changed(&mut self, cx: &mut EventCx) {
        cx.emit(self.selection.message());
        cx.request_paint();
    }

    fn update_marquee(&mut self, cx: &mut EventCx) {
        let Some((start, end, before)) = &self.marquee else {
            return;
        };
        let area = Rect::new(
            start.x.min(end.x),
            start.y.min(end.y),
            (start.x - end.x).abs(),
            (start.y - end.y).abs(),
        );
        let lh = self.line_height();
        let first = ((area.y - self.spacing) / lh).floor().max(0.0) as usize;
        let last = ((area.bottom() - self.spacing) / lh).ceil().max(0.0) as usize;
        let cols = self.columns();
        let mut marks = vec![false; self.items.len()];
        for &i in before {
            marks[i] = true;
        }
        let end = (last * cols).min(self.items.len());
        for (i, mark) in marks.iter_mut().enumerate().take(end).skip(first * cols) {
            let r = self.item_rect(i);
            *mark |= r.x < area.right()
                && area.x < r.right()
                && r.y < area.bottom()
                && area.y < r.bottom();
        }
        let ids: Vec<usize> = (0..marks.len()).filter(|&i| marks[i]).collect();
        if ids != self.selection.ids() {
            self.selection.set_many(ids);
            cx.emit(self.selection.message());
        }
        cx.request_paint();
    }

    fn key(&mut self, cx: &mut EventCx, key: Key, shift: bool, command: bool) -> bool {
        let n = self.items.len();
        if n == 0 {
            return false;
        }
        let cols = self.columns();
        let lead = self.selection.lead;
        let next = match (key, lead) {
            (Key::Char('a'), _) if command => {
                if self.selection.select_all(&self.order) {
                    self.changed(cx);
                }
                return true;
            }
            (Key::Enter, Some(id)) => {
                if self.renamable {
                    self.start_rename(id);
                    cx.request_paint();
                } else {
                    cx.emit(RowActivated(id));
                }
                return true;
            }
            (Key::ArrowLeft | Key::ArrowUp | Key::ArrowRight | Key::ArrowDown, None) => 0,
            (Key::ArrowLeft, Some(i)) => i.saturating_sub(1),
            (Key::ArrowRight, Some(i)) => (i + 1).min(n - 1),
            (Key::ArrowUp, Some(i)) => i.checked_sub(cols).unwrap_or(i),
            (Key::ArrowDown, Some(i)) => {
                if i + cols < n {
                    i + cols
                } else if i / cols + 1 < self.lines() {
                    n - 1
                } else {
                    i
                }
            }
            (Key::Home, _) => 0,
            (Key::End, _) => n - 1,
            _ => return false,
        };
        if self.selection.move_to(&self.order, next, shift) {
            self.changed(cx);
        }
        if self.reveal(next) {
            self.scroller.flash(cx);
            cx.request_paint();
        }
        true
    }
}

impl Widget for CollectionView {
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
        self.renamer.begin_paint(cx.text_store());
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
        let v = self.viewport();
        let off = self.scroller.offset.y;
        let focused = cx.is_focused();
        let (selected_fill, selected_ink) = selection_fill(cx, focused);
        let selected_ink = cx.color(selected_ink);
        let ink = cx.color(Role::Text);
        let well = mix(surface, ink, 0.06);
        let style = TextStyle::new(12.0);
        let thumb_h = self.item_size.height - TITLE;

        cx.push_clip(v);
        self.titles.begin();
        for i in self.visible_items() {
            let r = self.item_rect(i);
            let r = Rect::new(v.x + r.x, v.y + r.y - off, r.width, r.height);
            let selected = self.selection.is_selected(i);
            let thumb = Rect::new(r.x, r.y, r.width, thumb_h);
            if selected {
                let ring = Rect::new(
                    thumb.x - 4.0,
                    thumb.y - 4.0,
                    thumb.width + 8.0,
                    thumb_h + 8.0,
                );
                let radius = cx.corner(Corner::Container, 8.0, ring.height);
                cx.rounded_rect(ring, radius, mix(surface, ink, 0.12), Border::default());
            }
            let item = &self.items[i];
            let (image, color) = (item.image, item.color);
            let title = item.title.clone();
            let radius = cx.corner(Corner::Container, 6.0, thumb.height);
            cx.rounded_rect(thumb, radius, color.unwrap_or(well), Border::default());
            if let Some(image) = image {
                let side = thumb.width.min(thumb.height) * 0.6;
                cx.image(
                    Rect::new(
                        thumb.x + (thumb.width - side) / 2.0,
                        thumb.y + (thumb.height - side) / 2.0,
                        side,
                        side,
                    ),
                    image,
                );
            }
            if let Some(editor) = self.renamer.editor_in(i, v) {
                let size = editor.shape(cx.text_store());
                editor.max_width = r.width;
                let w = editor.rect().width;
                editor.at = Point::new(
                    r.x + (r.width - w) / 2.0 + 3.0,
                    thumb.bottom() + (TITLE - size.height) / 2.0 + 2.0,
                );
                editor.paint(cx);
                continue;
            }
            let (layout, size) = self.titles.get(cx.text_store(), i, &title, style);
            if let Some(layout) = layout {
                let w = size.width.min(r.width);
                let label = Rect::new(
                    r.x + (r.width - w) / 2.0,
                    thumb.bottom() + (TITLE - size.height) / 2.0 + 2.0,
                    w,
                    size.height,
                );
                if selected {
                    let pill = Rect::new(label.x - 6.0, label.y - 1.0, w + 12.0, size.height + 2.0);
                    let radius = cx.corner(Corner::Inner, 5.0, pill.height);
                    cx.rounded_rect(pill, radius, selected_fill, Border::default());
                }
                let clip = size.width > r.width;
                if clip {
                    cx.push_clip(Rect::new(r.x, label.y, r.width, size.height));
                }
                let x = if clip { r.x } else { label.x };
                cx.text(
                    Rect::new(x, label.y, size.width, size.height),
                    layout,
                    if selected { selected_ink } else { ink },
                );
                if clip {
                    cx.pop_clip();
                }
            }
        }
        self.titles.end(cx.text_store());
        if let Some(ItemDrag {
            dragging: true,
            drop: Some((_, line, slot)),
            ..
        }) = self.drag
        {
            let accent = cx.color(Role::Accent);
            let x = v.x + self.gap() / 2.0 + slot as f32 * (self.item_size.width + self.gap());
            let y = v.y + self.spacing + line as f32 * self.line_height() - off;
            let bar = Rect::new(x - 1.5, y, 3.0, self.item_size.height);
            cx.rounded_rect(bar, 1.5, accent, Border::default());
        }
        if let Some((start, end, _)) = &self.marquee {
            let accent = cx.color(Role::Accent);
            let area = Rect::new(
                v.x + start.x.min(end.x),
                v.y + start.y.min(end.y) - off,
                (start.x - end.x).abs(),
                (start.y - end.y).abs(),
            );
            cx.rounded_rect(
                area,
                0.0,
                accent.with_alpha(0.15),
                Border {
                    width: 1.0,
                    color: accent,
                },
            );
        }
        cx.pop_clip();
        self.scroller.paint(cx);
        if focused {
            focus_ring(cx, bounds, 0.0);
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        self.sync(cx.size());
        if self.rename_event(cx, event) {
            return;
        }
        if let Event::AnimationFrame { now } = event
            && let Some(p) = self
                .autoscroll
                .step(cx, self.viewport(), &mut self.scroller, *now)
        {
            // The items moved under the pointer: so does the drop place.
            let drop = self.drop_at(p);
            if let Some(drag) = &mut self.drag {
                drag.drop = Some(drop);
            }
        }
        if self.scroller.event(cx, event) {
            return;
        }
        match event {
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                let p = cx.local(*pos);
                cx.set_handled();
                match self.item_at(p) {
                    Some(i) => {
                        let now = cx.now();
                        let double = self
                            .last_click
                            .is_some_and(|(t, last)| last == i && now - t <= DOUBLE_CLICK);
                        self.last_click = Some((now, i));
                        let modifiers = cx.modifiers();
                        // Pressing an already selected item keeps the selection, so the
                        // items can be dragged together; the click lands on release.
                        let keep = self.reorderable
                            && self.selection.is_selected(i)
                            && !modifiers.command
                            && !modifiers.shift;
                        if !keep && self.selection.click(&self.order, i, modifiers) {
                            self.changed(cx);
                        }
                        if double {
                            cx.emit(RowActivated(i));
                            self.last_click = None;
                        }
                        if self.reorderable && self.selection.is_selected(i) {
                            self.drag = Some(ItemDrag {
                                at: p,
                                click: keep,
                                dragging: false,
                                drop: None,
                            });
                            cx.capture_pointer();
                        }
                    }
                    None if self.viewport().contains(p)
                        && self.selection.mode == SelectionMode::Multiple =>
                    {
                        let before = if cx.modifiers().command {
                            self.selection.ids()
                        } else {
                            Vec::new()
                        };
                        let start = self.to_content(p);
                        self.marquee = Some((start, start, before));
                        cx.capture_pointer();
                        self.update_marquee(cx);
                    }
                    None => {
                        if self.selection.count() > 0 {
                            self.selection.set(None);
                            self.changed(cx);
                        }
                    }
                }
            }
            Event::PointerMove { pos } if self.drag.is_some() => {
                let p = cx.local(*pos);
                let Some(drag) = self.drag else { return };
                let moved = (p.y - drag.at.y).abs().max((p.x - drag.at.x).abs()) >= DRAG_START;
                if moved || drag.dragging {
                    let drop = Some(self.drop_at(p));
                    if !drag.dragging || drop != drag.drop {
                        self.drag = Some(ItemDrag {
                            dragging: true,
                            drop,
                            ..drag
                        });
                        cx.request_paint();
                    }
                    let v = self.viewport();
                    self.autoscroll.track(cx, v, &self.scroller, p);
                }
                cx.set_handled();
            }
            Event::PointerUp { pos, .. } if self.drag.is_some() => {
                let p = cx.local(*pos);
                self.autoscroll.stop();
                let Some(drag) = self.drag.take() else { return };
                cx.set_handled();
                if drag.dragging {
                    // Where the pointer is now: the items may have scrolled (by the
                    // wheel) since it last moved.
                    let (to, ..) = self.drop_at(p);
                    self.drop_selected(cx, to);
                    cx.request_paint();
                } else if drag.click
                    && let Some(i) = self.item_at(p)
                    && self.selection.click(&self.order, i, cx.modifiers())
                {
                    self.changed(cx);
                }
            }
            Event::KeyDown {
                key: Key::Escape, ..
            } if self.drag.is_some_and(|d| d.dragging) => {
                self.drag = None;
                self.autoscroll.stop();
                cx.set_handled();
                cx.request_paint();
            }
            Event::PointerMove { pos } => {
                let c = self.to_content(cx.local(*pos));
                if let Some((_, end, _)) = &mut self.marquee {
                    *end = c;
                    self.update_marquee(cx);
                    cx.set_handled();
                }
            }
            Event::PointerUp { .. } => {
                if self.marquee.take().is_some() {
                    cx.release_pointer();
                    cx.request_paint();
                    cx.set_handled();
                }
            }
            Event::KeyDown {
                key: key @ (Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown),
                modifiers,
            } if cx.is_focused() && self.reorderable && modifiers.command && modifiers.alt => {
                self.key_move(cx, *key);
                cx.set_handled();
            }
            Event::KeyDown { key, modifiers } if cx.is_focused() => {
                if self.key(cx, *key, modifiers.shift, modifiers.command) {
                    cx.set_handled();
                }
            }
            Event::ActivateItem(i) => {
                let i = self.visible_items().start + i;
                if i < self.items.len() {
                    self.selection.set(Some(i));
                    self.changed(cx);
                }
            }
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn text_input_area(&self) -> Option<Rect> {
        self.renamer.input_area()
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Grid)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        self.scroller.accessibility(node);
    }

    fn accessibility_items(&self) -> Vec<accesskit::Node> {
        let v = self.viewport();
        self.visible_items()
            .map(|i| {
                let r = self.item_rect(i);
                let y = v.y + r.y - self.scroller.offset.y;
                let mut node = accesskit::Node::new(accesskit::Role::GridCell);
                node.set_label(self.items[i].title.clone());
                node.set_selected(self.selection.is_selected(i));
                node.set_bounds(accesskit::Rect {
                    x0: f64::from(v.x + r.x),
                    y0: f64::from(y),
                    x1: f64::from(v.x + r.right()),
                    y1: f64::from(y + r.height),
                });
                node.add_action(accesskit::Action::Click);
                node
            })
            .collect()
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.titles.release(text);
        self.renamer.removed(text);
    }
}

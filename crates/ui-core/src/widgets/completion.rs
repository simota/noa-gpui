use std::collections::HashSet;
use std::ops::Range;
use std::rc::Rc;

use noa_gpui_layout::{MeasureInput, Style};
use noa_gpui_scene::{Border, Rect, Size};
use noa_gpui_text::{TextStore, TextStyle};

use super::appear::shadow;
use super::{CachedText, fuzzy_match};
use crate::event::{Event, Key, Modifiers, PointerButton};
use crate::overlay::Placement;
use crate::theme::{Corner, Role};
use crate::widget::{EventCx, MeasureCx, PaintCx, Widget};

/// Called on every edit with the text being completed.
pub type CompletionSource = Rc<dyn Fn(&str) -> Vec<String>>;

const ROW: f32 = 24.0;
const PAD: f32 = 4.0;
const MAX_ROWS: usize = 8;
const CHECK: f32 = 22.0;
const INSET: f32 = 8.0;

/// One completion shown, with the bytes the query matched.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Shown {
    pub text: String,
    pub ranges: Vec<Range<usize>>,
    pub checked: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct ListState {
    items: Vec<Shown>,
    highlight: usize,
    multiple: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListInput {
    Hover(usize),
    /// A row was clicked: pick it, or (on its check box, or with ⌘) check it.
    Click {
        index: usize,
        toggle: bool,
    },
}

#[derive(Debug)]
struct CompletionList {
    state: ListState,
    width: f32,
    first: usize,
    texts: Vec<CachedText>,
}

impl CompletionList {
    fn rows(&self) -> usize {
        self.state.items.len().clamp(1, MAX_ROWS)
    }

    fn row_at(&self, y: f32) -> Option<usize> {
        let row = ((y - PAD) / ROW).floor();
        (row >= 0.0 && (row as usize) < self.rows())
            .then(|| self.first + row as usize)
            .filter(|&i| i < self.state.items.len())
    }

    fn reveal(&mut self) {
        let rows = self.rows();
        let h = self.state.highlight;
        if h < self.first {
            self.first = h;
        } else if h >= self.first + rows {
            self.first = h + 1 - rows;
        }
        self.first = self.first.min(self.state.items.len().saturating_sub(rows));
    }
}

impl Widget for CompletionList {
    fn style(&self) -> Style {
        Style {
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn measures(&self) -> bool {
        true
    }

    fn measure(&mut self, cx: &mut MeasureCx, _input: MeasureInput) -> Size {
        while self.texts.len() < self.state.items.len() {
            self.texts.push(CachedText::default());
        }
        for (cached, item) in self.texts.iter_mut().zip(&self.state.items) {
            cached.shape(cx.text, &item.text, TextStyle::new(13.0), None);
        }
        for cached in &mut self.texts[self.state.items.len()..] {
            cached.release(cx.text);
        }
        Size::new(self.width, 2.0 * PAD + self.rows() as f32 * ROW)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let radius = cx.corner(Corner::Container, 7.0, bounds.height);
        shadow(cx, bounds, radius);
        let fill = cx.color(Role::Surface);
        let border = Border {
            width: 1.0,
            color: cx.color(Role::Outline),
        };
        cx.floating_surface(bounds, radius, fill, border);
        let ink = cx.color(Role::Text);
        let on_accent = cx.color(Role::OnAccent);
        let accent = cx.color(Role::Accent);
        let outline = cx.color(Role::Outline);
        let mark = cx.theme().accent.with_alpha(0.28);
        let multiple = self.state.multiple;
        for row in 0..self.rows() {
            let index = self.first + row;
            let Some(item) = self.state.items.get(index) else {
                break;
            };
            let rect = Rect::new(PAD, PAD + row as f32 * ROW, bounds.width - 2.0 * PAD, ROW);
            let highlighted = index == self.state.highlight;
            if highlighted {
                let radius = cx.corner(Corner::Inner, 4.0, ROW);
                cx.rounded_rect(rect, radius, accent, Border::default());
            }
            let text_ink = if highlighted { on_accent } else { ink };
            let mut x = rect.x + INSET;
            if multiple {
                let side = 13.0;
                let check = Rect::new(x, rect.y + (ROW - side) / 2.0, side, side);
                let (fill, edge) = match (item.checked, highlighted) {
                    (true, false) => (accent, accent),
                    (true, true) => (on_accent, on_accent),
                    (false, true) => (noa_gpui_scene::Color::TRANSPARENT, on_accent),
                    (false, false) => (noa_gpui_scene::Color::TRANSPARENT, outline),
                };
                cx.rounded_rect(
                    check,
                    3.0,
                    fill,
                    Border {
                        width: 1.0,
                        color: edge,
                    },
                );
                if item.checked {
                    let tick = if highlighted { accent } else { on_accent };
                    let inner = Rect::new(check.x + 2.5, check.y + 3.0, side - 5.0, side - 6.0);
                    cx.checkmark(inner, 1.5, tick);
                }
                x += CHECK;
            }
            let Some(cached) = self.texts.get(index) else {
                continue;
            };
            let Some(layout) = cached.layout() else {
                continue;
            };
            let size = cached.size();
            let y = rect.y + (ROW - size.height) / 2.0;
            cx.push_clip(Rect::new(
                x,
                rect.y,
                (rect.right() - x - INSET).max(0.0),
                ROW,
            ));
            if !highlighted {
                for range in &item.ranges {
                    let spans = cx
                        .text_store()
                        .selection_rects(layout, range.start, range.end);
                    for r in spans {
                        let r = Rect::new(x + r.x, y + r.y + 1.0, r.width, r.height - 2.0);
                        cx.rounded_rect(r, 3.0, mark, Border::default());
                    }
                }
            }
            cx.text(Rect::new(x, y, size.width, size.height), layout, text_ink);
            if highlighted {
                for range in &item.ranges {
                    let spans = cx
                        .text_store()
                        .selection_rects(layout, range.start, range.end);
                    for r in spans {
                        let line = Rect::new(x + r.x, y + r.bottom() - 2.0, r.width, 1.5);
                        cx.fill_rect(line, on_accent);
                    }
                }
            }
            cx.pop_clip();
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            Event::FromOwner(message) => {
                if let Some(state) = message.downcast_ref::<ListState>() {
                    self.state = state.clone();
                    self.reveal();
                    cx.request_layout();
                    cx.request_paint();
                }
            }
            Event::PointerMove { pos } => {
                if let Some(i) = self.row_at(cx.local(*pos).y)
                    && i != self.state.highlight
                {
                    self.state.highlight = i;
                    cx.notify_owner(ListInput::Hover(i));
                    cx.request_paint();
                }
            }
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                let p = cx.local(*pos);
                if let Some(index) = self.row_at(p.y) {
                    let on_check = self.state.multiple && p.x < PAD + INSET + CHECK;
                    let toggle = self.state.multiple && (on_check || cx.modifiers().command);
                    cx.notify_owner(ListInput::Click { index, toggle });
                }
                cx.set_handled();
            }
            Event::Wheel { delta, .. } => {
                let max_first = self.state.items.len().saturating_sub(self.rows());
                if delta.y > 0.0 {
                    self.first = self.first.saturating_sub(1);
                } else if delta.y < 0.0 {
                    self.first = (self.first + 1).min(max_first);
                }
                cx.request_paint();
                cx.set_handled();
            }
            _ => {}
        }
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::ListBox)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        if self.state.multiple {
            node.set_multiselectable();
        }
    }

    fn accessibility_items(&self) -> Vec<accesskit::Node> {
        (self.first..self.state.items.len())
            .take(self.rows())
            .enumerate()
            .map(|(row, index)| {
                let item = &self.state.items[index];
                let mut node = accesskit::Node::new(accesskit::Role::ListBoxOption);
                node.set_label(item.text.clone());
                node.set_selected(index == self.state.highlight);
                if self.state.multiple {
                    node.set_toggled(if item.checked {
                        accesskit::Toggled::True
                    } else {
                        accesskit::Toggled::False
                    });
                }
                let y = PAD + row as f32 * ROW;
                node.set_bounds(accesskit::Rect {
                    x0: f64::from(PAD),
                    y0: f64::from(y),
                    x1: f64::from(self.width - PAD),
                    y1: f64::from(y + ROW),
                });
                node
            })
            .collect()
    }

    fn removed(&mut self, text: &mut TextStore) {
        for cached in &mut self.texts {
            cached.release(text);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Completed {
    /// Nothing for the field to do (the list changed, or it was not the list's).
    Nothing,
    Accept(Vec<String>),
}

/// Owned by the field, which passes it what is typed and its keys.
pub(crate) struct Completer {
    items: Vec<String>,
    source: Option<CompletionSource>,
    pub multiple: bool,
    shown: Vec<Shown>,
    highlight: usize,
    open: bool,
    /// The text completed last (to skip asking the source again).
    query: Option<String>,
}

impl std::fmt::Debug for Completer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Completer")
            .field("items", &self.items.len())
            .field("dynamic", &self.source.is_some())
            .field("shown", &self.shown.len())
            .field("open", &self.open)
            .finish()
    }
}

impl Completer {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            source: None,
            multiple: false,
            shown: Vec::new(),
            highlight: 0,
            open: false,
            query: None,
        }
    }

    pub fn is_enabled(&self) -> bool {
        !self.items.is_empty() || self.source.is_some()
    }

    pub fn set_items(&mut self, items: Vec<String>) {
        self.items = items;
        self.query = None;
    }

    pub fn set_source(&mut self, source: CompletionSource) {
        self.source = Some(source);
        self.query = None;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn shown(&self) -> Vec<String> {
        self.shown.iter().map(|s| s.text.clone()).collect()
    }

    pub fn highlighted(&self) -> Option<usize> {
        (self.open && !self.shown.is_empty()).then_some(self.highlight)
    }

    pub fn checked(&self) -> Vec<String> {
        self.shown
            .iter()
            .filter(|s| s.checked)
            .map(|s| s.text.clone())
            .collect()
    }

    /// `exclude` is matched case-insensitively. A fixed list is ranked best match first;
    /// a source's order is kept.
    pub fn update(&mut self, cx: &mut EventCx, query: &str, exclude: &[String], anchor: Rect) {
        if self.query.as_deref() == Some(query) {
            return;
        }
        self.query = Some(query.to_owned());
        let checked: Vec<String> = self.checked();
        // Lowercased once, not once per item.
        let exclude: HashSet<String> = exclude.iter().map(|e| e.to_lowercase()).collect();
        let excluded = |c: &str| !exclude.is_empty() && exclude.contains(&c.to_lowercase());
        let mut shown: Vec<(i32, Shown)> = Vec::new();
        if !query.trim().is_empty() {
            match &self.source {
                Some(source) => {
                    for (order, text) in source(query).into_iter().enumerate() {
                        if excluded(&text) {
                            continue;
                        }
                        let ranges = fuzzy_match(query, &text)
                            .map(|m| m.ranges)
                            .unwrap_or_default();
                        shown.push((
                            -(order as i32),
                            Shown {
                                checked: checked.contains(&text),
                                text,
                                ranges,
                            },
                        ));
                    }
                }
                None => {
                    for text in &self.items {
                        if excluded(text) {
                            continue;
                        }
                        if let Some(m) = fuzzy_match(query, text) {
                            shown.push((
                                m.score,
                                Shown {
                                    checked: checked.contains(text),
                                    text: text.clone(),
                                    ranges: m.ranges,
                                },
                            ));
                        }
                    }
                }
            }
        }
        shown.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
        self.shown = shown.into_iter().map(|(_, s)| s).collect();
        self.highlight = 0;
        self.sync(cx, anchor);
    }

    fn state(&self) -> ListState {
        ListState {
            items: self.shown.clone(),
            highlight: self.highlight,
            multiple: self.multiple,
        }
    }

    fn sync(&mut self, cx: &mut EventCx, anchor: Rect) {
        if self.shown.is_empty() {
            self.close(cx);
            return;
        }
        if self.open {
            cx.notify_popup(self.state());
        } else {
            self.open = true;
            let list = CompletionList {
                state: self.state(),
                width: anchor.width.max(160.0),
                first: 0,
                texts: Vec::new(),
            };
            cx.open_popup(list, Placement::Below(anchor));
        }
    }

    pub fn close(&mut self, cx: &mut EventCx) {
        if self.open {
            self.open = false;
            cx.close_child_menus();
        }
    }

    pub fn reset(&mut self, cx: &mut EventCx) {
        self.close(cx);
        self.shown.clear();
        self.query = None;
    }

    /// The completions Return takes: the checked ones, or else the highlighted one.
    fn accepted(&self) -> Vec<String> {
        let checked = self.checked();
        if !checked.is_empty() {
            return checked;
        }
        self.shown
            .get(self.highlight)
            .map(|s| vec![s.text.clone()])
            .unwrap_or_default()
    }

    /// Returns `None` when the key is not the list's.
    pub fn key(&mut self, cx: &mut EventCx, key: Key, modifiers: Modifiers) -> Option<Completed> {
        if !self.open || self.shown.is_empty() {
            return None;
        }
        let n = self.shown.len();
        match key {
            Key::ArrowDown => self.highlight = (self.highlight + 1) % n,
            Key::ArrowUp => self.highlight = (self.highlight + n - 1) % n,
            Key::Escape => {
                self.close(cx);
                return Some(Completed::Nothing);
            }
            Key::Enter if self.multiple && modifiers.command => {
                let item = &mut self.shown[self.highlight];
                item.checked = !item.checked;
            }
            Key::Enter | Key::Tab if !modifiers.shift => {
                let accepted = self.accepted();
                return Some(Completed::Accept(accepted));
            }
            _ => return None,
        }
        cx.notify_popup(self.state());
        Some(Completed::Nothing)
    }

    pub fn event(&mut self, cx: &mut EventCx, event: &Event) -> Option<Completed> {
        match event {
            Event::PopoverClosed if self.open => {
                self.open = false;
                Some(Completed::Nothing)
            }
            Event::FromPopover(message) => {
                let input = *message.downcast_ref::<ListInput>()?;
                match input {
                    ListInput::Hover(i) => {
                        self.highlight = i;
                        Some(Completed::Nothing)
                    }
                    ListInput::Click { index, toggle } => {
                        let item = self.shown.get_mut(index)?;
                        if toggle {
                            item.checked = !item.checked;
                            self.highlight = index;
                            cx.notify_popup(self.state());
                            return Some(Completed::Nothing);
                        }
                        let picked = item.text.clone();
                        let mut accepted = self.checked();
                        if !accepted.contains(&picked) {
                            accepted.push(picked);
                        }
                        Some(Completed::Accept(accepted))
                    }
                }
            }
            _ => None,
        }
    }
}

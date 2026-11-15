use noa_gpui_layout::{MeasureInput, Style};
use noa_gpui_scene::{Border, Direction, Point, Rect, Size};
use noa_gpui_text::TextStore;

use super::completion::{Completed, Completer};
use super::editable::SingleLine;
use super::text_field::{measure_field, paint_field_box};
use super::{MenuItem, SelectionChanged, TextChanged};
use crate::event::{Cursor, Event, Key, PointerButton};
use crate::overlay::Placement;
use crate::theme::{Corner, Role};
use crate::widget::{EventCx, MeasureCx, PaintCx, UpdateCx, Widget};

/// A password field (`NSSecureTextField`). Keeps the IME off, as macOS secure input
/// does. Emits [`TextChanged`] and [`super::TextSubmitted`].
#[derive(Debug)]
pub struct SecureTextField {
    line: SingleLine,
}

impl SecureTextField {
    pub fn new(placeholder: impl Into<String>, width: f32) -> Self {
        let mut line = SingleLine::new(placeholder.into(), width, 10.0);
        line.edit.secure = true;
        Self { line }
    }

    pub fn text(&self) -> &str {
        self.line.edit.editor.text()
    }

    pub fn set_text(&mut self, cx: &mut UpdateCx, text: impl Into<String>) {
        self.line.edit.editor.set_text(&text.into());
        cx.request_layout();
    }
}

impl Widget for SecureTextField {
    fn cursor(&self, local: Point, _size: Size) -> Option<Cursor> {
        self.line.cursor(local)
    }

    fn style(&self) -> Style {
        self.line.style()
    }

    fn measures(&self) -> bool {
        true
    }

    fn inherits_style(&self) -> bool {
        false
    }

    fn paints_style_box(&self) -> bool {
        true
    }

    fn measure(&mut self, cx: &mut MeasureCx, input: MeasureInput) -> Size {
        measure_field(&mut self.line, cx, input)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        paint_field_box(cx, Corner::Control, 6.0);
        self.line.edit.text_color = cx.styling().color;
        let focused = cx.is_focused();
        self.line.paint_text(cx, focused);
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        self.line.event(cx, event);
    }

    fn focusable(&self) -> bool {
        true
    }

    // No text input area: the IME stays off while a secure field is focused.

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::PasswordInput)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        self.line.edit.accessibility(node);
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.line.edit.removed(text);
    }
}

/// Sent on Return, or when a recent search is chosen from the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchSubmitted(pub String);

const SEARCH_LEFT: f32 = 32.0;
const SEARCH_RIGHT: f32 = 26.0;
const MAX_RECENTS: usize = 10;

/// A search field (`NSSearchField`) with a menu of recent searches. Emits
/// [`TextChanged`] while typing, and [`SearchSubmitted`].
#[derive(Debug)]
pub struct SearchField {
    line: SingleLine,
    recents: Vec<String>,
    capsule: bool,
}

impl SearchField {
    pub fn new(placeholder: impl Into<String>, width: f32) -> Self {
        let mut line = SingleLine::new(placeholder.into(), width, 0.0);
        line.pad_left = SEARCH_LEFT;
        line.pad_right = SEARCH_RIGHT;
        Self {
            line,
            recents: Vec::new(),
            capsule: true,
        }
    }

    /// On by default.
    pub fn capsule(mut self, capsule: bool) -> Self {
        self.capsule = capsule;
        self
    }

    pub fn text(&self) -> &str {
        self.line.edit.editor.text()
    }

    /// Newest first. Save them after each [`SearchSubmitted`] to keep them between
    /// launches.
    pub fn recents(&self) -> &[String] {
        &self.recents
    }

    /// Newest first. Empty ones and repeats are dropped; at most ten are kept.
    pub fn with_recents<S: Into<String>>(mut self, recents: impl IntoIterator<Item = S>) -> Self {
        self.set_recent_list(recents);
        self
    }

    /// Newest first.
    pub fn set_recents<S: Into<String>>(
        &mut self,
        _cx: &mut UpdateCx,
        recents: impl IntoIterator<Item = S>,
    ) {
        self.set_recent_list(recents);
    }

    fn set_recent_list<S: Into<String>>(&mut self, recents: impl IntoIterator<Item = S>) {
        self.recents.clear();
        for search in recents {
            let search = search.into();
            if !search.is_empty() && !self.recents.contains(&search) {
                self.recents.push(search);
            }
        }
        self.recents.truncate(MAX_RECENTS);
    }

    pub fn set_text(&mut self, cx: &mut UpdateCx, text: impl Into<String>) {
        self.line.edit.editor.set_text(&text.into());
        cx.request_layout();
    }

    fn icon_rect(&self, height: f32) -> Rect {
        Rect::new(6.0, (height - 18.0) / 2.0, 20.0, 18.0)
    }

    fn clear_rect(&self, height: f32) -> Rect {
        Rect::new(
            self.line.box_width() - 22.0,
            (height - 16.0) / 2.0,
            16.0,
            16.0,
        )
    }

    fn remember(&mut self, search: &str) {
        if search.is_empty() {
            return;
        }
        self.recents.retain(|r| r != search);
        self.recents.insert(0, search.to_owned());
        self.recents.truncate(MAX_RECENTS);
    }

    fn clear(&mut self, cx: &mut EventCx) {
        if self.text().is_empty() {
            return;
        }
        self.line.edit.editor.select_all();
        self.line.edit.editor.insert("");
        cx.emit(TextChanged(String::new()));
        cx.request_layout();
    }

    fn open_recents(&mut self, cx: &mut EventCx) {
        let items = if self.recents.is_empty() {
            vec![MenuItem::new("No Recent Searches").enabled(false)]
        } else {
            let mut items = vec![MenuItem::new("Recent Searches").enabled(false)];
            items.extend(self.recents.iter().map(MenuItem::new));
            items.push(MenuItem::separator());
            items.push(MenuItem::new("Clear Recent Searches"));
            items
        };
        let anchor = cx.window_rect();
        cx.open_menu(items, Placement::Below(anchor), 200.0, None);
    }
}

impl Widget for SearchField {
    fn cursor(&self, local: Point, _size: Size) -> Option<Cursor> {
        self.line.cursor(local)
    }

    fn style(&self) -> Style {
        self.line.style()
    }

    fn measures(&self) -> bool {
        true
    }

    fn inherits_style(&self) -> bool {
        false
    }

    fn paints_style_box(&self) -> bool {
        true
    }

    fn measure(&mut self, cx: &mut MeasureCx, input: MeasureInput) -> Size {
        measure_field(&mut self.line, cx, input)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let height = cx.size().height;
        let radius = if self.capsule { height / 2.0 } else { 6.0 };
        paint_field_box(cx, Corner::Control, radius);
        let muted = cx.color(Role::Muted);
        let icon = self.icon_rect(height);
        let lens = Rect::new(icon.x + 2.0, icon.y + 2.0, 10.0, 10.0);
        cx.rounded_rect(
            lens,
            5.0,
            noa_gpui_scene::Color::TRANSPARENT,
            Border {
                width: 1.5,
                color: muted,
            },
        );
        cx.line(
            Point::new(lens.right() - 1.5, lens.bottom() - 1.5),
            Point::new(lens.right() + 2.5, lens.bottom() + 2.5),
            1.5,
            muted,
        );
        cx.chevron(
            Rect::new(icon.right() - 4.0, icon.y + 12.0, 5.0, 2.5),
            Direction::Down,
            1.0,
            muted,
        );
        if !self.text().is_empty() {
            let clear = self.clear_rect(height);
            cx.rounded_rect(clear, 8.0, muted, Border::default());
            let surface = cx.color(Role::Surface);
            let (c, r) = (Point::new(clear.x + 8.0, clear.y + 8.0), 3.0);
            cx.line(
                Point::new(c.x - r, c.y - r),
                Point::new(c.x + r, c.y + r),
                1.5,
                surface,
            );
            cx.line(
                Point::new(c.x + r, c.y - r),
                Point::new(c.x - r, c.y + r),
                1.5,
                surface,
            );
        }
        self.line.edit.text_color = cx.styling().color;
        let focused = cx.is_focused();
        self.line.paint_text(cx, focused);
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        let composing = !self.line.edit.composition().is_empty();
        match event {
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } if !composing => {
                let p = cx.local(*pos);
                let height = cx.size().height;
                if self.icon_rect(height).contains(p) {
                    self.open_recents(cx);
                    cx.set_handled();
                    return;
                }
                if !self.text().is_empty() && self.clear_rect(height).contains(p) {
                    self.clear(cx);
                    cx.set_handled();
                    return;
                }
            }
            Event::KeyDown {
                key: Key::Enter, ..
            } if !composing => {
                let search = self.text().to_owned();
                self.remember(&search);
                cx.emit(SearchSubmitted(search));
                cx.set_handled();
                return;
            }
            Event::KeyDown {
                key: Key::Escape, ..
            } if !composing && !self.text().is_empty() => {
                self.clear(cx);
                cx.set_handled();
                return;
            }
            Event::MenuClosed { chosen: Some(i) } => {
                let n = self.recents.len();
                if (1..=n).contains(i) {
                    let search = self.recents[i - 1].clone();
                    self.line.edit.editor.set_text(&search);
                    self.remember(&search);
                    cx.emit(TextChanged(search.clone()));
                    cx.emit(SearchSubmitted(search));
                } else if n > 0 && *i == n + 2 {
                    self.recents.clear();
                }
                cx.request_layout();
                return;
            }
            _ => {}
        }
        self.line.event(cx, event);
    }

    fn focusable(&self) -> bool {
        true
    }

    fn text_input_area(&self) -> Option<Rect> {
        Some(self.line.input_area())
    }

    fn ime_context(&self, text: &TextStore) -> Option<crate::ImeContext> {
        self.line.ime_context(text)
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::SearchInput)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        self.line.edit.accessibility(node);
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.line.edit.removed(text);
    }
}

const ARROW_W: f32 = 22.0;

/// A text field with a list of suggestions (`NSComboBox`). Emits [`TextChanged`],
/// [`SelectionChanged`] and [`super::TextSubmitted`].
#[derive(Debug)]
pub struct ComboBox {
    line: SingleLine,
    items: Vec<String>,
    completes: bool,
    menu_open: bool,
    completer: Completer,
    multiple: bool,
}

impl ComboBox {
    pub fn new<S: Into<String>>(items: impl IntoIterator<Item = S>, width: f32) -> Self {
        let mut line = SingleLine::new(String::new(), width, 10.0);
        line.pad_right = ARROW_W + 6.0;
        Self {
            line,
            items: items.into_iter().map(Into::into).collect(),
            completes: true,
            menu_open: false,
            completer: Completer::new(),
            multiple: false,
        }
    }

    pub fn completion_list(mut self, enabled: bool) -> Self {
        self.completer.set_items(if enabled {
            self.items.clone()
        } else {
            Vec::new()
        });
        self
    }

    pub fn completion_source(mut self, source: impl Fn(&str) -> Vec<String> + 'static) -> Self {
        self.completer.set_source(std::rc::Rc::new(source));
        self
    }

    /// The values are comma-separated; the last one is completed.
    pub fn multiple(mut self, multiple: bool) -> Self {
        self.multiple = multiple;
        self.completer.multiple = multiple;
        self
    }

    pub fn completions_open(&self) -> bool {
        self.completer.is_open()
    }

    pub fn completions_shown(&self) -> Vec<String> {
        self.completer.shown()
    }

    pub fn values(&self) -> Vec<String> {
        self.text()
            .split(',')
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
            .collect()
    }

    /// Where the text being completed starts: after the last comma when multiple.
    fn completing_from(&self) -> usize {
        if self.multiple {
            self.text().rfind(',').map_or(0, |i| i + 1)
        } else {
            0
        }
    }

    fn update_completions(&mut self, cx: &mut EventCx) {
        let from = self.completing_from();
        let query = self.text()[from..].trim_start().to_owned();
        let exclude = if self.multiple {
            let mut values = self.values();
            if !query.is_empty() {
                values.pop();
            }
            values
        } else {
            Vec::new()
        };
        let anchor = cx.window_rect();
        self.completer.update(cx, &query, &exclude, anchor);
    }

    fn accept(&mut self, cx: &mut EventCx, items: Vec<String>) {
        self.completer.reset(cx);
        let Some(first) = items.first().cloned() else {
            return;
        };
        let text = if self.multiple {
            let kept = self.text()[..self.completing_from()].trim_end().to_owned();
            let mut values: Vec<String> = if kept.is_empty() {
                Vec::new()
            } else {
                vec![kept.trim_end_matches(',').to_owned()]
            };
            values.extend(items);
            format!("{}, ", values.join(", "))
        } else {
            first.clone()
        };
        self.line.edit.editor.select_all();
        self.line.edit.editor.insert(&text);
        cx.emit(TextChanged(text));
        if !self.multiple
            && let Some(index) = self.items.iter().position(|i| *i == first)
        {
            cx.emit(SelectionChanged(index));
        }
        cx.request_layout();
    }

    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.line.edit.placeholder = placeholder.into();
        self
    }

    /// On by default.
    pub fn completes(mut self, completes: bool) -> Self {
        self.completes = completes;
        self
    }

    pub fn text(&self) -> &str {
        self.line.edit.editor.text()
    }

    pub fn selection(&self) -> std::ops::Range<usize> {
        self.line.edit.editor.selection()
    }

    pub fn items(&self) -> &[String] {
        &self.items
    }

    pub fn set_text(&mut self, cx: &mut UpdateCx, text: impl Into<String>) {
        self.line.edit.editor.set_text(&text.into());
        cx.request_layout();
    }

    fn arrow_rect(&self, height: f32) -> Rect {
        Rect::new(
            self.line.box_width() - ARROW_W - 2.0,
            2.0,
            ARROW_W,
            height - 4.0,
        )
    }

    fn open_list(&mut self, cx: &mut EventCx) {
        if self.items.is_empty() || self.menu_open {
            return;
        }
        self.menu_open = true;
        let items = self.items.iter().map(MenuItem::new).collect();
        let checked = self.items.iter().position(|i| i == self.text());
        let anchor = cx.window_rect();
        cx.open_menu(items, Placement::Below(anchor), anchor.width, checked);
        cx.request_paint();
    }

    fn complete(&mut self) {
        let editor = &mut self.line.edit.editor;
        let typed = editor.text().to_owned();
        if typed.is_empty() || editor.caret() != typed.len() || editor.has_selection() {
            return;
        }
        let lower = typed.to_lowercase();
        let Some(item) = self
            .items
            .iter()
            .find(|i| i.len() > typed.len() && i.to_lowercase().starts_with(&lower))
        else {
            return;
        };
        let Some(rest) = item.get(typed.len()..) else {
            return;
        };
        editor.insert(rest);
        editor.select(typed.len()..typed.len() + rest.len());
    }
}

impl Widget for ComboBox {
    fn cursor(&self, local: Point, _size: Size) -> Option<Cursor> {
        self.line.cursor(local)
    }

    fn style(&self) -> Style {
        self.line.style()
    }

    fn measures(&self) -> bool {
        true
    }

    fn inherits_style(&self) -> bool {
        false
    }

    fn paints_style_box(&self) -> bool {
        true
    }

    fn measure(&mut self, cx: &mut MeasureCx, input: MeasureInput) -> Size {
        measure_field(&mut self.line, cx, input)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        paint_field_box(cx, Corner::Control, 6.0);
        let height = cx.size().height;
        let arrow = self.arrow_rect(height);
        let fill = cx.color(if self.menu_open {
            Role::AccentPressed
        } else {
            Role::Accent
        });
        let radius = cx.corner(Corner::Control, 5.0, arrow.height);
        cx.bezel(arrow, radius, fill, Border::default());
        let ink = cx.color(Role::OnAccent);
        cx.chevron(
            Rect::new(arrow.x + 7.5, arrow.y + arrow.height / 2.0 - 2.0, 7.0, 3.5),
            Direction::Down,
            1.5,
            ink,
        );
        self.line.edit.text_color = cx.styling().color;
        let focused = cx.is_focused();
        self.line.paint_text(cx, focused);
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        let composing = !self.line.edit.composition().is_empty();
        if self.completer.is_enabled() {
            if let Event::FocusChanged(false) = event {
                self.completer.close(cx);
                self.line.event(cx, event);
                return;
            }
            if let Some(done) = self.completer.event(cx, event) {
                if let Completed::Accept(items) = done {
                    self.accept(cx, items);
                }
                return;
            }
            if let Event::KeyDown { key, modifiers } = event
                && !composing
                && let Some(done) = self.completer.key(cx, *key, *modifiers)
            {
                if let Completed::Accept(items) = done {
                    self.accept(cx, items);
                }
                cx.set_handled();
                return;
            }
            if matches!(
                event,
                Event::Text(_) | Event::Composition { .. } | Event::KeyDown { .. }
            ) && !matches!(
                event,
                Event::KeyDown {
                    key: Key::ArrowDown,
                    ..
                }
            ) {
                self.line.event(cx, event);
                self.update_completions(cx);
                return;
            }
        }
        match event {
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } if !composing && self.arrow_rect(cx.size().height).contains(cx.local(*pos)) => {
                self.completer.close(cx);
                self.open_list(cx);
                cx.set_handled();
                return;
            }
            Event::KeyDown {
                key: Key::ArrowDown,
                ..
            } if !composing => {
                self.open_list(cx);
                cx.set_handled();
                return;
            }
            Event::MenuClosed { chosen } => {
                self.menu_open = false;
                if let Some(item) = chosen.and_then(|i| self.items.get(i)) {
                    let item = item.clone();
                    self.line.edit.editor.set_text(&item);
                    cx.emit(TextChanged(item));
                    cx.emit(SelectionChanged(chosen.unwrap_or_default()));
                }
                cx.request_layout();
                return;
            }
            Event::Text(_) if self.completes => {
                self.line.event(cx, event);
                let before = self.text().len();
                self.complete();
                if self.text().len() != before {
                    cx.emit(TextChanged(self.text().to_owned()));
                }
                return;
            }
            _ => {}
        }
        self.line.event(cx, event);
    }

    fn focusable(&self) -> bool {
        true
    }

    fn text_input_area(&self) -> Option<Rect> {
        Some(self.line.input_area())
    }

    fn ime_context(&self, text: &TextStore) -> Option<crate::ImeContext> {
        self.line.ime_context(text)
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::ComboBox)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        self.line.edit.accessibility(node);
        node.set_has_popup(accesskit::HasPopup::Listbox);
        node.set_expanded(self.menu_open || self.completer.is_open());
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.line.edit.removed(text);
    }
}

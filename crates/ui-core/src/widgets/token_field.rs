use noa_gpui_layout::style::{Dimension, Size as LayoutSize, length};
use noa_gpui_layout::{MeasureInput, Style};
use noa_gpui_scene::{Border, Point, Rect, Size};
use noa_gpui_text::{TextStore, TextStyle};

use super::CachedText;
use super::completion::{Completed, Completer};
use super::editable::Editable;
use super::text_field::paint_field_box;
use crate::event::{Cursor, Event, Key, PointerButton};
use crate::text_edit::TextEditor;
use crate::theme::{Corner, Role};
use crate::widget::{EventCx, MeasureCx, PaintCx, UpdateCx, Widget};

/// Emitted by a [`TokenField`] with all tokens whenever they change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokensChanged(pub Vec<String>);

const PAD: f32 = 6.0;
const GAP: f32 = 4.0;
const TOKEN_PAD: f32 = 8.0;
const MIN_INPUT: f32 = 40.0;

/// Text gathered into tokens, such as tags or recipients (`NSTokenField`). Return or a
/// comma turns the typed text into a token. Completions leave out tokens already in
/// the field. Emits [`TokensChanged`].
#[derive(Debug)]
pub struct TokenField {
    tokens: Vec<String>,
    shaped: Vec<CachedText>,
    edit: Editable,
    placeholder: String,
    width: f32,
    selected: Option<usize>,
    rects: Vec<Rect>,
    input_origin: Point,
    row_height: f32,
    completer: Completer,
}

impl TokenField {
    pub fn new(placeholder: impl Into<String>, width: f32) -> Self {
        let placeholder = placeholder.into();
        Self {
            tokens: Vec::new(),
            shaped: Vec::new(),
            edit: Editable::new(
                TextEditor::single_line(),
                placeholder.clone(),
                TextStyle::new(14.0),
            ),
            placeholder,
            width,
            selected: None,
            rects: Vec::new(),
            input_origin: Point::default(),
            row_height: 0.0,
            completer: Completer::new(),
        }
    }

    pub fn completions<S: Into<String>>(mut self, items: impl IntoIterator<Item = S>) -> Self {
        self.completer
            .set_items(items.into_iter().map(Into::into).collect());
        self
    }

    /// Complete with what `source` returns for the text typed (called on every edit).
    pub fn completion_source(mut self, source: impl Fn(&str) -> Vec<String> + 'static) -> Self {
        self.completer.set_source(std::rc::Rc::new(source));
        self
    }

    pub fn multiple_completions(mut self, multiple: bool) -> Self {
        self.completer.multiple = multiple;
        self
    }

    pub fn completions_open(&self) -> bool {
        self.completer.is_open()
    }

    pub fn completions_shown(&self) -> Vec<String> {
        self.completer.shown()
    }

    /// The highlighted completion (an index into [`Self::completions_shown`]).
    pub fn highlighted_completion(&self) -> Option<usize> {
        self.completer.highlighted()
    }

    /// Add completions as tokens and empty the input.
    fn accept(&mut self, cx: &mut EventCx, items: Vec<String>) {
        self.completer.reset(cx);
        let mut added = false;
        for item in items {
            if !self.tokens.contains(&item) {
                self.tokens.push(item);
                added = true;
            }
        }
        self.edit.editor.set_text("");
        self.selected = None;
        self.sync_caches();
        if added {
            cx.emit(TokensChanged(self.tokens.clone()));
        }
        cx.request_layout();
    }

    pub fn with_tokens<S: Into<String>>(mut self, tokens: impl IntoIterator<Item = S>) -> Self {
        self.tokens = tokens.into_iter().map(Into::into).collect();
        self.sync_caches();
        self
    }

    pub fn tokens(&self) -> &[String] {
        &self.tokens
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn input(&self) -> &str {
        self.edit.editor.text()
    }

    pub fn composition(&self) -> &str {
        self.edit.composition()
    }

    pub fn set_tokens<S: Into<String>>(
        &mut self,
        cx: &mut UpdateCx,
        tokens: impl IntoIterator<Item = S>,
    ) {
        self.tokens = tokens.into_iter().map(Into::into).collect();
        self.selected = None;
        self.sync_caches();
        cx.request_layout();
    }

    /// One text cache per token; surplus caches keep their layout until reshaped or
    /// removed.
    fn sync_caches(&mut self) {
        while self.shaped.len() < self.tokens.len() {
            self.shaped.push(CachedText::default());
        }
    }

    /// Turn the input's text into tokens (split at commas). Returns whether any were
    /// added.
    fn tokenize(&mut self, cx: &mut EventCx) -> bool {
        let text = self.edit.editor.text().to_owned();
        let new: Vec<String> = text
            .split(',')
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
            .collect();
        self.edit.editor.set_text("");
        if new.is_empty() {
            return false;
        }
        self.tokens.extend(new);
        self.sync_caches();
        cx.emit(TokensChanged(self.tokens.clone()));
        true
    }

    fn remove_selected(&mut self, cx: &mut EventCx) {
        let Some(i) = self.selected.take() else {
            return;
        };
        self.tokens.remove(i);
        // Keep each cache with its token.
        let cache = self.shaped.remove(i);
        self.shaped.push(cache);
        // Select the token before, as macOS does, until there is none.
        self.selected = i.checked_sub(1).filter(|_| !self.tokens.is_empty());
        cx.emit(TokensChanged(self.tokens.clone()));
    }

    fn token_at(&self, p: Point) -> Option<usize> {
        self.rects.iter().position(|r| r.contains(p))
    }

    fn input_empty_at_start(&self) -> bool {
        self.edit.editor.caret() == 0 && !self.edit.editor.has_selection()
    }

    fn token_event(&mut self, cx: &mut EventCx, key: Key) -> bool {
        let Some(i) = self.selected else {
            return false;
        };
        match key {
            Key::ArrowLeft => self.selected = Some(i.saturating_sub(1)),
            Key::ArrowRight if i + 1 < self.tokens.len() => self.selected = Some(i + 1),
            Key::ArrowRight | Key::Escape => self.selected = None,
            Key::Backspace | Key::Delete => self.remove_selected(cx),
            _ => return false,
        }
        true
    }
    fn token_input(&mut self, cx: &mut EventCx, event: &Event, composing: bool) {
        match event {
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } if !composing => {
                if let Some(i) = self.token_at(cx.local(*pos)) {
                    self.selected = Some(i);
                    cx.request_paint();
                    cx.set_handled();
                    return;
                }
                self.selected = None;
            }
            Event::KeyDown { key, .. } if !composing && self.selected.is_some() => {
                if self.token_event(cx, *key) {
                    cx.request_layout();
                    cx.set_handled();
                    return;
                }
            }
            Event::KeyDown {
                key: Key::Enter, ..
            } if !composing && !self.input().trim().is_empty() => {
                self.tokenize(cx);
                cx.request_layout();
                cx.set_handled();
                return;
            }
            Event::KeyDown {
                key: Key::Backspace | Key::ArrowLeft,
                modifiers,
            } if !composing
                && !modifiers.shift
                && self.input_empty_at_start()
                && !self.tokens.is_empty() =>
            {
                self.selected = Some(self.tokens.len() - 1);
                cx.request_paint();
                cx.set_handled();
                return;
            }
            Event::Text(text) if text.contains(',') => {
                self.selected = None;
                self.edit.event(cx, event, self.input_origin);
                self.tokenize(cx);
                cx.request_layout();
                return;
            }
            Event::Text(_) | Event::Composition { .. } => self.selected = None,
            _ => {}
        }
        self.edit.event(cx, event, self.input_origin);
    }
}

impl Widget for TokenField {
    fn paints_style_box(&self) -> bool {
        true
    }

    fn cursor(&self, _local: Point, _size: Size) -> Option<Cursor> {
        Some(Cursor::Text)
    }

    fn style(&self) -> Style {
        Style {
            size: LayoutSize {
                width: length(self.width),
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
        let style = TextStyle::new(13.0);
        let line = self.edit.style.line_height;
        self.row_height = line + 6.0;
        let right = self.width - PAD;
        let (mut x, mut y) = (PAD, PAD);
        self.rects.clear();
        for (i, token) in self.tokens.iter().enumerate() {
            let w = self.shaped[i].shape(cx.text, token, style, None).width + 2.0 * TOKEN_PAD;
            if x > PAD && x + w > right {
                x = PAD;
                y += self.row_height + GAP;
            }
            self.rects.push(Rect::new(x, y, w, self.row_height));
            x += w + GAP;
        }
        self.edit.placeholder = if self.tokens.is_empty() {
            self.placeholder.clone()
        } else {
            String::new()
        };
        let input = self.edit.shape(cx.text, None);
        if x > PAD && x + input.width.max(MIN_INPUT) > right {
            x = PAD;
            y += self.row_height + GAP;
        }
        self.input_origin = Point::new(x + 2.0, y + (self.row_height - line) / 2.0);
        Size::new(self.width, y + self.row_height + PAD)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        paint_field_box(cx, Corner::Inner, 6.0);
        let focused = cx.is_focused();
        let accent = cx.color(Role::Accent);
        let text = cx.color(Role::Text);
        let on_accent = cx.color(Role::OnAccent);
        for (i, rect) in self.rects.iter().enumerate() {
            let selected = focused && self.selected == Some(i);
            let fill = if selected {
                accent
            } else {
                accent.with_alpha(accent.a * 0.18)
            };
            cx.rounded_rect(*rect, rect.height / 2.0, fill, Border::default());
            if let Some(layout) = self.shaped[i].layout() {
                let size = self.shaped[i].size();
                let ink = if selected { on_accent } else { text };
                let y = rect.y + (rect.height - size.height) / 2.0;
                cx.text(
                    Rect::new(rect.x + TOKEN_PAD, y, size.width, size.height),
                    layout,
                    ink,
                );
            }
        }
        let size = cx.size();
        cx.push_clip(Rect::new(0.0, 0.0, size.width, size.height));
        self.edit
            .paint(cx, self.input_origin, focused && self.selected.is_none());
        cx.pop_clip();
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        let composing = !self.edit.composition().is_empty();
        if let Event::FocusChanged(false) = event {
            self.completer.close(cx);
            self.edit.event(cx, event, self.input_origin);
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
            && self.selected.is_none()
            && let Some(done) = self.completer.key(cx, *key, *modifiers)
        {
            if let Completed::Accept(items) = done {
                self.accept(cx, items);
            }
            cx.set_handled();
            return;
        }
        self.token_input(cx, event, composing);
        if self.completer.is_enabled()
            && matches!(
                event,
                Event::Text(_) | Event::Composition { .. } | Event::KeyDown { .. }
            )
        {
            let input = self.edit.editor.text().to_owned();
            let anchor = cx.window_rect();
            self.completer.update(cx, &input, &self.tokens, anchor);
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn text_input_area(&self) -> Option<Rect> {
        Some(self.edit.input_area(self.input_origin))
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::TextInput)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        self.edit.accessibility(node);
        node.set_description(self.tokens.join(", "));
        if self.completer.is_enabled() {
            node.set_has_popup(accesskit::HasPopup::Listbox);
            node.set_expanded(self.completer.is_open());
        }
    }

    fn accessibility_items(&self) -> Vec<accesskit::Node> {
        self.tokens
            .iter()
            .zip(&self.rects)
            .enumerate()
            .map(|(i, (token, r))| {
                let mut node = accesskit::Node::new(accesskit::Role::ListItem);
                node.set_label(token.clone());
                node.set_bounds(accesskit::Rect {
                    x0: f64::from(r.x),
                    y0: f64::from(r.y),
                    x1: f64::from(r.right()),
                    y1: f64::from(r.bottom()),
                });
                if self.selected == Some(i) {
                    node.set_selected(true);
                }
                node
            })
            .collect()
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.edit.removed(text);
        for cached in &mut self.shaped {
            cached.release(text);
        }
    }
}

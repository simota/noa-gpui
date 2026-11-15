use noa_gpui_layout::Style;
use noa_gpui_layout::style::{Dimension, Rect as LayoutRect, Size as LayoutSize, length};
use noa_gpui_scene::{Point, Rect, Size};
use noa_gpui_text::{CaretPosition, TextStore, TextStyle};
use unicode_segmentation::UnicodeSegmentation;

use super::CachedText;
use crate::event::{Cursor, Event, Key, PointerButton};
use crate::styling::StylePatch;
use crate::text_edit::TextEditor;
use crate::theme::Role;
use crate::widget::{EventCx, PaintCx};

/// Emitted by a text input after its committed text changes (not during composition).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextChanged(pub String);

/// Emitted by a single-line text input when Return is pressed, with its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSubmitted(pub String);

/// Emitted by a [`super::TextField`] that captures readings
/// ([`super::TextField::capture_reading`]) when its reading changes: the kana typed
/// into the IME before conversion, for every committed piece of the text, as typed
/// (usually hiragana). [`crate::kana::to_katakana`] gives the フリガナ form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadingChanged(pub String);

/// The reading of the committed text, collected from IME compositions.
#[derive(Debug, Default)]
pub(crate) struct Reading {
    pub text: String,
    /// The last composition that was only kana: the reading of the next commit.
    pending: Option<String>,
}

impl Reading {
    fn composed(&mut self, composition: &str) {
        if crate::kana::is_kana(composition) {
            self.pending = Some(composition.to_owned());
        }
    }

    /// The reading of `committed`: the kana composed before it was converted, or the
    /// text itself when it was typed as kana. ASCII typed directly has no reading.
    fn committed(&mut self, committed: &str) -> Option<String> {
        match self.pending.take() {
            Some(kana) if !committed.is_ascii() => Some(kana),
            _ => crate::kana::is_kana(committed).then(|| committed.to_owned()),
        }
    }
}

const BULLET: &str = "\u{2022}";

/// Seconds within which a second press counts as a double click.
const MULTI_CLICK: f64 = 0.4;

#[derive(Debug)]
pub(crate) struct Editable {
    pub editor: TextEditor,
    pub placeholder: String,
    pub style: TextStyle,
    pub shaped: CachedText,
    /// Selection, navigation and copying only; no edits, no caret.
    pub read_only: bool,
    /// Show a bullet per character, refuse copy and cut, ignore IME composition.
    pub secure: bool,
    preedit: String,
    preedit_cursor: Option<(usize, usize)>,
    /// Geometry of the last [`Editable::shape`], in text layout coordinates.
    caret: CaretPosition,
    selection: Vec<Rect>,
    preedit_spans: Vec<Rect>,
    clause_spans: Vec<Rect>,
    goal_x: Option<f32>,
    /// Time, byte index and count of the last press, for double / triple clicks.
    last_press: Option<(f64, usize, u8)>,
    dragging: bool,
    /// Ranges found by a search (see [`super::FindBar`]) and the index of the current
    /// one, highlighted in the text.
    pub found: Vec<std::ops::Range<usize>>,
    pub found_current: Option<usize>,
    found_spans: Vec<Rect>,
    current_spans: Vec<Rect>,
    base_style: TextStyle,
    /// The stylesheet's `color` for the typed text (the placeholder stays muted).
    pub text_color: Option<crate::theme::Paint>,
    /// `Some` when the reading of the text is captured from the IME.
    pub reading: Option<Reading>,
}

impl Editable {
    pub fn new(editor: TextEditor, placeholder: String, style: TextStyle) -> Self {
        Self {
            editor,
            placeholder,
            style,
            shaped: CachedText::default(),
            read_only: false,
            secure: false,
            preedit: String::new(),
            preedit_cursor: None,
            caret: CaretPosition {
                x: 0.0,
                top: 0.0,
                height: style.line_height,
            },
            selection: Vec::new(),
            preedit_spans: Vec::new(),
            clause_spans: Vec::new(),
            goal_x: None,
            last_press: None,
            dragging: false,
            found: Vec::new(),
            found_current: None,
            found_spans: Vec::new(),
            current_spans: Vec::new(),
            base_style: style,
            text_color: None,
            reading: None,
        }
    }

    /// Applies the stylesheet's `font-size` and `font-weight`. Call while measuring; a
    /// style change releases the shaped text, so the next [`Editable::shape`] uses it.
    pub fn restyle(&mut self, store: &mut TextStore, patch: &StylePatch) {
        let mut style = self.base_style;
        if let Some(size) = patch.font_size {
            style.font_size = size;
            style.line_height = TextStyle::new(size).line_height;
        }
        if let Some(weight) = patch.font_weight {
            style = style.weight(weight);
        }
        if style != self.style {
            self.style = style;
            self.shaped.release(store);
        }
    }

    pub fn composition(&self) -> &str {
        &self.preedit
    }

    pub fn showing_placeholder(&self) -> bool {
        self.editor.text().is_empty() && self.preedit.is_empty()
    }

    /// Caret geometry in text layout coordinates, from the last [`Editable::shape`].
    pub fn caret(&self) -> CaretPosition {
        self.caret
    }

    /// What is laid out: composition replaces the selection visually, without
    /// changing the committed text until the IME commits it.
    fn display(&self) -> String {
        if self.showing_placeholder() {
            return self.placeholder.clone();
        }
        if self.secure {
            return BULLET.repeat(self.editor.text().graphemes(true).count());
        }
        let text = self.editor.text();
        let range = if self.preedit.is_empty() {
            self.editor.caret()..self.editor.caret()
        } else {
            self.editor.selection()
        };
        let mut shown = String::with_capacity(text.len() + self.preedit.len());
        shown.push_str(&text[..range.start]);
        shown.push_str(&self.preedit);
        shown.push_str(&text[range.end..]);
        shown
    }

    /// A byte index into the content as an index into what is shown (they differ only
    /// for secure text).
    fn shown_index(&self, index: usize) -> usize {
        if self.secure {
            self.editor.text()[..index].graphemes(true).count() * BULLET.len()
        } else {
            index
        }
    }

    fn content_index(&self, index: usize) -> usize {
        if !self.secure {
            return index;
        }
        let n = index / BULLET.len();
        self.editor
            .text()
            .grapheme_indices(true)
            .nth(n)
            .map_or(self.editor.text().len(), |(i, _)| i)
    }

    /// Returns the text size.
    pub fn shape(&mut self, store: &mut TextStore, wrap: Option<f32>) -> Size {
        let shown = self.display();
        let size = self.shaped.shape(store, &shown, self.style, wrap);
        let layout = self.shaped.layout().expect("shaped above");
        self.selection.clear();
        self.preedit_spans.clear();
        self.clause_spans.clear();
        self.found_spans.clear();
        self.current_spans.clear();
        if self.showing_placeholder() {
            self.caret = CaretPosition {
                x: 0.0,
                top: 0.0,
                height: self.style.line_height,
            };
            return size;
        }
        let caret = self.editor.caret();
        if self.preedit.is_empty() && !self.secure {
            let len = self.editor.text().len();
            for (i, range) in self.found.iter().enumerate() {
                if range.end > len {
                    continue;
                }
                let rects = store.selection_rects(layout, range.start, range.end);
                if Some(i) == self.found_current {
                    self.current_spans.extend(rects);
                } else {
                    self.found_spans.extend(rects);
                }
            }
        }
        if self.preedit.is_empty() {
            self.caret = store.caret(layout, self.shown_index(caret));
            if self.editor.has_selection() {
                let range = self.editor.selection();
                let (start, end) = (self.shown_index(range.start), self.shown_index(range.end));
                self.selection = store.selection_rects(layout, start, end);
            }
        } else {
            // The IME reports the clause being converted as a range; the caret sits at
            // its end, as in native text fields. Without a range it follows the
            // composition.
            let caret = self.editor.selection().start;
            let (start, end) = self
                .preedit_cursor
                .unwrap_or((self.preedit.len(), self.preedit.len()));
            self.caret = store.caret(layout, caret + end);
            self.preedit_spans = store.selection_rects(layout, caret, caret + self.preedit.len());
            if start < end {
                self.clause_spans = store.selection_rects(layout, caret + start, caret + end);
            }
        }
        size
    }

    /// `origin` is the text layout's origin in widget coordinates. Emits [`TextChanged`]
    /// when the content changed, and [`ReadingChanged`] when the captured reading did.
    pub fn event(&mut self, cx: &mut EventCx, event: &Event, origin: Point) {
        let reading_before = self.reading.as_ref().map(|r| r.text.len());
        let changed = match event {
            Event::FocusChanged(false) => {
                if let Some(reading) = &mut self.reading {
                    reading.pending = None;
                }
                if !self.preedit.is_empty() {
                    self.preedit.clear();
                    cx.request_layout();
                }
                self.preedit_cursor = None;
                self.goal_x = None;
                self.last_press = None;
                if self.dragging {
                    self.dragging = false;
                    cx.release_pointer();
                }
                return;
            }
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } if self.preedit.is_empty() => {
                let index = self.hit(cx, *pos, origin);
                let count = match self.last_press {
                    Some((time, at, count)) if cx.now() - time < MULTI_CLICK && at == index => {
                        count % 3 + 1
                    }
                    _ => 1,
                };
                self.last_press = Some((cx.now(), index, count));
                match count {
                    // A secure field has no visible words: select everything.
                    2 if self.secure => self.editor.select_all(),
                    2 => self.editor.select_word_at(index),
                    3 => {
                        let start = self.editor.line_start(index);
                        let end = self.editor.line_end(index);
                        self.editor.select(start..end);
                    }
                    _ => {
                        let extend = cx.is_focused() && cx.modifiers().shift;
                        self.editor.move_to(index, extend);
                    }
                }
                self.goal_x = None;
                self.dragging = true;
                cx.capture_pointer();
                cx.set_handled();
                false
            }
            Event::PointerMove { pos } if self.dragging => {
                let index = self.hit(cx, *pos, origin);
                self.editor.move_to(index, true);
                false
            }
            Event::PointerUp {
                button: PointerButton::Primary,
                ..
            } if self.dragging => {
                self.dragging = false;
                cx.release_pointer();
                cx.set_handled();
                return;
            }
            Event::Text(_) | Event::Composition { .. } if self.read_only => return,
            Event::Composition { .. } if self.secure => return,
            Event::Text(input) => {
                self.preedit.clear();
                self.preedit_cursor = None;
                self.goal_x = None;
                let piece = self.reading.as_mut().and_then(|r| r.committed(input));
                let changed = self.editor.insert(input);
                if let (true, Some(piece), Some(reading)) = (changed, piece, &mut self.reading) {
                    reading.text.push_str(&piece);
                }
                cx.set_handled();
                changed
            }
            Event::Composition { text, cursor } => {
                if let Some(reading) = &mut self.reading {
                    reading.composed(text);
                }
                self.preedit = text.clone();
                self.preedit_cursor = cursor.filter(|(start, end)| {
                    *start <= *end && text.is_char_boundary(*start) && text.is_char_boundary(*end)
                });
                cx.request_layout();
                cx.set_handled();
                return;
            }
            // Keys belong to the IME while composing.
            Event::KeyDown { .. } if !self.preedit.is_empty() => {
                cx.set_handled();
                return;
            }
            Event::KeyDown { key, modifiers } if self.refuses(*key, *modifiers) => {
                // Swallow Cmd+C / Cmd+X in a secure field so nothing else copies.
                if self.secure && modifiers.command {
                    cx.set_handled();
                }
                return;
            }
            Event::KeyDown {
                key: key @ (Key::ArrowLeft | Key::ArrowRight),
                modifiers,
            } if self.secure && (modifiers.alt || (modifiers.command && !modifiers.meta)) => {
                // Word movement would reveal where the hidden words break: go to the end.
                let target = if *key == Key::ArrowLeft {
                    0
                } else {
                    self.editor.text().len()
                };
                self.editor.move_to(target, modifiers.shift);
                cx.set_handled();
                false
            }
            Event::KeyDown { key, modifiers } => {
                if self.editor.is_multiline()
                    && matches!(key, Key::ArrowUp | Key::ArrowDown | Key::Home | Key::End)
                    && !(modifiers.command && modifiers.meta)
                {
                    self.move_vertically(cx, *key, modifiers.shift);
                    cx.set_handled();
                    cx.request_layout();
                    return;
                }
                let outcome = self.editor.handle_key(*key, *modifiers, cx.clipboard());
                if !outcome.handled {
                    return;
                }
                if let Some(reading) = &mut self.reading {
                    reading.pending = None;
                }
                self.goal_x = None;
                cx.set_handled();
                outcome.changed
            }
            _ => return,
        };
        cx.request_layout();
        if changed {
            cx.emit(TextChanged(self.editor.text().to_owned()));
            if let Some(reading) = &mut self.reading {
                if self.editor.text().is_empty() {
                    reading.text.clear();
                }
                if reading_before != Some(reading.text.len()) {
                    cx.emit(ReadingChanged(reading.text.clone()));
                }
            }
        }
    }

    fn refuses(&self, key: Key, modifiers: crate::event::Modifiers) -> bool {
        let command = |c: char| modifiers.command && key == Key::Char(c);
        if self.secure && (command('c') || command('x')) {
            return true;
        }
        if !self.read_only {
            return false;
        }
        let navigation = matches!(
            key,
            Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown | Key::Home | Key::End
        );
        !(navigation || command('a') || command('c'))
    }

    /// Moves through visual lines. Needs the shaped layout.
    fn move_vertically(&mut self, cx: &mut EventCx, key: Key, extend: bool) {
        let Some(layout) = self.shaped.layout() else {
            return;
        };
        let store = cx.text_store();
        let caret = store.caret(layout, self.editor.caret());
        let middle = caret.top + caret.height / 2.0;
        let target = match key {
            Key::ArrowUp if caret.top <= 0.0 => 0,
            Key::ArrowUp | Key::ArrowDown => {
                let x = *self.goal_x.get_or_insert(caret.x);
                let y = if key == Key::ArrowUp {
                    middle - caret.height
                } else {
                    middle + caret.height
                };
                let size = store.size(layout);
                if y > size.height {
                    self.editor.text().len()
                } else {
                    store.hit(layout, x, y)
                }
            }
            Key::Home => store.hit(layout, 0.0, middle),
            _ => store.hit(layout, f32::MAX / 4.0, middle),
        };
        if matches!(key, Key::Home | Key::End) {
            self.goal_x = None;
        }
        self.editor.move_to(target, extend);
    }

    fn hit(&self, cx: &mut EventCx, pos: Point, origin: Point) -> usize {
        let Some(layout) = self.shaped.layout() else {
            return 0;
        };
        if self.showing_placeholder() {
            return 0;
        }
        let local = cx.local(pos);
        let shown = cx
            .text_store()
            .hit(layout, local.x - origin.x, local.y - origin.y);
        self.content_index(shown)
    }

    /// Draws the caret only when `focused`. `origin` is the text layout's origin.
    pub fn paint(&self, cx: &mut PaintCx, origin: Point, focused: bool) {
        let color = cx.color(if self.showing_placeholder() {
            Role::Muted.into()
        } else {
            self.text_color.unwrap_or(Role::Text.into())
        });
        self.paint_colored(cx, origin, focused, color);
    }

    pub fn paint_colored(
        &self,
        cx: &mut PaintCx,
        origin: Point,
        focused: bool,
        color: noa_gpui_scene::Color,
    ) {
        let at = |r: &Rect| r.translate(origin.x, origin.y);
        if !self.found_spans.is_empty() || !self.current_spans.is_empty() {
            let found = cx.color(super::find::FOUND);
            let current = cx.color(super::find::FOUND_CURRENT);
            for rect in &self.found_spans {
                cx.rounded_rect(at(rect), 2.0, found, noa_gpui_scene::Border::default());
            }
            for rect in &self.current_spans {
                cx.rounded_rect(at(rect), 2.0, current, noa_gpui_scene::Border::default());
            }
        }
        if focused {
            let selection = cx.color(Role::Selection);
            for rect in &self.selection {
                cx.fill_rect(at(rect), selection);
            }
        }
        if let Some(layout) = self.shaped.layout() {
            let size = self.shaped.size();
            cx.text(
                Rect::new(origin.x, origin.y, size.width, size.height),
                layout,
                color,
            );
        }
        let ink = cx.color(Role::Text);
        let underline = |cx: &mut PaintCx, rect: &Rect, thickness: f32| {
            let r = at(rect);
            cx.fill_rect(
                Rect::new(r.x, r.bottom() - 2.0, r.width.max(1.0), thickness),
                ink,
            );
        };
        for rect in &self.preedit_spans {
            underline(cx, rect, 1.0);
        }
        for rect in &self.clause_spans {
            underline(cx, rect, 2.5);
        }
        // During composition the IME decides whether a cursor is shown.
        let caret_visible = self.preedit.is_empty() || self.preedit_cursor.is_some();
        if focused
            && caret_visible
            && !self.read_only
            && (!self.preedit.is_empty() || !self.editor.has_selection())
        {
            cx.fill_rect(
                Rect::new(
                    origin.x + self.caret.x,
                    origin.y + self.caret.top + 1.0,
                    1.5,
                    self.caret.height - 2.0,
                ),
                ink,
            );
        }
    }

    /// The caret rectangle in widget coordinates, for placing the IME candidate window.
    pub fn input_area(&self, origin: Point) -> Rect {
        Rect::new(
            origin.x + self.caret.x,
            origin.y + self.caret.top,
            1.0,
            self.caret.height,
        )
    }

    pub fn accessibility(&self, node: &mut accesskit::Node) {
        if self.secure {
            node.set_value(BULLET.repeat(self.editor.text().graphemes(true).count()));
        } else {
            node.set_value(self.editor.text().to_owned());
        }
        if self.read_only {
            node.set_read_only();
        }
        if !self.placeholder.is_empty() {
            node.set_placeholder(self.placeholder.clone());
        }
    }

    pub fn removed(&mut self, text: &mut TextStore) {
        self.shaped.release(text);
    }
}

pub(crate) const FIELD_PAD_Y: f32 = 7.0;

/// A single-line [`Editable`] that scrolls horizontally to keep the caret visible.
/// Shared by the text field, secure field, search field and combo box.
#[derive(Debug)]
pub(crate) struct SingleLine {
    pub edit: Editable,
    pub width: f32,
    pub scroll_x: f32,
    pub pad_left: f32,
    pub pad_right: f32,
    /// Room inside the padding for fixed text before and after the input (a
    /// [`super::TextField`]'s prefix and suffix), set when measuring.
    pub affix_left: f32,
    pub affix_right: f32,
    /// The border-box width layout gave the field when a stylesheet changed it
    /// (see [`SingleLine::measure_styled`]); `None` uses [`SingleLine::width`].
    laid_width: Option<f32>,
}

impl SingleLine {
    pub fn new(placeholder: String, width: f32, pad: f32) -> Self {
        Self {
            edit: Editable::new(
                crate::text_edit::TextEditor::single_line(),
                placeholder,
                TextStyle::new(14.0),
            ),
            width,
            scroll_x: 0.0,
            pad_left: pad,
            pad_right: pad,
            affix_left: 0.0,
            affix_right: 0.0,
            laid_width: None,
        }
    }

    pub fn box_width(&self) -> f32 {
        self.laid_width.unwrap_or(self.width)
    }

    /// [`SingleLine::measure`] that applies the stylesheet's `font-size`, `font-weight`
    /// and width, so the text scrolls and clips inside the box that is drawn.
    pub fn measure_styled(
        &mut self,
        store: &mut TextStore,
        patch: &StylePatch,
        known_width: Option<f32>,
    ) -> Size {
        self.edit.restyle(store, patch);
        // Layout also measures without a known width (for content sizes): keep the
        // width from the last pass that had one.
        if let Some(width) = known_width {
            self.laid_width = ((width - self.width).abs() > 0.5).then_some(width);
        }
        self.measure(store)
    }

    pub fn style(&self) -> Style {
        Style {
            size: LayoutSize {
                width: length(self.width),
                height: Dimension::auto(),
            },
            padding: LayoutRect {
                left: length(self.pad_left),
                right: length(self.pad_right),
                top: length(FIELD_PAD_Y),
                bottom: length(FIELD_PAD_Y),
            },
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn visible(&self) -> f32 {
        (self.box_width() - self.pad_left - self.pad_right - self.affix_left - self.affix_right)
            .max(1.0)
    }

    pub fn measure(&mut self, store: &mut TextStore) -> Size {
        let size = self.edit.shape(store, None);
        let visible = self.visible();
        let caret = self.edit.caret().x;
        if caret - self.scroll_x > visible - 1.0 {
            self.scroll_x = caret - visible + 1.0;
        } else if caret < self.scroll_x {
            self.scroll_x = caret;
        }
        self.scroll_x = self
            .scroll_x
            .min((size.width + 1.0 - visible).max(0.0))
            .max(0.0);
        Size::new(size.width.min(visible), self.edit.style.line_height)
    }

    pub fn origin(&self) -> Point {
        Point::new(self.pad_left + self.affix_left - self.scroll_x, FIELD_PAD_Y)
    }

    pub fn paint_text(&self, cx: &mut PaintCx, focused: bool) {
        let size = cx.size();
        cx.push_clip(Rect::new(
            self.pad_left + self.affix_left - 2.0,
            0.0,
            self.visible() + 4.0,
            size.height,
        ));
        self.edit.paint(cx, self.origin(), focused);
        cx.pop_clip();
    }

    /// Editing, plus [`TextSubmitted`] on Return (left unhandled, so a default button
    /// still answers it).
    pub fn event(&mut self, cx: &mut EventCx, event: &Event) {
        if let Event::KeyDown {
            key: Key::Enter, ..
        } = event
            && self.edit.composition().is_empty()
        {
            cx.emit(TextSubmitted(self.edit.editor.text().to_owned()));
            return;
        }
        let origin = self.origin();
        self.edit.event(cx, event, origin);
    }

    pub fn input_area(&self) -> Rect {
        self.edit.input_area(self.origin())
    }

    pub fn cursor(&self, local: Point) -> Option<Cursor> {
        let inside = local.x >= self.pad_left && local.x < self.box_width() - self.pad_right;
        inside.then_some(Cursor::Text)
    }
}

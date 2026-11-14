use std::ops::Range;

use ui_layout::style::{Dimension, Rect as LayoutRect, Size as LayoutSize, length, percent};
use ui_layout::{MeasureInput, Style};
use ui_scene::{Point, Rect, Size};
use ui_text::{TextStore, TextStyle};

use super::editable::Editable;
use super::find::{FindAction, FindStatus, Finder};
use super::text_field::paint_field_box;
use crate::event::{Cursor, Event};
use crate::text_edit::TextEditor;
use crate::theme::Corner;
use crate::widget::{EventCx, MeasureCx, PaintCx, UpdateCx, Widget};

const PAD: f32 = 8.0;

/// A multi-line text input that wraps at its width and grows with its content. Put it
/// in a [`super::ScrollView`] to bound its height.
///
/// Emits [`super::TextChanged`] after the committed text changes.
#[derive(Debug)]
pub struct TextArea {
    edit: Editable,
    finder: Finder,
    width: Dimension,
    min_lines: u32,
}

impl TextArea {
    pub fn new(placeholder: impl Into<String>) -> Self {
        Self {
            edit: Editable::new(
                TextEditor::multi_line(),
                placeholder.into(),
                TextStyle::new(14.0),
            ),
            finder: Finder::default(),
            width: percent(1.0),
            min_lines: 3,
        }
    }

    /// A fixed width instead of filling the parent.
    pub fn width(mut self, width: f32) -> Self {
        self.width = length(width);
        self
    }

    /// Minimum height in lines, kept even when there is less text.
    pub fn min_lines(mut self, lines: u32) -> Self {
        self.min_lines = lines.max(1);
        self
    }

    pub fn text(&self) -> &str {
        self.edit.editor.text()
    }

    pub fn caret(&self) -> usize {
        self.edit.editor.caret()
    }

    pub fn selection(&self) -> Range<usize> {
        self.edit.editor.selection()
    }

    pub fn composition(&self) -> &str {
        self.edit.composition()
    }

    /// Replace the content and move the caret to the end. Clears the undo history.
    pub fn set_text(&mut self, cx: &mut UpdateCx, text: impl Into<String>) {
        let text = text.into();
        if text != self.text() {
            self.edit.editor.set_text(&text);
            cx.request_layout();
        }
    }

    pub fn select(&mut self, cx: &mut UpdateCx, range: Range<usize>) {
        self.edit.editor.select(range);
        cx.request_layout();
    }

    /// Find or replace text (see [`super::FindBar`] and [`super::drive_find`]).
    pub fn find(&mut self, cx: &mut UpdateCx, action: &FindAction) -> FindStatus {
        let status = self.finder.apply(&mut self.edit, action);
        cx.request_layout();
        cx.request_paint();
        status
    }

    /// The byte ranges found by the last search (empty when none is shown).
    pub fn found_ranges(&self) -> &[Range<usize>] {
        &self.edit.found
    }

    /// The current match, an index into [`Self::found_ranges`].
    pub fn current_found(&self) -> Option<usize> {
        self.edit.found_current
    }
}

const ORIGIN: Point = Point::new(PAD, PAD);

impl Widget for TextArea {
    fn cursor(&self, _local: Point, _size: Size) -> Option<Cursor> {
        Some(Cursor::Text)
    }

    fn style(&self) -> Style {
        Style {
            size: LayoutSize {
                width: self.width,
                height: Dimension::auto(),
            },
            padding: LayoutRect::length(PAD),
            flex_shrink: 0.0,
            ..Default::default()
        }
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
        let wrap = input.wrap_width();
        self.edit.restyle(cx.text, cx.style);
        // Edits move the matches.
        self.finder.sync(&mut self.edit);
        let size = self.edit.shape(cx.text, wrap);
        let min_height = self.edit.style.line_height * self.min_lines as f32;
        Size::new(wrap.unwrap_or(size.width), size.height.max(min_height))
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let focused = cx.is_focused();
        paint_field_box(cx, Corner::Container, 6.0);
        self.edit.text_color = cx.styling().color;
        let size = cx.size();
        cx.push_clip(Rect::new(0.0, 0.0, size.width, size.height));
        self.edit.paint(cx, ORIGIN, focused);
        cx.pop_clip();
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        self.edit.event(cx, event, ORIGIN);
    }

    fn focusable(&self) -> bool {
        true
    }

    fn text_input_area(&self) -> Option<Rect> {
        Some(self.edit.input_area(ORIGIN))
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::MultilineTextInput)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        self.edit.accessibility(node);
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.edit.removed(text);
    }
}

use std::ops::Range;

use noa_gpui_layout::style::{Dimension, Rect as LayoutRect, Size as LayoutSize, length, percent};
use noa_gpui_layout::{AvailableSpace, MeasureInput, Style};
use noa_gpui_scene::{Point, Rect, Size};
use noa_gpui_text::{TextStore, TextStyle};

use super::editable::Editable;
use super::find::{FindAction, FindStatus, Finder};
use super::text_field::paint_field_box;
use super::view_height;
use crate::event::{Cursor, Event};
use crate::text_edit::TextEditor;
use crate::theme::Role;
use crate::widget::{EventCx, MeasureCx, PaintCx, UpdateCx, Widget};

const PAD: f32 = 8.0;
/// Room on the right for the scroll indicator.
const BAR: f32 = 8.0;

/// A multi-line editor that scrolls its content (`NSTextView` in an `NSScrollView`).
/// Plain text only. Its height is fixed, or fills the parent ([`TextView::fill_height`]).
/// Emits [`super::TextChanged`].
#[derive(Debug)]
pub struct TextView {
    edit: Editable,
    finder: Finder,
    width: Dimension,
    /// `None` fills the parent ([`TextView::fill_height`]).
    height: Option<f32>,
    /// Height of the text area inside the padding, from the last layout.
    visible: f32,
    scroll_y: f32,
    content_height: f32,
}

impl TextView {
    pub fn new(height: f32) -> Self {
        Self {
            edit: Editable::new(
                TextEditor::multi_line(),
                String::new(),
                TextStyle::new(14.0),
            ),
            finder: Finder::default(),
            width: percent(1.0),
            height: Some(height),
            visible: (height - 2.0 * PAD).max(1.0),
            scroll_y: 0.0,
            content_height: 0.0,
        }
    }

    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.edit.placeholder = placeholder.into();
        self
    }

    /// A fixed width instead of filling the parent.
    pub fn width(mut self, width: f32) -> Self {
        self.width = length(width);
        self
    }

    /// Fill the height the parent has left: the rest of a column, or the height of a
    /// row, instead of the height given to [`TextView::new`].
    pub fn fill_height(mut self) -> Self {
        self.height = None;
        self
    }

    /// Change the height after the view was created: `Some` fixes it, `None` fills the
    /// parent as [`TextView::fill_height`] does.
    pub fn set_height(&mut self, cx: &mut UpdateCx, height: Option<f32>) {
        if self.height != height {
            self.height = height;
            if let Some(height) = height {
                self.visible = (height - 2.0 * PAD).max(1.0);
            }
            cx.request_layout();
        }
    }

    pub fn read_only(mut self, read_only: bool) -> Self {
        self.edit.read_only = read_only;
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

    pub fn scroll_y(&self) -> f32 {
        self.scroll_y
    }

    /// Replace the content and move the caret to the end. Clears the undo history.
    pub fn set_text(&mut self, cx: &mut UpdateCx, text: impl Into<String>) {
        self.edit.editor.set_text(&text.into());
        cx.request_layout();
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

    fn visible(&self) -> f32 {
        self.visible
    }

    fn max_scroll(&self) -> f32 {
        (self.content_height - self.visible()).max(0.0)
    }

    fn origin(&self) -> Point {
        Point::new(PAD, PAD - self.scroll_y)
    }
}

impl Widget for TextView {
    fn cursor(&self, _local: Point, _size: Size) -> Option<Cursor> {
        Some(Cursor::Text)
    }

    fn style(&self) -> Style {
        let mut style = Style {
            size: LayoutSize {
                width: self.width,
                height: Dimension::auto(),
            },
            padding: LayoutRect {
                left: length(PAD),
                right: length(PAD + BAR),
                top: length(PAD),
                bottom: length(PAD),
            },
            ..Default::default()
        };
        view_height(&mut style, self.height, 2.0 * PAD + 20.0);
        style
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
        let offered = match input.available.height {
            AvailableSpace::Definite(h) if self.height.is_none() => Some(h),
            _ => None,
        };
        if let Some(visible) = input.known.height.map(|h| h - 2.0 * PAD).or(offered) {
            self.visible = visible.max(1.0);
        }
        // Edits move the matches.
        self.finder.sync(&mut self.edit);
        let size = self.edit.shape(cx.text, wrap);
        self.content_height = size.height;
        // Follow the caret (layout runs after edits and caret moves, not after wheel
        // scrolling).
        let caret = self.edit.caret();
        let visible = self.visible();
        if caret.top < self.scroll_y {
            self.scroll_y = caret.top;
        } else if caret.top + caret.height > self.scroll_y + visible {
            self.scroll_y = caret.top + caret.height - visible;
        }
        self.scroll_y = self.scroll_y.clamp(0.0, self.max_scroll());
        Size::new(wrap.unwrap_or(size.width), visible)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        paint_field_box(cx, crate::theme::Corner::Container, 6.0);
        let size = cx.size();
        // The laid-out height decides what the wheel and the scroll indicator cover.
        self.visible = (size.height - 2.0 * PAD).max(1.0);
        self.scroll_y = self.scroll_y.clamp(0.0, self.max_scroll());
        cx.push_clip(Rect::new(2.0, 2.0, size.width - 4.0, size.height - 4.0));
        self.edit.text_color = cx.styling().color;
        let focused = cx.is_focused();
        self.edit.paint(cx, self.origin(), focused);
        cx.pop_clip();
        let max = self.max_scroll();
        if max > 0.0 {
            let track = size.height - 2.0 * PAD;
            let thumb = (track * self.visible() / self.content_height).max(20.0);
            let y = PAD + (track - thumb) * self.scroll_y / max;
            let color = cx.color(Role::Muted).with_alpha(0.5);
            cx.rounded_rect(
                Rect::new(size.width - BAR - 1.0, y, 5.0, thumb),
                2.5,
                color,
                noa_gpui_scene::Border::default(),
            );
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        if let Event::Wheel { delta, .. } = event {
            let max = self.max_scroll();
            let next = (self.scroll_y - delta.y).clamp(0.0, max);
            if next != self.scroll_y {
                self.scroll_y = next;
                cx.request_paint();
                cx.set_handled();
            }
            return;
        }
        self.edit.event(cx, event, self.origin());
    }

    fn focusable(&self) -> bool {
        true
    }

    fn text_input_area(&self) -> Option<Rect> {
        (!self.edit.read_only).then(|| self.edit.input_area(self.origin()))
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

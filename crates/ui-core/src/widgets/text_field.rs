use std::ops::Range;

use noa_gpui_layout::{MeasureInput, Style};
use noa_gpui_scene::{Border, Point, Rect, Size};
use noa_gpui_text::{TextStore, TextStyle};

use super::CachedText;
use super::editable::{FIELD_PAD_Y, SingleLine};
use crate::event::{Cursor, Event};
use crate::theme::{Corner, Role};
use crate::widget::{EventCx, MeasureCx, PaintCx, UpdateCx, Widget};

const PAD_X: f32 = 10.0;
const AFFIX_GAP: f32 = 3.0;

/// A single-line text input (`NSTextField`).
///
/// Editing works on grapheme clusters, so emoji sequences are never split. The
/// [`TextField::prefix`] and [`TextField::suffix`] texts are not part of
/// [`TextField::text`].
///
/// Emits [`super::TextChanged`] after the committed text changes and
/// [`super::TextSubmitted`] on Return.
#[derive(Debug)]
pub struct TextField {
    pub(crate) line: SingleLine,
    prefix: Affix,
    suffix: Affix,
}

#[derive(Debug, Default)]
pub(crate) struct Affix {
    pub text: String,
    pub shaped: CachedText,
}

impl Affix {
    /// Shape the text in the field's style; returns the room it takes, gap included.
    pub fn measure(&mut self, store: &mut TextStore, style: TextStyle) -> f32 {
        if self.text.is_empty() {
            self.shaped.release(store);
            return 0.0;
        }
        self.shaped.shape(store, &self.text, style, None).width + AFFIX_GAP
    }

    /// Draw the text with its left edge at `x`, on the field's text line.
    pub fn paint(&self, cx: &mut PaintCx, x: f32) {
        if self.text.is_empty() {
            return;
        }
        if let Some(layout) = self.shaped.layout() {
            let size = self.shaped.size();
            let ink = cx.color(Role::Muted);
            cx.text(
                Rect::new(x, FIELD_PAD_Y, size.width, size.height),
                layout,
                ink,
            );
        }
    }
}

impl TextField {
    pub fn new(placeholder: impl Into<String>, width: f32) -> Self {
        Self {
            line: SingleLine::new(placeholder.into(), width, PAD_X),
            prefix: Affix::default(),
            suffix: Affix::default(),
        }
    }

    pub fn prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix.text = prefix.into();
        self
    }

    pub fn suffix(mut self, suffix: impl Into<String>) -> Self {
        self.suffix.text = suffix.into();
        self
    }

    pub fn set_prefix(&mut self, cx: &mut UpdateCx, prefix: impl Into<String>) {
        self.prefix.text = prefix.into();
        cx.request_layout();
    }

    pub fn set_suffix(&mut self, cx: &mut UpdateCx, suffix: impl Into<String>) {
        self.suffix.text = suffix.into();
        cx.request_layout();
    }

    pub fn prefix_text(&self) -> &str {
        &self.prefix.text
    }

    pub fn suffix_text(&self) -> &str {
        &self.suffix.text
    }

    /// Selectable and copyable, but not editable.
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.line.edit.read_only = read_only;
        self
    }

    pub fn set_read_only(&mut self, cx: &mut UpdateCx, read_only: bool) {
        self.line.edit.read_only = read_only;
        cx.request_paint();
    }

    pub fn is_read_only(&self) -> bool {
        self.line.edit.read_only
    }

    pub fn text(&self) -> &str {
        self.line.edit.editor.text()
    }

    /// Caret position as a byte index into [`TextField::text`].
    pub fn caret(&self) -> usize {
        self.line.edit.editor.caret()
    }

    /// The selected byte range (empty when nothing is selected).
    pub fn selection(&self) -> Range<usize> {
        self.line.edit.editor.selection()
    }

    pub fn composition(&self) -> &str {
        self.line.edit.composition()
    }

    pub fn scroll_x(&self) -> f32 {
        self.line.scroll_x
    }

    /// Replace the content and move the caret to the end. Clears the undo history.
    pub fn set_text(&mut self, cx: &mut UpdateCx, text: impl Into<String>) {
        let text = text.into();
        if text != self.text() {
            self.line.edit.editor.set_text(&text);
            cx.request_layout();
        }
    }

    pub fn select(&mut self, cx: &mut UpdateCx, range: Range<usize>) {
        self.line.edit.editor.select(range);
        cx.request_layout();
    }
}

/// The rounded field box (a `corner` of `radius` in Big Sur): an accent border while
/// focused. The stylesheet's `background-color`, `border-color`, `border-width` and
/// `border-radius` replace the native fill, border and corner; the border keeps the
/// accent while focused unless a rule (for example `TextField:focus`) sets a color.
pub(crate) fn paint_field_box(cx: &mut PaintCx, corner: Corner, radius: f32) {
    let styled = cx.styling();
    let (fill, border_paint, border_width, styled_radius) = (
        styled.background_color,
        styled.border_color,
        styled.border_width,
        styled.border_radius,
    );
    let radius = styled_radius.unwrap_or_else(|| cx.corner(corner, radius, cx.size().height));
    let focused = cx.is_focused();
    let border = match (focused, border_paint) {
        (true, None) => Border {
            width: border_width.map_or(2.0, |w| w.max(2.0)),
            color: cx.color(Role::Accent),
        },
        (_, paint) => Border {
            width: border_width.unwrap_or(1.0),
            color: cx.color(paint.unwrap_or(Role::Outline.into())),
        },
    };
    let fill = cx.color(fill.unwrap_or(Role::Surface.into()));
    cx.bezel(cx.bounds(), radius, fill, border);
}

pub(crate) fn measure_field(
    line: &mut SingleLine,
    cx: &mut MeasureCx,
    input: MeasureInput,
) -> Size {
    line.measure_styled(cx.text, cx.style, input.known.width)
}

impl Widget for TextField {
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
        self.line.edit.restyle(cx.text, cx.style);
        let style = self.line.edit.style;
        self.line.affix_left = self.prefix.measure(cx.text, style);
        self.line.affix_right = self.suffix.measure(cx.text, style);
        measure_field(&mut self.line, cx, input)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        paint_field_box(cx, Corner::Control, 6.0);
        self.line.edit.text_color = cx.styling().color;
        let focused = cx.is_focused();
        self.prefix.paint(cx, self.line.pad_left);
        let suffix_x =
            self.line.box_width() - self.line.pad_right - self.line.affix_right + AFFIX_GAP;
        self.suffix.paint(cx, suffix_x);
        self.line.paint_text(cx, focused);
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        self.line.event(cx, event);
    }

    fn focusable(&self) -> bool {
        true
    }

    fn text_input_area(&self) -> Option<Rect> {
        // A read-only field takes no text input, so the IME stays off.
        (!self.line.edit.read_only).then(|| self.line.input_area())
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::TextInput)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        self.line.edit.accessibility(node);
        let affixes: Vec<&str> = [&self.prefix.text, &self.suffix.text]
            .into_iter()
            .filter(|a| !a.is_empty())
            .map(String::as_str)
            .collect();
        if !affixes.is_empty() {
            node.set_description(affixes.join(" … "));
        }
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.line.edit.removed(text);
        self.prefix.shaped.release(text);
        self.suffix.shaped.release(text);
    }
}

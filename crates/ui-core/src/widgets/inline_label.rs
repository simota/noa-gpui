use noa_gpui_layout::style::{Dimension, Size as LayoutSize, auto, length};
use noa_gpui_layout::{MeasureInput, Style};
use noa_gpui_scene::{Border, Point, Rect, Size};
use noa_gpui_text::{TextStore, TextStyle};

use super::CachedText;
use super::editable::Editable;
use crate::event::{Cursor, Event, Key, PointerButton};
use crate::text_edit::TextEditor;
use crate::theme::{Paint, Role};
use crate::widget::{EventCx, MeasureCx, PaintCx, UpdateCx, Widget};

/// Emitted by an [`InlineEditableLabel`] when an edit is committed with new text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelEdited(pub String);

/// Emitted by an [`super::OutlineView`] or [`super::CollectionView`] when an item is
/// renamed in place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemRenamed {
    pub item: usize,
    pub name: String,
}

const PAD_X: f32 = 3.0;
const PAD_Y: f32 = 1.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InlineOutcome {
    Editing,
    /// Return: keep the text.
    Commit(String),
    /// Escape: drop the edit.
    Cancel,
}

#[derive(Debug)]
pub(crate) struct InlineEditor {
    pub edit: Editable,
    /// Where the text is drawn when not edited (widget coordinates; the box grows to
    /// the right as the text does, up to `max_width`).
    pub at: Point,
    pub max_width: f32,
}

impl InlineEditor {
    /// All of `text` starts selected.
    pub fn new(text: &str, style: TextStyle, at: Point, max_width: f32) -> Self {
        let mut editor = TextEditor::single_line();
        editor.set_text(text);
        editor.select_all();
        Self {
            edit: Editable::new(editor, String::new(), style),
            at,
            max_width,
        }
    }

    pub fn text(&self) -> &str {
        self.edit.editor.text()
    }

    pub fn shape(&mut self, store: &mut TextStore) -> Size {
        self.edit.shape(store, None)
    }

    /// Widget coordinates.
    pub fn rect(&self) -> Rect {
        let size = self.edit.shaped.size();
        let width = (size.width + 2.0 * PAD_X + 2.0).min(self.max_width.max(2.0 * PAD_X));
        Rect::new(
            self.at.x - PAD_X,
            self.at.y - PAD_Y,
            width,
            self.edit.style.line_height + 2.0 * PAD_Y,
        )
    }

    /// Where the text layout's origin is, scrolled so the caret stays in the box.
    fn origin(&self) -> Point {
        let rect = self.rect();
        let visible = rect.width - 2.0 * PAD_X;
        let caret = self.edit.caret().x;
        let scroll = (caret + 1.0 - visible).max(0.0);
        Point::new(self.at.x - scroll, self.at.y)
    }

    pub fn event(&mut self, cx: &mut EventCx, event: &Event) -> InlineOutcome {
        let composing = !self.edit.composition().is_empty();
        match event {
            Event::KeyDown {
                key: Key::Enter, ..
            } if !composing => {
                cx.set_handled();
                InlineOutcome::Commit(self.text().to_owned())
            }
            Event::KeyDown {
                key: Key::Escape, ..
            } if !composing => {
                cx.set_handled();
                InlineOutcome::Cancel
            }
            _ => {
                let origin = self.origin();
                self.edit.event(cx, event, origin);
                if matches!(event, Event::KeyDown { .. })
                    && !matches!(event, Event::KeyDown { key: Key::Tab, .. })
                {
                    // Keys typed into the name do not reach the list around it.
                    cx.set_handled();
                }
                cx.request_paint();
                InlineOutcome::Editing
            }
        }
    }

    pub fn paint(&mut self, cx: &mut PaintCx) {
        self.shape(cx.text_store());
        let rect = self.rect();
        let fill = cx.color(Role::Surface);
        let border = Border {
            width: 1.0,
            color: cx.color(Role::Accent),
        };
        cx.rounded_rect(rect, 3.0, fill, border);
        cx.push_clip(Rect::new(
            rect.x + 1.0,
            rect.y,
            (rect.width - 2.0).max(0.0),
            rect.height,
        ));
        let origin = self.origin();
        self.edit.paint(cx, origin, true);
        cx.pop_clip();
    }

    pub fn input_area(&self) -> Rect {
        self.edit.input_area(self.origin())
    }

    pub fn removed(&mut self, text: &mut TextStore) {
        self.edit.removed(text);
    }
}

/// A [`super::Label`] that can be renamed in place, like a file name in the Finder.
/// Emits [`LabelEdited`] when the text changed. Empty text is refused unless
/// [`InlineEditableLabel::allow_empty`].
#[derive(Debug)]
pub struct InlineEditableLabel {
    text: String,
    style: TextStyle,
    color: Paint,
    allow_empty: bool,
    max_width: Option<f32>,
    shaped: CachedText,
    editing: Option<InlineEditor>,
}

impl InlineEditableLabel {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: TextStyle::new(14.0),
            color: Paint::Role(Role::Text),
            allow_empty: false,
            max_width: None,
            shaped: CachedText::default(),
            editing: None,
        }
    }

    pub fn font_size(mut self, px: f32) -> Self {
        self.style = TextStyle {
            font_size: px,
            line_height: TextStyle::new(px).line_height,
            ..self.style
        };
        self
    }

    pub fn weight(mut self, weight: u16) -> Self {
        self.style = self.style.weight(weight);
        self
    }

    pub fn color(mut self, color: impl Into<Paint>) -> Self {
        self.color = color.into();
        self
    }

    pub fn allow_empty(mut self, allow: bool) -> Self {
        self.allow_empty = allow;
        self
    }

    /// The text scrolls beyond this width.
    pub fn max_width(mut self, width: f32) -> Self {
        self.max_width = Some(width);
        self
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_editing(&self) -> bool {
        self.editing.is_some()
    }

    pub fn edited_text(&self) -> Option<&str> {
        self.editing.as_ref().map(InlineEditor::text)
    }

    pub fn set_text(&mut self, cx: &mut UpdateCx, text: impl Into<String>) {
        let text = text.into();
        if text != self.text {
            self.text = text;
            cx.request_layout();
        }
    }

    /// Does not focus the label; call [`crate::UiTree::set_focus`] too.
    pub fn begin_editing(&mut self, cx: &mut UpdateCx) {
        self.start();
        cx.request_layout();
    }

    fn start(&mut self) {
        if self.editing.is_none() {
            let max = self.max_width.unwrap_or(f32::MAX / 4.0);
            self.editing = Some(InlineEditor::new(
                &self.text,
                self.style,
                Point::new(PAD_X, PAD_Y),
                max,
            ));
        }
    }

    /// Stop editing, keeping `text` if it is acceptable and new.
    fn finish(&mut self, cx: &mut EventCx, text: Option<String>) {
        let Some(mut editor) = self.editing.take() else {
            return;
        };
        editor.removed(cx.text_store());
        if let Some(text) = text
            && (self.allow_empty || !text.trim().is_empty())
            && text != self.text
        {
            self.text.clone_from(&text);
            cx.emit(LabelEdited(text));
        }
        cx.request_layout();
    }
}

impl Widget for InlineEditableLabel {
    fn cursor(&self, _local: Point, _size: Size) -> Option<Cursor> {
        self.editing.as_ref().map(|_| Cursor::Text)
    }

    fn style(&self) -> Style {
        Style {
            size: LayoutSize {
                width: Dimension::auto(),
                height: auto(),
            },
            min_size: LayoutSize {
                width: length(0.0),
                height: auto(),
            },
            flex_shrink: 1.0,
            ..Default::default()
        }
    }

    fn measures(&self) -> bool {
        true
    }

    fn measure(&mut self, cx: &mut MeasureCx, input: MeasureInput) -> Size {
        let label = self.shaped.shape(cx.text, &self.text, self.style, None);
        let available = input.known.width.or(input.wrap_width());
        let mut width = label.width + 2.0 * PAD_X;
        if let Some(editor) = &mut self.editing {
            let edited = editor.shape(cx.text);
            width = width.max(edited.width + 2.0 * PAD_X + 2.0);
            editor.max_width = self
                .max_width
                .unwrap_or(f32::MAX / 4.0)
                .min(available.unwrap_or(f32::MAX / 4.0));
        }
        if let Some(available) = available {
            width = width.min(available);
        }
        if let Some(max) = self.max_width {
            width = width.min(max);
        }
        Size::new(width, self.style.line_height + 2.0 * PAD_Y)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        if let Some(editor) = &mut self.editing {
            editor.paint(cx);
            return;
        }
        let size = cx.size();
        if cx.is_focused() {
            let fill = cx.color(Role::Selection);
            cx.rounded_rect(cx.bounds(), 3.0, fill, Border::default());
        }
        if let Some(layout) = self.shaped.layout() {
            let text = self.shaped.size();
            let color = cx.color(self.color);
            cx.push_clip(Rect::new(0.0, 0.0, size.width, size.height));
            cx.text(
                Rect::new(PAD_X, PAD_Y, text.width, text.height),
                layout,
                color,
            );
            cx.pop_clip();
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        if let Some(editor) = &mut self.editing {
            match event {
                Event::FocusChanged(false) => {
                    let text = editor.text().to_owned();
                    self.finish(cx, Some(text));
                }
                _ => match editor.event(cx, event) {
                    InlineOutcome::Editing => {}
                    InlineOutcome::Commit(text) => self.finish(cx, Some(text)),
                    InlineOutcome::Cancel => self.finish(cx, None),
                },
            }
            return;
        }
        match event {
            Event::PointerDown {
                button: PointerButton::Primary,
                ..
            } => {
                cx.request_focus();
                cx.set_handled();
                if cx.click_count() == 2 {
                    self.start();
                    cx.request_layout();
                }
            }
            Event::KeyDown {
                key: Key::Enter, ..
            }
            | Event::Activate => {
                self.start();
                cx.request_layout();
                cx.set_handled();
            }
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn text_input_area(&self) -> Option<Rect> {
        self.editing.as_ref().map(InlineEditor::input_area)
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(if self.editing.is_some() {
            accesskit::Role::TextInput
        } else {
            accesskit::Role::Label
        })
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        match &self.editing {
            Some(editor) => editor.edit.accessibility(node),
            None => {
                node.set_value(self.text.clone());
                node.add_action(accesskit::Action::Click);
            }
        }
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.shaped.release(text);
        if let Some(editor) = &mut self.editing {
            editor.removed(text);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RenameStep {
    /// Not the editor's: handle it as usual.
    Ignored,
    Handled,
    /// Renaming `item` ended: with `name` (committed, not checked yet) or cancelled.
    /// `pass_on`: the event (a press elsewhere) should still be handled as usual.
    Done {
        item: usize,
        name: Option<String>,
        pass_on: bool,
    },
}

/// Shared by the outline and collection views.
#[derive(Debug, Default)]
pub(crate) struct Renamer {
    active: Option<(usize, InlineEditor)>,
    /// A press in the editor is being dragged (selecting its text).
    drag: bool,
    /// The area the editor showed in when its view was last painted: `None` when the
    /// item was not drawn (scrolled out of view), so that a press where the editor
    /// used to be, or on what covers it, is not taken for a press in it.
    shown: Option<Rect>,
    /// Editors of renames cancelled without a text store, released at the next paint.
    retired: Vec<InlineEditor>,
}

impl Renamer {
    /// Drop the rename without committing it (the items it pointed into changed).
    pub fn cancel(&mut self) {
        if let Some((_, editor)) = self.active.take() {
            self.retired.push(editor);
        }
        self.drag = false;
    }

    /// Start painting the view: release cancelled editors and forget where the editor
    /// showed, until [`Renamer::editor_in`] places it again.
    pub fn begin_paint(&mut self, store: &mut TextStore) {
        self.release_retired(store);
        self.shown = None;
    }

    /// The items were renumbered: follow the item being renamed to its new number, or
    /// drop the rename when `map` says it is gone.
    pub fn remap(&mut self, map: impl Fn(usize) -> Option<usize>) {
        let Some((item, _)) = self.active.as_mut() else {
            return;
        };
        match map(*item) {
            Some(new) => *item = new,
            None => self.cancel(),
        }
    }

    fn release_retired(&mut self, store: &mut TextStore) {
        for mut editor in self.retired.drain(..) {
            editor.removed(store);
        }
    }

    pub fn start(&mut self, item: usize, title: &str, style: TextStyle) {
        self.cancel();
        self.active = Some((
            item,
            InlineEditor::new(title, style, Point::default(), f32::MAX / 4.0),
        ));
        self.drag = false;
        self.shown = None;
    }

    pub fn item(&self) -> Option<usize> {
        self.active.as_ref().map(|(item, _)| *item)
    }

    pub fn text(&self) -> Option<&str> {
        self.active.as_ref().map(|(_, editor)| editor.text())
    }

    pub fn editor_for(&mut self, item: usize) -> Option<&mut InlineEditor> {
        self.active
            .as_mut()
            .filter(|(i, _)| *i == item)
            .map(|(_, editor)| editor)
    }

    /// The editor to paint, if `item` is being renamed. `clip` is the area (widget
    /// coordinates) it is drawn in: presses outside of it are not the editor's.
    pub fn editor_in(&mut self, item: usize, clip: Rect) -> Option<&mut InlineEditor> {
        let (_, editor) = self.active.as_mut().filter(|(i, _)| *i == item)?;
        self.shown = Some(clip);
        Some(editor)
    }

    pub fn input_area(&self) -> Option<Rect> {
        self.active.as_ref().map(|(_, editor)| editor.input_area())
    }

    pub fn event(&mut self, cx: &mut EventCx, event: &Event) -> RenameStep {
        let Some((item, editor)) = &mut self.active else {
            return RenameStep::Ignored;
        };
        let item = *item;
        let shown = self.shown;
        let inside =
            |p: Point| shown.is_some_and(|area| area.contains(p)) && editor.rect().contains(p);
        let (outcome, pass_on) = match event {
            Event::FocusChanged(false) => (InlineOutcome::Commit(editor.text().to_owned()), false),
            Event::PointerDown { pos, .. } if !inside(cx.local(*pos)) => {
                // A press elsewhere ends the rename, then does what it does.
                (InlineOutcome::Commit(editor.text().to_owned()), true)
            }
            Event::PointerDown { .. } => {
                self.drag = true;
                (editor.event(cx, event), false)
            }
            Event::PointerMove { .. } | Event::PointerUp { .. } if self.drag => {
                if matches!(event, Event::PointerUp { .. }) {
                    self.drag = false;
                }
                (editor.event(cx, event), false)
            }
            Event::KeyDown { .. } | Event::Text(_) | Event::Composition { .. } => {
                (editor.event(cx, event), false)
            }
            _ => return RenameStep::Ignored,
        };
        let name = match outcome {
            InlineOutcome::Editing => return RenameStep::Handled,
            InlineOutcome::Commit(name) => Some(name),
            InlineOutcome::Cancel => None,
        };
        if let Some((_, mut editor)) = self.active.take() {
            editor.removed(cx.text_store());
        }
        self.drag = false;
        cx.request_paint();
        RenameStep::Done {
            item,
            name,
            pass_on,
        }
    }

    pub fn removed(&mut self, text: &mut TextStore) {
        self.cancel();
        self.release_retired(text);
    }
}

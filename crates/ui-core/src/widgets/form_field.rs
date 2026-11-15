use noa_gpui_layout::Style;
use noa_gpui_layout::style::{
    AlignItems, Dimension, FlexDirection, Rect as LayoutRect, Size as LayoutSize, auto, length,
    percent,
};
use noa_gpui_scene::{Color, Rect};
use noa_gpui_text::{TextStore, TextStyle};

use super::{CachedText, Tone};
use crate::theme::Role;
use crate::widget::{PaintCx, UpdateCx, Widget};

const LABEL_SIZE: f32 = 13.0;
const MESSAGE_SIZE: f32 = 12.0;
const GAP: f32 = 4.0;
const LEADING_GAP: f32 = 10.0;
/// Content up to this tall centers a leading label on it; taller content (a text view)
/// puts it at the top.
const CENTER_LIMIT: f32 = 36.0;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum LabelPosition {
    #[default]
    Top,
    /// Right-aligned in a column this wide, so stacked fields line up.
    Leading(f32),
}

/// A control with its label, required mark, help text and error message. Insert the
/// control as the field's child. Messages are one line, clipped to the field's width.
#[derive(Debug)]
pub struct FormField {
    label: String,
    required: bool,
    help: Option<String>,
    error: Option<String>,
    position: LabelPosition,
    width: Dimension,
    shaped_label: CachedText,
    shaped_mark: CachedText,
    shaped_message: CachedText,
}

impl FormField {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            required: false,
            help: None,
            error: None,
            position: LabelPosition::Top,
            width: auto(),
            shaped_label: CachedText::default(),
            shaped_mark: CachedText::default(),
            shaped_message: CachedText::default(),
        }
    }

    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn label_position(mut self, position: LabelPosition) -> Self {
        self.position = position;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = length(width);
        self
    }

    pub fn fill_width(mut self) -> Self {
        self.width = percent(1.0);
        self
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn is_required(&self) -> bool {
        self.required
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// The error, or else the help text.
    pub fn message(&self) -> Option<&str> {
        self.error.as_deref().or(self.help.as_deref())
    }

    /// The error replaces the help text while it is set.
    pub fn set_error(&mut self, cx: &mut UpdateCx, error: Option<String>) {
        if error != self.error {
            self.error = error;
            cx.request_layout();
            cx.request_paint();
        }
    }

    pub fn set_help(&mut self, cx: &mut UpdateCx, help: Option<String>) {
        if help != self.help {
            self.help = help;
            cx.request_layout();
            cx.request_paint();
        }
    }

    fn label_height(&self) -> f32 {
        TextStyle::new(LABEL_SIZE).line_height
    }

    fn message_height(&self) -> f32 {
        if self.message().is_some() {
            GAP + TextStyle::new(MESSAGE_SIZE).line_height
        } else {
            0.0
        }
    }

    fn label_style(&self) -> TextStyle {
        TextStyle::new(LABEL_SIZE).weight(500)
    }
}

impl Widget for FormField {
    fn style(&self) -> Style {
        let (top, left) = match self.position {
            LabelPosition::Top => (self.label_height() + GAP, 0.0),
            LabelPosition::Leading(width) => (0.0, width + LEADING_GAP),
        };
        Style {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::START,
            padding: LayoutRect {
                left: length(left),
                right: length(0.0),
                top: length(top),
                bottom: length(self.message_height()),
            },
            size: LayoutSize {
                width: self.width,
                height: auto(),
            },
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let style = self.label_style();
        let label = self
            .shaped_label
            .shape(cx.text_store(), &self.label, style, None);
        let mark = if self.required {
            self.shaped_mark.shape(cx.text_store(), "*", style, None)
        } else {
            Default::default()
        };
        let critical = Tone::Critical.ink(cx);
        let message_height = self.message_height();
        let (x, y) = match self.position {
            LabelPosition::Top => (0.0, 0.0),
            LabelPosition::Leading(width) => {
                let content = bounds.height - message_height;
                let y = if content <= CENTER_LIMIT {
                    (content - label.height) / 2.0
                } else {
                    6.0
                };
                let total = label.width + if self.required { 2.0 + mark.width } else { 0.0 };
                ((width - total).max(0.0), y)
            }
        };
        if let Some(layout) = self.shaped_label.layout() {
            let ink = cx.color(Role::Text);
            cx.text(Rect::new(x, y, label.width, label.height), layout, ink);
        }
        if self.required
            && let Some(layout) = self.shaped_mark.layout()
        {
            let at = Rect::new(x + label.width + 2.0, y, mark.width, mark.height);
            cx.text(at, layout, critical);
        }
        let message = self.message().map(str::to_owned);
        match message {
            Some(message) => {
                let size = self.shaped_message.shape(
                    cx.text_store(),
                    &message,
                    TextStyle::new(MESSAGE_SIZE),
                    None,
                );
                let left = match self.position {
                    LabelPosition::Top => 0.0,
                    LabelPosition::Leading(width) => width + LEADING_GAP,
                };
                let ink: Color = if self.error.is_some() {
                    critical
                } else {
                    cx.color(Role::Muted)
                };
                if let Some(layout) = self.shaped_message.layout() {
                    let y = bounds.height - size.height;
                    cx.push_clip(Rect::new(
                        left,
                        y,
                        (bounds.width - left).max(0.0),
                        size.height,
                    ));
                    cx.text(Rect::new(left, y, size.width, size.height), layout, ink);
                    cx.pop_clip();
                }
            }
            None => {
                let store = cx.text_store();
                self.shaped_message.release(store);
            }
        }
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Group)
    }

    fn labels_content(&self) -> bool {
        true
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        node.set_label(self.label.clone());
        if self.required {
            node.set_required();
        }
        if let Some(message) = self.message() {
            node.set_description(message.to_owned());
        }
        if self.error.is_some() {
            node.set_invalid(accesskit::Invalid::True);
        }
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.shaped_label.release(text);
        self.shaped_mark.release(text);
        self.shaped_message.release(text);
    }
}

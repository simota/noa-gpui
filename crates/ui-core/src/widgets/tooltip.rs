//! The tree opens and closes tooltips (see [`crate::UiTree::next_timer`]); a widget
//! only says what its tooltip is, through [`crate::Widget::tooltip`].

use ui_layout::style::Rect as LayoutRect;
use ui_layout::style::length;
use ui_layout::{MeasureInput, Style};
use ui_scene::{Border, Rect, Size};
use ui_text::{TextStore, TextStyle};

use super::CachedText;
use super::appear::Appear;
use crate::event::Event;
use crate::theme::{Corner, Role};
use crate::widget::{EventCx, MeasureCx, PaintCx, Widget};

const MAX_WIDTH: f32 = 300.0;
const PAD_X: f32 = 7.0;
const PAD_Y: f32 = 4.0;
/// Fade-in time, seconds.
const FADE: f64 = 0.12;

#[derive(Debug)]
pub struct Tooltip {
    text: String,
    shaped: CachedText,
    /// Whether the text is too long for one line (decided on the first measure).
    wraps: Option<bool>,
    appear: Appear,
}

impl Tooltip {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            shaped: CachedText::default(),
            wraps: None,
            appear: Appear::new(FADE),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    fn text_style() -> TextStyle {
        TextStyle::new(12.0)
    }
}

impl Widget for Tooltip {
    fn style(&self) -> Style {
        Style {
            padding: LayoutRect {
                left: length(PAD_X),
                right: length(PAD_X),
                top: length(PAD_Y),
                bottom: length(PAD_Y),
            },
            ..Default::default()
        }
    }

    fn measures(&self) -> bool {
        true
    }

    fn measure(&mut self, cx: &mut MeasureCx, _input: MeasureInput) -> Size {
        let wraps = *self.wraps.get_or_insert_with(|| {
            let natural = self
                .shaped
                .shape(cx.text, &self.text, Self::text_style(), None);
            natural.width > MAX_WIDTH
        });
        let wrap = wraps.then_some(MAX_WIDTH);
        self.shaped
            .shape(cx.text, &self.text, Self::text_style(), wrap)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let fill = cx.color(Role::Surface);
        let border = Border {
            width: 1.0,
            color: cx.color(Role::Outline),
        };
        let radius = cx.corner(Corner::Container, 5.0, bounds.height);
        cx.floating_surface(bounds, radius, fill, border);
        if let Some(layout) = self.shaped.layout() {
            let size = self.shaped.size();
            let ink = cx.color(Role::Text);
            cx.text(
                Rect::new(PAD_X, PAD_Y, size.width, size.height),
                layout,
                ink,
            );
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        if let Event::AnimationFrame { now } = event {
            self.appear.frame(cx, *now);
            cx.request_composite();
        }
    }

    fn opacity(&self) -> f32 {
        self.appear.value()
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Tooltip)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        node.set_label(self.text.clone());
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.shaped.release(text);
    }
}

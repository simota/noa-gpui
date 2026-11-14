use ui_layout::{MeasureInput, Style};
use ui_scene::{Border, Point, Rect, Size};
use ui_text::{TextStore, TextStyle};

use super::CachedText;
use super::badge::{centered, cross};
use super::focus_ring;
use super::tone::Tone;
use crate::event::{Event, Key, PointerButton};
use crate::theme::{Corner, Role};
use crate::widget::{EventCx, MeasureCx, PaintCx, UpdateCx, Widget};

const PAD: f32 = 12.0;
const ICON: f32 = 16.0;
const CLOSE: f32 = 20.0;
const ACTION_PAD: f32 = 10.0;
const ACTION_H: f32 = 24.0;
const GAP: f32 = 10.0;
const TITLE_GAP: f32 = 2.0;
const RADIUS: f32 = 8.0;
/// Width when the parent leaves it unconstrained, as inside a row.
const DEFAULT_WIDTH: f32 = 420.0;

/// Emitted (with the banner as the source) when its action button is clicked, or
/// Return is pressed while it has focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BannerAction;

/// Emitted (with the banner as the source) when its × is clicked, or Escape is pressed
/// while it has focus. The banner then removes itself from the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BannerClosed;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Action,
    Close,
}

/// A notice inside the layout, as wide as its container. Emits [`BannerAction`] and
/// [`BannerClosed`]. Warnings and errors are announced as alerts.
#[derive(Debug)]
pub struct NotificationBanner {
    title: Option<String>,
    message: String,
    tone: Tone,
    action: Option<String>,
    closable: bool,

    over: Option<Part>,
    pressed: Option<Part>,

    text: CachedText,
    title_text: CachedText,
    action_text: CachedText,
    icon_text: CachedText,
}

impl NotificationBanner {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            title: None,
            message: message.into(),
            tone: Tone::Accent,
            action: None,
            closable: true,
            over: None,
            pressed: None,
            text: CachedText::default(),
            title_text: CachedText::default(),
            action_text: CachedText::default(),
            icon_text: CachedText::default(),
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    pub fn action(mut self, title: impl Into<String>) -> Self {
        self.action = Some(title.into());
        self
    }

    /// On by default.
    pub fn closable(mut self, closable: bool) -> Self {
        self.closable = closable;
        self
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn set_message(&mut self, cx: &mut UpdateCx, message: impl Into<String>) {
        let message = message.into();
        if message != self.message {
            self.message = message;
            cx.request_layout();
            cx.request_paint();
        }
    }

    pub fn set_tone(&mut self, cx: &mut UpdateCx, tone: Tone) {
        if tone != self.tone {
            self.tone = tone;
            cx.request_layout();
            cx.request_paint();
        }
    }

    fn icon_glyph(&self) -> &'static str {
        match self.tone {
            Tone::Success => "✓",
            Tone::Warning | Tone::Critical => "!",
            Tone::Neutral | Tone::Accent => "i",
        }
    }

    /// What assistive technology reads.
    fn body(&self) -> String {
        match &self.title {
            Some(title) => format!("{title}. {}", self.message),
            None => self.message.clone(),
        }
    }

    /// Zero without a title.
    fn title_height(&self) -> f32 {
        match self.title_text.layout() {
            Some(_) => self.title_text.size().height + TITLE_GAP,
            None => 0.0,
        }
    }

    fn text_style() -> TextStyle {
        TextStyle::new(13.0)
    }

    fn bold_style() -> TextStyle {
        TextStyle::new(13.0).weight(600)
    }

    fn trailing_width(&self) -> f32 {
        let action = match self.action {
            Some(_) => self.action_text.size().width + 2.0 * ACTION_PAD + GAP,
            None => 0.0,
        };
        let close = if self.closable { CLOSE + GAP } else { 0.0 };
        action + close
    }

    /// In the banner's coordinates.
    fn parts(&self, size: Size) -> (Option<Rect>, Option<Rect>) {
        let mut right = size.width - PAD;
        let close = self.closable.then(|| {
            right -= CLOSE;
            Rect::new(right, PAD + (ACTION_H - CLOSE) / 2.0, CLOSE, CLOSE)
        });
        if close.is_some() {
            right -= GAP;
        }
        let action = self.action.as_ref().map(|_| {
            let w = self.action_text.size().width + 2.0 * ACTION_PAD;
            Rect::new(right - w, PAD, w, ACTION_H)
        });
        (action, close)
    }

    fn part_at(&self, local: Point, size: Size) -> Option<Part> {
        let (action, close) = self.parts(size);
        if action.is_some_and(|r| r.contains(local)) {
            Some(Part::Action)
        } else if close.is_some_and(|r| r.contains(local)) {
            Some(Part::Close)
        } else {
            None
        }
    }

    fn activate(&mut self, cx: &mut EventCx, part: Part) {
        match part {
            Part::Action => cx.emit(BannerAction),
            Part::Close => {
                cx.emit(BannerClosed);
                cx.remove_self();
            }
        }
    }
}

impl Widget for NotificationBanner {
    fn style(&self) -> Style {
        Style {
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn measures(&self) -> bool {
        true
    }

    fn measure(&mut self, cx: &mut MeasureCx, input: MeasureInput) -> Size {
        let width = input
            .known
            .width
            .or(input.wrap_width())
            .unwrap_or(DEFAULT_WIDTH);
        if let Some(title) = &self.action {
            self.action_text
                .shape(cx.text, title, Self::bold_style(), None);
        }
        let glyph = self.icon_glyph();
        self.icon_text
            .shape(cx.text, glyph, TextStyle::new(11.0).weight(700), None);
        let wrap = (width - 2.0 * PAD - ICON - GAP - self.trailing_width()).max(40.0);
        match &self.title {
            Some(title) => {
                self.title_text
                    .shape(cx.text, title, Self::bold_style(), Some(wrap));
            }
            None => self.title_text.release(cx.text),
        }
        let message = self
            .text
            .shape(cx.text, &self.message, Self::text_style(), Some(wrap));
        let text_h = self.title_height() + message.height;
        Size::new(width, text_h.max(ACTION_H) + 2.0 * PAD)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let radius = cx.corner(Corner::Container, RADIUS, bounds.height);
        let tone = self.tone.color(cx);
        let surface = cx.color(Role::Surface);
        let border = Border {
            width: 1.0,
            color: tone.with_alpha(tone.a * 0.4),
        };
        cx.rounded_rect(bounds, radius, surface, Border::default());
        cx.rounded_rect(bounds, radius, self.tone.wash(cx, 0.12), border);

        let line = Self::text_style().line_height;
        let text_top = PAD + ((ACTION_H - line) / 2.0).max(0.0);
        let icon = Rect::new(PAD, text_top + (line - ICON) / 2.0, ICON, ICON);
        cx.rounded_rect(icon, ICON / 2.0, tone, Border::default());
        if let Some(layout) = self.icon_text.layout() {
            let ink = self.tone.on_solid(cx);
            cx.text(centered(icon, self.icon_text.size()), layout, ink);
        }

        let x = PAD + ICON + GAP;
        let ink = cx.color(Role::Text);
        if let Some(layout) = self.title_text.layout() {
            let size = self.title_text.size();
            cx.text(Rect::new(x, text_top, size.width, size.height), layout, ink);
        }
        if let Some(layout) = self.text.layout() {
            let size = self.text.size();
            let y = text_top + self.title_height();
            cx.text(Rect::new(x, y, size.width, size.height), layout, ink);
        }

        let (action, close) = self.parts(cx.size());
        let highlight = |part: Part, banner: &NotificationBanner| {
            let alpha = if banner.pressed == Some(part) {
                0.18
            } else if banner.over == Some(part) {
                0.1
            } else {
                0.0
            };
            ink.with_alpha(ink.a * alpha)
        };
        if let Some(rect) = action
            && let Some(layout) = self.action_text.layout()
        {
            let r = cx.corner(Corner::Control, 6.0, rect.height);
            let border = Border {
                width: 1.0,
                color: tone.with_alpha(tone.a * 0.5),
            };
            cx.rounded_rect(rect, r, highlight(Part::Action, self), border);
            let ink = self.tone.ink(cx);
            cx.text(centered(rect, self.action_text.size()), layout, ink);
        }
        if let Some(rect) = close {
            let r = cx.pill(rect.height);
            cx.rounded_rect(rect, r, highlight(Part::Close, self), Border::default());
            let muted = cx.color(Role::Muted);
            cross(cx, rect, 3.5, 1.5, muted);
        }
        if cx.is_focused() {
            focus_ring(cx, bounds, radius);
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            Event::PointerMove { pos } => {
                let over = self.part_at(cx.local(*pos), cx.size());
                if over != self.over {
                    self.over = over;
                    cx.request_paint();
                }
            }
            Event::HoverChanged(false) if self.over.is_some() => {
                self.over = None;
                cx.request_paint();
            }
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                self.pressed = self.part_at(cx.local(*pos), cx.size());
                if self.pressed.is_some() {
                    cx.capture_pointer();
                    cx.request_paint();
                    cx.set_handled();
                }
            }
            Event::PointerUp { pos, .. } => {
                if let Some(part) = self.pressed.take() {
                    cx.release_pointer();
                    cx.request_paint();
                    if self.part_at(cx.local(*pos), cx.size()) == Some(part) {
                        self.activate(cx, part);
                    }
                    cx.set_handled();
                }
            }
            Event::KeyDown {
                key: Key::Enter, ..
            }
            | Event::Activate
                if self.action.is_some() =>
            {
                self.activate(cx, Part::Action);
                cx.set_handled();
            }
            Event::KeyDown {
                key: Key::Escape, ..
            } if self.closable => {
                self.activate(cx, Part::Close);
                cx.set_handled();
            }
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        self.action.is_some() || self.closable
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(match self.tone {
            Tone::Warning | Tone::Critical => accesskit::Role::Alert,
            _ => accesskit::Role::Status,
        })
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        node.set_label(self.body());
        if self.action.is_some() {
            node.add_action(accesskit::Action::Click);
        }
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.text.release(text);
        self.title_text.release(text);
        self.action_text.release(text);
        self.icon_text.release(text);
    }
}

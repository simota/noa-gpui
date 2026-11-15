use noa_gpui_layout::style::{Position, Rect as LayoutRect, auto, length};
use noa_gpui_layout::{MeasureInput, Style};
use noa_gpui_scene::{Border, Point, Rect, Size};
use noa_gpui_text::{TextStore, TextStyle};

use super::tone::Tone;
use super::{CachedText, focus_ring};
use crate::event::{Event, Key, PointerButton};
use crate::theme::{Corner, Role};
use crate::widget::{EventCx, MeasureCx, PaintCx, UpdateCx, Widget};

const BADGE_H: f32 = 16.0;
const DOT: f32 = 8.0;
/// How far an attached badge sticks out past its parent's top-right corner.
const OVERHANG: f32 = 6.0;

#[derive(Debug, Clone, PartialEq, Eq)]
enum BadgeContent {
    Count(u32),
    Dot,
    Text(String),
}

/// A count, short text or dot, usually on the corner of an icon or button. Critical
/// tone by default; a count of zero hides it and it takes no space.
#[derive(Debug)]
pub struct Badge {
    content: BadgeContent,
    tone: Tone,
    max: u32,
    attached: bool,
    shaped: CachedText,
}

impl Badge {
    fn new(content: BadgeContent) -> Self {
        Self {
            content,
            tone: Tone::Critical,
            max: 99,
            attached: false,
            shaped: CachedText::default(),
        }
    }

    /// Hidden at zero.
    pub fn count(count: u32) -> Self {
        Self::new(BadgeContent::Count(count))
    }

    pub fn dot() -> Self {
        Self::new(BadgeContent::Dot)
    }

    pub fn text(text: impl Into<String>) -> Self {
        Self::new(BadgeContent::Text(text.into()))
    }

    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    /// The largest count shown as is; larger ones show as "`max`+".
    pub fn max(mut self, max: u32) -> Self {
        self.max = max;
        self
    }

    pub fn attached(mut self) -> Self {
        self.attached = true;
        self
    }

    pub fn set_count(&mut self, cx: &mut UpdateCx, count: u32) {
        let content = BadgeContent::Count(count);
        if self.content != content {
            self.content = content;
            cx.request_layout();
            cx.request_paint();
        }
    }

    /// The text drawn, or `None` for a dot or a hidden badge.
    pub fn label(&self) -> Option<String> {
        match &self.content {
            BadgeContent::Count(0) | BadgeContent::Dot => None,
            BadgeContent::Count(n) if *n > self.max => Some(format!("{}+", self.max)),
            BadgeContent::Count(n) => Some(n.to_string()),
            BadgeContent::Text(text) => Some(text.clone()),
        }
    }

    pub fn is_shown(&self) -> bool {
        self.content != BadgeContent::Count(0)
    }

    fn text_style() -> TextStyle {
        TextStyle::new(11.0).weight(600)
    }
}

impl Widget for Badge {
    fn style(&self) -> Style {
        let mut style = Style {
            flex_shrink: 0.0,
            ..Default::default()
        };
        if self.attached {
            style.position = Position::Absolute;
            style.inset = LayoutRect {
                left: auto(),
                right: length(-OVERHANG),
                top: length(-OVERHANG),
                bottom: auto(),
            };
        }
        style
    }

    fn measures(&self) -> bool {
        true
    }

    fn measure(&mut self, cx: &mut MeasureCx, _input: MeasureInput) -> Size {
        if !self.is_shown() {
            return Size::default();
        }
        match self.label() {
            Some(label) => {
                let text = self.shaped.shape(cx.text, &label, Self::text_style(), None);
                Size::new((text.width + 10.0).max(BADGE_H), BADGE_H)
            }
            None => Size::new(DOT, DOT),
        }
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        if !self.is_shown() {
            return;
        }
        let bounds = cx.bounds();
        let radius = cx.pill(bounds.height);
        if self.attached {
            // A ring of the background around it, so it reads as on top of the icon.
            let ring = cx.color(Role::Background);
            let grown = Rect::new(
                bounds.x - 1.5,
                bounds.y - 1.5,
                bounds.width + 3.0,
                bounds.height + 3.0,
            );
            cx.rounded_rect(grown, radius + 1.5, ring, Border::default());
        }
        let fill = self.tone.color(cx);
        match self.shaped.layout().filter(|_| self.label().is_some()) {
            Some(layout) => {
                let ink = self.tone.on_solid(cx);
                let label = (layout, self.shaped.size(), ink);
                pill(cx, bounds, radius, fill, Some(label));
            }
            None => pill(cx, bounds, radius, fill, None),
        }
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        self.is_shown().then_some(accesskit::Role::Status)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        if let Some(label) = self.label() {
            node.set_label(label);
        }
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.shaped.release(text);
    }
}

pub(crate) fn pill(
    cx: &mut PaintCx,
    rect: Rect,
    radius: f32,
    fill: noa_gpui_scene::Color,
    label: Option<(noa_gpui_scene::TextLayoutId, Size, noa_gpui_scene::Color)>,
) {
    cx.rounded_rect(rect, radius, fill, Border::default());
    if let Some((layout, size, ink)) = label {
        cx.text(centered(rect, size), layout, ink);
    }
}

pub(crate) fn centered(rect: Rect, size: Size) -> Rect {
    Rect::new(
        rect.x + (rect.width - size.width) / 2.0,
        rect.y + (rect.height - size.height) / 2.0,
        size.width,
        size.height,
    )
}

pub(crate) fn cross(
    cx: &mut PaintCx,
    rect: Rect,
    arm: f32,
    width: f32,
    color: noa_gpui_scene::Color,
) {
    let (x, y) = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    cx.line(
        Point::new(x - arm, y - arm),
        Point::new(x + arm, y + arm),
        width,
        color,
    );
    cx.line(
        Point::new(x - arm, y + arm),
        Point::new(x + arm, y - arm),
        width,
        color,
    );
}

const TAG_H: f32 = 20.0;
const TAG_PAD: f32 = 8.0;
const REMOVE_W: f32 = 16.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TagStyle {
    #[default]
    Pill,
    /// [`CellLook::OutlinedBadge`](super::CellLook) outside a table.
    Outlined,
    /// Matches [`CellLook::StrongOutlinedBadge`](super::CellLook).
    StrongOutlined,
}

/// Emitted (with the tag as the source) when a removable [`Tag`]'s × is clicked, or
/// Backspace / Delete is pressed while it has focus. The app removes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TagRemoved;

/// A small pill naming a status or category. With [`Tag::removable`] it takes keyboard
/// focus and emits [`TagRemoved`] on Backspace or Delete.
#[derive(Debug)]
pub struct Tag {
    text: String,
    tone: Tone,
    removable: bool,
    look: TagStyle,
    family: Option<String>,
    over_remove: bool,
    pressing: bool,
    shaped: CachedText,
}

impl Tag {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Neutral,
            removable: false,
            look: TagStyle::Pill,
            family: None,
            over_remove: false,
            pressing: false,
            shaped: CachedText::default(),
        }
    }

    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    pub fn style(mut self, style: TagStyle) -> Self {
        self.look = style;
        self
    }

    /// A font family name (see [`TextStore::font_family`]).
    pub fn font_family(mut self, family: impl Into<String>) -> Self {
        self.family = Some(family.into());
        self
    }

    pub fn removable(mut self, removable: bool) -> Self {
        self.removable = removable;
        self
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, cx: &mut UpdateCx, text: impl Into<String>) {
        let text = text.into();
        if text != self.text {
            self.text = text;
            cx.request_layout();
            cx.request_paint();
        }
    }

    pub fn set_tone(&mut self, cx: &mut UpdateCx, tone: Tone) {
        if tone != self.tone {
            self.tone = tone;
            cx.request_paint();
        }
    }

    /// The × in the tag's coordinates (`size` is the tag's size).
    fn remove_rect(&self, size: Size) -> Option<Rect> {
        self.removable.then(|| {
            Rect::new(
                size.width - REMOVE_W - 3.0,
                (size.height - REMOVE_W) / 2.0,
                REMOVE_W,
                REMOVE_W,
            )
        })
    }

    fn text_style(&self, text: &mut TextStore) -> TextStyle {
        let style = TextStyle::new(12.0).weight(500);
        match self.family.as_deref() {
            Some(family) => style.family(text.font_family(family)),
            None => style,
        }
    }
}

impl Widget for Tag {
    fn style(&self) -> Style {
        Style {
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn measures(&self) -> bool {
        true
    }

    fn measure(&mut self, cx: &mut MeasureCx, _input: MeasureInput) -> Size {
        let style = self.text_style(cx.text);
        let text = self.shaped.shape(cx.text, &self.text, style, None);
        let extra = if self.removable { REMOVE_W - 2.0 } else { 0.0 };
        Size::new(text.width + 2.0 * TAG_PAD + extra, TAG_H)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let tone = self.tone.color(cx);
        let (radius, fill, edge, ink) = match self.look {
            TagStyle::Pill => (
                cx.pill(bounds.height),
                self.tone.wash(cx, 0.16),
                tone.with_alpha(tone.a * 0.35),
                self.tone.ink(cx),
            ),
            TagStyle::Outlined | TagStyle::StrongOutlined => {
                let radius = cx.corner(Corner::Inner, 4.0, bounds.height);
                let fill = self.tone.wash(cx, 0.14);
                if self.look == TagStyle::StrongOutlined {
                    (radius, fill, tone, tone)
                } else {
                    (
                        radius,
                        fill,
                        tone.with_alpha(tone.a * 0.6),
                        self.tone.ink(cx),
                    )
                }
            }
        };
        let border = Border {
            width: 1.0,
            color: edge,
        };
        cx.rounded_rect(bounds, radius, fill, border);
        if let Some(layout) = self.shaped.layout() {
            let size = self.shaped.size();
            let r = Rect::new(
                TAG_PAD,
                (bounds.height - size.height) / 2.0,
                size.width,
                size.height,
            );
            cx.text(r, layout, ink);
        }
        if let Some(rect) = self.remove_rect(cx.size()) {
            if self.over_remove {
                let alpha = if self.pressing { 0.3 } else { 0.18 };
                let hover = ink.with_alpha(ink.a * alpha);
                cx.rounded_rect(rect, rect.height / 2.0, hover, Border::default());
            }
            cross(cx, rect, 3.0, 1.4, ink);
        }
        if cx.is_focused() {
            focus_ring(cx, bounds, radius);
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        if !self.removable {
            return;
        }
        let over = |cx: &EventCx, pos: Point, tag: &Tag| {
            tag.remove_rect(cx.size())
                .is_some_and(|r| r.contains(cx.local(pos)))
        };
        match event {
            Event::PointerMove { pos } => {
                let now_over = over(cx, *pos, self);
                if now_over != self.over_remove {
                    self.over_remove = now_over;
                    cx.request_paint();
                }
            }
            Event::HoverChanged(false) if self.over_remove && !self.pressing => {
                self.over_remove = false;
                cx.request_paint();
            }
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } if over(cx, *pos, self) => {
                self.pressing = true;
                cx.capture_pointer();
                cx.request_paint();
                cx.set_handled();
            }
            Event::PointerUp { pos, .. } if self.pressing => {
                self.pressing = false;
                cx.release_pointer();
                cx.request_paint();
                if over(cx, *pos, self) {
                    cx.emit(TagRemoved);
                }
                cx.set_handled();
            }
            Event::KeyDown {
                key: Key::Backspace | Key::Delete,
                ..
            } => {
                cx.emit(TagRemoved);
                cx.set_handled();
            }
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        self.removable
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Label)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        node.set_label(self.text.clone());
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.shaped.release(text);
    }
}

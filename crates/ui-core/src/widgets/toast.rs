use noa_gpui_layout::style::{AlignItems, FlexDirection, Size as LayoutSize, length};
use noa_gpui_layout::{MeasureInput, Style};
use noa_gpui_scene::{Border, Point, Rect, Size};
use noa_gpui_text::{TextStore, TextStyle};

use super::CachedText;
use super::appear::{Appear, shadow};
use super::badge::cross;
use crate::event::{Event, PointerButton};
use crate::overlay::{OverlayOptions, Placement, WindowCorner};
use crate::theme::{Corner, Role};
use crate::tree::{UiTree, WidgetId};
use crate::widget::{EventCx, MeasureCx, PaintCx, UpdateCx, Widget};

/// How long a toast stays by default, in seconds.
pub const TOAST_DURATION: f64 = 4.0;
pub const TOAST_WIDTH: f32 = 320.0;
const INSET: f32 = 16.0;
const GAP: f32 = 8.0;
const PAD_X: f32 = 14.0;
const PAD_Y: f32 = 11.0;
const CLOSE: f32 = 20.0;
const ACTION_PAD: f32 = 8.0;
const ACTION_H: f32 = 24.0;
const RADIUS: f32 = 10.0;
const FADE: f64 = 0.2;
/// While the pointer rests on a toast it stays; it looks again this often.
const HOVER_RECHECK: f64 = 1.0;

/// Emitted (with the toast as the source) when its action button is clicked. The toast
/// then hides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToastAction;

/// Emitted (with the toast as the source) when a toast starts hiding, for whatever
/// reason: its time ran out, its × or action was clicked, or the app dismissed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToastClosed;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Action,
    Close,
}

/// A short non-modal notice in a corner of the window.
///
/// Newer toasts stack nearer the corner. A toast stays while the pointer rests on it
/// and does not take keyboard focus; assistive technology announces it as an alert.
/// Emits [`ToastAction`] and [`ToastClosed`].
#[derive(Debug)]
pub struct Toast {
    message: String,
    action: Option<String>,
    duration: Option<f64>,

    hovered: bool,
    over: Option<Part>,
    pressed: Option<Part>,
    appear: Appear,

    message_text: CachedText,
    action_text: CachedText,
}

impl Toast {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            action: None,
            duration: Some(TOAST_DURATION),
            hovered: false,
            over: None,
            pressed: None,
            appear: Appear::new(FADE),
            message_text: CachedText::default(),
            action_text: CachedText::default(),
        }
    }

    /// A button after the message ("Undo", "View") that emits [`ToastAction`].
    pub fn action(mut self, title: impl Into<String>) -> Self {
        self.action = Some(title.into());
        self
    }

    /// How long it stays, in seconds.
    pub fn duration(mut self, seconds: f64) -> Self {
        self.duration = Some(seconds);
        self
    }

    /// Stay until the × is clicked or the app calls [`Toast::dismiss`].
    pub fn persistent(mut self) -> Self {
        self.duration = None;
        self
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn is_closing(&self) -> bool {
        self.appear.is_closing()
    }

    /// Add the toast to the stack in the bottom-right corner, opening the stack if
    /// needed, and start its timer. Returns the toast's id.
    pub fn show(self, tree: &mut UiTree) -> WidgetId {
        self.show_at(tree, WindowCorner::BottomRight)
    }

    pub fn show_at(self, tree: &mut UiTree, corner: WindowCorner) -> WidgetId {
        let existing = tree.overlays().find(|&id| {
            tree.get::<ToastStack>(id)
                .is_some_and(|s| s.corner == corner)
        });
        let stack = existing.unwrap_or_else(|| {
            tree.open_overlay(
                ToastStack { corner },
                Placement::Corner {
                    corner,
                    inset: INSET,
                },
                OverlayOptions::PASSIVE,
            )
        });
        let id = tree.insert(Some(stack), self);
        tree.update::<Toast, _>(id, |toast, cx| {
            cx.request_animation_frame();
            if let Some(seconds) = toast.duration {
                cx.request_timer(seconds);
            }
        });
        id
    }

    /// Start hiding (fading out, then removing itself). Emits nothing: the app asked.
    pub fn dismiss(&mut self, cx: &mut UpdateCx) {
        if self.appear.close(cx.now()) {
            cx.request_animation_frame();
        }
    }

    fn close(&mut self, cx: &mut EventCx) {
        if self.appear.close(cx.now()) {
            cx.emit(ToastClosed);
            cx.request_animation_frame();
        }
    }

    fn message_style() -> TextStyle {
        TextStyle::new(13.0)
    }

    fn action_style() -> TextStyle {
        TextStyle::new(13.0).weight(600)
    }

    /// The action button and the × in the toast's coordinates.
    fn parts(&self, size: Size) -> (Option<Rect>, Rect) {
        let close = Rect::new(
            size.width - PAD_X - CLOSE + 4.0,
            (size.height - CLOSE) / 2.0,
            CLOSE,
            CLOSE,
        );
        let action = self.action.as_ref().map(|_| {
            let w = self.action_text.size().width + 2.0 * ACTION_PAD;
            Rect::new(
                close.x - 6.0 - w,
                (size.height - ACTION_H) / 2.0,
                w,
                ACTION_H,
            )
        });
        (action, close)
    }

    fn part_at(&self, local: Point, size: Size) -> Option<Part> {
        let (action, close) = self.parts(size);
        if action.is_some_and(|r| r.contains(local)) {
            Some(Part::Action)
        } else if close.contains(local) {
            Some(Part::Close)
        } else {
            None
        }
    }
}

impl Widget for Toast {
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
        let action_w = match &self.action {
            Some(title) => {
                let size = self
                    .action_text
                    .shape(cx.text, title, Self::action_style(), None);
                size.width + 2.0 * ACTION_PAD + 6.0
            }
            None => 0.0,
        };
        let wrap = TOAST_WIDTH - 2.0 * PAD_X - CLOSE - 6.0 - action_w;
        let message =
            self.message_text
                .shape(cx.text, &self.message, Self::message_style(), Some(wrap));
        Size::new(TOAST_WIDTH, message.height.max(ACTION_H) + 2.0 * PAD_Y)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let radius = cx.corner(Corner::Container, RADIUS, bounds.height);
        shadow(cx, bounds, radius);
        let fill = cx.color(Role::Surface);
        let border = Border {
            width: 1.0,
            color: cx.color(Role::Outline),
        };
        cx.floating_surface(bounds, radius, fill, border);
        let ink = cx.color(Role::Text);
        if let Some(layout) = self.message_text.layout() {
            let size = self.message_text.size();
            let r = Rect::new(
                PAD_X,
                (bounds.height - size.height) / 2.0,
                size.width,
                size.height,
            );
            cx.text(r, layout, ink);
        }
        let (action, close) = self.parts(cx.size());
        let highlight = |part: Part, toast: &Toast| {
            let alpha = if toast.pressed == Some(part) {
                0.16
            } else if toast.over == Some(part) {
                0.08
            } else {
                0.0
            };
            ink.with_alpha(ink.a * alpha)
        };
        if let Some(rect) = action
            && let Some(layout) = self.action_text.layout()
        {
            let r = cx.corner(Corner::Inner, 6.0, rect.height);
            cx.rounded_rect(rect, r, highlight(Part::Action, self), Border::default());
            let size = self.action_text.size();
            let accent = cx.color(Role::Accent);
            cx.text(super::badge::centered(rect, size), layout, accent);
        }
        let r = cx.pill(close.height);
        cx.rounded_rect(close, r, highlight(Part::Close, self), Border::default());
        let muted = cx.color(Role::Muted);
        cross(cx, close, 3.5, 1.5, muted);
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            Event::AnimationFrame { now } => {
                if self.appear.frame(cx, *now) {
                    cx.remove_self();
                }
                cx.request_composite();
            }
            Event::Timer { .. } if !self.appear.is_closing() => {
                if self.hovered {
                    cx.request_timer(HOVER_RECHECK);
                } else {
                    self.close(cx);
                }
            }
            Event::HoverChanged(hovered) => {
                self.hovered = *hovered;
                if !hovered && self.over.take().is_some() {
                    cx.request_paint();
                }
            }
            Event::PointerMove { pos } => {
                let over = self.part_at(cx.local(*pos), cx.size());
                if over != self.over {
                    self.over = over;
                    cx.request_paint();
                }
            }
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                self.pressed = self.part_at(cx.local(*pos), cx.size());
                if self.pressed.is_some() {
                    cx.capture_pointer();
                    cx.request_paint();
                }
                cx.set_handled();
            }
            Event::PointerUp { pos, .. } => {
                if let Some(part) = self.pressed.take() {
                    cx.release_pointer();
                    cx.request_paint();
                    if self.part_at(cx.local(*pos), cx.size()) == Some(part) {
                        if part == Part::Action {
                            cx.emit(ToastAction);
                        }
                        self.close(cx);
                    }
                }
                cx.set_handled();
            }
            _ => {}
        }
    }

    fn opacity(&self) -> f32 {
        self.appear.value()
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Alert)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        node.set_label(self.message.clone());
        node.set_live(accesskit::Live::Polite);
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.message_text.release(text);
        self.action_text.release(text);
    }
}

/// The overlay holding the toasts of one corner, newest nearest the corner.
#[derive(Debug)]
pub(crate) struct ToastStack {
    corner: WindowCorner,
}

impl Widget for ToastStack {
    fn style(&self) -> Style {
        let top = matches!(self.corner, WindowCorner::TopLeft | WindowCorner::TopRight);
        Style {
            flex_direction: if top {
                FlexDirection::ColumnReverse
            } else {
                FlexDirection::Column
            },
            align_items: AlignItems::STRETCH,
            gap: LayoutSize {
                width: length(0.0),
                height: length(GAP),
            },
            ..Default::default()
        }
    }

    fn paint(&mut self, _cx: &mut PaintCx) {}
}

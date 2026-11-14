use std::any::Any;
use std::rc::Rc;

use ui_layout::{MeasureInput, Style};
use ui_scene::{
    Border, Color, Direction, DrawCommand, ImageId, Point, Rect, Shape, Size, TextLayoutId,
    VectorIcon,
};
use ui_text::TextStore;

use crate::clipboard::Clipboard;
use crate::event::{Cursor, DragData, Event, Modifiers};
use crate::invalidation::Dirty;
use crate::overlay::Placement;
use crate::shortcut::Shortcut;
use crate::styling::StylePatch;
use crate::theme::{Appearance, Corner, Design, Paint, Theme};
use crate::tree::WidgetId;
use crate::widgets::MenuItem;

/// How opaque the base of a floating panel or window is in [`Design::Noa`]: enough
/// of what is behind shows to tell it is glass, little enough to keep text legible.
pub const NOA_BASE_OPACITY: f32 = 0.72;

/// Widgets paint in their own coordinates (origin at the top-left). The tree caches the
/// result and calls [`Widget::paint`] again only after the widget asks for it or its size
/// changes.
pub trait Widget: Any {
    /// Selector name in stylesheets, without the Rust module path.
    fn style_type(&self) -> &'static str {
        last_path_segment(std::any::type_name::<Self>())
    }

    /// Whether stylesheet text properties (`color`, `font-size`, `font-weight`,
    /// `text-align`) inherit into this widget.
    fn inherits_style(&self) -> bool {
        true
    }

    /// Whether the widget draws the stylesheet's box (`background-color`, `border`,
    /// `border-radius`) itself. When `false`, the tree draws that box below the
    /// widget's own drawing.
    fn paints_style_box(&self) -> bool {
        false
    }

    /// Layout style. Read when the widget is inserted and after it requests layout.
    fn style(&self) -> Style;

    fn measures(&self) -> bool {
        false
    }

    fn measure(&mut self, _cx: &mut MeasureCx, _input: MeasureInput) -> Size {
        Size::default()
    }

    fn paint(&mut self, cx: &mut PaintCx);

    fn paint_over(&mut self, _cx: &mut PaintCx) {}

    fn event(&mut self, _cx: &mut EventCx, _event: &Event) {}

    fn focusable(&self) -> bool {
        false
    }

    /// The widget gets [`Event::Activate`] when this key is pressed and the focused widget
    /// does not handle it.
    fn key_equivalent(&self) -> Option<Shortcut> {
        None
    }

    /// Matches arrive as [`Event::KeyEquivalent`]. Hidden and disabled widgets are skipped.
    fn key_equivalents(&self) -> &[Shortcut] {
        &[]
    }

    /// Shown on a right click, a Ctrl-click on a Mac, or the context menu key or Shift+F10
    /// while focused. `local` is in widget coordinates. The innermost widget that returns
    /// a menu gets [`Event::MenuClosed`].
    fn context_menu(&self, _local: Point) -> Option<Vec<MenuItem>> {
        None
    }

    /// Arrow keys move focus among the children, and Tab treats the group as one stop,
    /// landing on the item focused last.
    fn focus_group(&self) -> bool {
        false
    }

    /// A hidden child takes no space and is not drawn, hit or focused. Read when the
    /// widget's layout is requested.
    fn hides_child(&self, _index: usize) -> bool {
        false
    }

    /// `local` is in widget coordinates. `true` sends the pointer to this widget instead of
    /// the children under it, as for scroll bars drawn over content.
    fn claims_pointer(&self, _local: Point, _size: Size) -> bool {
        false
    }

    fn clips_children(&self) -> bool {
        false
    }

    /// How far children are shifted up/left (a scroll position).
    fn child_offset(&self) -> Point {
        Point::default()
    }

    /// `rect` is in the coordinates of the unscrolled content. Returns whether the position
    /// changed. The tree calls this on every ancestor of a widget that keyboard focus moved
    /// to.
    fn reveal(
        &mut self,
        _cx: &mut UpdateCx,
        _size: Size,
        _content_size: Size,
        _rect: Rect,
    ) -> bool {
        false
    }

    /// 0 to 1, applied when the frame is composited. Ask for a composite after changing it.
    fn opacity(&self) -> f32 {
        1.0
    }

    /// `local` is in widget coordinates. Also given to assistive technology as the widget's
    /// description.
    fn tooltip(&self, _local: Point) -> Option<String> {
        None
    }

    /// `local` is in widget coordinates; `None` uses the parent's. While a widget captures
    /// the pointer only it is asked, so a divider keeps its resize cursor when the drag
    /// leaves it.
    fn cursor(&self, _local: Point, _size: Size) -> Option<Cursor> {
        None
    }

    /// Whether the widget is a focus scope (an inner window): while focus is inside it,
    /// Tab cycles through its content only, and key equivalents are looked up in it
    /// before the rest of the tree (other scopes are skipped).
    fn focus_scope(&self) -> bool {
        false
    }

    fn observes_presses(&self) -> bool {
        false
    }

    /// Whether the widget, as a direct child of a focus scope, is modal within it: the
    /// scope's other content takes no pointer or keyboard input while it is there (a
    /// sheet inside an inner window). It should be the scope's last child.
    fn modal(&self) -> bool {
        false
    }

    /// `rect` is in window coordinates. Return `true` to be painted again (a popover's
    /// arrow points at its anchor from whichever side it ended up on).
    fn overlay_placed(&mut self, _rect: Rect) -> bool {
        false
    }

    /// In widget coordinates. While the widget is focused the platform places the IME
    /// candidate window next to it.
    fn text_input_area(&self) -> Option<Rect> {
        None
    }

    /// `None` leaves the widget out of the accessibility tree; its children are still
    /// included.
    fn accessibility_role(&self) -> Option<accesskit::Role> {
        None
    }

    /// Bounds, focusability, the disabled state and children are set by the tree.
    fn accessibility(&self, _node: &mut accesskit::Node) {}

    /// Bounds are in widget coordinates. A Click on item `i` arrives as
    /// [`Event::ActivateItem`].
    fn accessibility_items(&self) -> Vec<accesskit::Node> {
        Vec::new()
    }

    /// Whether the widget's accessibility label names the unnamed controls inside it, as an
    /// HTML `<label>` does.
    fn labels_content(&self) -> bool {
        false
    }

    /// Such a widget has two children: the content, then the card, which
    /// [`Widget::hides_child`] should hide. After a short delay the tree lifts the card
    /// into an overlay (see [`crate::widgets::HoverCard`]).
    fn hover_card(&self) -> Option<crate::overlay::Edge> {
        None
    }

    /// Called before the widget is dropped, to release text layouts and similar resources.
    fn removed(&mut self, _text: &mut TextStore) {}

    /// What the user arranged in this widget by hand (column widths, a divider's
    /// position, the open tab), as text. Asked only for widgets named with
    /// [`crate::UiTree::set_autosave_name`]. `None`: nothing to keep.
    fn save_state(&self) -> Option<String> {
        None
    }

    /// `state` may come from an earlier version of the application: ignore what no longer
    /// fits. No messages are emitted.
    fn restore_state(&mut self, _cx: &mut UpdateCx, _state: &str) {}
}

pub struct MeasureCx<'a> {
    pub text: &'a mut TextStore,
    pub(crate) style: &'a StylePatch,
}

impl MeasureCx<'_> {
    pub fn styling(&self) -> &StylePatch {
        self.style
    }
}

pub struct PaintCx<'a> {
    pub(crate) size: Size,
    pub(crate) content_size: Size,
    pub(crate) focused: bool,
    pub(crate) hovered: bool,
    pub(crate) disabled: bool,
    pub(crate) theme: &'a Theme,
    pub(crate) style: &'a StylePatch,
    pub(crate) commands: &'a mut Vec<DrawCommand>,
    pub(crate) text: &'a mut TextStore,
}

impl PaintCx<'_> {
    pub fn size(&self) -> Size {
        self.size
    }

    /// Size of the laid-out content (larger than `size` when it overflows).
    pub fn content_size(&self) -> Size {
        self.content_size
    }

    pub fn bounds(&self) -> Rect {
        Rect::new(0.0, 0.0, self.size.width, self.size.height)
    }

    pub fn is_focused(&self) -> bool {
        self.focused
    }

    pub fn is_hovered(&self) -> bool {
        self.hovered
    }

    /// Whether the widget or an ancestor is disabled (see [`crate::UiTree::set_enabled`]).
    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    pub fn theme(&self) -> &Theme {
        self.theme
    }

    pub fn styling(&self) -> &StylePatch {
        self.style
    }

    pub fn text_store(&mut self) -> &mut TextStore {
        self.text
    }

    /// Resolve a color against the theme, dimmed when the widget is disabled.
    pub fn color(&self, paint: impl Into<Paint>) -> Color {
        let color = self.theme.resolve(paint.into());
        if self.disabled {
            color.with_alpha(color.a * self.theme.disabled_opacity)
        } else {
            color
        }
    }

    pub fn corner(&self, corner: Corner, radius: f32, height: f32) -> f32 {
        self.theme.corner(corner, radius, height)
    }

    /// Half of `height` (a pill), or square in Metro (see [`Theme::pill`]).
    pub fn pill(&self, height: f32) -> f32 {
        self.theme.pill(height)
    }

    pub fn bezel(&mut self, rect: Rect, radius: f32, fill: Color, border: Border) {
        self.rounded_rect(rect, radius, fill, border);
        if fill.a > 0.0 {
            self.sheen(rect, radius);
        }
    }

    /// The background of a menu, popover, dialog or window drawn over other content:
    /// opaque in Big Sur, Aero and Metro, smoked glass in Noa (the content behind,
    /// blurred, under her translucent base), a blurred material in Flat + Vibrancy and
    /// Fluent 2 (Acrylic), glass in Liquid Glass.
    pub fn floating_surface(&mut self, rect: Rect, radius: f32, fill: Color, border: Border) {
        match self.theme.design {
            Design::BigSur | Design::Aero | Design::Metro => {
                self.rounded_rect(rect, radius, fill, border);
            }
            Design::Noa => {
                self.smoked_glass(rect, radius);
                self.rounded_rect(rect, radius, fill, border);
            }
            Design::Fluent2 => {
                self.backdrop(rect, radius, 30.0, fill.with_alpha(fill.a * 0.86));
                self.rounded_rect(rect, radius, Color::TRANSPARENT, border);
            }
            Design::FlatVibrancy => {
                self.backdrop(rect, radius, 30.0, fill.with_alpha(fill.a * 0.82));
                self.rounded_rect(rect, radius, Color::TRANSPARENT, border);
            }
            Design::LiquidGlass => {
                let tint = match self.theme.appearance {
                    Appearance::Light => Color::WHITE.with_alpha(0.42),
                    Appearance::Dark => Color::hex(0x1e2027).with_alpha(0.55),
                };
                self.backdrop(rect, radius, 14.0, tint);
                self.rounded_rect(rect, radius, Color::TRANSPARENT, border);
                self.sheen(rect, radius);
            }
        }
    }

    /// Noa's translucent base: what is below `rect`, blurred, under the theme's
    /// background at [`NOA_BASE_OPACITY`]. Panels draw their surface wash over it.
    pub(crate) fn smoked_glass(&mut self, rect: Rect, radius: f32) {
        let base = self.theme.background.with_alpha(NOA_BASE_OPACITY);
        self.backdrop(rect, radius, 20.0, base);
    }

    /// Light fading down from the top edge and a bright inner rim: Liquid Glass's glass
    /// and Aero's gloss. Nothing in the other designs.
    pub fn sheen(&mut self, rect: Rect, radius: f32) {
        let dark = self.theme.appearance == Appearance::Dark;
        let (sheen, rim) = match (self.theme.design, dark) {
            (Design::LiquidGlass, false) => (0.38, 0.7),
            (Design::LiquidGlass, true) => (0.1, 0.2),
            (Design::Aero, false) => (0.7, 0.55),
            (Design::Aero, true) => (0.16, 0.14),
            _ => return,
        };
        let top = self.color(Color::WHITE.with_alpha(sheen));
        let rim = self.color(Color::WHITE.with_alpha(rim));
        self.gradient(rect, radius, top, Color::WHITE.with_alpha(0.0), true);
        self.rounded_rect(
            rect,
            radius,
            Color::TRANSPARENT,
            Border {
                width: 1.0,
                color: rim,
            },
        );
    }

    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        self.rounded_rect(rect, 0.0, color, Border::default());
    }

    pub fn rounded_rect(&mut self, rect: Rect, radius: f32, color: Color, border: Border) {
        self.commands.push(DrawCommand::Quad {
            rect,
            radius: [radius; 4],
            color,
            border,
        });
    }

    /// Clip what follows to `rect` (widget coordinates) until [`PaintCx::pop_clip`].
    /// Clips nest: the visible area is the intersection.
    pub fn push_clip(&mut self, rect: Rect) {
        self.commands.push(DrawCommand::PushClip { rect });
    }

    pub fn pop_clip(&mut self) {
        self.commands.push(DrawCommand::PopClip);
    }

    pub fn line(&mut self, from: Point, to: Point, width: f32, color: Color) {
        self.commands.push(DrawCommand::Shape {
            shape: Shape::Line { from, to, width },
            color,
        });
    }

    pub fn triangle(&mut self, rect: Rect, direction: Direction, color: Color) {
        self.commands.push(DrawCommand::Shape {
            shape: Shape::Triangle { rect, direction },
            color,
        });
    }

    /// `blur` is in pixels. Without blur this is a plain tinted fill, which skips the
    /// offscreen passes.
    pub fn backdrop(&mut self, rect: Rect, radius: f32, blur: f32, tint: Color) {
        if blur <= 0.0 {
            self.rounded_rect(rect, radius, tint, Border::default());
            return;
        }
        self.commands.push(DrawCommand::Backdrop {
            rect,
            radius,
            blur,
            tint,
        });
    }

    /// A five-pointed star filling `rect`; `outline` > 0 draws only its outline.
    pub fn star(&mut self, rect: Rect, outline: f32, color: Color) {
        self.commands.push(DrawCommand::Shape {
            shape: Shape::Star { rect, outline },
            color,
        });
    }

    /// A round-capped arc around `center` from 12 o'clock clockwise over `sweep` of a
    /// turn (`0..=1`).
    pub fn arc(&mut self, center: Point, radius: f32, width: f32, sweep: f32, color: Color) {
        self.commands.push(DrawCommand::Shape {
            shape: Shape::Arc {
                center,
                radius,
                width,
                sweep,
            },
            color,
        });
    }

    /// `rect` filled with a linear gradient from `start` (left, or top when `vertical`)
    /// to `end`, with rounded corners.
    pub fn gradient(&mut self, rect: Rect, radius: f32, start: Color, end: Color, vertical: bool) {
        self.commands.push(DrawCommand::Shape {
            shape: Shape::Gradient {
                rect,
                radius,
                end,
                vertical,
            },
            color: start,
        });
    }

    pub fn chevron(&mut self, rect: Rect, direction: Direction, width: f32, color: Color) {
        let (l, t, r, b) = (rect.x, rect.y, rect.right(), rect.bottom());
        let (cx, cy) = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
        let (a, tip, c) = match direction {
            Direction::Down => (Point::new(l, t), Point::new(cx, b), Point::new(r, t)),
            Direction::Up => (Point::new(l, b), Point::new(cx, t), Point::new(r, b)),
            Direction::Right => (Point::new(l, t), Point::new(r, cy), Point::new(l, b)),
            Direction::Left => (Point::new(r, t), Point::new(l, cy), Point::new(r, b)),
        };
        self.line(a, tip, width, color);
        self.line(tip, c, width, color);
    }

    pub fn checkmark(&mut self, rect: Rect, width: f32, color: Color) {
        let p = |fx: f32, fy: f32| Point::new(rect.x + rect.width * fx, rect.y + rect.height * fy);
        self.line(p(0.1, 0.55), p(0.4, 0.85), width, color);
        self.line(p(0.4, 0.85), p(0.92, 0.18), width, color);
    }

    pub fn image(&mut self, rect: Rect, image: ImageId) {
        self.commands.push(DrawCommand::Image { rect, image });
    }

    pub fn icon(&mut self, rect: Rect, icon: &VectorIcon, color: Color) {
        self.commands.push(DrawCommand::Icon {
            rect,
            icon: icon.clone(),
            color,
        });
    }

    pub fn text(&mut self, rect: Rect, layout: TextLayoutId, color: Color) {
        self.commands.push(DrawCommand::Text {
            rect,
            layout,
            color,
        });
    }
}

/// Passed to [`Widget::event`]. Requests are applied after the handler returns.
pub struct EventCx<'a> {
    pub(crate) id: WidgetId,
    pub(crate) origin: Point,
    pub(crate) size: Size,
    pub(crate) content_size: Size,
    pub(crate) focused: bool,
    pub(crate) hovered: bool,
    pub(crate) captured: bool,
    pub(crate) now: f64,
    pub(crate) reduce_motion: bool,
    pub(crate) modifiers: Modifiers,
    pub(crate) text: &'a mut TextStore,
    pub(crate) clipboard: &'a mut dyn Clipboard,
    pub(crate) requests: Requests,
}

#[derive(Default)]
pub(crate) struct Requests {
    pub dirty: Dirty,
    pub handled: bool,
    pub focus: bool,
    pub capture: Option<bool>,
    pub close_children: bool,
    pub animation: bool,
    pub drag: Option<DragData>,
    pub accept_drop: bool,
    pub menu: Option<MenuRequest>,
    pub close_menu: Option<Option<usize>>,
    pub popover: Option<PopoverRequest>,
    pub popup: Option<PopoverRequest>,
    pub to_popup: Vec<Rc<dyn Any>>,
    pub close_popover: bool,
    pub remove: bool,
    pub timer: Option<f64>,
    pub move_overlay: Option<Point>,
    pub to_owner: Vec<Rc<dyn Any>>,
    pub messages: Vec<Box<dyn Any>>,
}

impl EventCx<'_> {
    pub fn id(&self) -> WidgetId {
        self.id
    }

    pub fn size(&self) -> Size {
        self.size
    }

    /// Size of the laid-out content (larger than `size` when it overflows).
    pub fn content_size(&self) -> Size {
        self.content_size
    }

    pub fn local(&self, pos: Point) -> Point {
        Point::new(pos.x - self.origin.x, pos.y - self.origin.y)
    }

    /// `pos` is in window coordinates.
    pub fn contains(&self, pos: Point) -> bool {
        let p = self.local(pos);
        Rect::new(0.0, 0.0, self.size.width, self.size.height).contains(p)
    }

    pub fn is_focused(&self) -> bool {
        self.focused
    }

    pub fn is_hovered(&self) -> bool {
        self.hovered
    }

    pub fn has_capture(&self) -> bool {
        self.captured
    }

    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    /// Seconds, for starting animations.
    pub fn now(&self) -> f64 {
        self.now
    }

    pub fn reduces_motion(&self) -> bool {
        self.reduce_motion
    }

    /// `seconds`, or 0 when the user asked for less motion: pass it to
    /// [`crate::animation::Transition::animate_to`] so the transition jumps to its end.
    pub fn motion(&self, seconds: f64) -> f64 {
        if self.reduce_motion { 0.0 } else { seconds }
    }

    /// Receive one [`Event::AnimationFrame`] on the next frame. Ask again from that
    /// event while the animation is running.
    pub fn request_animation_frame(&mut self) {
        self.requests.animation = true;
    }

    /// No frames are drawn in between. A widget has one timer at a time: asking again
    /// replaces it.
    pub fn request_timer(&mut self, delay: f64) {
        self.requests.timer = Some(self.now + delay.max(0.0));
    }

    /// Takes effect once the handler returns.
    pub fn remove_self(&mut self) {
        self.requests.remove = true;
    }

    pub fn text_store(&mut self) -> &mut TextStore {
        self.text
    }

    pub fn clipboard(&mut self) -> &mut dyn Clipboard {
        &mut *self.clipboard
    }

    pub fn set_handled(&mut self) {
        self.requests.handled = true;
    }

    pub fn request_layout(&mut self) {
        self.requests.dirty |= Dirty::LAYOUT;
    }

    pub fn request_paint(&mut self) {
        self.requests.dirty |= Dirty::PAINT;
    }

    /// Children moved without changing (for example, scrolling).
    pub fn request_composite(&mut self) {
        self.requests.dirty |= Dirty::COMPOSITE;
    }

    pub fn request_focus(&mut self) {
        self.requests.focus = true;
    }

    /// Receive pointer events even when the pointer leaves the widget.
    pub fn capture_pointer(&mut self) {
        self.requests.capture = Some(true);
    }

    pub fn release_pointer(&mut self) {
        self.requests.capture = Some(false);
    }

    /// In window coordinates.
    pub fn window_rect(&self) -> Rect {
        Rect::new(
            self.origin.x,
            self.origin.y,
            self.size.width,
            self.size.height,
        )
    }

    /// When it closes, this widget gets [`Event::MenuClosed`] with the chosen index.
    pub fn open_menu(
        &mut self,
        items: Vec<MenuItem>,
        placement: Placement,
        min_width: f32,
        checked: Option<usize>,
    ) {
        self.requests.menu = Some(MenuRequest {
            items,
            placement,
            min_width,
            checked,
            highlight: checked,
            submenu: false,
            bar: false,
        });
    }

    pub(crate) fn open_menu_request(&mut self, request: MenuRequest) {
        self.requests.menu = Some(request);
    }

    /// Close the menus and popovers this widget opened, and any they opened in turn
    /// (before a menu opened by this event, if any). The widget gets
    /// [`Event::MenuClosed`] with no choice, or [`Event::PopoverClosed`].
    pub fn close_child_menus(&mut self) {
        self.requests.close_children = true;
    }

    pub fn close_menu(&mut self, chosen: Option<usize>) {
        self.requests.close_menu = Some(chosen);
    }

    /// The content gets keyboard focus; a press outside or Escape closes it. This widget
    /// then gets [`Event::PopoverClosed`] and focus back.
    pub fn open_popover(&mut self, content: impl Widget, placement: Placement) {
        self.requests.popover = Some(PopoverRequest {
            content: Box::new(content),
            placement,
        });
    }

    /// Leaves the keyboard focus with this widget. A press outside, which still reaches
    /// what is under the pointer, or an unhandled Escape closes it; this widget then gets
    /// [`Event::PopoverClosed`].
    pub fn open_popup(&mut self, content: impl Widget, placement: Placement) {
        self.requests.popup = Some(PopoverRequest {
            content: Box::new(content),
            placement,
        });
    }

    /// Send `message` to the popup or popover this widget opened; its root gets it as
    /// [`Event::FromOwner`].
    pub fn notify_popup<T: Any>(&mut self, message: T) {
        self.requests.to_popup.push(Rc::new(message));
    }

    pub fn close_popover(&mut self) {
        self.requests.close_popover = true;
    }

    /// Close the overlay this widget is in (an alert answering, a sheet that finished
    /// sliding out). A widget that opened it as a popover is told as with
    /// [`EventCx::close_popover`]; otherwise focus returns to where it was before a modal
    /// overlay opened. Outside an overlay, a widget that is [`Widget::modal`] within a
    /// focus scope is removed, and focus goes to the scope.
    pub fn close_overlay(&mut self) {
        self.requests.close_popover = true;
    }

    /// It stays inside the window.
    pub fn move_overlay(&mut self, delta: Point) {
        let moved = self.requests.move_overlay.get_or_insert_default();
        moved.x += delta.x;
        moved.y += delta.y;
    }

    /// Send `message` to the widget that opened the popover this widget is in; it
    /// arrives as [`Event::FromPopover`].
    pub fn notify_owner<T: Any>(&mut self, message: T) {
        self.requests.to_owner.push(Rc::new(message));
    }

    /// Start dragging `data` (call it from a pointer move while the button is down).
    /// Until the button is released, widgets under the pointer get
    /// [`Event::DragOver`]; this widget gets [`Event::DragEnd`] at the end.
    pub fn start_drag(&mut self, data: DragData) {
        self.requests.drag = Some(data);
    }

    /// Answer an [`Event::DragOver`]: this widget takes the data if it is dropped. Also
    /// stops the event from bubbling.
    pub fn accept_drop(&mut self) {
        self.requests.accept_drop = true;
        self.requests.handled = true;
    }

    pub fn emit<T: Any>(&mut self, message: T) {
        self.requests.messages.push(Box::new(message));
    }
}

pub(crate) struct MenuRequest {
    pub items: Vec<MenuItem>,
    pub placement: Placement,
    pub min_width: f32,
    pub checked: Option<usize>,
    pub highlight: Option<usize>,
    /// Opened by another menu: Left closes it, and Escape closes the whole chain.
    pub submenu: bool,
    /// Opened by a menu bar: Left / Right (where they do nothing else) ask the bar
    /// for the neighboring menu.
    pub bar: bool,
}

pub(crate) struct PopoverRequest {
    pub content: Box<dyn Widget>,
    pub placement: Placement,
}

#[derive(Default)]
pub struct UpdateCx {
    pub(crate) dirty: Dirty,
    pub(crate) animation: bool,
    pub(crate) timer: Option<f64>,
    pub(crate) now: f64,
    pub(crate) reduce_motion: bool,
}

impl UpdateCx {
    pub fn now(&self) -> f64 {
        self.now
    }

    pub fn reduces_motion(&self) -> bool {
        self.reduce_motion
    }

    pub fn motion(&self, seconds: f64) -> f64 {
        if self.reduce_motion { 0.0 } else { seconds }
    }

    pub fn request_animation_frame(&mut self) {
        self.animation = true;
    }

    pub fn request_timer(&mut self, delay: f64) {
        self.timer = Some(self.now + delay.max(0.0));
    }

    pub fn request_layout(&mut self) {
        self.dirty |= Dirty::LAYOUT;
    }

    pub fn request_paint(&mut self) {
        self.dirty |= Dirty::PAINT;
    }

    pub fn request_composite(&mut self) {
        self.dirty |= Dirty::COMPOSITE;
    }
}

/// What follows the last `::` of `name` (all of `name` without one). Called once per
/// stylesheet rule for each widget resolved, so it scans bytes instead of setting up a
/// substring search.
fn last_path_segment(name: &str) -> &str {
    match name.as_bytes().windows(2).rposition(|pair| pair == b"::") {
        Some(at) => &name[at + 2..],
        None => name,
    }
}

#[cfg(test)]
mod tests {
    use super::last_path_segment;

    #[test]
    fn the_last_path_segment_is_what_rsplit_gives() {
        for name in [
            "ui_core::widgets::label::Label",
            "Label",
            "",
            "::",
            ":::",
            "a::",
            "::b",
            "a:b::c",
            "app::Wrapper<ui_core::widgets::label::Label>",
            "app::Pair<(u8, alloc::string::String)>",
            "[ui_core::widgets::button::Button; 2]",
        ] {
            assert_eq!(
                last_path_segment(name),
                name.rsplit("::").next().unwrap(),
                "{name}"
            );
        }
    }
}

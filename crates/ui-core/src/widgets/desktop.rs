//! The windows are ordinary children of the [`Desktop`], positioned absolutely; their
//! order among its children is their stacking order. A window draws its own frame and
//! reports what the user does with it as [`WindowGesture`]s. The [`WindowManager`],
//! which the application keeps and passes messages to, turns those into positions,
//! stacking, snapping, minimizing and closing, because that needs the whole tree and a
//! widget only reaches itself.

use noa_gpui_layout::Style;
use noa_gpui_layout::style::{
    FlexDirection, LengthPercentageAuto, Position, Rect as LayoutRect, Size as LayoutSize, auto,
    length, percent,
};
use noa_gpui_scene::{Border, Color, Point, Rect, Size};
use noa_gpui_text::{TextStore, TextStyle};

use super::appear::shadow;
use super::resize::resize_rect;
use super::{Alert, CachedText, ResizeEdges, Sheet};
use crate::event::{Cursor, Event, Message, PointerButton};
use crate::shortcut::Shortcut;
use crate::theme::{Corner, Design, Role};
use crate::tree::{Subtree, UiTree, WidgetId};
use crate::view_state;
use crate::widget::{EventCx, PaintCx, UpdateCx, Widget};

pub const WINDOW_TITLE_BAR: f32 = 26.0;
pub const DESKTOP_DOCK: f32 = 30.0;
pub const SNAP_DISTANCE: f32 = 8.0;
pub const WINDOW_MIN_SIZE: Size = Size::new(160.0, 80.0);

/// Width of the band along a window's edges that resizes it.
const EDGE: f32 = 6.0;
const RADIUS: f32 = 9.0;
const BUTTON: f32 = 12.0;
/// Width of a Windows caption button (minimize, maximize, close).
const CAPTION: f32 = 34.0;
/// The block cursor in the title bar of a [`Design::Noa`] window.
const CURSOR: Size = Size::new(7.0, 13.0);
/// Two presses on the title bar this close together (seconds) zoom the window.
const DOUBLE_CLICK: f64 = 0.4;
/// How much of a window must stay reachable when it is dragged past an edge.
const KEEP_VISIBLE: f32 = 48.0;
/// How far outside the desktop a window must be dropped to be torn off.
const TEAR_OFF: f32 = 24.0;

/// Pass it to [`WindowManager::handle`]. Offsets are from where the gesture started.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindowGesture {
    Pressed,
    /// The title bar is dragged: the pointer is at `pos` (window coordinates), `delta`
    /// from where it was pressed.
    Drag {
        delta: Point,
        pos: Point,
    },
    DragEnd {
        pos: Point,
    },
    Resize {
        edges: ResizeEdges,
        delta: Point,
    },
    ResizeEnd,
    Close,
    Minimize,
    Zoom,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindowPlacement {
    /// Desktop coordinates.
    Frame(Rect),
    /// The whole desktop above the dock (follows the desktop's size).
    Maximized,
    LeftHalf,
    RightHalf,
}

/// A window inside a [`Desktop`], controlled through a [`WindowManager`]. It is a focus
/// scope: Tab stays inside it, and its default and cancel buttons answer only while it
/// holds the focus.
#[derive(Debug)]
pub struct InnerWindow {
    title: String,
    placement: WindowPlacement,
    /// The latest [`WindowPlacement::Frame`]: where the window goes back to from
    /// maximized or a half.
    last_frame: Rect,
    active: bool,
    title_text: CachedText,
    gesture: Option<Gesture>,
    pressed_button: Option<usize>,
    button_hovered: bool,
    last_title_press: f64,
    /// The design and width of the last paint, which place the title bar buttons.
    chrome: Design,
    width: f32,
}

#[derive(Debug, Clone, Copy)]
enum Gesture {
    Drag { from: Point },
    Resize { edges: ResizeEdges, from: Point },
}

impl InnerWindow {
    pub fn new(title: impl Into<String>, frame: Rect) -> Self {
        Self {
            title: title.into(),
            placement: WindowPlacement::Frame(frame),
            last_frame: frame,
            active: false,
            title_text: CachedText::default(),
            gesture: None,
            pressed_button: None,
            button_hovered: false,
            last_title_press: f64::MIN,
            chrome: Design::default(),
            width: 0.0,
        }
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn placement(&self) -> WindowPlacement {
        self.placement
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    /// The latest frame the window had as a [`WindowPlacement::Frame`] (its frame now,
    /// when it has one).
    pub fn last_frame(&self) -> Rect {
        self.last_frame
    }

    pub fn set_placement(&mut self, cx: &mut UpdateCx, placement: WindowPlacement) {
        if placement != self.placement {
            self.placement = placement;
            if let WindowPlacement::Frame(frame) = placement {
                self.last_frame = frame;
            }
            cx.request_layout();
            cx.request_paint();
        }
    }

    pub fn set_active(&mut self, cx: &mut UpdateCx, active: bool) {
        if active != self.active {
            self.active = active;
            cx.request_paint();
        }
    }

    fn fills(&self) -> bool {
        !matches!(self.placement, WindowPlacement::Frame(_))
    }

    /// The close, minimize and zoom buttons: at the left in the macOS designs, at the
    /// right (minimize, maximize, close) in the Windows ones.
    fn button_rect(&self, i: usize) -> Rect {
        if self.chrome.windows_chrome() {
            let slot = [0.0, 2.0, 1.0][i];
            return Rect::new(
                self.width - 1.0 - CAPTION * (slot + 1.0),
                1.0,
                CAPTION,
                WINDOW_TITLE_BAR - 1.0,
            );
        }
        Rect::new(
            10.0 + i as f32 * (BUTTON + 7.0),
            (WINDOW_TITLE_BAR - BUTTON) / 2.0,
            BUTTON,
            BUTTON,
        )
    }

    fn cursor_rect(&self) -> Rect {
        Rect::new(
            self.width - 10.0 - CURSOR.width,
            (WINDOW_TITLE_BAR - CURSOR.height) / 2.0,
            CURSOR.width,
            CURSOR.height,
        )
    }

    fn button_at(&self, p: Point) -> Option<usize> {
        let slop = if self.chrome.windows_chrome() {
            0.0
        } else {
            3.0
        };
        (0..3).find(|&i| {
            let r = self.button_rect(i);
            Rect::new(
                r.x - slop,
                r.y - slop,
                r.width + 2.0 * slop,
                r.height + 2.0 * slop,
            )
            .contains(p)
        })
    }

    fn paint_caption_buttons(&self, cx: &mut PaintCx, ink: Color) {
        let aero = self.chrome == Design::Aero;
        for i in 0..3 {
            let r = self.button_rect(i);
            let pressed = self.pressed_button == Some(i) && self.button_hovered;
            let red = Color::hex(if aero { 0xc75050 } else { 0xc42b1c });
            let mut glyph = ink;
            if aero {
                let face = Rect::new(r.x + 1.0, r.y + 2.0, r.width - 2.0, r.height - 6.0);
                let mut fill = if i == 0 {
                    red
                } else {
                    cx.color(Role::Surface).with_alpha(0.55)
                };
                if pressed {
                    fill = super::pressed_tint(cx, fill);
                }
                let border = Border {
                    width: 1.0,
                    color: cx.color(Role::Outline),
                };
                let radius = cx.corner(Corner::Control, 3.0, face.height);
                cx.bezel(face, radius, fill, border);
                if i == 0 {
                    glyph = Color::WHITE;
                }
            } else if pressed {
                let fill = if i == 0 {
                    red
                } else {
                    super::pressed_tint(cx, cx.color(Role::SurfaceAlt))
                };
                cx.fill_rect(r, fill);
                if i == 0 {
                    glyph = Color::WHITE;
                }
            }
            let (mx, my) = (r.x + r.width / 2.0, r.y + r.height / 2.0);
            let half = 4.5;
            match i {
                0 => {
                    cx.line(
                        Point::new(mx - half, my - half),
                        Point::new(mx + half, my + half),
                        1.0,
                        glyph,
                    );
                    cx.line(
                        Point::new(mx - half, my + half),
                        Point::new(mx + half, my - half),
                        1.0,
                        glyph,
                    );
                }
                1 => cx.fill_rect(Rect::new(mx - half, my, 2.0 * half, 1.0), glyph),
                _ => cx.rounded_rect(
                    Rect::new(mx - half, my - half, 2.0 * half, 2.0 * half),
                    if self.chrome == Design::Fluent2 {
                        1.5
                    } else {
                        0.0
                    },
                    Color::TRANSPARENT,
                    Border {
                        width: 1.0,
                        color: glyph,
                    },
                ),
            }
        }
    }

    /// The edges a press at `p` would resize (none while the window fills its area).
    fn edges_at(&self, p: Point, size: Size) -> ResizeEdges {
        if self.fills() {
            return ResizeEdges::NONE;
        }
        ResizeEdges::at(p, size, EDGE, ResizeEdges::ALL)
    }

    fn press_button(&mut self, cx: &mut EventCx, button: usize) {
        cx.emit(match button {
            0 => WindowGesture::Close,
            1 => WindowGesture::Minimize,
            _ => WindowGesture::Zoom,
        });
    }
}

fn title_style() -> TextStyle {
    TextStyle::new(12.0).weight(600)
}

fn inset(
    left: f32,
    top: f32,
    right: Option<f32>,
    bottom: Option<f32>,
) -> LayoutRect<LengthPercentageAuto> {
    LayoutRect {
        left: length(left),
        top: length(top),
        right: right.map_or(auto(), length),
        bottom: bottom.map_or(auto(), length),
    }
}

impl Widget for InnerWindow {
    /// Where the window is: `placement:maximized;frame:40,30,480,320`, with `frame`,
    /// `maximized`, `left` or `right` and the latest frame (the one it returns to).
    fn save_state(&self) -> Option<String> {
        let placement = match self.placement {
            WindowPlacement::Frame(_) => "frame",
            WindowPlacement::Maximized => "maximized",
            WindowPlacement::LeftHalf => "left",
            WindowPlacement::RightHalf => "right",
        };
        let mut out = format!("placement:{placement};frame:");
        let f = self.last_frame;
        for (i, n) in [f.x, f.y, f.width, f.height].into_iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            view_state::push_number(&mut out, n);
        }
        Some(out)
    }

    fn restore_state(&mut self, cx: &mut UpdateCx, state: &str) {
        if let Some(&[x, y, width, height]) = view_state::field(state, "frame")
            .and_then(view_state::parse_numbers)
            .as_deref()
            && width >= WINDOW_MIN_SIZE.width
            && height >= WINDOW_MIN_SIZE.height
        {
            self.last_frame = Rect::new(x, y, width, height);
        }
        let placement = match view_state::field(state, "placement") {
            Some("maximized") => WindowPlacement::Maximized,
            Some("left") => WindowPlacement::LeftHalf,
            Some("right") => WindowPlacement::RightHalf,
            _ => WindowPlacement::Frame(self.last_frame),
        };
        self.set_placement(cx, placement);
        // A frame restored under a placement that is not one takes effect on
        // un-zooming, so it repaints nothing now.
        cx.request_layout();
    }

    fn style(&self) -> Style {
        let (inset, size) = match self.placement {
            WindowPlacement::Frame(f) => (
                inset(f.x, f.y, None, None),
                LayoutSize {
                    width: length(f.width),
                    height: length(f.height),
                },
            ),
            WindowPlacement::Maximized => (
                inset(0.0, 0.0, Some(0.0), Some(DESKTOP_DOCK)),
                LayoutSize::auto(),
            ),
            WindowPlacement::LeftHalf => (
                inset(0.0, 0.0, None, Some(DESKTOP_DOCK)),
                LayoutSize {
                    width: percent(0.5),
                    height: auto(),
                },
            ),
            WindowPlacement::RightHalf => (
                LayoutRect {
                    left: percent(0.5),
                    top: length(0.0),
                    right: length(0.0),
                    bottom: length(DESKTOP_DOCK),
                },
                LayoutSize::auto(),
            ),
        };
        Style {
            position: Position::Absolute,
            inset,
            size,
            flex_direction: FlexDirection::Column,
            padding: LayoutRect {
                left: length(1.0),
                right: length(1.0),
                top: length(WINDOW_TITLE_BAR + 1.0),
                bottom: length(1.0),
            },
            ..Default::default()
        }
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let design = cx.theme().design;
        self.chrome = design;
        self.width = bounds.width;
        let radius = if self.fills() {
            0.0
        } else {
            cx.corner(Corner::Container, RADIUS, bounds.height)
        };
        if !self.fills() {
            shadow(cx, bounds, radius);
        }
        let fill = cx.color(Role::Surface);
        let noa_front = design == Design::Noa && self.active;
        let border = Border {
            width: 1.0,
            color: cx.color(if noa_front {
                Role::Accent
            } else {
                Role::Outline
            }),
        };
        let bar_rect = Rect::new(0.0, 0.0, bounds.width, WINDOW_TITLE_BAR);
        match design {
            Design::Aero => {
                let tint = Color::hex(0x7fb2e5).with_alpha(if self.active { 0.6 } else { 0.32 });
                let tint = cx.color(tint);
                cx.push_clip(bar_rect);
                cx.backdrop(bounds, radius, 12.0, tint);
                cx.sheen(bounds, radius);
                cx.pop_clip();
                cx.push_clip(Rect::new(
                    0.0,
                    WINDOW_TITLE_BAR,
                    bounds.width,
                    bounds.height - WINDOW_TITLE_BAR,
                ));
                cx.rounded_rect(bounds, radius, fill, Border::default());
                cx.pop_clip();
                cx.rounded_rect(bounds, radius, Color::TRANSPARENT, border);
            }
            _ => {
                if design == Design::LiquidGlass && !self.fills() {
                    cx.backdrop(bounds, radius, 24.0, Color::TRANSPARENT);
                }
                if design == Design::Noa {
                    // Smoked glass, also when it fills the desktop: the surface alone
                    // is only a wash and would leave text on the bare wallpaper.
                    cx.smoked_glass(bounds, radius);
                }
                cx.rounded_rect(bounds, radius, fill, border);
            }
        }
        let bar = match design {
            Design::LiquidGlass | Design::Aero => None,
            Design::Metro if self.active => Some(cx.color(Role::Accent)),
            Design::Metro => Some(cx.color(Role::SurfaceAlt)),
            Design::Fluent2 => Some(cx.color(Role::Background)),
            _ if self.active => Some(cx.color(Role::SurfaceAlt)),
            _ => None,
        };
        if design == Design::LiquidGlass {
            cx.push_clip(bar_rect);
            cx.sheen(bounds, radius);
            cx.pop_clip();
        } else if let Some(bar) = bar {
            cx.push_clip(Rect::new(
                1.0,
                1.0,
                bounds.width - 2.0,
                WINDOW_TITLE_BAR - 1.0,
            ));
            cx.rounded_rect(
                Rect::new(1.0, 1.0, bounds.width - 2.0, WINDOW_TITLE_BAR + radius),
                (radius - 1.0).max(0.0),
                bar,
                Border::default(),
            );
            cx.pop_clip();
        }
        if matches!(
            design,
            Design::FlatVibrancy | Design::BigSur | Design::Aero | Design::Noa
        ) {
            let line = cx.color(Role::Separator);
            cx.fill_rect(
                Rect::new(1.0, WINDOW_TITLE_BAR, bounds.width - 2.0, 1.0),
                line,
            );
        }
        let ink = if design == Design::Metro && self.active {
            cx.color(Role::OnAccent)
        } else {
            cx.color(if self.active { Role::Text } else { Role::Muted })
        };
        let size = self
            .title_text
            .shape(cx.text_store(), &self.title, title_style(), None);
        if let Some(layout) = self.title_text.layout() {
            let (left, right, x) = if design.windows_chrome() {
                let right = self.button_rect(1).x - 8.0;
                let x = if design == Design::Metro {
                    ((bounds.width - size.width) / 2.0).max(10.0)
                } else {
                    10.0
                };
                (10.0, right, x)
            } else {
                let left = self.button_rect(2).right() + 8.0;
                let x = ((bounds.width - size.width) / 2.0).max(left);
                let right = if design == Design::Noa {
                    self.cursor_rect().x
                } else {
                    bounds.width
                };
                (left, right - 8.0, x)
            };
            cx.push_clip(Rect::new(
                left,
                0.0,
                (right - left).max(0.0),
                WINDOW_TITLE_BAR,
            ));
            cx.text(
                Rect::new(
                    x,
                    (WINDOW_TITLE_BAR - size.height) / 2.0,
                    size.width,
                    size.height,
                ),
                layout,
                ink,
            );
            cx.pop_clip();
        }
        if design.windows_chrome() {
            self.paint_caption_buttons(cx, ink);
            return;
        }
        if design == Design::Noa {
            let cursor = cx.color(if self.active {
                Role::Accent
            } else {
                Role::Outline
            });
            cx.rounded_rect(self.cursor_rect(), 1.0, cursor, Border::default());
        }
        let colors = [0xFF5F57, 0xFEBC2E, 0x28C840];
        for (i, rgb) in colors.into_iter().enumerate() {
            let mut color = if noa_front {
                // One accent: the three buttons are told apart by their place.
                cx.color(Role::Accent)
            } else if self.active {
                Color::hex(rgb)
            } else {
                cx.color(Role::Outline)
            };
            if self.pressed_button == Some(i) && self.button_hovered {
                color = super::pressed_tint(cx, color);
            }
            cx.rounded_rect(self.button_rect(i), BUTTON / 2.0, color, Border::default());
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            Event::PressWithin => cx.emit(WindowGesture::Pressed),
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                let local = cx.local(*pos);
                let edges = self.edges_at(local, cx.size());
                if let Some(button) = self.button_at(local) {
                    self.pressed_button = Some(button);
                    self.button_hovered = true;
                    cx.request_paint();
                } else if edges != ResizeEdges::NONE {
                    self.gesture = Some(Gesture::Resize { edges, from: *pos });
                } else if local.y < WINDOW_TITLE_BAR {
                    if cx.now() - self.last_title_press < DOUBLE_CLICK {
                        self.last_title_press = f64::MIN;
                        cx.emit(WindowGesture::Zoom);
                        cx.set_handled();
                        return;
                    }
                    self.last_title_press = cx.now();
                    self.gesture = Some(Gesture::Drag { from: *pos });
                } else {
                    return;
                }
                cx.capture_pointer();
                cx.request_focus();
                cx.set_handled();
            }
            Event::PointerMove { pos } => {
                if let Some(button) = self.pressed_button {
                    let over = self.button_at(cx.local(*pos)) == Some(button);
                    if over != self.button_hovered {
                        self.button_hovered = over;
                        cx.request_paint();
                    }
                    return;
                }
                match self.gesture {
                    Some(Gesture::Drag { from }) => cx.emit(WindowGesture::Drag {
                        delta: Point::new(pos.x - from.x, pos.y - from.y),
                        pos: *pos,
                    }),
                    Some(Gesture::Resize { edges, from }) => cx.emit(WindowGesture::Resize {
                        edges,
                        delta: Point::new(pos.x - from.x, pos.y - from.y),
                    }),
                    None => return,
                }
                cx.set_handled();
            }
            Event::PointerUp { pos, .. } => {
                if let Some(button) = self.pressed_button.take() {
                    cx.release_pointer();
                    cx.request_paint();
                    cx.set_handled();
                    if self.button_at(cx.local(*pos)) == Some(button) {
                        self.press_button(cx, button);
                    }
                    return;
                }
                match self.gesture.take() {
                    Some(Gesture::Drag { .. }) => cx.emit(WindowGesture::DragEnd { pos: *pos }),
                    Some(Gesture::Resize { .. }) => cx.emit(WindowGesture::ResizeEnd),
                    None => return,
                }
                cx.release_pointer();
                cx.set_handled();
            }
            Event::ActivateItem(i) if *i < 3 => self.press_button(cx, *i),
            _ => {}
        }
    }

    /// The title bar and the resize band take the pointer before the content.
    fn claims_pointer(&self, local: Point, size: Size) -> bool {
        local.y < WINDOW_TITLE_BAR || self.edges_at(local, size) != ResizeEdges::NONE
    }

    fn cursor(&self, local: Point, size: Size) -> Option<Cursor> {
        match &self.gesture {
            Some(Gesture::Resize { edges, .. }) => edges.cursor(),
            Some(Gesture::Drag { .. }) => None,
            None => self.edges_at(local, size).cursor(),
        }
    }

    fn clips_children(&self) -> bool {
        true
    }

    fn focusable(&self) -> bool {
        true
    }

    fn focus_scope(&self) -> bool {
        true
    }

    fn observes_presses(&self) -> bool {
        true
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Window)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        node.set_label(self.title.clone());
    }

    fn accessibility_items(&self) -> Vec<accesskit::Node> {
        ["Close", "Minimize", "Zoom"]
            .into_iter()
            .enumerate()
            .map(|(i, label)| {
                let r = self.button_rect(i);
                let mut node = accesskit::Node::new(accesskit::Role::Button);
                node.set_label(label);
                node.set_bounds(accesskit::Rect {
                    x0: r.x as f64,
                    y0: r.y as f64,
                    x1: r.right() as f64,
                    y1: r.bottom() as f64,
                });
                node.add_action(accesskit::Action::Click);
                node
            })
            .collect()
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.title_text.release(text);
    }
}

/// Carries the index of the minimized window. Pass it to [`WindowManager::handle`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DockClicked(pub usize);

/// Pass it to [`WindowManager::handle`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopCommand {
    /// ⌘` (Ctrl+` elsewhere).
    NextWindow,
    /// ⌘W.
    CloseWindow,
    /// ⌘M.
    MinimizeWindow,
}

const SHORTCUTS: [Shortcut; 3] = [
    Shortcut::command('`'),
    Shortcut::command('w'),
    Shortcut::command('m'),
];

/// The area inner windows live in, with a dock of minimized windows. Its children are
/// [`InnerWindow`]s added by a [`WindowManager`].
#[derive(Debug)]
pub struct Desktop {
    style: Style,
    dock: Vec<String>,
    dock_texts: Vec<CachedText>,
    dock_rects: Vec<Rect>,
    pressed: Option<usize>,
    preview: Option<Rect>,
}

impl Desktop {
    pub fn new(style: Style) -> Self {
        Self {
            style: Style {
                position: Position::Relative,
                ..style
            },
            dock: Vec::new(),
            dock_texts: Vec::new(),
            dock_rects: Vec::new(),
            pressed: None,
            preview: None,
        }
    }

    /// In dock order.
    pub fn dock_items(&self) -> &[String] {
        &self.dock
    }

    /// Widget coordinates, after a paint.
    pub fn dock_item_rect(&self, i: usize) -> Option<Rect> {
        self.dock_rects.get(i).copied()
    }

    /// Desktop coordinates.
    pub fn snap_preview(&self) -> Option<Rect> {
        self.preview
    }

    fn set_dock(&mut self, cx: &mut UpdateCx, items: Vec<String>) {
        if items != self.dock {
            self.dock = items;
            cx.request_paint();
        }
    }

    fn set_preview(&mut self, cx: &mut UpdateCx, preview: Option<Rect>) {
        if preview != self.preview {
            self.preview = preview;
            cx.request_paint();
        }
    }

    fn dock_item_at(&self, p: Point) -> Option<usize> {
        self.dock_rects.iter().position(|r| r.contains(p))
    }
}

impl Widget for Desktop {
    fn style(&self) -> Style {
        self.style.clone()
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let fill = cx.color(Role::Background);
        cx.fill_rect(cx.bounds(), fill);
    }

    fn paint_over(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        if let Some(preview) = self.preview {
            let accent = cx.color(Role::Accent);
            cx.rounded_rect(
                Rect::new(
                    preview.x + 3.0,
                    preview.y + 3.0,
                    preview.width - 6.0,
                    preview.height - 6.0,
                ),
                8.0,
                accent.with_alpha(0.16),
                Border {
                    width: 2.0,
                    color: accent.with_alpha(0.7),
                },
            );
        }
        let top = bounds.height - DESKTOP_DOCK;
        let bar = cx.color(Role::Surface);
        cx.fill_rect(Rect::new(0.0, top, bounds.width, DESKTOP_DOCK), bar);
        let line = cx.color(Role::Separator);
        cx.fill_rect(Rect::new(0.0, top, bounds.width, 1.0), line);
        self.dock_texts
            .resize_with(self.dock.len(), CachedText::default);
        self.dock_rects.clear();
        let mut x = 8.0;
        for (i, title) in self.dock.iter().enumerate() {
            let size = self.dock_texts[i].shape(cx.text_store(), title, TextStyle::new(12.0), None);
            let rect = Rect::new(x, top + 5.0, size.width + 20.0, DESKTOP_DOCK - 10.0);
            let mut fill = cx.color(Role::SurfaceAlt);
            if self.pressed == Some(i) {
                fill = super::pressed_tint(cx, fill);
            }
            let border = Border {
                width: 1.0,
                color: cx.color(Role::Outline),
            };
            let radius = cx.corner(Corner::Control, 5.0, rect.height);
            cx.bezel(rect, radius, fill, border);
            if let Some(layout) = self.dock_texts[i].layout() {
                let ink = cx.color(Role::Text);
                cx.text(
                    Rect::new(
                        rect.x + 10.0,
                        rect.y + (rect.height - size.height) / 2.0,
                        size.width,
                        size.height,
                    ),
                    layout,
                    ink,
                );
            }
            self.dock_rects.push(rect);
            x += rect.width + 6.0;
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                if let Some(i) = self.dock_item_at(cx.local(*pos)) {
                    self.pressed = Some(i);
                    cx.capture_pointer();
                    cx.request_paint();
                    cx.set_handled();
                }
            }
            Event::PointerUp { pos, .. } => {
                if let Some(pressed) = self.pressed.take() {
                    cx.release_pointer();
                    cx.request_paint();
                    cx.set_handled();
                    if self.dock_item_at(cx.local(*pos)) == Some(pressed) {
                        cx.emit(DockClicked(pressed));
                    }
                }
            }
            Event::ActivateItem(i) if *i < self.dock.len() => cx.emit(DockClicked(*i)),
            Event::KeyEquivalent(shortcut) => {
                let command = match SHORTCUTS.iter().position(|s| s == shortcut) {
                    Some(0) => DesktopCommand::NextWindow,
                    Some(1) => DesktopCommand::CloseWindow,
                    Some(2) => DesktopCommand::MinimizeWindow,
                    _ => return,
                };
                cx.emit(command);
            }
            _ => {}
        }
    }

    fn key_equivalents(&self) -> &[Shortcut] {
        &SHORTCUTS
    }

    fn claims_pointer(&self, local: Point, size: Size) -> bool {
        local.y >= size.height - DESKTOP_DOCK
    }

    fn clips_children(&self) -> bool {
        true
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Group)
    }

    fn accessibility_items(&self) -> Vec<accesskit::Node> {
        self.dock
            .iter()
            .enumerate()
            .map(|(i, title)| {
                let mut node = accesskit::Node::new(accesskit::Role::Button);
                node.set_label(format!("Restore {title}"));
                if let Some(r) = self.dock_rects.get(i) {
                    node.set_bounds(accesskit::Rect {
                        x0: r.x as f64,
                        y0: r.y as f64,
                        x1: r.right() as f64,
                        y1: r.bottom() as f64,
                    });
                }
                node.add_action(accesskit::Action::Click);
                node
            })
            .collect()
    }

    fn removed(&mut self, text: &mut TextStore) {
        for t in &mut self.dock_texts {
            t.release(text);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindowEvent {
    Activated(WidgetId),
    Closed(WidgetId),
    /// The window was dropped outside the desktop, at `at` (window coordinates): take
    /// it with [`WindowManager::take_window`] to move it to an OS window, or leave it.
    TornOff {
        window: WidgetId,
        at: Point,
    },
}

#[derive(Debug)]
struct Managed {
    id: WidgetId,
    title: String,
    /// The frame to go back to from a maximized or half placement.
    restore: Option<Rect>,
    minimized: bool,
}

/// Manages the windows of one [`Desktop`]. Keep it next to the tree and pass it every
/// message.
#[derive(Debug)]
pub struct WindowManager {
    desktop: WidgetId,
    /// Bottom to top.
    windows: Vec<Managed>,
    start: Option<Rect>,
    snap: Option<WindowPlacement>,
    opened: usize,
}

impl WindowManager {
    pub fn new(tree: &mut UiTree, parent: WidgetId, style: Style) -> Self {
        let desktop = tree.insert(Some(parent), Desktop::new(style));
        Self {
            desktop,
            windows: Vec::new(),
            start: None,
            snap: None,
            opened: 0,
        }
    }

    pub fn desktop(&self) -> WidgetId {
        self.desktop
    }

    /// Bottom to top, minimized ones included.
    pub fn windows(&self) -> Vec<WidgetId> {
        self.windows.iter().map(|w| w.id).collect()
    }

    /// The topmost window that is not minimized.
    pub fn active(&self) -> Option<WidgetId> {
        self.windows
            .iter()
            .rev()
            .find(|w| !w.minimized)
            .map(|w| w.id)
    }

    pub fn is_minimized(&self, window: WidgetId) -> bool {
        self.windows.iter().any(|w| w.id == window && w.minimized)
    }

    /// Without a `frame` (desktop coordinates) the window is placed a step down and right
    /// of the previous one. Add its content as children of the returned widget.
    pub fn open(&mut self, tree: &mut UiTree, title: &str, frame: Option<Rect>) -> WidgetId {
        let step = (self.opened % 8) as f32 * 26.0;
        self.opened += 1;
        let frame = frame.unwrap_or(Rect::new(32.0 + step, 24.0 + step, 360.0, 240.0));
        let id = tree.insert(Some(self.desktop), InnerWindow::new(title, frame));
        self.windows.push(Managed {
            id,
            title: title.to_owned(),
            restore: None,
            minimized: false,
        });
        self.activate(tree, id);
        id
    }

    /// Focus moves into it unless it is already inside. A minimized window is restored.
    pub fn activate(&mut self, tree: &mut UiTree, window: WidgetId) {
        let Some(index) = self.index(window) else {
            return;
        };
        let mut managed = self.windows.remove(index);
        if managed.minimized {
            managed.minimized = false;
            tree.set_hidden(window, false);
        }
        self.windows.push(managed);
        tree.bring_to_front(window);
        self.sync(tree);
        let inside = tree
            .focused()
            .is_some_and(|f| tree.ancestors(f).any(|a| a == window));
        if !inside {
            tree.set_focus(Some(window));
        }
    }

    pub fn close(&mut self, tree: &mut UiTree, text: &mut TextStore, window: WidgetId) {
        if let Some(index) = self.index(window) {
            self.windows.remove(index);
            tree.remove(window, text);
            self.focus_active(tree);
        }
    }

    pub fn minimize(&mut self, tree: &mut UiTree, window: WidgetId) {
        if let Some(index) = self.index(window)
            && !self.windows[index].minimized
        {
            // Minimized windows keep their dock order: the latest goes last.
            let mut managed = self.windows.remove(index);
            managed.minimized = true;
            self.windows.insert(0, managed);
            tree.set_hidden(window, true);
            self.focus_active(tree);
        }
    }

    pub fn zoom(&mut self, tree: &mut UiTree, window: WidgetId) {
        let Some(index) = self.index(window) else {
            return;
        };
        let placement = match self.earlier_frame(tree, index) {
            Some(frame) => WindowPlacement::Frame(frame),
            None => {
                self.windows[index].restore = self.frame(tree, window);
                WindowPlacement::Maximized
            }
        };
        self.place(tree, window, placement);
    }

    /// Whether `window` has a sheet or alert open (present one at a time).
    pub fn is_blocked(&self, tree: &UiTree, window: WidgetId) -> bool {
        tree.children(window)
            .iter()
            .any(|&c| tree.get::<Sheet>(c).is_some() || tree.get::<Alert>(c).is_some())
    }

    /// Only that window is blocked while the sheet is open. Close it with
    /// [`Sheet::close`].
    pub fn present_sheet(&mut self, tree: &mut UiTree, window: WidgetId, sheet: Sheet) -> WidgetId {
        let id = tree.insert(Some(window), sheet.in_window(WINDOW_TITLE_BAR));
        tree.update::<Sheet, _>(id, |_, cx| cx.request_animation_frame());
        self.activate(tree, window);
        id
    }

    /// Only that window is blocked until the alert is answered.
    pub fn present_alert(&mut self, tree: &mut UiTree, window: WidgetId, alert: Alert) -> WidgetId {
        let id = tree.insert(Some(window), alert.in_window(WINDOW_TITLE_BAR));
        tree.update::<Alert, _>(id, |_, cx| cx.request_animation_frame());
        self.activate(tree, window);
        tree.set_focus(Some(id));
        id
    }

    /// Returns its title and its content's subtrees, to show in its own OS window.
    pub fn take_window(
        &mut self,
        tree: &mut UiTree,
        text: &mut TextStore,
        window: WidgetId,
    ) -> Option<(String, Vec<Subtree>)> {
        let index = self.index(window)?;
        let managed = self.windows.remove(index);
        let children: Vec<WidgetId> = tree.children(window).to_vec();
        let content = children
            .into_iter()
            .filter_map(|c| tree.take_subtree(c))
            .collect();
        tree.remove(window, text);
        self.focus_active(tree);
        Some((managed.title, content))
    }

    /// `content` comes from [`WindowManager::take_window`].
    pub fn adopt(&mut self, tree: &mut UiTree, title: &str, content: Vec<Subtree>) -> WidgetId {
        let window = self.open(tree, title, None);
        for subtree in content {
            tree.insert_subtree(Some(window), subtree);
        }
        window
    }

    pub fn handle(
        &mut self,
        tree: &mut UiTree,
        text: &mut TextStore,
        message: &Message,
    ) -> Option<WindowEvent> {
        let source = message.source;
        if source == self.desktop {
            if let Some(&DockClicked(i)) = message.downcast_ref() {
                let window = self.windows.iter().filter(|w| w.minimized).nth(i)?.id;
                self.activate(tree, window);
                return Some(WindowEvent::Activated(window));
            }
            let active = self.active();
            return match *message.downcast_ref::<DesktopCommand>()? {
                DesktopCommand::NextWindow => {
                    let next = self.windows.iter().find(|w| !w.minimized)?.id;
                    self.activate(tree, next);
                    Some(WindowEvent::Activated(next))
                }
                DesktopCommand::CloseWindow => {
                    self.close(tree, text, active?);
                    Some(WindowEvent::Closed(active?))
                }
                DesktopCommand::MinimizeWindow => {
                    self.minimize(tree, active?);
                    None
                }
            };
        }
        self.index(source)?;
        match *message.downcast_ref::<WindowGesture>()? {
            WindowGesture::Pressed => {
                if self.active() == Some(source) {
                    return None;
                }
                self.activate(tree, source);
                Some(WindowEvent::Activated(source))
            }
            WindowGesture::Drag { delta, pos } => {
                self.drag(tree, source, delta, pos);
                None
            }
            WindowGesture::DragEnd { pos } => {
                self.start = None;
                self.set_preview(tree, None);
                if let Some(placement) = self.snap.take() {
                    let index = self.index(source)?;
                    if self.windows[index].restore.is_none() {
                        self.windows[index].restore = self.frame(tree, source);
                    }
                    self.place(tree, source, placement);
                    return None;
                }
                let area = tree.window_rect(self.desktop)?;
                let outside = pos.x < area.x - TEAR_OFF
                    || pos.x > area.right() + TEAR_OFF
                    || pos.y < area.y - TEAR_OFF
                    || pos.y > area.bottom() + TEAR_OFF;
                outside.then_some(WindowEvent::TornOff {
                    window: source,
                    at: pos,
                })
            }
            WindowGesture::Resize { edges, delta } => {
                self.resize(tree, source, edges, delta);
                None
            }
            WindowGesture::ResizeEnd => {
                self.start = None;
                None
            }
            WindowGesture::Close => {
                self.close(tree, text, source);
                Some(WindowEvent::Closed(source))
            }
            WindowGesture::Minimize => {
                self.minimize(tree, source);
                None
            }
            WindowGesture::Zoom => {
                self.zoom(tree, source);
                None
            }
        }
    }

    fn index(&self, window: WidgetId) -> Option<usize> {
        self.windows.iter().position(|w| w.id == window)
    }

    /// For a maximized or half window, the frame it had before (taken): the one the
    /// manager kept when it zoomed or snapped, or else the window's own latest frame
    /// (a window restored maximized from saved state). `None` for a window with a
    /// frame.
    fn earlier_frame(&mut self, tree: &UiTree, index: usize) -> Option<Rect> {
        let kept = self.windows[index].restore.take();
        let window = tree.get::<InnerWindow>(self.windows[index].id)?;
        if !window.fills() {
            return kept;
        }
        kept.or(Some(window.last_frame()))
    }

    fn area(&self, tree: &UiTree) -> Size {
        let rect = tree
            .layout(self.desktop)
            .map(|l| l.rect)
            .unwrap_or_default();
        Size::new(rect.width, (rect.height - DESKTOP_DOCK).max(0.0))
    }

    /// Desktop coordinates, as laid out.
    fn frame(&self, tree: &UiTree, window: WidgetId) -> Option<Rect> {
        if let WindowPlacement::Frame(frame) = tree.get::<InnerWindow>(window)?.placement() {
            return Some(frame);
        }
        tree.layout(window).map(|l| l.rect)
    }

    fn place(&mut self, tree: &mut UiTree, window: WidgetId, placement: WindowPlacement) {
        tree.update::<InnerWindow, _>(window, |w, cx| w.set_placement(cx, placement));
    }

    fn set_preview(&mut self, tree: &mut UiTree, preview: Option<Rect>) {
        tree.update::<Desktop, _>(self.desktop, |d, cx| d.set_preview(cx, preview));
    }

    fn sync(&mut self, tree: &mut UiTree) {
        let active = self.active();
        for w in &self.windows {
            let is_active = Some(w.id) == active;
            tree.update::<InnerWindow, _>(w.id, |w, cx| w.set_active(cx, is_active));
        }
        let dock = self
            .windows
            .iter()
            .filter(|w| w.minimized)
            .map(|w| w.title.clone())
            .collect();
        tree.update::<Desktop, _>(self.desktop, |d, cx| d.set_dock(cx, dock));
    }

    fn focus_active(&mut self, tree: &mut UiTree) {
        self.sync(tree);
        if let Some(active) = self.active()
            && tree.focused().is_none()
        {
            tree.set_focus(Some(active));
        }
    }

    fn drag(&mut self, tree: &mut UiTree, window: WidgetId, delta: Point, pos: Point) {
        let area = self.area(tree);
        let Some(origin) = tree.window_origin(self.desktop) else {
            return;
        };
        let Some(index) = self.index(window) else {
            return;
        };
        let start = match self.start {
            Some(start) => start,
            None => {
                let current = self.frame(tree, window).unwrap_or_default();
                // A maximized or half window comes off at its earlier size, under the
                // pointer at the same fraction of its width.
                let start = match self.earlier_frame(tree, index) {
                    Some(restore) => {
                        let press_x = pos.x - delta.x - origin.x;
                        let fraction = ((press_x - current.x) / current.width).clamp(0.0, 1.0);
                        Rect::new(
                            press_x - restore.width * fraction,
                            current.y,
                            restore.width,
                            restore.height,
                        )
                    }
                    None => current,
                };
                self.start = Some(start);
                start
            }
        };
        let mut frame = Rect::new(
            start.x + delta.x,
            start.y + delta.y,
            start.width,
            start.height,
        );
        let p = Point::new(pos.x - origin.x, pos.y - origin.y);
        let inside_y = p.y >= 0.0 && p.y <= area.height;
        self.snap = if p.y <= 2.0 && p.y >= -TEAR_OFF && p.x >= 0.0 && p.x <= area.width {
            Some(WindowPlacement::Maximized)
        } else if p.x <= 2.0 && p.x >= -TEAR_OFF && inside_y {
            Some(WindowPlacement::LeftHalf)
        } else if p.x >= area.width - 2.0 && p.x <= area.width + TEAR_OFF && inside_y {
            Some(WindowPlacement::RightHalf)
        } else {
            None
        };
        let preview = self.snap.map(|snap| match snap {
            WindowPlacement::LeftHalf => Rect::new(0.0, 0.0, area.width / 2.0, area.height),
            WindowPlacement::RightHalf => {
                Rect::new(area.width / 2.0, 0.0, area.width / 2.0, area.height)
            }
            _ => Rect::new(0.0, 0.0, area.width, area.height),
        });
        self.set_preview(tree, preview);
        if self.snap.is_none() {
            let others: Vec<Rect> = self
                .windows
                .iter()
                .filter(|w| w.id != window && !w.minimized)
                .filter_map(|w| tree.layout(w.id).map(|l| l.rect))
                .collect();
            frame.x += snap_offset(
                frame.x,
                frame.right(),
                area.width,
                others.iter().map(|o| (o.x, o.right())),
            );
            frame.y += snap_offset(
                frame.y,
                frame.bottom(),
                area.height,
                others.iter().map(|o| (o.y, o.bottom())),
            );
        }
        frame.x = frame.x.clamp(
            KEEP_VISIBLE - frame.width,
            (area.width - KEEP_VISIBLE).max(0.0),
        );
        frame.y = frame
            .y
            .clamp(0.0, (area.height - WINDOW_TITLE_BAR).max(0.0));
        self.place(tree, window, WindowPlacement::Frame(frame));
    }

    fn resize(&mut self, tree: &mut UiTree, window: WidgetId, edges: ResizeEdges, delta: Point) {
        let area = self.area(tree);
        let Some(index) = self.index(window) else {
            return;
        };
        let start = match self.start {
            Some(start) => start,
            None => {
                let start = self.frame(tree, window).unwrap_or_default();
                self.start = Some(start);
                self.windows[index].restore = None;
                start
            }
        };
        let unlimited = Size::new(f32::INFINITY, f32::INFINITY);
        let frame = resize_rect(start, edges, delta, WINDOW_MIN_SIZE, unlimited, Some(area));
        self.place(tree, window, WindowPlacement::Frame(frame));
    }
}

/// How far to shift a span `near..far` so that one of its ends meets the start or end
/// of the area (`0..limit`) or of another span, if one is within [`SNAP_DISTANCE`].
fn snap_offset(near: f32, far: f32, limit: f32, others: impl Iterator<Item = (f32, f32)>) -> f32 {
    let mut best: Option<f32> = None;
    let mut consider = |edge: f32, target: f32| {
        let offset = target - edge;
        if offset.abs() <= SNAP_DISTANCE && best.is_none_or(|b| offset.abs() < b.abs()) {
            best = Some(offset);
        }
    };
    consider(near, 0.0);
    consider(far, limit);
    for (start, end) in others {
        for target in [start, end] {
            consider(near, target);
            consider(far, target);
        }
    }
    best.unwrap_or(0.0)
}

//! Platform-independent input events. Positions are logical pixels in window space.

use std::any::Any;
use std::path::PathBuf;
use std::rc::Rc;

use noa_gpui_scene::Point;

use crate::tree::WidgetId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
    /// The platform's shortcut modifier: Cmd (`meta`) on Apple platforms, Ctrl
    /// elsewhere. Set by the platform layer.
    pub command: bool,
    /// [`Self::command`] is Cmd (Apple platforms), not Ctrl. Set by the platform layer
    /// with the keys, as the keys alone cannot tell Ctrl+Cmd on a Mac from Ctrl+Super
    /// elsewhere. Off by default.
    pub command_is_meta: bool,
}

impl Modifiers {
    /// No keys held, on an Apple platform (Cmd is the command key).
    pub fn on_apple() -> Self {
        Self {
            command_is_meta: true,
            ..Self::default()
        }
    }

    /// No keys held, elsewhere (Ctrl is the command key).
    pub fn elsewhere() -> Self {
        Self::default()
    }

    /// Whether Ctrl is held as a key of its own, apart from [`Self::command`]: only on
    /// Apple platforms, where the command key is Cmd. Elsewhere Ctrl is the command key.
    pub fn control(&self) -> bool {
        self.ctrl && self.command_is_meta
    }
}

/// Printable input arrives as [`Event::Text`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Tab,
    Enter,
    Space,
    Escape,
    Backspace,
    Delete,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Home,
    End,
    PageUp,
    PageDown,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    /// Shift+F10 opens the focused widget's context menu.
    F10,
    F11,
    F12,
    ContextMenu,
    /// Lowercase, for shortcuts such as Cmd+C. The text it types, if any, also arrives
    /// as [`Event::Text`].
    Char(char),
    Other,
}

impl Key {
    const FUNCTION: [Key; 12] = [
        Key::F1,
        Key::F2,
        Key::F3,
        Key::F4,
        Key::F5,
        Key::F6,
        Key::F7,
        Key::F8,
        Key::F9,
        Key::F10,
        Key::F11,
        Key::F12,
    ];

    /// The function key F`n`, for `n` in 1..=12.
    pub fn function(n: u8) -> Option<Key> {
        Self::FUNCTION.get(usize::from(n).checked_sub(1)?).copied()
    }

    /// `n` for the function key F`n`.
    pub fn function_number(self) -> Option<u8> {
        Self::FUNCTION
            .iter()
            .position(|&k| k == self)
            .map(|i| i as u8 + 1)
    }
}

#[derive(Clone)]
pub enum DragData {
    Text(String),
    Files(Vec<PathBuf>),
    Widget(WidgetId),
    /// Drop targets downcast it.
    Custom(Rc<dyn Any>),
}

impl std::fmt::Debug for DragData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DragData::Text(text) => f.debug_tuple("Text").field(text).finish(),
            DragData::Files(files) => f.debug_tuple("Files").field(files).finish(),
            DragData::Widget(id) => f.debug_tuple("Widget").field(id).finish(),
            DragData::Custom(_) => f.write_str("Custom(..)"),
        }
    }
}

impl PartialEq for DragData {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (DragData::Text(a), DragData::Text(b)) => a == b,
            (DragData::Files(a), DragData::Files(b)) => a == b,
            (DragData::Widget(a), DragData::Widget(b)) => a == b,
            (DragData::Custom(a), DragData::Custom(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// See [`crate::Widget::cursor`]. The platform layer shows the OS's own cursor of that
/// kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Cursor {
    #[default]
    Default,
    Pointer,
    Text,
    Crosshair,
    Move,
    Grab,
    Grabbing,
    NotAllowed,
    EwResize,
    NsResize,
    NwseResize,
    NeswResize,
    ColResize,
    RowResize,
}

impl Cursor {
    /// The resize cursor for an edge or corner, given the sides it touches.
    pub fn for_edges(left: bool, right: bool, top: bool, bottom: bool) -> Option<Cursor> {
        match (left || right, top || bottom) {
            (true, true) if (left && top) || (right && bottom) => Some(Cursor::NwseResize),
            (true, true) => Some(Cursor::NeswResize),
            (true, false) => Some(Cursor::EwResize),
            (false, true) => Some(Cursor::NsResize),
            (false, false) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    PointerMove {
        pos: Point,
    },
    PointerDown {
        pos: Point,
        button: PointerButton,
    },
    PointerUp {
        pos: Point,
        button: PointerButton,
    },
    PointerLeave,
    /// Scroll by `delta` logical pixels (positive `y` moves content down, like a wheel
    /// turned towards the user).
    Wheel {
        pos: Point,
        delta: Point,
    },
    KeyDown {
        key: Key,
        modifiers: Modifiers,
    },
    /// Committed text input (one or more characters), including text committed by an IME.
    Text(String),
    /// IME composition in progress: `text` is shown at the caret but not yet part of the
    /// content. `cursor` is a byte range inside `text` (`None` hides the cursor). An empty
    /// `text` ends the composition.
    Composition {
        text: String,
        cursor: Option<(usize, usize)>,
    },
    /// The input method is about to convert bytes `range` of the text again (bytes of
    /// the text shown: [`crate::ImeContext::base`] plus a range of its text): select
    /// them, so that the composition that follows takes their place.
    ImeSelect(std::ops::Range<usize>),
    /// The modifier keys changed. Not delivered to widgets; read the current state with
    /// [`crate::EventCx::modifiers`].
    ModifiersChanged(Modifiers),
    /// Sent only to the widget itself (no bubbling) when the pointer starts or stops
    /// hovering it.
    HoverChanged(bool),
    /// Something is dragged over `pos`. Bubbles from the widget under the pointer; a
    /// widget that can take the data calls `accept_drop` and becomes the drop target.
    /// The platform layer dispatches it for files dragged in from the OS.
    DragOver {
        pos: Point,
        data: DragData,
    },
    /// The drag left the current drop target (sent only to it), or the platform reports
    /// that an OS drag left the window.
    DragLeave,
    /// The data was dropped on the drop target (sent only to it).
    Drop {
        pos: Point,
        data: DragData,
    },
    /// Perform the widget's primary action (sent only to it): a matching key equivalent
    /// such as Return for the default button, or a Click from assistive technology.
    Activate,
    /// Perform the action of one of the widget's accessibility items (see
    /// `Widget::accessibility_items`), such as selecting a radio option. Sent only to
    /// the widget.
    ActivateItem(usize),
    /// A menu opened by this widget with `open_menu` closed (sent only to it):
    /// `chosen` is the item index, or `None` when it was dismissed.
    MenuClosed {
        chosen: Option<usize>,
    },
    /// The pointer was pressed on this widget or inside it, whoever handles the press
    /// (sent before the press, only to widgets with [`crate::Widget::observes_presses`];
    /// an inner window uses it to become active).
    PressWithin,
    /// A popover opened by this widget with `open_popover` closed (sent only to it).
    PopoverClosed,
    /// A value sent by the content of a popover this widget opened, with
    /// `notify_owner` (sent only to it).
    FromPopover(PopoverMessage),
    /// A value sent by the widget that opened the popup or popover this widget is the
    /// root of, with `notify_popup` (sent only to it).
    FromOwner(PopoverMessage),
    /// The widget gained (`true`) or lost the keyboard focus (sent only to it, after the
    /// event that moved the focus has been handled).
    FocusChanged(bool),
    /// Assistive technology asks to raise the value (sliders, steppers). Sent only to
    /// the widget.
    Increment,
    /// Assistive technology asks to lower the value. Sent only to the widget.
    Decrement,
    /// One of the widget's `key_equivalents` was pressed and no focused widget handled
    /// the key (sent only to it).
    KeyEquivalent(crate::shortcut::Shortcut),
    /// Sent to the widget that started a drag when the drag ends.
    DragEnd {
        dropped: bool,
    },
    /// Sent only to widgets that called `request_animation_frame`, once per frame.
    /// `now` is in seconds (see [`crate::UiTree::set_now`]).
    AnimationFrame {
        now: f64,
    },
    /// Sent to a widget when a timer it asked for with [`crate::EventCx::request_timer`]
    /// is due, whether or not it is visible. `now` is in seconds.
    Timer {
        now: f64,
    },
}

impl Event {
    pub fn position(&self) -> Option<Point> {
        match self {
            Event::PointerMove { pos }
            | Event::PointerDown { pos, .. }
            | Event::PointerUp { pos, .. }
            | Event::Wheel { pos, .. } => Some(*pos),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct PopoverMessage(pub(crate) Rc<dyn Any>);

impl PopoverMessage {
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        self.0.downcast_ref()
    }
}

impl std::fmt::Debug for PopoverMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PopoverMessage(..)")
    }
}

impl PartialEq for PopoverMessage {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

pub struct Message {
    pub source: WidgetId,
    payload: Box<dyn Any>,
}

impl Message {
    pub(crate) fn new(source: WidgetId, payload: Box<dyn Any>) -> Self {
        Self { source, payload }
    }

    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        self.payload.downcast_ref()
    }
}

impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Message")
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}

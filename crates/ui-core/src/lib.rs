//! Produces a [`noa_gpui_scene::DisplayList`] and knows nothing about the GPU or the window
//! system.

pub mod animation;
pub mod clipboard;
pub mod date;
pub mod event;
pub mod inspect;
pub mod invalidation;
pub mod kana;
pub mod overlay;
pub mod recording;
pub mod shortcut;
pub mod styling;
pub mod text_edit;
pub mod theme;
pub mod tree;
pub mod view_state;
pub mod widget;
pub mod widgets;

pub use accesskit;
pub use date::DateTime;
pub use event::{Cursor, DragData, Event, Key, Message, Modifiers, PointerButton, PopoverMessage};
pub use invalidation::Dirty;
pub use noa_gpui_layout::{MeasureInput, Style, style};
pub use overlay::{Edge, OverlayDismissed, OverlayOptions, Placement, WindowCorner};
pub use shortcut::{Shortcut, ShortcutPressed};
pub use styling::{
    Length, MatchedRule, StyleError, StyleMeta, StyleNode, StylePatch, StyleSheet, StyleState,
    StyleTree, TextProperty,
};
pub use theme::{Appearance, Corner, Design, Paint, Role, Theme};
pub use tree::{ACCESS_ROOT, Subtree, UiTree, UpdateStats, WidgetId, access_id, access_item_id};
pub use view_state::ViewState;
pub use widget::{EventCx, MeasureCx, PaintCx, UpdateCx, Widget};

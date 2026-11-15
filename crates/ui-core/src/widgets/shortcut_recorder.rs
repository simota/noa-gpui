use noa_gpui_layout::style::{Dimension, Size as LayoutSize, length};
use noa_gpui_layout::{MeasureInput, Style};
use noa_gpui_scene::{Border, Point, Rect, Size};
use noa_gpui_text::{TextStore, TextStyle};

use super::CachedText;
use super::kbd::{KEYCAP_HEIGHT, keycaps_width, paint_keycaps, shape_keycaps};
use super::text_field::paint_field_box;
use crate::event::{Event, Key, PointerButton};
use crate::shortcut::Shortcut;
use crate::theme::{Corner, Role};
use crate::widget::{EventCx, MeasureCx, PaintCx, UpdateCx, Widget};

/// Emitted when the shortcut is recorded or cleared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShortcutChanged(pub Option<Shortcut>);

const HEIGHT: f32 = 28.0;
const PAD: f32 = 8.0;
const CLEAR: f32 = 16.0;
const PROMPT: &str = "Type shortcut…";
const EMPTY: &str = "Record Shortcut";

/// A field that records the keyboard shortcut pressed in it (the shortcut fields of
/// macOS's Keyboard settings). Emits [`ShortcutChanged`].
///
/// While it records, every key goes to it, so shortcuts registered on the tree do not
/// fire.
#[derive(Debug)]
pub struct ShortcutRecorder {
    shortcut: Option<Shortcut>,
    recording: bool,
    bare_keys: bool,
    glyphs: bool,
    width: f32,
    keys: Vec<String>,
    keycaps: Vec<CachedText>,
    prompt: CachedText,
}

impl ShortcutRecorder {
    pub fn new(shortcut: Option<Shortcut>) -> Self {
        let mut recorder = Self {
            shortcut,
            recording: false,
            bare_keys: false,
            glyphs: cfg!(target_vendor = "apple"),
            width: 180.0,
            keys: Vec::new(),
            keycaps: Vec::new(),
            prompt: CachedText::default(),
        };
        recorder.sync_keys();
        recorder
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Also accept keys pressed without a modifier (a single letter, an arrow).
    pub fn allow_bare_keys(mut self, allow: bool) -> Self {
        self.bare_keys = allow;
        self
    }

    /// Show the keys as Mac glyphs (`⌘` `⇧` `S`) or names (`Ctrl` `Shift` `S`); by
    /// default as the platform's menus do.
    pub fn glyphs(mut self, glyphs: bool) -> Self {
        self.glyphs = glyphs;
        self.sync_keys();
        self
    }

    pub fn shortcut(&self) -> Option<Shortcut> {
        self.shortcut
    }

    pub fn is_recording(&self) -> bool {
        self.recording
    }

    /// Set the shortcut without emitting.
    pub fn set_shortcut(&mut self, cx: &mut UpdateCx, shortcut: Option<Shortcut>) {
        self.shortcut = shortcut;
        self.sync_keys();
        cx.request_layout();
    }

    fn sync_keys(&mut self) {
        self.keys = self.shortcut.map_or_else(Vec::new, |s| s.keys(self.glyphs));
    }

    fn change(&mut self, cx: &mut EventCx, shortcut: Option<Shortcut>) {
        self.recording = false;
        if shortcut != self.shortcut {
            self.shortcut = shortcut;
            self.sync_keys();
            cx.emit(ShortcutChanged(shortcut));
        }
        cx.request_layout();
    }

    fn clear_rect(&self, height: f32) -> Rect {
        Rect::new(
            self.width - PAD - CLEAR,
            (height - CLEAR) / 2.0,
            CLEAR,
            CLEAR,
        )
    }

    fn shows_clear(&self) -> bool {
        self.shortcut.is_some() && !self.recording
    }

    fn record(&self, key: Key, modifiers: crate::event::Modifiers) -> Option<Shortcut> {
        let usable = !matches!(
            key,
            Key::Other | Key::ContextMenu | Key::Tab | Key::Escape | Key::Enter
        ) || modifiers.command
            || modifiers.alt;
        if !usable {
            return None;
        }
        let modified = modifiers.command || modifiers.alt;
        let function_key = key.function_number().is_some();
        if !(modified || function_key || self.bare_keys) {
            return None;
        }
        Some(Shortcut {
            key,
            command: modifiers.command,
            shift: modifiers.shift,
            alt: modifiers.alt,
        })
    }
}

impl Widget for ShortcutRecorder {
    fn paints_style_box(&self) -> bool {
        true
    }

    fn style(&self) -> Style {
        Style {
            size: LayoutSize {
                width: length(self.width),
                height: Dimension::auto(),
            },
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn measures(&self) -> bool {
        true
    }

    fn measure(&mut self, cx: &mut MeasureCx, _input: MeasureInput) -> Size {
        shape_keycaps(cx.text, &self.keys, &mut self.keycaps);
        self.keycaps.truncate(self.keys.len());
        let prompt = if self.recording { PROMPT } else { EMPTY };
        self.prompt
            .shape(cx.text, prompt, TextStyle::new(13.0), None);
        Size::new(self.width, HEIGHT)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        paint_field_box(cx, Corner::Control, 6.0);
        let height = cx.size().height;
        if self.recording {
            let tint = cx.theme().accent.with_alpha(0.12);
            let radius = cx.corner(Corner::Control, 6.0, height);
            cx.rounded_rect(cx.bounds(), radius, tint, Border::default());
        }
        if self.recording || self.shortcut.is_none() {
            if let Some(layout) = self.prompt.layout() {
                let size = self.prompt.size();
                let ink = cx.color(if self.recording {
                    Role::Accent
                } else {
                    Role::Muted
                });
                let at = Point::new(
                    (self.width - size.width) / 2.0,
                    (height - size.height) / 2.0,
                );
                cx.text(Rect::new(at.x, at.y, size.width, size.height), layout, ink);
            }
        } else {
            let width = keycaps_width(&self.keycaps);
            let room = self.width - 2.0 * PAD - CLEAR;
            let origin = Point::new(
                PAD + ((room - width) / 2.0).max(0.0),
                (height - KEYCAP_HEIGHT) / 2.0,
            );
            paint_keycaps(cx, &self.keycaps, origin);
        }
        if self.shows_clear() {
            let rect = self.clear_rect(height);
            let muted = cx.color(Role::Muted);
            cx.rounded_rect(rect, CLEAR / 2.0, muted, Border::default());
            let surface = cx.color(Role::Surface);
            let (c, r) = (Point::new(rect.x + CLEAR / 2.0, rect.y + CLEAR / 2.0), 3.0);
            cx.line(
                Point::new(c.x - r, c.y - r),
                Point::new(c.x + r, c.y + r),
                1.5,
                surface,
            );
            cx.line(
                Point::new(c.x + r, c.y - r),
                Point::new(c.x - r, c.y + r),
                1.5,
                surface,
            );
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                let p = cx.local(*pos);
                cx.request_focus();
                cx.set_handled();
                if self.shows_clear() && self.clear_rect(cx.size().height).contains(p) {
                    self.change(cx, None);
                    return;
                }
                self.recording = !self.recording;
                cx.request_layout();
            }
            Event::KeyDown { key, modifiers } if self.recording => {
                let bare = !(modifiers.command || modifiers.alt || modifiers.shift);
                match key {
                    Key::Escape if bare => {
                        self.recording = false;
                        cx.request_layout();
                    }
                    Key::Backspace | Key::Delete if bare => self.change(cx, None),
                    // Leaving with Tab stops recording; the tree moves the focus.
                    Key::Tab if bare => {
                        self.recording = false;
                        cx.request_layout();
                        return;
                    }
                    _ => {
                        if let Some(shortcut) = self.record(*key, *modifiers) {
                            self.change(cx, Some(shortcut));
                        }
                    }
                }
                // Every key belongs to the recorder while it records.
                cx.set_handled();
            }
            // The text a recorded key types is not wanted.
            Event::Text(_) | Event::Composition { .. } if self.recording => cx.set_handled(),
            Event::KeyDown {
                key: Key::Space | Key::Enter,
                ..
            }
            | Event::Activate => {
                self.recording = true;
                cx.request_layout();
                cx.set_handled();
            }
            Event::KeyDown {
                key: Key::Backspace | Key::Delete,
                ..
            } if self.shortcut.is_some() => {
                self.change(cx, None);
                cx.set_handled();
            }
            Event::FocusChanged(false) if self.recording => {
                self.recording = false;
                cx.request_layout();
            }
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Button)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        node.set_label("Keyboard shortcut");
        let value = match (self.recording, self.shortcut) {
            (true, _) => PROMPT.to_owned(),
            (false, Some(s)) => s.label(false),
            (false, None) => "None".to_owned(),
        };
        node.set_value(value);
        if let Some(s) = self.shortcut {
            node.set_keyboard_shortcut(s.label(false));
        }
        node.add_action(accesskit::Action::Click);
    }

    fn removed(&mut self, text: &mut TextStore) {
        for cached in &mut self.keycaps {
            cached.release(text);
        }
        self.prompt.release(text);
    }
}

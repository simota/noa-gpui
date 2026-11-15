use noa_gpui_core::{DragData, Event, Key, Modifiers, PointerButton};
use noa_gpui_scene::Point;
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::{Key as WinitKey, NamedKey};
#[cfg(not(target_arch = "wasm32"))]
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;

/// Logical pixels per wheel "line".
const LINE_HEIGHT: f32 = 40.0;

#[derive(Default)]
pub struct InputState {
    cursor: Option<Point>,
    modifiers: Modifiers,
    hovered_files: Vec<std::path::PathBuf>,
    /// Files dropped on the window, until [`InputState::take_drop`] hands them on.
    dropped_files: Vec<std::path::PathBuf>,
}

impl InputState {
    /// The files dropped on the window since the last call, as one drop. The OS reports
    /// each file by itself: call this once its events are through (`about_to_wait`).
    pub fn take_drop(&mut self) -> Option<Event> {
        if self.dropped_files.is_empty() {
            return None;
        }
        Some(Event::Drop {
            pos: self.cursor.unwrap_or_default(),
            data: DragData::Files(std::mem::take(&mut self.dropped_files)),
        })
    }

    /// Translate one window event. A key press may produce both a key and a text event.
    pub fn translate(&mut self, event: &WindowEvent, scale: f64) -> Vec<Event> {
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let p = position.to_logical::<f32>(scale);
                let pos = Point::new(p.x, p.y);
                self.cursor = Some(pos);
                vec![Event::PointerMove { pos }]
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor = None;
                vec![Event::PointerLeave]
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let (Some(pos), Some(button)) = (self.cursor, pointer_button(*button)) else {
                    return vec![];
                };
                vec![match state {
                    ElementState::Pressed => Event::PointerDown { pos, button },
                    ElementState::Released => Event::PointerUp { pos, button },
                }]
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let Some(pos) = self.cursor else {
                    return vec![];
                };
                let delta = match delta {
                    MouseScrollDelta::LineDelta(x, y) => {
                        Point::new(x * LINE_HEIGHT, y * LINE_HEIGHT)
                    }
                    MouseScrollDelta::PixelDelta(p) => {
                        let p = p.to_logical::<f32>(scale);
                        Point::new(p.x, p.y)
                    }
                };
                vec![Event::Wheel { pos, delta }]
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                let state = modifiers.state();
                let (ctrl, meta) = (state.control_key(), state.super_key());
                self.modifiers = Modifiers {
                    shift: state.shift_key(),
                    ctrl,
                    alt: state.alt_key(),
                    meta,
                    command: if command_is_meta() { meta } else { ctrl },
                    command_is_meta: command_is_meta(),
                };
                vec![Event::ModifiersChanged(self.modifiers)]
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let mut events = Vec::new();
                let key = match &event.logical_key {
                    WinitKey::Named(named) => named_key(*named),
                    WinitKey::Character(c) => character_key(event, c),
                    _ => Key::Other,
                };
                if key != Key::Other {
                    events.push(Event::KeyDown {
                        key,
                        modifiers: self.modifiers,
                    });
                }
                let shortcut = self.modifiers.ctrl || self.modifiers.meta;
                if let Some(text) = &event.text
                    && !shortcut
                    && text.chars().all(|c| !c.is_control())
                {
                    events.push(Event::Text(text.to_string()));
                }
                events
            }
            // The OS reports one event per file; the position is the last known cursor
            // position (cursor moves are not reported during an OS drag on every
            // platform).
            WindowEvent::HoveredFile(path) => {
                self.hovered_files.push(path.clone());
                vec![Event::DragOver {
                    pos: self.cursor.unwrap_or_default(),
                    data: DragData::Files(self.hovered_files.clone()),
                }]
            }
            WindowEvent::HoveredFileCancelled => {
                self.hovered_files.clear();
                vec![Event::DragLeave]
            }
            // One event per file here too: they are kept until the queue is empty and
            // dropped together (see `take_drop`), so that what takes them sees how many
            // and which there are.
            WindowEvent::DroppedFile(path) => {
                self.hovered_files.clear();
                self.dropped_files.push(path.clone());
                vec![]
            }
            WindowEvent::Ime(ime) => {
                log::debug!("ime: {ime:?}");
                self.translate_ime(ime)
            }
            _ => vec![],
        }
    }

    fn translate_ime(&mut self, ime: &Ime) -> Vec<Event> {
        match ime {
            Ime::Preedit(text, cursor) => vec![Event::Composition {
                text: text.clone(),
                cursor: *cursor,
            }],
            Ime::Commit(text) => vec![Event::Text(text.clone())],
            Ime::Disabled => vec![Event::Composition {
                text: String::new(),
                cursor: None,
            }],
            Ime::Enabled => vec![],
        }
    }
}

/// A character key, reported lowercase and without modifiers applied, so Cmd+Shift+Z
/// reports 'z'.
fn character_key(event: &winit::event::KeyEvent, logical: &str) -> Key {
    #[cfg(not(target_arch = "wasm32"))]
    let unmodified = match event.key_without_modifiers() {
        WinitKey::Character(c) => c.chars().next(),
        _ => None,
    };
    // The Web reports the character with Shift applied; lowercase covers letters.
    #[cfg(target_arch = "wasm32")]
    let unmodified = {
        let _ = event;
        None
    };
    unmodified
        .or_else(|| logical.chars().next())
        .map_or(Key::Other, |c| Key::Char(c.to_ascii_lowercase()))
}

/// Logical pixels the pointer may move between the presses of a double click.
const MULTI_CLICK_DISTANCE: f32 = 4.0;

/// Use the OS double-click interval for `tree`'s click counting, where the platform
/// reports one (macOS); elsewhere the tree keeps its default.
pub fn sync_multi_click(tree: &mut noa_gpui_core::UiTree) {
    #[cfg(all(feature = "native", target_os = "macos"))]
    {
        // SAFETY: `+[NSEvent doubleClickInterval]` takes no arguments and returns an
        // NSTimeInterval (a double).
        let interval: f64 =
            unsafe { objc2::msg_send![objc2::class!(NSEvent), doubleClickInterval] };
        tree.set_multi_click(interval, MULTI_CLICK_DISTANCE);
    }
    #[cfg(not(all(feature = "native", target_os = "macos")))]
    let _ = (tree, MULTI_CLICK_DISTANCE);
}

/// Whether the shortcut modifier is Cmd (Apple platforms) rather than Ctrl.
pub fn command_is_meta() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|w| w.navigator().platform().ok())
            .is_some_and(|p| p.starts_with("Mac") || p.starts_with("iP"))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        cfg!(target_vendor = "apple")
    }
}

fn pointer_button(button: MouseButton) -> Option<PointerButton> {
    match button {
        MouseButton::Left => Some(PointerButton::Primary),
        MouseButton::Right => Some(PointerButton::Secondary),
        MouseButton::Middle => Some(PointerButton::Middle),
        _ => None,
    }
}

fn named_key(key: NamedKey) -> Key {
    match key {
        NamedKey::Tab => Key::Tab,
        NamedKey::Enter => Key::Enter,
        NamedKey::Space => Key::Space,
        NamedKey::Escape => Key::Escape,
        NamedKey::Backspace => Key::Backspace,
        NamedKey::Delete => Key::Delete,
        NamedKey::ArrowLeft => Key::ArrowLeft,
        NamedKey::ArrowRight => Key::ArrowRight,
        NamedKey::ArrowUp => Key::ArrowUp,
        NamedKey::ArrowDown => Key::ArrowDown,
        NamedKey::Home => Key::Home,
        NamedKey::End => Key::End,
        NamedKey::PageUp => Key::PageUp,
        NamedKey::PageDown => Key::PageDown,
        NamedKey::F1 => Key::F1,
        NamedKey::F2 => Key::F2,
        NamedKey::F3 => Key::F3,
        NamedKey::F4 => Key::F4,
        NamedKey::F5 => Key::F5,
        NamedKey::F6 => Key::F6,
        NamedKey::F7 => Key::F7,
        NamedKey::F8 => Key::F8,
        NamedKey::F9 => Key::F9,
        NamedKey::F10 => Key::F10,
        NamedKey::F11 => Key::F11,
        NamedKey::F12 => Key::F12,
        NamedKey::ContextMenu => Key::ContextMenu,
        _ => Key::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_function_key_has_its_own_key() {
        let keys = [
            NamedKey::F1,
            NamedKey::F2,
            NamedKey::F3,
            NamedKey::F4,
            NamedKey::F5,
            NamedKey::F6,
            NamedKey::F7,
            NamedKey::F8,
            NamedKey::F9,
            NamedKey::F10,
            NamedKey::F11,
            NamedKey::F12,
        ];
        for (n, named) in (1..).zip(keys) {
            assert_eq!(named_key(named), Key::function(n).unwrap());
        }
        assert_eq!(named_key(NamedKey::F13), Key::Other);
    }

    #[test]
    fn files_dropped_together_arrive_as_one_drop() {
        let mut input = InputState::default();
        let hover = input.translate(&WindowEvent::HoveredFile("/tmp/a.png".into()), 1.0);
        assert!(matches!(hover[..], [Event::DragOver { .. }]));
        assert!(input.take_drop().is_none());
        for name in ["/tmp/a.png", "/tmp/b.png"] {
            let events = input.translate(&WindowEvent::DroppedFile(name.into()), 1.0);
            assert!(events.is_empty());
        }
        let Some(Event::Drop {
            data: DragData::Files(files),
            ..
        }) = input.take_drop()
        else {
            panic!("expected a drop of files");
        };
        assert_eq!(files.len(), 2);
        assert!(input.take_drop().is_none());
    }
}

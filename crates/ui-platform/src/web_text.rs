//! Text input on the Web, from the DOM events of the hidden text area that
//! [`crate::web_ime`] keeps focused while a text widget is: keys, typed text and IME
//! composition, translated to the same [`Event`]s the desktop gets from winit.
//!
//! winit's Web backend reports no IME events, so typing goes through a text area
//! instead of the canvas while a text widget has focus. This part holds no DOM types,
//! so it is tested on the desktop.

use noa_gpui_core::{Event, Key, Modifiers};

/// The modifier keys of a DOM keyboard event.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct DomModifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

/// A DOM `keydown` / `keyup` event.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DomKey<'a> {
    /// `KeyboardEvent.key`.
    pub key: &'a str,
    /// `KeyboardEvent.keyCode`: 229 while an IME handles the key.
    pub key_code: u32,
    /// `KeyboardEvent.isComposing`.
    pub composing: bool,
    pub modifiers: DomModifiers,
}

impl DomKey<'_> {
    /// The IME takes this key. Key code 229 also covers the case reported for WebKit
    /// (not checked in a browser here): the key that commits a composition arriving
    /// after `compositionend`, with `isComposing` false.
    fn for_ime(&self) -> bool {
        self.composing || self.key_code == 229 || matches!(self.key, "Process" | "Dead")
    }
}

/// What a key press produced, and whether the text area must not handle it itself.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct KeyOutcome {
    pub events: Vec<Event>,
    /// Call `preventDefault`: the key is not typed into the text area (named keys and
    /// shortcuts). Text keys are left to the browser, whose `input` and composition
    /// events bring the text.
    pub prevent_default: bool,
}

#[derive(Debug)]
pub(crate) struct WebTextInput {
    command_is_meta: bool,
    modifiers: Modifiers,
}

impl WebTextInput {
    pub fn new(command_is_meta: bool) -> Self {
        Self {
            command_is_meta,
            modifiers: Modifiers::default(),
        }
    }

    pub fn key_down(&mut self, key: DomKey<'_>) -> KeyOutcome {
        let mut events = self.modifiers_changed(key.modifiers);
        if key.for_ime() {
            return KeyOutcome {
                events,
                prevent_default: false,
            };
        }
        let shortcut = key.modifiers.ctrl || key.modifiers.meta;
        let (key, named) = match named_key(key.key) {
            Some(named) => (named, true),
            None => (character_key(key.key), false),
        };
        if key != Key::Other {
            events.push(Event::KeyDown {
                key,
                modifiers: self.modifiers,
            });
        }
        KeyOutcome {
            events,
            // Space types a space through `input`, like any other text key.
            prevent_default: (named && key != Key::Space) || shortcut,
        }
    }

    pub fn key_up(&mut self, key: DomKey<'_>) -> Vec<Event> {
        self.modifiers_changed(key.modifiers)
    }

    /// An `input` event. Composed text arrives through the composition events, so the
    /// input types that report it are skipped here.
    pub fn input(&self, input_type: &str, data: Option<&str>, composing: bool) -> Vec<Event> {
        match (input_type, data) {
            ("insertText" | "insertReplacementText", Some(text))
                if !composing && !text.is_empty() =>
            {
                vec![Event::Text(text.to_string())]
            }
            _ => vec![],
        }
    }

    /// `compositionupdate`: the text being composed, with the cursor at its end. The
    /// event carries only the string; the IME's cursor and clauses are not read.
    pub fn composition_update(&self, data: &str) -> Vec<Event> {
        vec![Event::Composition {
            text: data.to_string(),
            cursor: Some((data.len(), data.len())),
        }]
    }

    /// `compositionend`: the composition ends, and its text is committed unless it was
    /// cancelled (empty).
    pub fn composition_end(&self, data: &str) -> Vec<Event> {
        let mut events = vec![Event::Composition {
            text: String::new(),
            cursor: None,
        }];
        if !data.is_empty() {
            events.push(Event::Text(data.to_string()));
        }
        events
    }

    fn modifiers_changed(&mut self, dom: DomModifiers) -> Vec<Event> {
        let modifiers = Modifiers {
            shift: dom.shift,
            ctrl: dom.ctrl,
            alt: dom.alt,
            meta: dom.meta,
            command: if self.command_is_meta {
                dom.meta
            } else {
                dom.ctrl
            },
            command_is_meta: self.command_is_meta,
        };
        if modifiers == self.modifiers {
            return vec![];
        }
        self.modifiers = modifiers;
        vec![Event::ModifiersChanged(modifiers)]
    }
}

/// `KeyboardEvent.key` values of the keys [`Key`] names.
fn named_key(key: &str) -> Option<Key> {
    Some(match key {
        "Tab" => Key::Tab,
        "Enter" => Key::Enter,
        " " => Key::Space,
        "Escape" => Key::Escape,
        "Backspace" => Key::Backspace,
        "Delete" => Key::Delete,
        "ArrowLeft" => Key::ArrowLeft,
        "ArrowRight" => Key::ArrowRight,
        "ArrowUp" => Key::ArrowUp,
        "ArrowDown" => Key::ArrowDown,
        "Home" => Key::Home,
        "End" => Key::End,
        "PageUp" => Key::PageUp,
        "PageDown" => Key::PageDown,
        "ContextMenu" => Key::ContextMenu,
        _ => Key::function(key.strip_prefix('F')?.parse().ok()?)?,
    })
}

/// A single character, lowercase as on the desktop, so Cmd+Shift+Z reports 'z'. Other
/// key names (Shift, Meta, ...) are [`Key::Other`].
fn character_key(key: &str) -> Key {
    let mut chars = key.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => Key::Char(c.to_ascii_lowercase()),
        _ => Key::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: &str) -> DomKey<'_> {
        DomKey {
            key,
            key_code: 0,
            composing: false,
            modifiers: DomModifiers::default(),
        }
    }

    fn key_downs(events: &[Event]) -> Vec<Key> {
        events
            .iter()
            .filter_map(|e| match e {
                Event::KeyDown { key, .. } => Some(*key),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn text_keys_report_the_key_and_leave_the_text_to_input_events() {
        let mut input = WebTextInput::new(false);
        let outcome = input.key_down(key("A"));
        assert_eq!(key_downs(&outcome.events), [Key::Char('a')]);
        assert!(!outcome.prevent_default);
        assert!(matches!(
            &input.input("insertText", Some("A"), false)[..],
            [Event::Text(t)] if t == "A"
        ));
    }

    #[test]
    fn named_keys_and_shortcuts_are_not_typed_into_the_text_area() {
        let mut input = WebTextInput::new(true);
        for name in ["Enter", "Tab", "Backspace", "ArrowLeft"] {
            let outcome = input.key_down(key(name));
            assert_eq!(key_downs(&outcome.events).len(), 1, "{name}");
            assert!(outcome.prevent_default, "{name}");
        }
        let space = input.key_down(key(" "));
        assert_eq!(key_downs(&space.events), [Key::Space]);
        assert!(!space.prevent_default);

        let copy = input.key_down(DomKey {
            modifiers: DomModifiers {
                meta: true,
                ..Default::default()
            },
            ..key("c")
        });
        assert!(copy.prevent_default);
        let Some(Event::KeyDown { key, modifiers }) = copy.events.last() else {
            panic!("expected a key, got {:?}", copy.events);
        };
        assert_eq!(*key, Key::Char('c'));
        assert!(modifiers.command && modifiers.meta);
    }

    #[test]
    fn the_command_modifier_is_ctrl_off_apple_platforms() {
        let mut input = WebTextInput::new(false);
        let events = input
            .key_down(DomKey {
                modifiers: DomModifiers {
                    ctrl: true,
                    ..Default::default()
                },
                ..key("Control")
            })
            .events;
        assert!(matches!(
            events[..],
            [Event::ModifiersChanged(m)] if m.command && m.ctrl
        ));
        let released = input.key_up(key("Control"));
        assert!(matches!(
            released[..],
            [Event::ModifiersChanged(m)] if m == Modifiers::default()
        ));
        assert!(input.key_up(key("Control")).is_empty());
    }

    #[test]
    fn keys_the_ime_takes_produce_nothing() {
        let mut input = WebTextInput::new(false);
        for ime_key in [
            DomKey {
                composing: true,
                ..key("Enter")
            },
            // The committing Enter after compositionend, as reported for WebKit.
            DomKey {
                key_code: 229,
                ..key("Enter")
            },
            key("Process"),
        ] {
            let outcome = input.key_down(ime_key);
            assert_eq!(outcome, KeyOutcome::default(), "{ime_key:?}");
        }
    }

    #[test]
    fn a_composition_is_shown_then_committed_once() {
        let input = WebTextInput::new(false);
        assert!(matches!(
            &input.composition_update("にほん")[..],
            [Event::Composition { text, cursor: Some((9, 9)) }] if text == "にほん"
        ));
        // The input events around a composition do not commit the text again
        // (insertCompositionText, and insertFromComposition from Input Events Level 2).
        assert!(
            input
                .input("insertCompositionText", Some("日本"), true)
                .is_empty()
        );
        assert!(
            input
                .input("insertFromComposition", Some("日本"), false)
                .is_empty()
        );
        assert!(input.input("insertText", Some("日本"), true).is_empty());
        assert!(matches!(
            &input.composition_end("日本")[..],
            [Event::Composition { text, cursor: None }, Event::Text(t)]
                if text.is_empty() && t == "日本"
        ));
    }

    #[test]
    fn a_cancelled_composition_commits_nothing() {
        let input = WebTextInput::new(false);
        assert!(matches!(
            &input.composition_end("")[..],
            [Event::Composition { text, cursor: None }] if text.is_empty()
        ));
    }
}

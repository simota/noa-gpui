use crate::event::{Key, Modifiers};

/// A key with modifiers, such as Cmd+S (Ctrl+S outside Apple platforms).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Shortcut {
    pub key: Key,
    /// The platform's command modifier ([`Modifiers::command`]).
    pub command: bool,
    /// The Control key apart from [`Self::command`], as in ⌃⌘F on a Mac. Outside Apple
    /// platforms Ctrl is the command key, so this means the same as `command` there.
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Shortcut {
    pub const fn key(key: Key) -> Self {
        Self {
            key,
            command: false,
            ctrl: false,
            shift: false,
            alt: false,
        }
    }

    /// `c` is a lowercase character key, such as `Shortcut::command('s')`.
    pub const fn command(c: char) -> Self {
        Self {
            key: Key::Char(c),
            command: true,
            ctrl: false,
            shift: false,
            alt: false,
        }
    }

    pub const fn with_shift(self) -> Self {
        Self {
            shift: true,
            ..self
        }
    }

    pub const fn with_alt(self) -> Self {
        Self { alt: true, ..self }
    }

    pub const fn with_ctrl(self) -> Self {
        Self { ctrl: true, ..self }
    }

    /// How the shortcut is written in a menu: Mac glyphs in the Mac order ("⌃⌥⇧⌘S"),
    /// or names joined with "+" ("Ctrl+Alt+Shift+S") elsewhere.
    pub fn label(&self, glyphs: bool) -> String {
        self.keys(glyphs).join(if glyphs { "" } else { "+" })
    }

    /// The keys to press, modifiers first, each as written on its keycap: Mac glyphs in
    /// the Mac order (`["⌃", "⌥", "⇧", "⌘", "S"]`), or names (`["Ctrl", "Alt", "Shift",
    /// "S"]`) elsewhere. See [`crate::widgets::Kbd`].
    pub fn keys(&self, glyphs: bool) -> Vec<String> {
        let key = match (self.key, glyphs) {
            (Key::Char(c), _) => c.to_uppercase().to_string(),
            (Key::Enter, true) => "↩".into(),
            (Key::Enter, false) => "Enter".into(),
            (Key::Tab, true) => "⇥".into(),
            (Key::Tab, false) => "Tab".into(),
            (Key::Space, _) => "Space".into(),
            (Key::Escape, true) => "⎋".into(),
            (Key::Escape, false) => "Esc".into(),
            (Key::Backspace, true) => "⌫".into(),
            (Key::Backspace, false) => "Backspace".into(),
            (Key::Delete, true) => "⌦".into(),
            (Key::Delete, false) => "Del".into(),
            (Key::ArrowLeft, _) => "←".into(),
            (Key::ArrowRight, _) => "→".into(),
            (Key::ArrowUp, _) => "↑".into(),
            (Key::ArrowDown, _) => "↓".into(),
            (Key::Home, true) => "↖".into(),
            (Key::Home, false) => "Home".into(),
            (Key::End, true) => "↘".into(),
            (Key::End, false) => "End".into(),
            (Key::PageUp, true) => "⇞".into(),
            (Key::PageUp, false) => "PgUp".into(),
            (Key::PageDown, true) => "⇟".into(),
            (Key::PageDown, false) => "PgDn".into(),
            (
                Key::F1
                | Key::F2
                | Key::F3
                | Key::F4
                | Key::F5
                | Key::F6
                | Key::F7
                | Key::F8
                | Key::F9
                | Key::F10
                | Key::F11
                | Key::F12,
                _,
            ) => format!("F{}", self.key.function_number().unwrap_or_default()),
            (Key::ContextMenu | Key::Other, _) => String::new(),
        };
        let modifiers: &[(bool, &str, &str)] = if glyphs {
            &[
                (self.ctrl, "⌃", ""),
                (self.alt, "⌥", ""),
                (self.shift, "⇧", ""),
                (self.command, "⌘", ""),
            ]
        } else {
            &[
                (self.command || self.ctrl, "", "Ctrl"),
                (self.alt, "", "Alt"),
                (self.shift, "", "Shift"),
            ]
        };
        modifiers
            .iter()
            .filter(|(on, ..)| *on)
            .map(|&(_, glyph, name)| if glyphs { glyph } else { name }.to_owned())
            .chain((!key.is_empty()).then_some(key))
            .collect()
    }

    pub fn matches(&self, key: Key, modifiers: Modifiers) -> bool {
        let control = if modifiers.command_is_meta {
            self.command == modifiers.command && self.ctrl == modifiers.control()
        } else {
            // Ctrl is the command key, so either flag asks for it.
            (self.command || self.ctrl) == modifiers.command
        };
        self.key == key && control && self.shift == modifiers.shift && self.alt == modifiers.alt
    }
}

/// Emitted when a registered shortcut is pressed and no focused widget handled the key.
/// The message's source is the widget given to [`crate::UiTree::add_shortcut`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShortcutPressed(pub Shortcut);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers_must_match_exactly() {
        let save = Shortcut::command('s');
        let command = Modifiers {
            command: true,
            meta: true,
            ..Default::default()
        };
        assert!(save.matches(Key::Char('s'), command));
        assert!(!save.matches(Key::Char('s'), Modifiers::default()));
        let shifted = Modifiers {
            shift: true,
            ..command
        };
        assert!(!save.matches(Key::Char('s'), shifted));
        assert!(save.with_shift().matches(Key::Char('s'), shifted));
    }

    #[test]
    fn labels_use_mac_glyphs_or_names() {
        let redo = Shortcut::command('z').with_shift();
        assert_eq!(redo.label(true), "⇧⌘Z");
        assert_eq!(redo.label(false), "Ctrl+Shift+Z");
        assert_eq!(Shortcut::key(Key::Delete).label(true), "⌦");
        assert_eq!(Shortcut::command('w').with_alt().label(true), "⌥⌘W");
        assert_eq!(Shortcut::command('w').with_alt().label(false), "Ctrl+Alt+W");
        // The Mac order is ⌥⇧⌘.
        let all = Shortcut::command('z').with_shift().with_alt();
        assert_eq!(all.label(true), "⌥⇧⌘Z");
        assert_eq!(all.label(false), "Ctrl+Alt+Shift+Z");
    }

    #[test]
    fn control_is_its_own_key_on_a_mac_and_the_command_key_elsewhere() {
        let find = Shortcut::command('f').with_ctrl();
        let mac_cmd = Modifiers {
            command: true,
            meta: true,
            ..Modifiers::on_apple()
        };
        let mac_ctrl_cmd = Modifiers {
            ctrl: true,
            ..mac_cmd
        };
        assert!(find.matches(Key::Char('f'), mac_ctrl_cmd));
        assert!(!find.matches(Key::Char('f'), mac_cmd));
        assert!(!Shortcut::command('f').matches(Key::Char('f'), mac_ctrl_cmd));
        let mac_ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::on_apple()
        };
        let tab = Shortcut::key(Key::Tab).with_ctrl();
        assert!(tab.matches(Key::Tab, mac_ctrl));
        assert!(!tab.matches(Key::Tab, Modifiers::default()));
        assert!(!Shortcut::key(Key::Tab).matches(Key::Tab, mac_ctrl));

        // Elsewhere the platform sets `command` with Ctrl.
        let ctrl = Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        assert!(find.matches(Key::Char('f'), ctrl));
        assert!(tab.matches(Key::Tab, ctrl));
        assert!(Shortcut::command('f').matches(Key::Char('f'), ctrl));
        assert!(!tab.matches(Key::Tab, Modifiers::default()));

        assert_eq!(find.label(true), "⌃⌘F");
        assert_eq!(find.label(false), "Ctrl+F");
        assert_eq!(tab.with_shift().keys(true), ["⌃", "⇧", "⇥"]);
    }

    /// Outside Apple platforms the Super (Windows) key is not Cmd: holding it with
    /// Ctrl still presses the Ctrl shortcut's key with an extra modifier, and a
    /// recorder takes Ctrl as the command key.
    #[test]
    fn ctrl_with_super_elsewhere_is_not_a_mac_command() {
        let ctrl_super = Modifiers {
            ctrl: true,
            meta: true,
            command: true,
            ..Modifiers::elsewhere()
        };
        assert!(!ctrl_super.control(), "Ctrl is the command key here");
        assert!(Shortcut::command('s').matches(Key::Char('s'), ctrl_super));
        // A Ctrl-only shortcut is the same as the command one there.
        let tab = Shortcut::key(Key::Tab).with_ctrl();
        let ctrl = Modifiers {
            ctrl: true,
            command: true,
            ..Modifiers::elsewhere()
        };
        assert!(tab.matches(Key::Tab, ctrl));
        assert!(Shortcut::command('s').matches(Key::Char('s'), ctrl));
        // On a Mac the same keys are Ctrl+Cmd.
        let mac = Modifiers {
            ctrl: true,
            meta: true,
            command: true,
            ..Modifiers::on_apple()
        };
        assert!(mac.control());
    }

    #[test]
    fn function_keys_are_named_by_number() {
        assert_eq!(Shortcut::key(Key::F2).label(true), "F2");
        assert_eq!(
            Shortcut::key(Key::F12).with_shift().label(false),
            "Shift+F12"
        );
        for n in 1..=12 {
            let key = Key::function(n).unwrap();
            assert_eq!(key.function_number(), Some(n));
        }
        assert_eq!(Key::function(0), None);
        assert_eq!(Key::function(13), None);
        assert_eq!(Key::Char('f').function_number(), None);
    }

    #[test]
    fn keys_are_one_keycap_each() {
        let palette = Shortcut::command('p').with_shift();
        assert_eq!(palette.keys(true), ["⇧", "⌘", "P"]);
        assert_eq!(palette.keys(false), ["Ctrl", "Shift", "P"]);
        assert_eq!(Shortcut::key(Key::Escape).keys(false), ["Esc"]);
    }
}

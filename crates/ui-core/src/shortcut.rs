use crate::event::{Key, Modifiers};

/// A key with modifiers, such as Cmd+S (Ctrl+S outside Apple platforms).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Shortcut {
    pub key: Key,
    /// The platform's command modifier ([`Modifiers::command`]).
    pub command: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Shortcut {
    pub const fn key(key: Key) -> Self {
        Self {
            key,
            command: false,
            shift: false,
            alt: false,
        }
    }

    /// `c` is a lowercase character key, such as `Shortcut::command('s')`.
    pub const fn command(c: char) -> Self {
        Self {
            key: Key::Char(c),
            command: true,
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

    /// How the shortcut is written in a menu: Mac glyphs in the Mac order ("⌥⇧⌘S"),
    /// or names joined with "+" ("Ctrl+Alt+Shift+S") elsewhere.
    pub fn label(&self, glyphs: bool) -> String {
        self.keys(glyphs).join(if glyphs { "" } else { "+" })
    }

    /// The keys to press, modifiers first, each as written on its keycap: Mac glyphs in
    /// the Mac order (`["⌥", "⇧", "⌘", "S"]`), or names (`["Ctrl", "Alt", "Shift",
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
            (Key::F10, _) => "F10".into(),
            (Key::ContextMenu | Key::Other, _) => String::new(),
        };
        let modifiers: &[(bool, &str, &str)] = if glyphs {
            &[
                (self.alt, "⌥", ""),
                (self.shift, "⇧", ""),
                (self.command, "⌘", ""),
            ]
        } else {
            &[
                (self.command, "", "Ctrl"),
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
        self.key == key
            && self.command == modifiers.command
            && self.shift == modifiers.shift
            && self.alt == modifiers.alt
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
    fn keys_are_one_keycap_each() {
        let palette = Shortcut::command('p').with_shift();
        assert_eq!(palette.keys(true), ["⇧", "⌘", "P"]);
        assert_eq!(palette.keys(false), ["Ctrl", "Shift", "P"]);
        assert_eq!(Shortcut::key(Key::Escape).keys(false), ["Esc"]);
    }
}

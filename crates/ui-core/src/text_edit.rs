//! [`TextEditor`] knows nothing about layout. Positions are byte indices into the text,
//! always on grapheme cluster boundaries, so emoji sequences and combining characters
//! are never split. Movement that depends on layout (up / down between visual lines,
//! the ends of a wrapped line) is done by the widget, which then calls
//! [`TextEditor::move_to`].

use std::ops::Range;

use unicode_segmentation::{GraphemeCursor, UnicodeSegmentation};

use crate::clipboard::Clipboard;
use crate::event::{Key, Modifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeyOutcome {
    /// The key should not bubble further.
    pub handled: bool,
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Snapshot {
    text: String,
    anchor: usize,
    caret: usize,
}

/// The last edit's kind, so consecutive typing undoes as one step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditKind {
    Typing,
    Deleting,
}

const MAX_UNDO: usize = 200;

#[derive(Debug, Clone, Default)]
pub struct TextEditor {
    text: String,
    /// Where the selection started; equals `caret` when nothing is selected.
    anchor: usize,
    caret: usize,
    multiline: bool,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// The last edit and the caret right after it, for coalescing.
    last_edit: Option<(EditKind, usize)>,
}

impl TextEditor {
    /// A single-line editor: pasted line breaks become spaces, Enter is not handled.
    pub fn single_line() -> Self {
        Self::default()
    }

    /// A multi-line editor: Enter inserts a line break.
    pub fn multi_line() -> Self {
        Self {
            multiline: true,
            ..Self::default()
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_multiline(&self) -> bool {
        self.multiline
    }

    /// The moving end of the selection.
    pub fn caret(&self) -> usize {
        self.caret
    }

    /// The fixed end of the selection.
    pub fn anchor(&self) -> usize {
        self.anchor
    }

    /// Empty when nothing is selected.
    pub fn selection(&self) -> Range<usize> {
        self.anchor.min(self.caret)..self.anchor.max(self.caret)
    }

    pub fn has_selection(&self) -> bool {
        self.anchor != self.caret
    }

    pub fn selected_text(&self) -> &str {
        &self.text[self.selection()]
    }

    /// Puts the caret at the end and clears the history.
    pub fn set_text(&mut self, text: &str) {
        self.text = self.clean(text);
        self.caret = self.text.len();
        self.anchor = self.caret;
        self.undo.clear();
        self.redo.clear();
        self.last_edit = None;
    }

    /// `range` is clamped and snapped to grapheme boundaries; the caret goes to its end.
    pub fn select(&mut self, range: Range<usize>) {
        self.anchor = self.snap(range.start);
        self.caret = self.snap(range.end);
        self.last_edit = None;
    }

    pub fn select_all(&mut self) {
        self.select(0..self.text.len());
    }

    /// Selects the word, or run of spaces or punctuation, around `index`.
    pub fn select_word_at(&mut self, index: usize) {
        let index = self.snap(index);
        let word = self
            .text
            .split_word_bound_indices()
            .map(|(start, word)| start..start + word.len())
            .find(|r| r.contains(&index) || (r.end == index && index == self.text.len()));
        if let Some(range) = word {
            self.select(range);
        }
    }

    pub fn move_to(&mut self, index: usize, extend: bool) {
        self.caret = self.snap(index);
        if !extend {
            self.anchor = self.caret;
        }
        self.last_edit = None;
    }

    pub fn insert(&mut self, input: &str) -> bool {
        let input = self.clean(input);
        if input.is_empty() && !self.has_selection() {
            return false;
        }
        let range = self.selection();
        let coalesce = !self.has_selection()
            && !input.is_empty()
            && self.last_edit == Some((EditKind::Typing, self.caret))
            && !input.contains(char::is_whitespace);
        self.record(coalesce);
        self.text.replace_range(range.clone(), &input);
        self.caret = self.snap_after_edit(range.start + input.len());
        self.anchor = self.caret;
        // Whitespace ends a word, so the next word typed is its own undo step.
        self.last_edit =
            (!input.contains(char::is_whitespace)).then_some((EditKind::Typing, self.caret));
        true
    }

    /// Delete the selection, or the grapheme (word with `word`) before the caret.
    pub fn backspace(&mut self, word: bool) -> bool {
        if self.has_selection() {
            return self.delete_range(self.selection());
        }
        let start = if word {
            self.word_start_before(self.caret)
        } else {
            prev_boundary(&self.text, self.caret).unwrap_or(self.caret)
        };
        self.delete_coalesced(start..self.caret)
    }

    /// Delete the selection, or the grapheme (word with `word`) after the caret.
    pub fn delete_forward(&mut self, word: bool) -> bool {
        if self.has_selection() {
            return self.delete_range(self.selection());
        }
        let end = if word {
            self.word_end_after(self.caret)
        } else {
            next_boundary(&self.text, self.caret).unwrap_or(self.caret)
        };
        self.delete_coalesced(self.caret..end)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        let current = self.snapshot();
        self.redo.push(current);
        self.restore(previous);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        let current = self.snapshot();
        self.undo.push(current);
        self.restore(next);
        true
    }

    /// Returns whether anything was copied.
    pub fn copy(&self, clipboard: &mut dyn Clipboard) -> bool {
        if !self.has_selection() {
            return false;
        }
        clipboard.write_text(self.selected_text());
        true
    }

    pub fn cut(&mut self, clipboard: &mut dyn Clipboard) -> bool {
        self.copy(clipboard) && self.delete_range(self.selection())
    }

    pub fn paste(&mut self, clipboard: &mut dyn Clipboard) -> bool {
        match clipboard.read_text() {
            Some(text) => {
                // A paste is its own undo step.
                self.last_edit = None;
                let changed = self.insert(&text);
                self.last_edit = None;
                changed
            }
            None => false,
        }
    }

    /// Keys that need layout (Up / Down in multi-line text) are not handled here.
    ///
    /// Word movement is Option+Arrow on Apple platforms and Ctrl+Arrow elsewhere; line
    /// movement is Cmd+Arrow (Apple) or Home / End.
    pub fn handle_key(
        &mut self,
        key: Key,
        modifiers: Modifiers,
        clipboard: &mut dyn Clipboard,
    ) -> KeyOutcome {
        // On Apple platforms `command` is Cmd (meta); elsewhere it is Ctrl.
        let apple = modifiers.command && modifiers.meta;
        let word = modifiers.alt || (modifiers.command && !apple);
        let line = apple;
        let extend = modifiers.shift;
        let moved = |handled: bool| KeyOutcome {
            handled,
            changed: false,
        };
        let edited = |changed: bool| KeyOutcome {
            handled: true,
            changed,
        };
        match key {
            Key::ArrowLeft | Key::ArrowRight => {
                let forward = key == Key::ArrowRight;
                let target = if line {
                    if forward {
                        self.line_end(self.caret)
                    } else {
                        self.line_start(self.caret)
                    }
                } else if word {
                    if forward {
                        self.word_end_after(self.caret)
                    } else {
                        self.word_start_before(self.caret)
                    }
                } else if self.has_selection() && !extend {
                    let r = self.selection();
                    if forward { r.end } else { r.start }
                } else if forward {
                    next_boundary(&self.text, self.caret).unwrap_or(self.caret)
                } else {
                    prev_boundary(&self.text, self.caret).unwrap_or(self.caret)
                };
                self.move_to(target, extend);
                moved(true)
            }
            Key::ArrowUp | Key::ArrowDown if !self.multiline => {
                let target = if key == Key::ArrowUp {
                    0
                } else {
                    self.text.len()
                };
                self.move_to(target, extend);
                moved(true)
            }
            Key::Home => {
                self.move_to(self.line_start(self.caret), extend);
                moved(true)
            }
            Key::End => {
                self.move_to(self.line_end(self.caret), extend);
                moved(true)
            }
            Key::Backspace if line && !self.has_selection() => {
                let start = self.line_start(self.caret);
                edited(self.delete_range(start..self.caret))
            }
            Key::Backspace => edited(self.backspace(word)),
            Key::Delete => edited(self.delete_forward(word)),
            Key::Enter if self.multiline => {
                self.last_edit = None;
                edited(self.insert("\n"))
            }
            Key::Char(c) if modifiers.command => match c {
                'a' => {
                    self.select_all();
                    moved(true)
                }
                'c' => {
                    self.copy(clipboard);
                    moved(true)
                }
                'x' => edited(self.cut(clipboard)),
                'v' => edited(self.paste(clipboard)),
                'z' if modifiers.shift => edited(self.redo()),
                'z' => edited(self.undo()),
                'y' if !apple => edited(self.redo()),
                _ => moved(false),
            },
            _ => moved(false),
        }
    }

    /// Start of the paragraph containing `index`.
    pub fn line_start(&self, index: usize) -> usize {
        self.text[..index].rfind('\n').map_or(0, |i| i + 1)
    }

    /// End of the paragraph containing `index` (before its line break).
    pub fn line_end(&self, index: usize) -> usize {
        self.text[index..]
            .find('\n')
            .map_or(self.text.len(), |i| index + i)
    }

    /// The paragraph starting at byte `start` (a paragraph start), with its line
    /// break: `start..end`.
    fn paragraph_from(&self, start: usize) -> Range<usize> {
        let end = self.text[start..]
            .find('\n')
            .map_or(self.text.len(), |i| start + i + 1);
        start..end
    }

    // Words never span a line feed (UAX #29 always breaks after one), so the words of
    // the whole text are those of its paragraphs. Look only at the paragraphs near
    // `index`, not at all the text before it.

    fn word_start_before(&self, index: usize) -> usize {
        let mut start = self.line_start(index);
        loop {
            let paragraph = self.paragraph_from(start);
            let found = self.text[paragraph]
                .unicode_word_indices()
                .map(|(i, _)| start + i)
                .take_while(|&s| s < index)
                .last();
            match found {
                Some(found) => return found,
                None if start == 0 => return 0,
                None => start = self.line_start(start - 1),
            }
        }
    }

    fn word_end_after(&self, index: usize) -> usize {
        let mut paragraph = self.paragraph_from(self.line_start(index));
        loop {
            let start = paragraph.start;
            let found = self.text[paragraph.clone()]
                .unicode_word_indices()
                .map(|(i, word)| start + i + word.len())
                .find(|&end| end > index);
            match found {
                Some(found) => return found,
                None if paragraph.end == self.text.len() => return self.text.len(),
                None => paragraph = self.paragraph_from(paragraph.end),
            }
        }
    }

    fn delete_range(&mut self, range: Range<usize>) -> bool {
        if range.is_empty() {
            return false;
        }
        self.record(false);
        self.text.replace_range(range.clone(), "");
        self.caret = self.snap_after_edit(range.start);
        self.anchor = self.caret;
        self.last_edit = None;
        true
    }

    /// Merges consecutive single deletions into one undo step.
    fn delete_coalesced(&mut self, range: Range<usize>) -> bool {
        if range.is_empty() {
            return false;
        }
        let coalesce = self.last_edit == Some((EditKind::Deleting, self.caret));
        self.record(coalesce);
        self.text.replace_range(range.clone(), "");
        self.caret = self.snap_after_edit(range.start);
        self.anchor = self.caret;
        self.last_edit = Some((EditKind::Deleting, self.caret));
        true
    }

    fn record(&mut self, coalesce: bool) {
        self.redo.clear();
        if coalesce {
            return;
        }
        let snapshot = self.snapshot();
        self.undo.push(snapshot);
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.text.clone(),
            anchor: self.anchor,
            caret: self.caret,
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.text = snapshot.text;
        self.anchor = snapshot.anchor;
        self.caret = snapshot.caret;
        self.last_edit = None;
    }

    /// Strips control characters, turns `\r\n` into `\n`, and in single-line mode turns
    /// line breaks into spaces.
    fn clean(&self, input: &str) -> String {
        let input = input.replace("\r\n", "\n").replace('\r', "\n");
        input
            .chars()
            .filter_map(|c| match c {
                '\n' if self.multiline => Some('\n'),
                '\n' | '\t' => Some(' '),
                c if c.is_control() => None,
                c => Some(c),
            })
            .collect()
    }

    /// Clamps `index` to the text and moves it back to a grapheme boundary.
    fn snap(&self, index: usize) -> usize {
        let mut index = index.min(self.text.len());
        while !self.text.is_char_boundary(index) {
            index -= 1;
        }
        let mut cursor = GraphemeCursor::new(index, self.text.len(), true);
        if cursor.is_boundary(&self.text, 0).unwrap_or(true) {
            index
        } else {
            prev_boundary(&self.text, index).unwrap_or(0)
        }
    }

    /// An edit may join the graphemes on either side of it (a deleted line break before
    /// a combining mark, or an inserted emoji joiner). Keeping the caret after that whole
    /// cluster stops the next deletion from splitting it.
    fn snap_after_edit(&self, index: usize) -> usize {
        let start = self.snap(index);
        if start == index {
            index
        } else {
            next_boundary(&self.text, start).unwrap_or(self.text.len())
        }
    }
}

pub(crate) fn prev_boundary(text: &str, at: usize) -> Option<usize> {
    GraphemeCursor::new(at, text.len(), true)
        .prev_boundary(text, 0)
        .ok()
        .flatten()
}

pub(crate) fn next_boundary(text: &str, at: usize) -> Option<usize> {
    GraphemeCursor::new(at, text.len(), true)
        .next_boundary(text, 0)
        .ok()
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard::MemoryClipboard;

    fn with(text: &str) -> TextEditor {
        let mut e = TextEditor::single_line();
        e.set_text(text);
        e
    }

    fn mods(f: impl FnOnce(&mut Modifiers)) -> Modifiers {
        let mut m = Modifiers::default();
        f(&mut m);
        m
    }

    const APPLE_COMMAND: Modifiers = Modifiers {
        shift: false,
        ctrl: false,
        alt: false,
        meta: true,
        command: true,
        command_is_meta: true,
    };

    #[test]
    fn typing_replaces_the_selection_and_undoes_as_one_step() {
        let mut e = with("hello world");
        e.select(0..5);
        for c in ["H", "i"] {
            e.insert(c);
        }
        assert_eq!(e.text(), "Hi world");
        assert_eq!(e.caret(), 2);
        assert!(e.undo());
        assert_eq!(e.text(), "hello world", "typing coalesces into one step");
        assert_eq!(e.selection(), 0..5, "undo restores the selection");
        assert!(e.redo());
        assert_eq!(e.text(), "Hi world");
        assert!(!e.redo());
    }

    #[test]
    fn a_space_or_a_caret_move_starts_a_new_undo_step() {
        let mut e = with("");
        e.insert("a");
        e.insert("b");
        e.insert(" ");
        e.insert("c");
        assert!(e.undo());
        assert_eq!(e.text(), "ab ");
        assert!(e.undo());
        assert_eq!(e.text(), "ab");
        e.move_to(0, false);
        e.insert("x");
        e.move_to(3, false);
        e.insert("y");
        assert!(e.undo());
        assert_eq!(e.text(), "xab");
    }

    #[test]
    fn shift_extends_and_plain_arrows_collapse_the_selection() {
        let mut clip = MemoryClipboard::default();
        let mut e = with("abcdef");
        e.move_to(2, false);
        let shift = mods(|m| m.shift = true);
        e.handle_key(Key::ArrowRight, shift, &mut clip);
        e.handle_key(Key::ArrowRight, shift, &mut clip);
        assert_eq!(e.selection(), 2..4);
        assert_eq!(e.selected_text(), "cd");
        e.handle_key(Key::ArrowLeft, Modifiers::default(), &mut clip);
        assert_eq!((e.caret(), e.has_selection()), (2, false));
    }

    #[test]
    fn word_and_line_movement_follow_platform_conventions() {
        let mut clip = MemoryClipboard::default();
        let mut e = with("one two  three");
        let alt = mods(|m| m.alt = true);
        e.handle_key(Key::ArrowLeft, alt, &mut clip);
        assert_eq!(e.caret(), 9, "Option+Left: start of 'three'");
        e.handle_key(Key::ArrowLeft, alt, &mut clip);
        assert_eq!(e.caret(), 4);
        e.handle_key(Key::ArrowRight, alt, &mut clip);
        assert_eq!(e.caret(), 7, "Option+Right: end of 'two'");
        // Ctrl+Arrow (command off Apple) moves by word too.
        let ctrl = mods(|m| {
            m.ctrl = true;
            m.command = true;
        });
        e.handle_key(Key::ArrowLeft, ctrl, &mut clip);
        assert_eq!(e.caret(), 4);
        // Cmd+Arrow on Apple goes to the line ends.
        e.handle_key(Key::ArrowRight, APPLE_COMMAND, &mut clip);
        assert_eq!(e.caret(), e.text().len());
        e.handle_key(Key::ArrowLeft, APPLE_COMMAND, &mut clip);
        assert_eq!(e.caret(), 0);
    }

    #[test]
    fn clipboard_shortcuts_copy_cut_and_paste() {
        let mut clip = MemoryClipboard::default();
        let mut e = with("copy me");
        let key = |e: &mut TextEditor, c, clip: &mut MemoryClipboard| {
            e.handle_key(Key::Char(c), APPLE_COMMAND, clip)
        };
        assert!(!key(&mut e, 'a', &mut clip).changed);
        assert_eq!(e.selection(), 0..7);
        key(&mut e, 'c', &mut clip);
        assert_eq!(clip.read_text().as_deref(), Some("copy me"));
        e.select(4..7);
        assert!(key(&mut e, 'x', &mut clip).changed);
        assert_eq!(e.text(), "copy");
        assert_eq!(clip.read_text().as_deref(), Some(" me"));
        e.move_to(0, false);
        assert!(key(&mut e, 'v', &mut clip).changed);
        assert_eq!(e.text(), " mecopy");
        let undo = key(&mut e, 'z', &mut clip);
        assert!(undo.changed);
        assert_eq!(e.text(), "copy");
        let redo = e.handle_key(
            Key::Char('z'),
            Modifiers {
                shift: true,
                ..APPLE_COMMAND
            },
            &mut clip,
        );
        assert!(redo.changed);
        assert_eq!(e.text(), " mecopy");
        assert!(!key(&mut e, 's', &mut clip).handled);
    }

    #[test]
    fn single_line_flattens_pasted_line_breaks_and_multi_line_keeps_them() {
        let mut single = with("");
        single.insert("a\r\nb\tc\u{7}");
        assert_eq!(single.text(), "a b c");
        let mut multi = TextEditor::multi_line();
        multi.insert("a\r\nb");
        assert_eq!(multi.text(), "a\nb");
        let mut clip = MemoryClipboard::default();
        assert!(
            multi
                .handle_key(Key::Enter, Modifiers::default(), &mut clip)
                .changed
        );
        assert_eq!(multi.text(), "a\nb\n");
        assert!(
            !single
                .handle_key(Key::Enter, Modifiers::default(), &mut clip)
                .handled
        );
    }

    #[test]
    fn deletion_respects_graphemes_words_and_lines() {
        let mut e = with("a👍🏽 b");
        e.move_to(e.text().find(' ').unwrap(), false);
        assert!(e.backspace(false));
        assert_eq!(e.text(), "a b", "the emoji with its skin tone goes as one");
        let mut e = with("one two three");
        assert!(e.backspace(true));
        assert_eq!(e.text(), "one two ");
        let mut multi = TextEditor::multi_line();
        multi.insert("first\nsecond line");
        let mut clip = MemoryClipboard::default();
        multi.handle_key(Key::Backspace, APPLE_COMMAND, &mut clip);
        assert_eq!(
            multi.text(),
            "first\n",
            "Cmd+Backspace deletes to the line start"
        );
        let mut e = with("abcd");
        e.backspace(false);
        e.backspace(false);
        e.undo();
        assert_eq!(e.text(), "abcd");
    }

    #[test]
    fn double_click_word_selection_and_snapping() {
        let mut e = with("hello, world");
        e.select_word_at(9);
        assert_eq!(e.selected_text(), "world");
        e.select_word_at(5);
        assert_eq!(e.selected_text(), ",");
        let mut e = with("👍🏽x");
        e.move_to(3, false);
        assert_eq!(e.caret(), 0, "snapped back to a grapheme boundary");
    }

    #[test]
    fn word_moves_match_the_words_of_the_whole_text() {
        // The words of the whole text, as word moves found them before looking only at
        // the paragraphs near the caret.
        let start_before = |text: &str, index: usize| {
            (text.unicode_word_indices())
                .map(|(start, _)| start)
                .take_while(|&start| start < index)
                .last()
                .unwrap_or(0)
        };
        let end_after = |text: &str, index: usize| {
            (text.unicode_word_indices())
                .map(|(start, word)| start + word.len())
                .find(|&end| end > index)
                .unwrap_or(text.len())
        };
        let texts = [
            "",
            "\n",
            "\n\n\n",
            "word",
            "  two words  ",
            "one\ntwo\r\nthree\n",
            "\n\n  \n...\nlast",
            "first\n\n\n, ;\n\nend\n\n",
            "über e\u{301}clair 🇯🇵🇯🇵 x\n👍🏽 can't 3.14\r\n日本語の文\n",
            "a\u{301}\n\u{301}b\n\u{200d}c",
            "lone\rreturn\n\rx",
        ];
        for text in texts {
            let mut editor = TextEditor::multi_line();
            editor.set_text(text);
            // As the editor keeps it (line breaks normalized).
            let text = editor.text();
            for index in (0..=text.len()).filter(|&i| text.is_char_boundary(i)) {
                assert_eq!(
                    editor.word_start_before(index),
                    start_before(text, index),
                    "word start before {index} in {text:?}"
                );
                assert_eq!(
                    editor.word_end_after(index),
                    end_after(text, index),
                    "word end after {index} in {text:?}"
                );
            }
        }
    }
}

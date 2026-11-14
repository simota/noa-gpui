use ui_core::style::{FlexDirection, Size as LayoutSize, percent};
use ui_core::widgets::{Button, Container, TermColor, Terminal, TerminalInput};
use ui_core::{Event, Key, Message, Modifiers, Style, UiTree, WidgetId};
use ui_scene::Size;
use ui_text::{FontOptions, TextStore};

struct Harness {
    tree: UiTree,
    text: TextStore,
    term: WidgetId,
    viewport: Size,
}

impl Harness {
    fn new(terminal: Terminal) -> Self {
        let mut tree = UiTree::new();
        let root = tree.insert(
            None,
            Container::new(Style {
                flex_direction: FlexDirection::Column,
                size: LayoutSize {
                    width: percent(1.0),
                    height: percent(1.0),
                },
                ..Default::default()
            }),
        );
        let term = tree.insert(Some(root), terminal);
        let mut h = Self {
            tree,
            text: TextStore::new(FontOptions {
                locale: "en-US".into(),
                load_system_fonts: false,
            }),
            term,
            viewport: Size::new(640.0, 400.0),
        };
        h.prepare();
        h.tree.set_focus(Some(term));
        h
    }

    fn prepare(&mut self) {
        self.tree.prepare(self.viewport, &mut self.text);
    }

    fn feed(&mut self, bytes: &[u8]) -> Vec<u8> {
        let replies = self
            .tree
            .update::<Terminal, _>(self.term, |t, cx| t.feed(cx, bytes))
            .unwrap();
        self.prepare();
        replies
    }

    fn term(&self) -> &Terminal {
        self.tree.get::<Terminal>(self.term).unwrap()
    }

    fn line(&self, row: usize) -> String {
        self.term().line_text(row)
    }

    fn key_with(&mut self, key: Key, modifiers: Modifiers) -> Vec<u8> {
        self.tree
            .dispatch(Event::ModifiersChanged(modifiers), &mut self.text);
        self.tree
            .dispatch(Event::KeyDown { key, modifiers }, &mut self.text);
        self.tree.dispatch(
            Event::ModifiersChanged(Modifiers::default()),
            &mut self.text,
        );
        self.input()
    }

    fn key(&mut self, key: Key) -> Vec<u8> {
        self.key_with(key, Modifiers::default())
    }

    fn input(&mut self) -> Vec<u8> {
        self.tree
            .take_messages()
            .iter()
            .filter_map(Message::downcast_ref::<TerminalInput>)
            .flat_map(|TerminalInput(b)| b.clone())
            .collect()
    }
}

#[test]
fn text_wraps_at_the_last_column_and_lines_scroll_into_the_scrollback() {
    let mut h = Harness::new(Terminal::new().rows(4));
    let (cols, rows) = h.term().grid_size();
    assert_eq!(rows, 4);
    assert!(cols > 20, "{cols} columns");
    h.feed(b"hello\r\nworld");
    assert_eq!((h.line(0), h.line(1)), ("hello".into(), "world".into()));
    assert_eq!(h.term().cursor(), (5, 1));

    // A full line leaves the cursor on the last column until the next character.
    let full = "x".repeat(cols);
    h.feed(format!("\r\n{full}").as_bytes());
    assert_eq!(h.term().cursor(), (cols - 1, 2));
    h.feed(b"y");
    assert_eq!(h.line(3), "y");

    // Two more lines push the first two into the scrollback.
    h.feed(b"\r\nmore\r\nlast");
    assert_eq!(h.term().scrollback_len(), 2);
    assert_eq!(h.line(0), full);
    assert_eq!(h.line(3), "last");
}

#[test]
fn escape_sequences_move_the_cursor_erase_and_report_its_position() {
    let mut h = Harness::new(Terminal::new().rows(5));
    h.feed(b"abcdef\r\nghijkl");
    // Row 1, column 3 (1-based), then erase to the end of the line.
    h.feed(b"\x1b[1;3H\x1b[K");
    assert_eq!(h.line(0), "ab");
    // Delete two characters, insert one blank.
    h.feed(b"\x1b[2;2H\x1b[2P");
    assert_eq!(h.line(1), "gjkl");
    h.feed(b"\x1b[@");
    assert_eq!(h.line(1), "g jkl");
    // Erase the whole display.
    h.feed(b"\x1b[2J");
    assert_eq!((h.line(0), h.line(1)), (String::new(), String::new()));
    // A cursor position report goes back to the program.
    let reply = h.feed(b"\x1b[3;4H\x1b[6n");
    assert_eq!(reply, b"\x1b[3;4R");
    assert_eq!(h.term().cursor(), (3, 2));
}

#[test]
fn sgr_sets_named_256_and_true_colors_and_resets() {
    let mut h = Harness::new(Terminal::new().rows(3));
    h.feed(b"\x1b[1;31mR\x1b[38;5;208mO\x1b[48;2;1;2;3mB\x1b[0mN\x1b[7mI");
    let t = h.term();
    let r = t.cell(0, 0).style;
    assert!(r.bold);
    assert_eq!(r.fg, TermColor::Indexed(1));
    assert_eq!(t.cell(1, 0).style.fg, TermColor::Indexed(208));
    assert_eq!(t.cell(2, 0).style.bg, TermColor::Rgb(1, 2, 3));
    assert_eq!(t.cell(3, 0).style, Default::default());
    assert!(t.cell(4, 0).style.inverse);
}

#[test]
fn full_screen_programs_draw_on_the_alternate_screen_and_restore_the_shell() {
    let mut h = Harness::new(Terminal::new().rows(4));
    h.feed(b"$ vim\r\n");
    h.feed(b"\x1b[?1049h\x1b[H\x1b[2Jediting\x1b]2;vim notes\x07");
    assert!(h.term().is_alternate_screen());
    assert_eq!(h.line(0), "editing");
    assert_eq!(h.term().title(), "vim notes");
    // Lines scrolled off the alternate screen are not kept.
    h.feed(b"\r\n\r\n\r\n\r\n\r\n");
    assert_eq!(h.term().scrollback_len(), 0);
    h.feed(b"\x1b[?1049l");
    assert!(!h.term().is_alternate_screen());
    assert_eq!(h.line(0), "$ vim");
    assert_eq!(h.term().cursor(), (0, 1));
}

#[test]
fn a_scroll_region_scrolls_only_its_lines() {
    let mut h = Harness::new(Terminal::new().rows(5));
    h.feed(b"head\r\n1\r\n2\r\n3\r\nfoot");
    // Rows 2–4 scroll; the head and foot stay.
    h.feed(b"\x1b[2;4r\x1b[4;1H\n");
    assert_eq!(
        (0..5).map(|r| h.line(r)).collect::<Vec<_>>(),
        ["head", "2", "3", "", "foot"]
    );
    // Insert a line at row 2: the region's last line drops out.
    h.feed(b"\x1b[2;1H\x1b[L");
    assert_eq!(
        (0..5).map(|r| h.line(r)).collect::<Vec<_>>(),
        ["head", "", "2", "3", "foot"]
    );
}

#[test]
fn utf8_split_across_reads_and_wide_characters_take_two_cells() {
    let mut h = Harness::new(Terminal::new().rows(2));
    let bytes = "日本a".as_bytes();
    h.feed(&bytes[..2]);
    h.feed(&bytes[2..]);
    assert_eq!(h.line(0), "日本a");
    assert_eq!(h.term().cursor(), (5, 0));
}

#[test]
fn excessive_csi_parameters_are_discarded_until_the_sequence_ends() {
    let mut h = Harness::new(Terminal::new().rows(3));
    h.feed(b"\x1b[31");
    // Feed the unfinished sequence across reads, as a PTY would deliver it.
    for _ in 0..4 {
        h.feed(&[b';'; 4096]);
    }
    h.feed(b"1mX\x1b[32mY");
    assert_eq!(h.line(0), "XY");
    assert_eq!(h.term().cell(0, 0).style, Default::default());
    assert_eq!(h.term().cell(1, 0).style.fg, TermColor::Indexed(2));

    // ESC cancels an ignored sequence and starts a fresh control sequence.
    h.feed(b"\x1b[");
    h.feed(&[b';'; 4096]);
    h.feed(b"\x1b[0mZ");
    assert_eq!(h.line(0), "XYZ");
    assert_eq!(h.term().cell(2, 0).style, Default::default());
}

#[test]
fn restoring_a_cursor_restores_origin_mode_and_respects_changed_margins() {
    let mut h = Harness::new(Terminal::new().rows(5));
    // Save in absolute mode, then restore after enabling relative origin mode.
    h.feed(b"\x1b7\x1b[2;4r\x1b[?6h\x1b8");
    assert_eq!(h.term().cursor(), (0, 0));
    assert_eq!(h.feed(b"\x1b[6n"), b"\x1b[1;1R");

    // Save in origin mode, then move the region below that saved position.
    h.feed(b"\x1b[?6h\x1b7\x1b[3;5r\x1b8");
    assert_eq!(h.term().cursor(), (0, 2));
    assert_eq!(h.feed(b"\x1b[6n"), b"\x1b[1;1R");
    h.feed(b"\x1b[?6l\x1b8");
    assert_eq!(h.feed(b"\x1b[6n"), b"\x1b[1;1R");
}

#[test]
fn overwriting_either_half_of_a_wide_character_keeps_cell_positions() {
    let mut h = Harness::new(Terminal::new().rows(3));
    h.feed("日本a\rX".as_bytes());
    assert_eq!(h.line(0), "X 本a");
    assert_eq!(h.term().cell(1, 0).ch, ' ');

    h.feed("\r日本a\x1b[1;2HX".as_bytes());
    assert_eq!(h.line(0), " X本a");
    assert_eq!(h.term().cell(0, 0).ch, ' ');

    // A wide replacement can overlap the trailing half of one character and the
    // leading half of another. Both old characters must be erased completely.
    h.feed("\r日本a\x1b[1;2H語".as_bytes());
    assert_eq!(h.line(0), " 語 a");
    assert_eq!(h.term().cell(3, 0).ch, ' ');
}

#[test]
fn erasing_inserting_and_deleting_preserve_wide_character_boundaries() {
    let mut h = Harness::new(Terminal::new().rows(3));
    for erase in ["\x1b[X", "\x1b[1K", "\x1b[1J"] {
        h.feed("\x1b[2J\x1b[H日本a".as_bytes());
        h.feed(format!("\x1b[1;1H{erase}").as_bytes());
        assert_eq!(h.line(0), "  本a", "erase leading half: {erase:?}");
    }
    for erase in ["\x1b[X", "\x1b[K", "\x1b[J"] {
        h.feed("\x1b[2J\x1b[H日本".as_bytes());
        h.feed(format!("\x1b[1;4H{erase}").as_bytes());
        assert_eq!(h.line(0), "日", "erase trailing half: {erase:?}");
    }
    h.feed("\x1b[2J\x1b[H日本a\x1b[1;2H\x1b[@".as_bytes());
    assert_eq!(h.line(0), "   本a");
    h.feed("\x1b[2J\x1b[H日本a\x1b[1;2H\x1b[P".as_bytes());
    assert_eq!(h.line(0), " 本a");
    h.feed("\x1b[2J\x1b[H日本a\x1b[1;3H\x1b[P".as_bytes());
    assert_eq!(h.line(0), "日 a");

    // Insertion at the start can push only half a wide character off the edge.
    let cols = h.term().grid_size().0;
    h.feed(format!("\x1b[2J\x1b[1;{}H日\r\x1b[@", cols - 1).as_bytes());
    assert_eq!(h.line(0), "");
}

#[test]
fn keys_send_the_bytes_a_terminal_program_reads() {
    let mut h = Harness::new(Terminal::new().rows(3));
    h.tree.dispatch(Event::Text("ls".into()), &mut h.text);
    assert_eq!(h.input(), b"ls");
    assert_eq!(h.key(Key::Enter), b"\r");
    assert_eq!(h.key(Key::Backspace), [0x7f]);
    let ctrl = Modifiers {
        ctrl: true,
        ..Default::default()
    };
    assert_eq!(h.key_with(Key::Char('c'), ctrl), [3]);
    assert_eq!(h.key(Key::ArrowUp), b"\x1b[A");
    // Application cursor keys (vim, less) use SS3.
    h.feed(b"\x1b[?1h");
    assert_eq!(h.key(Key::ArrowUp), b"\x1bOA");
    let alt = Modifiers {
        alt: true,
        ..Default::default()
    };
    assert_eq!(h.key_with(Key::Backspace, alt), [0x1b, 0x7f]);
    assert!(h.key(Key::F10).is_empty());
}

/// The terminal takes Tab (shells complete with it); Ctrl+Tab leaves the terminal, so
/// the keyboard is not trapped in it (WCAG 2.1.2).
#[test]
fn tab_goes_to_the_program_and_ctrl_tab_moves_the_focus_on() {
    let mut h = Harness::new(Terminal::new().rows(3));
    let root = h.tree.root().unwrap();
    let after = h.tree.insert(Some(root), Button::new("After"));
    h.prepare();
    assert_eq!(h.key(Key::Tab), b"\t");
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    assert_eq!(h.key_with(Key::Tab, shift), b"\x1b[Z");
    assert_eq!(h.tree.focused(), Some(h.term));
    let ctrl = Modifiers {
        ctrl: true,
        ..Default::default()
    };
    assert!(h.key_with(Key::Tab, ctrl).is_empty());
    assert_eq!(h.tree.focused(), Some(after));
}

#[test]
fn the_grid_follows_the_widget_size_and_reports_changes_once() {
    let mut h = Harness::new(Terminal::new());
    let first = h.term().grid_size();
    let taken = h
        .tree
        .update::<Terminal, _>(h.term, |t, _| t.take_resized())
        .unwrap();
    assert_eq!(taken, Some(first));
    h.feed(b"keep me");
    h.viewport = Size::new(320.0, 200.0);
    h.prepare();
    let smaller = h.term().grid_size();
    assert!(
        smaller.0 < first.0 && smaller.1 < first.1,
        "{first:?} -> {smaller:?}"
    );
    let taken = h
        .tree
        .update::<Terminal, _>(h.term, |t, _| (t.take_resized(), t.take_resized()))
        .unwrap();
    assert_eq!(taken, (Some(smaller), None));
    assert_eq!(h.line(0), "keep me");
}

#[test]
fn repainting_an_unchanged_screen_shapes_no_text() {
    let mut h = Harness::new(Terminal::new());
    h.feed(b"hello \x1b[1mbold\x1b[0m world");
    let grid = h.term().grid_size();
    let revision = h.text.layouts().revision();
    for focus in [None, Some(h.term), None, Some(h.term)] {
        h.tree.set_focus(focus);
        assert!(
            h.tree.prepare(h.viewport, &mut h.text).is_some(),
            "repainted"
        );
    }
    assert_eq!(
        h.text.layouts().revision(),
        revision,
        "no text was reshaped"
    );
    assert_eq!(h.term().grid_size(), grid);
    assert_eq!(h.line(0), "hello bold world");
}

#[test]
fn the_wheel_scrolls_back_through_earlier_lines_and_typing_returns() {
    let mut h = Harness::new(Terminal::new().rows(3));
    for i in 0..10 {
        h.feed(format!("line {i}\r\n").as_bytes());
    }
    assert!(h.term().scrollback_len() >= 7);
    let pos = {
        let r = h.tree.window_rect(h.term).unwrap();
        ui_scene::Point::new(r.x + 20.0, r.y + 20.0)
    };
    h.tree.dispatch(
        Event::Wheel {
            pos,
            delta: ui_scene::Point::new(0.0, 200.0),
        },
        &mut h.text,
    );
    assert!(h.term().scroll_offset() > 0);
    h.tree.dispatch(Event::Text("q".into()), &mut h.text);
    assert_eq!(h.term().scroll_offset(), 0);
}

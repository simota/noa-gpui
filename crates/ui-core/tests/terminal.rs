use noa_gpui_core::style::{FlexDirection, Size as LayoutSize, percent};
use noa_gpui_core::widgets::{Button, Container, TermColor, Terminal, TerminalInput};
use noa_gpui_core::{Event, Key, Message, Modifiers, PointerButton, Style, UiTree, WidgetId};
use noa_gpui_scene::{Point, Size};
use noa_gpui_text::{FontOptions, TextStore, TextStyle};

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

    /// The middle of the cell at `col`, `row` of the visible grid, in window coordinates.
    fn cell_center(&mut self, col: usize, row: usize) -> Point {
        let style = TextStyle::new(13.0).family(self.text.font_family("monospace"));
        let probe = self.text.create("M", style, None);
        let width = self.text.caret_x(probe, 1);
        self.text.remove(probe);
        let r = self.tree.window_rect(self.term).unwrap();
        Point::new(
            r.x + 6.0 + (col as f32 + 0.5) * width,
            r.y + 6.0 + (row as f32 + 0.5) * style.line_height,
        )
    }

    fn drag(&mut self, from: (usize, usize), to: (usize, usize)) {
        let (from, to) = (
            self.cell_center(from.0, from.1),
            self.cell_center(to.0, to.1),
        );
        let button = PointerButton::Primary;
        self.tree
            .dispatch(Event::PointerDown { pos: from, button }, &mut self.text);
        self.tree
            .dispatch(Event::PointerMove { pos: to }, &mut self.text);
        self.tree
            .dispatch(Event::PointerUp { pos: to, button }, &mut self.text);
        self.prepare();
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

/// A cell is as wide as a glyph's advance, not the rounded-up width of a laid-out
/// "M": the grid (and the cursor drawn on it) keeps pace with the text.
#[test]
fn cells_are_as_wide_as_the_glyphs_advance() {
    let mut h = Harness::new(Terminal::new().font_size(12.0));
    // Wide enough that the rounding adds up to whole cells.
    h.viewport = Size::new(1200.0, 400.0);
    h.prepare();
    let family = h.text.font_family("monospace");
    let probe = h.text.create(
        &"M".repeat(10),
        noa_gpui_text::TextStyle::new(12.0).family(family),
        None,
    );
    let advance = h.text.caret_x(probe, 10) / 10.0;
    assert!(
        advance.fract() != 0.0,
        "the font's advance is fractional: {advance}"
    );
    let cols = h.term().grid_size().0;
    let available = h.viewport.width - 2.0 * 6.0;
    assert_eq!(
        cols,
        (available / advance).floor() as usize,
        "advance {advance}"
    );
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
        noa_gpui_scene::Point::new(r.x + 20.0, r.y + 20.0)
    };
    h.tree.dispatch(
        Event::Wheel {
            pos,
            delta: noa_gpui_scene::Point::new(0.0, 200.0),
        },
        &mut h.text,
    );
    assert!(h.term().scroll_offset() > 0);
    h.tree.dispatch(Event::Text("q".into()), &mut h.text);
    assert_eq!(h.term().scroll_offset(), 0);
}

/// Cmd on Apple platforms.
const CMD: Modifiers = Modifiers {
    shift: false,
    ctrl: false,
    alt: false,
    meta: true,
    command: true,
};
/// The clipboard shortcuts where Ctrl is the shortcut modifier.
const CTRL_SHIFT: Modifiers = Modifiers {
    shift: true,
    ctrl: true,
    alt: false,
    meta: false,
    command: true,
};

#[test]
fn dragging_selects_text_across_lines_and_the_shortcut_copies_it() {
    let mut h = Harness::new(Terminal::new().rows(4));
    h.feed(b"hello world\r\n\xe6\x97\xa5\xe6\x9c\xac\xe8\xaa\x9e ok\r\nlast");
    assert_eq!(h.term().selected_text(), None);
    h.drag((6, 0), (4, 0));
    assert_eq!(h.term().selected_text().as_deref(), Some("o w"));
    // Ending on the second half of a wide character takes the whole character.
    h.drag((6, 0), (3, 1));
    assert_eq!(h.term().selected_text().as_deref(), Some("world\n日本"));
    assert!(h.key_with(Key::Char('c'), CMD).is_empty());
    assert_eq!(
        h.tree.clipboard().read_text().as_deref(),
        Some("world\n日本")
    );
    h.drag((0, 2), (3, 2));
    assert!(h.key_with(Key::Char('C'), CTRL_SHIFT).is_empty());
    assert_eq!(h.tree.clipboard().read_text().as_deref(), Some("last"));
    // Ctrl+C alone still interrupts the program.
    let ctrl = Modifiers {
        shift: false,
        ..CTRL_SHIFT
    };
    assert_eq!(h.key_with(Key::Char('c'), ctrl), [3]);
    assert_eq!(h.tree.clipboard().read_text().as_deref(), Some("last"));
    // A click selects nothing and drops the selection.
    h.drag((2, 1), (2, 1));
    assert_eq!(h.term().selected_text(), None);
}

#[test]
fn a_selection_follows_its_lines_into_the_scrollback_until_they_change() {
    let mut h = Harness::new(Terminal::new().rows(3));
    h.feed(b"first\r\nsecond");
    h.drag((0, 0), (4, 0));
    assert_eq!(h.term().selected_text().as_deref(), Some("first"));
    h.feed(b"\r\nthird\r\nfourth\r\nfifth");
    assert!(h.term().scrollback_len() >= 1);
    assert_eq!(h.term().selected_text().as_deref(), Some("first"));
    // Scrolled back, the same cells are the ones under the pointer.
    let pos = h.cell_center(0, 0);
    h.tree.dispatch(
        Event::Wheel {
            pos,
            delta: Point::new(0.0, 200.0),
        },
        &mut h.text,
    );
    h.drag((0, 0), (4, 1));
    assert_eq!(h.term().selected_text().as_deref(), Some("first\nsecon"));
    h.tree.dispatch(Event::Text("q".into()), &mut h.text);
    h.drag((0, 2), (4, 2));
    assert_eq!(h.term().selected_text().as_deref(), Some("fifth"));
    // Output over the selected cells ends the selection.
    h.feed(b"\rFIFTH");
    assert_eq!(h.term().selected_text(), None);
    // Clearing the scrollback leaves a selection on the screen where it is.
    h.drag((0, 0), (4, 0));
    assert_eq!(h.term().selected_text().as_deref(), Some("third"));
    h.feed(b"\x1b[3J");
    assert_eq!(h.term().scrollback_len(), 0);
    assert_eq!(h.term().selected_text().as_deref(), Some("third"));
}

#[test]
fn pasting_sends_the_clipboard_text_bracketed_when_the_program_asks() {
    let mut h = Harness::new(Terminal::new().rows(3));
    assert!(h.key_with(Key::Char('v'), CMD).is_empty());
    h.tree.clipboard().write_text("echo a\r\necho b\n");
    assert_eq!(h.key_with(Key::Char('v'), CMD), b"echo a\recho b\r");
    assert_eq!(h.key_with(Key::Char('V'), CTRL_SHIFT), b"echo a\recho b\r");
    // Ctrl+V alone is the program's.
    let ctrl = Modifiers {
        shift: false,
        ..CTRL_SHIFT
    };
    assert_eq!(h.key_with(Key::Char('v'), ctrl), [0x16]);
    h.feed(b"\x1b[?2004h");
    // Pasted text cannot close the bracket itself.
    h.tree.clipboard().write_text("ls\x1b[201~\nrm");
    assert_eq!(
        h.key_with(Key::Char('v'), CMD),
        b"\x1b[200~ls[201~\rrm\x1b[201~"
    );
    h.feed(b"\x1b[?2004l");
    h.tree.clipboard().write_text("x");
    assert_eq!(h.key_with(Key::Char('v'), CMD), b"x");
}

/// With the scrollback full, every scrolled line pushes the oldest one out: a selection
/// stays on its own line, and a drag still starts where it was pressed.
#[test]
fn a_selection_stays_on_its_line_while_a_full_scrollback_drops_older_lines() {
    let mut h = Harness::new(Terminal::new().rows(3).scrollback(2));
    h.feed(b"one\r\ntwo\r\nthree\r\nfour\r\nfive");
    assert_eq!(h.term().scrollback_len(), 2);
    h.drag((0, 1), (3, 1));
    assert_eq!(h.term().selected_text().as_deref(), Some("four"));
    h.feed(b"\r\nsix");
    assert_eq!(h.term().scrollback_len(), 2);
    assert_eq!(h.line(0), "four");
    assert_eq!(h.term().selected_text().as_deref(), Some("four"));

    // Pressed on "six", which scrolls up a row before the pointer moves.
    let button = PointerButton::Primary;
    let pos = h.cell_center(0, 2);
    h.tree
        .dispatch(Event::PointerDown { pos, button }, &mut h.text);
    h.feed(b"\r\nseven");
    assert_eq!(h.line(1), "six");
    let pos = h.cell_center(2, 1);
    h.tree.dispatch(Event::PointerMove { pos }, &mut h.text);
    h.tree
        .dispatch(Event::PointerUp { pos, button }, &mut h.text);
    assert_eq!(h.term().selected_text().as_deref(), Some("six"));
}

/// A line pushed out of the scrollback takes its selection with it, even when the line
/// that takes its place reads the same.
#[test]
fn a_selection_ends_when_its_line_leaves_the_scrollback_for_an_identical_one() {
    let mut h = Harness::new(Terminal::new().rows(2).scrollback(1));
    h.feed(b"same\r\nsame\r\nsame");
    assert_eq!(h.term().scrollback_len(), 1);
    let pos = h.cell_center(0, 0);
    h.tree.dispatch(
        Event::Wheel {
            pos,
            delta: Point::new(0.0, 200.0),
        },
        &mut h.text,
    );
    assert_eq!(h.term().scroll_offset(), 1);
    h.drag((0, 0), (3, 0));
    assert_eq!(h.term().selected_text().as_deref(), Some("same"));
    h.feed(b"\r\nsame");
    assert_eq!(h.term().selected_text(), None);

    // Clearing the scrollback ends a selection in it too.
    h.drag((0, 0), (3, 0));
    assert_eq!(h.term().selected_text().as_deref(), Some("same"));
    h.feed(b"\x1b[3J");
    assert_eq!(h.term().selected_text(), None);
}

/// A scroll region at the top of the screen sends its lines to the scrollback while
/// the lines below it stay where they are, and so does a selection on them.
#[test]
fn a_selection_below_a_scrolling_region_stays_on_its_line() {
    let mut h = Harness::new(Terminal::new().rows(4));
    h.feed(b"one\r\ntwo\r\nthree\r\nfour");
    h.drag((0, 3), (3, 3));
    assert_eq!(h.term().selected_text().as_deref(), Some("four"));
    h.feed(b"\x1b[1;2r\x1b[2;1H\n");
    assert_eq!(h.term().scrollback_len(), 1);
    assert_eq!(h.line(0), "two");
    assert_eq!(h.line(3), "four");
    assert_eq!(h.term().selected_text().as_deref(), Some("four"));
}

/// A drag whose first line has left the scrollback selects nothing: it does not start
/// over from a line that was never pressed.
#[test]
fn a_drag_ends_when_the_line_it_started_on_leaves_the_scrollback() {
    let button = PointerButton::Primary;
    for clear in [&b"\r\nddd"[..], b"\x1b[3J"] {
        let mut h = Harness::new(Terminal::new().rows(2).scrollback(1));
        h.feed(b"aaa\r\nbbb\r\nccc");
        let pos = h.cell_center(0, 0);
        h.tree.dispatch(
            Event::Wheel {
                pos,
                delta: Point::new(0.0, 200.0),
            },
            &mut h.text,
        );
        assert_eq!(h.term().scroll_offset(), 1);
        h.tree
            .dispatch(Event::PointerDown { pos, button }, &mut h.text);
        h.feed(clear);
        let pos = h.cell_center(2, 1);
        h.tree.dispatch(Event::PointerMove { pos }, &mut h.text);
        assert_eq!(h.term().selected_text(), None);
        h.tree
            .dispatch(Event::PointerUp { pos, button }, &mut h.text);
        assert_eq!(h.term().selected_text(), None);
        // The next drag selects as usual.
        h.drag((0, 1), (2, 1));
        assert!(h.term().selected_text().is_some());
    }
}

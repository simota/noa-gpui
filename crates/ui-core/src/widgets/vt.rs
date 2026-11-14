//! A VT100 / xterm screen covering what full-screen programs (vim, top, less) and
//! shells use: cursor movement, erasing, insert / delete of lines and characters,
//! scroll regions, the alternate screen, SGR colors (16, 256 and true color),
//! application cursor keys and the cursor position report. Other sequences are parsed
//! and ignored.

use std::collections::VecDeque;

/// A color a program chose for text or a cell's background.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TermColor {
    /// The terminal's own text or background color.
    #[default]
    Default,
    /// One of the 256 xterm colors: 0–7 normal, 8–15 bright, then the 6×6×6 cube and
    /// the gray ramp.
    Indexed(u8),
    Rgb(u8, u8, u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CellStyle {
    pub fg: TermColor,
    pub bg: TermColor,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    /// Swap the text and background colors.
    pub inverse: bool,
}

/// One character cell. The cell right of a wide character holds [`WIDE_TAIL`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalCell {
    pub ch: char,
    pub style: CellStyle,
}

/// The character of the second cell a wide (East Asian) character covers.
pub const WIDE_TAIL: char = '\0';

impl Default for TerminalCell {
    fn default() -> Self {
        Self {
            ch: ' ',
            style: CellStyle::default(),
        }
    }
}

type Line = Vec<TerminalCell>;

#[derive(Debug, Clone, Copy, Default)]
struct Cursor {
    col: usize,
    row: usize,
    style: CellStyle,
    /// DECSC / DECRC save and restore the origin mode with the cursor.
    origin_mode: bool,
    /// The last column was written: the next character goes to the next line.
    pending_wrap: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Ground,
    Escape,
    /// `ESC` and an intermediate byte (character set selection and the like): the
    /// next byte ends the sequence.
    EscapeIntermediate,
    Csi,
    /// A CSI with too many parameters: discard it through its final byte.
    CsiIgnore,
    /// An OSC, DCS or other string, up to BEL or ST.
    String,
    StringEscape,
}

/// The longest OSC kept (for the window title); longer strings are cut.
const MAX_STRING: usize = 4096;
/// Most CSI parameters kept; a sequence with more is discarded.
const MAX_PARAMS: usize = 64;

#[derive(Debug)]
pub(crate) struct VtScreen {
    cols: usize,
    rows: usize,
    lines: Vec<Line>,
    /// The main screen and its cursor while the alternate screen is shown.
    saved_main: Option<(Vec<Line>, Cursor)>,
    scrollback: VecDeque<Line>,
    max_scrollback: usize,
    cursor: Cursor,
    saved_cursor: Cursor,
    /// Scroll region, inclusive.
    top: usize,
    bottom: usize,
    autowrap: bool,
    insert_mode: bool,
    pub app_cursor_keys: bool,
    pub cursor_visible: bool,
    pub title: String,
    state: State,
    params: Vec<u16>,
    param: Option<u32>,
    private: Option<u8>,
    intermediate: Option<u8>,
    string: Vec<u8>,
    utf8: Vec<u8>,
    /// Replies to the program (cursor position reports), to write to its input.
    responses: Vec<u8>,
}

impl VtScreen {
    pub fn new(cols: usize, rows: usize, max_scrollback: usize) -> Self {
        let (cols, rows) = (cols.max(1), rows.max(1));
        Self {
            cols,
            rows,
            lines: vec![blank_line(cols, CellStyle::default()); rows],
            saved_main: None,
            scrollback: VecDeque::new(),
            max_scrollback,
            cursor: Cursor::default(),
            saved_cursor: Cursor::default(),
            top: 0,
            bottom: rows - 1,
            autowrap: true,
            insert_mode: false,
            app_cursor_keys: false,
            cursor_visible: true,
            title: String::new(),
            state: State::Ground,
            params: Vec::new(),
            param: None,
            private: None,
            intermediate: None,
            string: Vec::new(),
            utf8: Vec::new(),
            responses: Vec::new(),
        }
    }

    pub fn size(&self) -> (usize, usize) {
        (self.cols, self.rows)
    }

    pub fn line(&self, row: usize) -> &[TerminalCell] {
        &self.lines[row]
    }

    pub fn scrollback(&self) -> &VecDeque<Line> {
        &self.scrollback
    }

    pub fn cursor(&self) -> (usize, usize) {
        (self.cursor.col, self.cursor.row)
    }

    pub fn is_alternate_screen(&self) -> bool {
        self.saved_main.is_some()
    }

    pub fn take_responses(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.responses)
    }

    /// Change the grid size. Lines that no longer fit above the cursor go to the
    /// scrollback; the scroll region resets to the whole screen.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        let (cols, rows) = (cols.max(1), rows.max(1));
        if (cols, rows) == (self.cols, self.rows) {
            return;
        }
        let alternate = self.saved_main.is_some();
        if self.cursor.row >= rows {
            let shift = self.cursor.row + 1 - rows;
            for line in self.lines.drain(..shift) {
                if !alternate {
                    push_bounded(&mut self.scrollback, line, self.max_scrollback);
                }
            }
            self.cursor.row -= shift;
        }
        fit(&mut self.lines, cols, rows);
        if let Some((main, cursor)) = &mut self.saved_main {
            fit(main, cols, rows);
            clamp_cursor(cursor, cols, rows);
        }
        self.cols = cols;
        self.rows = rows;
        self.top = 0;
        self.bottom = rows - 1;
        clamp_cursor(&mut self.cursor, cols, rows);
        clamp_cursor(&mut self.saved_cursor, cols, rows);
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.byte(byte);
        }
    }

    fn byte(&mut self, byte: u8) {
        match self.state {
            State::Ground => self.ground(byte),
            State::Escape => self.escape(byte),
            State::EscapeIntermediate => self.state = State::Ground,
            State::Csi => self.csi_byte(byte),
            State::CsiIgnore => match byte {
                0x1b => self.state = State::Escape,
                0x18 | 0x1a | 0x40..=0x7e => self.state = State::Ground,
                0x00..=0x1f => self.control(byte),
                _ => {}
            },
            State::String => match byte {
                0x07 => self.end_string(),
                0x1b => self.state = State::StringEscape,
                _ if self.string.len() < MAX_STRING => self.string.push(byte),
                _ => {}
            },
            State::StringEscape => {
                self.end_string();
                if byte != b'\\' {
                    self.escape(byte);
                }
            }
        }
    }

    fn ground(&mut self, byte: u8) {
        if !self.utf8.is_empty() || byte >= 0x80 {
            self.utf8_byte(byte);
            return;
        }
        match byte {
            0x1b => self.state = State::Escape,
            0x20..=0x7e => self.print(byte as char),
            _ => self.control(byte),
        }
    }

    fn utf8_byte(&mut self, byte: u8) {
        let continuation = byte & 0xc0 == 0x80;
        if self.utf8.is_empty() && continuation {
            self.print(char::REPLACEMENT_CHARACTER);
            return;
        }
        if !self.utf8.is_empty() && !continuation {
            // A sequence cut short: show it as one bad character, then start over.
            self.utf8.clear();
            self.print(char::REPLACEMENT_CHARACTER);
            self.ground(byte);
            return;
        }
        self.utf8.push(byte);
        let expected = match self.utf8[0] {
            0xc0..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf7 => 4,
            _ => 1,
        };
        if self.utf8.len() >= expected {
            let c = std::str::from_utf8(&self.utf8)
                .ok()
                .and_then(|s| s.chars().next())
                .unwrap_or(char::REPLACEMENT_CHARACTER);
            self.utf8.clear();
            self.print(c);
        }
    }

    fn control(&mut self, byte: u8) {
        match byte {
            0x08 => {
                self.cursor.col = self.cursor.col.saturating_sub(1);
                self.cursor.pending_wrap = false;
            }
            0x09 => {
                let next = (self.cursor.col / 8 + 1) * 8;
                self.cursor.col = next.min(self.cols - 1);
                self.cursor.pending_wrap = false;
            }
            0x0a..=0x0c => self.linefeed(),
            0x0d => {
                self.cursor.col = 0;
                self.cursor.pending_wrap = false;
            }
            // BEL, shift in / out and the rest: nothing to draw.
            _ => {}
        }
    }

    fn escape(&mut self, byte: u8) {
        self.state = State::Ground;
        match byte {
            b'[' => {
                self.params.clear();
                self.param = None;
                self.private = None;
                self.intermediate = None;
                self.state = State::Csi;
            }
            b']' | b'P' | b'X' | b'^' | b'_' => {
                self.string.clear();
                self.state = State::String;
            }
            b'(' | b')' | b'*' | b'+' | b'#' | b'%' => self.state = State::EscapeIntermediate,
            b'7' => self.saved_cursor = self.cursor,
            b'8' => self.restore_cursor(),
            b'D' => self.linefeed(),
            b'E' => {
                self.cursor.col = 0;
                self.linefeed();
            }
            b'M' => self.reverse_index(),
            b'c' => {
                let (cols, rows, max) = (self.cols, self.rows, self.max_scrollback);
                let scrollback = std::mem::take(&mut self.scrollback);
                *self = Self::new(cols, rows, max);
                self.scrollback = scrollback;
            }
            _ => {}
        }
    }

    fn csi_byte(&mut self, byte: u8) {
        match byte {
            b'0'..=b'9' => {
                let digit = u32::from(byte - b'0');
                self.param = Some((self.param.unwrap_or(0) * 10 + digit).min(65_535));
            }
            b';' | b':' => {
                // A separator starts another parameter, even when it is empty.
                if self.params.len() < MAX_PARAMS - 1 {
                    self.params.push(self.param.take().unwrap_or(0) as u16);
                } else {
                    self.params.clear();
                    self.param = None;
                    self.state = State::CsiIgnore;
                }
            }
            b'?' | b'>' | b'=' | b'<' if self.params.is_empty() && self.param.is_none() => {
                self.private = Some(byte);
            }
            0x20..=0x2f => self.intermediate = Some(byte),
            0x40..=0x7e => {
                if let Some(p) = self.param.take() {
                    self.params.push(p as u16);
                }
                self.state = State::Ground;
                self.csi(byte);
            }
            0x1b => self.state = State::Escape,
            0x18 | 0x1a => self.state = State::Ground,
            0x00..=0x1f => self.control(byte),
            _ => self.state = State::Ground,
        }
    }

    fn end_string(&mut self) {
        self.state = State::Ground;
        // OSC 0 and 2 set the window title.
        let text = String::from_utf8_lossy(&self.string);
        if let Some(title) = text.strip_prefix("0;").or_else(|| text.strip_prefix("2;")) {
            self.title = title.to_owned();
        }
        self.string.clear();
    }

    /// Parameter `i`, with `default` for a missing or zero one.
    fn arg(&self, i: usize, default: usize) -> usize {
        match self.params.get(i) {
            Some(&p) if p != 0 => p as usize,
            _ => default,
        }
    }

    fn csi(&mut self, command: u8) {
        if self.intermediate.is_some() {
            // DECSCUSR (cursor shape) and other sequences with intermediates.
            return;
        }
        let private = self.private;
        let n = self.arg(0, 1);
        match (private, command) {
            (None, b'A') => self.move_up(n),
            (None, b'B' | b'e') => self.move_down(n),
            (None, b'C' | b'a') => self.goto(self.cursor.col + n, self.cursor.row),
            (None, b'D') => self.goto(self.cursor.col.saturating_sub(n), self.cursor.row),
            (None, b'E') => {
                self.move_down(n);
                self.cursor.col = 0;
            }
            (None, b'F') => {
                self.move_up(n);
                self.cursor.col = 0;
            }
            (None, b'G' | b'`') => self.goto(n - 1, self.cursor.row),
            (None, b'd') => {
                let row = self.origin_row(n - 1);
                self.goto(self.cursor.col, row);
            }
            (None, b'H' | b'f') => {
                let row = self.origin_row(self.arg(0, 1) - 1);
                self.goto(self.arg(1, 1) - 1, row);
            }
            (None | Some(b'?'), b'J') => self.erase_display(self.params.first().copied()),
            (None | Some(b'?'), b'K') => self.erase_line(self.params.first().copied()),
            (None, b'L') => self.insert_lines(n),
            (None, b'M') => self.delete_lines(n),
            (None, b'@') => self.insert_chars(n),
            (None, b'P') => self.delete_chars(n),
            (None, b'X') => {
                let (col, row) = (self.cursor.col, self.cursor.row);
                let end = (col + n).min(self.cols);
                let blank = self.blank();
                clear_cells(&mut self.lines[row], col, end, blank);
            }
            (None, b'S') => self.scroll_up(n),
            (None, b'T') => self.scroll_down(n),
            (None, b'r') => {
                let top = self.arg(0, 1) - 1;
                let bottom = self.arg(1, self.rows).min(self.rows) - 1;
                if top < bottom {
                    self.top = top;
                    self.bottom = bottom;
                    let row = if self.cursor.origin_mode { top } else { 0 };
                    self.goto(0, row);
                }
            }
            (None, b'm') => self.sgr(),
            (Some(b'?'), b'h') => self.private_modes(true),
            (Some(b'?'), b'l') => self.private_modes(false),
            (None, b'h' | b'l') => {
                if self.params.contains(&4) {
                    self.insert_mode = command == b'h';
                }
            }
            (None, b's') => self.saved_cursor = self.cursor,
            (None, b'u') => self.restore_cursor(),
            (None, b'n') => match self.params.first() {
                Some(5) => self.responses.extend_from_slice(b"\x1b[0n"),
                Some(6) => {
                    let row = if self.cursor.origin_mode {
                        self.cursor.row.saturating_sub(self.top)
                    } else {
                        self.cursor.row
                    };
                    let report = format!("\x1b[{};{}R", row + 1, self.cursor.col + 1);
                    self.responses.extend_from_slice(report.as_bytes());
                }
                _ => {}
            },
            (None, b'c') if self.arg(0, 0) == 0 => {
                // A VT100 with advanced video.
                self.responses.extend_from_slice(b"\x1b[?1;2c");
            }
            _ => {}
        }
    }

    fn private_modes(&mut self, on: bool) {
        for i in 0..self.params.len() {
            match self.params[i] {
                1 => self.app_cursor_keys = on,
                6 => {
                    self.cursor.origin_mode = on;
                    let row = if on { self.top } else { 0 };
                    self.goto(0, row);
                }
                7 => self.autowrap = on,
                25 => self.cursor_visible = on,
                47 | 1047 => self.alternate_screen(on, false),
                1049 => self.alternate_screen(on, true),
                _ => {}
            }
        }
    }

    fn alternate_screen(&mut self, on: bool, save_cursor: bool) {
        if on && self.saved_main.is_none() {
            if save_cursor {
                self.saved_cursor = self.cursor;
            }
            let blank = vec![blank_line(self.cols, CellStyle::default()); self.rows];
            let main = std::mem::replace(&mut self.lines, blank);
            self.saved_main = Some((main, self.cursor));
        } else if !on && let Some((main, cursor)) = self.saved_main.take() {
            self.lines = main;
            self.cursor = if save_cursor {
                self.saved_cursor
            } else {
                cursor
            };
        }
        self.top = 0;
        self.bottom = self.rows - 1;
    }

    fn restore_cursor(&mut self) {
        self.cursor = self.saved_cursor;
        // The scroll region may have changed since DECSC. In origin mode the
        // restored cursor must still lie inside the current region.
        if self.cursor.origin_mode {
            self.cursor.row = self.cursor.row.clamp(self.top, self.bottom);
        }
    }

    fn sgr(&mut self) {
        if self.params.is_empty() {
            self.cursor.style = CellStyle::default();
            return;
        }
        let style = &mut self.cursor.style;
        let mut i = 0;
        while i < self.params.len() {
            let p = self.params[i];
            match p {
                0 => *style = CellStyle::default(),
                1 => style.bold = true,
                2 => style.dim = true,
                3 => style.italic = true,
                4 => style.underline = true,
                7 => style.inverse = true,
                22 => {
                    style.bold = false;
                    style.dim = false;
                }
                23 => style.italic = false,
                24 => style.underline = false,
                27 => style.inverse = false,
                30..=37 => style.fg = TermColor::Indexed(p as u8 - 30),
                39 => style.fg = TermColor::Default,
                40..=47 => style.bg = TermColor::Indexed(p as u8 - 40),
                49 => style.bg = TermColor::Default,
                90..=97 => style.fg = TermColor::Indexed(p as u8 - 90 + 8),
                100..=107 => style.bg = TermColor::Indexed(p as u8 - 100 + 8),
                38 | 48 => {
                    let rest = &self.params[i + 1..];
                    let (color, used) = match rest {
                        [5, n, ..] => (Some(TermColor::Indexed(*n as u8)), 2),
                        [2, r, g, b, ..] => (Some(TermColor::Rgb(*r as u8, *g as u8, *b as u8)), 4),
                        _ => (None, rest.len()),
                    };
                    if let Some(color) = color {
                        if p == 38 {
                            style.fg = color;
                        } else {
                            style.bg = color;
                        }
                    }
                    i += used;
                }
                _ => {}
            }
            i += 1;
        }
    }

    /// A blank cell in the current background (erasing keeps the background color).
    fn blank(&self) -> TerminalCell {
        TerminalCell {
            ch: ' ',
            style: CellStyle {
                bg: self.cursor.style.bg,
                ..CellStyle::default()
            },
        }
    }

    fn print(&mut self, c: char) {
        let width = char_width(c);
        if width == 0 {
            return;
        }
        if self.cursor.pending_wrap && self.autowrap {
            self.cursor.col = 0;
            self.linefeed();
        }
        self.cursor.pending_wrap = false;
        if width == 2 && self.cursor.col + 1 >= self.cols {
            if !self.autowrap || self.cols < 2 {
                return;
            }
            // A wide character does not fit in the last column: it starts the next line.
            let blank = self.blank();
            let (col, row) = (self.cursor.col, self.cursor.row);
            clear_cells(&mut self.lines[row], col, self.cols, blank);
            self.cursor.col = 0;
            self.linefeed();
        }
        let (col, row) = (self.cursor.col, self.cursor.row);
        if self.insert_mode {
            self.insert_chars(width);
        }
        let blank = self.blank();
        clear_cells(&mut self.lines[row], col, col + width, blank);
        let style = self.cursor.style;
        self.lines[row][col] = TerminalCell { ch: c, style };
        if width == 2 {
            self.lines[row][col + 1] = TerminalCell {
                ch: WIDE_TAIL,
                style,
            };
        }
        if col + width >= self.cols {
            self.cursor.col = self.cols - 1;
            self.cursor.pending_wrap = true;
        } else {
            self.cursor.col = col + width;
        }
    }

    fn origin_row(&self, row: usize) -> usize {
        if self.cursor.origin_mode {
            (self.top + row).min(self.bottom)
        } else {
            row
        }
    }

    fn goto(&mut self, col: usize, row: usize) {
        self.cursor.col = col.min(self.cols - 1);
        self.cursor.row = row.min(self.rows - 1);
        self.cursor.pending_wrap = false;
    }

    fn move_up(&mut self, n: usize) {
        let limit = if self.cursor.row >= self.top {
            self.top
        } else {
            0
        };
        let row = self.cursor.row.saturating_sub(n).max(limit);
        self.goto(self.cursor.col, row);
    }

    fn move_down(&mut self, n: usize) {
        let limit = if self.cursor.row <= self.bottom {
            self.bottom
        } else {
            self.rows - 1
        };
        let row = (self.cursor.row + n).min(limit);
        self.goto(self.cursor.col, row);
    }

    fn linefeed(&mut self) {
        if self.cursor.row == self.bottom {
            self.scroll_up(1);
        } else if self.cursor.row + 1 < self.rows {
            self.cursor.row += 1;
        }
        self.cursor.pending_wrap = false;
    }

    fn reverse_index(&mut self) {
        if self.cursor.row == self.top {
            self.scroll_down(1);
        } else {
            self.cursor.row = self.cursor.row.saturating_sub(1);
        }
        self.cursor.pending_wrap = false;
    }

    /// Move the lines of the scroll region up by `n`; lines leaving the top of the
    /// main screen go to the scrollback.
    fn scroll_up(&mut self, n: usize) {
        let n = n.min(self.bottom - self.top + 1);
        let blank = blank_line(self.cols, self.blank().style);
        let keep = self.top == 0 && self.saved_main.is_none();
        for _ in 0..n {
            let line = self.lines.remove(self.top);
            if keep {
                push_bounded(&mut self.scrollback, line, self.max_scrollback);
            }
            self.lines.insert(self.bottom, blank.clone());
        }
    }

    fn scroll_down(&mut self, n: usize) {
        let n = n.min(self.bottom - self.top + 1);
        let blank = blank_line(self.cols, self.blank().style);
        for _ in 0..n {
            self.lines.remove(self.bottom);
            self.lines.insert(self.top, blank.clone());
        }
    }

    fn insert_lines(&mut self, n: usize) {
        let row = self.cursor.row;
        if row < self.top || row > self.bottom {
            return;
        }
        let blank = blank_line(self.cols, self.blank().style);
        for _ in 0..n.min(self.bottom - row + 1) {
            self.lines.remove(self.bottom);
            self.lines.insert(row, blank.clone());
        }
        self.cursor.col = 0;
        self.cursor.pending_wrap = false;
    }

    fn delete_lines(&mut self, n: usize) {
        let row = self.cursor.row;
        if row < self.top || row > self.bottom {
            return;
        }
        let blank = blank_line(self.cols, self.blank().style);
        for _ in 0..n.min(self.bottom - row + 1) {
            self.lines.remove(row);
            self.lines.insert(self.bottom, blank.clone());
        }
        self.cursor.col = 0;
        self.cursor.pending_wrap = false;
    }

    fn insert_chars(&mut self, n: usize) {
        let (col, row) = (self.cursor.col, self.cursor.row);
        let n = n.min(self.cols - col);
        if n == 0 {
            return;
        }
        let blank = self.blank();
        let line = &mut self.lines[row];
        // Neither the insertion point nor the right edge may split a wide cell.
        clear_split(line, col, blank);
        clear_split(line, self.cols - n, blank);
        line[col..].rotate_right(n);
        line[col..col + n].fill(blank);
        self.cursor.pending_wrap = false;
    }

    fn delete_chars(&mut self, n: usize) {
        let (col, row) = (self.cursor.col, self.cursor.row);
        let n = n.min(self.cols - col);
        if n == 0 {
            return;
        }
        let blank = self.blank();
        let line = &mut self.lines[row];
        clear_split(line, col, blank);
        clear_split(line, col + n, blank);
        line[col..].rotate_left(n);
        line[self.cols - n..].fill(blank);
        self.cursor.pending_wrap = false;
    }

    fn erase_display(&mut self, mode: Option<u16>) {
        let (col, row) = (self.cursor.col, self.cursor.row);
        let blank = self.blank();
        match mode.unwrap_or(0) {
            0 => {
                clear_cells(&mut self.lines[row], col, self.cols, blank);
                for line in &mut self.lines[row + 1..] {
                    line.fill(blank);
                }
            }
            1 => {
                clear_cells(&mut self.lines[row], 0, col + 1, blank);
                for line in &mut self.lines[..row] {
                    line.fill(blank);
                }
            }
            2 => {
                for line in &mut self.lines {
                    line.fill(blank);
                }
            }
            3 => self.scrollback.clear(),
            _ => {}
        }
    }

    fn erase_line(&mut self, mode: Option<u16>) {
        let (col, row) = (self.cursor.col, self.cursor.row);
        let blank = self.blank();
        let line = &mut self.lines[row];
        match mode.unwrap_or(0) {
            0 => clear_cells(line, col, self.cols, blank),
            1 => clear_cells(line, 0, col + 1, blank),
            2 => line.fill(blank),
            _ => {}
        }
    }
}

/// Erase whole characters even when the requested range cuts through a wide one.
fn clear_cells(line: &mut [TerminalCell], start: usize, end: usize, blank: TerminalCell) {
    if start == end {
        return;
    }
    let start = if start > 0 && line[start].ch == WIDE_TAIL {
        start - 1
    } else {
        start
    };
    let end = if end < line.len() && line[end].ch == WIDE_TAIL {
        end + 1
    } else {
        end
    };
    line[start..end].fill(blank);
}

/// Blank a wide character crossing a boundary before shifting part of a line.
fn clear_split(line: &mut [TerminalCell], at: usize, blank: TerminalCell) {
    if at > 0 && at < line.len() && line[at].ch == WIDE_TAIL {
        line[at - 1..=at].fill(blank);
    }
}

fn blank_line(cols: usize, style: CellStyle) -> Line {
    vec![TerminalCell { ch: ' ', style }; cols]
}

fn push_bounded(scrollback: &mut VecDeque<Line>, line: Line, max: usize) {
    if max == 0 {
        return;
    }
    if scrollback.len() == max {
        scrollback.pop_front();
    }
    scrollback.push_back(line);
}

fn fit(lines: &mut Vec<Line>, cols: usize, rows: usize) {
    lines.truncate(rows);
    for line in lines.iter_mut() {
        line.resize(cols, TerminalCell::default());
        // A wide character cut in half leaves a blank.
        if line.last().is_some_and(|c| char_width(c.ch) == 2) {
            *line.last_mut().expect("not empty") = TerminalCell::default();
        }
    }
    while lines.len() < rows {
        lines.push(blank_line(cols, CellStyle::default()));
    }
}

fn clamp_cursor(cursor: &mut Cursor, cols: usize, rows: usize) {
    cursor.col = cursor.col.min(cols - 1);
    cursor.row = cursor.row.min(rows - 1);
    cursor.pending_wrap = false;
}

/// Columns a character takes: 0 for combining marks and controls, 2 for East Asian
/// wide and fullwidth characters and emoji, 1 otherwise.
pub(crate) fn char_width(c: char) -> usize {
    let u = c as u32;
    match u {
        0 => 1,
        0x01..=0x1f | 0x7f..=0x9f => 0,
        0x0300..=0x036f | 0x200b..=0x200f | 0xfe00..=0xfe0f => 0,
        0x1100..=0x115f
        | 0x2e80..=0x303e
        | 0x3041..=0x33ff
        | 0x3400..=0x4dbf
        | 0x4e00..=0x9fff
        | 0xa000..=0xa4cf
        | 0xac00..=0xd7a3
        | 0xf900..=0xfaff
        | 0xfe30..=0xfe4f
        | 0xff00..=0xff60
        | 0xffe0..=0xffe6
        | 0x1f300..=0x1f64f
        | 0x1f900..=0x1f9ff
        | 0x20000..=0x3fffd => 2,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unfinished_csi_parameters_have_bounded_storage() {
        let mut screen = VtScreen::new(10, 3, 0);
        screen.feed(b"\x1b[31");
        for _ in 0..100 {
            screen.feed(&[b';'; 1024]);
            assert!(screen.params.len() <= 64);
            assert!(screen.params.capacity() <= 64);
        }
        screen.feed(b"mX");
        assert_eq!(screen.line(0)[0].ch, 'X');
        assert_eq!(screen.line(0)[0].style, CellStyle::default());
    }

    #[test]
    fn edits_never_leave_half_of_a_wide_character() {
        for cols in 1..=8 {
            for col in 0..cols {
                for count in 1..=cols + 1 {
                    for command in ["@", "P", "X", "K", "J", "h"] {
                        let mut screen = VtScreen::new(cols, 3, 0);
                        screen.feed("日a本b語".as_bytes());
                        screen.feed(format!("\x1b[1;{}H", col + 1).as_bytes());
                        if command == "h" {
                            screen.feed(b"\x1b[4h");
                            screen.feed("日x".as_bytes());
                        } else {
                            screen.feed(format!("\x1b[{count}{command}").as_bytes());
                        }
                        for row in 0..3 {
                            let line = screen.line(row);
                            for (i, cell) in line.iter().enumerate() {
                                if cell.ch == WIDE_TAIL {
                                    assert!(i > 0 && char_width(line[i - 1].ch) == 2);
                                    assert_eq!(cell.style, line[i - 1].style);
                                } else if char_width(cell.ch) == 2 {
                                    assert!(i + 1 < cols && line[i + 1].ch == WIDE_TAIL);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

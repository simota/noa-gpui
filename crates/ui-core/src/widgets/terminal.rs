use noa_gpui_layout::Style;
use noa_gpui_layout::style::{Dimension, Size as LayoutSize, percent};
use noa_gpui_scene::{Border, Color, Point, Rect, Size};
use noa_gpui_text::{FontFamily, TextStore, TextStyle};

use super::rows::TextCache;
use super::view_height;
use super::vt::{CellStyle, TermColor, TerminalCell, VtScreen, WIDE_TAIL, char_width};
use crate::event::{Cursor, Event, Key};
use crate::theme::Role;
use crate::widget::{EventCx, PaintCx, UpdateCx, Widget};

const PAD: f32 = 6.0;

/// Emitted with the bytes a key press or typed text sends. Write them to the
/// pseudo-terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalInput(pub Vec<u8>);

/// A terminal screen: shows what a program writes to it, escape sequences included,
/// and turns keys into the bytes the program reads.
///
/// The widget does not run the program: the app passes its output to
/// [`Terminal::feed`], writes [`TerminalInput`] to it, and sends it the grid size from
/// [`Terminal::take_resized`] (on desktop through `noa_gpui_platform::Pty`). The grid fills
/// the height its parent leaves unless [`Terminal::rows`] fixes it.
pub struct Terminal {
    screen: VtScreen,
    font_size: f32,
    rows: Option<usize>,
    cell: Size,
    cell_width: Option<(TextStyle, f32)>,
    resized: Option<(usize, usize)>,
    scroll: usize,
    wheel: f32,
    runs: TextCache<(usize, usize, bool, bool)>,
}

impl std::fmt::Debug for Terminal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Terminal")
            .field("size", &self.screen.size())
            .finish_non_exhaustive()
    }
}

impl Default for Terminal {
    fn default() -> Self {
        Self::new()
    }
}

impl Terminal {
    /// An 80×24 screen until it is laid out, keeping 1,000 lines of scrollback.
    pub fn new() -> Self {
        Self {
            screen: VtScreen::new(80, 24, 1000),
            font_size: 13.0,
            rows: None,
            cell: Size::default(),
            cell_width: None,
            resized: None,
            scroll: 0,
            wheel: 0.0,
            runs: TextCache::default(),
        }
    }

    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    pub fn scrollback(mut self, lines: usize) -> Self {
        let (cols, rows) = self.screen.size();
        self.screen = VtScreen::new(cols, rows, lines);
        self
    }

    /// A fixed height of `rows` lines instead of filling the parent.
    pub fn rows(mut self, rows: usize) -> Self {
        self.rows = Some(rows.max(1));
        self
    }

    /// Show the program's output. Returns the replies the program asked for (a cursor
    /// position report), to write back to it.
    pub fn feed(&mut self, cx: &mut UpdateCx, bytes: &[u8]) -> Vec<u8> {
        self.screen.feed(bytes);
        cx.request_paint();
        self.screen.take_responses()
    }

    pub fn grid_size(&self) -> (usize, usize) {
        self.screen.size()
    }

    /// The new grid size if it changed since the last call (the widget was resized):
    /// pass it on to the program (`noa_gpui_platform::Pty::resize`).
    pub fn take_resized(&mut self) -> Option<(usize, usize)> {
        self.resized.take()
    }

    /// The text of screen row `row`, without trailing spaces.
    pub fn line_text(&self, row: usize) -> String {
        line_text(self.screen.line(row))
    }

    pub fn cell(&self, col: usize, row: usize) -> TerminalCell {
        self.screen.line(row)[col]
    }

    /// The cursor's column and row.
    pub fn cursor(&self) -> (usize, usize) {
        self.screen.cursor()
    }

    /// The title the program set (OSC 0 or 2).
    pub fn title(&self) -> &str {
        &self.screen.title
    }

    pub fn is_alternate_screen(&self) -> bool {
        self.screen.is_alternate_screen()
    }

    pub fn scrollback_len(&self) -> usize {
        self.screen.scrollback().len()
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll
    }

    fn text_style(&self, family: FontFamily, bold: bool, italic: bool) -> TextStyle {
        TextStyle::new(self.font_size)
            .family(family)
            .weight(if bold { 700 } else { 400 })
            .italic(italic)
    }

    fn fit(&mut self, store: &mut TextStore, family: FontFamily, size: Size) {
        let style = self.text_style(family, false, false);
        // Shaped once per style, not on every paint.
        let width = match self.cell_width {
            Some((measured, width)) if measured == style => width,
            _ => {
                let probe = store.create("M", style, None);
                let width = store.size(probe).width;
                store.remove(probe);
                self.cell_width = Some((style, width));
                width
            }
        };
        self.cell = Size::new(width.max(1.0), style.line_height);
        let cols = ((size.width - 2.0 * PAD) / self.cell.width)
            .floor()
            .max(1.0) as usize;
        let rows = ((size.height - 2.0 * PAD) / self.cell.height)
            .floor()
            .max(1.0) as usize;
        if (cols, rows) != self.screen.size() {
            self.screen.resize(cols, rows);
            self.resized = Some((cols, rows));
            self.scroll = self.scroll.min(self.screen.scrollback().len());
        }
    }

    /// The bytes a key sends, or `None` for keys the terminal leaves alone.
    fn key_bytes(&self, key: Key, cx: &EventCx) -> Option<Vec<u8>> {
        let m = cx.modifiers();
        let app = self.screen.app_cursor_keys;
        let cursor = |c: u8| {
            if app {
                vec![0x1b, b'O', c]
            } else {
                vec![0x1b, b'[', c]
            }
        };
        let bytes = match key {
            Key::Enter => vec![b'\r'],
            // Ctrl+Tab moves the focus out of the terminal, which takes Tab itself.
            Key::Tab if m.ctrl => return None,
            Key::Tab if m.shift => b"\x1b[Z".to_vec(),
            Key::Tab => vec![b'\t'],
            Key::Backspace => vec![0x7f],
            Key::Escape => vec![0x1b],
            Key::Delete => b"\x1b[3~".to_vec(),
            Key::ArrowUp => cursor(b'A'),
            Key::ArrowDown => cursor(b'B'),
            Key::ArrowRight => cursor(b'C'),
            Key::ArrowLeft => cursor(b'D'),
            Key::Home => cursor(b'H'),
            Key::End => cursor(b'F'),
            Key::PageUp => b"\x1b[5~".to_vec(),
            Key::PageDown => b"\x1b[6~".to_vec(),
            Key::Space if m.ctrl => vec![0],
            Key::Char(c) if m.ctrl && !m.meta => match c {
                'a'..='z' => vec![c as u8 - b'a' + 1],
                '[' => vec![0x1b],
                '\\' => vec![0x1c],
                ']' => vec![0x1d],
                '@' | '2' => vec![0],
                _ => return None,
            },
            _ => return None,
        };
        Some(bytes)
    }

    fn send(&mut self, cx: &mut EventCx, bytes: Vec<u8>) {
        if self.scroll != 0 {
            self.scroll = 0;
            cx.request_paint();
        }
        cx.emit(TerminalInput(bytes));
        cx.set_handled();
    }
}

fn line_text(line: &[TerminalCell]) -> String {
    let text: String = line
        .iter()
        .filter(|c| c.ch != WIDE_TAIL)
        .map(|c| c.ch)
        .collect();
    text.trim_end().to_owned()
}

/// The xterm palette: 16 named colors, the 6×6×6 cube and 24 grays.
fn indexed(i: u8) -> Color {
    const NAMED: [u32; 16] = [
        0x000000, 0xcd3131, 0x0dbc79, 0xe5e510, 0x2472c8, 0xbc3fbc, 0x11a8cd, 0xe5e5e5, 0x666666,
        0xf14c4c, 0x23d18b, 0xf5f543, 0x3b8eea, 0xd670d6, 0x29b8db, 0xffffff,
    ];
    match i {
        0..=15 => Color::hex(NAMED[i as usize]),
        16..=231 => {
            let i = i - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            let (r, g, b) = (level(i / 36), level(i / 6 % 6), level(i % 6));
            Color::hex(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b))
        }
        _ => {
            let v = u32::from(8 + (i - 232) * 10);
            Color::hex(v << 16 | v << 8 | v)
        }
    }
}

fn colors(style: CellStyle, text: Color, background: Color) -> (Color, Color) {
    let resolve = |c: TermColor, default: Color| match c {
        TermColor::Default => default,
        TermColor::Indexed(i) => indexed(i),
        TermColor::Rgb(r, g, b) => {
            Color::hex(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b))
        }
    };
    // Bold text in one of the 8 normal colors shows in its bright variant.
    let fg = match style.fg {
        TermColor::Indexed(i) if style.bold && i < 8 => TermColor::Indexed(i + 8),
        fg => fg,
    };
    let (mut fg, mut bg) = (resolve(fg, text), resolve(style.bg, background));
    if style.inverse {
        std::mem::swap(&mut fg, &mut bg);
    }
    if style.dim {
        fg = fg.with_alpha(fg.a * 0.6);
    }
    (fg, bg)
}

impl Widget for Terminal {
    fn cursor(&self, _local: Point, _size: Size) -> Option<Cursor> {
        Some(Cursor::Text)
    }

    fn style(&self) -> Style {
        let mut style = Style {
            size: LayoutSize {
                width: percent(1.0),
                height: Dimension::auto(),
            },
            ..Default::default()
        };
        let line = TextStyle::new(self.font_size).line_height;
        let height = self.rows.map(|rows| rows as f32 * line + 2.0 * PAD);
        view_height(&mut style, height, line + 2.0 * PAD);
        style
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let size = cx.size();
        let family = cx.text_store().font_family("monospace");
        self.fit(cx.text_store(), family, size);
        let background = cx.color(Role::Surface);
        let text_color = cx.color(Role::Text);
        let outline = cx.color(Role::Outline);
        cx.rounded_rect(
            cx.bounds(),
            6.0,
            background,
            Border {
                width: 1.0,
                color: outline,
            },
        );
        let (cols, rows) = self.screen.size();
        let cell = self.cell;
        cx.push_clip(Rect::new(
            PAD,
            PAD,
            cols as f32 * cell.width,
            rows as f32 * cell.height,
        ));
        self.runs.begin();
        let mut text = String::new();
        let scrollback = self.screen.scrollback().len();
        let scroll = self.scroll.min(scrollback);
        for view_row in 0..rows {
            // Scrolled back: the last `scroll` scrollback lines come first.
            let line: &[TerminalCell] = if view_row < scroll {
                &self.screen.scrollback()[scrollback - scroll + view_row]
            } else {
                self.screen.line(view_row - scroll)
            };
            let y = PAD + view_row as f32 * cell.height;
            let mut start = 0;
            while start < line.len() {
                let style = line[start].style;
                let mut end = start + 1;
                while end < line.len() && line[end].style == style {
                    end += 1;
                }
                let (fg, bg) = colors(style, text_color, background);
                let x = PAD + start as f32 * cell.width;
                let width = (end - start) as f32 * cell.width;
                if bg != background {
                    cx.fill_rect(Rect::new(x, y, width, cell.height), bg);
                }
                text.clear();
                text.extend(
                    line[start..end]
                        .iter()
                        .filter(|c| c.ch != WIDE_TAIL)
                        .map(|c| c.ch),
                );
                let shown = text.trim_end();
                if !shown.is_empty() {
                    let text_style = self.text_style(family, style.bold, style.italic);
                    let key = (view_row, start, style.bold, style.italic);
                    let (layout, size) = self.runs.get(cx.text_store(), key, shown, text_style);
                    if let Some(layout) = layout {
                        let rect = Rect::new(
                            x,
                            y + (cell.height - size.height) / 2.0,
                            size.width,
                            size.height,
                        );
                        cx.text(rect, layout, fg);
                    }
                }
                if style.underline {
                    let base = y + cell.height - 1.5;
                    cx.line(Point::new(x, base), Point::new(x + width, base), 1.0, fg);
                }
                start = end;
            }
        }
        let store = cx.text_store();
        self.runs.end(store);
        let (col, row) = self.screen.cursor();
        if self.screen.cursor_visible && scroll == 0 && row < rows {
            let wide = self
                .screen
                .line(row)
                .get(col)
                .is_some_and(|c| char_width(c.ch) == 2);
            let width = if wide { 2.0 } else { 1.0 } * cell.width;
            let rect = Rect::new(
                PAD + col as f32 * cell.width,
                PAD + row as f32 * cell.height,
                width,
                cell.height,
            );
            let accent = cx.color(Role::Accent);
            if cx.is_focused() {
                cx.fill_rect(rect, accent.with_alpha(0.5));
            } else {
                cx.rounded_rect(
                    rect,
                    0.0,
                    Color::TRANSPARENT,
                    Border {
                        width: 1.0,
                        color: accent,
                    },
                );
            }
        }
        cx.pop_clip();
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            Event::PointerDown { .. } => {
                cx.request_focus();
                cx.set_handled();
            }
            Event::Wheel { delta, .. } if !self.screen.is_alternate_screen() => {
                if self.cell.height <= 0.0 {
                    return;
                }
                self.wheel += delta.y / self.cell.height;
                let lines = self.wheel.trunc();
                self.wheel -= lines;
                let max = self.screen.scrollback().len() as f32;
                let next = (self.scroll as f32 + lines).clamp(0.0, max) as usize;
                if next != self.scroll {
                    self.scroll = next;
                    cx.request_paint();
                }
                cx.set_handled();
            }
            Event::KeyDown { key, modifiers } if cx.is_focused() => {
                if let Some(mut bytes) = self.key_bytes(*key, cx) {
                    // Alt (Option) with a key sends ESC first, as xterm's meta does.
                    if modifiers.alt {
                        bytes.insert(0, 0x1b);
                    }
                    self.send(cx, bytes);
                }
            }
            Event::Text(text) if cx.is_focused() => {
                // Option+key text (é, ø) is sent as typed.
                let text: String = text.chars().filter(|c| !c.is_control()).collect();
                if !text.is_empty() {
                    self.send(cx, text.into_bytes());
                }
            }
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.runs.release(text);
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Terminal)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        let (_, rows) = self.screen.size();
        let text: Vec<String> = (0..rows).map(|r| self.line_text(r)).collect();
        node.set_value(text.join("\n").trim_end().to_owned());
    }
}

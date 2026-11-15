use std::collections::VecDeque;
use std::ops::Range;

use noa_gpui_layout::Style;
use noa_gpui_layout::style::{Dimension, Size as LayoutSize, length, percent};
use noa_gpui_scene::{Border, Point, Rect, Size};
use noa_gpui_text::{TextStore, TextStyle};
use regex::Regex;

use super::find::{FOUND, FOUND_CURRENT, FindAction, FindError, FindOptions, FindStatus, compile};
use super::rows::TextCache;
use super::scroller::Scroller;
use super::table::selection_fill;
use super::{focus_ring, view_height};
use crate::event::{Event, Key, PointerButton};
use crate::theme::{Paint, Role};
use crate::widget::{EventCx, PaintCx, UpdateCx, Widget};

const PAD: f32 = 6.0;

#[derive(Debug, Clone, PartialEq)]
pub struct LogLine {
    text: String,
    color: Option<Paint>,
    background: Option<Paint>,
}

impl LogLine {
    /// A line of `text`. A trailing `\r` or `\n` is dropped; a line has no line break
    /// of its own.
    pub fn new(text: impl Into<String>) -> Self {
        let mut text = text.into();
        while text.ends_with(['\n', '\r']) {
            text.pop();
        }
        Self {
            text,
            color: None,
            background: None,
        }
    }

    /// The color of the text ([`Role::Text`] by default).
    pub fn color(mut self, color: impl Into<Paint>) -> Self {
        self.color = Some(color.into());
        self
    }

    pub fn background(mut self, fill: impl Into<Paint>) -> Self {
        self.background = Some(fill.into());
        self
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

impl From<&str> for LogLine {
    fn from(text: &str) -> Self {
        LogLine::new(text)
    }
}

impl From<String> for LogLine {
    fn from(text: String) -> Self {
        LogLine::new(text)
    }
}

/// A match of the search: the line and the byte range in its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogMatch {
    pub line: usize,
    pub range: Range<usize>,
}

/// A read-only view of many monospaced lines, for logs that keep growing. Only the lines
/// on screen are laid out, so hundreds of thousands of lines stay cheap. Lines do not
/// wrap.
pub struct LogView {
    lines: VecDeque<LogLine>,
    /// How many lines were dropped from the front: a line's number since the view was
    /// made, which keys its cached layout.
    dropped: usize,
    max_lines: Option<usize>,
    family: String,
    font_size: f32,
    fill: Option<Paint>,
    bordered: bool,
    width: Dimension,
    height: Option<f32>,
    follows_tail: bool,
    /// Scroll to the bottom at the next layout (new lines arrived at the bottom).
    to_bottom: bool,
    scroller: Scroller,
    cells: TextCache<usize>,
    size: Size,
    /// Width of the longest line drawn so far and the most characters in a line, for
    /// how far the view scrolls sideways without laying every line out.
    widest: f32,
    longest: usize,
    /// Width of one character of the font, from the last paint.
    advance: f32,
    anchor: Option<usize>,
    lead: Option<usize>,
    dragging: bool,
    search: Option<(Regex, String, FindOptions)>,
    search_error: Option<String>,
    found: Vec<LogMatch>,
    current: Option<usize>,
    reveal: Option<usize>,
}

impl std::fmt::Debug for LogView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LogView")
            .field("lines", &self.lines.len())
            .field("found", &self.found.len())
            .finish_non_exhaustive()
    }
}

impl Default for LogView {
    fn default() -> Self {
        Self::new()
    }
}

impl LogView {
    pub fn new() -> Self {
        let mut scroller = Scroller::new();
        scroller.horizontal = true;
        Self {
            lines: VecDeque::new(),
            dropped: 0,
            max_lines: None,
            family: "monospace".to_owned(),
            font_size: 12.0,
            fill: None,
            bordered: true,
            width: percent(1.0),
            height: Some(240.0),
            follows_tail: true,
            to_bottom: false,
            scroller,
            cells: TextCache::default(),
            size: Size::default(),
            widest: 0.0,
            longest: 0,
            advance: 0.0,
            anchor: None,
            lead: None,
            dragging: false,
            search: None,
            search_error: None,
            found: Vec::new(),
            current: None,
            reveal: None,
        }
    }

    pub fn lines<L: Into<LogLine>>(mut self, lines: impl IntoIterator<Item = L>) -> Self {
        self.append(lines.into_iter().map(Into::into));
        self.to_bottom = self.follows_tail;
        self
    }

    /// Keep at most `count` lines, dropping the oldest (unlimited by default).
    pub fn max_lines(mut self, count: usize) -> Self {
        self.max_lines = Some(count);
        self.trim();
        self
    }

    /// The font family by name (`"monospace"` by default).
    pub fn font_family(mut self, family: impl Into<String>) -> Self {
        self.family = family.into();
        self
    }

    /// The font size in logical pixels (12 by default).
    pub fn font_size(mut self, px: f32) -> Self {
        self.font_size = px.max(1.0);
        self
    }

    /// The background ([`Role::Surface`] by default).
    pub fn background(mut self, fill: impl Into<Paint>) -> Self {
        self.fill = Some(fill.into());
        self
    }

    /// The frame around the view (on by default).
    pub fn bordered(mut self, bordered: bool) -> Self {
        self.bordered = bordered;
        self
    }

    /// Keep the view at the bottom as lines arrive while it is there (on by default).
    pub fn follows_tail(mut self, follows: bool) -> Self {
        self.follows_tail = follows;
        self
    }

    /// Keep moving after a wheel gesture ends (on by default). Platforms that send
    /// their own momentum events (macOS trackpads) keep sending them either way.
    pub fn momentum(mut self, momentum: bool) -> Self {
        self.scroller.momentum = momentum;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = length(width);
        self
    }

    /// A fixed height (240 by default).
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    pub fn fill_height(mut self) -> Self {
        self.height = None;
        self
    }

    pub fn line_height(&self) -> f32 {
        TextStyle::new(self.font_size).line_height
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn line(&self, index: usize) -> Option<&LogLine> {
        self.lines.get(index)
    }

    pub fn push(&mut self, cx: &mut UpdateCx, line: impl Into<LogLine>) {
        self.extend(cx, [line.into()]);
    }

    pub fn extend<L: Into<LogLine>>(
        &mut self,
        cx: &mut UpdateCx,
        lines: impl IntoIterator<Item = L>,
    ) {
        if self.follows_tail && self.at_bottom() {
            self.to_bottom = true;
        }
        self.append(lines.into_iter().map(Into::into));
        cx.request_paint();
    }

    pub fn set_lines<L: Into<LogLine>>(
        &mut self,
        cx: &mut UpdateCx,
        lines: impl IntoIterator<Item = L>,
    ) {
        self.drop_front(self.lines.len());
        self.widest = 0.0;
        self.longest = 0;
        self.scroller.set_offset(Point::default());
        self.extend(cx, lines);
        self.to_bottom = self.follows_tail;
    }

    pub fn clear(&mut self, cx: &mut UpdateCx) {
        self.set_lines(cx, std::iter::empty::<LogLine>());
    }

    pub fn selected_lines(&self) -> Option<Range<usize>> {
        let (a, b) = (self.anchor?, self.lead?);
        Some(a.min(b)..a.max(b) + 1)
    }

    /// Select `lines` (an empty range clears the selection) and scroll to their end.
    pub fn select_lines(&mut self, cx: &mut UpdateCx, lines: Range<usize>) {
        let end = lines.end.min(self.lines.len());
        if lines.start >= end {
            self.anchor = None;
            self.lead = None;
        } else {
            self.anchor = Some(lines.start);
            self.lead = Some(end - 1);
            self.reveal_line(end - 1);
        }
        cx.request_paint();
    }

    /// The selected lines, one per line, or `None` without a selection.
    pub fn copy_text(&self) -> Option<String> {
        let lines = self.selected_lines()?;
        Some(self.text_of(lines))
    }

    pub fn all_text(&self) -> String {
        self.text_of(0..self.lines.len())
    }

    pub fn scroll_y(&self) -> f32 {
        self.scroller.offset.y
    }

    /// The lines on screen, from the last layout.
    pub fn visible_lines(&self) -> Range<usize> {
        let lh = self.line_height();
        let v = self.viewport();
        let first = (self.scroller.offset.y.max(0.0) / lh) as usize;
        let last = ((self.scroller.offset.y + v.height) / lh).ceil().max(0.0) as usize;
        first.min(self.lines.len())..last.min(self.lines.len())
    }

    pub fn scroll_to_line(&mut self, cx: &mut UpdateCx, index: usize) {
        self.to_bottom = false;
        self.reveal_line(index);
        cx.request_paint();
    }

    /// Apply what a [`super::FindBar`] asks (the view is read-only: replacing only
    /// moves to a match) and say where the search stands.
    pub fn find(&mut self, cx: &mut UpdateCx, action: &FindAction) -> FindStatus {
        match action {
            FindAction::Search { query, options } => {
                self.found.clear();
                self.current = None;
                self.search = None;
                self.search_error = None;
                if !query.is_empty() {
                    match compile(query, *options) {
                        Ok(re) => self.search = Some((re, query.clone(), *options)),
                        Err(FindError(message)) => self.search_error = Some(message),
                    }
                }
                self.search_lines(0);
                let top = self.visible_lines().start;
                let current = (self.found.iter().position(|m| m.line >= top))
                    .or((!self.found.is_empty()).then_some(0));
                self.go_to(current);
            }
            FindAction::Next | FindAction::Replace(_) | FindAction::ReplaceAll(_) => {
                self.step(true);
            }
            FindAction::Previous => self.step(false),
            FindAction::Close => {
                self.search = None;
                self.search_error = None;
                self.found.clear();
                self.current = None;
            }
        }
        cx.request_paint();
        self.status()
    }

    pub fn found(&self) -> &[LogMatch] {
        &self.found
    }

    pub fn current_found(&self) -> Option<usize> {
        self.current
    }

    fn status(&self) -> FindStatus {
        FindStatus {
            total: self.found.len(),
            current: self.current,
            error: self.search_error.clone(),
            replaced: None,
        }
    }

    fn step(&mut self, forward: bool) {
        let n = self.found.len();
        if n == 0 {
            return;
        }
        let next = match self.current {
            Some(c) if forward => (c + 1) % n,
            Some(c) => (c + n - 1) % n,
            None if forward => 0,
            None => n - 1,
        };
        self.go_to(Some(next));
    }

    fn go_to(&mut self, current: Option<usize>) {
        self.current = current;
        if let Some(c) = current {
            self.to_bottom = false;
            self.reveal = Some(c);
        }
    }

    fn reveal_line(&mut self, index: usize) {
        let lh = self.line_height();
        let x = self.scroller.offset.x;
        self.scroller
            .reveal(Rect::new(x, index as f32 * lh, 1.0, lh));
    }

    fn text_of(&self, lines: Range<usize>) -> String {
        let mut out = String::new();
        for (i, line) in self.lines.range(lines).enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(&line.text);
        }
        out
    }

    fn append(&mut self, lines: impl Iterator<Item = LogLine>) {
        let from = self.lines.len();
        for line in lines {
            self.longest = self.longest.max(line.text.chars().count());
            self.lines.push_back(line);
        }
        self.search_lines(from);
        self.trim();
    }

    fn search_lines(&mut self, from: usize) {
        let Some((re, ..)) = &self.search else { return };
        for (i, line) in self.lines.iter().enumerate().skip(from) {
            self.found.extend(
                (re.find_iter(&line.text))
                    .filter(|m| !m.is_empty())
                    .map(|m| LogMatch {
                        line: i,
                        range: m.range(),
                    }),
            );
        }
    }

    fn trim(&mut self) {
        if let Some(max) = self.max_lines
            && self.lines.len() > max
        {
            self.drop_front(self.lines.len() - max);
        }
    }

    /// Drop the first `n` lines, moving what points at lines along.
    fn drop_front(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        self.lines.drain(..n);
        self.dropped += n;
        let gone = self.found.partition_point(|m| m.line < n);
        self.found.drain(..gone);
        for m in &mut self.found {
            m.line -= n;
        }
        self.current = self.current.and_then(|c| c.checked_sub(gone));
        self.reveal = self.reveal.and_then(|c| c.checked_sub(gone));
        let shift = |i: Option<usize>| i.map(|i| i.saturating_sub(n));
        if self.lead.is_some_and(|l| l < n) && self.anchor.is_some_and(|a| a < n) {
            self.anchor = None;
            self.lead = None;
        }
        self.anchor = shift(self.anchor);
        self.lead = shift(self.lead);
        if self.lines.is_empty() {
            self.anchor = None;
            self.lead = None;
        }
        // Keep the lines on screen where they are.
        let lh = self.line_height();
        let y = (self.scroller.offset.y - n as f32 * lh).max(0.0);
        self.scroller
            .set_offset_unclamped(Point::new(self.scroller.offset.x, y));
    }

    fn inset(&self) -> f32 {
        if self.bordered { 1.0 } else { 0.0 }
    }

    fn viewport(&self) -> Rect {
        let i = self.inset();
        Rect::new(
            i,
            i,
            (self.size.width - 2.0 * i).max(0.0),
            (self.size.height - 2.0 * i).max(0.0),
        )
    }

    fn content(&self) -> Size {
        let width = self.widest.max(self.longest as f32 * self.advance) + 2.0 * PAD;
        Size::new(width, self.lines.len() as f32 * self.line_height())
    }

    fn at_bottom(&self) -> bool {
        self.size.height <= 0.0 || self.scroller.offset.y >= self.scroller.max().y - 0.5
    }

    fn sync(&mut self, size: Size) {
        self.size = size;
        self.scroller.set_geometry(self.viewport(), self.content());
        if std::mem::take(&mut self.to_bottom) {
            let max = self.scroller.max();
            self.scroller.set_offset(Point::new(0.0, max.y));
        }
        if let Some(c) = self.reveal.take()
            && let Some(m) = self.found.get(c)
        {
            let lh = self.line_height();
            // Byte offsets as a guess of where the match is: exact for ASCII.
            let x = PAD + self.line_chars(m.line, m.range.start) as f32 * self.advance;
            let width =
                (self.line_chars(m.line, m.range.end) as f32 * self.advance + PAD - x).max(1.0);
            let view = self.viewport().width;
            let x = if x + width <= view { 0.0 } else { x };
            self.scroller
                .reveal(Rect::new(x, m.line as f32 * lh, width.min(view), lh));
        }
    }

    fn line_chars(&self, line: usize, byte: usize) -> usize {
        self.lines[line].text[..byte].chars().count()
    }

    fn line_at(&self, p: Point) -> Option<usize> {
        let v = self.viewport();
        let row = ((p.y - v.y + self.scroller.offset.y) / self.line_height()).floor();
        (row >= 0.0 && (row as usize) < self.lines.len()).then_some(row as usize)
    }

    fn style(&self, store: &mut TextStore) -> TextStyle {
        TextStyle::new(self.font_size).family(store.font_family(&self.family))
    }

    fn key(&mut self, cx: &mut EventCx, key: Key, shift: bool, command: bool) -> bool {
        let n = self.lines.len();
        let page = (self.viewport().height / self.line_height())
            .floor()
            .max(1.0);
        let lh = self.line_height();
        let y = self.scroller.offset.y;
        match key {
            Key::Char('c') if command => {
                let Some(text) = self.copy_text() else {
                    return false;
                };
                cx.clipboard().write_text(&text);
            }
            Key::Char('a') if command && n > 0 => {
                self.anchor = Some(0);
                self.lead = Some(n - 1);
            }
            Key::ArrowUp | Key::ArrowDown if n > 0 => {
                let next = match (self.lead, key) {
                    (Some(l), Key::ArrowUp) => l.saturating_sub(1),
                    (Some(l), _) => (l + 1).min(n - 1),
                    (None, Key::ArrowUp) => n - 1,
                    (None, _) => 0,
                };
                if !shift || self.anchor.is_none() {
                    self.anchor = Some(next);
                }
                self.lead = Some(next);
                self.reveal_line(next);
            }
            Key::Home => {
                self.scroller.set_offset(Point::new(0.0, 0.0));
            }
            Key::End => {
                let max = self.scroller.max();
                self.scroller.set_offset(Point::new(0.0, max.y));
            }
            Key::PageUp => {
                let at = Point::new(self.scroller.offset.x, y - page * lh);
                self.scroller.set_offset(at);
            }
            Key::PageDown => {
                let at = Point::new(self.scroller.offset.x, y + page * lh);
                self.scroller.set_offset(at);
            }
            Key::Escape if self.anchor.is_some() => {
                self.anchor = None;
                self.lead = None;
            }
            _ => return false,
        }
        cx.request_paint();
        true
    }
}

impl Widget for LogView {
    fn style(&self) -> Style {
        let mut style = Style {
            size: LayoutSize {
                width: self.width,
                height: Dimension::auto(),
            },
            ..Default::default()
        };
        view_height(
            &mut style,
            self.height,
            self.line_height() + 2.0 * self.inset(),
        );
        style
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let style = self.style(cx.text_store());
        if self.advance == 0.0 {
            let id = cx.text_store().create("0000000000", style, None);
            self.advance = cx.text_store().size(id).width / 10.0;
            cx.text_store().remove(id);
        }
        self.sync(cx.size());
        let bounds = cx.bounds();
        let background = cx.color(self.fill.unwrap_or(Paint::Role(Role::Surface)));
        let frame = if self.bordered {
            Border {
                width: 1.0,
                color: cx.color(Role::Outline),
            }
        } else {
            Border::default()
        };
        cx.rounded_rect(bounds, 0.0, background, frame);
        let v = self.viewport();
        let lh = self.line_height();
        let off = self.scroller.offset;
        let focused = cx.is_focused();
        let (selected_fill, selected_ink) = selection_fill(cx, focused);
        let selected_ink = cx.color(selected_ink);
        let ink = cx.color(Role::Text);
        let selected = self.selected_lines().unwrap_or(0..0);
        let content_width = self.content().width.max(v.width);

        cx.push_clip(v);
        self.cells.begin();
        let lines = self.visible_lines();
        let mut found = self.found.partition_point(|m| m.line < lines.start);
        let mut widest = self.widest;
        for i in lines {
            let y = v.y + i as f32 * lh - off.y;
            let row = Rect::new(v.x - off.x, y, content_width, lh);
            let line = &self.lines[i];
            let is_selected = selected.contains(&i);
            if is_selected {
                cx.fill_rect(row, selected_fill);
            } else if let Some(fill) = line.background {
                cx.fill_rect(row, cx.color(fill));
            }
            let key = self.dropped + i;
            let (layout, size) = self.cells.get(cx.text_store(), key, &line.text, style);
            widest = widest.max(size.width);
            let Some(layout) = layout else { continue };
            let x = v.x + PAD - off.x;
            let ty = y + (lh - size.height) / 2.0;
            while let Some(m) = self.found.get(found).filter(|m| m.line == i) {
                let store = cx.text_store();
                let rects = store.selection_rects(layout, m.range.start, m.range.end);
                let color = if self.current == Some(found) {
                    FOUND_CURRENT
                } else {
                    FOUND
                };
                for rect in rects {
                    cx.rounded_rect(rect.translate(x, ty), 2.0, color, Border::default());
                }
                found += 1;
            }
            let color = if is_selected {
                selected_ink
            } else {
                line.color.map_or(ink, |c| cx.color(c))
            };
            cx.text(Rect::new(x, ty, size.width, size.height), layout, color);
        }
        self.cells.end(cx.text_store());
        cx.pop_clip();
        if widest > self.widest {
            self.widest = widest;
            self.scroller.set_geometry(v, self.content());
        }
        self.scroller.paint(cx);
        if focused {
            focus_ring(cx, bounds, 0.0);
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        self.sync(cx.size());
        if self.scroller.event(cx, event) {
            return;
        }
        match event {
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                let p = cx.local(*pos);
                cx.request_focus();
                if let Some(line) = self.line_at(p) {
                    if !cx.modifiers().shift || self.anchor.is_none() {
                        self.anchor = Some(line);
                    }
                    self.lead = Some(line);
                    self.dragging = true;
                    cx.capture_pointer();
                } else {
                    self.anchor = None;
                    self.lead = None;
                }
                cx.request_paint();
                cx.set_handled();
            }
            Event::PointerMove { pos } if self.dragging => {
                let p = cx.local(*pos);
                let v = self.viewport();
                let y = p.y.clamp(v.y, (v.bottom() - 1.0).max(v.y));
                if let Some(line) = self.line_at(Point::new(p.x, y))
                    && self.lead != Some(line)
                {
                    self.lead = Some(line);
                    self.reveal_line(line);
                    cx.request_paint();
                }
                cx.set_handled();
            }
            Event::PointerUp { .. } if self.dragging => {
                self.dragging = false;
                cx.release_pointer();
                cx.set_handled();
            }
            Event::KeyDown { key, modifiers }
                if cx.is_focused() && self.key(cx, *key, modifiers.shift, modifiers.command) =>
            {
                cx.set_handled();
            }
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Log)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        self.scroller.accessibility(node);
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.cells.release(text);
    }
}

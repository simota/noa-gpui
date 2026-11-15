use std::ops::Range;

use noa_gpui_layout::{MeasureInput, Style};
use noa_gpui_scene::{Border, Color, Point, Rect, Size};
use noa_gpui_text::{TextStore, TextStyle};

use super::appear::{Appear, shadow};
use super::editable::SingleLine;
use super::kbd::{KEYCAP_HEIGHT, keycaps_width, paint_keycaps, shape_keycaps};
use super::menu::MenuItem;
use super::menu_bar::{MenuBarChosen, MenuBarMenu};
use super::{CachedText, Icon};
use crate::event::{Cursor, Event, Key, PointerButton};
use crate::overlay::{OverlayOptions, Placement};
use crate::shortcut::Shortcut;
use crate::theme::{Corner, Role};
use crate::tree::{UiTree, WidgetId};
use crate::widget::{EventCx, MeasureCx, PaintCx, Widget};

/// ⌘K and ⇧⌘P (Ctrl+K and Ctrl+Shift+P outside Apple platforms). Register them with
/// [`UiTree::add_shortcut`].
pub const COMMAND_PALETTE_SHORTCUTS: [Shortcut; 2] =
    [Shortcut::command('k'), Shortcut::command('p').with_shift()];

/// Carries the command's index in the commands given. The palette closes itself after
/// emitting it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandChosen(pub usize);

#[derive(Debug, Clone, PartialEq)]
pub struct PaletteCommand {
    title: String,
    detail: Option<String>,
    keywords: String,
    shortcut: Option<Shortcut>,
    icon: Option<Icon>,
    checked: bool,
}

impl PaletteCommand {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            detail: None,
            keywords: String::new(),
            shortcut: None,
            icon: None,
            checked: false,
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Searched but not shown.
    pub fn keywords(mut self, keywords: impl Into<String>) -> Self {
        self.keywords = keywords.into();
        self
    }

    /// Shown as keycaps; not registered.
    pub fn shortcut(mut self, shortcut: Shortcut) -> Self {
        self.shortcut = Some(shortcut);
        self
    }

    /// Screen readers hear a checked command as checked.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    /// 16 × 16, drawn in the accent color.
    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn is_checked(&self) -> bool {
        self.checked
    }
}

/// On [`CommandChosen`] with index `i`, carry out `commands[i].1`. Separators, disabled
/// items and items that only open a submenu are left out. An item in a submenu is
/// titled with the items it is under ("Appearance: Dark").
pub fn menu_commands(menus: &[MenuBarMenu]) -> Vec<(PaletteCommand, MenuBarChosen)> {
    fn walk(
        items: &[MenuItem],
        prefix: &str,
        menu: usize,
        title: &str,
        next: &mut usize,
        out: &mut Vec<(PaletteCommand, MenuBarChosen)>,
    ) {
        for item in items {
            let index = *next;
            *next += 1;
            let label = if prefix.is_empty() {
                item.label().to_owned()
            } else {
                format!("{prefix}: {}", item.label())
            };
            if !item.submenu_items().is_empty() {
                // The submenu's items are counted even when it is disabled, so the
                // indices of the items after it stay right.
                let mut nested = Vec::new();
                walk(item.submenu_items(), &label, menu, title, next, &mut nested);
                if item.is_enabled() {
                    out.extend(nested);
                }
                continue;
            }
            if item.is_separator() || !item.is_enabled() {
                continue;
            }
            let mut command = PaletteCommand::new(label).detail(title);
            if let Some(shortcut) = item.shortcut_key() {
                command = command.shortcut(shortcut);
            }
            command = command.checked(item.is_checked());
            out.push((command, MenuBarChosen { menu, item: index }));
        }
    }
    let mut out = Vec::new();
    for (menu, m) in menus.iter().enumerate() {
        let mut next = 0;
        walk(&m.items, "", menu, &m.title, &mut next, &mut out);
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuzzyMatch {
    /// Higher is better.
    pub score: i32,
    /// Matched characters of `text`, merged into runs.
    pub ranges: Vec<Range<usize>>,
}

/// Every character of `pattern` (ignoring case and spaces) must appear in `text` in
/// order. Matches score higher at the start of `text`, at word starts and in
/// consecutive runs, and lower for skipped characters. An empty pattern matches
/// everything with a score of 0.
pub fn fuzzy_match(pattern: &str, text: &str) -> Option<FuzzyMatch> {
    // One character for each one, as for `text` below: some letters have a lowercase
    // of several characters, which would never line up with the text's.
    let fold = |c: char| c.to_lowercase().next().unwrap_or(c);
    let pattern: Vec<char> = pattern
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(fold)
        .collect();
    if pattern.is_empty() {
        return Some(FuzzyMatch {
            score: 0,
            ranges: Vec::new(),
        });
    }
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let lower: Vec<char> = chars.iter().map(|&(_, c)| fold(c)).collect();
    let word_start = |i: usize| {
        i == 0 || {
            let prev = chars[i - 1].1;
            let c = chars[i].1;
            !prev.is_alphanumeric() || (prev.is_lowercase() && c.is_uppercase())
        }
    };
    // Greedy from every place the first character occurs; keep the best.
    let mut best: Option<(i32, Vec<usize>)> = None;
    for start in (0..lower.len()).filter(|&i| lower[i] == pattern[0]) {
        let mut at = vec![start];
        let mut next = start + 1;
        for (k, &p) in pattern.iter().enumerate().skip(1) {
            // Prefer the next word start holding the character over a nearer
            // occurrence inside a word, unless the nearer one continues a run or the
            // rest of the pattern would no longer fit after the word start.
            let Some(found) = (next..lower.len()).find(|&i| lower[i] == p) else {
                break;
            };
            let rest = &pattern[k + 1..];
            let pick = if found == next {
                found
            } else {
                (found..lower.len())
                    .find(|&i| {
                        lower[i] == p && word_start(i) && is_subsequence(rest, &lower[i + 1..])
                    })
                    .unwrap_or(found)
            };
            at.push(pick);
            next = pick + 1;
        }
        if at.len() < pattern.len() {
            // Later starts cannot find more.
            break;
        }
        let mut score = 0;
        for (k, &i) in at.iter().enumerate() {
            score += 1;
            if i == 0 {
                score += 8;
            } else if word_start(i) {
                score += 6;
            }
            if k > 0 {
                let gap = (i - at[k - 1] - 1) as i32;
                if gap == 0 {
                    score += 4;
                } else {
                    score -= gap.min(6);
                }
            }
        }
        if best.as_ref().is_none_or(|(s, _)| score > *s) {
            best = Some((score, at));
        }
    }
    let (score, at) = best?;
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for i in at {
        let (start, c) = chars[i];
        let end = start + c.len_utf8();
        match ranges.last_mut() {
            Some(last) if last.end == start => last.end = end,
            _ => ranges.push(start..end),
        }
    }
    Some(FuzzyMatch { score, ranges })
}

fn is_subsequence(pattern: &[char], text: &[char]) -> bool {
    let mut text = text.iter();
    pattern.iter().all(|p| text.any(|c| c == p))
}

const WIDTH: f32 = 560.0;
const FIELD_H: f32 = 52.0;
const FIELD_LEFT: f32 = 44.0;
const FIELD_RIGHT: f32 = 16.0;
const ROW_H: f32 = 40.0;
const MAX_ROWS: usize = 8;
const LIST_PAD: f32 = 8.0;
const ROW_INSET: f32 = 12.0;
const ICON: f32 = 16.0;
const CHECK: f32 = 10.0;
const RADIUS: f32 = 12.0;
const TOP: f32 = 72.0;
/// Seconds.
const APPEAR: f64 = 0.14;
const EMPTY: &str = "No matching commands";

/// Most recent first. Keep it between palettes, as the palette closes after a choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentCommands {
    titles: Vec<String>,
    /// For each command noted with [`RecentCommands::note_command`], its detail (empty
    /// when it has none), which tells apart commands with the same title ("Close" in
    /// the File and in the Window menu). `None` for a title noted alone.
    details: Vec<Option<String>>,
    limit: usize,
}

impl Default for RecentCommands {
    fn default() -> Self {
        Self::new(8)
    }
}

impl RecentCommands {
    pub fn new(limit: usize) -> Self {
        Self {
            titles: Vec::new(),
            details: Vec::new(),
            limit,
        }
    }

    /// Moves `title` to the front (once). The palette lists the first command with that
    /// title.
    pub fn note(&mut self, title: impl Into<String>) {
        self.remember(title.into(), None);
    }

    /// Moves `command` to the front (once). Its detail tells it apart from commands with
    /// the same title.
    pub fn note_command(&mut self, command: &PaletteCommand) {
        let detail = command.detail.clone().unwrap_or_default();
        self.remember(command.title.clone(), Some(detail));
    }

    fn remember(&mut self, title: String, detail: Option<String>) {
        if let Some(i) =
            (0..self.titles.len()).find(|&i| self.titles[i] == title && self.details[i] == detail)
        {
            self.titles.remove(i);
            self.details.remove(i);
        }
        self.titles.insert(0, title);
        self.details.insert(0, detail);
        self.titles.truncate(self.limit);
        self.details.truncate(self.limit);
    }

    /// Most recent first.
    pub fn titles(&self) -> &[String] {
        &self.titles
    }

    /// One command per line: its title, then a tab and its detail when it was noted with
    /// one. A title holding a tab or a line break does not read back.
    pub fn to_text(&self) -> String {
        let lines: Vec<String> = (self.titles.iter().zip(&self.details))
            .map(|(title, detail)| match detail {
                Some(detail) => format!("{title}\t{detail}"),
                None => title.clone(),
            })
            .collect();
        lines.join("\n")
    }

    pub fn from_text(text: &str, limit: usize) -> Self {
        let mut recent = Self::new(limit);
        for line in text.lines().rev().filter(|t| !t.trim().is_empty()) {
            match line.split_once('\t') {
                Some((title, detail)) => recent.remember(title.into(), Some(detail.into())),
                None => recent.remember(line.into(), None),
            }
        }
        recent
    }
}

#[derive(Debug, Clone)]
struct Found {
    command: usize,
    /// Matched bytes of the title (empty when only the keywords matched).
    ranges: Vec<Range<usize>>,
}

/// A command palette floating near the top of the window (VS Code's ⇧⌘P, Raycast,
/// Linear's ⌘K). Emits [`CommandChosen`]. While it is open every key goes to it, so
/// shortcuts registered on the tree do not fire.
#[derive(Debug)]
pub struct CommandPalette {
    commands: Vec<PaletteCommand>,
    keys: Vec<Vec<String>>,
    top: f32,
    line: SingleLine,
    found: Vec<Found>,
    /// Index into `found` of the highlighted command.
    highlight: usize,
    /// Index into `found` of the first row shown.
    first: usize,
    appear: Appear,
    closing: bool,
    titles: Vec<CachedText>,
    details: Vec<CachedText>,
    keycaps: Vec<Vec<CachedText>>,
    empty_text: CachedText,
    query: String,
    /// For each command, how recently it was chosen (0: last), if it was.
    recency: Vec<Option<usize>>,
}

impl CommandPalette {
    pub fn new(commands: Vec<PaletteCommand>) -> Self {
        let mut line = SingleLine::new("Type a command or search…".into(), WIDTH, 0.0);
        line.pad_left = FIELD_LEFT;
        line.pad_right = FIELD_RIGHT;
        line.edit.style = TextStyle::new(16.0);
        let n = commands.len();
        let mut palette = Self {
            commands,
            keys: Vec::new(),
            top: TOP,
            line,
            found: Vec::new(),
            highlight: 0,
            first: 0,
            appear: Appear::new(APPEAR),
            closing: false,
            titles: (0..n).map(|_| CachedText::default()).collect(),
            details: (0..n).map(|_| CachedText::default()).collect(),
            keycaps: (0..n).map(|_| Vec::new()).collect(),
            empty_text: CachedText::default(),
            query: String::new(),
            recency: vec![None; n],
        };
        palette.set_glyphs(cfg!(target_vendor = "apple"));
        palette.filter();
        palette
    }

    /// Each entry picks the first command with its title (and detail). Commands no
    /// longer given are skipped.
    pub fn recent(mut self, recent: &RecentCommands) -> Self {
        for (rank, (title, detail)) in recent.titles.iter().zip(&recent.details).enumerate() {
            if let Some(i) = self.commands.iter().position(|c| {
                c.title == *title
                    && detail
                        .as_deref()
                        .is_none_or(|d| c.detail.as_deref().unwrap_or_default() == d)
            }) && self.recency[i].is_none()
            {
                self.recency[i] = Some(rank);
            }
        }
        self.filter();
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.line.edit.placeholder = placeholder.into();
        self
    }

    pub fn query(mut self, query: impl Into<String>) -> Self {
        self.line.edit.editor.set_text(&query.into());
        self.filter();
        self
    }

    /// 72 by default.
    pub fn top(mut self, top: f32) -> Self {
        self.top = top;
        self
    }

    /// Opens with keyboard focus. Shortcuts are written as [`UiTree::shortcut_glyphs`]
    /// writes them.
    pub fn open(mut self, tree: &mut UiTree) -> WidgetId {
        self.set_glyphs(tree.shortcut_glyphs());
        let placement = Placement::Sheet { top: self.top };
        // Dismissed by a click outside, which is consumed, as a menu is.
        let id = tree.open_overlay(self, placement, OverlayOptions::MENU);
        tree.set_focus(Some(id));
        tree.update::<CommandPalette, _>(id, |_, cx| cx.request_animation_frame());
        id
    }

    pub fn commands(&self) -> &[PaletteCommand] {
        &self.commands
    }

    pub fn query_text(&self) -> &str {
        self.line.edit.editor.text()
    }

    /// Best first; indices into the commands.
    pub fn matches(&self) -> Vec<usize> {
        self.found.iter().map(|f| f.command).collect()
    }

    /// An index into the commands.
    pub fn highlighted(&self) -> Option<usize> {
        self.found.get(self.highlight).map(|f| f.command)
    }

    fn set_glyphs(&mut self, glyphs: bool) {
        self.keys = self
            .commands
            .iter()
            .map(|c| c.shortcut.map_or_else(Vec::new, |s| s.keys(glyphs)))
            .collect();
    }

    /// Best first; ties keep their order.
    fn filter(&mut self) {
        let query = self.line.edit.editor.text().to_owned();
        let mut found: Vec<(i32, Found)> = self
            .commands
            .iter()
            .enumerate()
            .filter_map(|(command, c)| {
                if let Some(m) = fuzzy_match(&query, &c.title) {
                    return Some((
                        m.score,
                        Found {
                            command,
                            ranges: m.ranges,
                        },
                    ));
                }
                // Keywords find a command too, ranked below title matches.
                let m = fuzzy_match(&query, &c.keywords)?;
                Some((
                    m.score - 100,
                    Found {
                        command,
                        ranges: Vec::new(),
                    },
                ))
            })
            .collect();
        // Best first; among equals (every command, before anything is typed) the
        // recently chosen first, latest first.
        let recency = &self.recency;
        found.sort_by_key(|(score, f)| {
            let recent = recency[f.command].unwrap_or(usize::MAX);
            (std::cmp::Reverse(*score), recent)
        });
        self.found = found.into_iter().map(|(_, f)| f).collect();
        self.highlight = 0;
        self.first = 0;
        self.query = query;
    }

    fn rows(&self) -> usize {
        self.found.len().clamp(1, MAX_ROWS)
    }

    fn height(&self) -> f32 {
        FIELD_H + 1.0 + 2.0 * LIST_PAD + self.rows() as f32 * ROW_H
    }

    fn text_origin(&self) -> Point {
        let line_height = self.line.edit.style.line_height;
        Point::new(
            FIELD_LEFT - self.line.scroll_x,
            (FIELD_H - line_height) / 2.0,
        )
    }

    fn list_top(&self) -> f32 {
        FIELD_H + 1.0 + LIST_PAD
    }

    /// `p` is in widget coordinates; returns an index into `found`.
    fn row_at(&self, p: Point) -> Option<usize> {
        let y = p.y - self.list_top();
        if y < 0.0 || p.x < LIST_PAD || p.x > WIDTH - LIST_PAD {
            return None;
        }
        let row = (y / ROW_H) as usize;
        (row < self.rows())
            .then_some(self.first + row)
            .filter(|&i| i < self.found.len())
    }

    fn highlight_at(&mut self, cx: &mut EventCx, index: usize) {
        if self.found.is_empty() {
            return;
        }
        let index = index.min(self.found.len() - 1);
        self.highlight = index;
        let rows = self.rows();
        if index < self.first {
            self.first = index;
        } else if index >= self.first + rows {
            self.first = index + 1 - rows;
        }
        cx.request_paint();
    }

    fn choose(&mut self, cx: &mut EventCx, index: usize) {
        if let Some(found) = self.found.get(index) {
            cx.emit(CommandChosen(found.command));
            self.close(cx);
        }
    }

    fn close(&mut self, cx: &mut EventCx) {
        self.closing = true;
        self.appear.close(cx.now());
        cx.request_animation_frame();
    }

    fn after_edit(&mut self, cx: &mut EventCx) {
        if self.line.edit.editor.text() != self.query {
            self.filter();
            cx.request_layout();
            cx.request_paint();
        }
    }

    fn key(&mut self, cx: &mut EventCx, key: Key) -> bool {
        let n = self.found.len();
        match key {
            Key::Escape => self.close(cx),
            Key::Enter => self.choose(cx, self.highlight),
            Key::ArrowDown if n > 0 => self.highlight_at(cx, (self.highlight + 1) % n),
            Key::ArrowUp if n > 0 => self.highlight_at(cx, (self.highlight + n - 1) % n),
            Key::PageDown => {
                let to = self.highlight + self.rows();
                self.highlight_at(cx, to);
            }
            Key::PageUp => {
                let to = self.highlight.saturating_sub(self.rows());
                self.highlight_at(cx, to);
            }
            Key::Tab => {}
            _ => return false,
        }
        true
    }

    fn paint_magnifier(cx: &mut PaintCx, color: Color) {
        let lens = Rect::new(17.0, FIELD_H / 2.0 - 8.0, 12.0, 12.0);
        cx.rounded_rect(lens, 6.0, Color::TRANSPARENT, Border { width: 1.5, color });
        cx.line(
            Point::new(lens.right() - 1.5, lens.bottom() - 1.5),
            Point::new(lens.right() + 3.0, lens.bottom() + 3.0),
            1.5,
            color,
        );
    }

    fn paint_row(&self, cx: &mut PaintCx, row: Rect, found: &Found, highlighted: bool) {
        let accent = cx.color(Role::Accent);
        if highlighted {
            let fill = cx.theme().accent.with_alpha(0.16);
            let radius = cx.corner(Corner::Inner, 6.0, row.height);
            cx.rounded_rect(row, radius, fill, Border::default());
        }
        let command = &self.commands[found.command];
        let mut x = row.x + ROW_INSET;
        if let Some(icon) = &command.icon {
            let rect = Rect::new(x, row.y + (row.height - ICON) / 2.0, ICON, ICON);
            icon.paint(cx, rect, accent);
            x += ICON + 10.0;
        }
        let mut right = row.right() - ROW_INSET;
        let caps = &self.keycaps[found.command];
        if !caps.is_empty() {
            let width = keycaps_width(caps);
            right -= width;
            let origin = Point::new(right, row.y + (row.height - KEYCAP_HEIGHT) / 2.0);
            paint_keycaps(cx, caps, origin);
            right -= 12.0;
        }
        if command.checked {
            right -= CHECK;
            let y = row.y + (row.height - CHECK) / 2.0;
            let ink = cx.color(Role::Text);
            cx.checkmark(Rect::new(right, y, CHECK, CHECK), 1.8, ink);
            right -= 12.0;
        }
        let detail = &self.details[found.command];
        if command.detail.is_some()
            && let Some(layout) = detail.layout()
        {
            let size = detail.size();
            right -= size.width;
            let y = row.y + (row.height - size.height) / 2.0;
            let muted = cx.color(Role::Muted);
            cx.text(Rect::new(right, y, size.width, size.height), layout, muted);
            right -= 12.0;
        }
        let title = &self.titles[found.command];
        let Some(layout) = title.layout() else {
            return;
        };
        let size = title.size();
        let y = row.y + (row.height - size.height) / 2.0;
        cx.push_clip(Rect::new(x, row.y, (right - x).max(0.0), row.height));
        let mark = cx.theme().accent.with_alpha(0.24);
        for range in &found.ranges {
            let rects = cx
                .text_store()
                .selection_rects(layout, range.start, range.end);
            for r in rects {
                let r = Rect::new(x + r.x, y + r.y + 1.0, r.width, r.height - 2.0);
                cx.rounded_rect(r, 3.0, mark, Border::default());
            }
        }
        let ink = cx.color(Role::Text);
        cx.text(Rect::new(x, y, size.width, size.height), layout, ink);
        cx.pop_clip();
    }
}

impl Widget for CommandPalette {
    fn cursor(&self, local: Point, _size: Size) -> Option<Cursor> {
        (local.y < FIELD_H)
            .then(|| self.line.cursor(local))
            .flatten()
    }

    fn style(&self) -> Style {
        Style {
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn measures(&self) -> bool {
        true
    }

    fn measure(&mut self, cx: &mut MeasureCx, _input: MeasureInput) -> Size {
        self.line.measure(cx.text);
        let title_style = TextStyle::new(14.0);
        let detail_style = TextStyle::new(12.0);
        for (i, command) in self.commands.iter().enumerate() {
            self.titles[i].shape(cx.text, &command.title, title_style, None);
            if let Some(detail) = &command.detail {
                self.details[i].shape(cx.text, detail, detail_style, None);
            }
            shape_keycaps(cx.text, &self.keys[i], &mut self.keycaps[i]);
            self.keycaps[i].truncate(self.keys[i].len());
        }
        self.empty_text
            .shape(cx.text, EMPTY, TextStyle::new(14.0), None);
        Size::new(WIDTH, self.height())
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let radius = cx.corner(Corner::Container, RADIUS, bounds.height);
        shadow(cx, bounds, radius);
        let fill = cx.color(Role::Surface);
        let border = Border {
            width: 1.0,
            color: cx.color(Role::Outline),
        };
        cx.floating_surface(bounds, radius, fill, border);

        let muted = cx.color(Role::Muted);
        Self::paint_magnifier(cx, muted);
        cx.push_clip(Rect::new(
            FIELD_LEFT - 2.0,
            0.0,
            WIDTH - FIELD_LEFT - FIELD_RIGHT + 4.0,
            FIELD_H,
        ));
        let focused = cx.is_focused();
        self.line.edit.paint(cx, self.text_origin(), focused);
        cx.pop_clip();
        let line = cx.color(Role::Separator);
        cx.fill_rect(Rect::new(0.0, FIELD_H, WIDTH, 1.0), line);

        let top = self.list_top();
        if self.found.is_empty() {
            if let Some(layout) = self.empty_text.layout() {
                let size = self.empty_text.size();
                let at = Point::new(
                    (WIDTH - size.width) / 2.0,
                    top + (ROW_H - size.height) / 2.0,
                );
                cx.text(
                    Rect::new(at.x, at.y, size.width, size.height),
                    layout,
                    muted,
                );
            }
            return;
        }
        let rows = self.rows();
        for (row, index) in (self.first..self.found.len()).take(rows).enumerate() {
            let rect = Rect::new(
                LIST_PAD,
                top + row as f32 * ROW_H,
                WIDTH - 2.0 * LIST_PAD,
                ROW_H,
            );
            self.paint_row(cx, rect, &self.found[index], index == self.highlight);
        }
        if self.found.len() > rows {
            let track = rows as f32 * ROW_H;
            let length = (track * rows as f32 / self.found.len() as f32).max(16.0);
            let max_first = (self.found.len() - rows) as f32;
            let y = top + (track - length) * self.first as f32 / max_first;
            let thumb = Rect::new(WIDTH - 5.0, y, 3.0, length);
            cx.rounded_rect(thumb, 1.5, muted.with_alpha(0.5), Border::default());
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        if let Event::AnimationFrame { now } = event {
            if self.appear.frame(cx, *now) {
                cx.close_overlay();
            }
            cx.request_composite();
            return;
        }
        if self.closing {
            cx.set_handled();
            return;
        }
        let origin = self.text_origin();
        let composing = !self.line.edit.composition().is_empty();
        match event {
            Event::KeyDown { key, .. } if !composing && self.key(cx, *key) => {
                cx.set_handled();
                return;
            }
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                let p = cx.local(*pos);
                cx.set_handled();
                if let Some(row) = self.row_at(p) {
                    self.choose(cx, row);
                    return;
                }
                if p.y >= FIELD_H {
                    return;
                }
            }
            Event::PointerMove { pos } if !cx.has_capture() => {
                if let Some(row) = self.row_at(cx.local(*pos))
                    && row != self.highlight
                {
                    self.highlight = row;
                    cx.request_paint();
                }
                return;
            }
            Event::Wheel { delta, .. } => {
                let rows = self.rows();
                let max_first = self.found.len().saturating_sub(rows);
                let first = if delta.y > 0.0 {
                    self.first.saturating_sub(1)
                } else if delta.y < 0.0 {
                    (self.first + 1).min(max_first)
                } else {
                    self.first
                };
                if first != self.first {
                    self.first = first;
                    self.highlight = self.highlight.clamp(first, first + rows - 1);
                    cx.request_paint();
                }
                cx.set_handled();
                return;
            }
            _ => {}
        }
        self.line.edit.event(cx, event, origin);
        self.after_edit(cx);
        if matches!(event, Event::KeyDown { .. }) {
            // Every key is the palette's: nothing below acts on one.
            cx.set_handled();
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn opacity(&self) -> f32 {
        self.appear.value()
    }

    fn text_input_area(&self) -> Option<Rect> {
        Some(self.line.edit.input_area(self.text_origin()))
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::ComboBox)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        node.set_label("Command palette");
        self.line.edit.accessibility(node);
        node.set_expanded(true);
        node.set_has_popup(accesskit::HasPopup::Listbox);
        node.set_modal();
    }

    fn accessibility_items(&self) -> Vec<accesskit::Node> {
        let top = self.list_top();
        (self.first..self.found.len())
            .take(self.rows())
            .enumerate()
            .map(|(row, index)| {
                let command = &self.commands[self.found[index].command];
                let mut node = accesskit::Node::new(accesskit::Role::ListBoxOption);
                node.set_label(command.title.clone());
                if let Some(detail) = &command.detail {
                    node.set_description(detail.clone());
                }
                if let Some(shortcut) = command.shortcut {
                    node.set_keyboard_shortcut(shortcut.label(false));
                }
                if command.checked {
                    node.set_toggled(accesskit::Toggled::True);
                }
                node.set_selected(index == self.highlight);
                let y = top + row as f32 * ROW_H;
                node.set_bounds(accesskit::Rect {
                    x0: f64::from(LIST_PAD),
                    y0: f64::from(y),
                    x1: f64::from(WIDTH - LIST_PAD),
                    y1: f64::from(y + ROW_H),
                });
                node.add_action(accesskit::Action::Click);
                node
            })
            .collect()
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.line.edit.removed(text);
        for cached in self.titles.iter_mut().chain(&mut self.details) {
            cached.release(text);
        }
        for caps in &mut self.keycaps {
            for cached in caps {
                cached.release(text);
            }
        }
        self.empty_text.release(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_in_order_ignoring_case_and_spaces() {
        assert!(fuzzy_match("tgsb", "Toggle Sidebar").is_some());
        assert!(fuzzy_match("tog side", "Toggle Sidebar").is_some());
        assert!(fuzzy_match("bst", "Toggle Sidebar").is_none());
        assert_eq!(fuzzy_match("", "anything").unwrap().score, 0);
        // Letters whose lowercase is more than one character: a title matches itself.
        assert!(fuzzy_match("İstanbul", "İstanbul").is_some());
        assert!(fuzzy_match("ist", "İstanbul").is_some());
    }

    #[test]
    fn word_starts_and_runs_score_higher() {
        let start = fuzzy_match("sb", "Show Sidebar").unwrap();
        let inner = fuzzy_match("sb", "Absorb").unwrap();
        assert!(start.score > inner.score);
        let run = fuzzy_match("side", "Sidebar").unwrap();
        let spread = fuzzy_match("side", "Show Inline Details").unwrap();
        assert!(run.score > spread.score);
    }

    #[test]
    fn ranges_are_merged_byte_ranges_of_the_matched_characters() {
        let m = fuzzy_match("tosi", "Toggle Sidebar").unwrap();
        assert_eq!(m.ranges, [0..2, 7..9]);
        // Word starts are preferred over nearer letters inside words.
        let m = fuzzy_match("ts", "Toggle Sidebar").unwrap();
        assert_eq!(m.ranges, [0..1, 7..8]);
        // ...unless the rest of the pattern no longer fits after the word start.
        let m = fuzzy_match("xyz", "x ay z yq").unwrap();
        assert_eq!(m.ranges, [0..1, 3..4, 5..6]);
        let m = fuzzy_match("設定", "環境設定を開く").unwrap();
        assert_eq!(m.ranges, vec![6..12]);
    }
}

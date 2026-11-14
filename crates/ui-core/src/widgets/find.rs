use std::ops::Range;

use regex::{Regex, RegexBuilder};
use ui_layout::style::{Dimension, Size as LayoutSize, percent};
use ui_layout::{MeasureInput, Style};
use ui_scene::{Border, Color, Direction, Point, Rect, Size};
use ui_text::{TextStore, TextStyle};

use super::editable::{Editable, SingleLine};
use super::{CachedText, LogView, TextArea, TextView};
use crate::event::{Cursor, Event, Key, Message, PointerButton};
use crate::shortcut::Shortcut;
use crate::theme::{Corner, Role};
use crate::tree::{UiTree, WidgetId};
use crate::widget::{EventCx, MeasureCx, PaintCx, UpdateCx, Widget};

pub(crate) const FOUND: Color = Color::rgba(1.0, 0.85, 0.2, 0.35);
pub(crate) const FOUND_CURRENT: Color = Color::rgba(1.0, 0.75, 0.0, 0.85);

/// ⌘F (Ctrl+F elsewhere). Register it with [`UiTree::add_shortcut`].
pub const FIND_SHORTCUT: Shortcut = Shortcut::command('f');

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FindOptions {
    pub case_sensitive: bool,
    /// Only whole words: no letter, digit or `_` right before or after a match.
    pub whole_word: bool,
    /// The query is a regular expression (Rust `regex` syntax); replacements may then
    /// use `$1` or `${name}` for its groups.
    pub regex: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindError(pub String);

pub(crate) fn compile(query: &str, options: FindOptions) -> Result<Regex, FindError> {
    let pattern = if options.regex {
        query.to_owned()
    } else {
        regex::escape(query)
    };
    // A whole word has no word character right before or after it. The pattern asks
    // for that itself, rather than matches being checked afterwards: a match that is
    // not a whole word may hide one that is ("a a" in "ba a a"), and looking again
    // from inside each rejected match takes quadratic time on a long run of them.
    let pattern = if options.whole_word {
        format!(r"\b{{start-half}}(?:{pattern})\b{{end-half}}")
    } else {
        pattern
    };
    RegexBuilder::new(&pattern)
        .case_insensitive(!options.case_sensitive)
        .multi_line(true)
        .build()
        .map_err(|e| FindError(e.to_string()))
}

/// The byte ranges of `text` that `query` matches, in order and not overlapping. An
/// empty query matches nothing, and neither do empty matches of a regular expression.
pub fn find_matches(
    text: &str,
    query: &str,
    options: FindOptions,
) -> Result<Vec<Range<usize>>, FindError> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let re = compile(query, options)?;
    Ok(re
        .find_iter(text)
        .map(|m| m.range())
        .filter(|r| !r.is_empty())
        .collect())
}

fn expand(
    text: &str,
    range: &Range<usize>,
    re: &Regex,
    options: FindOptions,
    replacement: &str,
) -> String {
    if !options.regex {
        return replacement.to_owned();
    }
    match re
        .captures_at(text, range.start)
        .filter(|c| c.get(0).map(|m| m.range()) == Some(range.clone()))
    {
        Some(captures) => {
            let mut out = String::new();
            captures.expand(replacement, &mut out);
            out
        }
        None => replacement.to_owned(),
    }
}

/// Returns the new text and the number of matches.
pub fn replace_all(
    text: &str,
    query: &str,
    options: FindOptions,
    replacement: &str,
) -> Result<(String, usize), FindError> {
    let matches = find_matches(text, query, options)?;
    if matches.is_empty() {
        return Ok((text.to_owned(), 0));
    }
    let re = compile(query, options)?;
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for range in &matches {
        out.push_str(&text[last..range.start]);
        out.push_str(&expand(text, range, &re, options, replacement));
        last = range.end;
    }
    out.push_str(&text[last..]);
    Ok((out, matches.len()))
}

/// Emitted by a [`FindBar`]. Hand it to [`drive_find`], or to [`TextView::find`] /
/// [`TextArea::find`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindAction {
    Search {
        query: String,
        options: FindOptions,
    },
    /// Wraps around.
    Next,
    Previous,
    /// Replaces the current match and moves to the next. A read-only text is not changed;
    /// both replacements then only select a match.
    Replace(String),
    ReplaceAll(String),
    /// Removes the highlights.
    Close,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FindStatus {
    pub total: usize,
    /// Selected in the text.
    pub current: Option<usize>,
    pub error: Option<String>,
    /// How many matches the last [`FindAction::ReplaceAll`] replaced.
    pub replaced: Option<usize>,
}

#[derive(Debug, Default)]
pub(crate) struct Finder {
    query: String,
    options: FindOptions,
    active: bool,
    /// The text the matches were found in.
    searched: String,
    error: Option<String>,
}

impl Finder {
    /// Find the matches again if the text changed since.
    pub fn sync(&mut self, edit: &mut Editable) {
        if !self.active || self.searched == edit.editor.text() {
            return;
        }
        self.search(edit);
    }

    fn search(&mut self, edit: &mut Editable) {
        self.searched = edit.editor.text().to_owned();
        match find_matches(&self.searched, &self.query, self.options) {
            Ok(found) => {
                self.error = None;
                let current = edit.found_current.and_then(|c| edit.found.get(c)).cloned();
                edit.found = found;
                edit.found_current = current.and_then(|c| edit.found.iter().position(|r| *r == c));
            }
            Err(FindError(message)) => {
                self.error = Some(message);
                edit.found.clear();
                edit.found_current = None;
            }
        }
    }

    fn status(&self, edit: &Editable) -> FindStatus {
        FindStatus {
            total: edit.found.len(),
            current: edit.found_current,
            error: self.error.clone(),
            replaced: None,
        }
    }

    /// The match after (or before) the current one, or else the selection, wrapping
    /// around.
    fn neighbor(&self, edit: &Editable, forward: bool) -> Option<usize> {
        let n = edit.found.len();
        if n == 0 {
            return None;
        }
        if let Some(c) = edit.found_current.filter(|&c| c < n) {
            return Some(if forward {
                (c + 1) % n
            } else {
                (c + n - 1) % n
            });
        }
        let selection = edit.editor.selection();
        Some(if forward {
            edit.found
                .iter()
                .position(|r| r.start >= selection.end && *r != selection)
                .unwrap_or(0)
        } else {
            edit.found
                .iter()
                .rposition(|r| r.end <= selection.start && *r != selection)
                .unwrap_or(n - 1)
        })
    }

    fn select(edit: &mut Editable, index: Option<usize>) {
        edit.found_current = index;
        if let Some(range) = index.and_then(|i| edit.found.get(i)).cloned() {
            edit.editor.select(range);
        }
    }

    pub fn apply(&mut self, edit: &mut Editable, action: &FindAction) -> FindStatus {
        match action {
            FindAction::Search { query, options } => {
                self.query.clone_from(query);
                self.options = *options;
                self.active = true;
                edit.found_current = None;
                self.search(edit);
                let start = edit.editor.selection().start;
                edit.found_current = edit
                    .found
                    .iter()
                    .position(|r| r.start >= start)
                    .or((!edit.found.is_empty()).then_some(0));
            }
            // A read-only text is searched but not changed: replacing only moves on.
            FindAction::Replace(_) | FindAction::ReplaceAll(_) if edit.read_only => {
                self.sync(edit);
                let index = edit.found_current.or_else(|| self.neighbor(edit, true));
                Self::select(edit, index);
            }
            FindAction::Next | FindAction::Previous => {
                self.sync(edit);
                let index = self.neighbor(edit, *action == FindAction::Next);
                Self::select(edit, index);
            }
            FindAction::Replace(replacement) => {
                self.sync(edit);
                let selected = edit.editor.selection();
                match edit.found.iter().position(|r| *r == selected) {
                    Some(_) if self.error.is_none() => {
                        let text = edit.editor.text().to_owned();
                        if let Ok(re) = compile(&self.query, self.options) {
                            let with = expand(&text, &selected, &re, self.options, replacement);
                            edit.editor.insert(&with);
                        }
                        self.search(edit);
                        // Go on from the caret, after what was put in: the old range
                        // may match again there, as a part of the new text.
                        edit.found_current = None;
                        let index = self.neighbor(edit, true);
                        Self::select(edit, index);
                    }
                    _ => {
                        let index = edit.found_current.or_else(|| self.neighbor(edit, true));
                        Self::select(edit, index);
                    }
                }
            }
            FindAction::ReplaceAll(replacement) => {
                let text = edit.editor.text().to_owned();
                let mut status = FindStatus::default();
                if let Ok((new, count)) = replace_all(&text, &self.query, self.options, replacement)
                    && count > 0
                {
                    edit.editor.select_all();
                    edit.editor.insert(&new);
                    status.replaced = Some(count);
                }
                edit.found_current = None;
                self.search(edit);
                return FindStatus {
                    replaced: status.replaced,
                    ..self.status(edit)
                };
            }
            FindAction::Close => {
                self.active = false;
                self.searched.clear();
                edit.found.clear();
                edit.found_current = None;
            }
        }
        self.status(edit)
    }
}

/// Returns whether `message` was a [`FindAction`] from `bar`. `target` is a
/// [`TextView`], [`TextArea`] or [`LogView`]. On [`FindAction::Close`] the bar is hidden
/// and the text gets the focus back.
pub fn drive_find(tree: &mut UiTree, bar: WidgetId, target: WidgetId, message: &Message) -> bool {
    if message.source != bar {
        return false;
    }
    let Some(action) = message.downcast_ref::<FindAction>() else {
        return false;
    };
    let status = tree
        .update::<TextView, _>(target, |view, cx| view.find(cx, action))
        .or_else(|| tree.update::<TextArea, _>(target, |area, cx| area.find(cx, action)))
        .or_else(|| tree.update::<LogView, _>(target, |log, cx| log.find(cx, action)))
        .unwrap_or_default();
    if *action == FindAction::Close {
        tree.set_hidden(bar, true);
        tree.set_focus(Some(target));
    } else {
        tree.update::<FindBar, _>(bar, |bar, cx| bar.set_status(cx, status));
    }
    true
}

const PAD: f32 = 6.0;
const ROW: f32 = 26.0;
const BUTTON: f32 = 26.0;
const GAP: f32 = 4.0;
const FIELD_LEFT: f32 = 24.0;
const FIELD_RIGHT: f32 = 8.0;
const DONE_W: f32 = 52.0;
const ACTION_W: f32 = 64.0;
const TEXT: f32 = 13.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Disclosure,
    Case,
    Word,
    Regex,
    Previous,
    Next,
    Done,
    Replace,
    ReplaceAll,
}

const PARTS: [Part; 9] = [
    Part::Disclosure,
    Part::Case,
    Part::Word,
    Part::Regex,
    Part::Previous,
    Part::Next,
    Part::Done,
    Part::Replace,
    Part::ReplaceAll,
];

/// A find and replace bar for a [`TextView`] or [`TextArea`] (`NSTextFinder`). Emits
/// [`FindAction`]s; pass them to [`drive_find`]. Show it by un-hiding it and giving it
/// the focus.
#[derive(Debug)]
pub struct FindBar {
    search: SingleLine,
    replace: SingleLine,
    options: FindOptions,
    replacing: bool,
    /// The replace field has the keyboard (else the search field).
    in_replace: bool,
    status: FindStatus,
    width: f32,
    pressed: Option<Part>,
    count: CachedText,
    labels: Vec<CachedText>,
    searched: Option<(String, FindOptions)>,
}

impl Default for FindBar {
    fn default() -> Self {
        Self::new()
    }
}

impl FindBar {
    pub fn new() -> Self {
        let field = |placeholder: &str| {
            let mut line = SingleLine::new(placeholder.into(), 200.0, FIELD_RIGHT);
            line.edit.style = TextStyle::new(TEXT);
            line
        };
        Self {
            search: field("Find"),
            replace: field("Replace"),
            options: FindOptions::default(),
            replacing: false,
            in_replace: false,
            status: FindStatus::default(),
            width: 480.0,
            pressed: None,
            count: CachedText::default(),
            labels: (0..6).map(|_| CachedText::default()).collect(),
            searched: None,
        }
    }

    pub fn replacing(mut self, replacing: bool) -> Self {
        self.replacing = replacing;
        self
    }

    pub fn options(mut self, options: FindOptions) -> Self {
        self.options = options;
        self
    }

    pub fn query(&self) -> &str {
        self.search.edit.editor.text()
    }

    pub fn replacement(&self) -> &str {
        self.replace.edit.editor.text()
    }

    pub fn find_options(&self) -> FindOptions {
        self.options
    }

    pub fn is_replacing(&self) -> bool {
        self.replacing
    }

    pub fn status(&self) -> &FindStatus {
        &self.status
    }

    /// [`drive_find`] calls this.
    pub fn set_status(&mut self, cx: &mut UpdateCx, status: FindStatus) {
        if status != self.status {
            self.status = status;
            cx.request_layout();
        }
    }

    pub fn set_query(&mut self, cx: &mut UpdateCx, query: impl Into<String>) {
        self.search.edit.editor.set_text(&query.into());
        self.search.edit.editor.select_all();
        cx.request_layout();
    }

    fn count_text(&self) -> String {
        if self.query().is_empty() {
            return String::new();
        }
        if self.status.error.is_some() {
            return "Invalid".into();
        }
        if let Some(n) = self.status.replaced {
            return format!("{n} replaced");
        }
        match (self.status.total, self.status.current) {
            (0, _) => "No results".into(),
            (n, Some(i)) => format!("{} of {n}", i + 1),
            (n, None) => format!("{n} found"),
        }
    }

    fn height(&self) -> f32 {
        let rows = if self.replacing { 2.0 } else { 1.0 };
        PAD * 2.0 + rows * ROW + (rows - 1.0) * GAP
    }

    /// Widget coordinates.
    fn rect(&self, part: Part) -> Rect {
        let w = self.width;
        let top = PAD;
        let second = PAD + ROW + GAP;
        let right_of = |n: f32| w - PAD - DONE_W - GAP - n * (BUTTON + GAP);
        match part {
            Part::Disclosure => Rect::new(PAD, top, 18.0, ROW),
            Part::Done => Rect::new(w - PAD - DONE_W, top, DONE_W, ROW),
            Part::Next => Rect::new(right_of(1.0), top, BUTTON, ROW),
            Part::Previous => Rect::new(right_of(2.0), top, BUTTON, ROW),
            Part::Regex => Rect::new(right_of(3.0) - GAP, top, BUTTON, ROW),
            Part::Word => Rect::new(right_of(4.0) - GAP, top, BUTTON, ROW),
            Part::Case => Rect::new(right_of(5.0) - GAP, top, BUTTON, ROW),
            Part::ReplaceAll => Rect::new(w - PAD - ACTION_W, second, ACTION_W, ROW),
            Part::Replace => Rect::new(w - PAD - 2.0 * ACTION_W - GAP, second, ACTION_W, ROW),
        }
    }

    fn search_field(&self) -> Rect {
        let left = PAD + 18.0 + GAP;
        let right = self.rect(Part::Case).x - GAP;
        Rect::new(left, PAD, (right - left).max(40.0), ROW)
    }

    fn replace_field(&self) -> Rect {
        let left = PAD + 18.0 + GAP;
        let right = self.rect(Part::Replace).x - GAP;
        Rect::new(left, PAD + ROW + GAP, (right - left).max(40.0), ROW)
    }

    fn part_at(&self, p: Point) -> Option<Part> {
        PARTS
            .into_iter()
            .filter(|part| self.replacing || !matches!(part, Part::Replace | Part::ReplaceAll))
            .find(|&part| self.rect(part).contains(p))
    }

    /// Lay a field's line out as if the field started at the bar's left edge.
    fn fit_line(line: &mut SingleLine, field: Rect, left: f32, right: f32) {
        line.width = field.right();
        line.pad_left = field.x + left;
        line.pad_right = right;
    }

    fn text_origin(line: &SingleLine, field: Rect) -> Point {
        let o = line.origin();
        Point::new(
            o.x,
            field.y + (field.height - line.edit.style.line_height) / 2.0,
        )
    }

    fn active(&mut self) -> (&mut SingleLine, Rect) {
        if self.in_replace && self.replacing {
            let field = self.replace_field();
            (&mut self.replace, field)
        } else {
            let field = self.search_field();
            (&mut self.search, field)
        }
    }

    fn emit_search(&mut self, cx: &mut EventCx) {
        let key = (self.query().to_owned(), self.options);
        if self.searched.as_ref() != Some(&key) {
            self.status.replaced = None;
            cx.emit(FindAction::Search {
                query: key.0.clone(),
                options: key.1,
            });
            self.searched = Some(key);
        }
    }

    /// Ask to close. Closing clears the matches in the text, so the query is searched
    /// again when the bar is used next.
    fn close(&mut self, cx: &mut EventCx) {
        self.searched = None;
        self.status = FindStatus::default();
        cx.emit(FindAction::Close);
    }

    fn press(&mut self, cx: &mut EventCx, part: Part) {
        match part {
            Part::Disclosure => {
                self.replacing = !self.replacing;
                self.in_replace = self.replacing;
            }
            Part::Case => self.options.case_sensitive = !self.options.case_sensitive,
            Part::Word => self.options.whole_word = !self.options.whole_word,
            Part::Regex => self.options.regex = !self.options.regex,
            Part::Previous => cx.emit(FindAction::Previous),
            Part::Next => cx.emit(FindAction::Next),
            Part::Done => self.close(cx),
            Part::Replace => cx.emit(FindAction::Replace(self.replacement().to_owned())),
            Part::ReplaceAll => cx.emit(FindAction::ReplaceAll(self.replacement().to_owned())),
        }
        if matches!(part, Part::Case | Part::Word | Part::Regex) {
            self.emit_search(cx);
        }
        cx.request_layout();
    }

    fn label(part: Part) -> &'static str {
        match part {
            Part::Case => "Aa",
            Part::Word => "W",
            Part::Regex => ".*",
            Part::Done => "Done",
            Part::Replace => "Replace",
            Part::ReplaceAll => "All",
            _ => "",
        }
    }

    fn toggled(&self, part: Part) -> bool {
        match part {
            Part::Case => self.options.case_sensitive,
            Part::Word => self.options.whole_word,
            Part::Regex => self.options.regex,
            _ => false,
        }
    }

    fn paint_field(&self, cx: &mut PaintCx, field: Rect, focused: bool) {
        let radius = cx.corner(Corner::Control, 5.0, field.height);
        let fill = cx.color(Role::Surface);
        let border = if focused {
            Border {
                width: 2.0,
                color: cx.color(Role::Accent),
            }
        } else {
            Border {
                width: 1.0,
                color: cx.color(Role::Outline),
            }
        };
        cx.bezel(field, radius, fill, border);
    }
}

impl Widget for FindBar {
    fn cursor(&self, local: Point, _size: Size) -> Option<Cursor> {
        let over = self.search_field().contains(local)
            || (self.replacing && self.replace_field().contains(local));
        over.then_some(Cursor::Text)
    }

    fn style(&self) -> Style {
        Style {
            size: LayoutSize {
                width: percent(1.0),
                height: Dimension::auto(),
            },
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn measures(&self) -> bool {
        true
    }

    fn measure(&mut self, cx: &mut MeasureCx, input: MeasureInput) -> Size {
        if let Some(width) = input.known.width.or(input.wrap_width()) {
            self.width = width.max(360.0);
        }
        let count = self.count_text();
        let count_w = if count.is_empty() {
            self.count.release(cx.text);
            0.0
        } else {
            self.count
                .shape(cx.text, &count, TextStyle::new(11.0), None)
                .width
                + 8.0
        };
        let search = self.search_field();
        let replace = self.replace_field();
        Self::fit_line(&mut self.search, search, FIELD_LEFT, FIELD_RIGHT + count_w);
        Self::fit_line(&mut self.replace, replace, FIELD_LEFT, FIELD_RIGHT);
        self.search.measure(cx.text);
        if self.replacing {
            self.replace.measure(cx.text);
        }
        let parts = [
            Part::Case,
            Part::Word,
            Part::Regex,
            Part::Done,
            Part::Replace,
            Part::ReplaceAll,
        ];
        for (cached, part) in self.labels.iter_mut().zip(parts) {
            let style = TextStyle::new(12.0).weight(if part == Part::Case { 600 } else { 500 });
            cached.shape(cx.text, Self::label(part), style, None);
        }
        Size::new(self.width, self.height())
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let fill = cx.color(Role::SurfaceAlt);
        cx.fill_rect(bounds, fill);
        let line = cx.color(Role::Separator);
        cx.fill_rect(Rect::new(0.0, bounds.height - 1.0, bounds.width, 1.0), line);
        let focused = cx.is_focused();
        let ink = cx.color(Role::Text);
        let muted = cx.color(Role::Muted);

        let d = self.rect(Part::Disclosure);
        let (r, dir) = if self.replacing {
            (
                Rect::new(d.x + 4.0, d.y + d.height / 2.0 - 2.5, 9.0, 5.0),
                Direction::Down,
            )
        } else {
            (
                Rect::new(d.x + 6.0, d.y + d.height / 2.0 - 4.5, 5.0, 9.0),
                Direction::Right,
            )
        };
        cx.chevron(r, dir, 1.5, muted);

        let search = self.search_field();
        let search_focused = focused && !(self.in_replace && self.replacing);
        self.paint_field(cx, search, search_focused);
        let lens = Rect::new(
            search.x + 7.0,
            search.y + search.height / 2.0 - 6.0,
            9.0,
            9.0,
        );
        cx.rounded_rect(
            lens,
            4.5,
            Color::TRANSPARENT,
            Border {
                width: 1.5,
                color: muted,
            },
        );
        cx.line(
            Point::new(lens.right() - 1.0, lens.bottom() - 1.0),
            Point::new(lens.right() + 2.5, lens.bottom() + 2.5),
            1.5,
            muted,
        );
        let origin = Self::text_origin(&self.search, search);
        cx.push_clip(Rect::new(
            search.x + FIELD_LEFT - 2.0,
            search.y,
            (self.search.width - self.search.pad_left - self.search.pad_right + 4.0).max(0.0),
            search.height,
        ));
        self.search.edit.paint(cx, origin, search_focused);
        cx.pop_clip();
        if let Some(layout) = self.count.layout().filter(|_| !self.query().is_empty()) {
            let size = self.count.size();
            let ink = if self.status.error.is_some() || self.status.total == 0 {
                super::Tone::Critical.ink(cx)
            } else {
                muted
            };
            let at = Point::new(
                search.right() - FIELD_RIGHT - size.width,
                search.y + (search.height - size.height) / 2.0,
            );
            cx.text(Rect::new(at.x, at.y, size.width, size.height), layout, ink);
        }

        if self.replacing {
            let replace = self.replace_field();
            let replace_focused = focused && self.in_replace;
            self.paint_field(cx, replace, replace_focused);
            let origin = Self::text_origin(&self.replace, replace);
            cx.push_clip(Rect::new(
                replace.x + FIELD_LEFT - 2.0,
                replace.y,
                (self.replace.width - self.replace.pad_left - self.replace.pad_right + 4.0)
                    .max(0.0),
                replace.height,
            ));
            self.replace.edit.paint(cx, origin, replace_focused);
            cx.pop_clip();
        }

        let parts = [
            Part::Case,
            Part::Word,
            Part::Regex,
            Part::Done,
            Part::Replace,
            Part::ReplaceAll,
        ];
        let surface = cx.color(Role::Surface);
        let outline = cx.color(Role::Outline);
        let accent = cx.color(Role::Accent);
        for part in [
            Part::Case,
            Part::Word,
            Part::Regex,
            Part::Previous,
            Part::Next,
            Part::Done,
        ]
        .into_iter()
        .chain(
            self.replacing
                .then_some([Part::Replace, Part::ReplaceAll])
                .into_iter()
                .flatten(),
        ) {
            let rect = self.rect(part);
            let radius = cx.corner(Corner::Control, 5.0, rect.height);
            let on = self.toggled(part);
            let fill = if on {
                accent.with_alpha(0.2)
            } else if self.pressed == Some(part) {
                super::pressed_tint(cx, surface)
            } else {
                surface
            };
            let border = Border {
                width: 1.0,
                color: if on { accent } else { outline },
            };
            cx.bezel(rect, radius, fill, border);
            let color = if on { accent } else { ink };
            match part {
                Part::Previous | Part::Next => {
                    let dir = if part == Part::Next {
                        Direction::Right
                    } else {
                        Direction::Left
                    };
                    let c = Rect::new(
                        rect.x + rect.width / 2.0 - 2.5,
                        rect.y + rect.height / 2.0 - 5.0,
                        5.0,
                        10.0,
                    );
                    cx.chevron(c, dir, 1.5, color);
                }
                _ => {
                    let index = parts.iter().position(|p| *p == part).expect("labelled");
                    let cached = &self.labels[index];
                    if let Some(layout) = cached.layout() {
                        let size = cached.size();
                        let at = Point::new(
                            rect.x + (rect.width - size.width) / 2.0,
                            rect.y + (rect.height - size.height) / 2.0,
                        );
                        cx.text(
                            Rect::new(at.x, at.y, size.width, size.height),
                            layout,
                            color,
                        );
                        if part == Part::Word {
                            let y = at.y + size.height - 2.0;
                            cx.fill_rect(Rect::new(at.x - 3.0, y, size.width + 6.0, 1.0), color);
                        }
                    }
                }
            }
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        let composing = !self.search.edit.composition().is_empty()
            || !self.replace.edit.composition().is_empty();
        match event {
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } if !composing => {
                let p = cx.local(*pos);
                cx.request_focus();
                if let Some(part) = self.part_at(p) {
                    self.pressed = Some(part);
                    cx.capture_pointer();
                    cx.request_paint();
                    cx.set_handled();
                    return;
                }
                if self.replacing && self.replace_field().contains(p) {
                    self.in_replace = true;
                } else if self.search_field().contains(p) {
                    self.in_replace = false;
                } else {
                    cx.set_handled();
                    return;
                }
                cx.request_paint();
            }
            // Shown again after closing: search the query again.
            Event::FocusChanged(true) => {
                if self.searched.is_none() && !self.query().is_empty() {
                    self.emit_search(cx);
                }
                return;
            }
            Event::PointerUp { pos, .. } if self.pressed.is_some() => {
                let part = self.pressed.take().expect("checked");
                cx.release_pointer();
                if self.rect(part).contains(cx.local(*pos)) {
                    self.press(cx, part);
                }
                cx.request_paint();
                cx.set_handled();
                return;
            }
            Event::KeyDown { key, modifiers } if !composing => {
                let handled = match key {
                    Key::Escape => {
                        self.close(cx);
                        true
                    }
                    Key::Enter if self.in_replace && self.replacing => {
                        cx.emit(FindAction::Replace(self.replacement().to_owned()));
                        true
                    }
                    Key::Enter => {
                        self.emit_search(cx);
                        cx.emit(if modifiers.shift {
                            FindAction::Previous
                        } else {
                            FindAction::Next
                        });
                        true
                    }
                    Key::Char('g') if modifiers.command => {
                        cx.emit(if modifiers.shift {
                            FindAction::Previous
                        } else {
                            FindAction::Next
                        });
                        true
                    }
                    Key::Tab if self.replacing && self.in_replace == modifiers.shift => {
                        self.in_replace = !modifiers.shift;
                        cx.request_paint();
                        true
                    }
                    _ => false,
                };
                if handled {
                    cx.set_handled();
                    return;
                }
            }
            _ => {}
        }
        let in_replace = self.in_replace && self.replacing;
        let (line, field) = self.active();
        let origin = Self::text_origin(line, field);
        line.edit.event(cx, event, origin);
        if !in_replace && matches!(event, Event::Text(_) | Event::KeyDown { .. }) {
            self.emit_search(cx);
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn text_input_area(&self) -> Option<Rect> {
        let (line, field) = if self.in_replace && self.replacing {
            (&self.replace, self.replace_field())
        } else {
            (&self.search, self.search_field())
        };
        Some(line.edit.input_area(Self::text_origin(line, field)))
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::SearchInput)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        node.set_label("Find");
        self.search.edit.accessibility(node);
        let count = self.count_text();
        if !count.is_empty() {
            node.set_description(count);
        }
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.search.edit.removed(text);
        self.replace.edit.removed(text);
        self.count.release(text);
        for cached in &mut self.labels {
            cached.release(text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(case_sensitive: bool, whole_word: bool, regex: bool) -> FindOptions {
        FindOptions {
            case_sensitive,
            whole_word,
            regex,
        }
    }

    #[test]
    fn plain_queries_ignore_case_unless_asked() {
        let text = "Cat cat CAT concat";
        assert_eq!(
            find_matches(text, "cat", FindOptions::default()).unwrap(),
            [0..3, 4..7, 8..11, 15..18]
        );
        assert_eq!(
            find_matches(text, "cat", opts(true, false, false)).unwrap(),
            [4..7, 15..18]
        );
        // Regex syntax is literal text in a plain query.
        let found = find_matches("a.b axb", "a.b", FindOptions::default()).unwrap();
        assert_eq!((found.len(), found[0].clone()), (1, 0..3));
        assert!(
            find_matches(text, "", FindOptions::default())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn whole_words_need_boundaries_on_both_sides() {
        let text = "cat concat cat_x cats (cat)";
        assert_eq!(
            find_matches(text, "cat", opts(false, true, false)).unwrap(),
            [0..3, 23..26]
        );
        let text = "猫 と 猫たち";
        let found = find_matches(text, "猫", opts(false, true, false)).unwrap();
        assert_eq!((found.len(), found[0].clone()), (1, 0..3));
        // A whole word that starts inside a match that is not one is still found.
        let found = find_matches("ba a a", "a a", opts(false, true, false)).unwrap();
        assert_eq!((found.len(), found[0].clone()), (1, 3..6));
    }

    #[test]
    fn a_long_run_that_is_no_whole_word_is_searched_in_linear_time() {
        // Every `a+` here is followed by a letter: none is a whole word. Looking again
        // from each character of the run would take 50 000 scans of it.
        let text = format!("{}b", "a".repeat(50_000));
        let start = std::time::Instant::now();
        let found = find_matches(&text, "a+", opts(false, true, true)).unwrap();
        assert!(found.is_empty());
        assert!(start.elapsed().as_secs() < 5, "took {:?}", start.elapsed());
    }

    #[test]
    fn regular_expressions_match_and_report_errors() {
        let text = "id=12, id=345";
        assert_eq!(
            find_matches(text, r"\d+", opts(false, false, true)).unwrap(),
            [3..5, 10..13]
        );
        assert_eq!(
            find_matches("abc", "x*", opts(false, false, true)).unwrap(),
            Vec::<Range<usize>>::new()
        );
        assert!(find_matches(text, "(", opts(false, false, true)).is_err());
    }

    #[test]
    fn replacing_all_expands_groups_in_regex_mode() {
        let (text, n) =
            replace_all("a-1 b-2", r"(\w)-(\d)", opts(false, false, true), "$2:$1").unwrap();
        assert_eq!((text.as_str(), n), ("1:a 2:b", 2));
        let (text, n) = replace_all("$1 $1", "$1", FindOptions::default(), "x").unwrap();
        assert_eq!((text.as_str(), n), ("x x", 2));
    }
}

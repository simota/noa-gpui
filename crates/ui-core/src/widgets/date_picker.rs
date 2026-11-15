use std::f32::consts::TAU;

use noa_gpui_layout::{MeasureInput, Style};
use noa_gpui_scene::{Border, Direction, Point, Rect, Size};
use noa_gpui_text::{TextStore, TextStyle};

use super::{CachedText, ControlSize, focus_ring, pressed_tint};
use crate::date::{DateTime, days_in_month};
use crate::event::{Event, Key, PointerButton};
use crate::overlay::Placement;
use crate::theme::{Corner, Role};
use crate::widget::{EventCx, MeasureCx, PaintCx, UpdateCx, Widget};

/// Emitted by a [`DatePicker`] when its value changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateChanged(pub DateTime);

/// Emitted with [`DatePicker::range_selection`]. `start <= end`; both keep the picker's
/// time of day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateRangeChanged {
    pub start: DateTime,
    pub end: DateTime,
}

/// Year, month and day, for comparing dates whatever their time.
fn day_key(d: DateTime) -> (i32, u32, u32) {
    (d.year, d.month, d.day)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatePickerStyle {
    /// Editable fields such as `11/12/2026, 14:05`.
    TextField,
    TextFieldAndStepper,
    /// A month calendar and / or an analog clock.
    Graphical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateElements {
    Date,
    Time,
    DateAndTime,
}

impl DateElements {
    fn date(self) -> bool {
        self != DateElements::Time
    }

    fn time(self) -> bool {
        self != DateElements::Date
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DateOrder {
    /// 11/13/2026 (the United States).
    #[default]
    MonthDayYear,
    /// 13/11/2026 or 13.11.2026 (most of Europe, Latin America, India).
    DayMonthYear,
    /// 2026/11/13 or 2026-11-13 (Japan, China, Korea, ISO 8601).
    YearMonthDay,
}

/// Defaults to month/day/year with a 24-hour clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateFormat {
    pub order: DateOrder,
    /// Between the date's fields: `"/"`, `"."` or `"-"`.
    pub separator: &'static str,
    /// Show hours 1–12 with an AM / PM field.
    pub twelve_hour: bool,
}

impl Default for DateFormat {
    fn default() -> Self {
        Self {
            order: DateOrder::MonthDayYear,
            separator: "/",
            twelve_hour: false,
        }
    }
}

impl DateFormat {
    /// ISO 8601: 2026-11-13, 24-hour.
    pub const ISO: Self = Self {
        order: DateOrder::YearMonthDay,
        separator: "-",
        twelve_hour: false,
    };

    /// Takes a BCP 47 locale such as `"ja-JP"` (also `"en_US.UTF-8"`). Unknown languages
    /// get day/month/year with a 24-hour clock.
    pub fn for_locale(locale: &str) -> Self {
        let mut parts = locale.split(['-', '_', '.', '@']);
        let language = parts.next().unwrap_or("").to_ascii_lowercase();
        // The region is the first two-letter (or three-digit) subtag after the language;
        // a script subtag such as "Hant" comes before it.
        let region = parts
            .find(|p| p.len() == 2 || (p.len() == 3 && p.bytes().all(|b| b.is_ascii_digit())))
            .unwrap_or("")
            .to_ascii_uppercase();
        let format = |order, separator, twelve_hour| Self {
            order,
            separator,
            twelve_hour,
        };
        use DateOrder::*;
        match (language.as_str(), region.as_str()) {
            ("en", "US" | "PH" | "") => format(MonthDayYear, "/", true),
            ("en", "CA") => format(YearMonthDay, "-", true),
            ("en", "AU" | "NZ" | "IN") => format(DayMonthYear, "/", true),
            ("en", _) => format(DayMonthYear, "/", false),
            ("ja" | "zh", _) => format(YearMonthDay, "/", false),
            ("ko", _) => format(YearMonthDay, ".", true),
            ("hu" | "lt", _) => format(YearMonthDay, ".", false),
            ("sv" | "fr", "SE" | "CA") | ("sv", _) => format(YearMonthDay, "-", false),
            ("hi" | "bn", _) => format(DayMonthYear, "/", true),
            ("ar", _) => format(DayMonthYear, "/", true),
            (
                "de" | "ru" | "pl" | "cs" | "sk" | "fi" | "nb" | "nn" | "no" | "da" | "uk" | "tr"
                | "ro" | "hr" | "sl" | "sr" | "bg" | "et" | "lv" | "is",
                _,
            ) => format(DayMonthYear, ".", false),
            ("nl", _) => format(DayMonthYear, "-", false),
            _ => format(DayMonthYear, "/", false),
        }
    }

    pub fn order(self, order: DateOrder) -> Self {
        Self { order, ..self }
    }

    pub fn separator(self, separator: &'static str) -> Self {
        Self { separator, ..self }
    }

    pub fn twelve_hour(self, twelve_hour: bool) -> Self {
        Self {
            twelve_hour,
            ..self
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Segment {
    Month,
    Day,
    Year,
    Hour,
    Minute,
    Period,
}

impl Segment {
    fn digits(self) -> usize {
        if self == Segment::Year { 4 } else { 2 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Piece {
    Field(Segment),
    Separator(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Calendar,
    Clock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hand {
    Hour,
    Minute,
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WEEKDAYS: [&str; 7] = ["S", "M", "T", "W", "T", "F", "S"];

const FIELD_PAD: f32 = 6.0;
const FIELD_H: f32 = 24.0;
const STEPPER_W: f32 = 16.0;
const STEPPER_GAP: f32 = 4.0;
const PAD: f32 = 8.0;
const CELL_W: f32 = 28.0;
const CELL_H: f32 = 24.0;
const HEADER: f32 = 28.0;
const WEEK_ROW: f32 = 20.0;
const CLOCK: f32 = 150.0;
const PART_GAP: f32 = 16.0;

/// A date and / or time picker (`NSDatePicker`). The value stays inside
/// [`DatePicker::range`]. Emits [`DateChanged`], and [`DateRangeChanged`] with
/// [`DatePicker::range_selection`].
#[derive(Debug)]
pub struct DatePicker {
    value: DateTime,
    min: Option<DateTime>,
    max: Option<DateTime>,
    style: DatePickerStyle,
    elements: DateElements,
    format: DateFormat,
    calendar_popover: bool,
    for_owner: bool,
    size: ControlSize,
    pieces: Vec<Piece>,
    piece_texts: Vec<CachedText>,
    /// Left edge of each piece and the right edge of the last.
    edges: Vec<f32>,
    /// Index into `pieces` of the selected field.
    selected: usize,
    typed: String,
    stepper_pressed: Option<bool>,
    popover_open: bool,
    shown: DateTime,
    part: Part,
    dragging: Option<Hand>,
    header: CachedText,
    weekdays: Vec<CachedText>,
    days: Vec<CachedText>,
    meridiem: CachedText,
    /// The selected days of a range selection, in order; `None` for a single date.
    days_range: Option<(DateTime, DateTime)>,
    /// Where the range was started: the end that stays while extending.
    anchor: DateTime,
    range_dragging: bool,
}

impl DatePicker {
    pub fn new(value: DateTime) -> Self {
        let mut picker = Self {
            value,
            min: None,
            max: None,
            style: DatePickerStyle::TextField,
            elements: DateElements::Date,
            format: DateFormat::default(),
            calendar_popover: false,
            for_owner: false,
            size: ControlSize::Regular,
            pieces: Vec::new(),
            piece_texts: Vec::new(),
            edges: Vec::new(),
            selected: 0,
            typed: String::new(),
            stepper_pressed: None,
            popover_open: false,
            shown: value.first_of_month(),
            part: Part::Calendar,
            dragging: None,
            header: CachedText::default(),
            weekdays: (0..7).map(|_| CachedText::default()).collect(),
            days: (0..31).map(|_| CachedText::default()).collect(),
            meridiem: CachedText::default(),
            days_range: None,
            anchor: value,
            range_dragging: false,
        };
        picker.rebuild_pieces();
        picker
    }

    pub fn style(mut self, style: DatePickerStyle) -> Self {
        self.style = style;
        self
    }

    pub fn elements(mut self, elements: DateElements) -> Self {
        self.elements = elements;
        self.part = if elements.date() {
            Part::Calendar
        } else {
            Part::Clock
        };
        self.rebuild_pieces();
        self
    }

    /// Month/day/year with a 24-hour clock by default.
    pub fn format(mut self, format: DateFormat) -> Self {
        self.format = format;
        self.rebuild_pieces();
        self
    }

    /// Keep the value within `min..=max` (either may be `None`).
    pub fn range(mut self, min: Option<DateTime>, max: Option<DateTime>) -> Self {
        self.min = min;
        self.max = max;
        self.value = self.clamp(self.value);
        self.shown = self.value.first_of_month();
        self.anchor = self.clamp(self.anchor);
        self.days_range = self
            .days_range
            .map(|(start, end)| (self.clamp(start), self.clamp(end)));
        self
    }

    /// Graphical style only. The range starts as the value's day alone.
    pub fn range_selection(mut self, enabled: bool) -> Self {
        self.days_range = enabled.then_some((self.value, self.value));
        self.anchor = self.value;
        self
    }

    /// The selected days of a range selection (`start <= end`).
    pub fn date_range(&self) -> Option<(DateTime, DateTime)> {
        self.days_range
    }

    /// Select the days from `start` to `end` (ordered and clamped to the range) without
    /// emitting.
    pub fn set_date_range(&mut self, cx: &mut UpdateCx, start: DateTime, end: DateTime) {
        if self.days_range.is_none() {
            return;
        }
        let (start, end) = (self.clamp(start.min(end)), self.clamp(start.max(end)));
        self.days_range = Some((start, end));
        self.anchor = start;
        self.value = end;
        self.shown = end.first_of_month();
        cx.request_layout();
    }

    fn select_days(&mut self, cx: &mut EventCx, to: DateTime) {
        let to = self.clamp(to);
        let (a, b) = (self.anchor, to);
        let range = if day_key(a) <= day_key(b) {
            (a, b)
        } else {
            (b, a)
        };
        self.value = to;
        if self.days_range != Some(range) {
            self.days_range = Some(range);
            cx.emit(DateRangeChanged {
                start: range.0,
                end: range.1,
            });
        }
        cx.request_paint();
    }

    /// Returns whether the event was used.
    fn range_event(&mut self, cx: &mut EventCx, event: &Event) -> bool {
        let Some(calendar) = self.calendar_rect() else {
            return false;
        };
        let day_date = |picker: &Self, day: u32| DateTime {
            year: picker.shown.year,
            month: picker.shown.month,
            day,
            ..picker.value
        };
        match event {
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                let p = cx.local(*pos);
                let Some(day) = self.day_at(calendar, p) else {
                    return false;
                };
                let date = day_date(self, day);
                cx.set_handled();
                if !self.day_in_range(date) {
                    return true;
                }
                self.part = Part::Calendar;
                if !cx.modifiers().shift {
                    self.anchor = self.clamp(date);
                }
                self.select_days(cx, date);
                self.range_dragging = true;
                cx.capture_pointer();
                true
            }
            Event::PointerMove { pos } if self.range_dragging => {
                if let Some(day) = self.day_at(calendar, cx.local(*pos)) {
                    let date = day_date(self, day);
                    if self.day_in_range(date) {
                        self.select_days(cx, date);
                    }
                }
                cx.set_handled();
                true
            }
            Event::PointerUp { .. } if self.range_dragging => {
                self.range_dragging = false;
                cx.release_pointer();
                cx.set_handled();
                true
            }
            Event::KeyDown { key, modifiers } if self.part == Part::Calendar => {
                let to = match key {
                    Key::ArrowLeft => self.value.add_days(-1),
                    Key::ArrowRight => self.value.add_days(1),
                    Key::ArrowUp => self.value.add_days(-7),
                    Key::ArrowDown => self.value.add_days(7),
                    Key::PageUp => self.value.add_months(-1),
                    Key::PageDown => self.value.add_months(1),
                    _ => return false,
                };
                let to = self.clamp(to);
                if !modifiers.shift {
                    self.anchor = to;
                }
                if (to.year, to.month) != (self.shown.year, self.shown.month) {
                    self.shown = to.first_of_month();
                    cx.request_layout();
                }
                self.select_days(cx, to);
                cx.set_handled();
                true
            }
            _ => false,
        }
    }

    /// Text styles only.
    pub fn calendar_popover(mut self, enabled: bool) -> Self {
        self.calendar_popover = enabled;
        self
    }

    pub fn control_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    pub fn value(&self) -> DateTime {
        self.value
    }

    /// Set the value (clamped to the range) without emitting.
    pub fn set_value(&mut self, cx: &mut UpdateCx, value: DateTime) {
        self.value = self.clamp(value);
        self.shown = self.value.first_of_month();
        cx.request_layout();
    }

    pub fn formatted(&self) -> String {
        self.pieces
            .iter()
            .map(|piece| match piece {
                Piece::Field(segment) => self.segment_text(*segment),
                Piece::Separator(s) => (*s).to_owned(),
            })
            .collect()
    }

    /// Its first day.
    pub fn shown_month(&self) -> DateTime {
        self.shown
    }

    fn rebuild_pieces(&mut self) {
        let mut pieces = Vec::new();
        if self.elements.date() {
            let order = match self.format.order {
                DateOrder::MonthDayYear => [Segment::Month, Segment::Day, Segment::Year],
                DateOrder::DayMonthYear => [Segment::Day, Segment::Month, Segment::Year],
                DateOrder::YearMonthDay => [Segment::Year, Segment::Month, Segment::Day],
            };
            let separator = self.format.separator;
            pieces.extend([
                Piece::Field(order[0]),
                Piece::Separator(separator),
                Piece::Field(order[1]),
                Piece::Separator(separator),
                Piece::Field(order[2]),
            ]);
        }
        if self.elements == DateElements::DateAndTime {
            pieces.push(Piece::Separator(", "));
        }
        if self.elements.time() {
            pieces.extend([
                Piece::Field(Segment::Hour),
                Piece::Separator(":"),
                Piece::Field(Segment::Minute),
            ]);
            if self.format.twelve_hour {
                pieces.extend([Piece::Separator(" "), Piece::Field(Segment::Period)]);
            }
        }
        self.piece_texts = (0..pieces.len()).map(|_| CachedText::default()).collect();
        self.pieces = pieces;
        self.selected = 0;
    }

    fn clamp(&self, value: DateTime) -> DateTime {
        let value = self.min.map_or(value, |min| value.max(min));
        self.max.map_or(value, |max| value.min(max))
    }

    /// Whether the day of `value` has a time in the range: the first and the last day
    /// can be picked whatever the time of day of the value is (it is clamped).
    fn day_in_range(&self, value: DateTime) -> bool {
        let day = day_key(value);
        self.min.is_none_or(|min| day >= day_key(min))
            && self.max.is_none_or(|max| day <= day_key(max))
    }

    fn apply(&mut self, cx: &mut EventCx, value: DateTime) {
        let value = self.clamp(value);
        if value == self.value {
            return;
        }
        let month_changed = (value.year, value.month) != (self.value.year, self.value.month);
        self.value = value;
        if month_changed {
            self.shown = value.first_of_month();
        }
        if self.for_owner {
            cx.notify_owner(DateChanged(value));
        } else {
            cx.emit(DateChanged(value));
        }
        cx.request_layout();
        cx.request_paint();
    }

    fn segment(&self) -> Option<Segment> {
        match self.pieces.get(self.selected) {
            Some(Piece::Field(segment)) => Some(*segment),
            _ => None,
        }
    }

    fn segment_text(&self, segment: Segment) -> String {
        let v = &self.value;
        match segment {
            Segment::Month => format!("{:02}", v.month),
            Segment::Day => format!("{:02}", v.day),
            Segment::Year => format!("{:04}", v.year),
            Segment::Hour if self.format.twelve_hour => format!("{}", (v.hour + 11) % 12 + 1),
            Segment::Hour => format!("{:02}", v.hour),
            Segment::Minute => format!("{:02}", v.minute),
            Segment::Period if v.hour < 12 => "AM".to_owned(),
            Segment::Period => "PM".to_owned(),
        }
    }

    /// Wraps within the segment's range; the year does not wrap.
    fn stepped(&self, segment: Segment, delta: i32) -> DateTime {
        let wrap = |value: u32, low: u32, count: u32| {
            ((value as i32 - low as i32 + delta).rem_euclid(count as i32)) as u32 + low
        };
        let mut v = self.value;
        match segment {
            Segment::Month => v.month = wrap(v.month, 1, 12),
            Segment::Day => v.day = wrap(v.day, 1, days_in_month(v.year, v.month)),
            Segment::Year => v.year += delta,
            Segment::Hour => v.hour = wrap(v.hour, 0, 24),
            Segment::Minute => v.minute = wrap(v.minute, 0, 60),
            Segment::Period => v.hour = (v.hour + 12) % 24,
        }
        v.day = v.day.min(days_in_month(v.year, v.month));
        v
    }

    fn step_selected(&mut self, cx: &mut EventCx, delta: i32) {
        if let Some(segment) = self.segment() {
            self.typed.clear();
            let value = self.stepped(segment, delta);
            self.apply(cx, value);
        }
    }

    fn move_selection(&mut self, forward: bool) {
        let fields: Vec<usize> = (0..self.pieces.len())
            .filter(|&i| matches!(self.pieces[i], Piece::Field(_)))
            .collect();
        let at = fields.iter().position(|&i| i == self.selected).unwrap_or(0);
        let next = if forward {
            (at + 1).min(fields.len() - 1)
        } else {
            at.saturating_sub(1)
        };
        self.selected = fields[next];
        self.typed.clear();
    }

    fn type_digit(&mut self, cx: &mut EventCx, digit: char) {
        let Some(segment) = self.segment().filter(|&s| s != Segment::Period) else {
            return;
        };
        let twelve = self.format.twelve_hour;
        self.typed.push(digit);
        let max = match segment {
            Segment::Month => 12,
            Segment::Day => days_in_month(self.value.year, self.value.month),
            Segment::Year => 9999,
            Segment::Hour if twelve => 12,
            Segment::Hour => 23,
            Segment::Minute | Segment::Period => 59,
        };
        let mut n: u32 = self.typed.parse().unwrap_or(0);
        if n > max {
            self.typed = digit.to_string();
            n = digit.to_digit(10).unwrap_or(0);
        }
        let complete = self.typed.len() >= segment.digits() || n * 10 > max;
        // A partial month or day ("0") or year ("20") is not applied yet.
        let valid = match segment {
            Segment::Month | Segment::Day => n >= 1,
            Segment::Hour if twelve => n >= 1,
            Segment::Year => complete,
            Segment::Hour | Segment::Minute | Segment::Period => true,
        };
        if valid {
            let mut v = self.value;
            match segment {
                Segment::Month => v.month = n,
                Segment::Day => v.day = n,
                Segment::Year => v.year = n as i32,
                // Typing an hour on a 12-hour clock keeps AM or PM.
                Segment::Hour if twelve => v.hour = n % 12 + if v.hour >= 12 { 12 } else { 0 },
                Segment::Hour => v.hour = n,
                Segment::Minute => v.minute = n,
                Segment::Period => {}
            }
            v.day = v.day.min(days_in_month(v.year, v.month));
            self.apply(cx, v);
        }
        if complete {
            let last = self.selected;
            self.move_selection(true);
            if self.selected == last {
                self.typed.clear();
            }
        }
        cx.request_paint();
    }

    /// "a" or "p" in the AM / PM field.
    fn type_period(&mut self, cx: &mut EventCx, c: char) {
        if self.segment() != Some(Segment::Period) {
            return;
        }
        let pm = match c.to_ascii_lowercase() {
            'a' => false,
            'p' => true,
            _ => return,
        };
        if pm != (self.value.hour >= 12) {
            let value = self.stepped(Segment::Period, 1);
            self.apply(cx, value);
        }
        cx.request_paint();
    }

    fn field_width(&self) -> f32 {
        let k = self.size.scale();
        self.edges.last().copied().unwrap_or(0.0) + FIELD_PAD * k
    }

    fn has_stepper(&self) -> bool {
        self.style == DatePickerStyle::TextFieldAndStepper
    }

    fn open_calendar(&mut self, cx: &mut EventCx) {
        if self.popover_open || !self.elements.date() {
            return;
        }
        self.popover_open = true;
        let mut calendar = DatePicker::new(self.value)
            .style(DatePickerStyle::Graphical)
            .elements(DateElements::Date)
            .range(self.min, self.max);
        calendar.for_owner = true;
        let anchor = cx.window_rect();
        cx.open_popover(calendar, Placement::Below(anchor));
    }

    fn calendar_rect(&self) -> Option<Rect> {
        self.elements
            .date()
            .then(|| Rect::new(PAD, PAD, 7.0 * CELL_W, HEADER + WEEK_ROW + 6.0 * CELL_H))
    }

    fn clock_rect(&self) -> Option<Rect> {
        self.elements.time().then(|| {
            let x = self.calendar_rect().map_or(PAD, |c| c.right() + PART_GAP);
            Rect::new(x, PAD, CLOCK, CLOCK)
        })
    }

    fn month_buttons(&self, calendar: Rect) -> (Rect, Rect) {
        let y = calendar.y + 4.0;
        (
            Rect::new(calendar.right() - 48.0, y, 20.0, 20.0),
            Rect::new(calendar.right() - 24.0, y, 20.0, 20.0),
        )
    }

    fn day_cell(&self, calendar: Rect, day: u32) -> Rect {
        let index = self.shown.weekday() + day - 1;
        let (row, col) = (index / 7, index % 7);
        Rect::new(
            calendar.x + col as f32 * CELL_W,
            calendar.y + HEADER + WEEK_ROW + row as f32 * CELL_H,
            CELL_W,
            CELL_H,
        )
    }

    fn day_at(&self, calendar: Rect, p: Point) -> Option<u32> {
        (1..=days_in_month(self.shown.year, self.shown.month))
            .find(|&d| self.day_cell(calendar, d).contains(p))
    }

    fn drag_hand(&mut self, cx: &mut EventCx, hand: Hand, p: Point) {
        let Some(clock) = self.clock_rect() else {
            return;
        };
        let (dx, dy) = (
            p.x - (clock.x + clock.width / 2.0),
            p.y - (clock.y + clock.height / 2.0),
        );
        let turn = dx.atan2(-dy).rem_euclid(TAU) / TAU;
        let mut v = self.value;
        match hand {
            Hand::Minute => v.minute = (turn * 60.0).round() as u32 % 60,
            Hand::Hour => {
                let pm = v.hour >= 12;
                v.hour = (turn * 12.0).round() as u32 % 12 + if pm { 12 } else { 0 };
            }
        }
        self.apply(cx, v);
    }

    fn graphical_event(&mut self, cx: &mut EventCx, event: &Event) {
        if self.days_range.is_some() && self.range_event(cx, event) {
            return;
        }
        match event {
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                let p = cx.local(*pos);
                cx.set_handled();
                if let Some(calendar) = self.calendar_rect()
                    && calendar.contains(p)
                {
                    self.part = Part::Calendar;
                    let (prev, next) = self.month_buttons(calendar);
                    if prev.contains(p) || next.contains(p) {
                        self.shown = self.shown.add_months(if prev.contains(p) { -1 } else { 1 });
                        cx.request_layout();
                    } else if let Some(day) = self.day_at(calendar, p) {
                        let v = DateTime {
                            year: self.shown.year,
                            month: self.shown.month,
                            day,
                            ..self.value
                        };
                        if self.day_in_range(v) {
                            self.apply(cx, v);
                            if self.for_owner {
                                cx.close_popover();
                            }
                        }
                    }
                    cx.request_paint();
                    return;
                }
                if let Some(clock) = self.clock_rect()
                    && clock.contains(p)
                {
                    self.part = Part::Clock;
                    let center =
                        Point::new(clock.x + clock.width / 2.0, clock.y + clock.height / 2.0);
                    let distance = ((p.x - center.x).powi(2) + (p.y - center.y).powi(2)).sqrt();
                    let hand = if distance < clock.width * 0.28 {
                        Hand::Hour
                    } else {
                        Hand::Minute
                    };
                    self.dragging = Some(hand);
                    cx.capture_pointer();
                    self.drag_hand(cx, hand, p);
                    cx.request_paint();
                }
            }
            Event::PointerMove { pos } => {
                if let Some(hand) = self.dragging {
                    let p = cx.local(*pos);
                    self.drag_hand(cx, hand, p);
                    cx.set_handled();
                }
            }
            Event::PointerUp { .. } => {
                if self.dragging.take().is_some() {
                    cx.release_pointer();
                    cx.set_handled();
                }
            }
            Event::KeyDown { key, modifiers } => {
                let both = self.elements == DateElements::DateAndTime;
                match (key, self.part) {
                    (Key::Tab, Part::Calendar) if both && !modifiers.shift => {
                        self.part = Part::Clock;
                    }
                    (Key::Tab, Part::Clock) if both && modifiers.shift => {
                        self.part = Part::Calendar;
                    }
                    (Key::Enter | Key::Space, _) if self.for_owner => cx.close_popover(),
                    (Key::ArrowLeft, Part::Calendar) => self.apply(cx, self.value.add_days(-1)),
                    (Key::ArrowRight, Part::Calendar) => self.apply(cx, self.value.add_days(1)),
                    (Key::ArrowUp, Part::Calendar) => self.apply(cx, self.value.add_days(-7)),
                    (Key::ArrowDown, Part::Calendar) => self.apply(cx, self.value.add_days(7)),
                    (Key::PageUp, Part::Calendar) => self.apply(cx, self.value.add_months(-1)),
                    (Key::PageDown, Part::Calendar) => self.apply(cx, self.value.add_months(1)),
                    (Key::ArrowUp | Key::ArrowDown, Part::Clock) => {
                        let up = *key == Key::ArrowUp;
                        let value = self.stepped(Segment::Minute, if up { 1 } else { -1 });
                        self.apply(cx, value);
                    }
                    (Key::PageUp | Key::PageDown, Part::Clock) => {
                        let up = *key == Key::PageUp;
                        let value = self.stepped(Segment::Hour, if up { 1 } else { -1 });
                        self.apply(cx, value);
                    }
                    _ => return,
                }
                cx.request_paint();
                cx.set_handled();
            }
            Event::Increment | Event::Decrement => {
                let delta = if *event == Event::Increment { 1 } else { -1 };
                let value = match self.part {
                    Part::Calendar => self.value.add_days(delta),
                    Part::Clock => self.stepped(Segment::Minute, delta as i32),
                };
                self.apply(cx, value);
            }
            _ => {}
        }
    }

    fn text_event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                let p = cx.local(*pos);
                cx.set_handled();
                let k = self.size.scale();
                if self.has_stepper() && p.x >= self.field_width() + STEPPER_GAP * k {
                    let up = p.y < cx.size().height / 2.0;
                    self.stepper_pressed = Some(up);
                    cx.capture_pointer();
                    self.step_selected(cx, if up { 1 } else { -1 });
                    cx.request_paint();
                    return;
                }
                let nearest = (0..self.pieces.len())
                    .filter(|&i| matches!(self.pieces[i], Piece::Field(_)))
                    .min_by(|&a, &b| {
                        let distance = |i: usize| {
                            let (l, r) = (self.edges[i], self.edges[i + 1]);
                            if p.x < l {
                                l - p.x
                            } else if p.x > r {
                                p.x - r
                            } else {
                                0.0
                            }
                        };
                        distance(a).total_cmp(&distance(b))
                    });
                if let Some(i) = nearest {
                    self.selected = i;
                    self.typed.clear();
                }
                if self.calendar_popover
                    && matches!(
                        self.segment(),
                        Some(Segment::Month | Segment::Day | Segment::Year)
                    )
                {
                    self.open_calendar(cx);
                }
                cx.request_paint();
            }
            Event::PointerUp { .. } => {
                if self.stepper_pressed.take().is_some() {
                    cx.release_pointer();
                    cx.request_paint();
                }
            }
            Event::KeyDown { key, .. } => {
                match key {
                    Key::ArrowLeft | Key::ArrowRight => {
                        self.move_selection(*key == Key::ArrowRight)
                    }
                    Key::ArrowUp => self.step_selected(cx, 1),
                    Key::ArrowDown => self.step_selected(cx, -1),
                    _ => return,
                }
                cx.request_paint();
                cx.set_handled();
            }
            Event::Text(text) => {
                for c in text.chars() {
                    if c.is_ascii_digit() {
                        self.type_digit(cx, c);
                    } else {
                        self.type_period(cx, c);
                    }
                }
                cx.set_handled();
            }
            Event::Increment => self.step_selected(cx, 1),
            Event::Decrement => self.step_selected(cx, -1),
            Event::FromPopover(message) => {
                if let Some(&DateChanged(date)) = message.downcast_ref() {
                    // The calendar edits only the date.
                    let value = DateTime {
                        hour: self.value.hour,
                        minute: self.value.minute,
                        ..date
                    };
                    self.apply(cx, value);
                }
            }
            Event::PopoverClosed => {
                self.popover_open = false;
                cx.request_paint();
            }
            _ => {}
        }
    }

    fn paint_text(&mut self, cx: &mut PaintCx) {
        let k = self.size.scale();
        let height = cx.size().height;
        let field = Rect::new(0.0, 0.0, self.field_width(), height);
        let surface = cx.color(Role::Surface);
        let outline = cx.color(Role::Outline);
        let radius = cx.corner(Corner::Control, 5.0 * k, height);
        cx.bezel(
            field,
            radius,
            surface,
            Border {
                width: 1.0,
                color: outline,
            },
        );
        let focused = cx.is_focused() || self.popover_open;
        for (i, piece) in self.pieces.iter().enumerate() {
            let (left, right) = (self.edges[i], self.edges[i + 1]);
            let selected = focused && i == self.selected;
            if selected {
                let accent = cx.color(Role::Accent);
                cx.rounded_rect(
                    Rect::new(left - 1.0, 3.0 * k, right - left + 2.0, height - 6.0 * k),
                    cx.corner(Corner::Inner, 3.0, height - 6.0 * k),
                    accent,
                    Border::default(),
                );
            }
            if let Some(layout) = self.piece_texts[i].layout() {
                let size = self.piece_texts[i].size();
                let role = match piece {
                    _ if selected => Role::OnAccent,
                    Piece::Separator(_) => Role::Muted,
                    Piece::Field(_) => Role::Text,
                };
                let ink = cx.color(role);
                let y = (height - size.height) / 2.0;
                cx.text(Rect::new(left, y, size.width, size.height), layout, ink);
            }
        }
        if cx.is_focused() {
            focus_ring(cx, field, radius);
        }
        if self.has_stepper() {
            let x = field.right() + STEPPER_GAP * k;
            let stepper = Rect::new(x, 0.0, STEPPER_W * k, height);
            let stepper_radius = cx.corner(Corner::Inner, 5.0 * k, stepper.width);
            cx.bezel(
                stepper,
                stepper_radius,
                surface,
                Border {
                    width: 1.0,
                    color: outline,
                },
            );
            let half = height / 2.0;
            if let Some(up) = self.stepper_pressed {
                let tint = pressed_tint(cx, surface);
                let y = if up { 1.0 } else { half };
                cx.rounded_rect(
                    Rect::new(x + 1.0, y, stepper.width - 2.0, half - 1.0),
                    4.0 * k,
                    tint,
                    Border::default(),
                );
            }
            cx.fill_rect(
                Rect::new(x + 3.0, half - 0.5, stepper.width - 6.0, 1.0),
                outline,
            );
            let ink = cx.color(Role::Text);
            let (w, h) = (7.0 * k, 3.5 * k);
            let cx_ = x + (stepper.width - w) / 2.0;
            cx.chevron(
                Rect::new(cx_, (half - h) / 2.0, w, h),
                Direction::Up,
                1.5,
                ink,
            );
            cx.chevron(
                Rect::new(cx_, half + (half - h) / 2.0, w, h),
                Direction::Down,
                1.5,
                ink,
            );
        }
    }

    fn paint_graphical(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let surface = cx.color(Role::Surface);
        let outline = cx.color(Role::Outline);
        cx.rounded_rect(
            bounds,
            8.0,
            surface,
            Border {
                width: 1.0,
                color: outline,
            },
        );
        let text_color = cx.color(Role::Text);
        let muted = cx.color(Role::Muted);
        let focused = cx.is_focused();
        let draw_centered = |cx: &mut PaintCx, cached: &CachedText, rect: Rect, ink| {
            if let Some(layout) = cached.layout() {
                let size = cached.size();
                let x = rect.x + (rect.width - size.width) / 2.0;
                let y = rect.y + (rect.height - size.height) / 2.0;
                cx.text(Rect::new(x, y, size.width, size.height), layout, ink);
            }
        };
        if let Some(calendar) = self.calendar_rect() {
            if let Some(layout) = self.header.layout() {
                let size = self.header.size();
                let y = calendar.y + (HEADER - size.height) / 2.0;
                cx.text(
                    Rect::new(calendar.x + 6.0, y, size.width, size.height),
                    layout,
                    text_color,
                );
            }
            let (prev, next) = self.month_buttons(calendar);
            let chevron = |r: Rect| Rect::new(r.x + 7.0, r.y + 5.0, 5.0, 10.0);
            cx.chevron(chevron(prev), Direction::Left, 1.5, text_color);
            cx.chevron(chevron(next), Direction::Right, 1.5, text_color);
            for (i, cached) in self.weekdays.iter().enumerate() {
                let rect = Rect::new(
                    calendar.x + i as f32 * CELL_W,
                    calendar.y + HEADER,
                    CELL_W,
                    WEEK_ROW,
                );
                draw_centered(cx, cached, rect, muted);
            }
            let accent = cx.color(Role::Accent);
            let on_accent = cx.color(Role::OnAccent);
            let band = accent.with_alpha(accent.a * 0.18);
            for day in 1..=days_in_month(self.shown.year, self.shown.month) {
                let rect = self.day_cell(calendar, day);
                let date = DateTime {
                    year: self.shown.year,
                    month: self.shown.month,
                    day,
                    ..self.value
                };
                let selected = match self.days_range {
                    Some((start, end)) => {
                        let key = day_key(date);
                        let (first, last) = (day_key(start), day_key(end));
                        if first < key && key < last || (key == first) != (key == last) {
                            // The band joining the ends: half a cell at each end.
                            let (l, r) = (
                                if key == first {
                                    rect.x + rect.width / 2.0
                                } else {
                                    rect.x
                                },
                                if key == last {
                                    rect.x + rect.width / 2.0
                                } else {
                                    rect.right()
                                },
                            );
                            let h = CELL_H - 2.0;
                            cx.fill_rect(Rect::new(l, rect.y + 1.0, r - l, h), band);
                        }
                        key == first || key == last
                    }
                    None => date == self.value,
                };
                let ink = if selected {
                    let d = CELL_H - 2.0;
                    let circle = Rect::new(rect.x + (rect.width - d) / 2.0, rect.y + 1.0, d, d);
                    cx.rounded_rect(circle, d / 2.0, accent, Border::default());
                    if focused && self.part == Part::Calendar {
                        focus_ring(cx, circle, d / 2.0);
                    }
                    on_accent
                } else if self.day_in_range(date) {
                    text_color
                } else {
                    muted.with_alpha(muted.a * 0.5)
                };
                draw_centered(cx, &self.days[day as usize - 1], rect, ink);
            }
        }
        if let Some(clock) = self.clock_rect() {
            let r = clock.width / 2.0;
            let c = Point::new(clock.x + r, clock.y + r);
            let face = cx.color(Role::SurfaceAlt);
            cx.rounded_rect(
                clock,
                r,
                face,
                Border {
                    width: 1.0,
                    color: outline,
                },
            );
            let at = |turn: f32, length: f32| {
                let a = turn * TAU;
                Point::new(c.x + length * a.sin(), c.y - length * a.cos())
            };
            for i in 0..12 {
                let t = i as f32 / 12.0;
                let inner = if i % 3 == 0 { r - 14.0 } else { r - 9.0 };
                cx.line(at(t, r - 5.0), at(t, inner), 1.5, muted);
            }
            let v = self.value;
            if let Some(layout) = self.meridiem.layout() {
                let size = self.meridiem.size();
                let rect = Rect::new(
                    c.x - size.width / 2.0,
                    c.y + r * 0.35,
                    size.width,
                    size.height,
                );
                cx.text(rect, layout, muted);
            }
            let minute_turn = v.minute as f32 / 60.0;
            let hour_turn = ((v.hour % 12) as f32 + minute_turn) / 12.0;
            cx.line(c, at(hour_turn, r * 0.5), 4.0, text_color);
            let accent = cx.color(Role::Accent);
            cx.line(c, at(minute_turn, r * 0.78), 2.5, accent);
            cx.rounded_rect(
                Rect::new(c.x - 4.0, c.y - 4.0, 8.0, 8.0),
                4.0,
                text_color,
                Border::default(),
            );
            if focused && self.part == Part::Clock {
                focus_ring(cx, clock, r);
            }
        }
    }
}

impl Widget for DatePicker {
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
        if self.style == DatePickerStyle::Graphical {
            let style = TextStyle::new(13.0);
            let title = format!(
                "{} {}",
                MONTHS[self.shown.month as usize - 1],
                self.shown.year
            );
            self.header
                .shape(cx.text, &title, TextStyle::new(14.0), None);
            for (cached, name) in self.weekdays.iter_mut().zip(WEEKDAYS) {
                cached.shape(cx.text, name, TextStyle::new(11.0), None);
            }
            for (i, cached) in self.days.iter_mut().enumerate() {
                cached.shape(cx.text, &(i + 1).to_string(), style, None);
            }
            let meridiem = if self.value.hour < 12 { "AM" } else { "PM" };
            self.meridiem
                .shape(cx.text, meridiem, TextStyle::new(11.0), None);
            let right = self
                .clock_rect()
                .or(self.calendar_rect())
                .map_or(0.0, |r| r.right());
            let bottom = self
                .calendar_rect()
                .into_iter()
                .chain(self.clock_rect())
                .map(|r| r.bottom())
                .fold(0.0, f32::max);
            return Size::new(right + PAD, bottom + PAD);
        }
        let k = self.size.scale();
        let style = TextStyle::new(self.size.font(14.0));
        let mut x = FIELD_PAD * k;
        self.edges.clear();
        for i in 0..self.pieces.len() {
            self.edges.push(x);
            let text = match self.pieces[i] {
                Piece::Field(segment) => self.segment_text(segment),
                Piece::Separator(s) => s.to_owned(),
            };
            x += self.piece_texts[i].shape(cx.text, &text, style, None).width;
        }
        self.edges.push(x);
        let mut width = self.field_width();
        if self.has_stepper() {
            width += (STEPPER_GAP + STEPPER_W) * k;
        }
        Size::new(width, FIELD_H * k)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        if self.style == DatePickerStyle::Graphical {
            self.paint_graphical(cx);
        } else {
            self.paint_text(cx);
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        if self.style == DatePickerStyle::Graphical {
            self.graphical_event(cx, event);
        } else {
            self.text_event(cx, event);
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(match self.elements {
            DateElements::Date => accesskit::Role::DateInput,
            DateElements::Time => accesskit::Role::TimeInput,
            DateElements::DateAndTime => accesskit::Role::DateTimeInput,
        })
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        let v = self.value;
        let date = format!("{:04}-{:02}-{:02}", v.year, v.month, v.day);
        let time = format!("{:02}:{:02}", v.hour, v.minute);
        node.set_value(match self.elements {
            DateElements::Date => date,
            DateElements::Time => time,
            DateElements::DateAndTime => format!("{date} {time}"),
        });
        node.add_action(accesskit::Action::Increment);
        node.add_action(accesskit::Action::Decrement);
    }

    fn removed(&mut self, text: &mut TextStore) {
        let caches = self
            .piece_texts
            .iter_mut()
            .chain(self.weekdays.iter_mut())
            .chain(self.days.iter_mut())
            .chain([&mut self.header, &mut self.meridiem]);
        for cached in caches {
            cached.release(text);
        }
    }
}

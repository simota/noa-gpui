use ui_layout::Style;
use ui_layout::style::{Dimension, Size as LayoutSize, length, percent};
use ui_scene::{Border, Color, Point, Rect, Size, TextLayoutId};
use ui_text::{TextStore, TextStyle};

use super::focus_ring;
use super::rows::TextCache;
use crate::event::{Event, Key, PointerButton};
use crate::theme::Role;
use crate::widget::{EventCx, PaintCx, UpdateCx, Widget};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChartKind {
    #[default]
    Line,
    Bar,
}

/// Colors given to series that do not name their own, after the accent color.
const PALETTE: [Color; 6] = [
    Color::hex(0x34c759),
    Color::hex(0xff9500),
    Color::hex(0xaf52de),
    Color::hex(0xff3b30),
    Color::hex(0x00c7be),
    Color::hex(0xffcc00),
];

const FONT: f32 = 11.0;
const AXIS_GAP: f32 = 6.0;
const LABEL_ROW: f32 = 18.0;
const LEGEND_SWATCH: f32 = 8.0;
const LEGEND_GAP: f32 = 14.0;
const LINE_WIDTH: f32 = 2.0;
/// As a share of a category's width.
const BAR_GAP: f32 = 0.25;
const READOUT_PAD: f32 = 6.0;

/// A value of `f64::NAN` is missing: lines break there and no bar is drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartSeries {
    pub name: String,
    pub values: Vec<f64>,
    /// `None` takes the next color of the chart's palette.
    pub color: Option<Color>,
}

impl ChartSeries {
    pub fn new(name: impl Into<String>, values: Vec<f64>) -> Self {
        Self {
            name: name.into(),
            values,
            color: None,
        }
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

/// Emitted when a category of a [`Chart`] is clicked, or chosen with the arrow keys
/// and Return: its index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChartPointActivated(pub usize);

/// Emitted when a series of a [`Chart`] is shown or hidden from its legend (a click on
/// its entry, or its number key): the series' index and whether it is now shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChartSeriesToggled {
    pub index: usize,
    pub visible: bool,
}

/// Rounded steps for a value axis from `min` to `max` with about `count` lines: the
/// first and last lines (at or beyond `min` and `max`) and the step between them.
pub fn nice_ticks(min: f64, max: f64, count: usize) -> (f64, f64, f64) {
    let (min, max) = if max > min {
        (min, max)
    } else if min == 0.0 {
        (0.0, 1.0)
    } else {
        let pad = min.abs() * 0.5;
        (min - pad, max + pad)
    };
    let raw = (max - min) / count.max(1) as f64;
    let magnitude = 10f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 2.5, 5.0, 10.0]
        .iter()
        .map(|m| m * magnitude)
        .find(|&s| s >= raw)
        .unwrap_or(10.0 * magnitude);
    let lo = (min / step).floor() * step;
    let hi = (max / step).ceil() * step;
    (lo, hi, step)
}

/// The default text of a value: whole numbers without decimals, others with as few
/// as tell the steps of the axis apart; thousands and millions shortened (12.5k, 3M).
fn format_value(value: f64, step: f64) -> String {
    let abs = value.abs();
    if abs >= 1e6 && step >= 1e5 {
        return trim(format!("{:.1}", value / 1e6)) + "M";
    }
    if abs >= 1e4 && step >= 1e3 {
        return trim(format!("{:.1}", value / 1e3)) + "k";
    }
    let decimals = if step > 0.0 && step.is_finite() {
        (0..=6usize)
            .find(|&d| {
                let scaled = step * 10f64.powi(d as i32);
                (scaled - scaled.round()).abs() < 1e-6 * scaled.max(1.0)
            })
            .unwrap_or(6)
    } else {
        0
    };
    trim(format!("{value:.decimals$}"))
}

fn trim(text: String) -> String {
    if text.contains('.') {
        let t = text.trim_end_matches('0').trim_end_matches('.');
        if t == "-0" { "0".into() } else { t.to_owned() }
    } else if text == "-0" {
        "0".into()
    } else {
        text
    }
}

type Formatter = Box<dyn Fn(f64) -> String>;

/// Values drawn as lines or bars against a value axis. Emits [`ChartPointActivated`] and
/// [`ChartSeriesToggled`]. A hidden series is not drawn, read out or counted in the value
/// axis.
pub struct Chart {
    kind: ChartKind,
    labels: Vec<String>,
    series: Vec<ChartSeries>,
    range: Option<(f64, f64)>,
    format: Option<Formatter>,
    capacity: Option<usize>,
    sparkline: bool,
    width: Dimension,
    height: f32,
    inspected: Option<usize>,
    texts: TextCache<(u8, usize)>,
    /// From the last paint.
    plot: Rect,
    hidden: Vec<bool>,
    /// From the last paint, in widget coordinates.
    legend: Vec<Rect>,
    stacked: bool,
}

impl std::fmt::Debug for Chart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Chart")
            .field("kind", &self.kind)
            .field("categories", &self.labels.len())
            .field("series", &self.series.len())
            .finish_non_exhaustive()
    }
}

impl Chart {
    pub fn new(kind: ChartKind, labels: Vec<String>) -> Self {
        Self {
            kind,
            labels,
            series: Vec::new(),
            range: None,
            format: None,
            capacity: None,
            sparkline: false,
            width: percent(1.0),
            height: 200.0,
            inspected: None,
            texts: TextCache::default(),
            plot: Rect::new(0.0, 0.0, 0.0, 0.0),
            hidden: Vec::new(),
            legend: Vec::new(),
            stacked: false,
        }
    }

    pub fn line(labels: Vec<String>) -> Self {
        Self::new(ChartKind::Line, labels)
    }

    pub fn bar(labels: Vec<String>) -> Self {
        Self::new(ChartKind::Bar, labels)
    }

    /// Values past the last category are not drawn.
    pub fn series(mut self, series: ChartSeries) -> Self {
        self.series.push(series);
        self
    }

    /// Values outside are cut off at the edge.
    pub fn range(mut self, min: f64, max: f64) -> Self {
        self.range = (max > min).then_some((min, max));
        self
    }

    pub fn format(mut self, format: impl Fn(f64) -> String + 'static) -> Self {
        self.format = Some(Box::new(format));
        self
    }

    /// Keep at most `count` categories: [`Chart::push`] drops the oldest beyond it.
    pub fn capacity(mut self, count: usize) -> Self {
        self.capacity = Some(count.max(1));
        self
    }

    /// Bar charts only. Positive values stack up from zero, negative ones down; hidden
    /// series leave the stack.
    pub fn stacked(mut self, stacked: bool) -> Self {
        self.stacked = stacked;
        self
    }

    pub fn is_stacked(&self) -> bool {
        self.stacked && self.kind == ChartKind::Bar
    }

    /// Missing values are left out.
    pub fn total(&self, index: usize) -> f64 {
        self.shown()
            .filter_map(|(_, s)| s.values.get(index).copied())
            .filter(|v| v.is_finite())
            .sum()
    }

    /// Only the lines or bars: no axes, labels, legend or readout.
    pub fn sparkline(mut self) -> Self {
        self.sparkline = true;
        self
    }

    /// Defaults to the parent's width.
    pub fn width(mut self, width: f32) -> Self {
        self.width = length(width);
        self
    }

    /// Defaults to 200.
    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    pub fn kind(&self) -> ChartKind {
        self.kind
    }

    pub fn labels(&self) -> &[String] {
        &self.labels
    }

    pub fn series_list(&self) -> &[ChartSeries] {
        &self.series
    }

    pub fn is_series_visible(&self, index: usize) -> bool {
        index < self.series.len() && !self.hidden.get(index).copied().unwrap_or(false)
    }

    /// Emits nothing.
    pub fn set_series_visible(&mut self, cx: &mut UpdateCx, index: usize, visible: bool) {
        if self.toggle_to(index, visible) {
            cx.request_paint();
        }
    }

    /// Returns whether it changed.
    fn toggle_to(&mut self, index: usize, visible: bool) -> bool {
        if index >= self.series.len() || self.is_series_visible(index) == visible {
            return false;
        }
        if self.hidden.len() <= index {
            self.hidden.resize(index + 1, false);
        }
        self.hidden[index] = !visible;
        true
    }

    fn toggle(&mut self, cx: &mut EventCx, index: usize) {
        let visible = !self.is_series_visible(index);
        if self.toggle_to(index, visible) {
            cx.emit(ChartSeriesToggled { index, visible });
            cx.request_paint();
        }
    }

    fn shown(&self) -> impl Iterator<Item = (usize, &ChartSeries)> {
        (self.series.iter().enumerate()).filter(|&(i, _)| self.is_series_visible(i))
    }

    pub fn inspected(&self) -> Option<usize> {
        self.inspected
    }

    /// The readout for the inspected category: its label, then each shown series' name
    /// and value as text (missing values are "—"), and for [`Chart::stacked`] bars a
    /// last line, "Total", with their sum.
    pub fn readout(&self) -> Option<(String, Vec<(String, String)>)> {
        let i = self.inspected?;
        let step = self.ticks().2;
        let mut values: Vec<(String, String)> = self
            .shown()
            .map(|(_, s)| {
                let value = s.values.get(i).copied().unwrap_or(f64::NAN);
                (s.name.clone(), self.text_of(value, step))
            })
            .collect();
        if self.is_stacked() && !values.is_empty() {
            values.push(("Total".into(), self.text_of(self.total(i), step)));
        }
        Some((self.labels.get(i).cloned().unwrap_or_default(), values))
    }

    /// The value axis's first and last lines and the step between them.
    pub fn ticks(&self) -> (f64, f64, f64) {
        if let Some((min, max)) = self.range {
            let (_, _, step) = nice_ticks(min, max, 4);
            return (min, max, step);
        }
        let (mut min, mut max) = if self.is_stacked() {
            (0..self.labels.len()).fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), i| {
                let (down, up) = self.stack_ends(i);
                (a.min(down), b.max(up))
            })
        } else {
            let values = self
                .shown()
                .flat_map(|(_, s)| s.values.iter().take(self.labels.len()))
                .copied()
                .filter(|v| v.is_finite());
            values.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| {
                (a.min(v), b.max(v))
            })
        };
        if min > max {
            (min, max) = (0.0, 1.0);
        }
        if self.kind == ChartKind::Bar {
            min = min.min(0.0);
            max = max.max(0.0);
        }
        nice_ticks(min, max, 4)
    }

    /// How far category `i`'s stack reaches below and above zero: the sums of its
    /// shown negative and positive values.
    fn stack_ends(&self, i: usize) -> (f64, f64) {
        self.shown()
            .filter_map(|(_, s)| s.values.get(i).copied())
            .filter(|v| v.is_finite())
            .fold((0.0, 0.0), |(down, up), v| {
                if v < 0.0 {
                    (down + v, up)
                } else {
                    (down, up + v)
                }
            })
    }

    pub fn set_data(&mut self, cx: &mut UpdateCx, labels: Vec<String>, series: Vec<ChartSeries>) {
        self.labels = labels;
        self.series = series;
        // Series hidden from the legend stay hidden while there are as many.
        self.hidden.truncate(self.series.len());
        self.inspected = self.inspected.filter(|&i| i < self.labels.len());
        cx.request_paint();
    }

    /// Ignored if there is no such series.
    pub fn set_values(&mut self, cx: &mut UpdateCx, index: usize, values: Vec<f64>) {
        if let Some(series) = self.series.get_mut(index) {
            series.values = values;
            cx.request_paint();
        }
    }

    /// Missing values are `NAN`. Drops the oldest beyond [`Chart::capacity`].
    pub fn push(&mut self, cx: &mut UpdateCx, label: impl Into<String>, values: &[f64]) {
        let n = self.labels.len();
        for (i, series) in self.series.iter_mut().enumerate() {
            series.values.resize(n, f64::NAN);
            series
                .values
                .push(values.get(i).copied().unwrap_or(f64::NAN));
        }
        self.labels.push(label.into());
        if let Some(cap) = self.capacity
            && self.labels.len() > cap
        {
            let extra = self.labels.len() - cap;
            self.labels.drain(..extra);
            for series in &mut self.series {
                series.values.drain(..extra.min(series.values.len()));
            }
            self.inspected = self.inspected.and_then(|i| i.checked_sub(extra));
        }
        cx.request_paint();
    }

    fn text_of(&self, value: f64, step: f64) -> String {
        if !value.is_finite() {
            return "—".into();
        }
        match &self.format {
            Some(format) => format(value),
            None => format_value(value, step),
        }
    }

    fn color_of(&self, cx: &PaintCx, index: usize) -> Color {
        match self.series[index].color {
            Some(color) => cx.color(color),
            None if index == 0 => cx.color(Role::Accent),
            None => cx.color(PALETTE[(index - 1) % PALETTE.len()]),
        }
    }

    fn has_legend(&self) -> bool {
        !self.sparkline && self.series.len() > 1
    }

    fn x_of(&self, plot: Rect, i: usize) -> f32 {
        let n = self.labels.len();
        match self.kind {
            ChartKind::Line if n > 1 => plot.x + plot.width * i as f32 / (n - 1) as f32,
            ChartKind::Line => plot.x + plot.width / 2.0,
            ChartKind::Bar => plot.x + plot.width * (i as f32 + 0.5) / n.max(1) as f32,
        }
    }

    /// `x` is in widget coordinates.
    fn category_at(&self, x: f32) -> Option<usize> {
        let n = self.labels.len();
        if n == 0 || self.plot.width <= 0.0 {
            return None;
        }
        let t = ((x - self.plot.x) / self.plot.width).clamp(0.0, 1.0);
        let i = match self.kind {
            ChartKind::Line if n > 1 => (t * (n - 1) as f32).round() as usize,
            ChartKind::Line => 0,
            ChartKind::Bar => (t * n as f32).floor() as usize,
        };
        Some(i.min(n - 1))
    }

    fn inspect(&mut self, cx: &mut EventCx, index: Option<usize>) {
        if self.inspected != index {
            self.inspected = index;
            cx.request_paint();
        }
    }

    /// The y of `value`, held to the plot (for bars cut off at the edge).
    fn y_of(plot: Rect, value: f64, lo: f64, hi: f64) -> f32 {
        Self::y_free(plot, value.clamp(lo, hi), lo, hi)
    }

    /// The y of `value`, above or below the plot when it is out of range.
    fn y_free(plot: Rect, value: f64, lo: f64, hi: f64) -> f32 {
        let t = ((value - lo) / (hi - lo)) as f32;
        plot.bottom() - plot.height * t
    }

    /// The part of the segment from `a` to `b` between the plot's top and bottom, so
    /// that a line through values outside a fixed range keeps its slope.
    fn clip_segment(plot: Rect, a: Point, b: Point) -> Option<(Point, Point)> {
        let (top, bottom) = (plot.y, plot.bottom());
        let (mut t0, mut t1) = (0.0f32, 1.0f32);
        let dy = b.y - a.y;
        if dy.abs() < f32::EPSILON {
            return (a.y >= top && a.y <= bottom).then_some((a, b));
        }
        for edge in [top, bottom] {
            let t = (edge - a.y) / dy;
            // Entering through this edge raises t0; leaving lowers t1.
            let entering = if edge == top { dy > 0.0 } else { dy < 0.0 };
            if entering {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
        let at = |t: f32| Point::new(a.x + (b.x - a.x) * t, a.y + dy * t);
        (t0 <= t1).then(|| (at(t0), at(t1)))
    }

    fn shape(
        &mut self,
        text: &mut TextStore,
        kind: u8,
        index: usize,
        content: &str,
    ) -> (Option<TextLayoutId>, Size) {
        self.texts
            .get(text, (kind, index), content, TextStyle::new(FONT))
    }

    fn paint_series(&self, cx: &mut PaintCx, plot: Rect, lo: f64, hi: f64) {
        let n = self.labels.len();
        if n == 0 {
            return;
        }
        match self.kind {
            ChartKind::Line => {
                for (s, series) in self.shown() {
                    let color = self.color_of(cx, s);
                    let points: Vec<Option<Point>> = (0..n)
                        .map(|i| {
                            let v = series.values.get(i).copied().unwrap_or(f64::NAN);
                            v.is_finite().then(|| {
                                Point::new(self.x_of(plot, i), Self::y_free(plot, v, lo, hi))
                            })
                        })
                        .collect();
                    for pair in points.windows(2) {
                        if let [Some(a), Some(b)] = pair
                            && let Some((a, b)) = Self::clip_segment(plot, *a, *b)
                        {
                            cx.line(a, b, LINE_WIDTH, color);
                        }
                    }
                    // A value with no neighbor would not show: draw it as a dot.
                    for (i, p) in points.iter().enumerate() {
                        let alone = (i == 0 || points[i - 1].is_none())
                            && points.get(i + 1).is_none_or(Option::is_none);
                        if let Some(p) = p
                            && p.y >= plot.y
                            && p.y <= plot.bottom()
                            && (alone || self.inspected == Some(i))
                        {
                            let r = if alone { 2.0 } else { 4.0 };
                            let dot = Rect::new(p.x - r, p.y - r, 2.0 * r, 2.0 * r);
                            let ring = Border {
                                width: if alone { 0.0 } else { 2.0 },
                                color: cx.color(Role::Background),
                            };
                            cx.rounded_rect(dot, r, color, ring);
                        }
                    }
                }
            }
            ChartKind::Bar if self.stacked => {
                let band = plot.width / n as f32;
                let bar = band * (1.0 - BAR_GAP);
                for i in 0..n {
                    let left = plot.x + band * i as f32 + band * BAR_GAP / 2.0;
                    let (mut down, mut up) = (0.0, 0.0);
                    for (s, series) in self.shown() {
                        let v = series.values.get(i).copied().unwrap_or(f64::NAN);
                        if !v.is_finite() || v == 0.0 {
                            continue;
                        }
                        let (from, to) = if v < 0.0 {
                            down += v;
                            (down - v, down)
                        } else {
                            up += v;
                            (up - v, up)
                        };
                        let (a, b) = (Self::y_of(plot, from, lo, hi), Self::y_of(plot, to, lo, hi));
                        let mut color = self.color_of(cx, s);
                        if self.inspected.is_some_and(|k| k != i) {
                            color = color.with_alpha(color.a * 0.55);
                        }
                        // A hairline between the parts keeps neighbors apart.
                        let rect = Rect::new(left, a.min(b), (bar - 1.0).max(1.0), (a - b).abs());
                        let top =
                            Rect::new(rect.x, rect.y, rect.width, (rect.height - 1.0).max(0.5));
                        cx.fill_rect(top, color);
                    }
                }
            }
            ChartKind::Bar => {
                let band = plot.width / n as f32;
                let count = self.shown().count().max(1) as f32;
                let bar = band * (1.0 - BAR_GAP) / count;
                let zero = Self::y_of(plot, 0.0, lo, hi);
                for i in 0..n {
                    let left = plot.x + band * i as f32 + band * BAR_GAP / 2.0;
                    for (slot, (s, series)) in self.shown().enumerate() {
                        let v = series.values.get(i).copied().unwrap_or(f64::NAN);
                        if !v.is_finite() {
                            continue;
                        }
                        let y = Self::y_of(plot, v, lo, hi);
                        let (top, height) = if y < zero {
                            (y, zero - y)
                        } else {
                            (zero, y - zero)
                        };
                        let mut color = self.color_of(cx, s);
                        if self.inspected.is_some_and(|k| k != i) {
                            color = color.with_alpha(color.a * 0.55);
                        }
                        let rect =
                            Rect::new(left + bar * slot as f32, top, (bar - 1.0).max(1.0), height);
                        cx.rounded_rect(
                            rect,
                            (rect.width / 2.0).min(2.0),
                            color,
                            Border::default(),
                        );
                    }
                }
            }
        }
    }

    fn paint_readout(&mut self, cx: &mut PaintCx, plot: Rect, x: f32) {
        let Some((label, values)) = self.readout().filter(|(_, v)| !v.is_empty()) else {
            return;
        };
        let mut lines = Vec::with_capacity(values.len() + 1);
        lines.push(self.shape(cx.text_store(), 3, 0, &label));
        for (s, (name, value)) in values.iter().enumerate() {
            let text = if name.is_empty() {
                value.clone()
            } else {
                format!("{name}: {value}")
            };
            lines.push(self.shape(cx.text_store(), 4, s, &text));
        }
        let line_h = lines.iter().map(|(_, s)| s.height).fold(0.0, f32::max);
        let swatch = LEGEND_SWATCH + 4.0;
        let width = lines
            .iter()
            .enumerate()
            .map(|(i, (_, s))| s.width + if i > 0 { swatch } else { 0.0 })
            .fold(0.0, f32::max)
            + 2.0 * READOUT_PAD;
        let height = line_h * lines.len() as f32 + 2.0 * READOUT_PAD;
        let mut left = x + 10.0;
        if left + width > plot.right() {
            left = x - 10.0 - width;
        }
        let left = left.max(0.0);
        let rect = Rect::new(left, plot.y, width, height);
        let fill = cx.color(Role::Surface);
        let border = Border {
            width: 1.0,
            color: cx.color(Role::Separator),
        };
        cx.floating_surface(rect, 6.0, fill, border);
        let text = cx.color(Role::Text);
        let muted = cx.color(Role::Muted);
        let shown: Vec<usize> = self.shown().map(|(s, _)| s).collect();
        for (i, (layout, size)) in lines.into_iter().enumerate() {
            let y = rect.y + READOUT_PAD + line_h * i as f32;
            let mut tx = rect.x + READOUT_PAD;
            if i > 0 {
                if let Some(&s) = shown.get(i - 1) {
                    let color = self.color_of(cx, s);
                    let sy = y + (line_h - LEGEND_SWATCH) / 2.0;
                    let dot = Rect::new(tx, sy, LEGEND_SWATCH, LEGEND_SWATCH);
                    cx.rounded_rect(dot, LEGEND_SWATCH / 2.0, color, Border::default());
                }
                tx += swatch;
            }
            if let Some(layout) = layout {
                let color = if i == 0 { muted } else { text };
                cx.text(Rect::new(tx, y, size.width, size.height), layout, color);
            }
        }
    }
}

impl Widget for Chart {
    fn style(&self) -> Style {
        Style {
            size: LayoutSize {
                width: self.width,
                height: length(self.height),
            },
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        self.texts.begin();
        let (lo, hi, step) = self.ticks();
        let muted = cx.color(Role::Muted);
        let grid = cx.color(Role::Separator);

        let plot = if self.sparkline {
            // Keep round line caps inside.
            Rect::new(
                bounds.x + LINE_WIDTH,
                bounds.y + LINE_WIDTH,
                (bounds.width - 2.0 * LINE_WIDTH).max(1.0),
                (bounds.height - 2.0 * LINE_WIDTH).max(1.0),
            )
        } else {
            let mut top = 0.0;
            self.legend.clear();
            if self.has_legend() {
                let (mut x, mut y) = (0.0, 0.0);
                for s in 0..self.series.len() {
                    let name = self.series[s].name.clone();
                    let (layout, size) = self.shape(cx.text_store(), 2, s, &name);
                    let width = LEGEND_SWATCH + 4.0 + size.width;
                    if x > 0.0 && x + width > bounds.width {
                        x = 0.0;
                        y += LABEL_ROW;
                    }
                    let color = self.color_of(cx, s);
                    let visible = self.is_series_visible(s);
                    let sy = y + (LABEL_ROW - LEGEND_SWATCH) / 2.0;
                    let swatch = Rect::new(x, sy, LEGEND_SWATCH, LEGEND_SWATCH);
                    if visible {
                        cx.rounded_rect(swatch, 2.0, color, Border::default());
                    } else {
                        let ring = Border {
                            width: 1.5,
                            color: color.with_alpha(color.a * 0.6),
                        };
                        cx.rounded_rect(swatch, 2.0, Color::TRANSPARENT, ring);
                    }
                    if let Some(layout) = layout {
                        let ty = y + (LABEL_ROW - size.height) / 2.0;
                        let r = Rect::new(x + LEGEND_SWATCH + 4.0, ty, size.width, size.height);
                        cx.text(r, layout, muted);
                        if !visible {
                            // Struck through, not dimmed: the name stays as legible
                            // as the shown ones' (muted text is at the 4.5:1 floor).
                            let y = (r.y + r.height / 2.0).round();
                            cx.fill_rect(Rect::new(r.x, y - 0.5, r.width, 1.0), muted);
                        }
                    }
                    self.legend.push(Rect::new(x, y, width, LABEL_ROW));
                    x += width + LEGEND_GAP;
                }
                top = y + LABEL_ROW + 4.0;
            }

            // Value axis labels: the widest sets the plot's left edge.
            let count = ((hi - lo) / step).round().max(1.0) as usize;
            let ticks: Vec<f64> = (0..=count).map(|k| lo + step * k as f64).collect();
            let shaped: Vec<_> = ticks
                .iter()
                .enumerate()
                .map(|(k, &v)| {
                    let text = self.text_of(v, step);
                    self.shape(cx.text_store(), 0, k, &text)
                })
                .collect();
            let axis_w = shaped.iter().map(|(_, s)| s.width).fold(0.0, f32::max);
            // The lowest axis label hangs half a line below the plot: the category
            // labels go under it.
            let half_line = FONT * 0.7;
            let label_h = if self.labels.is_empty() {
                half_line
            } else {
                LABEL_ROW + half_line
            };
            let plot = Rect::new(
                axis_w + AXIS_GAP,
                top + half_line,
                (bounds.width - axis_w - AXIS_GAP - 4.0).max(1.0),
                (bounds.height - top - half_line - label_h).max(1.0),
            );
            for (&v, (layout, size)) in ticks.iter().zip(&shaped) {
                let y = Self::y_of(plot, v, lo, hi);
                cx.fill_rect(Rect::new(plot.x, y.round() - 0.5, plot.width, 1.0), grid);
                if let Some(layout) = layout {
                    let r = Rect::new(
                        axis_w - size.width,
                        y - size.height / 2.0,
                        size.width,
                        size.height,
                    );
                    cx.text(r, *layout, muted);
                }
            }

            let n = self.labels.len();
            if n > 0 {
                let widest = (0..n)
                    .map(|i| {
                        let label = self.labels[i].clone();
                        self.shape(cx.text_store(), 1, i, &label).1.width
                    })
                    .fold(0.0, f32::max);
                let room = plot.width / n as f32;
                let every = ((widest + 8.0) / room.max(1.0)).ceil().max(1.0) as usize;
                for i in (0..n).step_by(every) {
                    let label = self.labels[i].clone();
                    let (layout, size) = self.shape(cx.text_store(), 1, i, &label);
                    let x = self.x_of(plot, i);
                    let left =
                        (x - size.width / 2.0).clamp(0.0, (bounds.width - size.width).max(0.0));
                    if let Some(layout) = layout {
                        let y = plot.bottom() + half_line + (LABEL_ROW - size.height) / 2.0;
                        cx.text(Rect::new(left, y, size.width, size.height), layout, muted);
                    }
                }
            }

            if let Some(i) = self.inspected
                && self.kind == ChartKind::Line
            {
                let x = self.x_of(plot, i);
                cx.fill_rect(
                    Rect::new(x.round() - 0.5, plot.y, 1.0, plot.height),
                    muted.with_alpha(0.5),
                );
            }
            plot
        };
        self.plot = plot;

        cx.push_clip(Rect::new(
            plot.x - 4.0,
            plot.y - 4.0,
            plot.width + 8.0,
            plot.height + 8.0,
        ));
        self.paint_series(cx, plot, lo, hi);
        cx.pop_clip();

        if !self.sparkline
            && let Some(i) = self.inspected
        {
            let x = self.x_of(plot, i);
            self.paint_readout(cx, plot, x);
        }
        let store = cx.text_store();
        self.texts.end(store);
        if cx.is_focused() {
            focus_ring(cx, bounds, 4.0);
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        if self.sparkline {
            return;
        }
        match event {
            Event::PointerMove { pos } => {
                let p = cx.local(*pos);
                let inside = self.plot.width > 0.0
                    && p.y >= self.plot.y - 4.0
                    && p.y <= self.plot.bottom() + LABEL_ROW + FONT
                    && p.x >= self.plot.x - 8.0
                    && p.x <= self.plot.right() + 8.0;
                let index = if inside { self.category_at(p.x) } else { None };
                self.inspect(cx, index);
            }
            Event::PointerLeave | Event::HoverChanged(false) => self.inspect(cx, None),
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                let p = cx.local(*pos);
                if let Some(s) = self.legend.iter().position(|r| r.contains(p)) {
                    self.toggle(cx, s);
                    cx.set_handled();
                    return;
                }
                if p.y < self.plot.y - 4.0 {
                    return;
                }
                if let Some(i) = self.category_at(p.x) {
                    self.inspect(cx, Some(i));
                    cx.emit(ChartPointActivated(i));
                    cx.set_handled();
                }
            }
            Event::ActivateItem(s) if self.has_legend() && *s < self.series.len() => {
                self.toggle(cx, *s);
                cx.set_handled();
            }
            Event::KeyDown { key, modifiers } if cx.is_focused() => {
                if let Key::Char(c @ '1'..='9') = key
                    && !modifiers.command
                    && self.has_legend()
                {
                    let s = *c as usize - '1' as usize;
                    if s < self.series.len() {
                        self.toggle(cx, s);
                        cx.set_handled();
                    }
                    return;
                }
                let n = self.labels.len();
                if n == 0 {
                    return;
                }
                let index = match (key, self.inspected) {
                    (Key::ArrowLeft, Some(i)) => i.saturating_sub(1),
                    (Key::ArrowRight, Some(i)) => (i + 1).min(n - 1),
                    (Key::ArrowLeft | Key::ArrowRight | Key::End, _) => n - 1,
                    (Key::Home, _) => 0,
                    (Key::Enter | Key::Space, Some(i)) => {
                        cx.emit(ChartPointActivated(i));
                        cx.set_handled();
                        return;
                    }
                    (Key::Escape, Some(_)) => {
                        self.inspect(cx, None);
                        cx.set_handled();
                        return;
                    }
                    _ => return,
                };
                self.inspect(cx, Some(index));
                cx.set_handled();
            }
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        !self.sparkline
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Figure)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        let step = self.ticks().2;
        let mut parts: Vec<String> = self
            .series
            .iter()
            .enumerate()
            .map(|(i, s)| {
                if !self.is_series_visible(i) {
                    return format!("{}: hidden", s.name);
                }
                let finite = s.values.iter().copied().filter(|v| v.is_finite());
                let (min, max) = finite.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| {
                    (a.min(v), b.max(v))
                });
                if min > max {
                    format!("{}: no values", s.name)
                } else {
                    format!(
                        "{}: {} to {}",
                        s.name,
                        self.text_of(min, step),
                        self.text_of(max, step)
                    )
                }
            })
            .collect();
        if let Some((label, values)) = self.readout().filter(|(_, v)| !v.is_empty()) {
            let values: Vec<String> = values.iter().map(|(n, v)| format!("{n} {v}")).collect();
            parts.push(format!("{label}: {}", values.join(", ")));
        }
        node.set_description(parts.join("; "));
    }

    fn accessibility_items(&self) -> Vec<accesskit::Node> {
        if !self.has_legend() {
            return Vec::new();
        }
        (self.series.iter().enumerate())
            .map(|(s, series)| {
                let mut node = accesskit::Node::new(accesskit::Role::CheckBox);
                node.set_label(series.name.clone());
                node.set_toggled(accesskit::Toggled::from(self.is_series_visible(s)));
                if let Some(r) = self.legend.get(s) {
                    node.set_bounds(accesskit::Rect {
                        x0: r.x as f64,
                        y0: r.y as f64,
                        x1: r.right() as f64,
                        y1: r.bottom() as f64,
                    });
                }
                node.add_action(accesskit::Action::Click);
                node
            })
            .collect()
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.texts.release(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_are_round_and_cover_the_values() {
        assert_eq!(nice_ticks(0.0, 87.0, 4), (0.0, 100.0, 25.0));
        assert_eq!(nice_ticks(3.2, 9.7, 4), (2.0, 10.0, 2.0));
        assert_eq!(nice_ticks(-12.0, 30.0, 4), (-20.0, 40.0, 20.0));
        assert_eq!(nice_ticks(-12.0, 28.0, 4), (-20.0, 30.0, 10.0));
        // A flat or empty series still gets an axis.
        let (lo, hi, step) = nice_ticks(5.0, 5.0, 4);
        assert!(lo < 5.0 && hi > 5.0 && step > 0.0);
        assert_eq!(nice_ticks(0.0, 0.0, 4), (0.0, 1.0, 0.25));
    }

    #[test]
    fn values_are_written_as_tightly_as_their_axis_needs() {
        assert_eq!(format_value(25.0, 25.0), "25");
        assert_eq!(format_value(0.5, 0.25), "0.5");
        assert_eq!(format_value(0.25, 0.25), "0.25");
        assert_eq!(format_value(12_500.0, 2_500.0), "12.5k");
        assert_eq!(format_value(3_000_000.0, 1e6), "3M");
        assert_eq!(format_value(-0.0, 1.0), "0");
        // A step of 2.5 keeps its half steps.
        assert_eq!(format_value(7.5, 2.5), "7.5");
        assert_eq!(format_value(5.0, 2.5), "5");
        assert_eq!(format_value(0.125, 0.025), "0.125");
    }

    #[test]
    fn lines_out_of_a_fixed_range_are_cut_at_the_edge_with_their_slope() {
        let plot = Rect::new(0.0, 0.0, 100.0, 100.0);
        // From -100 to 100 on a 0..100 axis: below the plot to its top.
        let a = Point::new(0.0, Chart::y_free(plot, -100.0, 0.0, 100.0));
        let b = Point::new(100.0, Chart::y_free(plot, 100.0, 0.0, 100.0));
        let (from, to) = Chart::clip_segment(plot, a, b).unwrap();
        // The line enters at zero halfway across, not at the first category.
        assert!((from.x - 50.0).abs() < 1e-3 && (from.y - 100.0).abs() < 1e-3);
        assert!((to.x - 100.0).abs() < 1e-3 && to.y.abs() < 1e-3);
        let low = Point::new(0.0, 150.0);
        assert!(Chart::clip_segment(plot, low, Point::new(100.0, 130.0)).is_none());
    }
}

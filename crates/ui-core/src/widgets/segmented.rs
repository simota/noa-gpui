use ui_layout::{MeasureInput, Style};
use ui_scene::{Border, Rect, Size};
use ui_text::{TextStore, TextStyle};

use super::{CachedText, ControlSize, SelectionChanged, focus_ring, pressed_tint};
use crate::event::{Event, Key, PointerButton};
use crate::theme::{Corner, Role};
use crate::widget::{EventCx, MeasureCx, PaintCx, UpdateCx, Widget};

const PAD_Y: f32 = 5.0;

fn text_style(size: ControlSize) -> TextStyle {
    TextStyle::new(size.font(14.0))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentMode {
    /// Exactly one segment is selected ([`SelectionChanged`]).
    SelectOne,
    /// Each segment toggles on its own ([`SegmentsChanged`]).
    SelectAny,
    /// Segments act like buttons and stay unselected ([`SegmentClicked`]).
    Momentary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SegmentStyle {
    #[default]
    Bezel,
    Underline,
}

pub const SEGMENT_UNDERLINE: f32 = 2.0;

/// Emitted in [`SegmentMode::SelectAny`] with every segment's state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentsChanged(pub Vec<bool>);

/// Emitted in [`SegmentMode::Momentary`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentClicked(pub usize);

/// A row of segments (`NSSegmentedControl`).
#[derive(Debug)]
pub struct SegmentedControl {
    labels: Vec<String>,
    mode: SegmentMode,
    selected: Vec<bool>,
    shaped: Vec<CachedText>,
    /// Left edge of each segment plus the right edge of the last.
    edges: Vec<f32>,
    /// The segment the keyboard acts on.
    cursor: usize,
    pressed: Option<usize>,
    size: ControlSize,
    look: SegmentStyle,
    height: f32,
}

const SEGMENT_PAD: f32 = 12.0;

impl SegmentedControl {
    pub fn new<S: Into<String>>(labels: impl IntoIterator<Item = S>, mode: SegmentMode) -> Self {
        let labels: Vec<String> = labels.into_iter().map(Into::into).collect();
        let n = labels.len();
        let mut selected = vec![false; n];
        if mode == SegmentMode::SelectOne && n > 0 {
            selected[0] = true;
        }
        Self {
            shaped: (0..n).map(|_| CachedText::default()).collect(),
            labels,
            mode,
            selected,
            edges: Vec::new(),
            cursor: 0,
            pressed: None,
            size: ControlSize::Regular,
            look: SegmentStyle::Bezel,
            height: 0.0,
        }
    }

    pub fn control_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    /// [`SegmentStyle::Bezel`] by default.
    pub fn style(mut self, style: SegmentStyle) -> Self {
        self.look = style;
        self
    }

    /// Initial selection: for `SelectOne` only the first index counts.
    pub fn with_selected(mut self, indices: &[usize]) -> Self {
        if self.mode == SegmentMode::Momentary {
            return self;
        }
        self.selected.iter_mut().for_each(|s| *s = false);
        for &i in indices {
            if let Some(s) = self.selected.get_mut(i) {
                *s = true;
                if self.mode == SegmentMode::SelectOne {
                    self.cursor = i;
                    break;
                }
            }
        }
        self
    }

    /// Change the selection as [`SegmentedControl::with_selected`] does, without
    /// emitting a message. Does nothing in `Momentary` mode.
    pub fn set_selected(&mut self, cx: &mut UpdateCx, indices: &[usize]) {
        if self.mode == SegmentMode::Momentary {
            return;
        }
        let mut selected = vec![false; self.labels.len()];
        for &i in indices {
            if let Some(s) = selected.get_mut(i) {
                *s = true;
                if self.mode == SegmentMode::SelectOne {
                    self.cursor = i;
                    break;
                }
            }
        }
        // A `SelectOne` control keeps one segment selected.
        if self.mode == SegmentMode::SelectOne && !selected.contains(&true) {
            return;
        }
        self.selected = selected;
        cx.request_paint();
    }

    pub fn selected(&self) -> &[bool] {
        &self.selected
    }

    fn segment_at(&self, x: f32) -> Option<usize> {
        self.edges.windows(2).position(|w| x >= w[0] && x < w[1])
    }

    fn paint_underlined(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let radius = cx.corner(Corner::Control, 5.0, bounds.height);
        let n = self.labels.len();
        for i in 0..n {
            let (left, right) = (self.edges[i], self.edges[i + 1]);
            let selected = self.selected[i];
            if self.pressed == Some(i) {
                let tint = pressed_tint(cx, cx.color(Role::Background));
                let rect = Rect::new(left, 0.0, right - left, bounds.height);
                cx.rounded_rect(rect, radius, tint, Border::default());
            }
            if selected {
                let accent = cx.color(Role::Accent);
                let line = Rect::new(
                    left,
                    bounds.height - SEGMENT_UNDERLINE,
                    right - left,
                    SEGMENT_UNDERLINE,
                );
                cx.fill_rect(line, accent);
            }
            if let Some(layout) = self.shaped[i].layout() {
                let size = self.shaped[i].size();
                let ink = cx.color(if selected { Role::Text } else { Role::Muted });
                let x = left + (right - left - size.width) / 2.0;
                let y = (bounds.height - size.height) / 2.0;
                cx.text(Rect::new(x, y, size.width, size.height), layout, ink);
            }
        }
        if cx.is_focused() && self.cursor < n {
            let (left, right) = (self.edges[self.cursor], self.edges[self.cursor + 1]);
            focus_ring(
                cx,
                Rect::new(left, 0.0, right - left, bounds.height),
                radius,
            );
        }
    }

    fn activate(&mut self, cx: &mut EventCx, index: usize) {
        self.cursor = index;
        match self.mode {
            SegmentMode::SelectOne => {
                if !self.selected[index] {
                    self.selected.iter_mut().for_each(|s| *s = false);
                    self.selected[index] = true;
                    cx.emit(SelectionChanged(index));
                }
            }
            SegmentMode::SelectAny => {
                self.selected[index] = !self.selected[index];
                cx.emit(SegmentsChanged(self.selected.clone()));
            }
            SegmentMode::Momentary => cx.emit(SegmentClicked(index)),
        }
        cx.request_paint();
        cx.set_handled();
    }
}

impl Widget for SegmentedControl {
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
        let (k, style) = (self.size.scale(), text_style(self.size));
        let mut x = 0.0;
        let mut height: f32 = 0.0;
        self.edges.clear();
        for (i, label) in self.labels.iter().enumerate() {
            self.edges.push(x);
            let text = self.shaped[i].shape(cx.text, label, style, None);
            x += text.width + 2.0 * SEGMENT_PAD * k;
            height = height.max(text.height);
        }
        self.edges.push(x);
        self.height = height + 2.0 * PAD_Y * k;
        Size::new(x, self.height)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        if self.look == SegmentStyle::Underline {
            self.paint_underlined(cx);
            return;
        }
        let bounds = cx.bounds();
        let surface = cx.color(Role::Surface);
        let outline = cx.color(Role::Outline);
        let radius = cx.corner(Corner::Control, 6.0, bounds.height);
        let inner = cx.corner(Corner::Control, 5.0, bounds.height - 2.0);
        cx.bezel(
            bounds,
            radius,
            surface,
            Border {
                width: 1.0,
                color: outline,
            },
        );
        let n = self.labels.len();
        for i in 0..n {
            let (left, right) = (self.edges[i], self.edges[i + 1]);
            let rect = Rect::new(left + 1.0, 1.0, right - left - 2.0, bounds.height - 2.0);
            let selected = self.selected[i];
            if selected {
                let accent = cx.color(Role::Accent);
                cx.bezel(rect, inner, accent, Border::default());
            } else if self.pressed == Some(i) {
                let tint = pressed_tint(cx, surface);
                cx.rounded_rect(rect, inner, tint, Border::default());
            }
            if i + 1 < n && !selected && !self.selected[i + 1] {
                cx.fill_rect(Rect::new(right, 5.0, 1.0, bounds.height - 10.0), outline);
            }
            if let Some(layout) = self.shaped[i].layout() {
                let size = self.shaped[i].size();
                let ink = cx.color(if selected { Role::OnAccent } else { Role::Text });
                let x = left + (right - left - size.width) / 2.0;
                let y = (bounds.height - size.height) / 2.0;
                cx.text(Rect::new(x, y, size.width, size.height), layout, ink);
            }
        }
        if cx.is_focused() && self.cursor < n {
            let rect = Rect::new(
                self.edges[self.cursor],
                0.0,
                self.edges[self.cursor + 1] - self.edges[self.cursor],
                bounds.height,
            );
            focus_ring(cx, rect, radius);
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        let n = self.labels.len();
        if n == 0 {
            return;
        }
        match event {
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                self.pressed = self.segment_at(cx.local(*pos).x);
                cx.capture_pointer();
                cx.request_paint();
                cx.set_handled();
            }
            Event::PointerUp { pos, .. } => {
                let released = self.segment_at(cx.local(*pos).x);
                let pressed = self.pressed.take();
                cx.release_pointer();
                cx.request_paint();
                if let Some(i) = pressed.filter(|&p| Some(p) == released && cx.contains(*pos)) {
                    self.activate(cx, i);
                }
            }
            Event::KeyDown { key, .. } => match key {
                Key::ArrowLeft | Key::ArrowRight => {
                    self.cursor = if *key == Key::ArrowLeft {
                        (self.cursor + n - 1) % n
                    } else {
                        (self.cursor + 1) % n
                    };
                    cx.request_paint();
                    cx.set_handled();
                }
                Key::Space => self.activate(cx, self.cursor),
                _ => {}
            },
            Event::Activate => self.activate(cx, self.cursor),
            Event::ActivateItem(i) if *i < n => self.activate(cx, *i),
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::TabList)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        let chosen: Vec<&str> = self
            .labels
            .iter()
            .zip(&self.selected)
            .filter(|(_, s)| **s)
            .map(|(l, _)| l.as_str())
            .collect();
        node.set_value(chosen.join(", "));
    }

    fn accessibility_items(&self) -> Vec<accesskit::Node> {
        let height = self.height as f64;
        self.labels
            .iter()
            .enumerate()
            .map(|(i, label)| {
                let mut node = accesskit::Node::new(accesskit::Role::Tab);
                node.set_label(label.clone());
                if self.mode != SegmentMode::Momentary {
                    node.set_selected(self.selected[i]);
                }
                if let (Some(&x0), Some(&x1)) = (self.edges.get(i), self.edges.get(i + 1)) {
                    node.set_bounds(accesskit::Rect {
                        x0: x0 as f64,
                        y0: 0.0,
                        x1: x1 as f64,
                        y1: height,
                    });
                }
                node.add_action(accesskit::Action::Click);
                node
            })
            .collect()
    }

    fn removed(&mut self, text: &mut TextStore) {
        for cached in &mut self.shaped {
            cached.release(text);
        }
    }
}

use std::ops::Range;

use ui_layout::style::{
    AvailableSpace, Dimension, Overflow, Point as LayoutPoint, Size as LayoutSize, auto, length,
    percent,
};
use ui_layout::{MeasureInput, Style};
use ui_scene::{Border, Direction, Point, Rect, Size};
use ui_text::{TextStore, TextStyle};

use super::{CachedText, ControlSize, Icon, MenuItem, focus_ring, pressed_tint};
use crate::event::{Event, Key, PointerButton};
use crate::overlay::Placement;
use crate::theme::{Corner, Role};
use crate::widget::{EventCx, MeasureCx, PaintCx, UpdateCx, Widget};

/// Emitted by a [`PathControl`] with the index of the component chosen (0 = the root).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathItemClicked(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathStyle {
    Standard,
    PopUp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PathSeparator {
    #[default]
    Chevron,
    Slash,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PathItem {
    title: String,
    icon: Option<Icon>,
}

impl PathItem {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            icon: None,
        }
    }

    /// A 16 × 16 image or vector icon (in the text color) before the title.
    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn title(&self) -> &str {
        &self.title
    }
}

impl From<&str> for PathItem {
    fn from(title: &str) -> Self {
        Self::new(title)
    }
}

impl From<String> for PathItem {
    fn from(title: String) -> Self {
        Self::new(title)
    }
}

const PAD_X: f32 = 6.0;
const SEPARATOR_SPACE: f32 = 16.0;
const HEIGHT: f32 = 24.0;
const POPUP_ARROWS: f32 = 22.0;
const ICON: f32 = 16.0;
const ICON_GAP: f32 = 4.0;
const MORE: &str = "…";

#[derive(Debug, Clone, PartialEq)]
enum Part {
    Item(usize),
    More(Range<usize>),
}

/// The components of a path or other hierarchy, each clickable (`NSPathControl`).
/// Emits [`PathItemClicked`]; the application decides what that does.
#[derive(Debug)]
pub struct PathControl {
    items: Vec<PathItem>,
    style: PathStyle,
    separator: PathSeparator,
    size: ControlSize,
    width: Dimension,
    texts: Vec<CachedText>,
    more_text: CachedText,
    separator_text: CachedText,
    /// The components shown for the current width, with their left and right edges
    /// (standard style).
    parts: Vec<(Part, f32, f32)>,
    /// The width `parts` were arranged for.
    arranged: Option<f32>,
    /// Index into `parts` of the keyboard cursor.
    cursor: usize,
    pressed: Option<usize>,
    menu_open: bool,
    more_menu: Option<Range<usize>>,
}

impl PathControl {
    pub fn new<I: Into<PathItem>>(items: impl IntoIterator<Item = I>) -> Self {
        let items: Vec<PathItem> = items.into_iter().map(Into::into).collect();
        Self {
            texts: (0..items.len()).map(|_| CachedText::default()).collect(),
            items,
            style: PathStyle::Standard,
            separator: PathSeparator::Chevron,
            size: ControlSize::Regular,
            width: Dimension::auto(),
            more_text: CachedText::default(),
            separator_text: CachedText::default(),
            parts: Vec::new(),
            arranged: None,
            cursor: usize::MAX,
            pressed: None,
            menu_open: false,
            more_menu: None,
        }
    }

    pub fn style(mut self, style: PathStyle) -> Self {
        self.style = style;
        self
    }

    pub fn separator(mut self, separator: PathSeparator) -> Self {
        self.separator = separator;
        self
    }

    pub fn control_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    /// A fixed width (standard style collapses the middle of a path that does not fit).
    pub fn width(mut self, width: f32) -> Self {
        self.width = length(width);
        self
    }

    pub fn items(&self) -> &[PathItem] {
        &self.items
    }

    /// The indices of the components collapsed into "…" at the current width (empty
    /// when the whole path fits).
    pub fn collapsed(&self) -> Range<usize> {
        self.parts
            .iter()
            .find_map(|(part, ..)| match part {
                Part::More(range) => Some(range.clone()),
                Part::Item(_) => None,
            })
            .unwrap_or(0..0)
    }

    /// Replace the components. Call [`crate::UiTree::update`]; text is reshaped on the
    /// next layout.
    pub fn set_items<I: Into<PathItem>>(
        &mut self,
        cx: &mut UpdateCx,
        items: impl IntoIterator<Item = I>,
    ) {
        self.items = items.into_iter().map(Into::into).collect();
        // Old layouts are released when reshaped; extra caches keep theirs until
        // `removed`, so reuse rather than drop them.
        while self.texts.len() < self.items.len() {
            self.texts.push(CachedText::default());
        }
        self.cursor = usize::MAX;
        self.arranged = None;
        cx.request_layout();
        cx.request_paint();
    }

    fn text_style(&self) -> TextStyle {
        TextStyle::new(self.size.font(13.0))
    }

    fn separator_space(&self) -> f32 {
        SEPARATOR_SPACE * self.size.scale()
    }

    fn item_width(&self, i: usize) -> f32 {
        let k = self.size.scale();
        let icon = if self.items[i].icon.is_some() {
            (ICON + ICON_GAP) * k
        } else {
            0.0
        };
        self.texts[i].size().width + icon + 2.0 * PAD_X * k
    }

    fn more_width(&self) -> f32 {
        self.more_text.size().width + 2.0 * PAD_X * self.size.scale()
    }

    fn row_width(&self, parts: &[Part]) -> f32 {
        let widths: f32 = parts
            .iter()
            .map(|part| match part {
                Part::Item(i) => self.item_width(*i),
                Part::More(_) => self.more_width(),
            })
            .sum();
        widths + parts.len().saturating_sub(1) as f32 * self.separator_space()
    }

    /// The components to show in `width`: all of them if they fit; otherwise the first,
    /// "…" for the fewest components after it that make the rest fit, and the rest
    /// (at least the last component, even if it then has to be clipped).
    fn parts_for(&self, width: f32) -> Vec<Part> {
        let n = self.items.len();
        let all: Vec<Part> = (0..n).map(Part::Item).collect();
        if n <= 2 || self.row_width(&all) <= width + 0.5 {
            return all;
        }
        let collapsed = |end: usize| {
            let mut parts = vec![Part::Item(0), Part::More(1..end)];
            parts.extend((end..n).map(Part::Item));
            parts
        };
        (2..n)
            .map(collapsed)
            .find(|parts| self.row_width(parts) <= width + 0.5)
            .unwrap_or_else(|| collapsed(n - 1))
    }

    /// Lay the components out for `width` (standard style), keeping the cursor on the
    /// same component where it is still shown.
    fn arrange(&mut self, width: f32) {
        if self.arranged == Some(width) || self.style == PathStyle::PopUp {
            return;
        }
        let focused_item = self.parts.get(self.cursor).map(|(part, ..)| part.clone());
        let mut x = 0.0;
        self.parts = self
            .parts_for(width)
            .into_iter()
            .map(|part| {
                let w = match &part {
                    Part::Item(i) => self.item_width(*i),
                    Part::More(_) => self.more_width(),
                };
                let span = (part, x, x + w);
                x += w + self.separator_space();
                span
            })
            .collect();
        self.cursor = focused_item
            .and_then(|f| self.parts.iter().position(|(p, ..)| *p == f))
            .unwrap_or(self.parts.len().saturating_sub(1));
        self.arranged = Some(width);
    }

    fn part_at(&self, x: f32) -> Option<usize> {
        self.parts.iter().position(|&(_, l, r)| x >= l && x < r)
    }

    fn choose(&mut self, cx: &mut EventCx, index: usize) {
        cx.emit(PathItemClicked(index));
        cx.request_paint();
        cx.set_handled();
    }

    fn activate(&mut self, cx: &mut EventCx, index: usize) {
        let Some((part, left, right)) = self.parts.get(index).cloned() else {
            return;
        };
        self.cursor = index;
        match part {
            Part::Item(i) => self.choose(cx, i),
            Part::More(range) => {
                let items = range
                    .clone()
                    .map(|i| MenuItem::new(self.items[i].title.clone()))
                    .collect();
                let origin = cx.window_rect();
                let anchor = Rect::new(origin.x + left, origin.y, right - left, origin.height);
                cx.open_menu(items, Placement::Below(anchor), 0.0, None);
                self.more_menu = Some(range);
                cx.request_paint();
                cx.set_handled();
            }
        }
    }

    fn open_menu(&mut self, cx: &mut EventCx) {
        if self.items.is_empty() {
            return;
        }
        self.menu_open = true;
        let items = self
            .items
            .iter()
            .map(|item| MenuItem::new(item.title.clone()))
            .collect();
        let anchor = cx.window_rect();
        let checked = Some(self.items.len() - 1);
        cx.open_menu(items, Placement::Below(anchor), anchor.width, checked);
        cx.request_paint();
        cx.set_handled();
    }

    fn paint_pop_up(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let k = self.size.scale();
        let text = cx.color(Role::Text);
        let surface = cx.color(Role::Surface);
        let fill = if self.menu_open {
            pressed_tint(cx, surface)
        } else {
            surface
        };
        let outline = cx.color(Role::Outline);
        let radius = cx.corner(Corner::Control, 5.0 * k, bounds.height);
        cx.bezel(
            bounds,
            radius,
            fill,
            Border {
                width: 1.0,
                color: outline,
            },
        );
        if let Some(last) = self.items.len().checked_sub(1) {
            let mut x = PAD_X * k;
            if let Some(icon) = self.items[last].icon.clone() {
                let s = ICON * k;
                icon.paint(cx, Rect::new(x, (bounds.height - s) / 2.0, s, s), text);
                x += s + ICON_GAP * k;
            }
            let cached = &self.texts[last];
            if let Some(layout) = cached.layout() {
                let size = cached.size();
                let room = (bounds.width - POPUP_ARROWS * k - x).max(0.0);
                cx.push_clip(Rect::new(x, 0.0, room, bounds.height));
                let y = (bounds.height - size.height) / 2.0;
                cx.text(Rect::new(x, y, size.width, size.height), layout, text);
                cx.pop_clip();
            }
        }
        let x = bounds.width - POPUP_ARROWS * k + 6.0 * k;
        let (w, h) = (7.0 * k, 3.5 * k);
        let mid = bounds.height / 2.0;
        cx.chevron(Rect::new(x, mid - h - 1.5, w, h), Direction::Up, 1.5, text);
        cx.chevron(Rect::new(x, mid + 1.5, w, h), Direction::Down, 1.5, text);
        if cx.is_focused() && !self.menu_open {
            focus_ring(cx, bounds, radius);
        }
    }
}

impl Widget for PathControl {
    /// A standard control shrinks along a row and is never wider than its parent
    /// (a column does not shrink its children across). Its measured width is the whole
    /// path, but it draws the collapsed path inside its box, so the whole path is not
    /// overflow a scroll view should make room for.
    fn style(&self) -> Style {
        let standard = self.style == PathStyle::Standard;
        Style {
            overflow: LayoutPoint {
                x: if standard {
                    Overflow::Clip
                } else {
                    Overflow::Visible
                },
                y: Overflow::Visible,
            },
            size: LayoutSize {
                width: self.width,
                height: Dimension::auto(),
            },
            max_size: LayoutSize {
                width: if standard { percent(1.0) } else { auto() },
                height: auto(),
            },
            flex_shrink: if standard { 1.0 } else { 0.0 },
            ..Default::default()
        }
    }

    fn measures(&self) -> bool {
        true
    }

    /// The whole path, or at min-content the narrowest arrangement (the first and last
    /// components around "…"), which is how far a standard control shrinks.
    fn measure(&mut self, cx: &mut MeasureCx, input: MeasureInput) -> Size {
        let k = self.size.scale();
        let style = self.text_style();
        for (item, cached) in self.items.iter().zip(&mut self.texts) {
            cached.shape(cx.text, &item.title, style, None);
        }
        self.more_text.shape(cx.text, MORE, style, None);
        self.separator_text.shape(cx.text, "/", style, None);
        self.arranged = None;
        let n = self.items.len();
        let width = match self.style {
            PathStyle::Standard => {
                let all: Vec<Part> = (0..n).map(Part::Item).collect();
                let full = self.row_width(&all);
                if input.available.width == AvailableSpace::MinContent {
                    full.min(self.row_width(&self.parts_for(0.0)))
                } else {
                    full
                }
            }
            PathStyle::PopUp => {
                let last = n
                    .checked_sub(1)
                    .map_or(2.0 * PAD_X * k, |i| self.item_width(i));
                last + POPUP_ARROWS * k
            }
        };
        Size::new(width, HEIGHT * k)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        if self.style == PathStyle::PopUp {
            self.paint_pop_up(cx);
            return;
        }
        let bounds = cx.bounds();
        self.arrange(bounds.width);
        let k = self.size.scale();
        let text = cx.color(Role::Text);
        let muted = cx.color(Role::Muted);
        let last_part = self.parts.len().saturating_sub(1);
        cx.push_clip(Rect::new(
            -4.0,
            -4.0,
            bounds.width + 8.0,
            bounds.height + 8.0,
        ));
        for (index, (part, left, right)) in self.parts.clone().into_iter().enumerate() {
            let rect = Rect::new(left, 0.0, right - left, bounds.height);
            let open = matches!(&part, Part::More(r) if self.more_menu.as_ref() == Some(r));
            if self.pressed == Some(index) || open {
                let tint = pressed_tint(cx, cx.color(Role::Background));
                cx.rounded_rect(rect, 4.0 * k, tint, Border::default());
            }
            let ink = if index == last_part { text } else { muted };
            let mut x = left + PAD_X * k;
            let cached = match &part {
                Part::Item(i) => {
                    if let Some(icon) = self.items[*i].icon.clone() {
                        let s = ICON * k;
                        icon.paint(cx, Rect::new(x, (bounds.height - s) / 2.0, s, s), ink);
                        x += s + ICON_GAP * k;
                    }
                    &self.texts[*i]
                }
                Part::More(_) => &self.more_text,
            };
            if let Some(layout) = cached.layout() {
                let size = cached.size();
                let room = (bounds.width - x).max(0.0);
                let clip = size.width > room;
                if clip {
                    cx.push_clip(Rect::new(x, 0.0, room, bounds.height));
                }
                let y = (bounds.height - size.height) / 2.0;
                cx.text(Rect::new(x, y, size.width, size.height), layout, ink);
                if clip {
                    cx.pop_clip();
                }
            }
            if index < last_part {
                self.paint_separator(cx, right, bounds.height, muted);
            }
            if cx.is_focused() && index == self.cursor {
                focus_ring(cx, rect, 4.0 * k);
            }
        }
        cx.pop_clip();
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        if self.items.is_empty() {
            return;
        }
        if self.style == PathStyle::PopUp {
            match event {
                Event::PointerDown {
                    button: PointerButton::Primary,
                    ..
                }
                | Event::KeyDown {
                    key: Key::Space, ..
                }
                | Event::Activate => self.open_menu(cx),
                Event::MenuClosed { chosen } => {
                    self.menu_open = false;
                    cx.request_paint();
                    if let Some(i) = *chosen {
                        self.choose(cx, i);
                    }
                }
                _ => {}
            }
            return;
        }
        self.arrange(cx.size().width);
        let n = self.parts.len();
        match event {
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                self.pressed = self.part_at(cx.local(*pos).x);
                cx.capture_pointer();
                cx.request_paint();
                cx.set_handled();
            }
            Event::PointerUp { pos, .. } => {
                let released = self.part_at(cx.local(*pos).x);
                let pressed = self.pressed.take();
                cx.release_pointer();
                cx.request_paint();
                if let Some(i) = pressed.filter(|&p| Some(p) == released && cx.contains(*pos)) {
                    self.activate(cx, i);
                }
            }
            Event::MenuClosed { chosen } => {
                let range = self.more_menu.take();
                cx.request_paint();
                if let (Some(range), Some(j)) = (range, *chosen) {
                    self.choose(cx, range.start + j);
                }
            }
            Event::KeyDown { key, .. } => match key {
                Key::ArrowLeft | Key::ArrowRight if n > 0 => {
                    self.cursor = if *key == Key::ArrowLeft {
                        self.cursor.saturating_sub(1)
                    } else {
                        (self.cursor + 1).min(n - 1)
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
        Some(if self.style == PathStyle::PopUp {
            accesskit::Role::ComboBox
        } else {
            accesskit::Role::List
        })
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        let titles: Vec<&str> = self.items.iter().map(PathItem::title).collect();
        node.set_value(titles.join(" › "));
        if self.style == PathStyle::PopUp {
            node.set_has_popup(accesskit::HasPopup::Menu);
            node.add_action(accesskit::Action::Click);
        }
    }

    fn accessibility_items(&self) -> Vec<accesskit::Node> {
        if self.style == PathStyle::PopUp {
            return Vec::new();
        }
        let height = f64::from(HEIGHT * self.size.scale());
        self.parts
            .iter()
            .map(|(part, l, r)| {
                let mut node = match part {
                    Part::Item(i) => {
                        let mut node = accesskit::Node::new(accesskit::Role::Link);
                        node.set_label(self.items[*i].title.clone());
                        node
                    }
                    Part::More(range) => {
                        let mut node = accesskit::Node::new(accesskit::Role::Button);
                        let hidden: Vec<&str> = self.items[range.clone()]
                            .iter()
                            .map(PathItem::title)
                            .collect();
                        node.set_label(MORE);
                        node.set_description(hidden.join(" › "));
                        node.set_has_popup(accesskit::HasPopup::Menu);
                        node
                    }
                };
                node.set_bounds(accesskit::Rect {
                    x0: f64::from(*l),
                    y0: 0.0,
                    x1: f64::from(*r),
                    y1: height,
                });
                node.add_action(accesskit::Action::Click);
                node
            })
            .collect()
    }

    fn removed(&mut self, text: &mut TextStore) {
        for cached in &mut self.texts {
            cached.release(text);
        }
        self.more_text.release(text);
        self.separator_text.release(text);
    }
}

impl PathControl {
    fn paint_separator(&self, cx: &mut PaintCx, right: f32, height: f32, ink: ui_scene::Color) {
        let space = self.separator_space();
        match self.separator {
            PathSeparator::Chevron => {
                let k = self.size.scale();
                let (w, h) = (3.5 * k, 7.0 * k);
                let x = right + (space - w) / 2.0;
                cx.chevron(
                    Rect::new(x, (height - h) / 2.0, w, h),
                    Direction::Right,
                    1.2,
                    ink,
                );
            }
            PathSeparator::Slash => {
                if let Some(layout) = self.separator_text.layout() {
                    let size = self.separator_text.size();
                    let at = Point::new(
                        right + (space - size.width) / 2.0,
                        (height - size.height) / 2.0,
                    );
                    cx.text(Rect::new(at.x, at.y, size.width, size.height), layout, ink);
                }
            }
        }
    }
}

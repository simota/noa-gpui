use ui_layout::Style;
use ui_layout::style::{Size as LayoutSize, length, percent};
use ui_scene::{Border, Color, Direction, Point, Rect};
use ui_text::{TextStore, TextStyle};

use super::editable::SingleLine;
use super::{CachedText, Icon, MenuItem, TextSubmitted, pressed_tint};
use crate::event::{Event, Key, PointerButton};
use crate::overlay::Placement;
use crate::theme::{Corner, Role, mix};
use crate::widget::{EventCx, PaintCx, UpdateCx, Widget};

const PAD_X: f32 = 8.0;
const GAP: f32 = 6.0;
const ICON: f32 = 22.0;
const CONTROL: f32 = 28.0;
const SPACE: f32 = 10.0;
const SEPARATOR: f32 = 9.0;
/// Width of the overflow button.
const CHEVRON: f32 = 26.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarSymbol {
    Back,
    Forward,
    Add,
    Sidebar,
    Share,
    Info,
}

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Button {
        symbol: Option<ToolbarSymbol>,
        image: Option<Icon>,
    },
    Search {
        placeholder: String,
        width: f32,
        min_width: f32,
    },
    Space,
    FlexibleSpace,
    Separator,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolbarItem {
    kind: Kind,
    label: String,
    enabled: bool,
}

impl ToolbarItem {
    /// A button with a label (shown under the icon, or alone, by the toolbar's
    /// [`ToolbarDisplay`], and in the overflow menu).
    pub fn button(label: impl Into<String>) -> Self {
        Self {
            kind: Kind::Button {
                symbol: None,
                image: None,
            },
            label: label.into(),
            enabled: true,
        }
    }

    /// A search field `width` wide that may shrink to `min_width` before items move
    /// to the overflow menu. It emits [`super::TextChanged`] and [`TextSubmitted`].
    pub fn search(placeholder: impl Into<String>, width: f32, min_width: f32) -> Self {
        Self {
            kind: Kind::Search {
                placeholder: placeholder.into(),
                width,
                min_width: min_width.min(width),
            },
            label: "Search".into(),
            enabled: true,
        }
    }

    pub fn space() -> Self {
        Self {
            kind: Kind::Space,
            label: String::new(),
            enabled: false,
        }
    }

    /// A gap that takes the spare width, pushing later items to the right.
    pub fn flexible_space() -> Self {
        Self {
            kind: Kind::FlexibleSpace,
            label: String::new(),
            enabled: false,
        }
    }

    pub fn separator() -> Self {
        Self {
            kind: Kind::Separator,
            label: String::new(),
            enabled: false,
        }
    }

    pub fn symbol(mut self, symbol: ToolbarSymbol) -> Self {
        if let Kind::Button { symbol: s, .. } = &mut self.kind {
            *s = Some(symbol);
        }
        self
    }

    /// An image or a vector icon (drawn in the item's text color) instead of a symbol.
    pub fn image(mut self, image: impl Into<Icon>) -> Self {
        if let Kind::Button { image: i, .. } = &mut self.kind {
            *i = Some(image.into());
        }
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        if matches!(self.kind, Kind::Button { .. }) {
            self.enabled = enabled;
        }
        self
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    fn is_button(&self) -> bool {
        matches!(self.kind, Kind::Button { .. })
    }

    fn is_search(&self) -> bool {
        matches!(self.kind, Kind::Search { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToolbarDisplay {
    #[default]
    IconAndLabel,
    IconOnly,
    LabelOnly,
}

/// Emitted when a toolbar button is clicked, or an item is chosen from the overflow
/// menu, with the item's index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolbarItemClicked(pub usize);

/// A row of buttons, spaces and a search field across the top of a window (`NSToolbar`).
///
/// When the items do not fit, the search field first shrinks to its minimum width, then
/// items move, from the right, into an overflow menu. Emits [`ToolbarItemClicked`].
#[derive(Debug)]
pub struct Toolbar {
    items: Vec<ToolbarItem>,
    display: ToolbarDisplay,
    labels: Vec<CachedText>,
    search: Option<SingleLine>,
    /// Where each item is, or `None` when it is in the overflow menu (from the last
    /// paint).
    rects: Vec<Option<Rect>>,
    chevron: Option<Rect>,
    /// Menu position → item, for the open overflow menu.
    overflow: Vec<usize>,
    pressed: Option<usize>,
    hovered: Option<usize>,
    /// Typing goes to the search field (it was clicked last).
    searching: bool,
    width: f32,
}

impl Toolbar {
    pub fn new(items: Vec<ToolbarItem>) -> Self {
        let search = items.iter().find_map(|i| match &i.kind {
            Kind::Search {
                placeholder, width, ..
            } => {
                let mut line = SingleLine::new(placeholder.clone(), *width, 6.0);
                line.pad_left = 26.0;
                line.edit.style = TextStyle::new(13.0);
                Some(line)
            }
            _ => None,
        });
        Self {
            labels: (0..items.len()).map(|_| CachedText::default()).collect(),
            items,
            display: ToolbarDisplay::IconAndLabel,
            search,
            rects: Vec::new(),
            chevron: None,
            overflow: Vec::new(),
            pressed: None,
            hovered: None,
            searching: false,
            width: 0.0,
        }
    }

    pub fn display(mut self, display: ToolbarDisplay) -> Self {
        self.display = display;
        self
    }

    pub fn items(&self) -> &[ToolbarItem] {
        &self.items
    }

    pub fn set_display(&mut self, cx: &mut UpdateCx, display: ToolbarDisplay) {
        if display != self.display {
            self.display = display;
            cx.request_layout();
            cx.request_paint();
        }
    }

    pub fn overflow_items(&self) -> Vec<usize> {
        (0..self.rects.len())
            .filter(|&i| self.rects[i].is_none() && self.items[i].enabled_kind())
            .collect()
    }

    /// Where item `i` is (widget coordinates), or `None` when it is in the overflow
    /// menu, after the last paint.
    pub fn item_rect(&self, i: usize) -> Option<Rect> {
        self.rects.get(i).copied().flatten()
    }

    pub fn search_text(&self) -> &str {
        self.search.as_ref().map_or("", |s| s.edit.editor.text())
    }

    pub fn set_search_text(&mut self, cx: &mut UpdateCx, text: impl Into<String>) {
        if let Some(search) = &mut self.search {
            search.edit.editor.set_text(&text.into());
            cx.request_paint();
        }
    }

    pub fn set_enabled(&mut self, cx: &mut UpdateCx, item: usize, enabled: bool) {
        if let Some(i) = self.items.get_mut(item)
            && i.is_button()
        {
            i.enabled = enabled;
            cx.request_paint();
        }
    }

    fn height(&self) -> f32 {
        match self.display {
            ToolbarDisplay::IconAndLabel => 54.0,
            _ => 40.0,
        }
    }

    fn label_style() -> TextStyle {
        TextStyle::new(11.0)
    }

    fn preferred(&self, i: usize, label: f32) -> f32 {
        match &self.items[i].kind {
            Kind::Button { .. } => match self.display {
                ToolbarDisplay::IconAndLabel => (label + 12.0).max(40.0),
                ToolbarDisplay::IconOnly => 34.0,
                ToolbarDisplay::LabelOnly => label + 20.0,
            },
            Kind::Search { width, .. } => *width,
            Kind::Space => SPACE,
            Kind::FlexibleSpace => 0.0,
            Kind::Separator => SEPARATOR,
        }
    }

    /// Place the items in `width` (see the type's docs). `labels` are the label widths.
    fn arrange(&mut self, width: f32, labels: &[f32]) {
        let n = self.items.len();
        let avail = (width - 2.0 * PAD_X).max(0.0);
        let mut widths: Vec<f32> = (0..n).map(|i| self.preferred(i, labels[i])).collect();
        let mut shown = vec![true; n];
        let total = |widths: &[f32], shown: &[bool]| -> f32 {
            let count = shown.iter().filter(|&&s| s).count();
            let sum: f32 = (0..n).filter(|&i| shown[i]).map(|i| widths[i]).sum();
            sum + GAP * count.saturating_sub(1) as f32
        };
        let mut used = total(&widths, &shown);
        if used > avail
            && let Some(s) = self.items.iter().position(ToolbarItem::is_search)
            && let Kind::Search { min_width, .. } = self.items[s].kind
        {
            let shrink = (used - avail).min(widths[s] - min_width);
            widths[s] -= shrink;
            used -= shrink;
        }
        let overflowing = used > avail;
        let room = if overflowing {
            avail - CHEVRON - GAP
        } else {
            avail
        };
        let mut last = n;
        while total(&widths, &shown) > room && last > 0 {
            last -= 1;
            shown[last] = false;
        }
        // No lone separator or space at the end.
        while last > 0
            && matches!(
                self.items[last - 1].kind,
                Kind::Space | Kind::Separator | Kind::FlexibleSpace
            )
            && overflowing
        {
            last -= 1;
            shown[last] = false;
        }
        let used = total(&widths, &shown);
        let flexible: Vec<usize> = (0..n)
            .filter(|&i| shown[i] && self.items[i].kind == Kind::FlexibleSpace)
            .collect();
        if !flexible.is_empty() {
            let extra = (room - used).max(0.0) / flexible.len() as f32;
            for &i in &flexible {
                widths[i] += extra;
            }
        }
        let h = self.height();
        let mut x = PAD_X;
        self.rects = (0..n)
            .map(|i| {
                if !shown[i] {
                    return None;
                }
                let item_h = match (&self.items[i].kind, self.display) {
                    (Kind::Button { .. }, ToolbarDisplay::IconAndLabel) => h - 8.0,
                    (Kind::Button { .. } | Kind::Search { .. }, _) => CONTROL,
                    _ => h - 16.0,
                };
                let rect = Rect::new(x, (h - item_h) / 2.0, widths[i], item_h);
                x += widths[i] + GAP;
                Some(rect)
            })
            .collect();
        self.chevron = overflowing.then(|| {
            Rect::new(
                width - PAD_X - CHEVRON,
                (h - CONTROL) / 2.0,
                CHEVRON,
                CONTROL,
            )
        });
        if let (Some(s), Some(search)) = (
            self.items.iter().position(ToolbarItem::is_search),
            &mut self.search,
        ) {
            search.width = widths[s];
        }
    }

    fn search_index(&self) -> Option<usize> {
        self.items.iter().position(ToolbarItem::is_search)
    }

    /// Where the search text starts (widget coordinates).
    fn search_origin(&self) -> Option<Point> {
        let rect = self.item_rect(self.search_index()?)?;
        let line = self.search.as_ref()?;
        Some(Point::new(
            rect.x + line.pad_left - line.scroll_x,
            rect.y + (rect.height - line.edit.style.line_height) / 2.0,
        ))
    }

    fn forward(&mut self, cx: &mut EventCx, event: &Event) {
        let Some(origin) = self.search_origin() else {
            return;
        };
        let Some(search) = &mut self.search else {
            return;
        };
        if let Event::KeyDown {
            key: Key::Enter, ..
        } = event
            && search.edit.composition().is_empty()
        {
            cx.emit(TextSubmitted(search.edit.editor.text().to_owned()));
            cx.set_handled();
            return;
        }
        search.edit.event(cx, event, origin);
        cx.request_paint();
    }

    fn open_overflow(&mut self, cx: &mut EventCx) {
        let Some(chevron) = self.chevron else {
            return;
        };
        self.overflow.clear();
        let mut items = Vec::new();
        for i in 0..self.items.len() {
            if self.rects.get(i).copied().flatten().is_some() {
                continue;
            }
            let item = &self.items[i];
            match item.kind {
                Kind::Button { .. } => {
                    items.push(MenuItem::new(&item.label).enabled(item.enabled));
                }
                Kind::Search { .. } => items.push(MenuItem::new("Search…")),
                Kind::Separator if !items.is_empty() => items.push(MenuItem::separator()),
                _ => continue,
            }
            self.overflow.push(i);
        }
        let origin = cx.window_rect();
        let anchor = Rect::new(
            origin.x + chevron.x,
            origin.y + chevron.y,
            chevron.width,
            chevron.height,
        );
        cx.open_menu(items, Placement::Below(anchor), 0.0, None);
    }

    fn paint_symbol(cx: &mut PaintCx, symbol: ToolbarSymbol, r: Rect, ink: Color) {
        let (cx0, cy0) = (r.x + r.width / 2.0, r.y + r.height / 2.0);
        let w = 1.6;
        match symbol {
            ToolbarSymbol::Back => cx.chevron(
                Rect::new(cx0 - 3.0, cy0 - 6.0, 6.0, 12.0),
                Direction::Left,
                2.0,
                ink,
            ),
            ToolbarSymbol::Forward => cx.chevron(
                Rect::new(cx0 - 3.0, cy0 - 6.0, 6.0, 12.0),
                Direction::Right,
                2.0,
                ink,
            ),
            ToolbarSymbol::Add => {
                cx.line(
                    Point::new(cx0 - 7.0, cy0),
                    Point::new(cx0 + 7.0, cy0),
                    w,
                    ink,
                );
                cx.line(
                    Point::new(cx0, cy0 - 7.0),
                    Point::new(cx0, cy0 + 7.0),
                    w,
                    ink,
                );
            }
            ToolbarSymbol::Sidebar => {
                let frame = Rect::new(cx0 - 9.0, cy0 - 7.0, 18.0, 14.0);
                cx.rounded_rect(
                    frame,
                    3.0,
                    Color::TRANSPARENT,
                    Border {
                        width: w,
                        color: ink,
                    },
                );
                cx.line(
                    Point::new(cx0 - 3.0, cy0 - 7.0),
                    Point::new(cx0 - 3.0, cy0 + 7.0),
                    w,
                    ink,
                );
            }
            ToolbarSymbol::Share => {
                cx.line(
                    Point::new(cx0 - 3.5, cy0 - 2.0),
                    Point::new(cx0 - 7.0, cy0 - 2.0),
                    w,
                    ink,
                );
                cx.line(
                    Point::new(cx0 - 7.0, cy0 - 2.0),
                    Point::new(cx0 - 7.0, cy0 + 8.0),
                    w,
                    ink,
                );
                cx.line(
                    Point::new(cx0 - 7.0, cy0 + 8.0),
                    Point::new(cx0 + 7.0, cy0 + 8.0),
                    w,
                    ink,
                );
                cx.line(
                    Point::new(cx0 + 7.0, cy0 + 8.0),
                    Point::new(cx0 + 7.0, cy0 - 2.0),
                    w,
                    ink,
                );
                cx.line(
                    Point::new(cx0 + 7.0, cy0 - 2.0),
                    Point::new(cx0 + 3.5, cy0 - 2.0),
                    w,
                    ink,
                );
                cx.line(
                    Point::new(cx0, cy0 + 3.0),
                    Point::new(cx0, cy0 - 9.0),
                    w,
                    ink,
                );
                cx.chevron(
                    Rect::new(cx0 - 3.5, cy0 - 9.5, 7.0, 3.5),
                    Direction::Up,
                    w,
                    ink,
                );
            }
            ToolbarSymbol::Info => {
                cx.arc(Point::new(cx0, cy0), 8.0, w, 1.0, ink);
                cx.line(
                    Point::new(cx0, cy0 - 1.0),
                    Point::new(cx0, cy0 + 4.0),
                    2.0,
                    ink,
                );
                cx.line(
                    Point::new(cx0, cy0 - 4.5),
                    Point::new(cx0, cy0 - 4.4),
                    2.4,
                    ink,
                );
            }
        }
    }
}

impl ToolbarItem {
    /// Whether the item belongs in the overflow menu (buttons and the search field).
    fn enabled_kind(&self) -> bool {
        self.is_button() || self.is_search()
    }
}

impl Widget for Toolbar {
    fn tooltip(&self, local: Point) -> Option<String> {
        if self.display != ToolbarDisplay::IconOnly {
            return None;
        }
        let i = (0..self.items.len())
            .find(|&i| self.item_rect(i).is_some_and(|r| r.contains(local)))?;
        let item = &self.items[i];
        (item.is_button() && !item.label.is_empty()).then(|| item.label.clone())
    }

    fn style(&self) -> Style {
        Style {
            size: LayoutSize {
                width: percent(1.0),
                height: length(self.height()),
            },
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        self.width = bounds.width;
        let style = Self::label_style();
        let labels: Vec<f32> = (0..self.items.len())
            .map(|i| {
                if self.items[i].is_button() {
                    let label = self.items[i].label.clone();
                    self.labels[i]
                        .shape(cx.text_store(), &label, style, None)
                        .width
                } else {
                    0.0
                }
            })
            .collect();
        self.arrange(bounds.width, &labels);

        let surface = cx.color(Role::SurfaceAlt);
        cx.fill_rect(bounds, surface);
        let line = cx.color(Role::Separator);
        cx.fill_rect(Rect::new(0.0, bounds.height - 1.0, bounds.width, 1.0), line);
        let ink = cx.color(Role::Text);
        let muted = cx.color(Role::Muted);
        let focused = cx.is_focused();
        for i in 0..self.items.len() {
            let Some(rect) = self.rects[i] else { continue };
            let item = self.items[i].clone();
            match item.kind {
                Kind::Button { symbol, image } => {
                    let tint = if !item.enabled {
                        None
                    } else if self.pressed == Some(i) {
                        Some(pressed_tint(cx, mix(surface, ink, 0.1)))
                    } else if self.hovered == Some(i) {
                        Some(mix(surface, ink, 0.08))
                    } else {
                        None
                    };
                    let color = if item.enabled {
                        ink
                    } else {
                        ink.with_alpha(0.35)
                    };
                    let icon_box = match self.display {
                        ToolbarDisplay::IconAndLabel => Rect::new(
                            rect.x + (rect.width - ICON - 10.0) / 2.0,
                            rect.y + 2.0,
                            ICON + 10.0,
                            ICON + 4.0,
                        ),
                        _ => rect,
                    };
                    if let Some(tint) = tint {
                        let radius = cx.corner(Corner::Control, 6.0, icon_box.height);
                        cx.bezel(icon_box, radius, tint, Border::default());
                    }
                    if self.display != ToolbarDisplay::LabelOnly {
                        let icon = Rect::new(
                            icon_box.x + (icon_box.width - ICON) / 2.0,
                            icon_box.y + (icon_box.height - ICON) / 2.0,
                            ICON,
                            ICON,
                        );
                        if let Some(image) = image {
                            image.paint(cx, icon, color);
                        } else if let Some(symbol) = symbol {
                            Self::paint_symbol(cx, symbol, icon, color);
                        }
                    }
                    if self.display != ToolbarDisplay::IconOnly
                        && let Some(layout) = self.labels[i].layout()
                    {
                        let size = self.labels[i].size();
                        let y = match self.display {
                            ToolbarDisplay::IconAndLabel => icon_box.bottom() + 2.0,
                            _ => rect.y + (rect.height - size.height) / 2.0,
                        };
                        let label_color = if item.enabled {
                            muted
                        } else {
                            muted.with_alpha(0.5)
                        };
                        let label_color = if self.display == ToolbarDisplay::LabelOnly {
                            color
                        } else {
                            label_color
                        };
                        cx.text(
                            Rect::new(
                                rect.x + (rect.width - size.width) / 2.0,
                                y,
                                size.width,
                                size.height,
                            ),
                            layout,
                            label_color,
                        );
                    }
                }
                Kind::Search { .. } => {
                    let active = focused && self.searching;
                    let field = cx.color(Role::Surface);
                    let border = if active {
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
                    let radius = cx.corner(Corner::Control, 7.0, rect.height);
                    cx.bezel(rect, radius, field, border);
                    let c = Point::new(rect.x + 13.0, rect.y + rect.height / 2.0 - 1.0);
                    cx.arc(c, 4.5, 1.5, 1.0, muted);
                    cx.line(
                        Point::new(c.x + 3.5, c.y + 3.5),
                        Point::new(c.x + 7.0, c.y + 7.0),
                        1.5,
                        muted,
                    );
                    if let Some(search) = &mut self.search {
                        search.measure(cx.text_store());
                    }
                    if let (Some(origin), Some(search)) = (self.search_origin(), &self.search) {
                        cx.push_clip(Rect::new(
                            rect.x + 24.0,
                            rect.y,
                            rect.width - 28.0,
                            rect.height,
                        ));
                        search.edit.paint(cx, origin, active);
                        cx.pop_clip();
                    }
                }
                Kind::Separator => {
                    let x = rect.x + rect.width / 2.0;
                    cx.fill_rect(Rect::new(x - 0.5, rect.y, 1.0, rect.height), line);
                }
                Kind::Space | Kind::FlexibleSpace => {}
            }
        }
        if let Some(chevron) = self.chevron {
            let r = Rect::new(chevron.x + 7.0, chevron.y + 9.0, 5.0, 10.0);
            cx.chevron(r, Direction::Right, 1.6, ink);
            cx.chevron(r.translate(6.0, 0.0), Direction::Right, 1.6, ink);
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                let p = cx.local(*pos);
                if self.chevron.is_some_and(|c| c.contains(p)) {
                    self.searching = false;
                    self.open_overflow(cx);
                    cx.set_handled();
                    return;
                }
                let Some(i) = self
                    .rects
                    .iter()
                    .position(|r| r.is_some_and(|r| r.contains(p)))
                else {
                    return;
                };
                if self.items[i].is_search() {
                    self.searching = true;
                    self.forward(cx, event);
                } else if self.items[i].is_button() && self.items[i].enabled {
                    self.searching = false;
                    self.pressed = Some(i);
                    cx.capture_pointer();
                    cx.request_paint();
                }
                cx.set_handled();
            }
            Event::PointerMove { pos } => {
                if cx.has_capture() && self.pressed.is_none() {
                    self.forward(cx, event);
                    return;
                }
                let p = cx.local(*pos);
                let hovered = self
                    .rects
                    .iter()
                    .position(|r| r.is_some_and(|r| r.contains(p)))
                    .filter(|&i| self.items[i].is_button() && self.items[i].enabled);
                if hovered != self.hovered {
                    self.hovered = hovered;
                    cx.request_paint();
                }
            }
            Event::PointerUp { pos, .. } => {
                if let Some(i) = self.pressed.take() {
                    cx.release_pointer();
                    cx.request_paint();
                    if self.rects[i].is_some_and(|r| r.contains(cx.local(*pos))) {
                        cx.emit(ToolbarItemClicked(i));
                    }
                    cx.set_handled();
                } else if cx.has_capture() {
                    self.forward(cx, event);
                }
            }
            Event::HoverChanged(false) => {
                if self.hovered.take().is_some() {
                    cx.request_paint();
                }
            }
            Event::KeyDown { .. } | Event::Text(_) | Event::Composition { .. }
                if cx.is_focused() && self.searching =>
            {
                self.forward(cx, event);
            }
            Event::MenuClosed { chosen } => {
                if let Some(&i) = chosen.and_then(|k| self.overflow.get(k)) {
                    cx.emit(ToolbarItemClicked(i));
                }
            }
            Event::ActivateItem(i)
                if self
                    .items
                    .get(*i)
                    .is_some_and(|it| it.is_button() && it.enabled) =>
            {
                cx.emit(ToolbarItemClicked(*i));
            }
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        self.search.is_some()
    }

    fn text_input_area(&self) -> Option<Rect> {
        if !self.searching {
            return None;
        }
        let origin = self.search_origin()?;
        Some(self.search.as_ref()?.edit.input_area(origin))
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Toolbar)
    }

    fn accessibility_items(&self) -> Vec<accesskit::Node> {
        self.items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let role = match item.kind {
                    Kind::Button { .. } => accesskit::Role::Button,
                    Kind::Search { .. } => accesskit::Role::SearchInput,
                    _ => accesskit::Role::GenericContainer,
                };
                let mut node = accesskit::Node::new(role);
                if item.enabled_kind() {
                    node.set_label(item.label.clone());
                }
                if item.is_search() {
                    node.set_value(self.search_text().to_owned());
                }
                if item.is_button() && !item.enabled {
                    node.set_disabled();
                }
                if let Some(r) = self.item_rect(i) {
                    node.set_bounds(accesskit::Rect {
                        x0: f64::from(r.x),
                        y0: f64::from(r.y),
                        x1: f64::from(r.right()),
                        y1: f64::from(r.bottom()),
                    });
                } else {
                    node.set_hidden();
                }
                if item.is_button() {
                    node.add_action(accesskit::Action::Click);
                }
                node
            })
            .collect()
    }

    fn removed(&mut self, text: &mut TextStore) {
        for label in &mut self.labels {
            label.release(text);
        }
        if let Some(search) = &mut self.search {
            search.edit.removed(text);
        }
    }
}

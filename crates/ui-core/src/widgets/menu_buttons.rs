use noa_gpui_layout::style::{Rect as LayoutRect, length};
use noa_gpui_layout::{MeasureInput, Style};
use noa_gpui_scene::{Border, Direction, Point, Rect, Size};
use noa_gpui_text::{TextStore, TextStyle};

use super::{
    ButtonClicked, CachedText, ControlSize, MenuChosen, MenuItem, SelectionChanged, focus_ring,
    pressed_tint,
};
use crate::event::{Event, Key, PointerButton};
use crate::overlay::Placement;
use crate::theme::{Corner, Role};
use crate::widget::{EventCx, MeasureCx, PaintCx, UpdateCx, Widget};

const PAD_X: f32 = 10.0;
const PAD_Y: f32 = 5.0;
const ARROWS: f32 = 22.0;

fn text_style(size: ControlSize) -> TextStyle {
    TextStyle::new(size.font(14.0))
}

fn widest<'a>(
    store: &mut TextStore,
    labels: impl Iterator<Item = &'a String>,
    size: ControlSize,
) -> f32 {
    labels
        .map(|label| {
            let id = store.create(label, text_style(size), None);
            let width = store.size(id).width;
            store.remove(id);
            width
        })
        .fold(0.0, f32::max)
}

fn padding(size: ControlSize) -> Style {
    let k = size.scale();
    Style {
        padding: LayoutRect {
            left: length(PAD_X * k),
            right: length((PAD_X + ARROWS) * k),
            top: length(PAD_Y * k),
            bottom: length(PAD_Y * k),
        },
        flex_shrink: 0.0,
        ..Default::default()
    }
}

fn paint_bezel(cx: &mut PaintCx, pressed: bool) {
    let bounds = cx.bounds();
    let fill = cx.color(Role::Surface);
    let fill = if pressed {
        pressed_tint(cx, fill)
    } else {
        fill
    };
    let outline = cx.color(Role::Outline);
    let radius = bezel_radius(cx);
    cx.bezel(
        bounds,
        radius,
        fill,
        Border {
            width: 1.0,
            color: outline,
        },
    );
}

fn bezel_radius(cx: &PaintCx) -> f32 {
    cx.corner(Corner::Control, 6.0, cx.size().height)
}

fn paint_label(cx: &mut PaintCx, shaped: &CachedText, size: ControlSize) {
    let scale = size.scale();
    if let Some(layout) = shaped.layout() {
        let size = shaped.size();
        let ink = cx.color(Role::Text);
        let y = (cx.size().height - size.height) / 2.0;
        let x = PAD_X * scale;
        cx.text(Rect::new(x, y, size.width, size.height), layout, ink);
    }
}

/// Open `items` below the widget, at least as wide as it.
fn open(cx: &mut EventCx, items: Vec<MenuItem>, checked: Option<usize>) {
    let anchor = cx.window_rect();
    cx.open_menu(items, Placement::Below(anchor), anchor.width, checked);
    cx.set_handled();
}

/// Shows the selected option and opens a menu to change it (`NSPopUpButton`). Emits
/// [`SelectionChanged`].
#[derive(Debug)]
pub struct PopUpButton {
    options: Vec<String>,
    selected: usize,
    shaped: CachedText,
    /// The width of the widest option at a control size (the options never change).
    widest: Option<(ControlSize, f32)>,
    open: bool,
    size: ControlSize,
}

impl PopUpButton {
    pub fn control_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    pub fn new<S: Into<String>>(options: impl IntoIterator<Item = S>) -> Self {
        Self {
            options: options.into_iter().map(Into::into).collect(),
            selected: 0,
            shaped: CachedText::default(),
            widest: None,
            open: false,
            size: ControlSize::Regular,
        }
    }

    pub fn selected_index(mut self, index: usize) -> Self {
        self.selected = index.min(self.options.len().saturating_sub(1));
        self
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn set_selected(&mut self, cx: &mut UpdateCx, index: usize) {
        if index < self.options.len() && index != self.selected {
            self.selected = index;
            cx.request_layout();
        }
    }

    fn select(&mut self, cx: &mut EventCx, index: usize) {
        if index != self.selected && index < self.options.len() {
            self.selected = index;
            cx.emit(SelectionChanged(index));
            cx.request_layout();
        }
    }

    fn open_menu(&mut self, cx: &mut EventCx) {
        let items = self.options.iter().map(MenuItem::new).collect();
        self.open = true;
        cx.request_paint();
        open(cx, items, Some(self.selected));
    }
}

impl Widget for PopUpButton {
    fn style(&self) -> Style {
        padding(self.size)
    }

    fn measures(&self) -> bool {
        true
    }

    /// As wide as the widest option, so the button does not resize as it changes.
    fn measure(&mut self, cx: &mut MeasureCx, _input: MeasureInput) -> Size {
        let label = self.options.get(self.selected).map_or("", String::as_str);
        let text = self
            .shaped
            .shape(cx.text, label, text_style(self.size), None);
        // Measuring runs several times per layout: shape the options once per size.
        let width = match self.widest {
            Some((size, width)) if size == self.size => width,
            _ => {
                let width = widest(cx.text, self.options.iter(), self.size);
                self.widest = Some((self.size, width));
                width
            }
        };
        Size::new(width, text.height)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        paint_bezel(cx, self.open);
        paint_label(cx, &self.shaped, self.size);
        // Up and down chevrons: this is a choice, not an action.
        let size = cx.size();
        let k = self.size.scale();
        let x = size.width - (PAD_X + 12.0) * k;
        let ink = cx.color(Role::Text);
        let mid = size.height / 2.0;
        let (w, h) = (8.0 * k, 4.0 * k);
        cx.chevron(
            Rect::new(x, mid - 1.5 * h, w, h),
            Direction::Up,
            1.5 * k,
            ink,
        );
        cx.chevron(
            Rect::new(x, mid + 0.5 * h, w, h),
            Direction::Down,
            1.5 * k,
            ink,
        );
        if cx.is_focused() {
            focus_ring(cx, cx.bounds(), bezel_radius(cx));
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            Event::PointerDown {
                button: PointerButton::Primary,
                ..
            }
            | Event::KeyDown {
                key: Key::Space, ..
            }
            | Event::Activate => self.open_menu(cx),
            Event::KeyDown { key, .. } if matches!(key, Key::ArrowUp | Key::ArrowDown) => {
                let n = self.options.len();
                let next = if *key == Key::ArrowUp {
                    self.selected.saturating_sub(1)
                } else {
                    (self.selected + 1).min(n.saturating_sub(1))
                };
                self.select(cx, next);
                cx.set_handled();
            }
            Event::MenuClosed { chosen } => {
                self.open = false;
                cx.request_paint();
                if let Some(index) = chosen {
                    self.select(cx, *index);
                }
            }
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::ComboBox)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        if let Some(option) = self.options.get(self.selected) {
            node.set_value(option.clone());
        }
        node.set_has_popup(accesskit::HasPopup::Menu);
        node.set_expanded(self.open);
        node.add_action(accesskit::Action::Click);
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.shaped.release(text);
    }
}

/// A button with a fixed title that opens a menu of actions (`NSPopUpButton` with
/// `pullsDown`). Emits [`MenuChosen`] with the action's index.
#[derive(Debug)]
pub struct PullDownButton {
    title: String,
    items: Vec<MenuItem>,
    shaped: CachedText,
    open: bool,
    size: ControlSize,
}

impl PullDownButton {
    pub fn control_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    pub fn new(title: impl Into<String>, items: Vec<MenuItem>) -> Self {
        Self {
            title: title.into(),
            items,
            shaped: CachedText::default(),
            open: false,
            size: ControlSize::Regular,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }
}

impl Widget for PullDownButton {
    fn style(&self) -> Style {
        padding(self.size)
    }

    fn measures(&self) -> bool {
        true
    }

    fn measure(&mut self, cx: &mut MeasureCx, _input: MeasureInput) -> Size {
        self.shaped
            .shape(cx.text, &self.title, text_style(self.size), None)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        paint_bezel(cx, self.open);
        paint_label(cx, &self.shaped, self.size);
        let size = cx.size();
        let ink = cx.color(Role::Text);
        let k = self.size.scale();
        cx.chevron(
            Rect::new(
                size.width - (PAD_X + 12.0) * k,
                size.height / 2.0 - 2.0 * k,
                8.0 * k,
                4.5 * k,
            ),
            Direction::Down,
            1.6 * k,
            ink,
        );
        if cx.is_focused() {
            focus_ring(cx, cx.bounds(), bezel_radius(cx));
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            Event::PointerDown {
                button: PointerButton::Primary,
                ..
            }
            | Event::KeyDown {
                key: Key::Space | Key::ArrowDown,
                ..
            }
            | Event::Activate => {
                self.open = true;
                cx.request_paint();
                open(cx, self.items.clone(), None);
            }
            Event::MenuClosed { chosen } => {
                self.open = false;
                cx.request_paint();
                if let Some(index) = chosen {
                    cx.emit(MenuChosen(*index));
                }
            }
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Button)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        node.set_label(self.title.clone());
        node.set_has_popup(accesskit::HasPopup::Menu);
        node.set_expanded(self.open);
        node.add_action(accesskit::Action::Click);
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.shaped.release(text);
    }
}

/// A button split in two (`NSComboButton`): the main part performs the primary action
/// ([`ButtonClicked`]); the arrow part opens a menu of related actions ([`MenuChosen`]).
#[derive(Debug)]
pub struct ComboButton {
    title: String,
    items: Vec<MenuItem>,
    shaped: CachedText,
    /// Which part is held down: `Some(false)` main, `Some(true)` arrow.
    pressed: Option<bool>,
    open: bool,
    size: ControlSize,
}

impl ComboButton {
    pub fn control_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    pub fn new(title: impl Into<String>, items: Vec<MenuItem>) -> Self {
        Self {
            title: title.into(),
            items,
            shaped: CachedText::default(),
            pressed: None,
            open: false,
            size: ControlSize::Regular,
        }
    }

    fn in_arrow(&self, cx: &EventCx, pos: Point) -> bool {
        cx.local(pos).x >= cx.size().width - (ARROWS + PAD_X / 2.0) * self.size.scale()
    }
}

impl Widget for ComboButton {
    fn style(&self) -> Style {
        padding(self.size)
    }

    fn measures(&self) -> bool {
        true
    }

    fn measure(&mut self, cx: &mut MeasureCx, _input: MeasureInput) -> Size {
        let text = self
            .shaped
            .shape(cx.text, &self.title, text_style(self.size), None);
        Size::new(text.width + 4.0, text.height)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let split = bounds.width - (ARROWS + PAD_X / 2.0) * self.size.scale();
        paint_bezel(cx, false);
        if let Some(arrow) = self.pressed.or(self.open.then_some(true)) {
            let tint = pressed_tint(cx, cx.color(Role::Surface));
            let rect = if arrow {
                Rect::new(split, 1.0, bounds.width - split - 1.0, bounds.height - 2.0)
            } else {
                Rect::new(1.0, 1.0, split - 1.0, bounds.height - 2.0)
            };
            let radius = cx.corner(Corner::Control, 5.0, rect.height);
            cx.rounded_rect(rect, radius, tint, Border::default());
        }
        let divider = cx.color(Role::Outline);
        cx.fill_rect(Rect::new(split, 4.0, 1.0, bounds.height - 8.0), divider);
        paint_label(cx, &self.shaped, self.size);
        let ink = cx.color(Role::Text);
        cx.chevron(
            Rect::new(
                split + (bounds.width - split - 8.0 * self.size.scale()) / 2.0,
                bounds.height / 2.0 - 2.0 * self.size.scale(),
                8.0 * self.size.scale(),
                4.5 * self.size.scale(),
            ),
            Direction::Down,
            1.6,
            ink,
        );
        if cx.is_focused() {
            focus_ring(cx, bounds, bezel_radius(cx));
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                if self.in_arrow(cx, *pos) {
                    self.open = true;
                    cx.request_paint();
                    open(cx, self.items.clone(), None);
                } else {
                    self.pressed = Some(false);
                    cx.capture_pointer();
                    cx.request_paint();
                    cx.set_handled();
                }
            }
            Event::PointerUp { pos, .. } if self.pressed.is_some() => {
                self.pressed = None;
                cx.release_pointer();
                cx.request_paint();
                if cx.contains(*pos) && !self.in_arrow(cx, *pos) {
                    cx.emit(ButtonClicked);
                }
                cx.set_handled();
            }
            Event::KeyDown {
                key: Key::Space, ..
            }
            | Event::Activate => {
                cx.emit(ButtonClicked);
                cx.set_handled();
            }
            Event::KeyDown {
                key: Key::ArrowDown,
                ..
            } => {
                self.open = true;
                cx.request_paint();
                open(cx, self.items.clone(), None);
            }
            Event::MenuClosed { chosen } => {
                self.open = false;
                cx.request_paint();
                if let Some(index) = chosen {
                    cx.emit(MenuChosen(*index));
                }
            }
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Button)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        node.set_label(self.title.clone());
        node.set_has_popup(accesskit::HasPopup::Menu);
        node.add_action(accesskit::Action::Click);
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.shaped.release(text);
    }
}

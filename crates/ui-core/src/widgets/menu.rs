use ui_layout::{MeasureInput, Style};
use ui_scene::{Border, Direction, Rect, Size};
use ui_text::{TextStore, TextStyle};

use super::CachedText;
use crate::event::{Event, Key};
use crate::overlay::Placement;
use crate::shortcut::Shortcut;
use crate::theme::{Corner, Role};
use crate::widget::{EventCx, MeasureCx, MenuRequest, PaintCx, Widget};

const PAD: f32 = 5.0;
const ROW: f32 = 24.0;
const SEPARATOR: f32 = 9.0;
const CHECK_COLUMN: f32 = 26.0;
const RIGHT_PAD: f32 = 14.0;
const SHORTCUT_GAP: f32 = 28.0;
const CHEVRON: f32 = 16.0;
/// Seconds within which typed letters add to the item search.
const TYPE_SELECT: f64 = 1.0;

/// A submenu closed with Escape: close every menu of the chain.
pub(crate) const DISMISS_ALL: usize = usize::MAX;
/// A menu bar's menu asked for the menu on its left (Left arrow).
pub(crate) const PREVIOUS_MENU: usize = usize::MAX - 1;
/// A menu bar's menu asked for the menu on its right (Right arrow).
pub(crate) const NEXT_MENU: usize = usize::MAX - 2;

#[derive(Debug, Clone, PartialEq)]
pub struct MenuItem {
    label: String,
    enabled: bool,
    checked: bool,
    shortcut: Option<String>,
    key: Option<Shortcut>,
    separator: bool,
    submenu: Vec<MenuItem>,
}

impl MenuItem {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            enabled: true,
            checked: false,
            shortcut: None,
            key: None,
            separator: false,
            submenu: Vec::new(),
        }
    }

    pub fn separator() -> Self {
        Self {
            separator: true,
            enabled: false,
            ..Self::new("")
        }
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    /// Text shown on the right, such as "⌘C". Display only; see [`MenuItem::key`] for a
    /// shortcut that is also matched.
    pub fn shortcut(mut self, hint: impl Into<String>) -> Self {
        self.shortcut = Some(hint.into());
        self
    }

    /// The item's keyboard shortcut. It is shown on the right (as ⌘S or Ctrl+S, see
    /// [`crate::UiTree::set_shortcut_glyphs`]); a [`super::MenuBar`] also chooses the
    /// item when it is pressed. Other menus only show it: register it with
    /// [`crate::UiTree::add_shortcut`].
    pub fn key(mut self, shortcut: Shortcut) -> Self {
        self.key = Some(shortcut);
        self
    }

    /// The chosen item is reported as one index over the whole menu, counting depth
    /// first: an item, then its submenu's items, then the next item. Without submenus
    /// that is the position. [`menu_item`] finds the item for an index and
    /// [`menu_index`] the index for a path.
    pub fn submenu(mut self, items: Vec<MenuItem>) -> Self {
        self.submenu = items;
        self
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn is_separator(&self) -> bool {
        self.separator
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn is_checked(&self) -> bool {
        self.checked
    }

    pub fn shortcut_hint(&self) -> Option<&str> {
        self.shortcut.as_deref()
    }

    pub fn shortcut_key(&self) -> Option<Shortcut> {
        self.key
    }

    pub fn submenu_items(&self) -> &[MenuItem] {
        &self.submenu
    }

    /// This item and everything in its submenus.
    pub fn count(&self) -> usize {
        1 + self.submenu.iter().map(MenuItem::count).sum::<usize>()
    }

    fn selectable(&self) -> bool {
        self.enabled && !self.separator
    }

    fn hint(&self, glyphs: bool) -> Option<String> {
        self.shortcut
            .clone()
            .or_else(|| self.key.map(|k| k.label(glyphs)))
    }
}

/// The item at depth-first `index` over `items` and their submenus (see
/// [`MenuItem::submenu`]).
pub fn menu_item(items: &[MenuItem], index: usize) -> Option<&MenuItem> {
    let mut index = index;
    for item in items {
        if index == 0 {
            return Some(item);
        }
        let count = item.count();
        if index < count {
            return menu_item(&item.submenu, index - 1);
        }
        index -= count;
    }
    None
}

/// The depth-first index of the item at `path` (positions in each submenu).
pub fn menu_index(items: &[MenuItem], path: &[usize]) -> Option<usize> {
    let (&first, rest) = path.split_first()?;
    let item = items.get(first)?;
    let before: usize = items[..first].iter().map(MenuItem::count).sum();
    if rest.is_empty() {
        Some(before)
    } else {
        Some(before + 1 + menu_index(&item.submenu, rest)?)
    }
}

/// A menu shown in an overlay, opened with [`crate::EventCx::open_menu`] or as a context
/// menu ([`crate::Widget::context_menu`]). The opener gets [`Event::MenuClosed`] with
/// the chosen item's depth-first index (see [`MenuItem::submenu`]).
#[derive(Debug)]
pub struct Menu {
    items: Vec<MenuItem>,
    labels: Vec<CachedText>,
    shortcuts: Vec<CachedText>,
    hints: Vec<Option<String>>,
    min_width: f32,
    highlighted: Option<usize>,
    /// Top of each row (layout coordinates) from the last measure.
    tops: Vec<f32>,
    width: f32,
    submenu: bool,
    bar: bool,
    open: Option<usize>,
    /// A submenu is being closed to open another: keep `open`.
    replacing: bool,
    /// Letters typed for the item search, and when the last one was.
    typed: (String, f64),
}

impl Menu {
    pub fn new(items: Vec<MenuItem>, min_width: f32, checked: Option<usize>) -> Self {
        let mut items = items;
        if let Some(item) = checked.and_then(|i| items.get_mut(i)) {
            item.checked = true;
        }
        let n = items.len();
        let hints = items.iter().map(|i| i.hint(false)).collect();
        Self {
            items,
            labels: (0..n).map(|_| CachedText::default()).collect(),
            shortcuts: (0..n).map(|_| CachedText::default()).collect(),
            hints,
            min_width,
            highlighted: checked,
            tops: Vec::new(),
            width: 0.0,
            submenu: false,
            bar: false,
            open: None,
            replacing: false,
            typed: (String::new(), f64::NEG_INFINITY),
        }
    }

    pub(crate) fn opened_as(mut self, highlight: Option<usize>, submenu: bool, bar: bool) -> Self {
        self.highlighted = highlight.filter(|&i| self.items.get(i).is_some_and(|i| i.selectable()));
        self.submenu = submenu;
        self.bar = bar;
        self
    }

    pub(crate) fn shortcut_glyphs(mut self, glyphs: bool) -> Self {
        self.hints = self.items.iter().map(|i| i.hint(glyphs)).collect();
        self
    }

    pub fn items(&self) -> &[MenuItem] {
        &self.items
    }

    pub fn highlighted(&self) -> Option<usize> {
        self.highlighted
    }

    pub fn open_submenu(&self) -> Option<usize> {
        self.open
    }

    pub fn shortcut_text(&self, i: usize) -> Option<&str> {
        self.hints.get(i)?.as_deref()
    }

    fn row_height(item: &MenuItem) -> f32 {
        if item.separator { SEPARATOR } else { ROW }
    }

    fn row_at(&self, y: f32) -> Option<usize> {
        self.tops.iter().enumerate().find_map(|(i, &top)| {
            (y >= top && y < top + Self::row_height(&self.items[i])).then_some(i)
        })
    }

    fn step(&self, forward: bool) -> Option<usize> {
        let n = self.items.len();
        let start = self.highlighted.unwrap_or(if forward { n - 1 } else { 0 });
        (1..=n)
            .map(|k| {
                if forward {
                    (start + k) % n
                } else {
                    (start + n - k % n) % n
                }
            })
            .find(|&i| self.items[i].selectable())
    }

    fn flat(&self, i: usize) -> usize {
        self.items[..i].iter().map(MenuItem::count).sum()
    }

    fn choose(&mut self, cx: &mut EventCx, i: usize, keyboard: bool) {
        if !self.items[i].selectable() {
            return;
        }
        if self.items[i].submenu.is_empty() {
            cx.close_menu(Some(self.flat(i)));
        } else {
            self.open_child(cx, i, keyboard);
        }
    }

    /// Open item `i`'s submenu beside its row; from the keyboard, with its first item
    /// highlighted.
    fn open_child(&mut self, cx: &mut EventCx, i: usize, keyboard: bool) {
        if self.open == Some(i) {
            return;
        }
        if self.open.is_some() {
            self.replacing = true;
            cx.close_child_menus();
        }
        let items = self.items[i].submenu.clone();
        let highlight = keyboard
            .then(|| items.iter().position(MenuItem::selectable))
            .flatten();
        let origin = cx.window_rect();
        let top = self.tops.get(i).copied().unwrap_or(0.0);
        let anchor = Rect::new(origin.x, origin.y + top - PAD, origin.width - 2.0, ROW);
        cx.open_menu_request(MenuRequest {
            items,
            placement: Placement::Beside(anchor),
            min_width: 0.0,
            checked: None,
            highlight,
            submenu: true,
            bar: false,
        });
        self.open = Some(i);
        self.highlighted = Some(i);
        cx.request_paint();
    }

    fn close_child(&mut self, cx: &mut EventCx) {
        if self.open.is_some() {
            cx.close_child_menus();
        }
    }

    /// Close this menu without a choice: one level for Left, all of them for Escape.
    fn dismiss(&mut self, cx: &mut EventCx, all: bool) {
        if self.submenu && all {
            cx.close_menu(Some(DISMISS_ALL));
        } else {
            cx.close_menu(None);
        }
    }

    fn type_select(&mut self, cx: &mut EventCx, input: &str) {
        let now = cx.now();
        if now - self.typed.1 > TYPE_SELECT {
            self.typed.0.clear();
        }
        self.typed.0.push_str(&input.to_lowercase());
        self.typed.1 = now;
        let prefix = self.typed.0.clone();
        // Repeating one letter cycles through the items starting with it; longer text
        // narrows the match from the current item.
        let first = prefix.chars().next().unwrap_or_default();
        let (prefix, start) = if prefix.chars().all(|c| c == first) {
            (first.to_string(), self.highlighted.map_or(0, |h| h + 1))
        } else {
            (prefix, self.highlighted.unwrap_or(0))
        };
        let n = self.items.len();
        if let Some(i) = (0..n).map(|k| (start + k) % n).find(|&i| {
            self.items[i].selectable() && self.items[i].label.to_lowercase().starts_with(&prefix)
        }) {
            self.highlighted = Some(i);
            cx.request_paint();
        }
    }
}

impl Widget for Menu {
    fn style(&self) -> Style {
        Style::default()
    }

    fn measures(&self) -> bool {
        true
    }

    fn measure(&mut self, cx: &mut MeasureCx, _input: MeasureInput) -> Size {
        let style = TextStyle::new(14.0);
        let mut label_width: f32 = 0.0;
        let mut shortcut_width: f32 = 0.0;
        let mut y = PAD;
        let chevron = if self.items.iter().any(|i| !i.submenu.is_empty()) {
            CHEVRON
        } else {
            0.0
        };
        self.tops.clear();
        for (i, item) in self.items.iter().enumerate() {
            self.tops.push(y);
            y += Self::row_height(item);
            if item.separator {
                continue;
            }
            label_width = label_width.max(
                self.labels[i]
                    .shape(cx.text, &item.label, style, None)
                    .width,
            );
            if let Some(hint) = &self.hints[i] {
                shortcut_width =
                    shortcut_width.max(self.shortcuts[i].shape(cx.text, hint, style, None).width);
            }
        }
        let gap = if shortcut_width > 0.0 {
            SHORTCUT_GAP
        } else {
            0.0
        };
        self.width = (CHECK_COLUMN + label_width + gap + shortcut_width + chevron + RIGHT_PAD)
            .max(self.min_width);
        Size::new(self.width, y + PAD)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let fill = cx.color(Role::Surface);
        let border = Border {
            width: 1.0,
            color: cx.color(Role::Outline),
        };
        let radius = cx.corner(Corner::Container, 8.0, bounds.height);
        cx.floating_surface(bounds, radius, fill, border);
        let row_radius = cx.corner(Corner::Inner, 5.0, ROW);
        for (i, item) in self.items.iter().enumerate() {
            let top = self.tops.get(i).copied().unwrap_or(0.0);
            if item.separator {
                let color = cx.color(Role::Separator);
                cx.fill_rect(
                    Rect::new(10.0, top + SEPARATOR / 2.0, bounds.width - 20.0, 1.0),
                    color,
                );
                continue;
            }
            let highlighted = self.highlighted == Some(i) && item.enabled;
            if highlighted {
                let accent = cx.color(Role::Accent);
                cx.rounded_rect(
                    Rect::new(PAD, top, bounds.width - 2.0 * PAD, ROW),
                    row_radius,
                    accent,
                    Border::default(),
                );
            }
            let ink = cx.color(if highlighted {
                Role::OnAccent
            } else if item.enabled {
                Role::Text
            } else {
                Role::Muted
            });
            if item.checked {
                cx.checkmark(Rect::new(10.0, top + 7.0, 10.0, 10.0), 1.8, ink);
            }
            if let Some(layout) = self.labels[i].layout() {
                let size = self.labels[i].size();
                let y = top + (ROW - size.height) / 2.0;
                cx.text(
                    Rect::new(CHECK_COLUMN, y, size.width, size.height),
                    layout,
                    ink,
                );
            }
            if !item.submenu.is_empty() {
                let r = Rect::new(
                    bounds.width - RIGHT_PAD - 4.0,
                    top + ROW / 2.0 - 4.0,
                    4.0,
                    8.0,
                );
                cx.chevron(r, Direction::Right, 1.5, ink);
            } else if let Some(layout) = self.shortcuts[i].layout() {
                let size = self.shortcuts[i].size();
                let y = top + (ROW - size.height) / 2.0;
                let x = bounds.width - RIGHT_PAD - size.width;
                let hint = if highlighted {
                    ink
                } else {
                    cx.color(Role::Muted)
                };
                cx.text(Rect::new(x, y, size.width, size.height), layout, hint);
            }
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            Event::PointerMove { pos } => {
                let row = self
                    .row_at(cx.local(*pos).y)
                    .filter(|&i| self.items[i].selectable());
                if let Some(i) = row {
                    if !self.items[i].submenu.is_empty() {
                        self.open_child(cx, i, false);
                    } else if self.open.is_some() {
                        self.close_child(cx);
                    }
                    if self.highlighted != Some(i) {
                        self.highlighted = Some(i);
                        cx.request_paint();
                    }
                }
                cx.set_handled();
            }
            Event::PointerDown { .. } => cx.set_handled(),
            Event::PointerUp { pos, .. } => {
                if let Some(i) = self.row_at(cx.local(*pos).y) {
                    self.choose(cx, i, false);
                }
                cx.set_handled();
            }
            Event::KeyDown { key, .. } => {
                match key {
                    Key::ArrowDown | Key::ArrowUp => {
                        self.highlighted = self.step(*key == Key::ArrowDown);
                        cx.request_paint();
                    }
                    Key::ArrowRight => match self.highlighted {
                        Some(i) if !self.items[i].submenu.is_empty() => {
                            self.open_child(cx, i, true)
                        }
                        _ if self.bar => cx.close_menu(Some(NEXT_MENU)),
                        _ => {}
                    },
                    Key::ArrowLeft => {
                        if self.submenu {
                            self.dismiss(cx, false);
                        } else if self.bar {
                            cx.close_menu(Some(PREVIOUS_MENU));
                        }
                    }
                    Key::Enter | Key::Space => {
                        if let Some(i) = self.highlighted {
                            self.choose(cx, i, true);
                        }
                    }
                    Key::Escape | Key::Tab => self.dismiss(cx, true),
                    _ => {}
                }
                cx.set_handled();
            }
            Event::Text(input) => {
                self.type_select(cx, input);
                cx.set_handled();
            }
            Event::Activate => {
                if let Some(i) = self.highlighted {
                    self.choose(cx, i, true);
                }
            }
            // Accessibility items leave out separators.
            Event::ActivateItem(n) => {
                if let Some(i) = (0..self.items.len())
                    .filter(|&i| !self.items[i].separator)
                    .nth(*n)
                {
                    self.choose(cx, i, true);
                }
            }
            Event::MenuClosed { chosen } => {
                match *chosen {
                    Some(DISMISS_ALL) => self.dismiss(cx, true),
                    Some(j) => {
                        if let Some(i) = self.open {
                            cx.close_menu(Some(self.flat(i) + 1 + j));
                        }
                    }
                    None if self.replacing => self.replacing = false,
                    None => self.open = None,
                }
                cx.request_paint();
            }
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Menu)
    }

    /// The items, each with its label, checked and disabled state; a Click chooses it.
    fn accessibility_items(&self) -> Vec<accesskit::Node> {
        self.items
            .iter()
            .enumerate()
            .filter(|(_, item)| !item.separator)
            .map(|(i, item)| {
                let role = if item.checked {
                    accesskit::Role::MenuItemCheckBox
                } else {
                    accesskit::Role::MenuItem
                };
                let mut node = accesskit::Node::new(role);
                node.set_label(item.label.clone());
                if item.checked {
                    node.set_toggled(accesskit::Toggled::True);
                }
                if !item.enabled {
                    node.set_disabled();
                }
                if !item.submenu.is_empty() {
                    node.set_has_popup(accesskit::HasPopup::Menu);
                    node.set_expanded(self.open == Some(i));
                }
                if let Some(hint) = &self.hints[i] {
                    node.set_keyboard_shortcut(hint.clone());
                }
                let top = self.tops.get(i).copied().unwrap_or(0.0);
                node.set_bounds(accesskit::Rect {
                    x0: f64::from(PAD),
                    y0: f64::from(top),
                    x1: f64::from(self.width - PAD),
                    y1: f64::from(top + ROW),
                });
                node.add_action(accesskit::Action::Click);
                node
            })
            .collect()
    }

    fn removed(&mut self, text: &mut TextStore) {
        for cached in self.labels.iter_mut().chain(self.shortcuts.iter_mut()) {
            cached.release(text);
        }
    }
}

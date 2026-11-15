use noa_gpui_layout::Style;
use noa_gpui_layout::style::{Size as LayoutSize, length, percent};
use noa_gpui_scene::{Border, Point, Rect};
use noa_gpui_text::{TextStore, TextStyle};

use super::menu::{NEXT_MENU, PREVIOUS_MENU};
use super::{CachedText, MenuChosen, MenuItem, focus_ring, menu_index};
use crate::event::{Event, Key, PointerButton};
use crate::overlay::Placement;
use crate::shortcut::Shortcut;
use crate::theme::{Corner, Role, mix};
use crate::widget::{EventCx, MenuRequest, PaintCx, UpdateCx, Widget};

/// A box whose children get a context menu (`NSView.menu`). Emits [`MenuChosen`] with
/// the chosen item's depth-first index (see [`MenuItem::submenu`]).
#[derive(Debug, Clone)]
pub struct ContextMenuArea {
    style: Style,
    items: Vec<MenuItem>,
}

impl ContextMenuArea {
    pub fn new(style: Style, items: Vec<MenuItem>) -> Self {
        Self { style, items }
    }

    pub fn items(&self) -> &[MenuItem] {
        &self.items
    }

    pub fn set_items(&mut self, _cx: &mut UpdateCx, items: Vec<MenuItem>) {
        self.items = items;
    }
}

impl Widget for ContextMenuArea {
    fn style(&self) -> Style {
        self.style.clone()
    }

    fn paint(&mut self, _cx: &mut PaintCx) {}

    fn context_menu(&self, _local: Point) -> Option<Vec<MenuItem>> {
        Some(self.items.clone())
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        if let Event::MenuClosed { chosen: Some(i) } = event {
            cx.emit(MenuChosen(*i));
        }
    }
}

pub const MENU_BAR: f32 = 28.0;
const TITLE_PAD: f32 = 10.0;

#[derive(Debug, Clone, PartialEq)]
pub struct MenuBarMenu {
    pub title: String,
    pub items: Vec<MenuItem>,
}

impl MenuBarMenu {
    pub fn new(title: impl Into<String>, items: Vec<MenuItem>) -> Self {
        Self {
            title: title.into(),
            items,
        }
    }
}

/// Emitted when an item of a [`MenuBar`] is chosen, from its menu or with its shortcut:
/// `item` is its depth-first index in menu `menu` (see [`MenuItem::submenu`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuBarChosen {
    pub menu: usize,
    pub item: usize,
}

/// An in-window menu bar for platforms without a global one (Windows, Linux, the web).
/// On macOS the same [`MenuBarMenu`]s go to the system menu bar and this widget is
/// hidden. Emits [`MenuBarChosen`].
#[derive(Debug)]
pub struct MenuBar {
    menus: Vec<MenuBarMenu>,
    titles: Vec<CachedText>,
    rects: Vec<Rect>,
    open: Option<usize>,
    /// A menu is being closed to open another: keep `open`.
    switching: bool,
    current: usize,
    keys: Vec<Shortcut>,
    /// For each of `keys`: the menu and depth-first item it chooses.
    targets: Vec<(usize, usize)>,
}

impl MenuBar {
    pub fn new(menus: Vec<MenuBarMenu>) -> Self {
        let mut bar = Self {
            titles: Vec::new(),
            rects: Vec::new(),
            menus: Vec::new(),
            open: None,
            switching: false,
            current: 0,
            keys: Vec::new(),
            targets: Vec::new(),
        };
        bar.replace(menus);
        bar
    }

    pub fn menus(&self) -> &[MenuBarMenu] {
        &self.menus
    }

    pub fn open_menu(&self) -> Option<usize> {
        self.open
    }

    /// Where title `i` is (widget coordinates), after the last paint.
    pub fn title_rect(&self, i: usize) -> Option<Rect> {
        self.rects.get(i).copied()
    }

    /// An open menu shows the change the next time it opens.
    pub fn set_menus(&mut self, cx: &mut UpdateCx, menus: Vec<MenuBarMenu>) {
        self.replace(menus);
        cx.request_paint();
    }

    fn replace(&mut self, menus: Vec<MenuBarMenu>) {
        fn collect(
            items: &[MenuItem],
            path: &mut Vec<usize>,
            menu: usize,
            all: &[MenuItem],
            out: &mut Vec<(Shortcut, usize, usize)>,
        ) {
            for (i, item) in items.iter().enumerate() {
                if !item.is_enabled() {
                    continue;
                }
                path.push(i);
                if let Some(key) = item.shortcut_key()
                    && let Some(index) = menu_index(all, path)
                {
                    out.push((key, menu, index));
                }
                collect(item.submenu_items(), path, menu, all, out);
                path.pop();
            }
        }
        let mut found = Vec::new();
        for (m, menu) in menus.iter().enumerate() {
            collect(&menu.items, &mut Vec::new(), m, &menu.items, &mut found);
        }
        self.keys = found.iter().map(|&(k, _, _)| k).collect();
        self.targets = found.iter().map(|&(_, m, i)| (m, i)).collect();
        if self.titles.len() < menus.len() {
            self.titles.resize_with(menus.len(), CachedText::default);
        }
        self.menus = menus;
        self.current = self.current.min(self.menus.len().saturating_sub(1));
    }

    fn open(&mut self, cx: &mut EventCx, index: usize, keyboard: bool) {
        let Some(rect) = self.rects.get(index).copied() else {
            return;
        };
        let items = self.menus[index].items.clone();
        let highlight = keyboard
            .then(|| {
                items
                    .iter()
                    .position(|i| i.is_enabled() && !i.is_separator())
            })
            .flatten();
        let origin = cx.window_rect();
        let anchor = Rect::new(
            origin.x + rect.x,
            origin.y + rect.y,
            rect.width,
            rect.height,
        );
        cx.open_menu_request(MenuRequest {
            items,
            placement: Placement::Below(anchor),
            min_width: 0.0,
            checked: None,
            highlight,
            submenu: false,
            bar: true,
        });
        self.open = Some(index);
        self.current = index;
        cx.request_paint();
    }

    fn switch_to(&mut self, cx: &mut EventCx, index: usize) {
        if self.open == Some(index) {
            return;
        }
        if self.open.is_some() {
            self.switching = true;
            cx.close_child_menus();
        }
        self.open(cx, index, false);
    }

    fn title_at(&self, p: Point) -> Option<usize> {
        self.rects.iter().position(|r| r.contains(p))
    }
}

impl Widget for MenuBar {
    fn style(&self) -> Style {
        Style {
            size: LayoutSize {
                width: percent(1.0),
                height: length(MENU_BAR),
            },
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let surface = cx.color(Role::Surface);
        cx.fill_rect(bounds, surface);
        let line = cx.color(Role::Separator);
        cx.fill_rect(Rect::new(0.0, bounds.height - 1.0, bounds.width, 1.0), line);
        let ink = cx.color(Role::Text);
        let focused = cx.is_focused();
        let style = TextStyle::new(13.0);
        let mut x = 6.0;
        self.rects.clear();
        for i in 0..self.menus.len() {
            let title = self.menus[i].title.clone();
            let size = self.titles[i].shape(cx.text_store(), &title, style, None);
            let rect = Rect::new(x, 3.0, size.width + 2.0 * TITLE_PAD, MENU_BAR - 6.0);
            let open = self.open == Some(i);
            let radius = cx.corner(Corner::Inner, 5.0, rect.height);
            if open {
                let accent = cx.color(Role::Accent);
                cx.bezel(rect, radius, accent, Border::default());
            } else if focused && self.current == i {
                cx.rounded_rect(rect, radius, mix(surface, ink, 0.1), Border::default());
            }
            if let Some(layout) = self.titles[i].layout() {
                let color = cx.color(if open { Role::OnAccent } else { Role::Text });
                cx.text(
                    Rect::new(
                        rect.x + TITLE_PAD,
                        rect.y + (rect.height - size.height) / 2.0,
                        size.width,
                        size.height,
                    ),
                    layout,
                    color,
                );
            }
            self.rects.push(rect);
            x = rect.right();
        }
        if focused
            && self.open.is_none()
            && let Some(&rect) = self.rects.get(self.current)
        {
            focus_ring(cx, rect, cx.corner(Corner::Inner, 5.0, rect.height));
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                if let Some(i) = self.title_at(cx.local(*pos)) {
                    if self.open == Some(i) {
                        cx.close_child_menus();
                    } else {
                        self.switch_to(cx, i);
                    }
                }
                cx.set_handled();
            }
            Event::PointerMove { pos } => {
                if self.open.is_some()
                    && let Some(i) = self.title_at(cx.local(*pos))
                {
                    self.switch_to(cx, i);
                }
            }
            Event::KeyDown { key, .. } if cx.is_focused() && !self.menus.is_empty() => {
                let n = self.menus.len();
                match key {
                    Key::ArrowLeft => self.current = (self.current + n - 1) % n,
                    Key::ArrowRight => self.current = (self.current + 1) % n,
                    Key::ArrowDown | Key::Enter | Key::Space => {
                        let current = self.current;
                        self.open(cx, current, true);
                    }
                    _ => return,
                }
                cx.request_paint();
                cx.set_handled();
            }
            Event::MenuClosed { chosen } => {
                let n = self.menus.len();
                let from = self.open.take();
                match (*chosen, from) {
                    (None, _) if self.switching => {
                        self.switching = false;
                        self.open = from;
                    }
                    (Some(PREVIOUS_MENU), Some(i)) => self.open(cx, (i + n - 1) % n, true),
                    (Some(NEXT_MENU), Some(i)) => self.open(cx, (i + 1) % n, true),
                    (Some(item), Some(menu)) => cx.emit(MenuBarChosen { menu, item }),
                    _ => {}
                }
                cx.request_paint();
            }
            Event::KeyEquivalent(shortcut) => {
                if let Some(k) = self.keys.iter().position(|k| k == shortcut) {
                    let (menu, item) = self.targets[k];
                    cx.emit(MenuBarChosen { menu, item });
                }
            }
            Event::ActivateItem(i) if *i < self.menus.len() => self.switch_to(cx, *i),
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn key_equivalents(&self) -> &[Shortcut] {
        &self.keys
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::MenuBar)
    }

    fn accessibility_items(&self) -> Vec<accesskit::Node> {
        self.menus
            .iter()
            .enumerate()
            .map(|(i, menu)| {
                let mut node = accesskit::Node::new(accesskit::Role::MenuItem);
                node.set_label(menu.title.clone());
                node.set_has_popup(accesskit::HasPopup::Menu);
                node.set_expanded(self.open == Some(i));
                if let Some(r) = self.rects.get(i) {
                    node.set_bounds(accesskit::Rect {
                        x0: f64::from(r.x),
                        y0: f64::from(r.y),
                        x1: f64::from(r.right()),
                        y1: f64::from(r.bottom()),
                    });
                }
                node.add_action(accesskit::Action::Click);
                node
            })
            .collect()
    }

    fn removed(&mut self, text: &mut TextStore) {
        for title in &mut self.titles {
            title.release(text);
        }
    }
}

//! The app switcher opens with ⌥Tab (Ctrl+Tab or Alt+Tab elsewhere). Tab and Shift+Tab
//! move along it while the modifier is held; releasing the modifier brings the chosen
//! window forward.
//!
//! On macOS the system's own switcher takes ⌘Tab before the app sees it, and Tab moves
//! the focus inside the tree, so the Mini OS screen passes keys to [`Switcher::event`]
//! before the tree.

use noa_gpui_core::style::{Size as LayoutSize, length};
use noa_gpui_core::widgets::{InnerWindow, WindowManager};
use noa_gpui_core::{
    Event, EventCx, Key, Modifiers, OverlayOptions, PaintCx, Placement, PointerButton, Role, Style,
    UiTree, Widget, WidgetId,
};
use noa_gpui_scene::{Border, Color, Point, Rect, TextLayoutId};
use noa_gpui_text::{TextStore, TextStyle};

const TILE: f32 = 104.0;
const TILE_H: f32 = 108.0;
const ICON: f32 = 60.0;
const GAP: f32 = 6.0;
const PAD: f32 = 14.0;

/// Emitted by the switcher when a tile is clicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwitcherPicked(pub usize);

#[derive(Debug, Clone)]
pub struct SwitcherItem {
    pub window: WidgetId,
    pub title: String,
    pub color: Color,
}

/// Hovering a tile chooses it; clicking emits [`SwitcherPicked`].
#[derive(Debug)]
pub struct SwitcherView {
    items: Vec<SwitcherItem>,
    selected: usize,
    titles: Vec<Option<TextLayoutId>>,
    initials: Vec<Option<TextLayoutId>>,
    /// Layouts of titles shaped in another weight, released at the next paint.
    stale: Vec<Option<TextLayoutId>>,
}

impl SwitcherView {
    fn new(items: Vec<SwitcherItem>, selected: usize) -> Self {
        let n = items.len();
        Self {
            items,
            selected,
            titles: vec![None; n],
            initials: vec![None; n],
            stale: Vec::new(),
        }
    }

    #[cfg(test)]
    pub fn selected(&self) -> usize {
        self.selected
    }

    #[cfg(test)]
    pub fn items(&self) -> &[SwitcherItem] {
        &self.items
    }

    fn select(&mut self, index: usize) -> bool {
        let changed = index != self.selected && index < self.items.len();
        if changed {
            // Their titles change weight, so shape them again at the next paint.
            self.stale
                .extend([self.titles[self.selected].take(), self.titles[index].take()]);
            self.selected = index;
        }
        changed
    }

    fn size(n: usize) -> noa_gpui_scene::Size {
        let n = n as f32;
        noa_gpui_scene::Size::new(
            2.0 * PAD + n * TILE + (n - 1.0).max(0.0) * GAP,
            2.0 * PAD + TILE_H,
        )
    }

    /// The layout of the title under tile `i`, once painted.
    #[cfg(test)]
    pub fn title_layout(&self, i: usize) -> Option<TextLayoutId> {
        self.titles.get(i).copied().flatten()
    }

    fn tile(&self, i: usize) -> Rect {
        Rect::new(PAD + i as f32 * (TILE + GAP), PAD, TILE, TILE_H)
    }

    fn tile_at(&self, p: Point) -> Option<usize> {
        (0..self.items.len()).find(|&i| self.tile(i).contains(p))
    }
}

fn shaped(
    slot: &mut Option<TextLayoutId>,
    store: &mut TextStore,
    text: &str,
    style: TextStyle,
) -> (TextLayoutId, noa_gpui_scene::Size) {
    let id = *slot.get_or_insert_with(|| store.create(text, style, None));
    (id, store.size(id))
}

impl Widget for SwitcherView {
    fn style(&self) -> Style {
        let size = Self::size(self.items.len());
        Style {
            size: LayoutSize {
                width: length(size.width),
                height: length(size.height),
            },
            ..Default::default()
        }
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        for id in self.stale.drain(..).flatten() {
            cx.text_store().remove(id);
        }
        let bounds = cx.bounds();
        let fill = cx.color(Role::Surface);
        let border = Border {
            width: 1.0,
            color: cx.color(Role::Outline),
        };
        cx.floating_surface(bounds, 18.0, fill, border);
        let ink = cx.color(Role::Text);
        let tint = cx.color(Role::Text).with_alpha(0.12);
        for i in 0..self.items.len() {
            let tile = self.tile(i);
            if i == self.selected {
                cx.rounded_rect(tile, 12.0, tint, Border::default());
            }
            let icon = Rect::new(tile.x + (TILE - ICON) / 2.0, tile.y + 10.0, ICON, ICON);
            let color = self.items[i].color;
            cx.rounded_rect(icon, 14.0, color, Border::default());
            let initial: String = self.items[i].title.chars().take(1).collect();
            let style = TextStyle::new(30.0).weight(700);
            let (id, size) = shaped(&mut self.initials[i], cx.text_store(), &initial, style);
            let at = Rect::new(
                icon.x + (ICON - size.width) / 2.0,
                icon.y + (ICON - size.height) / 2.0,
                size.width,
                size.height,
            );
            cx.text(at, id, Color::WHITE);
            let style = TextStyle::new(12.0).weight(if i == self.selected { 600 } else { 400 });
            let title = self.items[i].title.clone();
            let (id, size) = shaped(&mut self.titles[i], cx.text_store(), &title, style);
            let y = icon.bottom() + 8.0;
            let x = tile.x + ((TILE - size.width) / 2.0).max(4.0);
            cx.push_clip(Rect::new(tile.x + 4.0, y, TILE - 8.0, size.height));
            cx.text(Rect::new(x, y, size.width, size.height), id, ink);
            cx.pop_clip();
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            Event::PointerMove { pos } => {
                if let Some(i) = self.tile_at(cx.local(*pos))
                    && self.select(i)
                {
                    cx.request_paint();
                }
            }
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                if let Some(i) = self.tile_at(cx.local(*pos)) {
                    self.select(i);
                    cx.emit(SwitcherPicked(i));
                }
                cx.set_handled();
            }
            _ => {}
        }
    }

    fn accessibility_role(&self) -> Option<noa_gpui_core::accesskit::Role> {
        Some(noa_gpui_core::accesskit::Role::ListBox)
    }

    fn accessibility(&self, node: &mut noa_gpui_core::accesskit::Node) {
        node.set_label("Windows");
        if let Some(item) = self.items.get(self.selected) {
            node.set_value(item.title.clone());
        }
    }

    fn removed(&mut self, text: &mut TextStore) {
        let layouts = self.titles.iter_mut().chain(&mut self.initials);
        for id in layouts.chain(&mut self.stale) {
            if let Some(id) = id.take() {
                text.remove(id);
            }
        }
    }
}

/// Which held modifier opened the switcher; letting go of it chooses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hold {
    Command,
    Alt,
    Ctrl,
}

impl Hold {
    fn of(modifiers: Modifiers) -> Option<Self> {
        if modifiers.alt {
            Some(Hold::Alt)
        } else if modifiers.command {
            Some(Hold::Command)
        } else if modifiers.ctrl {
            Some(Hold::Ctrl)
        } else {
            None
        }
    }

    fn held(self, modifiers: Modifiers) -> bool {
        match self {
            Hold::Command => modifiers.command,
            Hold::Alt => modifiers.alt,
            Hold::Ctrl => modifiers.ctrl,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchStep {
    /// Not the switcher's; dispatch it to the tree as usual.
    Pass,
    Taken,
    /// The switcher closed and brought this window forward.
    Switched(WidgetId),
}

#[derive(Debug, Default)]
pub struct Switcher {
    open: Option<(WidgetId, Option<Hold>)>,
}

impl Switcher {
    #[cfg(test)]
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    pub fn view(&self) -> Option<WidgetId> {
        self.open.map(|(view, _)| view)
    }

    /// Opens over the desktop with the windows from front to back and the second one
    /// chosen, so one Tab returns to the previous window. `modifiers` are those held:
    /// releasing them chooses. With none held it stays open until Return or a click.
    /// Returns whether it opened; there must be a window.
    pub fn open(
        &mut self,
        tree: &mut UiTree,
        manager: &WindowManager,
        color: impl Fn(WidgetId) -> Color,
        modifiers: Modifiers,
        backwards: bool,
    ) -> bool {
        if self.open.is_some() {
            return true;
        }
        // Front to back; minimized windows (at the bottom of the stack) come last.
        let items: Vec<SwitcherItem> = manager
            .windows()
            .into_iter()
            .rev()
            .filter_map(|window| {
                let title = tree.get::<InnerWindow>(window)?.title().to_owned();
                Some(SwitcherItem {
                    window,
                    title,
                    color: color(window),
                })
            })
            .collect();
        if items.is_empty() {
            return false;
        }
        let n = items.len();
        let selected = if backwards { n - 1 } else { 1.min(n - 1) };
        let desktop = tree.window_rect(manager.desktop()).unwrap_or_default();
        let size = SwitcherView::size(n);
        let at = Point::new(
            desktop.x + (desktop.width - size.width) / 2.0,
            desktop.y + (desktop.height - size.height) / 2.0,
        );
        let view = tree.open_overlay(
            SwitcherView::new(items, selected),
            Placement::At(at),
            OverlayOptions::POPOVER,
        );
        self.open = Some((view, Hold::of(modifiers)));
        true
    }

    /// Wraps around.
    fn step(&mut self, tree: &mut UiTree, forward: bool) {
        let Some((view, _)) = self.open else { return };
        tree.update::<SwitcherView, _>(view, |v, cx| {
            let n = v.items.len();
            let next = if forward {
                (v.selected + 1) % n
            } else {
                (v.selected + n - 1) % n
            };
            if v.select(next) {
                cx.request_paint();
            }
        });
    }

    fn close(&mut self, tree: &mut UiTree, text: &mut TextStore) {
        if let Some((view, _)) = self.open.take() {
            tree.close_overlay(view, text);
        }
    }

    pub fn choose(
        &mut self,
        tree: &mut UiTree,
        text: &mut TextStore,
        manager: &mut WindowManager,
    ) -> SwitchStep {
        let Some((view, _)) = self.open else {
            return SwitchStep::Pass;
        };
        let window = tree
            .get::<SwitcherView>(view)
            .and_then(|v| v.items.get(v.selected).map(|i| i.window));
        self.close(tree, text);
        match window {
            Some(window) => {
                manager.activate(tree, window);
                SwitchStep::Switched(window)
            }
            None => SwitchStep::Taken,
        }
    }

    /// Sees an event before the tree: opens on ⌥Tab / Ctrl+Tab / ⌘Tab, and while open
    /// takes the keys and chooses when the held modifier is released.
    pub fn event(
        &mut self,
        tree: &mut UiTree,
        text: &mut TextStore,
        manager: &mut WindowManager,
        color: impl Fn(WidgetId) -> Color,
        event: &Event,
    ) -> SwitchStep {
        let Some((_, hold)) = self.open else {
            if let Event::KeyDown {
                key: Key::Tab,
                modifiers,
            } = event
                && Hold::of(*modifiers).is_some()
                && self.open(tree, manager, color, *modifiers, modifiers.shift)
            {
                return SwitchStep::Taken;
            }
            return SwitchStep::Pass;
        };
        match event {
            Event::KeyDown { key, modifiers } => {
                match key {
                    Key::Tab => self.step(tree, !modifiers.shift),
                    Key::ArrowRight => self.step(tree, true),
                    Key::ArrowLeft => self.step(tree, false),
                    Key::Enter | Key::Space => return self.choose(tree, text, manager),
                    Key::Escape => self.close(tree, text),
                    _ => {}
                }
                SwitchStep::Taken
            }
            Event::Text(_) | Event::Composition { .. } => SwitchStep::Taken,
            Event::ModifiersChanged(modifiers) => {
                if hold.is_some_and(|h| !h.held(*modifiers)) {
                    // Keep the tree's modifier state correct, then choose.
                    tree.dispatch(event.clone(), text);
                    return self.choose(tree, text, manager);
                }
                SwitchStep::Pass
            }
            _ => SwitchStep::Pass,
        }
    }

    /// Handles the overlay closing by itself on a press outside it.
    pub fn dismissed(&mut self, view: WidgetId) -> bool {
        if self.view() == Some(view) {
            self.open = None;
            return true;
        }
        false
    }
}

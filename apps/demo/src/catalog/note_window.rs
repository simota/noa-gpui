use noa_gpui_core::style::{FlexDirection, Rect as LayoutRect, Size as LayoutSize, percent};
use noa_gpui_core::widgets::{Button, ButtonClicked, Container, Label, TextArea};
use noa_gpui_core::{Message, Role, Style, Theme, UiTree, WidgetId};
use noa_gpui_text::TextStore;

use super::kit::gap;
use noa_gpui_platform::Screen;

pub struct NoteWindow {
    tree: UiTree,
    close: WidgetId,
    closing: bool,
}

impl NoteWindow {
    pub fn new(number: u32, theme: Theme) -> Self {
        let mut tree = UiTree::new();
        tree.set_theme(theme);
        let root = tree.insert(
            None,
            Container::new(Style {
                flex_direction: FlexDirection::Column,
                gap: gap(0.0, 10.0),
                padding: LayoutRect::length(16.0),
                size: LayoutSize {
                    width: percent(1.0),
                    height: percent(1.0),
                },
                ..Default::default()
            })
            .background(Role::Background),
        );
        tree.insert(
            Some(root),
            Label::new(format!("Note {number}"))
                .font_size(20.0)
                .weight(700),
        );
        tree.insert(
            Some(root),
            Label::new("This window has its own surface, renderer and widget tree; the GPU device and fonts are shared.")
                .color(Role::Muted)
                .wrap(true),
        );
        let area = tree.insert(Some(root), TextArea::new("Write something…").min_lines(5));
        tree.set_accessible_label(area, format!("Note {number}"));
        let close = tree.insert(Some(root), Button::new("Close window"));
        tree.set_focus(Some(area));
        Self {
            tree,
            close,
            closing: false,
        }
    }
}

impl Screen for NoteWindow {
    fn tree(&self) -> &UiTree {
        &self.tree
    }

    fn tree_mut(&mut self) -> &mut UiTree {
        &mut self.tree
    }

    fn handle(&mut self, message: Message, _text: &mut TextStore) {
        if message.source == self.close && message.downcast_ref::<ButtonClicked>().is_some() {
            self.closing = true;
        }
    }

    fn wants_close(&self) -> bool {
        self.closing
    }
}

/// Opened with [`noa_gpui_platform::WindowRequest::floating`] so it stays above the other
/// windows.
pub struct PanelWindow {
    tree: UiTree,
}

impl PanelWindow {
    pub fn new(theme: Theme) -> Self {
        use noa_gpui_core::widgets::{Slider, Switch};
        let mut tree = UiTree::new();
        tree.set_theme(theme);
        let root = tree.insert(
            None,
            Container::new(Style {
                flex_direction: FlexDirection::Column,
                gap: gap(0.0, 8.0),
                padding: LayoutRect::length(14.0),
                size: LayoutSize {
                    width: percent(1.0),
                    height: percent(1.0),
                },
                ..Default::default()
            })
            .background(Role::Surface),
        );
        tree.insert(Some(root), Label::new("Opacity").font_size(12.0));
        let opacity = tree.insert(Some(root), Slider::new(0.0, 100.0, 80.0));
        tree.set_accessible_label(opacity, "Opacity");
        tree.insert(Some(root), Label::new("Line width").font_size(12.0));
        let line_width = tree.insert(Some(root), Slider::new(1.0, 12.0, 2.0));
        tree.set_accessible_label(line_width, "Line width");
        let row = tree.insert(Some(root), Container::new(super::kit::row_style(8.0)));
        let snap = tree.insert(Some(row), Switch::new(true));
        tree.set_accessible_label(snap, "Snap to grid");
        tree.insert(Some(row), Label::new("Snap to grid").font_size(12.0));
        Self { tree }
    }
}

impl Screen for PanelWindow {
    fn tree(&self) -> &UiTree {
        &self.tree
    }

    fn tree_mut(&mut self) -> &mut UiTree {
        &mut self.tree
    }

    fn handle(&mut self, _message: Message, _text: &mut TextStore) {}
}

/// Content dragged out of the catalog's desktop into its own OS window. "Return to
/// Desktop" gives it back.
pub struct TornWindow {
    tree: UiTree,
    title: String,
    content: WidgetId,
    back: WidgetId,
    detached: Option<noa_gpui_platform::Detached>,
    closing: bool,
}

impl TornWindow {
    pub fn new(detached: noa_gpui_platform::Detached, theme: Theme) -> Self {
        let mut tree = UiTree::new();
        tree.set_theme(theme);
        let root = tree.insert(
            None,
            Container::new(Style {
                flex_direction: FlexDirection::Column,
                size: LayoutSize {
                    width: percent(1.0),
                    height: percent(1.0),
                },
                ..Default::default()
            })
            .background(Role::Surface),
        );
        let content = tree.insert(
            Some(root),
            Container::new(Style {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                ..Default::default()
            }),
        );
        for subtree in detached.content {
            tree.insert_subtree(Some(content), subtree);
        }
        let bar = tree.insert(
            Some(root),
            Container::new(Style {
                padding: LayoutRect::length(10.0),
                flex_shrink: 0.0,
                ..Default::default()
            }),
        );
        let back = tree.insert(Some(bar), Button::new("Return to Desktop"));
        Self {
            tree,
            title: detached.title,
            content,
            back,
            detached: None,
            closing: false,
        }
    }

    fn give_back(&mut self) {
        if self.closing {
            return;
        }
        let children: Vec<WidgetId> = self.tree.children(self.content).to_vec();
        let content = children
            .into_iter()
            .filter_map(|c| self.tree.take_subtree(c))
            .collect();
        self.detached = Some(noa_gpui_platform::Detached {
            title: self.title.clone(),
            content,
        });
        self.closing = true;
    }
}

impl Screen for TornWindow {
    fn tree(&self) -> &UiTree {
        &self.tree
    }

    fn tree_mut(&mut self) -> &mut UiTree {
        &mut self.tree
    }

    fn handle(&mut self, message: Message, _text: &mut TextStore) {
        if message.source == self.back && message.downcast_ref::<ButtonClicked>().is_some() {
            self.give_back();
        }
    }

    /// Closing the OS window also gives the content back instead of losing it.
    fn close_requested(&mut self) {
        self.give_back();
    }

    fn take_detached(&mut self) -> Option<noa_gpui_platform::Detached> {
        self.detached.take()
    }

    fn wants_close(&self) -> bool {
        self.closing
    }
}

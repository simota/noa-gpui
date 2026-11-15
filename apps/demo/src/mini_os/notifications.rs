use noa_gpui_core::style::{
    AlignItems, FlexDirection, Rect as LayoutRect, Size as LayoutSize, auto, length, percent,
};
use noa_gpui_core::widgets::{
    Button, ButtonClicked, ButtonStyle, Container, Label, ScrollView, Toast, ToastAction,
    ToastClosed, Truncation,
};
use noa_gpui_core::{
    Message, OverlayDismissed, OverlayOptions, Placement, Role, Style, UiTree, WidgetId,
    WindowCorner,
};
use noa_gpui_scene::Rect;
use noa_gpui_text::TextStore;

pub const PANEL_WIDTH: f32 = 320.0;
/// Space between the panel and the desktop's edges.
const INSET: f32 = 8.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    pub app: String,
    pub title: String,
    pub body: String,
    /// Arrival time ([`UiTree::now`]).
    pub at: f64,
    pub read: bool,
}

#[derive(Debug)]
struct Panel {
    root: WidgetId,
    clear: WidgetId,
    list: WidgetId,
    /// The × button of each notification, in the order of `items`.
    removes: Vec<WidgetId>,
    fitted: Rect,
}

#[derive(Debug, Default)]
pub struct NotificationCenter {
    /// Oldest first.
    items: Vec<Notification>,
    panel: Option<Panel>,
    toasts: Vec<(WidgetId, usize)>,
    pressing: bool,
    /// The press now held closed the panel, as a press outside it.
    closed_by_press: bool,
}

impl NotificationCenter {
    pub fn items(&self) -> &[Notification] {
        &self.items
    }

    pub fn unread(&self) -> usize {
        self.items.iter().filter(|n| !n.read).count()
    }

    #[cfg(test)]
    pub fn is_open(&self) -> bool {
        self.panel.is_some()
    }

    #[cfg(test)]
    pub fn panel(&self) -> Option<WidgetId> {
        self.panel.as_ref().map(|p| p.root)
    }

    #[cfg(test)]
    pub fn clear_button(&self) -> Option<WidgetId> {
        self.panel.as_ref().map(|p| p.clear)
    }

    #[cfg(test)]
    pub fn remove_button(&self, index: usize) -> Option<WidgetId> {
        self.panel.as_ref()?.removes.get(index).copied()
    }

    #[cfg(test)]
    pub fn toasts(&self) -> impl Iterator<Item = WidgetId> + '_ {
        self.toasts.iter().map(|&(t, _)| t)
    }

    /// With the panel closed, the notification also shows as a toast (whose Show button
    /// opens the panel) and counts as unread. With the panel open it appears there as
    /// read, and `text` is needed to replace the panel's rows.
    pub fn post(
        &mut self,
        tree: &mut UiTree,
        text: Option<&mut TextStore>,
        app: &str,
        title: &str,
        body: &str,
    ) {
        let open = self.panel.is_some() && text.is_some();
        self.items.push(Notification {
            app: app.to_owned(),
            title: title.to_owned(),
            body: body.to_owned(),
            at: tree.now(),
            read: open,
        });
        if let Some(text) = text.filter(|_| open) {
            self.rebuild(tree, text);
        } else {
            let toast = Toast::new(format!("{title} — {body}"))
                .action("Show")
                .show_at(tree, WindowCorner::TopRight);
            self.toasts.push((toast, self.items.len() - 1));
        }
    }

    /// Opens the panel against the right edge of `desktop` (window coordinates), or
    /// closes it.
    pub fn toggle(&mut self, tree: &mut UiTree, text: &mut TextStore, desktop: Rect) {
        if std::mem::take(&mut self.closed_by_press) {
            // The press of this click was outside the panel and has closed it already.
        } else if self.panel.is_some() {
            self.close(tree, text);
        } else {
            self.open(tree, text, desktop);
        }
    }

    /// The panel closes on a press outside it, and the click on the button that toggles
    /// it is such a press, so [`NotificationCenter::toggle`] must not open it again for
    /// the same press.
    pub fn pointer(&mut self, pressed: bool) {
        self.pressing = pressed;
        self.closed_by_press = false;
    }

    /// Opening marks every notification read and removes the toasts, since the panel
    /// shows what they showed.
    pub fn open(&mut self, tree: &mut UiTree, text: &mut TextStore, desktop: Rect) {
        if self.panel.is_some() {
            return;
        }
        for item in &mut self.items {
            item.read = true;
        }
        for (toast, _) in self.toasts.drain(..) {
            tree.update::<Toast, _>(toast, |t, cx| t.dismiss(cx));
        }
        let root = tree.open_overlay(
            Container::new(panel_style(desktop))
                .background(Role::Surface)
                .border(1.0, Role::Outline)
                .radius(12.0),
            placement(desktop),
            OverlayOptions::POPOVER,
        );
        let header = tree.insert(
            Some(root),
            Container::new(Style {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::CENTER,
                flex_shrink: 0.0,
                ..Default::default()
            }),
        );
        tree.insert(
            Some(header),
            Label::new("Notifications").font_size(15.0).weight(700),
        );
        tree.insert(
            Some(header),
            Container::new(Style {
                flex_grow: 1.0,
                ..Default::default()
            }),
        );
        let clear = tree.insert(
            Some(header),
            Button::new("Clear All").style(ButtonStyle::Inline),
        );
        let scroll = tree.insert(
            Some(root),
            ScrollView::new(Style {
                flex_grow: 1.0,
                flex_basis: length(0.0),
                min_size: LayoutSize {
                    width: length(0.0),
                    height: length(0.0),
                },
                ..Default::default()
            }),
        );
        let list = tree.insert(
            Some(scroll),
            Container::new(Style {
                flex_direction: FlexDirection::Column,
                gap: LayoutSize {
                    width: length(0.0),
                    height: length(8.0),
                },
                size: LayoutSize {
                    width: percent(1.0),
                    height: auto(),
                },
                flex_shrink: 0.0,
                ..Default::default()
            }),
        );
        self.panel = Some(Panel {
            root,
            clear,
            list,
            removes: Vec::new(),
            fitted: desktop,
        });
        self.rebuild(tree, text);
    }

    pub fn close(&mut self, tree: &mut UiTree, text: &mut TextStore) {
        if let Some(panel) = self.panel.take() {
            tree.close_overlay(panel.root, text);
        }
    }

    pub fn clear(&mut self, tree: &mut UiTree, text: &mut TextStore) {
        self.items.clear();
        for (toast, _) in self.toasts.drain(..) {
            tree.update::<Toast, _>(toast, |t, cx| t.dismiss(cx));
        }
        self.rebuild(tree, text);
    }

    fn remove(&mut self, tree: &mut UiTree, text: &mut TextStore, index: usize) {
        if index >= self.items.len() {
            return;
        }
        self.items.remove(index);
        self.toasts.retain(|&(_, i)| i != index);
        for (_, i) in &mut self.toasts {
            if *i > index {
                *i -= 1;
            }
        }
        self.rebuild(tree, text);
    }

    /// Returns whether the panel moved.
    pub fn fit(&mut self, tree: &mut UiTree, desktop: Rect) -> bool {
        let Some(panel) = &mut self.panel else {
            return false;
        };
        if panel.fitted == desktop {
            return false;
        }
        panel.fitted = desktop;
        let root = panel.root;
        tree.update::<Container, _>(root, |c, cx| c.set_style(cx, panel_style(desktop)));
        tree.set_overlay_placement(root, placement(desktop));
        true
    }

    /// Newest first.
    fn rebuild(&mut self, tree: &mut UiTree, text: &mut TextStore) {
        let Some(panel) = &mut self.panel else { return };
        for child in tree.children(panel.list).to_vec() {
            tree.remove(child, text);
        }
        panel.removes.clear();
        tree.set_enabled(panel.clear, !self.items.is_empty());
        if self.items.is_empty() {
            tree.insert(
                Some(panel.list),
                Label::new("No Notifications").color(Role::Muted),
            );
            return;
        }
        let now = tree.now();
        let mut removes = vec![None; self.items.len()];
        for (index, item) in self.items.iter().enumerate().rev() {
            let card = tree.insert(
                Some(panel.list),
                Container::new(Style {
                    flex_direction: FlexDirection::Column,
                    gap: LayoutSize {
                        width: length(0.0),
                        height: length(2.0),
                    },
                    padding: LayoutRect::length(10.0),
                    flex_shrink: 0.0,
                    ..Default::default()
                })
                .background(Role::SurfaceAlt)
                .radius(8.0),
            );
            let top = tree.insert(
                Some(card),
                Container::new(Style {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::CENTER,
                    gap: LayoutSize {
                        width: length(6.0),
                        height: length(0.0),
                    },
                    ..Default::default()
                }),
            );
            tree.insert(
                Some(top),
                Label::new(item.app.to_uppercase())
                    .font_size(10.0)
                    .weight(700)
                    .color(Role::Muted),
            );
            tree.insert(
                Some(top),
                Container::new(Style {
                    flex_grow: 1.0,
                    ..Default::default()
                }),
            );
            tree.insert(
                Some(top),
                Label::new(ago(now - item.at))
                    .font_size(10.0)
                    .color(Role::Muted),
            );
            let remove = tree.insert(
                Some(top),
                Button::new("×")
                    .style(ButtonStyle::Inline)
                    .tooltip("Remove"),
            );
            removes[index] = Some(remove);
            tree.insert(
                Some(card),
                Label::new(item.title.clone())
                    .weight(600)
                    .truncation(Truncation::Tail),
            );
            tree.insert(
                Some(card),
                Label::new(item.body.clone())
                    .font_size(12.0)
                    .color(Role::Muted)
                    .wrap(true),
            );
        }
        panel.removes = removes.into_iter().flatten().collect();
    }

    /// Returns whether `message` was the notification center's.
    pub fn handle(
        &mut self,
        tree: &mut UiTree,
        text: &mut TextStore,
        desktop: Rect,
        message: &Message,
    ) -> bool {
        let source = message.source;
        if self.toasts.iter().any(|&(t, _)| t == source) {
            if message.downcast_ref::<ToastAction>().is_some() {
                self.open(tree, text, desktop);
            } else if message.downcast_ref::<ToastClosed>().is_some() {
                // Seen as a toast; it stays unread until the panel is opened.
                self.toasts.retain(|&(t, _)| t != source);
            }
            return true;
        }
        let Some(panel) = &self.panel else {
            return false;
        };
        if source == panel.root && message.downcast_ref::<OverlayDismissed>().is_some() {
            self.panel = None;
            self.closed_by_press = self.pressing;
            return true;
        }
        if message.downcast_ref::<ButtonClicked>().is_none() {
            return false;
        }
        if source == panel.clear {
            self.clear(tree, text);
            return true;
        }
        if let Some(index) = panel.removes.iter().position(|&b| b == source) {
            self.remove(tree, text, index);
            return true;
        }
        false
    }
}

fn panel_style(desktop: Rect) -> Style {
    Style {
        flex_direction: FlexDirection::Column,
        gap: LayoutSize {
            width: length(0.0),
            height: length(10.0),
        },
        padding: LayoutRect::length(12.0),
        size: LayoutSize {
            width: length(PANEL_WIDTH),
            height: length((desktop.height - 2.0 * INSET).max(0.0)),
        },
        ..Default::default()
    }
}

fn placement(desktop: Rect) -> Placement {
    Placement::At(noa_gpui_scene::Point::new(
        desktop.right() - PANEL_WIDTH - INSET,
        desktop.y + INSET,
    ))
}

fn ago(seconds: f64) -> String {
    let minutes = (seconds / 60.0).floor() as i64;
    match minutes {
        ..=0 => "now".into(),
        1..=59 => format!("{minutes}m ago"),
        _ => format!("{}h ago", minutes / 60),
    }
}

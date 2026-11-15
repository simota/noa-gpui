//! The inspector: press ⇧⌘I (Ctrl+Shift+I elsewhere; a browser may keep that
//! shortcut for its own developer tools) in an app built with
//! [`crate::App::inspector`] and the widget under the pointer is outlined, with a panel
//! naming it as a selector would, its state and frame, its ancestors and the style
//! rules that shape it (see [`noa_gpui_core::inspect`]).
//!
//! While it is on, the pointer still moves over the app, so `:hover` styles show, but
//! a click pins the widget under it instead of reaching the app (click again to
//! unpin), and Escape unpins, then turns the inspector off. Keys other than these reach
//! the app.

use noa_gpui_core::style::{AlignItems, JustifyContent, Size as LayoutSize, percent};
use noa_gpui_core::widgets::{Container, Label};
use noa_gpui_core::{Event, Key, PointerButton, Style, UiTree, Widget as _, WidgetId};
use noa_gpui_scene::{Border, Color, DisplayList, Point, Size};
use noa_gpui_text::TextStore;

const PANEL_WIDTH: f32 = 420.0;
const MARGIN: f32 = 8.0;
const HIGHLIGHT: Color = Color::rgba(0.25, 0.5, 1.0, 0.18);
const HIGHLIGHT_EDGE: Color = Color::rgba(0.25, 0.5, 1.0, 0.9);
const PINNED_EDGE: Color = Color::rgba(1.0, 0.45, 0.1, 0.95);

pub(crate) struct Inspector {
    active: bool,
    pointer: Option<Point>,
    pinned: Option<WidgetId>,
    /// The inspector took the last primary press, so it takes its release too. A press
    /// the app got before the inspector was turned on is released to the app.
    pressed: bool,
    /// The panel, a tree of its own drawn over the app's.
    panel: UiTree,
    root: WidgetId,
    card: WidgetId,
    label: WidgetId,
    /// What the panel shows, to update it only when it changes.
    shown: String,
    /// Where the panel was put (at the top, its width), to lay it out only on a change.
    placed: Option<(bool, f32)>,
    /// The app's drawing with the highlight and the panel on top.
    list: DisplayList,
}

/// The panel's card, before its width is set.
fn card() -> Container {
    Container::column()
        .padding(10.0)
        .background(Color::rgba(0.08, 0.09, 0.12, 0.94))
        .radius(8.0)
}

/// Fills the window and puts the card in a corner: the bottom right, or the top right
/// when `top`.
fn root_style(top: bool) -> Style {
    let justify = if top {
        JustifyContent::START
    } else {
        JustifyContent::END
    };
    Style {
        size: LayoutSize {
            width: percent(1.0),
            height: percent(1.0),
        },
        ..Container::column()
            .padding(MARGIN)
            .align_items(AlignItems::END)
            .justify_content(justify)
            .style()
    }
}

impl Inspector {
    pub fn new() -> Self {
        let mut panel = UiTree::new();
        let root = panel.insert(None, Container::new(root_style(false)));
        let card = panel.insert(Some(root), card().width(PANEL_WIDTH));
        let label = panel.insert(
            Some(card),
            Label::new("")
                .font_size(12.0)
                .color(Color::rgba(0.92, 0.94, 1.0, 1.0))
                .wrap(true),
        );
        Self {
            active: false,
            pointer: None,
            pinned: None,
            pressed: false,
            panel,
            root,
            card,
            label,
            shown: String::new(),
            placed: None,
            list: DisplayList::new(),
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Look at an input event before the app does. Returns `true` when the inspector
    /// took it (the app must not see it).
    pub fn filter(&mut self, event: &Event, tree: &UiTree) -> bool {
        match event {
            Event::KeyDown {
                key: Key::Char('i'),
                modifiers,
            } if modifiers.command && modifiers.shift => {
                self.active = !self.active;
                self.pinned = None;
                true
            }
            // The release of the inspector's own press, even if it was closed since.
            Event::PointerUp {
                button: PointerButton::Primary,
                ..
            } if self.pressed => {
                self.pressed = false;
                true
            }
            _ if !self.active => false,
            Event::PointerMove { pos } => {
                self.pointer = Some(*pos);
                false
            }
            Event::PointerLeave => {
                self.pointer = None;
                false
            }
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                let hit = tree.hit_test_including_disabled(*pos);
                self.pinned = if self.pinned.is_some() { None } else { hit };
                self.pressed = true;
                true
            }
            Event::KeyDown {
                key: Key::Escape, ..
            } => {
                if self.pinned.take().is_none() {
                    self.active = false;
                }
                true
            }
            _ => false,
        }
    }

    /// The widget shown: the pinned one, or the one under the pointer.
    pub fn target(&self, tree: &UiTree) -> Option<WidgetId> {
        self.pinned
            .filter(|id| tree.window_rect(*id).is_some())
            .or_else(|| {
                self.pointer
                    .and_then(|pos| tree.hit_test_including_disabled(pos))
            })
    }

    /// `app` with the highlight and the panel drawn over it, at `viewport` logical
    /// points. Call after the app's tree was laid out.
    pub fn compose(
        &mut self,
        app: &DisplayList,
        tree: &UiTree,
        text: &mut TextStore,
        viewport: Size,
    ) -> &DisplayList {
        let target = self.target(tree);
        let info = target.and_then(|id| tree.inspect(id));
        let pinned = self.pinned.is_some() && target == self.pinned;
        let message = match &info {
            Some(info) => {
                let hint = if pinned {
                    "pinned · click or Escape to unpin"
                } else {
                    "click to pin · Escape to close"
                };
                format!("{}\n{hint}", info.describe())
            }
            None => "Inspector: point at a widget · ⇧⌘I or Escape to close".to_owned(),
        };
        if message != self.shown {
            self.panel
                .update::<Label, _>(self.label, |label, cx| label.set_text(cx, message.clone()));
            self.shown = message;
        }
        // Keep the panel away from the widget: at the top when it is low on the screen.
        let low = info
            .as_ref()
            .is_some_and(|i| i.rect.y + i.rect.height / 2.0 > viewport.height / 2.0);
        let width = PANEL_WIDTH.min(viewport.width - 2.0 * MARGIN).max(0.0);
        if self.placed != Some((low, width)) {
            self.placed = Some((low, width));
            let root_style = root_style(low);
            let card_style = card().width(width).style();
            self.panel
                .update::<Container, _>(self.root, |root, cx| root.set_style(cx, root_style));
            self.panel
                .update::<Container, _>(self.card, |card, cx| card.set_style(cx, card_style));
        }
        self.panel.prepare(viewport, text);

        self.list.clear();
        self.list.extend_translated(app.commands(), 0.0, 0.0);
        if let Some(info) = &info {
            let edge = if pinned { PINNED_EDGE } else { HIGHLIGHT_EDGE };
            self.list.rounded_rect(
                info.rect,
                0.0,
                HIGHLIGHT,
                Border {
                    width: 1.5,
                    color: edge,
                },
            );
        }
        self.list
            .extend_translated(self.panel.display_list().commands(), 0.0, 0.0);
        &self.list
    }

    /// The panel's area, for tests.
    #[cfg(test)]
    fn panel_rect(&self) -> Option<noa_gpui_scene::Rect> {
        self.panel.window_rect(self.card)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noa_gpui_core::widgets::Button;
    use noa_gpui_core::{Modifiers, StyleMeta};
    use noa_gpui_scene::DrawCommand;

    const VIEWPORT: Size = Size::new(800.0, 600.0);

    fn toggle() -> Event {
        Event::KeyDown {
            key: Key::Char('i'),
            modifiers: Modifiers {
                command: true,
                shift: true,
                ..Default::default()
            },
        }
    }

    fn app() -> (UiTree, WidgetId, TextStore) {
        let mut tree = UiTree::new();
        let root = tree.insert(None, Container::column().padding(20.0));
        let button = tree.insert(Some(root), Button::new("Save"));
        tree.set_style_meta(button, StyleMeta::new().id("save"));
        let mut text = TextStore::new(Default::default());
        tree.prepare(VIEWPORT, &mut text);
        (tree, button, text)
    }

    fn panel_text(inspector: &Inspector) -> &str {
        &inspector.shown
    }

    #[test]
    fn the_shortcut_toggles_it_and_only_its_own_input_is_taken() {
        let (tree, _, _) = app();
        let mut inspector = Inspector::new();
        let click = Event::PointerDown {
            pos: Point::new(30.0, 30.0),
            button: PointerButton::Primary,
        };
        assert!(!inspector.filter(&click, &tree), "off: the app gets clicks");
        assert!(inspector.filter(&toggle(), &tree));
        assert!(inspector.is_active());
        assert!(!inspector.filter(
            &Event::PointerMove {
                pos: Point::new(1.0, 1.0)
            },
            &tree
        ));
        assert!(!inspector.filter(&Event::Text("x".into()), &tree));
        assert!(inspector.filter(&click, &tree), "on: a click pins");
        assert!(inspector.filter(
            &Event::KeyDown {
                key: Key::Escape,
                modifiers: Modifiers::default()
            },
            &tree
        ));
        assert!(inspector.is_active(), "the first Escape unpins");
        inspector.filter(
            &Event::KeyDown {
                key: Key::Escape,
                modifiers: Modifiers::default(),
            },
            &tree,
        );
        assert!(!inspector.is_active(), "the second closes");
    }

    #[test]
    fn the_widget_under_the_pointer_is_outlined_and_described_and_can_be_pinned() {
        let (tree, button, mut text) = app();
        let mut inspector = Inspector::new();
        inspector.filter(&toggle(), &tree);
        let rect = tree.window_rect(button).unwrap();
        let over = Point::new(rect.x + 5.0, rect.y + 5.0);
        inspector.filter(&Event::PointerMove { pos: over }, &tree);
        assert_eq!(inspector.target(&tree), Some(button));

        let app_list = tree.display_list().clone();
        let composed = inspector
            .compose(&app_list, &tree, &mut text, VIEWPORT)
            .clone();
        assert!(panel_text(&inspector).starts_with("Button#save\n"));
        // The app's drawing comes first, unchanged, then the outline and the panel.
        assert_eq!(&composed.commands()[..app_list.len()], app_list.commands());
        assert!(
            composed.commands()[app_list.len()..]
                .iter()
                .any(|c| matches!(
                    c,
                    DrawCommand::Quad { rect: r, .. } if *r == rect
                ))
        );
        // The button is near the top, so the panel goes to the bottom.
        assert!(inspector.panel_rect().unwrap().y > VIEWPORT.height / 2.0);

        inspector.filter(
            &Event::PointerDown {
                pos: over,
                button: PointerButton::Primary,
            },
            &tree,
        );
        inspector.filter(
            &Event::PointerMove {
                pos: Point::new(790.0, 590.0),
            },
            &tree,
        );
        assert_eq!(inspector.target(&tree), Some(button), "pinned");
        inspector.compose(&app_list, &tree, &mut text, VIEWPORT);
        assert!(panel_text(&inspector).contains("pinned"));
    }

    #[test]
    fn disabled_widgets_can_be_pointed_at_and_pinned() {
        let (mut tree, button, mut text) = app();
        tree.set_enabled(button, false);
        tree.prepare(VIEWPORT, &mut text);
        let rect = tree.window_rect(button).unwrap();
        let over = Point::new(rect.x + 5.0, rect.y + 5.0);
        assert_ne!(tree.hit_test(over), Some(button), "the app cannot hit it");
        let mut inspector = Inspector::new();
        inspector.filter(&toggle(), &tree);
        inspector.filter(&Event::PointerMove { pos: over }, &tree);
        assert_eq!(inspector.target(&tree), Some(button));
        inspector.filter(
            &Event::PointerDown {
                pos: over,
                button: PointerButton::Primary,
            },
            &tree,
        );
        assert_eq!(inspector.pinned, Some(button));
        let info = tree.inspect(button).unwrap();
        assert_eq!(info.pseudo_classes(), ":disabled");
    }

    #[test]
    fn a_press_made_before_turning_it_on_is_released_to_the_app() {
        let (tree, _, _) = app();
        let mut inspector = Inspector::new();
        let at = Point::new(30.0, 30.0);
        let down = Event::PointerDown {
            pos: at,
            button: PointerButton::Primary,
        };
        let up = Event::PointerUp {
            pos: at,
            button: PointerButton::Primary,
        };
        assert!(!inspector.filter(&down, &tree), "the app gets the press");
        inspector.filter(&toggle(), &tree);
        assert!(!inspector.filter(&up, &tree), "and its release");
        assert!(inspector.filter(&down, &tree), "the inspector's own press");
        assert!(inspector.filter(&up, &tree), "and its release");

        // Closed between its own press and the release: the release is still its own.
        assert!(inspector.filter(&down, &tree));
        inspector.filter(&toggle(), &tree);
        assert!(!inspector.is_active());
        assert!(inspector.filter(&up, &tree));
        assert!(
            !inspector.filter(&up, &tree),
            "later releases go to the app"
        );
    }
}

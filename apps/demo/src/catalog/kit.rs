use noa_gpui_core::style::{
    AlignItems, FlexDirection, FlexWrap, LengthPercentage, Rect as LayoutRect, Size as LayoutSize,
    auto, length, percent,
};
use noa_gpui_core::widgets::{Container, GroupBox, Label, Separator};
use noa_gpui_core::{
    Event, EventCx, MeasureCx, MeasureInput, PaintCx, PointerButton, Role, Style, UiTree, Widget,
    WidgetId,
};
use noa_gpui_scene::{Border, Rect, Size, TextLayoutId};
use noa_gpui_text::{TextStore, TextStyle};

use crate::assets::Assets;

pub const DIVIDER: Role = Role::Separator;

pub struct Kit<'a> {
    pub tree: &'a mut UiTree,
    pub assets: &'a Assets,
    pub grid: WidgetId,
}

impl Kit<'_> {
    /// Returns the content box.
    pub fn card(&mut self, caption: &str) -> WidgetId {
        card(self.tree, self.grid, caption)
    }

    pub fn wide_card(&mut self, caption: &str, width: f32) -> WidgetId {
        card_sized(self.tree, self.grid, caption, width)
    }
}

/// Returns `(section, card row)`.
pub fn section(
    tree: &mut UiTree,
    parent: WidgetId,
    title: &str,
    description: &str,
) -> (WidgetId, WidgetId) {
    let section = tree.insert(
        Some(parent),
        Container::new(Style {
            flex_direction: FlexDirection::Column,
            gap: gap(0.0, 8.0),
            flex_shrink: 0.0,
            ..Default::default()
        }),
    );
    tree.insert(Some(section), Label::new(title).font_size(24.0).weight(700));
    let text_box = tree.insert(
        Some(section),
        Container::new(Style {
            max_size: LayoutSize {
                width: length(720.0),
                height: auto(),
            },
            padding: LayoutRect {
                left: zero(),
                right: zero(),
                top: zero(),
                bottom: length(8.0),
            },
            ..Default::default()
        }),
    );
    tree.insert(
        Some(text_box),
        Label::new(description).color(Role::Muted).wrap(true),
    );
    let grid = tree.insert(Some(section), Container::wrapping_row(16.0, 16.0));
    (section, grid)
}

/// Returns the content box.
pub fn card(tree: &mut UiTree, grid: WidgetId, caption: &str) -> WidgetId {
    card_sized(tree, grid, caption, 300.0)
}

fn card_sized(tree: &mut UiTree, grid: WidgetId, caption: &str, width: f32) -> WidgetId {
    let card = tree.insert(Some(grid), GroupBox::card("").width(width));
    let content = tree.insert(
        Some(card),
        Container::new(Style {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::START,
            gap: gap(0.0, 8.0),
            ..Default::default()
        }),
    );
    tree.insert(Some(card), Separator::horizontal());
    tree.insert(
        Some(card),
        Label::new(caption)
            .font_size(12.0)
            .color(Role::Muted)
            .wrap(true),
    );
    content
}

/// `id`'s rectangle in window coordinates, for anchoring overlays.
pub fn window_rect(tree: &UiTree, id: WidgetId) -> Rect {
    let origin = tree.window_origin(id).unwrap_or_default();
    let size = tree.layout(id).map(|l| l.rect).unwrap_or_default();
    Rect::new(origin.x, origin.y, size.width, size.height)
}

pub fn overlay_panel(padding: f32) -> Container {
    Container::new(Style {
        flex_direction: FlexDirection::Column,
        gap: gap(0.0, 6.0),
        padding: LayoutRect::length(padding),
        ..Default::default()
    })
    .background(Role::Surface)
    .border(1.0, Role::Outline)
    .radius(8.0)
}

pub fn swatch(width: f32, height: f32) -> Container {
    Container::new(Style {
        size: LayoutSize {
            width: length(width),
            height: length(height),
        },
        flex_shrink: 0.0,
        ..Default::default()
    })
}

pub fn row_style(gap_x: f32) -> Style {
    Style {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::CENTER,
        flex_wrap: FlexWrap::Wrap,
        gap: gap(gap_x, gap_x),
        ..Default::default()
    }
}

pub fn gap(x: f32, y: f32) -> LayoutSize<LengthPercentage> {
    LayoutSize {
        width: length(x),
        height: length(y),
    }
}

pub fn zero() -> LengthPercentage {
    length(0.0)
}

/// Emitted by a [`NavItem`] when it is clicked.
pub struct NavSelected;

/// Defined here rather than in `noa-gpui-core` to show that apps can implement [`Widget`].
pub struct NavItem {
    label: String,
    pressed: bool,
    layout: Option<TextLayoutId>,
    text_size: Size,
}

impl NavItem {
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_owned(),
            pressed: false,
            layout: None,
            text_size: Size::default(),
        }
    }
}

impl Widget for NavItem {
    fn style(&self) -> Style {
        Style {
            padding: LayoutRect {
                left: length(12.0),
                right: length(12.0),
                top: length(6.0),
                bottom: length(6.0),
            },
            size: LayoutSize {
                width: percent(1.0),
                height: auto(),
            },
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn measures(&self) -> bool {
        true
    }

    fn measure(&mut self, cx: &mut MeasureCx, _input: MeasureInput) -> Size {
        if let Some(old) = self.layout.take() {
            cx.text.remove(old);
        }
        let id = cx
            .text
            .create(&self.label, TextStyle::new(14.0).weight(500), None);
        self.text_size = cx.text.size(id);
        self.layout = Some(id);
        self.text_size
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        if cx.is_hovered() {
            let fill = cx.color(Role::SurfaceAlt);
            cx.rounded_rect(bounds, 6.0, fill, Border::default());
        }
        if let Some(layout) = self.layout {
            let y = (bounds.height - self.text_size.height) / 2.0;
            let color = cx.color(Role::Text);
            cx.text(
                Rect::new(12.0, y, self.text_size.width, self.text_size.height),
                layout,
                color,
            );
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            Event::PointerDown {
                button: PointerButton::Primary,
                ..
            } => {
                self.pressed = true;
                cx.capture_pointer();
                cx.set_handled();
            }
            Event::PointerUp {
                pos,
                button: PointerButton::Primary,
            } if self.pressed => {
                self.pressed = false;
                cx.release_pointer();
                if cx.contains(*pos) {
                    cx.emit(NavSelected);
                }
                cx.set_handled();
            }
            _ => {}
        }
    }

    fn accessibility_role(&self) -> Option<noa_gpui_core::accesskit::Role> {
        Some(noa_gpui_core::accesskit::Role::Link)
    }

    fn accessibility(&self, node: &mut noa_gpui_core::accesskit::Node) {
        node.set_label(self.label.clone());
        node.add_action(noa_gpui_core::accesskit::Action::Click);
    }

    fn removed(&mut self, text: &mut TextStore) {
        if let Some(id) = self.layout.take() {
            text.remove(id);
        }
    }
}

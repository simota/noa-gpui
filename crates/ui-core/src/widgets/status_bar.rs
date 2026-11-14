use ui_layout::Style;
use ui_layout::style::{
    AlignItems, FlexDirection, JustifyContent, Rect as LayoutRect, Size as LayoutSize, auto,
    length, percent,
};
use ui_scene::{ImageId, Rect, Size};

use super::{Button, Image, Label};
use crate::theme::Role;
use crate::tree::{UiTree, WidgetId};
use crate::widget::{PaintCx, Widget};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmptyStateParts {
    pub root: WidgetId,
    pub title: WidgetId,
    pub message: Option<WidgetId>,
    /// The action button; its [`super::ButtonClicked`] is the app's to handle.
    pub action: Option<WidgetId>,
}

/// What a list, search or view shows when it has nothing to show: an optional image,
/// a title, an explanation and the action that fills it ("No documents — Create
/// Document"), centered in the space it is given.
#[derive(Debug, Clone)]
pub struct EmptyState {
    title: String,
    message: Option<String>,
    image: Option<(ImageId, Size)>,
    action: Option<String>,
    /// Longest line of the message before it wraps.
    width: f32,
}

impl EmptyState {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: None,
            image: None,
            action: None,
            width: 320.0,
        }
    }

    /// Text under the title, in the secondary color.
    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    pub fn image(mut self, image: ImageId, size: Size) -> Self {
        self.image = Some((image, size));
        self
    }

    pub fn action(mut self, title: impl Into<String>) -> Self {
        self.action = Some(title.into());
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Add it as the last child of `parent`, filling it.
    pub fn insert(self, tree: &mut UiTree, parent: WidgetId) -> EmptyStateParts {
        let root = tree.insert(Some(parent), Column);
        if let Some((image, size)) = self.image {
            tree.insert(Some(root), Image::new(image, size));
        }
        let title = tree.insert(
            Some(root),
            Label::new(self.title).font_size(17.0).weight(600),
        );
        let message = self.message.map(|message| {
            let wrap = tree.insert(
                Some(root),
                super::Container::new(Style {
                    max_size: LayoutSize {
                        width: length(self.width),
                        height: auto(),
                    },
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::CENTER,
                    ..Default::default()
                }),
            );
            tree.insert(
                Some(wrap),
                Label::new(message).wrap(true).color(Role::Muted),
            )
        });
        let action = self.action.map(|title| {
            tree.insert(Some(root), Spacer(6.0));
            tree.insert(Some(root), Button::new(title))
        });
        EmptyStateParts {
            root,
            title,
            message,
            action,
        }
    }
}

#[derive(Debug)]
struct Column;

impl Widget for Column {
    fn style(&self) -> Style {
        Style {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::CENTER,
            justify_content: JustifyContent::CENTER,
            flex_grow: 1.0,
            gap: LayoutSize {
                width: length(0.0),
                height: length(6.0),
            },
            padding: LayoutRect::length(24.0),
            size: LayoutSize {
                width: percent(1.0),
                height: auto(),
            },
            ..Default::default()
        }
    }

    fn paint(&mut self, _cx: &mut PaintCx) {}

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Group)
    }
}

#[derive(Debug)]
struct Spacer(f32);

impl Widget for Spacer {
    fn style(&self) -> Style {
        Style {
            size: LayoutSize {
                width: length(0.0),
                height: length(self.0),
            },
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn paint(&mut self, _cx: &mut PaintCx) {}
}

pub const STATUS_BAR_HEIGHT: f32 = 24.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusBarParts {
    pub bar: WidgetId,
    /// Items on the left (a mode, a message); add children here.
    pub leading: WidgetId,
    pub trailing: WidgetId,
}

/// A strip along the bottom of a window or view, with a hairline above it. The bar
/// itself is a row, so items can also be inserted into it directly.
#[derive(Debug, Default)]
pub struct StatusBar;

impl StatusBar {
    pub fn new() -> Self {
        Self
    }

    pub fn insert(tree: &mut UiTree, parent: WidgetId) -> StatusBarParts {
        let bar = tree.insert(Some(parent), StatusBar);
        let row = |grow: f32| {
            super::Container::new(Style {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::CENTER,
                gap: LayoutSize {
                    width: length(12.0),
                    height: length(0.0),
                },
                flex_grow: grow,
                ..Default::default()
            })
        };
        let leading = tree.insert(Some(bar), row(1.0));
        let trailing = tree.insert(Some(bar), row(0.0));
        StatusBarParts {
            bar,
            leading,
            trailing,
        }
    }
}

impl Widget for StatusBar {
    fn style(&self) -> Style {
        Style {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::CENTER,
            flex_shrink: 0.0,
            gap: LayoutSize {
                width: length(12.0),
                height: length(0.0),
            },
            padding: LayoutRect {
                left: length(10.0),
                right: length(10.0),
                top: length(0.0),
                bottom: length(0.0),
            },
            size: LayoutSize {
                width: percent(1.0),
                height: length(STATUS_BAR_HEIGHT),
            },
            ..Default::default()
        }
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let fill = cx.color(Role::SurfaceAlt);
        cx.fill_rect(bounds, fill);
        let hairline = cx.color(Role::Separator);
        cx.fill_rect(Rect::new(0.0, 0.0, bounds.width, 1.0), hairline);
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Status)
    }
}

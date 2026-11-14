use ui_layout::Style;
use ui_layout::style::{Size as LayoutSize, length};
use ui_scene::{Color, ImageId, Rect, Size, VectorIcon};

use crate::theme::{Paint, Role};
use crate::widget::{PaintCx, UpdateCx, Widget};

/// An icon given to a widget: a vector icon, drawn in the widget's text color (so it
/// follows the theme and the disabled state), or an image, drawn as it is.
#[derive(Debug, Clone, PartialEq)]
pub enum Icon {
    Vector(VectorIcon),
    Image(ImageId),
}

impl From<VectorIcon> for Icon {
    fn from(icon: VectorIcon) -> Self {
        Icon::Vector(icon)
    }
}

impl From<ImageId> for Icon {
    fn from(image: ImageId) -> Self {
        Icon::Image(image)
    }
}

impl Icon {
    /// Draw it filling `rect`; a vector icon in `ink`.
    pub(crate) fn paint(&self, cx: &mut PaintCx, rect: Rect, ink: Color) {
        match self {
            Icon::Vector(icon) => cx.icon(rect, icon, ink),
            Icon::Image(image) => cx.image(rect, *image),
        }
    }
}

/// A vector icon at a fixed size, in one theme color (the text color by default) that
/// is dimmed when disabled.
/// Without a [`SvgIcon::label`] it is decorative and left out of the accessibility
/// tree.
#[derive(Debug)]
pub struct SvgIcon {
    icon: VectorIcon,
    size: Size,
    color: Paint,
    label: Option<String>,
    /// The icon split by color ([`VectorIcon::color_parts`]) when drawn in its own
    /// colors.
    parts: Option<Vec<(VectorIcon, Option<Color>)>>,
}

impl SvgIcon {
    pub fn new(icon: VectorIcon, size: f32) -> Self {
        Self {
            icon,
            size: Size::new(size, size),
            color: Paint::Role(Role::Text),
            label: None,
            parts: None,
        }
    }

    /// A box of `width` × `height`; the icon keeps its proportions and is centered.
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.size = Size::new(width, height);
        self
    }

    pub fn color(mut self, color: impl Into<Paint>) -> Self {
        self.color = color.into();
        self
    }

    /// Draw each part in the color its SVG file gives it (an emblem in green and
    /// gray), and the parts in `currentColor` in [`SvgIcon::color`] (off by default:
    /// the whole icon in [`SvgIcon::color`]). The colors are dimmed when disabled.
    pub fn multicolor(mut self, multicolor: bool) -> Self {
        self.parts = multicolor.then(|| self.icon.color_parts());
        self
    }

    pub fn is_multicolor(&self) -> bool {
        self.parts.is_some()
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn icon(&self) -> &VectorIcon {
        &self.icon
    }

    pub fn set_size(&mut self, cx: &mut UpdateCx, width: f32, height: f32) {
        let size = Size::new(width, height);
        if size != self.size {
            self.size = size;
            cx.request_layout();
            cx.request_paint();
        }
    }

    pub fn set_icon(&mut self, cx: &mut UpdateCx, icon: VectorIcon) {
        if icon != self.icon {
            if self.parts.is_some() {
                self.parts = Some(icon.color_parts());
            }
            self.icon = icon;
            cx.request_paint();
        }
    }

    pub fn set_color(&mut self, cx: &mut UpdateCx, color: impl Into<Paint>) {
        let color = color.into();
        if color != self.color {
            self.color = color;
            cx.request_paint();
        }
    }
}

impl Widget for SvgIcon {
    fn style(&self) -> Style {
        // A fixed size, so a stretching container does not distort the box.
        Style {
            size: LayoutSize {
                width: length(self.size.width),
                height: length(self.size.height),
            },
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let color = cx.color(self.color);
        let bounds = cx.bounds();
        match &self.parts {
            Some(parts) => {
                for (part, own) in parts {
                    let ink = own.map_or(color, |own| cx.color(own));
                    cx.icon(bounds, part, ink);
                }
            }
            None => cx.icon(bounds, &self.icon, color),
        }
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        self.label.as_ref().map(|_| accesskit::Role::Image)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        if let Some(label) = &self.label {
            node.set_label(label.clone());
        }
    }
}

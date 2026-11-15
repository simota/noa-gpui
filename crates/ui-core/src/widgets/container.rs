use noa_gpui_layout::Style;
use noa_gpui_layout::style::{
    AlignItems, FlexDirection, FlexWrap, JustifyContent, Rect as LayoutRect, Size as LayoutSize,
    length,
};
use noa_gpui_scene::Border;

use crate::theme::Paint;
use crate::widget::{PaintCx, UpdateCx, Widget};

/// A box that lays out its children with Flexbox / Grid and may draw a background.
#[derive(Debug, Clone, Default)]
pub struct Container {
    style: Style,
    background: Option<Paint>,
    border: Option<(f32, Paint)>,
    radius: f32,
    focus_group: bool,
    clip: bool,
}

impl Container {
    pub fn new(style: Style) -> Self {
        Self {
            style,
            ..Default::default()
        }
    }

    /// Children top to bottom, stretched to its width.
    pub fn column() -> Self {
        Self::new(Style {
            flex_direction: FlexDirection::Column,
            ..Default::default()
        })
    }

    /// A row: children left to right, stretched to its height.
    pub fn row() -> Self {
        Self::new(Style {
            flex_direction: FlexDirection::Row,
            ..Default::default()
        })
    }

    /// How children sit across the main axis (CSS `align-items`).
    pub fn align_items(mut self, align: AlignItems) -> Self {
        self.style.align_items = align;
        self
    }

    /// How children are spread along the main axis (CSS `justify-content`).
    pub fn justify_content(mut self, justify: JustifyContent) -> Self {
        self.style.justify_content = justify;
        self
    }

    /// Applies in both directions.
    pub fn gap(mut self, gap: f32) -> Self {
        self.style.gap = LayoutSize {
            width: length(gap),
            height: length(gap),
        };
        self
    }

    /// On every side.
    pub fn padding(mut self, padding: f32) -> Self {
        self.style.padding = LayoutRect::length(padding);
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.style.size.width = length(width);
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.style.size.height = length(height);
        self
    }

    /// CSS `flex-grow`.
    pub fn grow(mut self, grow: f32) -> Self {
        self.style.flex_grow = grow;
        self
    }

    /// CSS `flex-wrap: wrap`; each child keeps its own height at the top of its line.
    /// See [`Container::wrap`] for children with percentage heights.
    pub fn wrapping_row(column_gap: f32, row_gap: f32) -> Self {
        Self::new(Style {
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            align_items: AlignItems::START,
            gap: LayoutSize {
                width: length(column_gap),
                height: length(row_gap),
            },
            ..Default::default()
        })
    }

    /// The container's height comes from its lines, so a child's percentage height
    /// has nothing fixed to refer to: a divider at `height: percent(1.0)` grows its
    /// line to the whole container and pushes the next lines out of it. Stretch such a
    /// child to its line instead (`align_self: Some(AlignItems::STRETCH)` with an
    /// `auto` height, as [`Separator::vertical`](super::Separator::vertical) does).
    pub fn wrap(mut self, wrap: bool) -> Self {
        self.style.flex_wrap = if wrap {
            FlexWrap::Wrap
        } else {
            FlexWrap::NoWrap
        };
        self
    }

    /// Keep `width / height` at `ratio` (16.0 / 9.0 for a video): with only one side
    /// set or stretched by the parent, the other follows it.
    pub fn aspect_ratio(mut self, ratio: f32) -> Self {
        self.style.aspect_ratio = Some(ratio);
        self
    }

    pub fn background(mut self, color: impl Into<Paint>) -> Self {
        self.background = Some(color.into());
        self
    }

    pub fn border(mut self, width: f32, color: impl Into<Paint>) -> Self {
        self.border = Some((width, color.into()));
        self
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    pub fn clip(mut self, clip: bool) -> Self {
        self.clip = clip;
        self
    }

    pub fn focus_group(mut self, group: bool) -> Self {
        self.focus_group = group;
        self
    }

    pub fn set_background(&mut self, cx: &mut UpdateCx, color: Option<Paint>) {
        if self.background != color {
            self.background = color;
            cx.request_paint();
        }
    }

    /// Change or remove (`None`) the border: `(width, color)`, as [`Container::border`].
    pub fn set_border(&mut self, cx: &mut UpdateCx, border: Option<(f32, Paint)>) {
        if self.border != border {
            self.border = border;
            cx.request_paint();
        }
    }

    pub fn set_radius(&mut self, cx: &mut UpdateCx, radius: f32) {
        if self.radius != radius {
            self.radius = radius;
            cx.request_paint();
        }
    }

    pub fn set_style(&mut self, cx: &mut UpdateCx, style: Style) {
        self.style = style;
        cx.request_layout();
    }
}

impl Widget for Container {
    fn style(&self) -> Style {
        self.style.clone()
    }

    fn focus_group(&self) -> bool {
        self.focus_group
    }

    fn paints_style_box(&self) -> bool {
        true
    }

    fn clips_children(&self) -> bool {
        self.clip
    }

    /// Focus groups are exposed as groups; other containers are only layout.
    fn accessibility_role(&self) -> Option<accesskit::Role> {
        self.focus_group.then_some(accesskit::Role::Group)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let overrides = cx.styling();
        let background = overrides.background_color.or(self.background);
        let border_width = overrides
            .border_width
            .or(self.border.map(|(width, _)| width))
            .unwrap_or(0.0);
        if background.is_none() && border_width <= 0.0 {
            return;
        }
        let fill = background.map_or(noa_gpui_scene::Color::TRANSPARENT, |p| cx.color(p));
        let border_color = overrides
            .border_color
            .or(self.border.map(|(_, color)| color))
            .unwrap_or(crate::theme::Role::Outline.into());
        let border = Border {
            width: border_width,
            color: cx.color(border_color),
        };
        cx.rounded_rect(
            cx.bounds(),
            overrides.border_radius.unwrap_or(self.radius),
            fill,
            border,
        );
    }
}

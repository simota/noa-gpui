use ui_layout::Style;
use ui_layout::style::{
    AlignItems, Dimension, Display, FlexDirection, GridAutoFlow, GridPlacement,
    GridTemplateComponent, JustifyContent, Line, Rect as LayoutRect, Size as LayoutSize,
    TrackSizingFunction, auto, fr, length, minmax, percent, span,
};
use ui_scene::{Border, Rect};
use ui_text::{TextStore, TextStyle};

use super::CachedText;
use crate::theme::Role;
use crate::widget::{PaintCx, UpdateCx, Widget};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StackAlignment {
    /// Top of a horizontal stack, left of a vertical one.
    Leading,
    #[default]
    Center,
    Trailing,
    /// Stretched to the stack's thickness.
    Fill,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StackDistribution {
    /// Children keep their own size, packed at the start.
    #[default]
    Natural,
    /// Every child gets the same share of the stack's length.
    FillEqually,
    /// Leftover space goes between the children, none at the ends.
    EqualSpacing,
    /// Leftover space goes around the children, half at the ends.
    EqualCentering,
}

/// Children in a row or column with fixed spacing (`NSStackView`). Hidden children
/// ([`crate::UiTree::set_hidden`]) leave no gap.
#[derive(Debug, Clone)]
pub struct StackView {
    axis: StackAxis,
    spacing: f32,
    alignment: StackAlignment,
    distribution: StackDistribution,
    insets: f32,
    fill: bool,
}

impl StackView {
    pub fn horizontal(spacing: f32) -> Self {
        Self::new(StackAxis::Horizontal, spacing)
    }

    pub fn vertical(spacing: f32) -> Self {
        Self::new(StackAxis::Vertical, spacing)
    }

    fn new(axis: StackAxis, spacing: f32) -> Self {
        Self {
            axis,
            spacing,
            alignment: StackAlignment::Center,
            distribution: StackDistribution::Natural,
            insets: 0.0,
            fill: false,
        }
    }

    pub fn alignment(mut self, alignment: StackAlignment) -> Self {
        self.alignment = alignment;
        self
    }

    pub fn distribution(mut self, distribution: StackDistribution) -> Self {
        self.distribution = distribution;
        self
    }

    pub fn edge_insets(mut self, insets: f32) -> Self {
        self.insets = insets;
        self
    }

    /// Fill the parent's width (horizontal stack) or height (vertical) instead of
    /// hugging the children, so the distribution has room to work with.
    pub fn fill_parent(mut self, fill: bool) -> Self {
        self.fill = fill;
        self
    }

    pub fn set_spacing(&mut self, cx: &mut UpdateCx, spacing: f32) {
        self.spacing = spacing;
        cx.request_layout();
    }
}

impl Widget for StackView {
    fn style(&self) -> Style {
        let horizontal = self.axis == StackAxis::Horizontal;
        let cross = match self.alignment {
            StackAlignment::Leading => AlignItems::START,
            StackAlignment::Center => AlignItems::CENTER,
            StackAlignment::Trailing => AlignItems::END,
            StackAlignment::Fill => AlignItems::STRETCH,
        };
        let gap = if horizontal {
            LayoutSize {
                width: length(self.spacing),
                height: length(0.0),
            }
        } else {
            LayoutSize {
                width: length(0.0),
                height: length(self.spacing),
            }
        };
        let size = if self.fill {
            if horizontal {
                LayoutSize {
                    width: percent(1.0),
                    height: auto(),
                }
            } else {
                LayoutSize {
                    width: auto(),
                    height: percent(1.0),
                }
            }
        } else {
            LayoutSize::auto()
        };
        let base = Style {
            gap,
            size,
            padding: LayoutRect::length(self.insets),
            flex_shrink: 0.0,
            ..Default::default()
        };
        if self.distribution == StackDistribution::FillEqually {
            // Equal tracks along the axis, each at least as large as its content allows
            // (minimum 0, so long content does not break the equality).
            let track = minmax(length(0.0), fr(1.0));
            let mut style = Style {
                display: Display::Grid,
                grid_auto_flow: if horizontal {
                    GridAutoFlow::Column
                } else {
                    GridAutoFlow::Row
                },
                align_items: if horizontal {
                    cross
                } else {
                    AlignItems::STRETCH
                },
                justify_items: if horizontal {
                    AlignItems::STRETCH
                } else {
                    cross
                },
                ..base
            };
            if horizontal {
                style.grid_auto_columns = vec![track];
            } else {
                style.grid_auto_rows = vec![track];
            }
            return style;
        }
        Style {
            flex_direction: if horizontal {
                FlexDirection::Row
            } else {
                FlexDirection::Column
            },
            align_items: cross,
            justify_content: match self.distribution {
                StackDistribution::EqualSpacing => JustifyContent::SPACE_BETWEEN,
                StackDistribution::EqualCentering => JustifyContent::SPACE_AROUND,
                _ => JustifyContent::START,
            },
            ..base
        }
    }

    fn paint(&mut self, _cx: &mut PaintCx) {}
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GridTrack {
    /// As wide as its widest cell.
    Auto,
    Fixed(f32),
    /// A share of the space left after the other columns (like CSS `fr`).
    Flexible(f32),
}

impl GridTrack {
    fn sizing(self) -> TrackSizingFunction {
        match self {
            GridTrack::Auto => auto(),
            GridTrack::Fixed(px) => length(px),
            GridTrack::Flexible(share) => minmax(length(0.0), fr(share)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GridPlacementMode {
    Leading,
    Center,
    Trailing,
    #[default]
    Fill,
}

impl GridPlacementMode {
    fn align(self) -> AlignItems {
        match self {
            GridPlacementMode::Leading => AlignItems::START,
            GridPlacementMode::Center => AlignItems::CENTER,
            GridPlacementMode::Trailing => AlignItems::END,
            GridPlacementMode::Fill => AlignItems::STRETCH,
        }
    }
}

/// Cells in rows and columns (`NSGridView`), laid out with CSS Grid. Children fill the
/// rows left to right, top to bottom.
#[derive(Debug, Clone)]
pub struct GridView {
    columns: Vec<GridTrack>,
    row_spacing: f32,
    column_spacing: f32,
    x_placement: GridPlacementMode,
    y_placement: GridPlacementMode,
    fill: bool,
}

impl GridView {
    pub fn new(columns: impl IntoIterator<Item = GridTrack>) -> Self {
        Self {
            columns: columns.into_iter().collect(),
            row_spacing: 8.0,
            column_spacing: 8.0,
            x_placement: GridPlacementMode::Leading,
            y_placement: GridPlacementMode::Center,
            fill: false,
        }
    }

    pub fn row_spacing(mut self, spacing: f32) -> Self {
        self.row_spacing = spacing;
        self
    }

    pub fn column_spacing(mut self, spacing: f32) -> Self {
        self.column_spacing = spacing;
        self
    }

    /// Horizontal placement of cells in their columns.
    pub fn x_placement(mut self, placement: GridPlacementMode) -> Self {
        self.x_placement = placement;
        self
    }

    /// Vertical placement of cells in their rows.
    pub fn y_placement(mut self, placement: GridPlacementMode) -> Self {
        self.y_placement = placement;
        self
    }

    /// Fill the parent's width, so [`GridTrack::Flexible`] columns get the rest.
    pub fn fill_width(mut self, fill: bool) -> Self {
        self.fill = fill;
        self
    }
}

impl Widget for GridView {
    fn style(&self) -> Style {
        Style {
            display: Display::Grid,
            grid_template_columns: self
                .columns
                .iter()
                .map(|c| GridTemplateComponent::Single(c.sizing()))
                .collect(),
            gap: LayoutSize {
                width: length(self.column_spacing),
                height: length(self.row_spacing),
            },
            justify_items: self.x_placement.align(),
            align_items: self.y_placement.align(),
            size: LayoutSize {
                width: if self.fill { percent(1.0) } else { auto() },
                height: auto(),
            },
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn paint(&mut self, _cx: &mut PaintCx) {}
}

/// Spans `columns` columns and `rows` rows of a [`GridView`].
#[derive(Debug, Clone)]
pub struct GridCell {
    columns: u16,
    rows: u16,
}

impl GridCell {
    pub fn new(columns: u16, rows: u16) -> Self {
        Self {
            columns: columns.max(1),
            rows: rows.max(1),
        }
    }
}

impl Widget for GridCell {
    fn style(&self) -> Style {
        Style {
            grid_column: Line {
                start: span(self.columns),
                end: GridPlacement::Auto,
            },
            grid_row: Line {
                start: span(self.rows),
                end: GridPlacement::Auto,
            },
            flex_direction: FlexDirection::Column,
            ..Default::default()
        }
    }

    fn paint(&mut self, _cx: &mut PaintCx) {}
}

const TITLE_SIZE: f32 = 12.0;
const BOX_PAD: f32 = 12.0;
const CARD_TITLE_SIZE: f32 = 14.0;
const CARD_PAD: f32 = 18.0;
const CARD_RADIUS: f32 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GroupBoxStyle {
    /// A lightly filled box with the title above it (`NSBox`).
    #[default]
    Box,
    /// A surface-colored card with the title as a heading inside it.
    Card,
}

/// A titled group box (`NSBox`), or a card ([`GroupBoxStyle::Card`]). Children go inside
/// in a column.
#[derive(Debug)]
pub struct GroupBox {
    title: String,
    shaped: CachedText,
    width: Dimension,
    style: GroupBoxStyle,
}

impl GroupBox {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            shaped: CachedText::default(),
            width: auto(),
            style: GroupBoxStyle::Box,
        }
    }

    /// An empty `title` gives no heading.
    pub fn card(title: impl Into<String>) -> Self {
        Self::new(title).style(GroupBoxStyle::Card)
    }

    pub fn style(mut self, style: GroupBoxStyle) -> Self {
        self.style = style;
        self
    }

    pub fn box_style(&self) -> GroupBoxStyle {
        self.style
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = length(width);
        self
    }

    pub fn fill_width(mut self) -> Self {
        self.width = percent(1.0);
        self
    }

    fn title_style(&self) -> TextStyle {
        match self.style {
            GroupBoxStyle::Box => TextStyle::new(TITLE_SIZE).weight(600),
            GroupBoxStyle::Card => TextStyle::new(CARD_TITLE_SIZE).weight(600),
        }
    }

    /// Height of the title and the space under it (0 without a title).
    fn title_height(&self) -> f32 {
        if self.title.is_empty() {
            return 0.0;
        }
        let line = self.title_style().line_height;
        match self.style {
            GroupBoxStyle::Box => line + 6.0,
            GroupBoxStyle::Card => line + 10.0,
        }
    }
}

impl Widget for GroupBox {
    fn style(&self) -> Style {
        let (pad, gap, top) = match self.style {
            GroupBoxStyle::Box => (BOX_PAD, 8.0, self.title_height() + BOX_PAD),
            GroupBoxStyle::Card => (CARD_PAD, 14.0, CARD_PAD + self.title_height()),
        };
        Style {
            flex_direction: FlexDirection::Column,
            gap: LayoutSize {
                width: length(0.0),
                height: length(gap),
            },
            padding: LayoutRect {
                left: length(pad),
                right: length(pad),
                top: length(top),
                bottom: length(pad),
            },
            size: LayoutSize {
                width: self.width,
                height: auto(),
            },
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let outline = cx.color(Role::Separator);
        let border = Border {
            width: 1.0,
            color: outline,
        };
        let (title_at, ink) = match self.style {
            GroupBoxStyle::Box => {
                let top = self.title_height();
                let fill = cx.color(Role::SurfaceAlt);
                cx.rounded_rect(
                    Rect::new(0.0, top, bounds.width, bounds.height - top),
                    8.0,
                    fill,
                    border,
                );
                ((4.0, 0.0), cx.color(Role::Muted))
            }
            GroupBoxStyle::Card => {
                let fill = cx.color(Role::Surface);
                cx.rounded_rect(bounds, CARD_RADIUS, fill, border);
                ((CARD_PAD, CARD_PAD), cx.color(Role::Text))
            }
        };
        if !self.title.is_empty() {
            let style = self.title_style();
            let size = self.shaped.shape(cx.text_store(), &self.title, style, None);
            if let Some(layout) = self.shaped.layout() {
                let rect = Rect::new(title_at.0, title_at.1, size.width, size.height);
                cx.text(rect, layout, ink);
            }
        }
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Group)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        if !self.title.is_empty() {
            node.set_label(self.title.clone());
        }
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.shaped.release(text);
    }
}

/// A hairline between groups of content (`NSBox` separator): horizontal lines fill the
/// parent's width, vertical ones its height.
#[derive(Debug, Clone)]
pub struct Separator {
    vertical: bool,
}

impl Separator {
    pub fn horizontal() -> Self {
        Self { vertical: false }
    }

    pub fn vertical() -> Self {
        Self { vertical: true }
    }
}

impl Widget for Separator {
    fn style(&self) -> Style {
        let size = if self.vertical {
            LayoutSize {
                width: length(1.0),
                height: auto(),
            }
        } else {
            LayoutSize {
                width: percent(1.0),
                height: length(1.0),
            }
        };
        Style {
            size,
            align_self: Some(AlignItems::STRETCH),
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let color = cx.color(Role::Separator);
        cx.fill_rect(cx.bounds(), color);
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Splitter)
    }
}

/// The translucent materials of a [`VisualEffectView`] (`NSVisualEffectView.Material`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Material {
    /// Window sidebars: light blur, mostly see-through.
    #[default]
    Sidebar,
    /// Menus and popovers: stronger blur and tint.
    Popover,
    /// Heads-up displays: a dark material in either appearance.
    Hud,
    /// Title bars and toolbars over scrolled content.
    Titlebar,
}

impl Material {
    /// Blur radius (logical px) and tint in `theme`.
    pub fn look(self, theme: &crate::Theme) -> (f32, ui_scene::Color) {
        use crate::theme::Design;
        let (blur, tint) = self.big_sur_look(theme.appearance);
        match theme.design {
            Design::FlatVibrancy => (blur * 1.25, tint.with_alpha(tint.a * 0.85)),
            Design::BigSur | Design::Noa => (blur, tint),
            Design::LiquidGlass => (blur * 0.5, tint.with_alpha(tint.a * 0.55)),
            Design::Aero => (blur * 0.6, tint.with_alpha(tint.a * 0.7)),
            Design::Metro => (0.0, tint.with_alpha(1.0)),
            Design::Fluent2 => (blur * 1.2, tint.with_alpha(tint.a * 1.1)),
        }
    }

    fn big_sur_look(self, appearance: crate::theme::Appearance) -> (f32, ui_scene::Color) {
        use ui_scene::Color;
        let dark = appearance == crate::theme::Appearance::Dark;
        let base = if dark {
            Color::hex(0x1e1e22)
        } else {
            Color::hex(0xf6f6f8)
        };
        match self {
            Material::Sidebar => (20.0, base.with_alpha(0.55)),
            Material::Popover => (30.0, base.with_alpha(0.72)),
            Material::Hud => (30.0, Color::hex(0x141418).with_alpha(0.68)),
            Material::Titlebar => (24.0, base.with_alpha(0.8)),
        }
    }
}

/// Blurs whatever is drawn behind it (`NSVisualEffectView`). The frame is drawn offscreen
/// while one is visible, because the blur needs the renderer's backdrop pass.
#[derive(Debug, Clone)]
pub struct VisualEffectView {
    style: Style,
    material: Material,
    radius: f32,
}

impl VisualEffectView {
    pub fn new(style: Style, material: Material) -> Self {
        Self {
            style,
            material,
            radius: 0.0,
        }
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    pub fn set_material(&mut self, cx: &mut UpdateCx, material: Material) {
        if material != self.material {
            self.material = material;
            cx.request_paint();
        }
    }
}

impl Widget for VisualEffectView {
    fn style(&self) -> Style {
        self.style.clone()
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let (blur, tint) = self.material.look(cx.theme());
        let tint = cx.color(tint);
        cx.backdrop(cx.bounds(), self.radius, blur, tint);
        cx.sheen(cx.bounds(), self.radius);
        let edge = cx.color(Role::Separator);
        if self.radius > 0.0 {
            cx.rounded_rect(
                cx.bounds(),
                self.radius,
                ui_scene::Color::TRANSPARENT,
                Border {
                    width: 0.5,
                    color: edge,
                },
            );
        }
    }
}

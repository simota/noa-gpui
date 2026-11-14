use ui_layout::Style;
use ui_layout::style::{AlignItems, GridPlacement, Line, Size, auto, length, span};

use super::{Container, GridPlacementMode, GridTrack, GridView, Label};
use crate::theme::{Paint, Role};
use crate::tree::{UiTree, WidgetId};

/// The ids of what [`DescriptionList::insert`] built.
#[derive(Debug, Clone, PartialEq)]
pub struct DescriptionListParts {
    /// The two-column [`GridView`].
    pub root: WidgetId,
    pub labels: Vec<WidgetId>,
    pub values: Vec<WidgetId>,
    pub separators: Vec<WidgetId>,
    look: ItemLook,
}

impl DescriptionListParts {
    pub fn set_value(&self, tree: &mut UiTree, index: usize, text: impl Into<String>) {
        if let Some(&id) = self.values.get(index) {
            let text = text.into();
            tree.update::<Label, _>(id, |l, cx| l.set_text(cx, text));
        }
    }

    pub fn push(&mut self, tree: &mut UiTree, label: impl Into<String>, value: impl Into<String>) {
        if self.look.separators && !self.labels.is_empty() {
            let line = tree.insert(Some(self.root), separator());
            self.separators.push(line);
        }
        let (l, v) = insert_item(tree, self.root, &self.look, label.into(), value.into());
        self.labels.push(l);
        self.values.push(v);
    }
}

/// Label and value pairs in two columns, like an HTML `<dl>`. Build it with
/// [`DescriptionList::insert`].
#[derive(Debug, Clone, Default)]
pub struct DescriptionList {
    items: Vec<(String, String)>,
    label_width: Option<f32>,
    row_spacing: Option<f32>,
    look: ItemLook,
}

/// Kept to add more items with [`DescriptionListParts::push`].
#[derive(Debug, Clone, Default, PartialEq)]
struct ItemLook {
    separators: bool,
    family: Option<String>,
    size: Option<f32>,
    label_size: Option<f32>,
    label_color: Option<Paint>,
}

impl DescriptionList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn item(mut self, label: impl Into<String>, value: impl Into<String>) -> Self {
        self.items.push((label.into(), value.into()));
        self
    }

    pub fn items<L: Into<String>, V: Into<String>>(
        mut self,
        items: impl IntoIterator<Item = (L, V)>,
    ) -> Self {
        self.items
            .extend(items.into_iter().map(|(l, v)| (l.into(), v.into())));
        self
    }

    /// A fixed width for the label column, to line several lists up.
    pub fn label_width(mut self, width: f32) -> Self {
        self.label_width = Some(width);
        self
    }

    /// 6 by default.
    pub fn row_spacing(mut self, spacing: f32) -> Self {
        self.row_spacing = Some(spacing);
        self
    }

    /// Off by default.
    pub fn separators(mut self, separators: bool) -> Self {
        self.look.separators = separators;
        self
    }

    /// A family name, as for [`ui_text::TextStore::font_family`].
    pub fn value_font_family(mut self, family: impl Into<String>) -> Self {
        self.look.family = Some(family.into());
        self
    }

    /// Logical pixels; the labels' size by default.
    pub fn value_font_size(mut self, px: f32) -> Self {
        self.look.size = Some(px.max(0.0));
        self
    }

    /// Logical pixels; the theme's text size by default.
    pub fn label_font_size(mut self, px: f32) -> Self {
        self.look.label_size = Some(px.max(0.0));
        self
    }

    /// [`Role::Muted`] by default.
    pub fn label_color(mut self, color: impl Into<Paint>) -> Self {
        self.look.label_color = Some(color.into());
        self
    }

    /// Add it as the last child of `parent`, filling its width.
    pub fn insert(self, tree: &mut UiTree, parent: WidgetId) -> DescriptionListParts {
        let label_column = self.label_width.map_or(GridTrack::Auto, GridTrack::Fixed);
        let root = tree.insert(
            Some(parent),
            GridView::new([label_column, GridTrack::Flexible(1.0)])
                .row_spacing(self.row_spacing.unwrap_or(6.0))
                .column_spacing(16.0)
                .y_placement(GridPlacementMode::Leading)
                .fill_width(true),
        );
        let mut parts = DescriptionListParts {
            root,
            labels: Vec::with_capacity(self.items.len()),
            values: Vec::with_capacity(self.items.len()),
            separators: Vec::new(),
            look: self.look,
        };
        for (label, value) in self.items {
            parts.push(tree, label, value);
        }
        parts
    }
}

fn separator() -> Container {
    Container::new(Style {
        grid_column: Line {
            start: span(2),
            end: GridPlacement::Auto,
        },
        justify_self: Some(AlignItems::STRETCH),
        size: Size {
            width: auto(),
            height: length(1.0),
        },
        ..Default::default()
    })
    .background(Role::Separator)
}

fn insert_item(
    tree: &mut UiTree,
    grid: WidgetId,
    look: &ItemLook,
    label: String,
    value: String,
) -> (WidgetId, WidgetId) {
    let mut label = Label::new(label).color(look.label_color.unwrap_or(Role::Muted.into()));
    if let Some(size) = look.label_size {
        label = label.font_size(size);
    }
    let label = tree.insert(Some(grid), label);
    let mut value = Label::new(value).wrap(true);
    if let Some(family) = &look.family {
        value = value.font_family(family.clone());
    }
    if let Some(size) = look.size {
        value = value.font_size(size);
    }
    let value = tree.insert(Some(grid), value);
    (label, value)
}

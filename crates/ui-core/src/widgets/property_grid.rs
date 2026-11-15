use noa_gpui_scene::Color;

use super::{
    ColorChanged, ColorWell, ControlSize, GridCell, GridPlacementMode, GridTrack, GridView, Label,
    NumberInput, PopUpButton, SelectionChanged, Switch, Toggled, ValueChanged,
};
use crate::event::Message;
use crate::theme::Role;
use crate::tree::{UiTree, WidgetId};

#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    Number(f64),
    Color(Color),
    Bool(bool),
    /// An index into the property's options.
    Choice(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct PropertyChanged {
    /// The property's index, in the order the properties were added.
    pub index: usize,
    pub value: PropertyValue,
}

#[derive(Debug, Clone)]
struct Property {
    name: String,
    label: WidgetId,
    editor: WidgetId,
    value: PropertyValue,
}

/// An inspector of named properties, each edited by the control for its type. Not a
/// widget: it builds a [`GridView`] into the tree and keeps the ids; pass the tree's
/// messages to [`PropertyGrid::handle`].
#[derive(Debug, Clone)]
pub struct PropertyGrid {
    grid: WidgetId,
    properties: Vec<Property>,
    sections: Vec<WidgetId>,
}

impl PropertyGrid {
    /// An empty inspector filling the width of `parent`. `name_width` fixes the width
    /// of the names' column; `None` makes it as wide as the longest name.
    pub fn new(tree: &mut UiTree, parent: WidgetId, name_width: Option<f32>) -> Self {
        let names = name_width.map_or(GridTrack::Auto, GridTrack::Fixed);
        let grid = tree.insert(
            Some(parent),
            GridView::new([names, GridTrack::Flexible(1.0)])
                .row_spacing(6.0)
                .column_spacing(12.0)
                .y_placement(GridPlacementMode::Center)
                .fill_width(true),
        );
        Self {
            grid,
            properties: Vec::new(),
            sections: Vec::new(),
        }
    }

    pub fn grid(&self) -> WidgetId {
        self.grid
    }

    /// A heading across both columns above the properties added after it.
    pub fn add_section(&mut self, tree: &mut UiTree, title: impl Into<String>) -> WidgetId {
        let cell = tree.insert(Some(self.grid), GridCell::new(2, 1));
        let heading = tree.insert(
            Some(cell),
            Label::new(title)
                .font_size(11.0)
                .weight(700)
                .color(Role::Muted),
        );
        self.sections.push(heading);
        heading
    }

    pub fn add_number(
        &mut self,
        tree: &mut UiTree,
        name: impl Into<String>,
        min: f64,
        max: f64,
        value: f64,
    ) -> usize {
        self.add_number_with(tree, name, NumberInput::new(min, max, value))
    }

    pub fn add_number_with(
        &mut self,
        tree: &mut UiTree,
        name: impl Into<String>,
        input: NumberInput,
    ) -> usize {
        let value = PropertyValue::Number(input.value());
        self.add(tree, name.into(), value, input)
    }

    pub fn add_color(&mut self, tree: &mut UiTree, name: impl Into<String>, color: Color) -> usize {
        let well = ColorWell::new(color).control_size(ControlSize::Small);
        self.add(tree, name.into(), PropertyValue::Color(color), well)
    }

    pub fn add_bool(&mut self, tree: &mut UiTree, name: impl Into<String>, on: bool) -> usize {
        let switch = Switch::new(on).control_size(ControlSize::Small);
        self.add(tree, name.into(), PropertyValue::Bool(on), switch)
    }

    pub fn add_choice<S: Into<String>>(
        &mut self,
        tree: &mut UiTree,
        name: impl Into<String>,
        options: impl IntoIterator<Item = S>,
        selected: usize,
    ) -> usize {
        let popup = PopUpButton::new(options)
            .selected_index(selected)
            .control_size(ControlSize::Small);
        let selected = popup.selected();
        self.add(tree, name.into(), PropertyValue::Choice(selected), popup)
    }

    fn add(
        &mut self,
        tree: &mut UiTree,
        name: String,
        value: PropertyValue,
        editor: impl crate::Widget,
    ) -> usize {
        let label = tree.insert(Some(self.grid), Label::new(name.clone()).color(Role::Muted));
        let editor = tree.insert(Some(self.grid), editor);
        self.properties.push(Property {
            name,
            label,
            editor,
            value,
        });
        self.properties.len() - 1
    }

    pub fn len(&self) -> usize {
        self.properties.len()
    }

    pub fn is_empty(&self) -> bool {
        self.properties.is_empty()
    }

    pub fn name(&self, index: usize) -> Option<&str> {
        self.properties.get(index).map(|p| p.name.as_str())
    }

    pub fn value(&self, index: usize) -> Option<&PropertyValue> {
        self.properties.get(index).map(|p| &p.value)
    }

    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.properties.iter().position(|p| p.name == name)
    }

    pub fn editor(&self, index: usize) -> Option<WidgetId> {
        self.properties.get(index).map(|p| p.editor)
    }

    pub fn label(&self, index: usize) -> Option<WidgetId> {
        self.properties.get(index).map(|p| p.label)
    }

    /// Set a property from the app (nothing is reported). A value of another type
    /// than the property's is ignored. Returns whether it was set.
    pub fn set_value(&mut self, tree: &mut UiTree, index: usize, value: PropertyValue) -> bool {
        let Some(property) = self.properties.get_mut(index) else {
            return false;
        };
        let editor = property.editor;
        let set = match value {
            PropertyValue::Number(v) => matches!(property.value, PropertyValue::Number(_))
                .then(|| tree.update::<NumberInput, _>(editor, |w, cx| w.set_value(cx, v))),
            PropertyValue::Color(c) => matches!(property.value, PropertyValue::Color(_))
                .then(|| tree.update::<ColorWell, _>(editor, |w, cx| w.set_color(cx, c))),
            PropertyValue::Bool(on) => matches!(property.value, PropertyValue::Bool(_))
                .then(|| tree.update::<Switch, _>(editor, |w, cx| w.set_on(cx, on))),
            PropertyValue::Choice(i) => matches!(property.value, PropertyValue::Choice(_))
                .then(|| tree.update::<PopUpButton, _>(editor, |w, cx| w.set_selected(cx, i))),
        };
        if set.flatten().is_none() {
            return false;
        }
        // Read the value back: the controls clamp and round what they are given.
        property.value = current_value(tree, editor, &property.value);
        // A choice is not clamped: one the control does not have leaves it as it was.
        !matches!(value, PropertyValue::Choice(_)) || property.value == value
    }

    /// Follow the user editing a property: returns the change if `message` came from
    /// one of the controls and changed its value.
    pub fn handle(&mut self, message: &Message) -> Option<PropertyChanged> {
        let index = self
            .properties
            .iter()
            .position(|p| p.editor == message.source)?;
        let property = &mut self.properties[index];
        let value = match property.value {
            PropertyValue::Number(_) => {
                PropertyValue::Number(message.downcast_ref::<ValueChanged>()?.0)
            }
            PropertyValue::Color(_) => {
                PropertyValue::Color(message.downcast_ref::<ColorChanged>()?.0)
            }
            PropertyValue::Bool(_) => PropertyValue::Bool(message.downcast_ref::<Toggled>()?.0),
            PropertyValue::Choice(_) => {
                PropertyValue::Choice(message.downcast_ref::<SelectionChanged>()?.0)
            }
        };
        if value == property.value {
            return None;
        }
        property.value = value.clone();
        Some(PropertyChanged { index, value })
    }
}

fn current_value(tree: &UiTree, editor: WidgetId, old: &PropertyValue) -> PropertyValue {
    let read = match old {
        PropertyValue::Number(_) => tree
            .get::<NumberInput>(editor)
            .map(|w| PropertyValue::Number(w.value())),
        PropertyValue::Color(_) => tree
            .get::<ColorWell>(editor)
            .map(|w| PropertyValue::Color(w.color())),
        PropertyValue::Bool(_) => tree
            .get::<Switch>(editor)
            .map(|w| PropertyValue::Bool(w.is_on())),
        PropertyValue::Choice(_) => tree
            .get::<PopUpButton>(editor)
            .map(|w| PropertyValue::Choice(w.selected())),
    };
    read.unwrap_or_else(|| old.clone())
}

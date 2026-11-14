//! The color and font panels are drawn here rather than borrowed from the OS: the Web
//! and Linux have no such panels, and AppKit's are wired to native text views through
//! the responder chain, which GPU-drawn widgets are not part of. Open and save panels
//! use the OS's own (see the demo app).

use ui_layout::Style;
use ui_layout::style::{AlignItems, FlexDirection, Size as LayoutSize, length};
use ui_scene::{Color, Point};
use ui_text::TextStore;

use super::{
    ColorChanged, ColorPicker, Container, Label, Panel, PopUpButton, RowSelectionChanged,
    SelectionChanged, SelectionMode, TableColumn, TableView,
};
use crate::event::Message;
use crate::theme::Role;
use crate::tree::{UiTree, WidgetId};

/// A color panel (`NSColorPanel`): a [`ColorPicker`] in a floating panel.
#[derive(Debug, Clone, Copy)]
pub struct ColorPanel {
    panel: WidgetId,
    picker: WidgetId,
}

impl ColorPanel {
    /// Open with its top-left corner at `at` (window coordinates).
    pub fn open(tree: &mut UiTree, color: Color, at: Point) -> Self {
        let panel = Panel::new("Colors").width(244.0).open(tree, at);
        let picker = tree.insert(Some(panel), ColorPicker::new(color));
        Self { panel, picker }
    }

    pub fn panel(&self) -> WidgetId {
        self.panel
    }

    pub fn is_open(&self, tree: &UiTree) -> bool {
        tree.contains(self.panel)
    }

    /// `None` unless `message` is the panel's color changing.
    pub fn handle(&self, message: &Message) -> Option<Color> {
        (message.source == self.picker)
            .then(|| message.downcast_ref::<ColorChanged>())
            .flatten()
            .map(|c| c.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FontChoice {
    pub family: String,
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
}

impl FontChoice {
    pub fn weight(&self) -> u16 {
        if self.bold { 700 } else { 400 }
    }
}

const SIZES: [f32; 12] = [
    9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 18.0, 24.0, 36.0, 48.0, 64.0, 72.0,
];
const TYPEFACES: [&str; 4] = ["Regular", "Italic", "Bold", "Bold Italic"];

/// A font panel (`NSFontPanel`) in a floating panel. The app passes it the messages it
/// gets; [`FontPanel::handle`] returns the new font when a control changed it.
#[derive(Debug)]
pub struct FontPanel {
    panel: WidgetId,
    list: WidgetId,
    typeface: WidgetId,
    size: WidgetId,
    preview: WidgetId,
    families: Vec<String>,
    choice: FontChoice,
}

impl FontPanel {
    /// Open with its top-left corner at `at`, showing `choice`. The list holds the
    /// families loaded in `text` (and `choice`'s family if it is not loaded).
    pub fn open(tree: &mut UiTree, text: &TextStore, choice: FontChoice, at: Point) -> Self {
        let mut families = text.families();
        if !families.contains(&choice.family) {
            families.push(choice.family.clone());
            families.sort_by_cached_key(|f| f.to_lowercase());
        }
        let panel = Panel::new("Fonts").width(292.0).open(tree, at);
        let rows: Vec<Vec<String>> = families.iter().map(|f| vec![f.clone()]).collect();
        let list = tree.insert(
            Some(panel),
            TableView::new(vec![TableColumn::new("Family", 250.0)], rows)
                .header(false)
                .selection_mode(SelectionMode::Single)
                .width(268.0)
                .height(150.0),
        );
        if let Some(row) = families.iter().position(|f| *f == choice.family) {
            tree.update::<TableView, _>(list, |t, cx| t.select_rows(cx, [row]));
        }
        let controls = tree.insert(
            Some(panel),
            Container::new(Style {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::CENTER,
                gap: LayoutSize {
                    width: length(8.0),
                    height: length(0.0),
                },
                ..Default::default()
            }),
        );
        let face = usize::from(choice.italic) + 2 * usize::from(choice.bold);
        let typeface = tree.insert(
            Some(controls),
            PopUpButton::new(TYPEFACES).selected_index(face),
        );
        let size_index = SIZES
            .iter()
            .position(|&s| s >= choice.size)
            .unwrap_or(SIZES.len() - 1);
        let size = tree.insert(
            Some(controls),
            PopUpButton::new(SIZES.iter().map(|s| format!("{s} pt"))).selected_index(size_index),
        );
        tree.insert(
            Some(panel),
            Label::new("Preview").font_size(11.0).color(Role::Muted),
        );
        let preview = tree.insert(Some(panel), Label::new("The quick brown fox 0123"));
        let mut this = Self {
            panel,
            list,
            typeface,
            size,
            preview,
            families,
            choice,
        };
        this.choice.size = SIZES[size_index];
        this.update_preview(tree);
        this
    }

    pub fn panel(&self) -> WidgetId {
        self.panel
    }

    pub fn choice(&self) -> &FontChoice {
        &self.choice
    }

    pub fn is_open(&self, tree: &UiTree) -> bool {
        tree.contains(self.panel)
    }

    pub fn handle(&mut self, tree: &mut UiTree, message: &Message) -> Option<FontChoice> {
        let before = self.choice.clone();
        if message.source == self.list && message.downcast_ref::<RowSelectionChanged>().is_some() {
            let row = tree
                .get::<TableView>(self.list)
                .and_then(|t| t.selected_rows().first().copied())?;
            self.choice.family = self.families.get(row)?.clone();
        } else if message.source == self.typeface {
            let SelectionChanged(face) = *message.downcast_ref::<SelectionChanged>()?;
            self.choice.italic = face % 2 == 1;
            self.choice.bold = face >= 2;
        } else if message.source == self.size {
            let SelectionChanged(index) = *message.downcast_ref::<SelectionChanged>()?;
            self.choice.size = *SIZES.get(index)?;
        } else {
            return None;
        }
        if self.choice == before {
            return None;
        }
        self.update_preview(tree);
        Some(self.choice.clone())
    }

    fn update_preview(&self, tree: &mut UiTree) {
        let c = &self.choice;
        // The preview stays readable at large sizes.
        let size = c.size.min(36.0);
        tree.update::<Label, _>(self.preview, |label, cx| {
            label.set_font(cx, Some(c.family.clone()), size, c.weight(), c.italic);
        });
    }
}

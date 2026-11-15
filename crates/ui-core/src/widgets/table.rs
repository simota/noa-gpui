use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::hash::Hash;
use std::ops::Range;

use noa_gpui_layout::Style;
use noa_gpui_layout::style::{Dimension, Size as LayoutSize, length, percent};
use noa_gpui_scene::{Border, Color, Direction, Point, Rect, Size};
use noa_gpui_text::{FontFamily, TextStore, TextStyle};

use super::inline_label::{RenameStep, Renamer};
use super::rows::{
    Selection, SelectionMode, TextCache, TextQuery, contains_ignoring_case, pinned_group,
    reveal_rect, step,
};
use super::scroller::Scroller;
use super::toggles::{CheckState, paint_check_box};
use super::tsv::TableText;
use super::{CachedText, Icon, MenuItem, Tone, Truncation, focus_ring, view_height};
use crate::event::{Cursor, Event, Key, Modifiers, PointerButton};
use crate::theme::{Corner, Paint, Role, mix};
use crate::view_state;
use crate::widget::{EventCx, PaintCx, UpdateCx, Widget};

pub const TABLE_HEADER: f32 = 28.0;
/// Default row height.
pub const TABLE_ROW: f32 = 24.0;
/// Default space between a cell's edge and its text (see [`TableView::cell_padding`]).
pub const TABLE_CELL_PADDING: f32 = 8.0;
pub const TABLE_CHECK_COLUMN: f32 = 32.0;
const CHECK_BOX: f32 = 16.0;
const BADGE_PAD: f32 = 6.0;
const BUTTON_PAD: f32 = 8.0;
/// Size of the icon in a [`CellButton`], the space between it and the button's label,
/// and the space between the buttons of a cell.
const BUTTON_ICON: f32 = 14.0;
const BUTTON_ICON_GAP: f32 = 4.0;
const BUTTON_GAP: f32 = 4.0;
const MEMBER_RULE: f32 = 2.0;
/// Half the width of the area around a column edge that resizes the column.
const RESIZE_GRAB: f32 = 4.0;
/// Pointer travel that turns a press on a row into a drag.
const DRAG_START: f32 = 4.0;
/// How close to the top or bottom of the rows a drag starts scrolling them.
const AUTOSCROLL_EDGE: f32 = 24.0;
/// How fast a drag at (or past) the edge scrolls, in pixels per second.
const AUTOSCROLL_SPEED: f32 = 900.0;
/// Seconds within which a second press counts as a double click.
const DOUBLE_CLICK: f64 = 0.4;
/// Seconds of not typing after which typing starts a new search for a row.
const TYPE_SELECT_PAUSE: f64 = 1.0;
const DETAIL_GAP: f32 = 1.0;
/// The text cache's part for a cell's second line (0 is its text, 1 + i a button's
/// label).
const DETAIL_PART: usize = usize::MAX;
/// The text cache's part for a cell's note ([`TableData::cell_note`]).
const NOTE_PART: usize = usize::MAX - 1;
const NOTE_GAP: f32 = 6.0;
/// Diameter of a cell's dot ([`TableData::cell_dot`]) and the space after it.
const DOT: f32 = 8.0;
const DOT_GAP: f32 = 6.0;
/// The text cache's column for a group row's own buttons.
const GROUP_BUTTONS: usize = usize::MAX;
/// Length of a dash and of the gap after it in a [`RowSeparator::Dashed`] line.
const DASH: f32 = 4.0;
const DASH_GAP: f32 = 3.0;
/// The displayed position of a data row the filter hides.
const HIDDEN: usize = usize::MAX;
/// How many changes to a data grid's cells Cmd+Z can take back.
const UNDO_LIMIT: usize = 100;

/// The rows a [`TableView`] shows. Only the cells on screen are asked for, so a source
/// can compute or load them lazily.
pub trait TableData {
    fn row_count(&self) -> usize;
    fn cell(&self, row: usize, column: usize) -> Cow<'_, str>;

    /// Whether `row` is a group row (`NSTableView` group rows), heading the rows up to the
    /// next group row. Asked once per row when the data is set.
    fn is_group_row(&self, _row: usize) -> bool {
        false
    }

    /// Store `text` as the cell's new content after it was edited in the table (see
    /// [`TableColumn::editable`]). Return whether it was stored; sources that keep
    /// their data elsewhere leave it to the app, which gets [`CellEdited`] either way.
    fn set_cell(&mut self, _row: usize, _column: usize, _text: &str) -> bool {
        false
    }

    /// Plain text by default. Asked for the cells on screen each time they are drawn.
    fn cell_look(&self, _row: usize, _column: usize) -> CellLook {
        CellLook::Text
    }

    /// In the order shown. Asked for the cells on screen each time they are drawn, so
    /// the set can follow the row's state.
    fn cell_buttons(&self, _row: usize, _column: usize) -> Vec<CellButton> {
        Vec::new()
    }

    /// A second, smaller line under the cell's text, or `None` for a one-line cell (the
    /// default). Shown for [`CellLook::Text`] and [`CellLook::Colored`] cells; give the
    /// table a [`TableView::row_height`] that fits both lines (about 36). Asked for the
    /// cells on screen each time they are drawn.
    fn cell_detail(&self, _row: usize, _column: usize) -> Option<Cow<'_, str>> {
        None
    }

    /// Smaller text right after the cell's text on the same line, or `None` (the
    /// default). Shown for [`CellLook::Text`] and [`CellLook::Colored`] cells and for a
    /// group row's heading; it takes what room the text leaves and is shortened first.
    /// Asked for the cells on screen each time they are drawn.
    fn cell_note(&self, _row: usize, _column: usize) -> Option<Cow<'_, str>> {
        None
    }

    /// A dot before the cell's content in the tone's color, or `None` (the default).
    /// Also drawn before a group row's heading. Asked for the cells on screen each time
    /// they are drawn.
    fn cell_dot(&self, _row: usize, _column: usize) -> Option<Tone> {
        None
    }

    /// The buttons of group row `row` itself, at the trailing edge of the view whatever
    /// the columns are (none by default). Asked for the group rows on screen each time
    /// they are drawn.
    fn group_row_buttons(&self, _row: usize) -> Vec<CellButton> {
        Vec::new()
    }

    /// The line under `row` when the table draws [`TableView::row_separators`]: a
    /// solid line by default, a dashed one to set a row apart (a stopped group), or
    /// none.
    fn row_separator(&self, _row: usize) -> RowSeparator {
        RowSeparator::Line
    }

    /// The number a [`ColumnSort::Number`] column sorts the cell by, or `None` to put
    /// the row last. By default the number the cell's text starts with; give the
    /// value itself when the text does not sort by it ("900 KB" against "1.2 GB", "2
    /// hours ago"). Asked for every row when the table sorts.
    fn sort_number(&self, row: usize, column: usize) -> Option<f64> {
        leading_number(&self.cell(row, column))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RowSeparator {
    #[default]
    Line,
    Dashed,
    None,
}

/// Emitted when a group row was clicked anywhere but on a button, with its data row:
/// the place to expand or collapse the group (the table leaves its rows as they are).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupRowClicked(pub usize);

/// Emitted when a button of [`TableData::group_row_buttons`] was clicked: the group's
/// data row and the button's [`CellButton::new`] action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupActionClicked {
    pub row: usize,
    pub action: Cow<'static, str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CellLook {
    #[default]
    Text,
    /// Text in the tone's color (on a selected row, the selection's text color).
    Colored(Tone),
    Badge(Tone),
    OutlinedBadge(Tone),
    /// Like [`CellLook::OutlinedBadge`], with the frame and the text in the tone's
    /// color itself. The text is not pulled toward the text color, so a light tone
    /// (yellow) can read below 4.5:1 on a light background.
    StrongOutlinedBadge(Tone),
    /// A button showing the text. A click on it emits [`CellButtonClicked`] and leaves
    /// the selection as it is.
    Button,
    /// A row of buttons from [`TableData::cell_buttons`] instead of the text. A click on
    /// one emits [`CellActionClicked`] and leaves the selection as it is; a click beside
    /// them selects the row as usual.
    Buttons,
    /// A placeholder bar instead of the text, for a row that is still being loaded
    /// (see [`super::RemoteRows`]). Read as "Loading" by assistive technology.
    Loading,
}

/// Emitted when a [`CellLook::Button`] cell was clicked: the data row and column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellButtonClicked {
    pub row: usize,
    pub column: usize,
}

/// One button of a [`CellLook::Buttons`] cell: an icon, a label or both.
#[derive(Debug, Clone, PartialEq)]
pub struct CellButton {
    action: Cow<'static, str>,
    label: Option<String>,
    icon: Option<Icon>,
    tone: Tone,
    enabled: bool,
    height: Option<f32>,
    tooltip: Option<String>,
}

impl CellButton {
    /// Reports `action` in [`CellActionClicked`] when clicked.
    pub fn new(action: impl Into<Cow<'static, str>>) -> Self {
        Self {
            action: action.into(),
            label: None,
            icon: None,
            tone: Tone::Neutral,
            enabled: true,
            height: None,
            tooltip: None,
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// The text color by default.
    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    /// Centered in the row; the row's height less 4 by default, and never more. An
    /// icon-only button is as wide as it is high.
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height.max(0.0));
        self
    }

    /// A disabled button is drawn dimmed and ignores clicks (enabled by default).
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Assistive technology reads the tooltip as the name of a button without a label,
    /// so give every icon-only button one.
    pub fn tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    pub fn action(&self) -> &str {
        &self.action
    }

    /// The name assistive technology reads: the label, or else the tooltip.
    fn name(&self) -> Option<&str> {
        self.label.as_deref().or(self.tooltip.as_deref())
    }
}

/// Emitted when a button of a [`CellLook::Buttons`] cell was clicked: the data row,
/// the column and the button's [`CellButton::new`] action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellActionClicked {
    pub row: usize,
    pub column: usize,
    pub action: Cow<'static, str>,
}

/// Where a button of a [`CellLook::Buttons`] cell was drawn by the last paint.
#[derive(Debug, Clone)]
struct ButtonHit {
    row: usize,
    /// `None` for a button of [`TableData::group_row_buttons`].
    column: Option<usize>,
    index: usize,
    /// The part of the button that showed (widget coordinates).
    rect: Rect,
    action: Cow<'static, str>,
}

impl TableData for Vec<Vec<String>> {
    fn row_count(&self) -> usize {
        self.len()
    }

    fn cell(&self, row: usize, column: usize) -> Cow<'_, str> {
        self.get(row)
            .and_then(|r| r.get(column))
            .map_or(Cow::Borrowed(""), |s| Cow::Borrowed(s.as_str()))
    }

    fn set_cell(&mut self, row: usize, column: usize, text: &str) -> bool {
        match self.get_mut(row).and_then(|r| r.get_mut(column)) {
            Some(cell) => {
                text.clone_into(cell);
                true
            }
            None => false,
        }
    }
}

type RowCondition = Box<dyn Fn(&dyn TableData, usize) -> bool>;

#[derive(Default)]
struct RowFilter {
    /// A row is shown when each word of the query is in one of its cells.
    query: TextQuery,
    /// The columns searched (every column when `None`).
    columns: Option<Vec<usize>>,
    keep: Option<RowCondition>,
    unmarked: bool,
}

impl RowFilter {
    fn is_active(&self) -> bool {
        !self.query.is_empty() || self.keep.is_some()
    }

    fn searched(&self, columns: usize) -> Vec<usize> {
        match &self.columns {
            Some(list) => list.iter().copied().filter(|&c| c < columns).collect(),
            None => (0..columns).collect(),
        }
    }

    /// Whether data row `row` (not a group row) is shown. `searched` is
    /// [`RowFilter::searched`]; `texts` is scratch space, reused from row to row.
    fn matches<'a>(
        &self,
        data: &'a dyn TableData,
        row: usize,
        searched: &[usize],
        texts: &mut Vec<Option<Option<Cow<'a, str>>>>,
    ) -> bool {
        if self.keep.as_ref().is_some_and(|keep| !keep(data, row)) {
            return false;
        }
        if self.query.is_empty() {
            return true;
        }
        // Cells are read as the words need them: a word found in the first column
        // does not read the others.
        texts.clear();
        texts.resize(2 * searched.len(), None);
        self.query.words().iter().all(|word| {
            (0..texts.len()).any(|i| {
                let (c, detail) = (searched[i / 2], i % 2 == 1);
                texts[i]
                    .get_or_insert_with(|| match detail {
                        false => Some(data.cell(row, c)),
                        true => data.cell_detail(row, c),
                    })
                    .as_deref()
                    .is_some_and(|text| contains_ignoring_case(text, word))
            })
        })
    }

    fn marks(&self, column: usize) -> bool {
        !self.unmarked
            && !self.query.is_empty()
            && self
                .columns
                .as_ref()
                .is_none_or(|list| list.contains(&column))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColumnAlign {
    #[default]
    Leading,
    Center,
    Trailing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColumnSort {
    #[default]
    None,
    /// Case-insensitive text order.
    Text,
    /// Numeric order of the number a cell starts with ("12 KB", "1,024"), or of
    /// [`TableData::sort_number`]; cells without a number go last.
    Number,
}

#[derive(Debug, Clone)]
pub struct TableColumn {
    title: String,
    width: f32,
    min_width: f32,
    /// Share of the width the columns leave in the view (0: a fixed width).
    flex: f32,
    /// How narrow the column may get when the view is too narrow for the columns
    /// ([`TableColumn::shrink_to`]); `None` keeps its width.
    shrink: Option<f32>,
    family: Option<String>,
    detail_family: Option<String>,
    align: ColumnAlign,
    sort: ColumnSort,
    resizable: bool,
    editable: bool,
    truncation: Truncation,
    /// The width was set by dragging the column's edge or with
    /// [`TableView::set_column_width`]: it is part of the table's saved state.
    resized: bool,
    hidden: bool,
    /// Whether the app made the column hidden: the saved state records the columns
    /// only when one differs.
    hidden_at_start: bool,
    hideable: bool,
}

impl TableColumn {
    pub fn new(title: impl Into<String>, width: f32) -> Self {
        Self {
            title: title.into(),
            width,
            min_width: 40.0,
            flex: 0.0,
            shrink: None,
            family: None,
            detail_family: None,
            align: ColumnAlign::Leading,
            sort: ColumnSort::None,
            resizable: true,
            editable: false,
            truncation: Truncation::Tail,
            resized: false,
            hidden: false,
            hidden_at_start: false,
            hideable: true,
        }
    }

    /// Let the column grow into the width the columns leave in the view, taking
    /// `weight` shares of it (as CSS `minmax(width, <weight>fr)`): the column is never
    /// narrower than its width (unless it can [`TableColumn::shrink_to`] less), and
    /// when the view is narrower than all the columns they scroll sideways. Dragging the
    /// column's edge gives it a fixed width again.
    pub fn flex(mut self, weight: f32) -> Self {
        self.flex = weight.max(0.0);
        self
    }

    /// Let the column get narrower than its width, down to `min`, when the view is
    /// too narrow for all the columns, before the table scrolls sideways (as CSS
    /// `minmax(min, width)`): the columns that can shrink give up width in proportion
    /// to how much each can lose, and their text is shortened with an ellipsis
    /// ([`TableColumn::truncation`]). Independent of [`TableColumn::min_width`],
    /// which only limits a drag. Dragging the column's edge gives it a fixed width
    /// again.
    pub fn shrink_to(mut self, min: f32) -> Self {
        self.shrink = Some(min.max(0.0));
        self
    }

    /// The column's [`TableColumn::flex`] weight (0 for a fixed width).
    pub fn flex_weight(&self) -> f32 {
        self.flex
    }

    /// By name (see [`noa_gpui_text::TextStore::font_family`]); the default sans-serif when
    /// not set.
    pub fn font_family(mut self, family: impl Into<String>) -> Self {
        self.family = Some(family.into());
        self
    }

    /// By name; the column's [`TableColumn::font_family`] when not set.
    pub fn detail_font_family(mut self, family: impl Into<String>) -> Self {
        self.detail_family = Some(family.into());
        self
    }

    /// The narrowest a drag can make the column (40 by default).
    pub fn min_width(mut self, min: f32) -> Self {
        self.min_width = min;
        self.width = self.width.max(min);
        self
    }

    pub fn align(mut self, align: ColumnAlign) -> Self {
        self.align = align;
        self
    }

    pub fn sort(mut self, sort: ColumnSort) -> Self {
        self.sort = sort;
        self
    }

    /// Whether dragging the header's right edge resizes the column (on by default).
    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    /// Only in a table with [`TableView::cell_selection`]; off by default. See
    /// [`CellEdited`].
    pub fn editable(mut self, editable: bool) -> Self {
        self.editable = editable;
        self
    }

    /// At the end by default. The pointer resting on a shortened cell shows the whole
    /// text in a tooltip.
    pub fn truncation(mut self, truncation: Truncation) -> Self {
        self.truncation = truncation;
        self
    }

    pub fn is_editable(&self) -> bool {
        self.editable
    }

    /// The person can show it from the header's menu ([`TableView::column_chooser`]).
    pub fn hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
        self.hidden_at_start = hidden;
        self
    }

    /// On by default. The app can still hide it with [`TableView::set_column_visible`].
    pub fn hideable(mut self, hideable: bool) -> Self {
        self.hideable = hideable;
        self
    }

    pub fn is_visible(&self) -> bool {
        !self.hidden
    }

    /// The width given to the column, or the width a drag left it at. A
    /// [`TableColumn::flex`] column can be wider on screen: see
    /// [`TableView::column_width`].
    pub fn width(&self) -> f32 {
        self.width
    }

    pub fn title(&self) -> &str {
        &self.title
    }
}

/// Emitted when a header click sorts the rows (the table has already reordered them).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SortChanged {
    pub column: usize,
    pub ascending: bool,
}

/// Emitted when a column was resized by dragging its header edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColumnResized {
    pub column: usize,
    pub width: f32,
}

/// Emitted when the person showed or hid a column from the header's menu (see
/// [`TableView::column_chooser`]). The table has already changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnVisibilityChanged {
    pub column: usize,
    pub visible: bool,
}

/// Emitted after rows were dragged to a new place (see [`TableView::reorderable`]):
/// `rows` (data rows, in their new order) now start at displayed position `to`.
///
/// A [`super::CollectionView`] emits it too ([`super::CollectionView::reorderable`]):
/// there `rows` are the items' numbers from before the move, and `to` is where the
/// first of them is now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowsMoved {
    pub rows: Vec<usize>,
    pub to: usize,
}

/// Emitted on a double click on a row, or Return, with the data row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowActivated(pub usize);

/// A rectangle of cells: displayed row positions and columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellRange {
    pub rows: Range<usize>,
    pub columns: Range<usize>,
}

impl CellRange {
    /// Whether the cell at displayed position `pos` and `column` is inside.
    pub fn contains(&self, pos: usize, column: usize) -> bool {
        self.rows.contains(&pos) && self.columns.contains(&column)
    }

    pub fn len(&self) -> usize {
        self.rows.len() * self.columns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Emitted when the focused cell or the selected range of cells changes (in a table
/// with [`TableView::cell_selection`]). `focus` is the data row and column of the
/// focused cell; `range` spans from the range's anchor to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellSelectionChanged {
    pub focus: (usize, usize),
    pub range: CellRange,
}

/// Emitted when a cell was edited in place and its text changed (see
/// [`TableColumn::editable`]). `stored` tells whether [`TableData::set_cell`] took
/// the text; if not, the app updates its data and gives it to the table with
/// [`TableView::update_data`], which keeps the selection and the undo history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellEdited {
    /// The data row.
    pub row: usize,
    pub column: usize,
    pub text: String,
    pub stored: bool,
}

#[derive(Debug, Clone)]
struct CellChange {
    row: usize,
    column: usize,
    before: String,
    after: String,
}

/// The changes to a data grid's cells that can be undone and redone. Each entry is
/// what one action changed (an edit, a paste, a cut, a clear), undone as a whole.
#[derive(Debug, Default)]
struct GridHistory {
    undo: Vec<Vec<CellChange>>,
    redo: Vec<Vec<CellChange>>,
    /// What the action under way has changed so far.
    pending: Vec<CellChange>,
}

impl GridHistory {
    /// Close the action under way: what it changed becomes the next thing to undo,
    /// and what was undone can no longer be redone.
    fn commit(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        self.undo.push(std::mem::take(&mut self.pending));
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.pending.clear();
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Press {
    None,
    Header(usize),
    Resize {
        column: usize,
        x: f32,
        width: f32,
    },
    Row {
        at: Point,
        /// The press kept a multiple selection; without a drag the click applies on
        /// release.
        click: bool,
        dragging: bool,
        /// Displayed position the rows would be dropped before.
        drop: usize,
    },
    Cells,
    Button {
        row: usize,
        column: usize,
    },
    /// Pressing button `index` of a [`CellLook::Buttons`] cell, or of a group row's
    /// own buttons (no column).
    Action {
        row: usize,
        column: Option<usize>,
        index: usize,
    },
    Group(usize),
    /// Pressing the checkbox of a data row, or the header's (`None`).
    Check(Option<usize>),
}

/// Rows of cells under column headers (`NSTableView`).
///
/// Only the rows inside the view are drawn and only their cells' text is shaped, so
/// scrolling and selecting cost the same with 100 or 100,000 rows. The data stays in
/// the [`TableData`]; the table keeps the displayed order and a selection mark per row.
pub struct TableView {
    data: Box<dyn TableData>,
    columns: Vec<TableColumn>,
    /// Each column's width on screen: its own, plus its share of the room left over
    /// for a flexible column ([`TableView::layout_columns`]).
    widths: Vec<f32>,
    /// Each column's font, resolved by the first paint.
    families: Option<Vec<Option<FontFamily>>>,
    /// Each column's font for the second line of its cells, resolved with `families`.
    detail_families: Vec<Option<FontFamily>>,
    /// Where the pointer is over the table (widget coordinates), if it is.
    hover: Option<Point>,
    hover_rows: bool,
    group_cells: bool,
    member_indent: f32,
    member_indent_column: usize,
    header_uppercase: bool,
    header_family: Option<String>,
    header_family_id: Option<FontFamily>,
    header_fill: Option<Paint>,
    bordered: bool,
    /// The table's background (the surface color when `None`).
    fill: Option<Paint>,
    row_separators: bool,
    content_inset: f32,
    cell_pad: f32,
    /// The buttons of [`CellLook::Buttons`] cells the last paint drew.
    hits: Vec<ButtonHit>,
    /// Data rows in displayed order: those of `all` that the filter lets through.
    order: Vec<usize>,
    /// Every data row in sorted (or dragged) order, shown or not.
    all: Vec<usize>,
    filter: RowFilter,
    groups: Vec<bool>,
    /// Displayed positions of the group rows, ascending.
    group_at: Vec<usize>,
    floats_group_rows: bool,
    sort: Option<(usize, bool)>,
    /// Whether the table orders the rows itself when sorted (see
    /// [`TableView::sorts_rows`]).
    sorts_rows: bool,
    selection: Selection,
    checkboxes: bool,
    scroller: Scroller,
    row_height: f32,
    alternating: bool,
    header: bool,
    reorderable: bool,
    width: Dimension,
    /// `None` fills the parent ([`TableView::fill_height`]).
    height: Option<f32>,
    /// Keyed by data row, column and part: 0 for the text, 1 + i for button i's label.
    cells: TextCache<(usize, usize, usize)>,
    /// Cells (data row, column) drawn shortened by the last paint: their tooltip is the
    /// whole text.
    shortened: HashSet<(usize, usize)>,
    /// While rows are dragged, or cells selected by dragging, near the top or bottom:
    /// the pointer (widget coordinates) and the time of the last scroll step.
    autoscroll: Option<(Point, f64)>,
    titles: Vec<CachedText>,
    press: Press,
    last_click: Option<(f64, usize)>,
    size: Size,
    /// A data row (and column, for a cell) to scroll to once the table has a size.
    pending_reveal: Option<(usize, Option<usize>)>,
    /// Displayed position of each data row (the inverse of `order`).
    position: Vec<usize>,
    cell_mode: bool,
    /// The focused cell and the anchor of the selected range, as (data row, column).
    cell_focus: Option<(usize, usize)>,
    cell_anchor: Option<(usize, usize)>,
    frozen: usize,
    frozen_trailing: usize,
    /// Edits a cell in place: the item is the data row, `edit_column` the column.
    editor: Renamer,
    edit_column: usize,
    /// What was typed to select a row by its text, and when the last of it was typed.
    typed: String,
    typed_at: f64,
    history: GridHistory,
    column_chooser: bool,
}

impl std::fmt::Debug for TableView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TableView")
            .field("rows", &self.order.len())
            .field("columns", &self.columns.len())
            .field("sort", &self.sort)
            .finish_non_exhaustive()
    }
}

impl TableView {
    pub fn new(columns: Vec<TableColumn>, data: impl TableData + 'static) -> Self {
        let rows = data.row_count();
        let mut scroller = Scroller::new();
        scroller.horizontal = true;
        let groups = (0..rows).map(|r| data.is_group_row(r)).collect();
        let mut table = Self {
            data: Box::new(data),
            titles: (0..columns.len()).map(|_| CachedText::default()).collect(),
            widths: columns.iter().map(|c| c.width).collect(),
            columns,
            families: None,
            detail_families: Vec::new(),
            hover: None,
            hover_rows: false,
            group_cells: false,
            member_indent: 0.0,
            member_indent_column: 0,
            header_uppercase: false,
            header_family: None,
            header_family_id: None,
            header_fill: None,
            bordered: true,
            fill: None,
            row_separators: false,
            content_inset: 0.0,
            cell_pad: TABLE_CELL_PADDING,
            hits: Vec::new(),
            order: (0..rows).collect(),
            all: (0..rows).collect(),
            filter: RowFilter::default(),
            groups,
            group_at: Vec::new(),
            floats_group_rows: true,
            sort: None,
            sorts_rows: true,
            selection: Selection::new(SelectionMode::Multiple, rows),
            checkboxes: false,
            scroller,
            row_height: TABLE_ROW,
            alternating: true,
            header: true,
            reorderable: false,
            width: percent(1.0),
            height: Some(320.0),
            cells: TextCache::default(),
            shortened: HashSet::new(),
            autoscroll: None,
            press: Press::None,
            last_click: None,
            size: Size::default(),
            pending_reveal: None,
            position: Vec::new(),
            cell_mode: false,
            cell_focus: None,
            cell_anchor: None,
            frozen: 0,
            frozen_trailing: 0,
            editor: Renamer::default(),
            edit_column: 0,
            typed: String::new(),
            typed_at: f64::NEG_INFINITY,
            history: GridHistory::default(),
            column_chooser: false,
        };
        table.index_groups();
        table
    }

    /// Keep the group row being scrolled through pinned to the top of the view (on by
    /// default; see [`TableData::is_group_row`]).
    pub fn floats_group_rows(mut self, floats: bool) -> Self {
        self.floats_group_rows = floats;
        self
    }

    pub fn is_group_row(&self, row: usize) -> bool {
        self.groups.get(row).copied().unwrap_or(false)
    }

    /// Draw group rows like other rows, with every column's cell and its
    /// [`TableData::cell_look`] (buttons included), in bold on the group's band,
    /// instead of only the first cell's text across the columns (off by default).
    pub fn group_row_cells(mut self, cells: bool) -> Self {
        self.group_cells = cells;
        self
    }

    /// Indent the first cell of the rows under a group row by `indent`, with a rule in
    /// the indent that runs down the group (0, no indent, by default). See
    /// [`TableView::group_member_indent_column`] to indent another column.
    pub fn group_member_indent(mut self, indent: f32) -> Self {
        self.member_indent = indent.max(0.0);
        self
    }

    /// The first column (0) by default.
    pub fn group_member_indent_column(mut self, column: usize) -> Self {
        self.member_indent_column = column;
        self
    }

    /// Tint the row under the pointer (off by default).
    pub fn highlights_hovered_row(mut self, highlights: bool) -> Self {
        self.hover_rows = highlights;
        self
    }

    /// The data row under the pointer, if the pointer is over a row that is not a
    /// group row.
    pub fn hovered_row(&self) -> Option<usize> {
        let pos = self.position_at(self.hover?)?;
        let id = self.order[pos];
        (!self.groups[id]).then_some(id)
    }

    /// Show the column titles in uppercase (off by default).
    pub fn header_uppercase(mut self, uppercase: bool) -> Self {
        self.header_uppercase = uppercase;
        self
    }

    pub fn header_font_family(mut self, family: impl Into<String>) -> Self {
        self.header_family = Some(family.into());
        self
    }

    /// The header's background (the table's [`TableView::background`] by default).
    pub fn header_background(mut self, fill: impl Into<Paint>) -> Self {
        self.header_fill = Some(fill.into());
        self
    }

    /// On by default.
    pub fn bordered(mut self, bordered: bool) -> Self {
        self.bordered = bordered;
        self
    }

    /// The table's background under the rows and the header (the surface color by
    /// default).
    pub fn background(mut self, fill: impl Into<Paint>) -> Self {
        self.fill = Some(fill.into());
        self
    }

    /// Draw a thin line under each row, as [`TableData::row_separator`] says (off by
    /// default).
    pub fn row_separators(mut self, separators: bool) -> Self {
        self.row_separators = separators;
        self
    }

    /// 0 by default. Row backgrounds, the selection and the lines under rows still run
    /// from side to side.
    pub fn content_inset(mut self, inset: f32) -> Self {
        self.content_inset = inset.max(0.0);
        self
    }

    /// Also applies to the header's titles. [`TABLE_CELL_PADDING`] (8) by default.
    pub fn cell_padding(mut self, padding: f32) -> Self {
        self.cell_pad = padding.max(0.0);
        self
    }

    /// The width column `column` takes on screen after the last layout: its
    /// [`TableColumn::width`] plus, for a [`TableColumn::flex`] column, its share of
    /// the room left in the view.
    pub fn column_width(&self, column: usize) -> f32 {
        self.widths[column]
    }

    /// Give column `column` a fixed `width` (at least its
    /// [`TableColumn::min_width`]), as dragging its edge does: a flexible or shrinking
    /// column stops being one. The width is part of the table's saved state (see
    /// [`crate::UiTree::set_autosave_name`]). Nothing is emitted.
    pub fn set_column_width(&mut self, cx: &mut UpdateCx, column: usize, width: f32) {
        let Some(col) = self.columns.get_mut(column) else {
            return;
        };
        if !width.is_finite() {
            return;
        }
        col.width = width.max(col.min_width);
        col.flex = 0.0;
        col.shrink = None;
        col.resized = true;
        self.sync(self.size);
        cx.request_paint();
    }

    /// The displayed position of the group row pinned to the top of the view, if one
    /// is (after the last layout).
    pub fn floating_group_row(&self) -> Option<usize> {
        self.floating_group().map(|(pos, _)| pos)
    }

    /// Focus and select cells instead of rows (a data grid; off by default). Row
    /// selection is not used then.
    pub fn cell_selection(mut self, cells: bool) -> Self {
        self.cell_mode = cells;
        self
    }

    pub fn frozen_columns(mut self, count: usize) -> Self {
        self.frozen = count;
        self
    }

    pub fn frozen_column_count(&self) -> usize {
        self.frozen.min(self.columns.len())
    }

    /// Keep the last `count` columns (actions, status) at the view's right edge while
    /// the others scroll sideways under them, as CSS `position: sticky; right: 0`:
    /// they sit after the other columns when everything fits, and stay in view when
    /// the columns are wider than the view. Columns already frozen at the start
    /// ([`TableView::frozen_columns`]) do not count.
    pub fn frozen_trailing_columns(mut self, count: usize) -> Self {
        self.frozen_trailing = count;
        self
    }

    pub fn frozen_trailing_column_count(&self) -> usize {
        (self.frozen_trailing).min(self.columns.len() - self.frozen_column_count())
    }

    /// Let the person show and hide columns from the header's context menu
    /// (right-click, Ctrl-click on a Mac, or the context menu key), as `NSTableView`
    /// does. Off by default. Choosing one emits [`ColumnVisibilityChanged`]; the last
    /// column shown cannot be hidden.
    pub fn column_chooser(mut self, chooser: bool) -> Self {
        self.column_chooser = chooser;
        self
    }

    /// Whether `column` is shown (`false` for a column that does not exist).
    pub fn is_column_visible(&self, column: usize) -> bool {
        self.columns.get(column).is_some_and(|c| !c.hidden)
    }

    pub fn visible_columns(&self) -> Vec<usize> {
        (0..self.columns.len())
            .filter(|&c| !self.columns[c].hidden)
            .collect()
    }

    /// Show or hide `column`. A hidden column keeps its place, width and data: it
    /// takes no room, is not drawn, focused, copied or exported, and the words of
    /// [`TableView::set_filter_text`] are not looked for in it. A focused cell in it
    /// moves to the nearest column shown, and an edit in it is cancelled. The last
    /// column shown cannot be hidden. Part of the table's saved state (see
    /// [`crate::UiTree::set_autosave_name`]). Returns whether anything changed; nothing
    /// is emitted.
    pub fn set_column_visible(&mut self, cx: &mut UpdateCx, column: usize, visible: bool) -> bool {
        let changed = self.change_visibility(column, visible);
        if changed {
            cx.request_paint();
        }
        changed
    }

    /// The header's menu: one item per hideable column, checked when it is shown. The
    /// item of the only column shown is disabled.
    fn column_menu(&self) -> Vec<MenuItem> {
        let shown = self.columns.iter().filter(|c| !c.hidden).count();
        self.columns
            .iter()
            .filter(|c| c.hideable)
            .map(|c| {
                MenuItem::new(c.title.clone())
                    .checked(!c.hidden)
                    .enabled(c.hidden || shown > 1)
            })
            .collect()
    }

    fn change_visibility(&mut self, column: usize, visible: bool) -> bool {
        let Some(col) = self.columns.get(column) else {
            return false;
        };
        if col.hidden != visible {
            return false;
        }
        if !visible && self.columns.iter().filter(|c| !c.hidden).count() <= 1 {
            return false;
        }
        self.columns[column].hidden = !visible;
        if !visible {
            if self.editor.item().is_some() && self.edit_column == column {
                self.editor.cancel();
            }
            let nearest = self.nearest_visible(column);
            for cell in [&mut self.cell_focus, &mut self.cell_anchor] {
                if let Some((_, c)) = cell
                    && *c == column
                {
                    *c = nearest;
                }
            }
        }
        if self.filter.is_active() {
            self.refilter();
        }
        self.sync(self.size);
        true
    }

    /// The column shown nearest `column`: itself, else the next one shown, else the
    /// one before.
    fn nearest_visible(&self, column: usize) -> usize {
        (column..self.columns.len())
            .chain((0..column).rev())
            .find(|&c| !self.columns[c].hidden)
            .unwrap_or(column)
    }

    /// The first column shown from `column` on in the direction of `forward`
    /// (`column` itself if shown), if there is one.
    fn visible_from(&self, column: usize, forward: bool) -> Option<usize> {
        if forward {
            (column..self.columns.len()).find(|&c| !self.columns[c].hidden)
        } else {
            (0..=column.min(self.columns.len().saturating_sub(1)))
                .rev()
                .find(|&c| !self.columns[c].hidden)
        }
    }

    fn shown_in(&self, range: Range<usize>) -> Vec<usize> {
        range.filter(|&c| !self.columns[c].hidden).collect()
    }

    /// The focused cell as (data row, column), with [`TableView::cell_selection`].
    pub fn focused_cell(&self) -> Option<(usize, usize)> {
        self.cell_focus
    }

    /// The selected rectangle of cells, from the anchor to the focused cell.
    pub fn selected_cells(&self) -> Option<CellRange> {
        let (focus, anchor) = (self.cell_focus?, self.cell_anchor.or(self.cell_focus)?);
        let (a, b) = (self.position[focus.0], self.position[anchor.0]);
        Some(CellRange {
            rows: a.min(b)..a.max(b) + 1,
            columns: focus.1.min(anchor.1)..focus.1.max(anchor.1) + 1,
        })
    }

    /// The selection as tab-separated text, as Cmd+C copies it: in a data grid the
    /// selected cells, otherwise every column of the selected rows, in displayed
    /// order. Group rows are left out. `None` when nothing is selected.
    pub fn copy_text(&self) -> Option<String> {
        let all = self.visible_columns();
        let rows: Vec<Vec<String>> = if self.cell_mode {
            let range = self.selected_cells()?;
            let columns = self.shown_in(range.columns.clone());
            range
                .rows
                .clone()
                .filter(|&pos| !self.is_group_at(pos))
                .map(|pos| self.row_text(self.order[pos], columns.iter().copied()))
                .collect()
        } else {
            (self.order.iter().copied())
                .filter(|&id| self.selection.is_selected(id) && !self.groups[id])
                .map(|id| self.row_text(id, all.iter().copied()))
                .collect()
        };
        (!rows.is_empty()).then(|| super::tsv::write(&rows))
    }

    /// The whole table as text in `format`, as an "Export…" command saves it: a first
    /// row of the titles of the columns shown, then every row as shown (sorted, and
    /// only those the filter lets through), group rows left out.
    /// Spreadsheets open it; [`TableText::read`] reads it back.
    pub fn export_text(&self, format: TableText) -> String {
        let all = self.visible_columns();
        let mut rows = vec![all.iter().map(|&c| self.columns[c].title.clone()).collect()];
        rows.extend(
            self.order
                .iter()
                .filter(|&&id| !self.groups[id])
                .map(|&id| self.row_text(id, all.iter().copied())),
        );
        format.write(&rows)
    }

    fn row_text(&self, id: usize, columns: impl Iterator<Item = usize>) -> Vec<String> {
        columns
            .map(|c| self.data.cell(id, c).into_owned())
            .collect()
    }

    /// Whether the cell of data row `row` and `column` is in the selected range.
    pub fn is_cell_selected(&self, row: usize, column: usize) -> bool {
        self.selected_cells().is_some_and(|r| {
            self.position
                .get(row)
                .is_some_and(|&pos| r.contains(pos, column) && !self.groups[row])
        })
    }

    /// Focus the cell of data row `row` and `column` (selecting only it) and scroll it
    /// into view. Nothing is emitted.
    pub fn select_cell(&mut self, cx: &mut UpdateCx, row: usize, column: usize) {
        if self.shown_at(row).is_some() && self.is_column_visible(column) && !self.groups[row] {
            self.cell_focus = Some((row, column));
            self.cell_anchor = Some((row, column));
            self.reveal_cell(row, column);
            cx.request_paint();
        }
    }

    /// Start editing the cell of data row `row` and `column` in place, if its column
    /// is editable (it is focused first). Returns whether editing started.
    pub fn edit_cell(&mut self, cx: &mut UpdateCx, row: usize, column: usize) -> bool {
        if self.shown_at(row).is_none() || !self.is_column_visible(column) || self.groups[row] {
            return false;
        }
        self.cell_focus = Some((row, column));
        self.cell_anchor = Some((row, column));
        let started = self.start_edit(row, column);
        cx.request_paint();
        started
    }

    /// The cell being edited as (data row, column), if any.
    pub fn editing_cell(&self) -> Option<(usize, usize)> {
        self.editor.item().map(|row| (row, self.edit_column))
    }

    pub fn edit_text(&self) -> Option<&str> {
        self.editor.text()
    }

    pub fn can_undo(&self) -> bool {
        !self.history.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.history.redo.is_empty()
    }

    /// Take back the last change to the cells, as Cmd+Z does in a data grid (for an
    /// Edit menu's Undo). A cell being edited is cancelled first. The changed cells are
    /// selected. Nothing is emitted: the cells the undo changed are returned as the
    /// [`CellEdited`] the key would have emitted, for an app that keeps its own copy of
    /// the data. Empty when there was nothing to undo.
    pub fn undo(&mut self, cx: &mut UpdateCx) -> Vec<CellEdited> {
        let editing = self.editor.item().is_some();
        self.editor.cancel();
        let edits = self.step_history(false);
        if editing || !edits.is_empty() {
            cx.request_paint();
        }
        edits
    }

    /// As Shift+Cmd+Z does; see [`TableView::undo`].
    pub fn redo(&mut self, cx: &mut UpdateCx) -> Vec<CellEdited> {
        let editing = self.editor.item().is_some();
        self.editor.cancel();
        let edits = self.step_history(true);
        if editing || !edits.is_empty() {
            cx.request_paint();
        }
        edits
    }

    /// [`TableView::set_data`] also clears it.
    pub fn clear_undo_history(&mut self) {
        self.history.clear();
    }

    fn step_history(&mut self, redo: bool) -> Vec<CellEdited> {
        let entry = if redo {
            self.history.redo.pop()
        } else {
            self.history.undo.pop()
        };
        let Some(changes) = entry else {
            return Vec::new();
        };
        let rows = self.data.row_count();
        let mut edits = Vec::new();
        // Undo in the reverse order of the changes, so a cell changed twice ends with
        // its first text.
        let ordered: Vec<&CellChange> = if redo {
            changes.iter().collect()
        } else {
            changes.iter().rev().collect()
        };
        for change in ordered {
            if change.row >= rows || change.column >= self.columns.len() {
                continue;
            }
            let text = if redo { &change.after } else { &change.before };
            let stored = self.data.set_cell(change.row, change.column, text);
            edits.push(CellEdited {
                row: change.row,
                column: change.column,
                text: text.clone(),
                stored,
            });
        }
        self.select_changed(&changes);
        if redo {
            self.history.undo.push(changes);
        } else {
            self.history.redo.push(changes);
        }
        edits
    }

    fn select_changed(&mut self, changes: &[CellChange]) -> bool {
        if !self.cell_mode {
            return false;
        }
        let shown = changes.iter().filter_map(|c| {
            let pos = *self.position.get(c.row)?;
            (pos != HIDDEN).then_some((pos, c.column))
        });
        let Some((top, bottom, left, right)) = shown.fold(None, |acc, (pos, c)| {
            Some(match acc {
                None => (pos, pos, c, c),
                Some((t, b, l, r)) => (pos.min(t), pos.max(b), c.min(l), c.max(r)),
            })
        }) else {
            return false;
        };
        let anchor = Some((self.order[top], left));
        let focus = Some((self.order[bottom], right));
        if (self.cell_anchor, self.cell_focus) == (anchor, focus) {
            return false;
        }
        self.cell_anchor = anchor;
        self.cell_focus = focus;
        self.reveal_cell(self.order[bottom], right);
        true
    }

    fn history_key(&mut self, cx: &mut EventCx, redo: bool) {
        let before = (self.cell_anchor, self.cell_focus);
        let edits = self.step_history(redo);
        if edits.is_empty() && before == (self.cell_anchor, self.cell_focus) {
            return;
        }
        for edit in edits {
            cx.emit(edit);
        }
        if before != (self.cell_anchor, self.cell_focus) {
            self.scroller.flash(cx);
            self.cells_changed(cx);
        } else {
            cx.request_paint();
        }
    }

    pub fn selection_mode(mut self, mode: SelectionMode) -> Self {
        self.selection.mode = mode;
        self
    }

    /// Draw a checkbox before the first column of every row, and one in the header.
    /// Group rows have none. Each change emits [`RowSelectionChanged`].
    ///
    /// The checkboxes are not a column of the data: column numbers, the copied and
    /// exported text, sorting and type to select are as without them. A data grid
    /// ([`TableView::cell_selection`]) and [`SelectionMode::None`] have no row
    /// selection, so no checkboxes either.
    pub fn selection_checkboxes(mut self, checkboxes: bool) -> Self {
        self.checkboxes = checkboxes;
        self
    }

    /// Alternate the row background (on by default).
    pub fn alternating(mut self, alternating: bool) -> Self {
        self.alternating = alternating;
        self
    }

    /// Show the column header (on by default).
    pub fn header(mut self, header: bool) -> Self {
        self.header = header;
        self
    }

    /// Let rows be dragged to a new place; emits [`RowsMoved`].
    pub fn reorderable(mut self, reorderable: bool) -> Self {
        self.reorderable = reorderable;
        self
    }

    pub fn row_height(mut self, height: f32) -> Self {
        self.row_height = height;
        self
    }

    /// A fixed width instead of filling the parent.
    pub fn width(mut self, width: f32) -> Self {
        self.width = length(width);
        self
    }

    /// A fixed height (320 by default).
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// Fill the height the parent has left: the rest of a column, or the height of a
    /// row. The table follows the parent as it resizes.
    pub fn fill_height(mut self) -> Self {
        self.height = None;
        self
    }

    /// Change the height after the table was created: `Some` fixes it, `None` fills the
    /// parent as [`TableView::fill_height`] does.
    pub fn set_height(&mut self, cx: &mut UpdateCx, height: Option<f32>) {
        if self.height != height {
            self.height = height;
            cx.request_layout();
        }
    }

    pub fn columns(&self) -> &[TableColumn] {
        &self.columns
    }

    pub fn row_count(&self) -> usize {
        self.order.len()
    }

    pub fn displayed_rows(&self) -> &[usize] {
        &self.order
    }

    /// The sorted column and whether it is ascending.
    pub fn sort_column(&self) -> Option<(usize, bool)> {
        self.sort
    }

    /// Selected data rows, ascending.
    pub fn selected_rows(&self) -> Vec<usize> {
        self.selection.ids()
    }

    pub fn is_selected(&self, row: usize) -> bool {
        self.selection.is_selected(row)
    }

    pub fn selected_count(&self) -> usize {
        self.selection.count()
    }

    /// The data row acted on last.
    pub fn lead_row(&self) -> Option<usize> {
        self.selection.lead
    }

    /// The vertical scroll position.
    pub fn scroll_offset(&self) -> f32 {
        self.scroller.offset.y
    }

    /// Displayed positions inside the view after the last layout.
    pub fn visible_rows(&self) -> Range<usize> {
        let v = self.viewport();
        let first = (self.scroller.offset.y.max(0.0) / self.row_height) as usize;
        let last = ((self.scroller.offset.y + v.height) / self.row_height).ceil() as usize;
        first.min(self.order.len())..last.min(self.order.len())
    }

    /// Select `rows` (data rows) and scroll the first into view.
    pub fn select_rows(&mut self, cx: &mut UpdateCx, rows: impl IntoIterator<Item = usize>) {
        let (groups, position) = (&self.groups, &self.position);
        self.selection.set_many(
            rows.into_iter()
                .filter(|&r| !groups.get(r).copied().unwrap_or(true) && position[r] != HIDDEN),
        );
        if let Some(pos) = self.selection.ids().first().map(|&id| self.position[id]) {
            self.reveal(pos);
        }
        cx.request_paint();
    }

    /// Replace the rows (keeping the sort column and the filter; the selection is
    /// cleared).
    pub fn set_data(&mut self, cx: &mut UpdateCx, data: impl TableData + 'static) {
        self.data = Box::new(data);
        let rows = self.data.row_count();
        self.all = (0..rows).collect();
        self.groups = (0..rows).map(|r| self.data.is_group_row(r)).collect();
        self.selection.resize(rows);
        self.selection.set(None);
        self.editor.cancel();
        self.cell_focus = None;
        self.cell_anchor = None;
        self.pending_reveal = None;
        self.history.clear();
        self.apply_sort();
        self.refilter();
        cx.request_paint();
    }

    /// Replace the data with the same rows after some of their cells changed: what an
    /// app whose [`TableData`] does not store edits does on [`CellEdited`] (with
    /// `stored` false). Unlike [`TableView::set_data`], it keeps the selection, the
    /// focused cell, a cell being edited, the scroll position, the rows' order and the
    /// undo history, as an edit the data stored itself does. When the row count or the
    /// group rows differ, the rows are not the same and it is [`TableView::set_data`].
    pub fn update_data(&mut self, cx: &mut UpdateCx, data: impl TableData + 'static) {
        let rows = data.row_count();
        let same = rows == self.data.row_count()
            && (0..rows).all(|r| data.is_group_row(r) == self.is_group_row(r));
        if !same {
            self.set_data(cx, data);
            return;
        }
        self.data = Box::new(data);
        cx.request_paint();
    }

    /// Replace the rows like [`TableView::set_data`] but keep the selection: a row of the
    /// new data stays selected (and the lead stays the lead) when `key` gives it the same
    /// key as a row selected before. The scroll position is kept.
    pub fn set_data_keeping_selection<K: Eq + Hash>(
        &mut self,
        cx: &mut UpdateCx,
        data: impl TableData + 'static,
        key: impl Fn(&dyn TableData, usize) -> K,
    ) {
        let selected: HashSet<K> = self
            .selection
            .ids()
            .into_iter()
            .map(|row| key(&*self.data, row))
            .collect();
        let lead = self.selection.lead.map(|row| key(&*self.data, row));
        self.set_data(cx, data);
        if selected.is_empty() {
            return;
        }
        let rows = self.data.row_count();
        let kept: Vec<usize> = (0..rows)
            .filter(|&row| {
                !self.groups[row]
                    && self.position[row] != HIDDEN
                    && selected.contains(&key(&*self.data, row))
            })
            .collect();
        self.selection.set_many(kept);
        if let Some(lead) = lead {
            let new_lead = (0..rows)
                .find(|&row| self.selection.is_selected(row) && key(&*self.data, row) == lead);
            if new_lead.is_some() {
                self.selection.lead = new_lead;
            }
        }
    }

    /// Scroll so that data row `row` is inside the view (once the table has a size, if it
    /// has none yet). The selection does not change.
    pub fn scroll_to_row(&mut self, cx: &mut UpdateCx, row: usize) {
        if let Some(pos) = self.shown_at(row) {
            self.reveal(pos);
            cx.request_paint();
        }
    }

    /// Sort by `column` as a header click would, without emitting [`SortChanged`].
    /// Whether a click on a sortable header orders the rows (the default). Off, the
    /// header still shows the sort and [`SortChanged`] is emitted, but the rows stay in
    /// the data's order: for data sorted elsewhere, such as rows a server returns in
    /// the order asked for (see [`super::RemoteRows`]), where sorting here would read
    /// every row.
    pub fn sorts_rows(mut self, sorts: bool) -> Self {
        self.sorts_rows = sorts;
        self
    }

    /// The data changed behind the table's back, through state it shares with the app
    /// (see [`super::RemoteRows`]): draw the cells again. When the row count or the
    /// group rows changed, the rows are taken again as by [`TableView::set_data`],
    /// keeping the scroll position as far as the rows allow.
    pub fn data_changed(&mut self, cx: &mut UpdateCx) {
        let rows = self.data.row_count();
        let same = rows == self.groups.len()
            && (0..rows).all(|r| self.data.is_group_row(r) == self.groups[r]);
        if !same {
            self.all = (0..rows).collect();
            self.groups = (0..rows).map(|r| self.data.is_group_row(r)).collect();
            self.selection.resize(rows);
            self.selection.set(None);
            self.editor.cancel();
            self.cell_focus = None;
            self.cell_anchor = None;
            self.pending_reveal = None;
            self.history.clear();
            self.apply_sort();
            self.refilter();
            cx.request_layout();
        }
        cx.request_paint();
    }

    pub fn sort_by(&mut self, cx: &mut UpdateCx, column: usize, ascending: bool) {
        if column < self.columns.len() {
            self.sort = Some((column, ascending));
            self.apply_sort();
            self.refilter();
            self.cells_reordered();
            cx.request_paint();
        }
    }

    /// Show only the rows that contain each word of `query` (separated by spaces), in
    /// any letter case, in the text or second line of one of their cells: in every
    /// column, or in those given to [`TableView::filter_columns`]. An empty query
    /// shows every row again. Group rows stay while a row under them is shown, the
    /// sort and the scroll position are kept, and rows that are hidden are unselected
    /// (nothing is emitted, as with [`TableView::set_data`]). The matched letters are
    /// marked in the cells (see [`TableView::marks_filter_matches`]).
    ///
    /// The rows are matched when the query is set and when the data is replaced, not
    /// as cells are edited: a row edited so it no longer matches stays until then.
    pub fn set_filter_text(&mut self, cx: &mut UpdateCx, query: &str) {
        if self.filter.query.set(query) {
            self.filter_changed(cx);
        }
    }

    /// The columns [`TableView::set_filter_text`] searches (every column by default).
    pub fn filter_columns(mut self, columns: impl IntoIterator<Item = usize>) -> Self {
        self.filter.columns = Some(columns.into_iter().collect());
        self
    }

    /// Mark the letters [`TableView::set_filter_text`] matched in the cells (on by
    /// default). Cells shortened to fit their column are not marked.
    pub fn marks_filter_matches(mut self, marks: bool) -> Self {
        self.filter.unmarked = !marks;
        self
    }

    /// Show only the rows `keep` accepts, in addition to [`TableView::set_filter_text`].
    /// It is asked only for rows that are not group rows, by data row.
    pub fn set_filter(
        &mut self,
        cx: &mut UpdateCx,
        keep: impl Fn(&dyn TableData, usize) -> bool + 'static,
    ) {
        self.filter.keep = Some(Box::new(keep));
        self.filter_changed(cx);
    }

    pub fn clear_filter(&mut self, cx: &mut UpdateCx) {
        if self.is_filtered() {
            self.filter.query.set("");
            self.filter.keep = None;
            self.filter_changed(cx);
        }
    }

    pub fn filter_text(&self) -> &str {
        self.filter.query.text()
    }

    /// Whether a filter text or condition is set (even if every row matches it).
    pub fn is_filtered(&self) -> bool {
        self.filter.is_active()
    }

    /// Rows shown that are not group rows: those that match the filter.
    pub fn matching_row_count(&self) -> usize {
        self.order.len() - self.group_at.len()
    }

    /// Rows of the data that are not group rows, shown or not.
    pub fn total_row_count(&self) -> usize {
        self.groups.iter().filter(|&&g| !g).count()
    }

    pub fn is_row_shown(&self, row: usize) -> bool {
        self.shown_at(row).is_some()
    }

    /// The displayed position of data row `id`, unless it is hidden or out of range.
    fn shown_at(&self, id: usize) -> Option<usize> {
        self.position.get(id).copied().filter(|&pos| pos != HIDDEN)
    }

    fn filter_changed(&mut self, cx: &mut UpdateCx) {
        self.refilter();
        self.cells_reordered();
        match self.selection.lead_position(&self.order) {
            Some(pos) => {
                self.reveal(pos);
            }
            None => self.scroller.offset.y = 0.0,
        }
        cx.request_paint();
    }

    /// Show the rows of `all` that the filter lets through, with the group rows over
    /// them, and forget the selection, focused cell and edit of rows now hidden.
    fn refilter(&mut self) {
        if self.filter.is_active() {
            let (data, groups) = (&*self.data, &self.groups);
            let mut searched = self.filter.searched(self.columns.len());
            searched.retain(|&c| !self.columns[c].hidden);
            let mut texts = Vec::new();
            let mut order = Vec::new();
            let mut group = None;
            for (at, &id) in self.all.iter().enumerate() {
                if groups[id] {
                    // A group row with no rows under it (a folded group) stays, so it
                    // can be unfolded while the filter is on.
                    let empty = self.all.get(at + 1).is_none_or(|&next| groups[next]);
                    group = (!empty).then_some(id);
                    if empty {
                        order.push(id);
                    }
                } else if self.filter.matches(data, id, &searched, &mut texts) {
                    order.extend(group.take());
                    order.push(id);
                }
            }
            self.order = order;
        } else {
            self.order.clone_from(&self.all);
        }
        self.index_groups();
        let position = &self.position;
        self.selection.retain(|id| position[id] != HIDDEN);
        let hidden =
            |cell: Option<(usize, usize)>| cell.is_some_and(|(id, _)| position[id] == HIDDEN);
        if hidden(self.cell_focus) || hidden(self.cell_anchor) {
            self.cell_focus = None;
            self.cell_anchor = None;
        }
        if self.editor.item().is_some_and(|id| position[id] == HIDDEN) {
            self.editor.cancel();
        }
        if self
            .pending_reveal
            .is_some_and(|(id, _)| position[id] == HIDDEN)
        {
            self.pending_reveal = None;
        }
    }

    /// The rows were put in another order. A range of cells is a rectangle of
    /// displayed positions between two data rows: those are now elsewhere, with other
    /// rows between them, so only the focused cell stays selected.
    fn cells_reordered(&mut self) {
        self.cell_anchor = self.cell_focus;
    }

    fn index_groups(&mut self) {
        // The buttons drawn last are of rows that may be elsewhere or gone now.
        self.hits.clear();
        self.position.clear();
        self.position.resize(self.groups.len(), HIDDEN);
        for (pos, &id) in self.order.iter().enumerate() {
            self.position[id] = pos;
        }
        self.group_at.clear();
        if self.groups.iter().any(|&g| g) {
            let groups = &self.groups;
            self.group_at.extend(
                self.order
                    .iter()
                    .enumerate()
                    .filter_map(|(pos, &id)| groups[id].then_some(pos)),
            );
        }
    }

    fn is_group_at(&self, pos: usize) -> bool {
        self.group_at.binary_search(&pos).is_ok()
    }

    /// The group row pinned to the top of the view and where it is drawn (widget `y`):
    /// the last group row above the top, pushed up by the next one as it arrives.
    fn floating_group(&self) -> Option<(usize, f32)> {
        if !self.floats_group_rows {
            return None;
        }
        let v = self.viewport();
        pinned_group(&self.group_at, self.scroller.offset.y, self.row_height, v.y)
    }

    fn drop_group_rows(&mut self) {
        for &pos in &self.group_at {
            self.selection.unselect(self.order[pos]);
        }
    }

    fn skip_groups(&self, mut pos: usize, down: bool) -> Option<usize> {
        while self.is_group_at(pos) {
            match down {
                true if pos + 1 < self.order.len() => pos += 1,
                false if pos > 0 => pos -= 1,
                _ => return None,
            }
        }
        Some(pos)
    }

    fn header_height(&self) -> f32 {
        if self.header { TABLE_HEADER } else { 0.0 }
    }

    /// The width of the frame around the table.
    fn inset(&self) -> f32 {
        if self.bordered { 1.0 } else { 0.0 }
    }

    fn viewport(&self) -> Rect {
        let h = self.header_height();
        let i = self.inset();
        Rect::new(
            i,
            h,
            (self.size.width - 2.0 * i).max(0.0),
            (self.size.height - h - i).max(0.0),
        )
    }

    fn total_width(&self) -> f32 {
        self.widths.iter().sum()
    }

    /// Width of the column of checkboxes before the first column (0 without it).
    fn check_width(&self) -> f32 {
        if self.checkboxes && !self.cell_mode && self.selection.mode != SelectionMode::None {
            TABLE_CHECK_COLUMN
        } else {
            0.0
        }
    }

    fn in_check_column(&self, x: f32) -> bool {
        let v = self.viewport();
        x >= v.x && x < v.x + self.content_inset + self.check_width()
    }

    /// The checkbox of a row (or the header) from widget `top` down `height`.
    fn check_box(&self, top: f32, height: f32) -> Rect {
        Rect::new(
            self.viewport().x + self.content_inset + (TABLE_CHECK_COLUMN - CHECK_BOX) / 2.0,
            top + (height - CHECK_BOX) / 2.0,
            CHECK_BOX,
            CHECK_BOX,
        )
    }

    fn has_header_check(&self) -> bool {
        self.header && self.check_width() > 0.0 && self.selection.mode == SelectionMode::Multiple
    }

    fn header_check_state(&self) -> CheckState {
        match self.selection.count() {
            0 => CheckState::Off,
            n if n == self.order.len() - self.group_at.len() => CheckState::On,
            _ => CheckState::Mixed,
        }
    }

    /// What the checkboxes on screen stand for, in the order assistive technology
    /// lists them: the header's (`None`), then the displayed position of each row.
    fn check_items(&self) -> Vec<Option<usize>> {
        if self.check_width() == 0.0 {
            return Vec::new();
        }
        let rows = self.visible_rows().filter(|&pos| !self.is_group_at(pos));
        (self.has_header_check().then_some(None).into_iter())
            .chain(rows.map(Some))
            .collect()
    }

    /// Select every row shown (Cmd+A). Returns whether anything changed.
    fn select_all_rows(&mut self) -> bool {
        if self.group_at.is_empty() {
            self.selection.select_all(&self.order)
        } else {
            let rows: Vec<usize> = (self.order.iter().copied())
                .filter(|&id| !self.groups[id])
                .collect();
            self.selection.select_all(&rows)
        }
    }

    fn toggle_all(&mut self, cx: &mut EventCx) {
        if self.header_check_state() == CheckState::On {
            self.selection.set(None);
            self.changed(cx);
        } else if self.select_all_rows() {
            self.changed(cx);
        }
    }

    fn toggle_row(&mut self, cx: &mut EventCx, pos: usize) {
        let id = self.order[pos];
        if self.selection.mode == SelectionMode::Single && self.selection.is_selected(id) {
            self.selection.set(None);
            self.changed(cx);
            return;
        }
        let command = Modifiers {
            command: true,
            ..Default::default()
        };
        if self.selection.click(&self.order, pos, command) {
            self.changed(cx);
        }
    }

    /// When the room is less than the columns' widths, the columns with a
    /// [`TableColumn::shrink_to`] floor give up the difference, each in
    /// proportion to how much it can lose, and the table scrolls only once they are
    /// all at their floors.
    /// Hidden columns take no width: they stay in place, between their neighbors,
    /// at zero width.
    fn layout_columns(&mut self) {
        let room = self.viewport().width - 2.0 * self.content_inset - self.check_width();
        let width = |c: &TableColumn| if c.hidden { 0.0 } else { c.width };
        let flex = |c: &TableColumn| if c.hidden { 0.0 } else { c.flex };
        let fixed: f32 = self.columns.iter().map(width).sum();
        self.widths.clear();
        if room < fixed {
            let slack = |c: &TableColumn| {
                c.shrink
                    .filter(|_| !c.hidden)
                    .map_or(0.0, |min| (c.width - min).max(0.0))
            };
            let capacity: f32 = self.columns.iter().map(slack).sum();
            let scale = if capacity > 0.0 {
                ((fixed - room) / capacity).min(1.0)
            } else {
                0.0
            };
            self.widths
                .extend(self.columns.iter().map(|c| width(c) - slack(c) * scale));
            return;
        }
        let weights: f32 = self.columns.iter().map(flex).sum();
        let share = if weights > 0.0 {
            (room - fixed) / weights
        } else {
            0.0
        };
        self.widths
            .extend(self.columns.iter().map(|c| width(c) + share * flex(c)));
    }

    fn sync(&mut self, size: Size) {
        self.size = size;
        self.layout_columns();
        let content = Size::new(
            self.total_width() + 2.0 * self.content_inset + self.check_width(),
            self.order.len() as f32 * self.row_height,
        );
        self.scroller.set_geometry(self.viewport(), content);
        if let Some((id, column)) = self.pending_reveal.take()
            && self.shown_at(id).is_some()
        {
            match column {
                Some(c) if c < self.columns.len() => self.reveal_cell(id, c),
                _ => self.reveal(self.position[id]),
            };
        }
    }

    /// Bring the scroller up to date with the rows and columns before scrolling to
    /// one of them: a row may be selected right after the data was replaced, before
    /// the next paint. Without a size yet (before the first layout) there is nothing
    /// to scroll in, so `target` (a data row and maybe a column) is kept for
    /// [`TableView::sync`]. Returns whether the geometry is known.
    fn ready_to_reveal(&mut self, target: (usize, Option<usize>)) -> bool {
        if self.size.width <= 0.0 || self.size.height <= 0.0 {
            self.pending_reveal = Some(target);
            return false;
        }
        self.sync(self.size);
        true
    }

    fn apply_sort(&mut self) {
        let Some((column, ascending)) = self.sort else {
            return;
        };
        if !self.sorts_rows {
            return;
        }
        let data = &*self.data;
        let groups = &self.groups;
        // Rows are sorted within their groups; group rows stay where they are.
        let runs = self.all.split_mut(|&id| groups[id]);
        match self.columns[column].sort {
            ColumnSort::None => {}
            ColumnSort::Text => {
                let keys: Vec<String> = (0..data.row_count())
                    .map(|r| data.cell(r, column).to_lowercase())
                    .collect();
                for run in runs {
                    run.sort_by(|&a, &b| directed(keys[a].cmp(&keys[b]), ascending));
                }
            }
            ColumnSort::Number => {
                let keys: Vec<f64> = (0..data.row_count())
                    .map(|r| data.sort_number(r, column).unwrap_or(f64::NAN))
                    .collect();
                for run in runs {
                    run.sort_by(|&a, &b| {
                        let (x, y) = (keys[a], keys[b]);
                        match (x.is_nan(), y.is_nan()) {
                            (true, true) => Ordering::Equal,
                            (true, false) => Ordering::Greater,
                            (false, true) => Ordering::Less,
                            _ => directed(x.total_cmp(&y), ascending),
                        }
                    });
                }
            }
        }
    }

    /// The resizable column whose header's right edge is under `p` (widget
    /// coordinates), if any.
    fn resize_edge_at(&self, p: Point) -> Option<usize> {
        if !self.header || p.y >= TABLE_HEADER {
            return None;
        }
        let spans = self.column_spans();
        let frozen = self.frozen_column_count();
        let trailing = self.trailing_start();
        let hidden_before = self.viewport().x + self.frozen_edge();
        let hidden_after = self.trailing_edge() + 0.5;
        // Edges of scrolling columns under the frozen ones are not there.
        (0..spans.len())
            .find(|&c| {
                !self.columns[c].hidden
                    && (p.x - spans[c].1).abs() <= RESIZE_GRAB
                    && (c < frozen
                        || c >= trailing
                        || (spans[c].1 > hidden_before + RESIZE_GRAB && spans[c].1 <= hidden_after))
            })
            .filter(|&c| self.columns[c].resizable && !self.columns[c].hidden)
    }

    /// Column edges in widget coordinates, as `(left, right)`: frozen columns in place,
    /// the others scrolled (and possibly under the frozen ones).
    fn column_spans(&self) -> Vec<(f32, f32)> {
        let shift = self.trailing_shift();
        let mut prefix = 0.0;
        self.widths
            .iter()
            .enumerate()
            .map(|(c, &width)| {
                let left = self.column_left(c, prefix, shift);
                prefix += width;
                (left, left + width)
            })
            .collect()
    }

    /// The left edge (widget coordinates) of `column`, which starts `start` into the
    /// columns, given the [`TableView::trailing_shift`].
    fn column_left(&self, column: usize, start: f32, shift: f32) -> f32 {
        let first = self.viewport().x + self.content_inset + self.check_width();
        if column < self.frozen_column_count() {
            first + start
        } else if column >= self.trailing_start() {
            first - self.scroller.offset.x + start - shift
        } else {
            first - self.scroller.offset.x + start
        }
    }

    fn trailing_start(&self) -> usize {
        self.columns.len() - self.frozen_trailing_column_count()
    }

    /// How far the frozen part at the end reaches in from the viewport's right edge
    /// when it is pinned there: those columns and the [`TableView::content_inset`]
    /// after them, or nothing without them.
    fn trailing_reach(&self) -> f32 {
        let start = self.trailing_start();
        if start == self.columns.len() {
            0.0
        } else {
            self.content_inset + self.widths[start..].iter().sum::<f32>()
        }
    }

    /// How far left of where they would scroll to the columns frozen at the end are
    /// pulled to stay in view (0 when they are where they would be).
    fn trailing_shift(&self) -> f32 {
        let start = self.trailing_start();
        if start == self.columns.len() {
            return 0.0;
        }
        let v = self.viewport();
        let natural = v.x + self.content_inset + self.check_width() - self.scroller.offset.x
            + self.widths[..start].iter().sum::<f32>();
        (natural - (v.right() - self.trailing_reach())).max(0.0)
    }

    /// Where the columns frozen at the end start (widget coordinates): the
    /// viewport's right edge without them.
    fn trailing_edge(&self) -> f32 {
        let v = self.viewport();
        let start = self.trailing_start();
        if start == self.columns.len() {
            v.right()
        } else {
            self.column_span(start).0.clamp(v.x, v.right())
        }
    }

    fn frozen_width(&self) -> f32 {
        let frozen = self.frozen_column_count();
        self.widths[..frozen].iter().sum()
    }

    /// How far the part that stays in place reaches from the viewport's left edge: the
    /// checkboxes and the frozen columns after the [`TableView::content_inset`], or
    /// nothing without them.
    fn frozen_edge(&self) -> f32 {
        let checks = self.check_width();
        if self.frozen_column_count() == 0 && checks == 0.0 {
            0.0
        } else {
            self.content_inset + checks + self.frozen_width()
        }
    }

    /// The part of the viewport where the scrolling columns show, between the columns
    /// frozen at either end (from `top` down `height`).
    fn scrolling_area(&self, top: f32, height: f32) -> Rect {
        let v = self.viewport();
        let left = (v.x + self.frozen_edge()).min(v.right());
        let right = self.trailing_edge().max(left);
        Rect::new(left, top, right - left, height)
    }

    fn trailing_area(&self, top: f32, height: f32) -> Rect {
        let v = self.viewport();
        let left = self.trailing_edge();
        Rect::new(left, top, v.right() - left, height)
    }

    /// The column under widget `x`, among those on screen.
    fn column_at(&self, x: f32) -> Option<usize> {
        let v = self.viewport();
        if x < v.x || x >= v.right() {
            return None;
        }
        let frozen = self.frozen_column_count();
        let edge = v.x + self.frozen_edge();
        let spans = self.column_spans();
        let trailing = self.trailing_start();
        let range = if x < edge {
            0..frozen
        } else if x >= self.trailing_edge() {
            trailing..spans.len()
        } else {
            frozen..trailing
        };
        range
            .into_iter()
            .find(|&c| x >= spans[c].0 && x < spans[c].1)
    }

    /// The [`CellLook::Button`] cell under widget point `p`, as (data row, column).
    fn button_at(&self, p: Point) -> Option<(usize, usize)> {
        let (pos, column) = self.cell_at(p)?;
        let row = self.order[pos];
        ((!self.groups[row] || self.group_cells)
            && self.data.cell_look(row, column) == CellLook::Button)
            .then_some((row, column))
    }

    /// The button of a [`CellLook::Buttons`] cell under widget point `p`, as the last
    /// paint drew it. A button hidden under the pinned group row is not there.
    fn action_at(&self, p: Point) -> Option<&ButtonHit> {
        let pos = self.position_at(p)?;
        self.hits
            .iter()
            .rev()
            .find(|hit| hit.rect.contains(p) && self.position[hit.row] == pos)
    }

    fn hover_target(&self, p: Option<Point>) -> (Option<usize>, Option<usize>) {
        let Some(p) = p else {
            return (None, None);
        };
        let row = self.position_at(p).filter(|_| self.hover_rows);
        let button = self.action_at(p).map(|hit| hit.index);
        (row, button)
    }

    fn is_member(&self, pos: usize) -> bool {
        self.group_at.first().is_some_and(|&g| g < pos) && !self.is_group_at(pos)
    }

    fn indent_at(&self, pos: usize, column: usize) -> f32 {
        if column == self.member_indent_column && self.member_indent > 0.0 && self.is_member(pos) {
            self.member_indent
        } else {
            0.0
        }
    }

    fn cell_style(&self, column: usize, group: bool) -> TextStyle {
        let mut style = TextStyle::new(13.0);
        if let Some(family) = self.families.as_ref().and_then(|f| f[column]) {
            style = style.family(family);
        }
        if group { style.weight(700) } else { style }
    }

    fn detail_style(&self, column: usize) -> TextStyle {
        let style = TextStyle::new(11.0);
        let own = self.detail_families.get(column).copied().flatten();
        match own.or_else(|| self.families.as_ref().and_then(|f| f[column])) {
            Some(family) => style.family(family),
            None => style,
        }
    }

    /// All the text of a cell: its text and note, then its second line after
    /// `line_break`.
    fn cell_text(&self, row: usize, column: usize, line_break: &str) -> String {
        let mut text = self.data.cell(row, column).into_owned();
        if let Some(note) = self.data.cell_note(row, column) {
            text.push(' ');
            text.push_str(&note);
        }
        if let Some(detail) = self.data.cell_detail(row, column) {
            text.push_str(line_break);
            text.push_str(&detail);
        }
        text
    }

    fn accessible_cell_text(&self, row: usize, column: usize) -> String {
        match self.data.cell_look(row, column) {
            CellLook::Buttons => {}
            CellLook::Loading => return "Loading".to_owned(),
            _ => return self.cell_text(row, column, " "),
        }
        let buttons = self.data.cell_buttons(row, column);
        let names: Vec<&str> = buttons.iter().filter_map(CellButton::name).collect();
        names.join(", ")
    }

    /// The cell under widget point `p`, as (displayed position, column).
    fn cell_at(&self, p: Point) -> Option<(usize, usize)> {
        Some((self.position_at(p)?, self.column_at(p.x)?))
    }

    /// Scroll the cell of data row `id` and `column` into view, clear of a pinned
    /// group row and of the frozen columns.
    fn reveal_cell(&mut self, id: usize, column: usize) -> bool {
        if self.shown_at(id).is_none() || !self.ready_to_reveal((id, Some(column))) {
            return false;
        }
        let pos = self.position[id];
        let rows = reveal_rect(
            &self.group_at,
            self.floats_group_rows,
            pos,
            self.row_height,
            self.scroller.offset.x,
            1.0,
        );
        let (x, width) = if column < self.frozen_column_count() || column >= self.trailing_start() {
            (self.scroller.offset.x, 1.0)
        } else {
            let start =
                self.content_inset + self.check_width() + self.widths[..column].iter().sum::<f32>();
            let frozen = self.frozen_edge();
            (
                start - frozen,
                self.widths[column] + frozen + self.trailing_reach(),
            )
        };
        self.scroller
            .reveal(Rect::new(x, rows.y, width, rows.height))
    }

    /// Where a cell's text starts (widget coordinates) for the editor.
    fn cell_text_origin(&self, pos: usize, column: usize, text_height: f32) -> Point {
        let v = self.viewport();
        let (left, _) = self.column_spans()[column];
        let y = v.y + pos as f32 * self.row_height - self.scroller.offset.y;
        Point::new(
            left + self.cell_pad + self.indent_at(pos, column),
            y + (self.row_height - text_height) / 2.0,
        )
    }

    fn start_edit(&mut self, id: usize, column: usize) -> bool {
        if !self.columns[column].editable || self.groups[id] {
            return false;
        }
        self.reveal_cell(id, column);
        let text = self.data.cell(id, column).into_owned();
        let style = self.cell_style(column, false);
        self.editor.start(id, &text, style);
        self.edit_column = column;
        let pos = self.position[id];
        let origin = self.cell_text_origin(pos, column, style.line_height);
        let (left, right) = self.column_spans()[column];
        let indent = self.indent_at(pos, column);
        if let Some(editor) = self.editor.editor_for(id) {
            editor.at = origin;
            editor.max_width = (right - left - self.cell_pad - indent).max(0.0);
        }
        true
    }

    fn edit_event(&mut self, cx: &mut EventCx, event: &Event) -> bool {
        match self.editor.event(cx, event) {
            RenameStep::Ignored => false,
            RenameStep::Handled => true,
            RenameStep::Done {
                item,
                name,
                pass_on,
            } => {
                let column = self.edit_column;
                if let Some(text) = name
                    && text != self.data.cell(item, column)
                {
                    self.history.pending.push(CellChange {
                        row: item,
                        column,
                        before: self.data.cell(item, column).into_owned(),
                        after: text.clone(),
                    });
                    self.history.commit();
                    let stored = self.data.set_cell(item, column, &text);
                    cx.emit(CellEdited {
                        row: item,
                        column,
                        text,
                        stored,
                    });
                }
                !pass_on
            }
        }
    }

    fn cells_changed(&mut self, cx: &mut EventCx) {
        if let (Some(focus), Some(range)) = (self.cell_focus, self.selected_cells()) {
            cx.emit(CellSelectionChanged { focus, range });
        }
        cx.request_paint();
    }

    fn focus_cell(&mut self, cx: &mut EventCx, pos: usize, column: usize, extend: bool) {
        let cell = (self.order[pos], column);
        let anchor = if extend {
            self.cell_anchor.or(self.cell_focus).unwrap_or(cell)
        } else {
            cell
        };
        if self.cell_focus == Some(cell) && self.cell_anchor == Some(anchor) {
            return;
        }
        self.cell_focus = Some(cell);
        self.cell_anchor = Some(anchor);
        if self.reveal_cell(cell.0, column) {
            self.scroller.flash(cx);
        }
        self.cells_changed(cx);
    }

    fn cell_press(&mut self, cx: &mut EventCx, p: Point) {
        let Some((pos, c)) = self.cell_at(p).filter(|&(pos, _)| !self.is_group_at(pos)) else {
            return;
        };
        let id = self.order[pos];
        let now = cx.now();
        let key = id * self.columns.len() + c;
        let double = self
            .last_click
            .is_some_and(|(t, last)| last == key && now - t <= DOUBLE_CLICK);
        self.last_click = Some((now, key));
        self.focus_cell(cx, pos, c, cx.modifiers().shift);
        if double {
            self.last_click = None;
            if self.start_edit(id, c) {
                cx.request_paint();
            } else {
                cx.emit(RowActivated(id));
            }
            return;
        }
        self.press = Press::Cells;
        cx.capture_pointer();
    }

    /// Store `text` in the cell of data row `id` and `column` if its column is
    /// editable and the text differs, emitting [`CellEdited`]. Returns whether it
    /// changed.
    fn replace_cell(&mut self, cx: &mut EventCx, id: usize, column: usize, text: &str) -> bool {
        if !self.columns[column].editable || self.groups[id] || self.data.cell(id, column) == text {
            return false;
        }
        self.history.pending.push(CellChange {
            row: id,
            column,
            before: self.data.cell(id, column).into_owned(),
            after: text.to_owned(),
        });
        let stored = self.data.set_cell(id, column, text);
        cx.emit(CellEdited {
            row: id,
            column,
            text: text.to_owned(),
            stored,
        });
        true
    }

    fn clear_cells(&mut self, cx: &mut EventCx) {
        let Some(range) = self.selected_cells() else {
            return;
        };
        let columns = self.shown_in(range.columns.clone());
        let mut changed = false;
        for pos in range.rows {
            for &c in &columns {
                changed |= self.replace_cell(cx, self.order[pos], c, "");
            }
        }
        self.history.commit();
        if changed {
            cx.request_paint();
        }
    }

    /// Paste tab-separated text from the clipboard into the data grid: one value fills
    /// the selected range; more are laid out from its top left cell, over the rows
    /// that are not group rows, as far as the table reaches. The cells pasted over
    /// become the selection.
    fn paste_cells(&mut self, cx: &mut EventCx) {
        let Some(range) = self.selected_cells() else {
            return;
        };
        let Some(text) = cx.clipboard().read_text() else {
            return;
        };
        let values = super::tsv::read(&text);
        if values.is_empty() {
            return;
        }
        let columns = self.columns.len();
        let single = values.len() == 1 && values[0].len() == 1;
        // Displayed positions to paste into, and the columns (only those shown: the
        // values go to the columns the person sees, side by side).
        let (positions, targets): (Vec<usize>, Vec<usize>) = if single {
            let rows = range.rows.clone().filter(|&p| !self.is_group_at(p));
            (rows.collect(), self.shown_in(range.columns.clone()))
        } else {
            let width = values.iter().map(Vec::len).max().unwrap_or(0);
            let rows = (range.rows.start..self.order.len())
                .filter(|&p| !self.is_group_at(p))
                .take(values.len());
            let targets = self.shown_in(range.columns.start..columns);
            (rows.collect(), targets.into_iter().take(width).collect())
        };
        let (Some(&first), Some(&last)) = (positions.first(), positions.last()) else {
            return;
        };
        let (Some(&left), Some(&right)) = (targets.first(), targets.last()) else {
            return;
        };
        let mut changed = false;
        for (i, &pos) in positions.iter().enumerate() {
            let id = self.order[pos];
            for (j, &c) in targets.iter().enumerate() {
                let value = if single {
                    values[0][0].as_str()
                } else {
                    match values[i].get(j) {
                        Some(v) => v.as_str(),
                        None => continue,
                    }
                };
                changed |= self.replace_cell(cx, id, c, value);
            }
        }
        self.history.commit();
        let anchor = Some((self.order[first], left));
        let focus = Some((self.order[last], right));
        if (self.cell_anchor, self.cell_focus) != (anchor, focus) {
            self.cell_anchor = anchor;
            self.cell_focus = focus;
            self.cells_changed(cx);
        } else if changed {
            cx.request_paint();
        }
    }

    /// Fill the selected range from its first row down (`down`) or from its first
    /// column to the right, as spreadsheets do with Cmd+D and Cmd+R. A range one row
    /// (or column) wide is filled from the row above (the column to the left) instead.
    /// Group rows are passed over and only editable cells change; the range ends up
    /// selected, and the fill is undone as a whole.
    fn fill_cells(&mut self, cx: &mut EventCx, down: bool) {
        let Some(range) = self.selected_cells() else {
            return;
        };
        let rows: Vec<usize> = range
            .rows
            .clone()
            .filter(|&p| !self.is_group_at(p))
            .collect();
        let Some(&first) = rows.first() else {
            return;
        };
        let mut changed = false;
        if down {
            let (source, targets) = if rows.len() > 1 {
                (first, &rows[1..])
            } else {
                let Some(above) = (0..first).rev().find(|&p| !self.is_group_at(p)) else {
                    return;
                };
                (above, &rows[..])
            };
            let source = self.order[source];
            for c in self.shown_in(range.columns.clone()) {
                let value = self.data.cell(source, c).into_owned();
                for &pos in targets {
                    changed |= self.replace_cell(cx, self.order[pos], c, &value);
                }
            }
        } else {
            // Hidden columns are neither filled nor filled from.
            let shown = self.shown_in(range.columns.clone());
            let (source, targets) = if shown.len() > 1 {
                (shown[0], shown[1..].to_vec())
            } else {
                let before = range
                    .columns
                    .start
                    .checked_sub(1)
                    .and_then(|c| self.visible_from(c, false));
                match before {
                    Some(before) => (before, shown),
                    None => return,
                }
            };
            for &pos in &rows {
                let id = self.order[pos];
                let value = self.data.cell(id, source).into_owned();
                for &c in &targets {
                    changed |= self.replace_cell(cx, id, c, &value);
                }
            }
        }
        self.history.commit();
        if changed {
            cx.request_paint();
        }
    }

    fn copy(&mut self, cx: &mut EventCx) -> bool {
        match self.copy_text() {
            Some(text) => {
                cx.clipboard().write_text(&text);
                true
            }
            None => false,
        }
    }

    /// Select the next row whose text in the sorted column (or the first) starts with
    /// what was typed: typing within [`TYPE_SELECT_PAUSE`] of the last key adds to the
    /// search and may keep the current row; after a pause the search starts over from
    /// the row after it.
    fn type_select(&mut self, cx: &mut EventCx, text: &str) -> bool {
        let now = cx.now();
        let fresh = now - self.typed_at > TYPE_SELECT_PAUSE;
        if fresh {
            self.typed.clear();
            // A space alone is not a search (it may be a key of its own).
            if text.trim().is_empty() {
                return false;
            }
        }
        self.typed.push_str(&text.to_lowercase());
        self.typed_at = now;
        let n = self.order.len();
        if n == 0 || self.columns.is_empty() {
            return true;
        }
        let column = self.nearest_visible(self.sort.map_or(0, |(c, _)| c));
        let current = self.selection.lead_position(&self.order);
        // A refined search may stay on the current row; a new one moves past it.
        let start = match current {
            Some(pos) if fresh || self.typed.chars().count() == 1 => pos + 1,
            Some(pos) => pos,
            None => 0,
        };
        let found = (0..n).map(|i| (start + i) % n).find(|&pos| {
            let id = self.order[pos];
            !self.groups[id]
                && self
                    .data
                    .cell(id, column)
                    .trim_start()
                    .to_lowercase()
                    .starts_with(&self.typed)
        });
        if let Some(pos) = found {
            if self.selection.move_to(&self.order, pos, false) {
                self.changed(cx);
            }
            if self.reveal(pos) {
                self.scroller.flash(cx);
                cx.request_paint();
            }
        }
        true
    }

    fn cell_key(&mut self, cx: &mut EventCx, key: Key, shift: bool, command: bool) -> bool {
        // Undo needs no shown cell: the filter may hide every row it would change.
        match key {
            Key::Char('z') if command => {
                self.history_key(cx, shift);
                return true;
            }
            // Ctrl+Y redoes off Apple platforms (on Apple platforms Cmd is Meta).
            Key::Char('y') if command && !cx.modifiers().meta => {
                self.history_key(cx, true);
                return true;
            }
            _ => {}
        }
        let n = self.order.len();
        let columns = self.columns.len();
        if n == 0 || columns == 0 {
            return false;
        }
        let current = self.cell_focus.map(|(id, c)| (self.position[id], c));
        match key {
            Key::Char('c') if command => self.copy(cx),
            Key::Char('x') if command => {
                if self.copy(cx) {
                    self.clear_cells(cx);
                }
                true
            }
            Key::Char('v') if command => {
                self.paste_cells(cx);
                true
            }
            Key::Char('d') if command && !shift => {
                self.fill_cells(cx, true);
                true
            }
            Key::Char('r') if command && !shift => {
                self.fill_cells(cx, false);
                true
            }
            Key::Delete | Key::Backspace if !command => {
                self.clear_cells(cx);
                true
            }
            Key::Char('a') if command => {
                let (Some(first), Some(last)) =
                    (self.skip_groups(0, true), self.skip_groups(n - 1, false))
                else {
                    return true;
                };
                let (Some(left), Some(right)) = (
                    self.visible_from(0, true),
                    self.visible_from(columns - 1, false),
                ) else {
                    return true;
                };
                let anchor = Some((self.order[first], left));
                let focus = Some((self.order[last], right));
                if (self.cell_anchor, self.cell_focus) != (anchor, focus) {
                    self.cell_anchor = anchor;
                    self.cell_focus = focus;
                    self.cells_changed(cx);
                }
                true
            }
            Key::Enter => {
                if let Some((id, c)) = self.cell_focus {
                    if self.start_edit(id, c) {
                        cx.request_paint();
                    } else {
                        cx.emit(RowActivated(id));
                    }
                }
                true
            }
            Key::ArrowLeft | Key::ArrowRight => {
                let (pos, c) = match current {
                    Some((pos, c)) if key == Key::ArrowLeft => (
                        pos,
                        c.checked_sub(1)
                            .and_then(|c| self.visible_from(c, false))
                            .unwrap_or(c),
                    ),
                    Some((pos, c)) => (pos, self.visible_from(c + 1, true).unwrap_or(c)),
                    // Nothing is focused yet: either key starts at the first cell.
                    None => match self.skip_groups(0, true) {
                        Some(pos) => (pos, self.nearest_visible(0)),
                        None => return true,
                    },
                };
                self.focus_cell(cx, pos, c, shift);
                true
            }
            Key::Home | Key::End if !command => {
                let Some((pos, _)) = current else {
                    return true;
                };
                let c = if key == Key::Home {
                    self.nearest_visible(0)
                } else {
                    self.visible_from(columns - 1, false).unwrap_or(columns - 1)
                };
                self.focus_cell(cx, pos, c, shift);
                true
            }
            Key::ArrowUp | Key::ArrowDown | Key::Home | Key::End | Key::PageUp | Key::PageDown => {
                let page = (self.viewport().height / self.row_height) as usize;
                let column = current.map_or_else(|| self.nearest_visible(0), |(_, c)| c);
                let down = matches!(key, Key::ArrowDown | Key::PageDown | Key::Home);
                let target = step(key, current.map(|(pos, _)| pos), n, page).and_then(|pos| {
                    self.skip_groups(pos, down)
                        .or_else(|| self.skip_groups(pos, !down))
                });
                if let Some(pos) = target {
                    self.focus_cell(cx, pos, column, shift);
                }
                true
            }
            _ => false,
        }
    }

    /// The displayed position under widget point `p`, if it is on a row (or on the
    /// pinned group row).
    fn position_at(&self, p: Point) -> Option<usize> {
        let v = self.viewport();
        if !v.contains(p) {
            return None;
        }
        if let Some((pos, y)) = self.floating_group()
            && p.y >= y
            && p.y < y + self.row_height
        {
            return Some(pos);
        }
        let pos = ((p.y - v.y + self.scroller.offset.y) / self.row_height).floor();
        (pos >= 0.0 && (pos as usize) < self.order.len()).then_some(pos as usize)
    }

    /// How fast (-1 up to 1 down, of [`AUTOSCROLL_SPEED`]) a drag at `p` scrolls the
    /// rows: only while rows are dragged or cells selected, and near or past an edge.
    fn autoscroll_rate(&self, p: Point) -> f32 {
        let drags = matches!(self.press, Press::Row { dragging: true, .. } | Press::Cells);
        if !drags {
            return 0.0;
        }
        let v = self.viewport();
        let edge = AUTOSCROLL_EDGE.min(v.height / 4.0);
        if edge <= 0.0 {
            return 0.0;
        }
        let up = (v.y + edge - p.y) / edge;
        let down = (p.y - (v.bottom() - edge)) / edge;
        if up > 0.0 && self.scroller.offset.y > 0.0 {
            -up.min(1.0)
        } else if down > 0.0 && self.scroller.offset.y < self.scroller.max().y {
            down.min(1.0)
        } else {
            0.0
        }
    }

    fn track_autoscroll(&mut self, cx: &mut EventCx, p: Point) {
        if self.autoscroll_rate(p) == 0.0 {
            self.autoscroll = None;
            return;
        }
        let since = self.autoscroll.map_or(cx.now(), |(_, t)| t);
        if self.autoscroll.is_none() {
            cx.request_animation_frame();
        }
        self.autoscroll = Some((p, since));
    }

    /// One frame of scrolling at an edge during a drag; the drop position or the range
    /// of cells follows the rows moving under the pointer.
    fn autoscroll_step(&mut self, cx: &mut EventCx, now: f64) {
        let Some((p, last)) = self.autoscroll else {
            return;
        };
        let rate = self.autoscroll_rate(p);
        // Long frames (or the first one) move at most a 30th of a second's worth.
        let dt = (now - last).clamp(0.0, 1.0 / 30.0) as f32;
        let offset = self.scroller.offset;
        let target = Point::new(
            offset.x,
            offset.y + rate * AUTOSCROLL_SPEED * dt.max(1.0 / 120.0),
        );
        if rate == 0.0 || !self.scroller.set_offset(target) {
            self.autoscroll = None;
            return;
        }
        match self.press {
            Press::Row {
                at,
                click,
                dragging,
                ..
            } => {
                self.press = Press::Row {
                    at,
                    click,
                    dragging,
                    drop: self.drop_position(p.y),
                };
            }
            Press::Cells => {
                let v = self.viewport();
                let inside = Point::new(
                    p.x.clamp(v.x, (v.right() - 1.0).max(v.x)),
                    p.y.clamp(v.y, (v.bottom() - 1.0).max(v.y)),
                );
                if let Some((pos, c)) = self.cell_at(inside)
                    && !self.is_group_at(pos)
                {
                    self.focus_cell(cx, pos, c, true);
                }
            }
            _ => {}
        }
        self.autoscroll = Some((p, now));
        cx.request_paint();
        cx.request_animation_frame();
    }

    /// Where dropped rows would go for a pointer at widget `y`.
    fn drop_position(&self, y: f32) -> usize {
        let v = self.viewport();
        let pos = ((y - v.y + self.scroller.offset.y) / self.row_height).round();
        (pos.max(0.0) as usize).min(self.order.len())
    }

    /// Scroll the row at `pos` into view, clear of a group row pinned over it.
    fn reveal(&mut self, pos: usize) -> bool {
        if !self.ready_to_reveal((self.order[pos], None)) {
            return false;
        }
        let rect = reveal_rect(
            &self.group_at,
            self.floats_group_rows,
            pos,
            self.row_height,
            self.scroller.offset.x,
            1.0,
        );
        self.scroller.reveal(rect)
    }

    fn changed(&mut self, cx: &mut EventCx) {
        cx.emit(self.selection.message());
        cx.request_paint();
    }

    fn hover_to(&mut self, cx: &mut EventCx, p: Option<Point>) {
        let before = self.hover_target(self.hover);
        self.hover = p;
        if self.hover_target(p) != before {
            cx.request_paint();
        }
    }

    /// Move the selected rows (in displayed order) to before position `to`.
    fn move_selected(&mut self, cx: &mut EventCx, to: usize) {
        let moving: Vec<usize> = self
            .order
            .iter()
            .copied()
            .filter(|&id| self.selection.is_selected(id))
            .collect();
        if moving.is_empty() {
            return;
        }
        let before = self.order[..to]
            .iter()
            .filter(|&&id| self.selection.is_selected(id))
            .count();
        let at = to - before;
        let unchanged = self.order[at..at + moving.len()] == moving[..];
        if unchanged {
            return;
        }
        // The rows go before the first shown row after the drop that is not moving, or
        // after the last shown row when there is none: rows a filter hides keep their
        // places around them.
        let next = self.order[to..]
            .iter()
            .copied()
            .find(|&id| !self.selection.is_selected(id));
        let mut rest: Vec<usize> = self
            .all
            .iter()
            .copied()
            .filter(|&id| !self.selection.is_selected(id))
            .collect();
        let insert = match next {
            Some(next) => rest.iter().position(|&id| id == next).unwrap_or(rest.len()),
            None => self.order[..to]
                .iter()
                .rev()
                .find(|&&id| !self.selection.is_selected(id))
                .and_then(|&last| rest.iter().position(|&id| id == last))
                .map_or(rest.len(), |p| p + 1),
        };
        rest.splice(insert..insert, moving.iter().copied());
        self.all = rest;
        self.refilter();
        // A dragged order is no longer sorted by a column.
        self.sort = None;
        cx.emit(RowsMoved {
            rows: moving,
            to: at,
        });
        cx.request_paint();
    }

    fn header_click(&mut self, cx: &mut EventCx, column: usize) {
        if self.columns[column].sort == ColumnSort::None {
            return;
        }
        let ascending = match self.sort {
            Some((c, asc)) if c == column => !asc,
            _ => true,
        };
        let cells = self.selected_cells();
        self.sort = Some((column, ascending));
        self.apply_sort();
        self.refilter();
        if let Some(pos) = self.selection.lead_position(&self.order) {
            self.reveal(pos);
        }
        self.cells_reordered();
        if let Some((id, c)) = self.cell_focus {
            self.reveal_cell(id, c);
        }
        if self.selected_cells() != cells {
            self.cells_changed(cx);
        }
        cx.emit(SortChanged { column, ascending });
        cx.request_paint();
    }

    fn paint_group_row(&mut self, cx: &mut PaintCx, id: usize, y: f32, background: Color) {
        let v = self.viewport();
        let rh = self.row_height;
        let ink = cx.color(Role::Text);
        cx.fill_rect(Rect::new(v.x, y, v.width, rh), mix(background, ink, 0.07));
        if self.group_cells {
            return;
        }
        let text = self.data.cell(id, 0);
        let mut style = TextStyle::new(12.0).weight(700);
        if let Some(family) = self.families.as_ref().and_then(|f| f[0]) {
            style = style.family(family);
        }
        let (layout, size) = self.cells.get(cx.text_store(), (id, 0, 0), &text, style);
        let Some(layout) = layout else { return };
        let muted = cx.color(Role::Muted);
        let mut x = v.x + self.content_inset + self.check_width() + self.cell_pad;
        let text_y = y + (rh - size.height) / 2.0;
        if let Some(dot) = self.data.cell_dot(id, 0) {
            let color = dot.color(cx);
            paint_dot(cx, Point::new(x, text_y + size.height / 2.0), color);
            x += DOT + DOT_GAP;
        }
        cx.text(Rect::new(x, text_y, size.width, size.height), layout, muted);
        if let Some(note) = self.data.cell_note(id, 0) {
            let style = self.detail_style(0);
            let (layout, note_size) =
                self.cells
                    .get(cx.text_store(), (id, 0, NOTE_PART), &note, style);
            if let Some(layout) = layout {
                let rect = Rect::new(
                    x + size.width + NOTE_GAP,
                    text_y + (size.height - note_size.height) / 2.0,
                    note_size.width,
                    note_size.height,
                );
                cx.text(rect, layout, muted);
            }
        }
    }

    fn paint_row_cells(
        &mut self,
        cx: &mut PaintCx,
        (pos, id, y): (usize, usize, f32),
        (area, columns): (Rect, Range<usize>),
        range: Option<&CellRange>,
        colors: &CellColors,
    ) {
        let group = self.groups[id];
        for c in columns {
            if self.columns[c].hidden {
                continue;
            }
            let (left, right) = self.column_span(c);
            if right < area.x || left > area.right() {
                continue;
            }
            let spot = CellSpot {
                pos,
                id,
                column: c,
                left,
                right,
                y,
                area,
                in_range: !group && range.is_some_and(|r| r.contains(pos, c)),
            };
            let style = self.cell_style(c, group);
            self.paint_cell(cx, &spot, colors, style);
        }
    }

    /// Left and right edge of `column` in widget coordinates.
    fn column_span(&self, column: usize) -> (f32, f32) {
        let start: f32 = self.widths[..column].iter().sum();
        let left = self.column_left(column, start, self.trailing_shift());
        (left, left + self.widths[column])
    }

    fn paint_cell(
        &mut self,
        cx: &mut PaintCx,
        spot: &CellSpot,
        colors: &CellColors,
        style: TextStyle,
    ) {
        let (id, c) = (spot.id, spot.column);
        let (left, right, y) = (spot.left, spot.right, spot.y);
        let rh = self.row_height;
        let cell = Rect::new(left, y, right - left, rh);
        if spot.in_range {
            cx.fill_rect(cell, colors.range);
        }
        if self.cell_mode && self.cell_focus == Some((id, c)) {
            cx.rounded_rect(
                Rect::new(
                    cell.x + 1.0,
                    cell.y + 1.0,
                    cell.width - 2.0,
                    cell.height - 2.0,
                ),
                2.0,
                noa_gpui_scene::Color::TRANSPARENT,
                Border {
                    width: 2.0,
                    color: colors.focus,
                },
            );
        }
        let indent = self.indent_at(spot.pos, c);
        if indent > 0.0 {
            let x = left + self.cell_pad + (indent - MEMBER_RULE) / 2.0;
            cx.fill_rect(Rect::new(x, y, MEMBER_RULE, rh), colors.rule);
        }
        if self.editor.item() == Some(id) && self.edit_column == c {
            let at = self.cell_text_origin(spot.pos, c, style.line_height);
            // What the cell is clipped to: scrolling columns slide under the frozen ones.
            if let Some(editor) = self.editor.editor_in(id, spot.area) {
                let size = editor.shape(cx.text_store());
                editor.at = Point::new(at.x, y + (rh - size.height) / 2.0);
                editor.max_width = (right - left - self.cell_pad - indent).max(0.0);
                editor.paint(cx);
            }
            return;
        }
        let selected = !self.cell_mode && self.selection.is_selected(id);
        let look = self.data.cell_look(id, c);
        let left = left + indent;
        if look == CellLook::Buttons {
            self.paint_buttons(cx, spot, left, selected, colors, style);
            return;
        }
        if look == CellLook::Loading {
            let room = (right - left - 2.0 * self.cell_pad).max(0.0);
            let height = (rh * 0.3).round().max(4.0);
            let bar = Rect::new(
                left + self.cell_pad,
                y + (rh - height) / 2.0,
                (room * 0.6).round(),
                height,
            );
            let fill = colors.ink.with_alpha(0.12);
            cx.rounded_rect(bar, height / 2.0, fill, Border::default());
            return;
        }
        let text = self.data.cell(id, c);
        let inset = match look {
            CellLook::Badge(_) | CellLook::OutlinedBadge(_) | CellLook::StrongOutlinedBadge(_) => {
                BADGE_PAD
            }
            CellLook::Button => BUTTON_PAD,
            _ => 0.0,
        };
        let room = right - left - 2.0 * self.cell_pad;
        let dot = self.data.cell_dot(id, c);
        let lead = if dot.is_some() { DOT + DOT_GAP } else { 0.0 };
        let fit = (room - lead - 2.0 * inset, self.columns[c].truncation);
        let (layout, size, shortened) =
            self.cells
                .get_fitted(cx.text_store(), (id, c, 0), &text, style, Some(fit));
        if shortened {
            self.shortened.insert((id, c));
        }
        let Some(layout) = layout else { return };
        let width = (size.width + 2.0 * inset).min((room - lead).max(0.0));
        let plain = matches!(look, CellLook::Text | CellLook::Colored(_));
        let mut note = None;
        if plain && let Some(text) = self.data.cell_note(id, c) {
            let avail = (room - lead - width - NOTE_GAP).floor();
            let fit = Some((avail.max(0.0), Truncation::Tail));
            let (layout, size, cut) = self.cells.get_fitted(
                cx.text_store(),
                (id, c, NOTE_PART),
                &text,
                self.detail_style(c),
                fit,
            );
            if cut {
                self.shortened.insert((id, c));
            }
            if avail > 0.0 {
                note = layout.map(|layout| (layout, size));
            }
        }
        let content = width + note.map_or(0.0, |(_, size)| NOTE_GAP + size.width);
        let align = self.columns[c].align;
        let x = aligned(align, (left, right), self.cell_pad, room, lead + content) + lead;
        let mut text_y = y + (rh - size.height) / 2.0;
        let muted = if selected {
            colors.selected_ink.with_alpha(colors.selected_ink.a * 0.75)
        } else {
            colors.muted
        };
        if plain && let Some(detail) = self.data.cell_detail(id, c) {
            let detail_style = self.detail_style(c);
            let fit = Some((room - lead, self.columns[c].truncation));
            let (detail_layout, detail_size, cut) = self.cells.get_fitted(
                cx.text_store(),
                (id, c, DETAIL_PART),
                &detail,
                detail_style,
                fit,
            );
            if cut {
                self.shortened.insert((id, c));
            }
            text_y = y + (rh - size.height - DETAIL_GAP - detail_size.height) / 2.0;
            if let Some(detail_layout) = detail_layout {
                let detail_width = detail_size.width.min((room - lead).max(0.0));
                let rect = Rect::new(
                    aligned(
                        align,
                        (left, right),
                        self.cell_pad,
                        room,
                        lead + detail_width,
                    ) + lead,
                    text_y + size.height + DETAIL_GAP,
                    detail_size.width,
                    detail_size.height,
                );
                cx.text(rect, detail_layout, muted);
            }
        }
        if let Some(dot) = dot {
            let color = dot.color(cx);
            paint_dot(cx, Point::new(x - lead, text_y + size.height / 2.0), color);
        }
        if let Some((layout, note_size)) = note {
            let rect = Rect::new(
                x + width + NOTE_GAP,
                text_y + (size.height - note_size.height) / 2.0,
                note_size.width,
                note_size.height,
            );
            cx.text(rect, layout, muted);
        }
        let color = match look {
            _ if selected
                && !matches!(
                    look,
                    CellLook::Badge(_)
                        | CellLook::OutlinedBadge(_)
                        | CellLook::StrongOutlinedBadge(_)
                        | CellLook::Button
                ) =>
            {
                colors.selected_ink
            }
            CellLook::Text | CellLook::Buttons | CellLook::Loading => colors.ink,
            CellLook::Colored(tone) => tone.ink(cx),
            CellLook::Badge(tone) => {
                let h = (size.height + 2.0).min(rh - 4.0);
                let pill = Rect::new(x, y + (rh - h) / 2.0, width, h);
                let fill = tone.color(cx);
                cx.rounded_rect(pill, h / 2.0, fill, Border::default());
                tone.on_solid(cx)
            }
            CellLook::OutlinedBadge(tone) | CellLook::StrongOutlinedBadge(tone) => {
                let strong = matches!(look, CellLook::StrongOutlinedBadge(_));
                let h = (size.height + 2.0).min(rh - 4.0);
                let tag = Rect::new(x, y + (rh - h) / 2.0, width, h);
                let radius = cx.theme().corner(Corner::Inner, 4.0, h);
                let edge = tone.color(cx);
                let fill = tone.wash(cx, 0.14);
                // On a selected row the wash would vanish into the selection: put the
                // surface under it.
                if selected {
                    let surface = cx.color(Role::Surface);
                    cx.rounded_rect(tag, radius, surface, Border::default());
                }
                cx.rounded_rect(
                    tag,
                    radius,
                    fill,
                    Border {
                        width: 1.0,
                        color: if strong {
                            edge
                        } else {
                            edge.with_alpha(edge.a * 0.6)
                        },
                    },
                );
                if strong { edge } else { tone.ink(cx) }
            }
            CellLook::Button => {
                let h = rh - 4.0;
                let button = Rect::new(x, y + 2.0, width, h);
                let surface = cx.color(Role::Surface);
                let fill = if self.press == (Press::Button { row: id, column: c }) {
                    super::pressed_tint(cx, surface)
                } else {
                    surface
                };
                let outline = cx.color(Role::Outline);
                cx.bezel(
                    button,
                    5.0,
                    fill,
                    Border {
                        width: 1.0,
                        color: outline,
                    },
                );
                cx.color(Role::Text)
            }
        };
        let rect = Rect::new(x + inset, text_y, size.width, size.height);
        if plain && !shortened && self.filter.marks(c) {
            let mark = if selected {
                colors.selected_ink.with_alpha(0.3)
            } else {
                cx.theme().accent.with_alpha(0.28)
            };
            for range in self.filter.query.ranges(&text) {
                let spans = cx
                    .text_store()
                    .selection_rects(layout, range.start, range.end);
                for r in spans {
                    let r = Rect::new(rect.x + r.x, rect.y + r.y + 1.0, r.width, r.height - 2.0);
                    cx.rounded_rect(r, 3.0, mark, Border::default());
                }
            }
        }
        let clip = width - 2.0 * inset < size.width;
        if clip {
            let shown = (width - 2.0 * inset).max(0.0);
            cx.push_clip(Rect::new(x + inset, y, shown, rh));
        }
        cx.text(rect, layout, color);
        if clip {
            cx.pop_clip();
        }
    }

    fn paint_buttons(
        &mut self,
        cx: &mut PaintCx,
        spot: &CellSpot,
        left: f32,
        selected: bool,
        colors: &CellColors,
        style: TextStyle,
    ) {
        let (id, c) = (spot.id, spot.column);
        let buttons = self.data.cell_buttons(id, c);
        let cell = Rect::new(left, spot.y, spot.right - left, self.row_height);
        let place = ButtonPlace {
            pos: spot.pos,
            id,
            column: Some(c),
            align: self.columns[c].align,
            cell,
            shown: cell.intersect(&spot.area),
            selected,
        };
        self.paint_button_row(cx, &buttons, &place, colors, style);
    }

    fn paint_group_buttons(
        &mut self,
        cx: &mut PaintCx,
        (pos, id, y): (usize, usize, f32),
        colors: &CellColors,
    ) {
        let buttons = self.data.group_row_buttons(id);
        let v = self.viewport();
        let i = self.content_inset;
        let row = Rect::new(v.x + i, y, (v.width - 2.0 * i).max(0.0), self.row_height);
        let place = ButtonPlace {
            pos,
            id,
            column: None,
            align: ColumnAlign::Trailing,
            cell: row,
            shown: row.intersect(&v),
            selected: false,
        };
        self.paint_button_row(cx, &buttons, &place, colors, TextStyle::new(12.0));
    }

    /// Records where each enabled one is for clicks.
    fn paint_button_row(
        &mut self,
        cx: &mut PaintCx,
        buttons: &[CellButton],
        place: &ButtonPlace,
        colors: &CellColors,
        style: TextStyle,
    ) {
        if buttons.is_empty() {
            return;
        }
        let (id, cell) = (place.id, place.cell);
        let y = cell.y;
        let key = place.column.unwrap_or(GROUP_BUTTONS);
        let rh = self.row_height;
        let full = (rh - 4.0).max(0.0);
        let heights: Vec<f32> = (buttons.iter())
            .map(|b| b.height.map_or(full, |h| h.min(full)))
            .collect();
        let labels: Vec<_> = buttons
            .iter()
            .enumerate()
            .map(|(i, b)| {
                b.label.as_deref().map(|label| {
                    self.cells
                        .get(cx.text_store(), (id, key, i + 1), label, style)
                })
            })
            .collect();
        let widths: Vec<f32> = buttons
            .iter()
            .zip(labels.iter().zip(&heights))
            .map(|(b, (label, &h))| match label {
                Some((_, size)) => {
                    let icon = if b.icon.is_some() {
                        BUTTON_ICON + BUTTON_ICON_GAP
                    } else {
                        0.0
                    };
                    size.width + icon + 2.0 * BUTTON_PAD
                }
                None => h,
            })
            .collect();
        let total = widths.iter().sum::<f32>() + BUTTON_GAP * (widths.len() - 1) as f32;
        let (left, right) = (cell.x, cell.right());
        let mut x = match place.align {
            ColumnAlign::Leading => left + self.cell_pad,
            ColumnAlign::Center => left + (right - left - total) / 2.0,
            ColumnAlign::Trailing => right - self.cell_pad - total,
        };
        cx.push_clip(cell);
        let surface = cx.color(Role::Surface);
        let outline = cx.color(Role::Outline);
        let dim = cx.theme().disabled_opacity;
        let hover = self
            .hover
            .filter(|&p| self.position_at(p) == Some(place.pos));
        for (i, ((button, &h), (label, width))) in (buttons.iter().zip(&heights))
            .zip(labels.iter().zip(&widths))
            .enumerate()
        {
            let rect = Rect::new(x, y + (rh - h) / 2.0, *width, h);
            x += width + BUTTON_GAP;
            let pressed = self.press
                == (Press::Action {
                    row: id,
                    column: place.column,
                    index: i,
                });
            let hovered = button.enabled && hover.is_some_and(|p| rect.contains(p));
            let radius = cx.theme().corner(Corner::Control, 5.0, h);
            let mut ink = if label.is_some() {
                // On the bezel, which is the same on a selected row.
                let fill = if pressed && button.enabled {
                    super::pressed_tint(cx, surface)
                } else {
                    surface
                };
                cx.bezel(
                    rect,
                    radius,
                    fill,
                    Border {
                        width: 1.0,
                        color: outline,
                    },
                );
                button.tone.ink(cx)
            } else {
                let ink = if place.selected {
                    colors.selected_ink
                } else {
                    button.tone.ink(cx)
                };
                if pressed || hovered {
                    let tint = ink.with_alpha(ink.a * if pressed { 0.2 } else { 0.1 });
                    cx.rounded_rect(rect, radius, tint, Border::default());
                }
                ink
            };
            if !button.enabled {
                ink = ink.with_alpha(ink.a * dim);
            }
            let mut content = rect.x + BUTTON_PAD;
            if let Some(icon) = &button.icon {
                let at = if label.is_some() {
                    content
                } else {
                    rect.x + (rect.width - BUTTON_ICON) / 2.0
                };
                let r = Rect::new(at, y + (rh - BUTTON_ICON) / 2.0, BUTTON_ICON, BUTTON_ICON);
                icon.paint(cx, r, ink);
                content += BUTTON_ICON + BUTTON_ICON_GAP;
            }
            if let Some((Some(layout), size)) = label {
                let r = Rect::new(
                    content,
                    y + (rh - size.height) / 2.0,
                    size.width,
                    size.height,
                );
                cx.text(r, *layout, ink);
            }
            if button.enabled
                && let Some(rect) = place.shown.and_then(|s| s.intersect(&rect))
            {
                self.hits.push(ButtonHit {
                    row: id,
                    column: place.column,
                    index: i,
                    rect,
                    action: button.action.clone(),
                });
            }
        }
        cx.pop_clip();
    }

    fn paint_separators(&self, cx: &mut PaintCx, rows: Range<usize>) {
        let v = self.viewport();
        let rh = self.row_height;
        let line = cx.color(Role::Separator);
        for pos in rows {
            let y = v.y + (pos + 1) as f32 * rh - self.scroller.offset.y - 1.0;
            match self.data.row_separator(self.order[pos]) {
                RowSeparator::Line => cx.fill_rect(Rect::new(v.x, y, v.width, 1.0), line),
                RowSeparator::Dashed => {
                    let mut x = v.x;
                    while x < v.right() {
                        let dash = DASH.min(v.right() - x);
                        cx.fill_rect(Rect::new(x, y, dash, 1.0), line);
                        x += DASH + DASH_GAP;
                    }
                }
                RowSeparator::None => {}
            }
        }
    }

    fn key(&mut self, cx: &mut EventCx, key: Key, shift: bool, command: bool) -> bool {
        let n = self.order.len();
        match key {
            Key::Char('c') if command => self.copy(cx),
            Key::Char('a') if command => {
                if self.select_all_rows() {
                    self.changed(cx);
                }
                true
            }
            Key::Enter => {
                if let Some(id) = self.selection.lead {
                    cx.emit(RowActivated(id));
                }
                true
            }
            Key::ArrowUp | Key::ArrowDown | Key::Home | Key::End | Key::PageUp | Key::PageDown => {
                let page = (self.viewport().height / self.row_height) as usize;
                let current = self.selection.lead_position(&self.order);
                let down = matches!(key, Key::ArrowDown | Key::PageDown | Key::Home);
                let target = step(key, current, n, page).and_then(|pos| {
                    self.skip_groups(pos, down)
                        .or_else(|| self.skip_groups(pos, !down))
                });
                if let Some(pos) = target {
                    if self.selection.move_to(&self.order, pos, shift) {
                        self.drop_group_rows();
                        self.changed(cx);
                    }
                    if self.reveal(pos) {
                        self.scroller.flash(cx);
                        cx.request_paint();
                    }
                }
                true
            }
            _ => false,
        }
    }
}

/// Where something `width` wide starts in a cell from `left` to `right` with `room`
/// inside its padding `pad`.
fn aligned(align: ColumnAlign, (left, right): (f32, f32), pad: f32, room: f32, width: f32) -> f32 {
    match align {
        ColumnAlign::Leading => left + pad,
        ColumnAlign::Center => left + pad + (room - width) / 2.0,
        ColumnAlign::Trailing => right - pad - width,
    }
}

/// A [`TableData::cell_dot`] with its left edge at `at.x`, centered on `at.y`.
fn paint_dot(cx: &mut PaintCx, at: Point, color: Color) {
    let rect = Rect::new(at.x, at.y - DOT / 2.0, DOT, DOT);
    cx.rounded_rect(rect, DOT / 2.0, color, Border::default());
}

/// The number `text` starts with, ignoring thousands separators and a unit after a
/// space ("1,024 KB" → 1024).
pub(crate) fn leading_number(text: &str) -> Option<f64> {
    let word = text.split_whitespace().next()?;
    word.replace(',', "").parse().ok()
}

fn directed(order: Ordering, ascending: bool) -> Ordering {
    if ascending { order } else { order.reverse() }
}

/// Background of a selected row: the accent while the view has focus, gray otherwise.
pub(crate) fn selection_fill(cx: &PaintCx, focused: bool) -> (noa_gpui_scene::Color, Role) {
    if focused {
        (cx.color(Role::Accent), Role::OnAccent)
    } else {
        let surface = cx.color(Role::Surface);
        (mix(surface, cx.color(Role::Text), 0.12), Role::Text)
    }
}

pub(crate) fn stripe(cx: &PaintCx) -> noa_gpui_scene::Color {
    mix(cx.color(Role::Surface), cx.color(Role::Text), 0.035)
}

struct CellSpot {
    pos: usize,
    id: usize,
    column: usize,
    left: f32,
    right: f32,
    y: f32,
    area: Rect,
    in_range: bool,
}

struct ButtonPlace {
    pos: usize,
    id: usize,
    column: Option<usize>,
    align: ColumnAlign,
    cell: Rect,
    shown: Option<Rect>,
    selected: bool,
}

struct CellColors {
    rule: noa_gpui_scene::Color,
    ink: noa_gpui_scene::Color,
    selected_ink: noa_gpui_scene::Color,
    /// The second line of a cell ([`TableData::cell_detail`]).
    muted: noa_gpui_scene::Color,
    range: noa_gpui_scene::Color,
    focus: noa_gpui_scene::Color,
}

impl Widget for TableView {
    /// The widths of the columns the user resized, the sort order and the columns
    /// hidden: `columns:4;widths:0=120,2=88.5;sort:1,desc;hidden:3`. Restored only into
    /// a table with the same number of columns. The hidden columns are recorded only
    /// when they differ from those the app hid (`hidden:` with none when every column
    /// is shown).
    fn save_state(&self) -> Option<String> {
        let resized: Vec<usize> = (0..self.columns.len())
            .filter(|&c| self.columns[c].resized)
            .collect();
        let shown_changed = self.columns.iter().any(|c| c.hidden != c.hidden_at_start);
        if resized.is_empty() && self.sort.is_none() && !shown_changed {
            return None;
        }
        let mut out = format!("columns:{}", self.columns.len());
        if !resized.is_empty() {
            out.push_str(";widths:");
            for (i, &c) in resized.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&format!("{c}="));
                view_state::push_number(&mut out, self.columns[c].width);
            }
        }
        if let Some((column, ascending)) = self.sort {
            let order = if ascending { "asc" } else { "desc" };
            out.push_str(&format!(";sort:{column},{order}"));
        }
        if shown_changed {
            let hidden: Vec<String> = (0..self.columns.len())
                .filter(|&c| self.columns[c].hidden)
                .map(|c| c.to_string())
                .collect();
            out.push_str(";hidden:");
            out.push_str(&hidden.join(","));
        }
        Some(out)
    }

    fn restore_state(&mut self, cx: &mut UpdateCx, state: &str) {
        let columns = view_state::field(state, "columns").and_then(|n| n.parse::<usize>().ok());
        if columns != Some(self.columns.len()) {
            return;
        }
        if let Some(widths) = view_state::field(state, "widths") {
            for part in widths.split(',') {
                let Some((column, width)) = part.split_once('=') else {
                    continue;
                };
                let (Ok(column), Some(width)) = (
                    column.trim().parse::<usize>(),
                    view_state::parse_length(width),
                ) else {
                    continue;
                };
                if self.columns.get(column).is_some_and(|c| c.resizable) {
                    self.set_column_width(cx, column, width);
                }
            }
        }
        if let Some((column, order)) =
            view_state::field(state, "sort").and_then(|s| s.split_once(','))
            && let Ok(column) = column.trim().parse::<usize>()
            && self
                .columns
                .get(column)
                .is_some_and(|c| c.sort != ColumnSort::None)
        {
            match order.trim() {
                "asc" => self.sort_by(cx, column, true),
                "desc" => self.sort_by(cx, column, false),
                _ => {}
            }
        }
        // State saved before columns could be hidden has no `hidden` field: the
        // columns stay as the app made them.
        if let Some(hidden) = view_state::field(state, "hidden") {
            let hidden: Vec<usize> = hidden
                .split(',')
                .filter_map(|c| c.trim().parse().ok())
                .collect();
            // Show first, so hiding never meets "the last column shown".
            for c in 0..self.columns.len() {
                if !hidden.contains(&c) {
                    self.set_column_visible(cx, c, true);
                }
            }
            for &c in &hidden {
                self.set_column_visible(cx, c, false);
            }
        }
    }

    fn context_menu(&self, local: Point) -> Option<Vec<MenuItem>> {
        // From the keyboard the menu is asked for at the table's top left corner.
        (self.column_chooser && local.y < TABLE_HEADER).then(|| self.column_menu())
    }

    /// A cell button's [`CellButton::tooltip`], or the whole text of a cell shown
    /// shortened with an ellipsis.
    fn tooltip(&self, local: Point) -> Option<String> {
        if self.header && local.y < TABLE_HEADER {
            return None;
        }
        if let Some(hit) = self.action_at(local) {
            let buttons = match hit.column {
                Some(column) => self.data.cell_buttons(hit.row, column),
                None => self.data.group_row_buttons(hit.row),
            };
            return buttons.into_iter().nth(hit.index).and_then(|b| b.tooltip);
        }
        let (pos, column) = self.cell_at(local)?;
        let row = *self.order.get(pos)?;
        if !self.shortened.contains(&(row, column)) {
            return None;
        }
        Some(self.cell_text(row, column, "\n"))
    }

    fn cursor(&self, local: Point, _size: Size) -> Option<Cursor> {
        let resizing = matches!(self.press, Press::Resize { .. });
        (resizing || self.resize_edge_at(local).is_some()).then_some(Cursor::ColResize)
    }

    fn style(&self) -> Style {
        let mut style = Style {
            size: LayoutSize {
                width: self.width,
                height: Dimension::auto(),
            },
            ..Default::default()
        };
        let min = if self.header { TABLE_HEADER } else { 0.0 } + self.row_height;
        view_height(&mut style, self.height, min);
        style
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        if self.families.is_none() {
            let store = cx.text_store();
            let families = (self.columns.iter())
                .map(|c| c.family.as_deref().map(|f| store.font_family(f)))
                .collect();
            self.families = Some(families);
            self.detail_families = (self.columns.iter())
                .map(|c| c.detail_family.as_deref().map(|f| store.font_family(f)))
                .collect();
            self.header_family_id = self.header_family.as_deref().map(|f| store.font_family(f));
        }
        self.sync(cx.size());
        self.hits.clear();
        self.editor.begin_paint(cx.text_store());
        let bounds = cx.bounds();
        let surface = cx.color(Role::Surface);
        let background = self.fill.map_or(surface, |fill| cx.color(fill));
        let frame = if self.bordered {
            Border {
                width: 1.0,
                color: cx.color(Role::Outline),
            }
        } else {
            Border::default()
        };
        cx.rounded_rect(bounds, 0.0, background, frame);
        let v = self.viewport();
        let rh = self.row_height;
        let off = self.scroller.offset;
        let spans = self.column_spans();
        let row_width =
            (self.total_width() + 2.0 * self.content_inset + self.check_width()).max(v.width);
        let focused = cx.is_focused();
        let (selected_fill, selected_ink) = selection_fill(cx, focused);
        let stripe = stripe(cx);
        let ink = cx.color(Role::Text);
        let selected_ink = cx.color(selected_ink);
        let hovered = (self.hover_rows)
            .then(|| self.hover.and_then(|p| self.position_at(p)))
            .flatten();
        let hover_fill = mix(background, ink, 0.06);

        cx.push_clip(v);
        let first = (off.y.max(0.0) / rh) as usize;
        let last = ((off.y + v.height) / rh).ceil().max(0.0) as usize;
        let row_mode = !self.cell_mode;
        self.cells.begin();
        self.shortened.clear();
        for pos in first..last {
            let y = v.y + pos as f32 * rh - off.y;
            let row = Rect::new(v.x - off.x, y, row_width, rh);
            let Some(&id) = self.order.get(pos) else {
                // Stripes continue below the last row.
                if self.alternating && pos % 2 == 1 {
                    cx.fill_rect(Rect::new(v.x, y, v.width, rh), stripe);
                }
                continue;
            };
            if self.groups[id] {
                self.paint_group_row(cx, id, y, background);
            } else if row_mode && self.selection.is_selected(id) {
                cx.fill_rect(row, selected_fill);
            } else if hovered == Some(pos) {
                cx.fill_rect(row, hover_fill);
            } else if self.alternating && pos % 2 == 1 {
                cx.fill_rect(row, stripe);
            }
        }
        if self.row_separators {
            self.paint_separators(cx, first..last.min(self.order.len()));
        }
        // The scrolling columns, then the frozen ones over their edges.
        let frozen = self.frozen_column_count();
        let trailing = self.trailing_start();
        let range = self.selected_cells();
        let colors = CellColors {
            rule: cx.color(Role::Outline),
            ink,
            selected_ink,
            muted: cx.color(Role::Muted),
            range: if focused {
                cx.color(Role::Accent).with_alpha(0.16)
            } else {
                mix(background, ink, 0.1)
            },
            focus: if focused {
                cx.color(Role::Accent)
            } else {
                cx.color(Role::Muted)
            },
        };
        let passes = [
            (self.scrolling_area(v.y, v.height), frozen..trailing),
            (self.trailing_area(v.y, v.height), trailing..spans.len()),
            (
                Rect::new(v.x, v.y, self.frozen_edge().min(v.width), v.height),
                0..frozen,
            ),
        ];
        for (area, columns) in passes.clone() {
            if columns.is_empty() || area.width <= 0.0 {
                continue;
            }
            cx.push_clip(area);
            for pos in first..last.min(self.order.len()) {
                let id = self.order[pos];
                if self.groups[id] && !self.group_cells {
                    continue;
                }
                let y = v.y + pos as f32 * rh - off.y;
                let at = (pos, id, y);
                self.paint_row_cells(cx, at, (area, columns.clone()), range.as_ref(), &colors);
            }
            cx.pop_clip();
        }
        for pos in first..last.min(self.order.len()) {
            let id = self.order[pos];
            if self.groups[id] {
                let y = v.y + pos as f32 * rh - off.y;
                self.paint_group_buttons(cx, (pos, id, y), &colors);
            }
        }
        if self.frozen_edge() > 0.0 && off.x > 0.0 {
            let edge = v.x + self.frozen_edge();
            let line = cx.color(Role::Separator);
            cx.fill_rect(Rect::new(edge - 0.5, v.y, 1.0, v.height), line);
        }
        if self.trailing_shift() > 0.0 {
            let edge = self.trailing_edge();
            let line = cx.color(Role::Separator);
            cx.fill_rect(Rect::new(edge - 0.5, v.y, 1.0, v.height), line);
        }
        if self.check_width() > 0.0 {
            let ring = cx.color(Role::OnAccent);
            for pos in first..last.min(self.order.len()) {
                let id = self.order[pos];
                if self.groups[id] {
                    continue;
                }
                let b = self.check_box(v.y + pos as f32 * rh - off.y, rh);
                let selected = self.selection.is_selected(id);
                if selected && focused {
                    // The box is the color of the selected row under it.
                    let around = Rect::new(b.x - 1.0, b.y - 1.0, b.width + 2.0, b.height + 2.0);
                    let radius = cx.corner(Corner::Inner, 4.0, b.height) + 1.0;
                    cx.rounded_rect(around, radius, ring, Border::default());
                }
                let state = if selected {
                    CheckState::On
                } else {
                    CheckState::Off
                };
                paint_check_box(cx, b, state, self.press == Press::Check(Some(id)), 1.0);
            }
        }
        if let Some((pos, y)) = self.floating_group() {
            let id = self.order[pos];
            cx.fill_rect(Rect::new(v.x, y, v.width, rh), background);
            self.paint_group_row(cx, id, y, background);
            if self.group_cells {
                for (area, columns) in passes.clone() {
                    if columns.is_empty() || area.width <= 0.0 {
                        continue;
                    }
                    cx.push_clip(area);
                    self.paint_row_cells(cx, (pos, id, y), (area, columns), None, &colors);
                    cx.pop_clip();
                }
            }
            self.paint_group_buttons(cx, (pos, id, y), &colors);
            let line = cx.color(Role::Separator);
            cx.fill_rect(Rect::new(v.x, y + rh - 1.0, v.width, 1.0), line);
        }
        self.cells.end(cx.text_store());
        if let Press::Row {
            dragging: true,
            drop,
            ..
        } = self.press
        {
            let y = v.y + drop as f32 * rh - off.y;
            let accent = cx.color(Role::Accent);
            cx.fill_rect(Rect::new(v.x, y - 1.0, v.width, 2.0), accent);
            cx.rounded_rect(
                Rect::new(v.x + 2.0, y - 3.0, 6.0, 6.0),
                3.0,
                surface,
                Border {
                    width: 1.5,
                    color: accent,
                },
            );
        }
        cx.pop_clip();

        if self.header {
            let i = self.inset();
            let header = Rect::new(i, i, bounds.width - 2.0 * i, TABLE_HEADER - i);
            let header_fill = self.header_fill.map_or(background, |fill| cx.color(fill));
            cx.fill_rect(header, header_fill);
            let line = cx.color(Role::Separator);
            cx.fill_rect(
                Rect::new(header.x, TABLE_HEADER - 1.0, header.width, 1.0),
                line,
            );
            let muted = cx.color(Role::Muted);
            let mut title_style = TextStyle::new(12.0).weight(600);
            if let Some(family) = self.header_family_id {
                title_style = title_style.family(family);
            }
            let frozen_area = Rect::new(
                header.x,
                header.y,
                self.frozen_edge().min(header.width),
                header.height,
            );
            let passes = [
                (
                    self.scrolling_area(header.y, header.height),
                    frozen..trailing,
                ),
                (
                    self.trailing_area(header.y, header.height),
                    trailing..spans.len(),
                ),
                (frozen_area, 0..frozen),
            ];
            for (area, columns) in passes {
                if columns.is_empty() || area.width <= 0.0 {
                    continue;
                }
                cx.push_clip(area);
                for c in columns {
                    if self.columns[c].hidden {
                        continue;
                    }
                    let (left, right) = spans[c];
                    if let Press::Header(p) = self.press
                        && p == c
                    {
                        let tint = mix(header_fill, ink, 0.08);
                        cx.fill_rect(Rect::new(left, 1.0, right - left, TABLE_HEADER - 2.0), tint);
                    }
                    let title = &self.columns[c].title;
                    let title = if self.header_uppercase {
                        Cow::Owned(title.to_uppercase())
                    } else {
                        Cow::Borrowed(title.as_str())
                    };
                    let size = self.titles[c].shape(cx.text_store(), &title, title_style, None);
                    let sorted = self.sort.filter(|&(s, _)| s == c);
                    let arrow = if sorted.is_some() { 14.0 } else { 0.0 };
                    let room = right - left - 2.0 * self.cell_pad - arrow;
                    let x = match self.columns[c].align {
                        ColumnAlign::Leading => left + self.cell_pad,
                        ColumnAlign::Center => left + self.cell_pad + (room - size.width) / 2.0,
                        ColumnAlign::Trailing => right - self.cell_pad - arrow - size.width,
                    };
                    if let Some(layout) = self.titles[c].layout() {
                        cx.push_clip(Rect::new(
                            left + self.cell_pad,
                            0.0,
                            room.max(0.0),
                            TABLE_HEADER,
                        ));
                        cx.text(
                            Rect::new(
                                x,
                                (TABLE_HEADER - size.height) / 2.0,
                                size.width,
                                size.height,
                            ),
                            layout,
                            if sorted.is_some() { ink } else { muted },
                        );
                        cx.pop_clip();
                    }
                    if let Some((_, ascending)) = sorted {
                        let r = Rect::new(
                            right - self.cell_pad - 8.0,
                            TABLE_HEADER / 2.0 - 2.0,
                            8.0,
                            4.0,
                        );
                        let direction = if ascending {
                            Direction::Up
                        } else {
                            Direction::Down
                        };
                        cx.chevron(r, direction, 1.5, muted);
                    }
                    cx.fill_rect(Rect::new(right - 0.5, 6.0, 1.0, TABLE_HEADER - 12.0), line);
                }
                cx.pop_clip();
            }
            if self.has_header_check() {
                let b = self.check_box(0.0, TABLE_HEADER);
                let pressed = self.press == Press::Check(None);
                paint_check_box(cx, b, self.header_check_state(), pressed, 1.0);
            }
        }
        self.scroller.paint(cx);
        if focused {
            focus_ring(cx, bounds, 0.0);
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        self.sync(cx.size());
        if self.edit_event(cx, event) {
            return;
        }
        if let Event::AnimationFrame { now } = event {
            self.autoscroll_step(cx, *now);
        }
        if self.scroller.event(cx, event) {
            return;
        }
        match event {
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                let p = cx.local(*pos);
                if self.header && p.y < TABLE_HEADER {
                    if self.has_header_check() && self.in_check_column(p.x) {
                        self.press = Press::Check(None);
                        cx.request_paint();
                    } else if let Some(column) = self.resize_edge_at(p) {
                        self.press = Press::Resize {
                            column,
                            x: p.x,
                            width: self.widths[column],
                        };
                    } else if let Some(c) = self.column_at(p.x) {
                        self.press = Press::Header(c);
                        cx.request_paint();
                    }
                    cx.capture_pointer();
                    cx.set_handled();
                    return;
                }
                if self.check_width() > 0.0
                    && self.in_check_column(p.x)
                    && let Some(pos) = self.position_at(p)
                    && !self.is_group_at(pos)
                {
                    self.press = Press::Check(Some(self.order[pos]));
                    cx.capture_pointer();
                    cx.request_paint();
                    cx.set_handled();
                    return;
                }
                if let Some(hit) = self.action_at(p) {
                    self.press = Press::Action {
                        row: hit.row,
                        column: hit.column,
                        index: hit.index,
                    };
                    cx.capture_pointer();
                    cx.request_paint();
                    cx.set_handled();
                    return;
                }
                if let Some(cell) = self.button_at(p) {
                    self.press = Press::Button {
                        row: cell.0,
                        column: cell.1,
                    };
                    cx.capture_pointer();
                    cx.request_paint();
                    cx.set_handled();
                    return;
                }
                if self.cell_mode {
                    self.cell_press(cx, p);
                    cx.set_handled();
                    return;
                }
                let Some(pos) = self.position_at(p) else {
                    // A press below the rows clears the selection.
                    if self.selection.count() > 0 && self.viewport().contains(p) {
                        self.selection.set(None);
                        self.changed(cx);
                    }
                    cx.set_handled();
                    return;
                };
                if self.is_group_at(pos) {
                    // Group rows are headings, not selected; a click is reported.
                    self.press = Press::Group(self.order[pos]);
                    cx.capture_pointer();
                    cx.set_handled();
                    return;
                }
                let id = self.order[pos];
                let now = cx.now();
                let double = self
                    .last_click
                    .is_some_and(|(t, last)| last == id && now - t <= DOUBLE_CLICK);
                self.last_click = Some((now, id));
                let modifiers = cx.modifiers();
                // Pressing an already selected row keeps the selection, so the rows
                // can be dragged together; the click lands on release.
                let keep = self.reorderable
                    && self.selection.is_selected(id)
                    && !modifiers.command
                    && !modifiers.shift;
                if !keep && self.selection.click(&self.order, pos, modifiers) {
                    self.drop_group_rows();
                    self.changed(cx);
                }
                if double {
                    cx.emit(RowActivated(id));
                    self.last_click = None;
                }
                if self.reorderable && self.selection.is_selected(id) {
                    self.press = Press::Row {
                        at: p,
                        click: keep,
                        dragging: false,
                        drop: pos,
                    };
                    cx.capture_pointer();
                }
                cx.set_handled();
            }
            Event::PointerMove { pos } => {
                let p = cx.local(*pos);
                match self.press {
                    Press::Resize { column, x, width } => {
                        let col = &mut self.columns[column];
                        let w = (width + p.x - x).max(col.min_width);
                        // A dragged column keeps the width it is dragged to.
                        if w != self.widths[column] || col.flex > 0.0 || col.shrink.is_some() {
                            col.width = w;
                            col.flex = 0.0;
                            col.shrink = None;
                            col.resized = true;
                            self.sync(cx.size());
                            cx.request_paint();
                        }
                        cx.set_handled();
                    }
                    Press::Row {
                        at,
                        click,
                        dragging,
                        drop,
                    } => {
                        let moved = (p.y - at.y).abs().max((p.x - at.x).abs()) >= DRAG_START;
                        let next = self.drop_position(p.y);
                        if (moved || dragging) && (!dragging || next != drop) {
                            self.press = Press::Row {
                                at,
                                click,
                                dragging: true,
                                drop: next,
                            };
                            cx.request_paint();
                        }
                        self.track_autoscroll(cx, p);
                        cx.set_handled();
                    }
                    Press::Cells => {
                        if let Some((pos, c)) = self.cell_at(p)
                            && !self.is_group_at(pos)
                        {
                            self.focus_cell(cx, pos, c, true);
                        }
                        self.track_autoscroll(cx, p);
                        cx.set_handled();
                    }
                    _ => {}
                }
                self.hover_to(cx, Some(p));
            }
            Event::PointerLeave | Event::HoverChanged(false) => self.hover_to(cx, None),
            Event::PointerUp { pos, .. } => {
                let p = cx.local(*pos);
                self.autoscroll = None;
                match std::mem::replace(&mut self.press, Press::None) {
                    Press::Header(c) => {
                        cx.request_paint();
                        let spans = self.column_spans();
                        if p.y < TABLE_HEADER && p.x >= spans[c].0 && p.x < spans[c].1 {
                            self.header_click(cx, c);
                        }
                    }
                    Press::Resize { column, .. } => {
                        cx.emit(ColumnResized {
                            column,
                            width: self.widths[column],
                        });
                    }
                    Press::Row {
                        click,
                        dragging,
                        drop,
                        ..
                    } => {
                        if dragging {
                            self.move_selected(cx, drop);
                            cx.request_paint();
                        } else if click && let Some(pos) = self.position_at(p) {
                            // A plain click on a selected row selects only it.
                            if self.selection.click(&self.order, pos, cx.modifiers()) {
                                self.changed(cx);
                            }
                        }
                    }
                    Press::Cells => {}
                    Press::Button { row, column } => {
                        cx.request_paint();
                        if self.button_at(p) == Some((row, column)) {
                            cx.emit(CellButtonClicked { row, column });
                        }
                    }
                    Press::Action { row, column, index } => {
                        cx.request_paint();
                        if let Some(hit) = self.action_at(p)
                            && (hit.row, hit.column, hit.index) == (row, column, index)
                        {
                            let action = hit.action.clone();
                            match column {
                                Some(column) => cx.emit(CellActionClicked {
                                    row,
                                    column,
                                    action,
                                }),
                                None => cx.emit(GroupActionClicked { row, action }),
                            }
                        }
                    }
                    Press::Group(row) => {
                        let over = self.position_at(p).map(|pos| self.order[pos]);
                        if over == Some(row) && self.action_at(p).is_none() {
                            cx.emit(GroupRowClicked(row));
                        }
                    }
                    Press::Check(None) => {
                        cx.request_paint();
                        if p.y < TABLE_HEADER && self.in_check_column(p.x) {
                            self.toggle_all(cx);
                        }
                    }
                    Press::Check(Some(row)) => {
                        cx.request_paint();
                        if self.in_check_column(p.x)
                            && let Some(pos) = self.position_at(p)
                            && self.order[pos] == row
                        {
                            self.toggle_row(cx, pos);
                        }
                    }
                    Press::None => return,
                }
                cx.release_pointer();
                cx.set_handled();
            }
            Event::KeyDown { key, modifiers } if cx.is_focused() => {
                let used = if self.cell_mode {
                    self.cell_key(cx, *key, modifiers.shift, modifiers.command)
                } else {
                    self.key(cx, *key, modifiers.shift, modifiers.command)
                };
                if used {
                    cx.set_handled();
                }
            }
            Event::Text(text) if cx.is_focused() && self.cell_mode => {
                // Typing into a focused editable cell replaces its text.
                if let Some((id, c)) = self.cell_focus
                    && !cx.modifiers().command
                    && !text.chars().any(char::is_control)
                    && self.start_edit(id, c)
                {
                    self.edit_event(cx, event);
                    cx.set_handled();
                }
            }
            Event::Text(text) if cx.is_focused() && !self.cell_mode && !cx.modifiers().command => {
                if !text.chars().any(char::is_control) && self.type_select(cx, text) {
                    cx.set_handled();
                }
            }
            Event::ActivateItem(i) if *i >= self.visible_rows().len() => {
                match self.check_items().get(i - self.visible_rows().len()) {
                    Some(None) => self.toggle_all(cx),
                    Some(&Some(pos)) => self.toggle_row(cx, pos),
                    None => {}
                }
            }
            Event::ActivateItem(i) if self.cell_mode => {
                let pos = self.visible_rows().start + i;
                if pos < self.order.len() && !self.is_group_at(pos) {
                    let column = self
                        .cell_focus
                        .map_or_else(|| self.nearest_visible(0), |(_, c)| c);
                    self.focus_cell(cx, pos, column, false);
                }
            }
            Event::ActivateItem(i) => {
                let pos = self.visible_rows().start + i;
                if pos < self.order.len() && !self.is_group_at(pos) {
                    self.selection.set(Some(self.order[pos]));
                    self.changed(cx);
                }
            }
            Event::MenuClosed { chosen: Some(i) } if self.column_chooser => {
                // The menu lists the hideable columns in order.
                let column = (0..self.columns.len())
                    .filter(|&c| self.columns[c].hideable)
                    .nth(*i);
                if let Some(column) = column {
                    let visible = self.columns[column].hidden;
                    if self.change_visibility(column, visible) {
                        cx.emit(ColumnVisibilityChanged { column, visible });
                        cx.request_paint();
                    }
                }
                cx.set_handled();
            }
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn text_input_area(&self) -> Option<Rect> {
        self.editor.input_area()
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Table)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        node.set_row_count(self.order.len());
        node.set_column_count(self.visible_columns().len());
        self.scroller.accessibility(node);
    }

    /// The rows on screen, each labelled with its cells, then their checkboxes
    /// ([`TableView::selection_checkboxes`]).
    fn accessibility_items(&self) -> Vec<accesskit::Node> {
        let v = self.viewport();
        let first_visible_column = self.columns.iter().position(|c| !c.hidden);
        let checks = self.check_items().into_iter().map(|target| {
            let mut node = accesskit::Node::new(accesskit::Role::CheckBox);
            let (state, top, height) = match target {
                None => {
                    node.set_label("Select all rows");
                    (self.header_check_state(), 0.0, TABLE_HEADER)
                }
                Some(pos) => {
                    let id = self.order[pos];
                    node.set_label(match first_visible_column {
                        Some(column) => format!("Select {}", self.accessible_cell_text(id, column)),
                        None => format!("Select row {}", pos + 1),
                    });
                    let state = if self.selection.is_selected(id) {
                        CheckState::On
                    } else {
                        CheckState::Off
                    };
                    let y = v.y + pos as f32 * self.row_height - self.scroller.offset.y;
                    (state, y, self.row_height)
                }
            };
            node.set_toggled(match state {
                CheckState::Off => accesskit::Toggled::False,
                CheckState::On => accesskit::Toggled::True,
                CheckState::Mixed => accesskit::Toggled::Mixed,
            });
            let b = self.check_box(top, height);
            node.set_bounds(accesskit::Rect {
                x0: f64::from(b.x),
                y0: f64::from(b.y),
                x1: f64::from(b.right()),
                y1: f64::from(b.bottom()),
            });
            node.add_action(accesskit::Action::Click);
            node
        });
        self.visible_rows()
            .map(|pos| {
                let id = self.order[pos];
                let mut node = accesskit::Node::new(accesskit::Role::Row);
                if self.groups[id] && !self.group_cells {
                    node.set_label(self.data.cell(id, 0).into_owned());
                } else {
                    let label: Vec<String> = (self.visible_columns().into_iter())
                        .map(|c| self.accessible_cell_text(id, c))
                        .collect();
                    node.set_label(label.join(", "));
                }
                node.set_row_index(pos);
                node.set_selected(self.selection.is_selected(id));
                let y = v.y + pos as f32 * self.row_height - self.scroller.offset.y;
                node.set_bounds(accesskit::Rect {
                    x0: f64::from(v.x),
                    y0: f64::from(y),
                    x1: f64::from(v.right()),
                    y1: f64::from(y + self.row_height),
                });
                node.add_action(accesskit::Action::Click);
                node
            })
            .chain(checks)
            .collect()
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.editor.removed(text);
        self.cells.release(text);
        for title in &mut self.titles {
            title.release(text);
        }
    }
}
